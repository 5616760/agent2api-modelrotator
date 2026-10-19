//! 模型级轮换（model-rotator 融合层）。
//!
//! 与 `upstream/rotate.rs` 的**账号级**选路互补：那里回答「某个模型名交给哪家
//! 账号去打」，本模块回答「客户端请求一个虚拟模型名（如 `AutoRotate`）时，
//! 这一发实际落到池里的哪个**真实模型**」。
//!
//! 设计取舍（与 Node 版 model-rotator 对齐）：
//!   · 时间窗轮换：每 `rotate_minutes` 切到池内下一个成员；权重决定每位出现的频率；
//!   · 失败转移：某成员命中 `fail_statuses` 时进入 `cooldown_minutes` 冷却，
//!     同窗内后续请求顺延到下一个可用成员；
//!   · 会话粘性（窗内）：同一 `session_key` 在当前时间窗内恒定绑定同一成员，
//!     窗口到点后随轮换一起前进（不永久钉死）；
//!   · 看门狗（hedge）：`hedge_after_seconds` 内有能力时由转发层并发抢答
//!     （本模块只暴露阈值，抢答编排在 `upstream` 层，见下方说明）。
//!
//! 持久化：配置存 `{config_dir}/model_rotation.json`，与改造后的主配置（`kv` 表）
//! 分开文件，避免动到全局 `CONFIG` 的 raw 键迁移 machinery；冷却 / 会话绑定是
//! 运行期状态，进程重启即清空（冷却时长通常分钟级，重启重建无碍）。
//!
//! 接入点：`api::chat::chat_completions` 在 `pipeline::resolve_model` **之前**调用
//! [`resolve_for_request`]：命中虚拟模型名就原地改写 `payload["model"]` 为具体模型名，
//! 让后续的目录校验 / 账号选路作用在真实模型上。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::HeaderMap;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::server::config;
use crate::server::logging;

/// 池内一位成员：某 provider 的某个具体上游模型。
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct RotationMember {
    /// provider id（与 `providers` 注册表一致，如 `workbuddy` / `catpaw`）。
    pub provider: String,
    /// 具体上游模型名（写进 `payload["model"]` 的那一个）。
    pub model: String,
    /// 权重：时间窗轮换时该成员在加权序列里出现的次数（≥1）。
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// 是否启用；缺省视为启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 一个轮换组：对外暴露一个虚拟模型名，对内维护一个真实模型池。
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct RotationGroup {
    /// 客户端请求的虚拟模型名（如 `AutoRotate`）。
    pub name: String,
    /// 组总开关。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 时间窗长度（分钟）；0 表示不轮换（永远用池内第一位可用成员）。
    #[serde(default = "default_rotate_minutes")]
    pub rotate_minutes: u64,
    /// 失败后冷却分钟数。
    #[serde(default = "default_cooldown_minutes")]
    pub cooldown_minutes: u64,
    /// 同窗会话粘性（见模块头）。
    #[serde(default = "default_true")]
    pub sticky_within_window: bool,
    /// 看门狗阈值（秒）：超过仍未出响应头则并发抢答（见模块头）。
    #[serde(default = "default_hedge_seconds")]
    pub hedge_after_seconds: f64,
    /// 触发冷却的失败状态码（缺省 429 / 500 / 502 / 503 / 504）。
    #[serde(default = "default_fail_statuses")]
    pub fail_statuses: Vec<u16>,
    /// 真实模型池。
    #[serde(default)]
    pub members: Vec<RotationMember>,
}

/// 顶层配置（落盘结构）。
#[derive(Clone, Debug, Serialize, serde::Deserialize, Default)]
pub struct ModelRotationConfig {
    /// 全局开关：关闭时 `resolve_for_request` 直接放行（不拦截任何模型名）。
    #[serde(default)]
    pub enabled: bool,
    /// 轮换组列表。
    #[serde(default)]
    pub groups: Vec<RotationGroup>,
}

/// 解析结果：虚拟模型名解析出的具体目标。
#[derive(Clone, Debug)]
pub struct ResolvedTarget {
    pub provider: String,
    pub model: String,
}

/// 单次选路后的窗口信息（按组存）。
struct WindowInfo {
    /// 当前窗口起点（ms）；手动切换 / 首次解析时刷新。
    started_at_ms: i64,
    /// 手动切换偏移（格数），叠加在时间窗序号上。
    offset: i64,
}

/// 会话粘性绑定（按 session_key 存）。
struct SessionBinding {
    member_key: String,
    expire_at_ms: i64,
}

/// 运行期状态：配置 + 窗口 + 冷却 + 会话绑定。
struct RotationState {
    config: ModelRotationConfig,
    windows: HashMap<String, WindowInfo>,
    /// group -> (member_key -> 冷却截至 ms)
    cooldowns: HashMap<String, HashMap<String, i64>>,
    /// session_key -> 绑定
    sessions: HashMap<String, SessionBinding>,
}

static STATE: OnceLock<Mutex<RotationState>> = OnceLock::new();

/// 取全局状态（懒加载；首次调用时从磁盘读配置）。
fn state() -> &'static Mutex<RotationState> {
    STATE.get_or_init(|| Mutex::new(load_state()))
}

// ── 默认值 ────────────────────────────────────────────────
fn default_true() -> bool {
    true
}
fn default_weight() -> u32 {
    1
}
fn default_rotate_minutes() -> u64 {
    30
}
fn default_cooldown_minutes() -> u64 {
    10
}
fn default_hedge_seconds() -> f64 {
    0.0
}
fn default_fail_statuses() -> Vec<u16> {
    vec![429, 500, 502, 503, 504]
}

/// member 的唯一键：`provider:model`。
fn member_key(provider: &str, model: &str) -> String {
    format!("{provider}:{model}")
}

fn now_ms() -> i64 {
    logging::now_ms()
}

fn config_path() -> PathBuf {
    config::config_dir().join("model_rotation.json")
}

/// 原子写：先写临时文件再 rename，避免半截写入。
fn save_config(cfg: &ModelRotationConfig) -> bool {
    let path = config_path();
    let tmp = path.with_extension("json.tmp");
    match serde_json::to_string_pretty(cfg) {
        Ok(text) => {
            if std::fs::write(&tmp, text).is_err() {
                return false;
            }
            std::fs::rename(&tmp, &path).is_ok()
        }
        Err(_) => false,
    }
}

/// 从磁盘读配置；缺失 / 损坏都回落默认（不阻断启动）。
fn load_state() -> RotationState {
    let cfg = match std::fs::read_to_string(config_path()) {
        Ok(text) => serde_json::from_str::<ModelRotationConfig>(&text).unwrap_or_default(),
        Err(_) => ModelRotationConfig::default(),
    };
    RotationState {
        config: cfg,
        windows: HashMap::new(),
        cooldowns: HashMap::new(),
        sessions: HashMap::new(),
    }
}

impl RotationState {
    /// 组里启用的成员下标（按原始顺序）。
    fn active_indices(group: &RotationGroup) -> Vec<usize> {
        group
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.enabled)
            .map(|(i, _)| i)
            .collect()
    }

    /// 把启用成员按权重展开成选路序列。
    fn expanded(group: &RotationGroup, active: &[usize]) -> Vec<usize> {
        let mut out = Vec::new();
        for &i in active {
            let w = group.members[i].weight.max(1) as usize;
            for _ in 0..w {
                out.push(i);
            }
        }
        out
    }

    fn is_cooled(&self, group: &str, key: &str, now: i64) -> bool {
        self.cooldowns
            .get(group)
            .and_then(|m| m.get(key))
            .map(|&until| until > now)
            .unwrap_or(false)
    }

    /// 核心选路：返回虚拟模型名对应的真实目标（已更新窗口 / 会话绑定）。
    fn resolve(&mut self, group: &RotationGroup, session_key: &str) -> Option<ResolvedTarget> {
        let active = Self::active_indices(group);
        if active.is_empty() {
            return None;
        }
        let now = now_ms();
        let expanded = Self::expanded(group, &active);
        if expanded.is_empty() {
            return None;
        }

        // 会话粘性（窗内）：命中且未过期且成员可用 → 直接返回。
        if group.sticky_within_window && !session_key.is_empty() {
            if let Some(bind) = self.sessions.get(session_key) {
                if bind.expire_at_ms > now {
                    if let Some((pi, mi)) = self.lookup_member(group, &bind.member_key) {
                        if mi.enabled && !self.is_cooled(group.name.as_str(), &bind.member_key, now) {
                            return Some(ResolvedTarget {
                                provider: pi.clone(),
                                model: mi.clone(),
                            });
                        }
                    }
                }
            }
        }

        // 时间窗：维护窗口起点与手动偏移。
        let win = self
            .windows
            .entry(group.name.clone())
            .or_insert(WindowInfo {
                started_at_ms: now,
                offset: 0,
            });
        let window_ms = (group.rotate_minutes as i64) * 60_000;
        let window_no = if window_ms > 0 {
            (now - win.started_at_ms) / window_ms
        } else {
            0
        };
        let slot = ((window_no + win.offset).rem_euclid(expanded.len() as i64)) as usize;
        let mut chosen = expanded[slot];

        // 失败转移：窗口成员在冷却 → 顺延到下一个未冷却的启用成员。
        if self.is_cooled(group.name.as_str(), &member_key_of(group, chosen), now) {
            let mut found = None;
            for step in 1..expanded.len() {
                let cand = expanded[(slot + step) % expanded.len()];
                if !self.is_cooled(group.name.as_str(), &member_key_of(group, cand), now) {
                    found = Some(cand);
                    break;
                }
            }
            // 全部冷却：取冷却最早到期的那一位（尽快恢复可用）。
            chosen = found.unwrap_or_else(|| {
                expanded
                    .iter()
                    .copied()
                    .min_by_key(|&i| {
                        self.cooldowns
                            .get(&group.name)
                            .and_then(|m| m.get(&member_key_of(group, i)))
                            .copied()
                            .unwrap_or(0)
                    })
                    .unwrap_or(chosen)
            });
        }

        let target = ResolvedTarget {
            provider: group.members[chosen].provider.clone(),
            model: group.members[chosen].model.clone(),
        };

        // 写会话粘性绑定（窗内）：绑定到本窗结束时刻，窗到点后随轮换前进。
        if group.sticky_within_window && !session_key.is_empty() {
            let expire = if window_ms > 0 {
                win.started_at_ms + window_ms
            } else {
                now + 6 * 3600 * 1000
            };
            self.sessions.insert(
                session_key.to_string(),
                SessionBinding {
                    member_key: member_key_of(group, chosen),
                    expire_at_ms: expire,
                },
            );
        }
        Some(target)
    }

    /// 在组里按 member_key 找回 (provider, model)。
    fn lookup_member(&self, group: &RotationGroup, key: &str) -> Option<(String, String)> {
        group
            .members
            .iter()
            .find(|m| member_key(&m.provider, &m.model) == key)
            .map(|m| (m.provider.clone(), m.model.clone()))
    }

    /// 标记某成员进入冷却（失败转移写侧）。
    fn mark_limited(&mut self, group: &str, key: &str, reset_at_ms: Option<i64>) {
        let until = match reset_at_ms.filter(|v| *v > 0) {
            Some(v) => v,
            None => now_ms() + 10 * 60_000, // 未知恢复时间：默认 10 分钟（见 default_cooldown_minutes）
        };
        self.cooldowns
            .entry(group.to_string())
            .or_default()
            .insert(key.to_string(), until);
    }
}

/// 取成员键（组下标 → `provider:model`）。
fn member_key_of(group: &RotationGroup, idx: usize) -> String {
    let m = &group.members[idx];
    member_key(&m.provider, &m.model)
}

/// 从请求体 / 请求头推导会话键（粘性用）。
/// 优先级：自定义头 `X-Rotation-Session` → `X-Rotate-Session` → body 的 `user` 字段。
fn session_key_of(payload: &Value, headers: &HeaderMap) -> String {
    if let Some(v) = headers.get("x-rotation-session").and_then(|h| h.to_str().ok()) {
        if !v.is_empty() {
            return v.to_string();
        }
    }
    if let Some(v) = headers.get("x-rotate-session").and_then(|h| h.to_str().ok()) {
        if !v.is_empty() {
            return v.to_string();
        }
    }
    payload
        .get("user")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// chat 入口钩子：若 `payload["model"]` 是某个启用组的虚拟名，原地改写为具体模型名。
///
/// 返回 `Some(ResolvedTarget)` 表示发生了轮换解析（调用方可用 `target.provider`
/// 做更精细的 provider 约束，本模块只保证 `payload["model"]` 已是真实模型名）。
/// 返回 `None` 表示不是轮换模型，原样放行。
pub fn resolve_for_request(payload: &mut Value, headers: &HeaderMap) -> Option<ResolvedTarget> {
    let requested = payload.get("model").and_then(Value::as_str)?.to_string();
    // 全局开关关闭 → 不拦截。
    if !state().lock().unwrap().config.enabled {
        return None;
    }
    let group = {
        let st = state().lock().unwrap();
        st.config
            .groups
            .iter()
            .find(|g| g.enabled && g.name == requested)
            .cloned()
    }?;
    let session_key = session_key_of(payload, headers);
    let target = state().lock().unwrap().resolve(&group, &session_key)?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("model".to_string(), Value::String(target.model.clone()));
    }
    Some(target)
}

/// 转发层失败回调（失败转移写侧）：某成员命中失败状态码时登记冷却。
///
/// 集成点：`upstream` 转发在收到终态响应后调用本函数。传入虚拟组名 `group` 与
/// 本次实际使用的 `provider:model`；命中 `fail_statuses` 才冷却。
/// （chat.rs 主链路暂未自动接此回调——见该文件钩子处的说明；手动刷新 /
/// 清冷却仍可用。若需自动失败转移，在 `provider_loop` 判定终态后加一行调用即可。）
pub fn report_member_outcome(group: &str, member_key: &str, success: bool, status: i32) {
    if success {
        // 成功：清掉该成员的冷却，使其尽快回到可用池。
        if let Ok(mut st) = state().lock() {
            if let Some(m) = st.cooldowns.get_mut(group) {
                m.remove(member_key);
            }
        }
        return;
    }
    let cfg = state().lock().unwrap().config.clone();
    let group_cfg = match cfg.groups.iter().find(|g| g.name == group) {
        Some(g) => g,
        None => return,
    };
    if !group_cfg.fail_statuses.contains(&(status as u16)) {
        return;
    }
    let until = now_ms() + (group_cfg.cooldown_minutes as i64) * 60_000;
    state()
        .lock()
        .unwrap()
        .mark_limited(group, member_key, Some(until));
}

/// 手动切换：把某组窗口偏移 +1（切到下一个加权成员）。
pub fn manual_switch(group: &str) -> bool {
    let mut st = state().lock().unwrap();
    if let Some(win) = st.windows.get_mut(group) {
        win.offset += 1;
        win.started_at_ms = now_ms();
        save_config(&st.config);
        true
    } else {
        // 组尚未有窗口（还没被请求过）：建一个并 +1。
        st.windows.insert(
            group.to_string(),
            WindowInfo {
                started_at_ms: now_ms(),
                offset: 1,
            },
        );
        save_config(&st.config);
        true
    }
}

/// 清某组全部冷却（手动刷新）。
pub fn refresh_cooldowns(group: &str) -> bool {
    let mut st = state().lock().unwrap();
    st.cooldowns.remove(group);
    save_config(&st.config);
    true
}

/// 清某组某成员的冷却。
pub fn clear_member_cooldown(group: &str, member_key: &str) -> bool {
    let mut st = state().lock().unwrap();
    if let Some(m) = st.cooldowns.get_mut(group) {
        m.remove(member_key);
    }
    save_config(&st.config);
    true
}

/// 读当前配置（克隆，避免长期持锁）。
pub fn current_config() -> ModelRotationConfig {
    state().lock().unwrap().config.clone()
}

/// 写配置：校验后落盘并替换内存态（窗口 / 冷却 / 会话绑定不清，仍生效）。
pub fn update_config(next: ModelRotationConfig) -> bool {
    // 组名唯一性校验。
    let mut seen = std::collections::HashSet::new();
    for g in &next.groups {
        if !seen.insert(g.name.clone()) {
            return false;
        }
    }
    if save_config(&next) {
        let mut st = state().lock().unwrap();
        st.config = next;
        true
    } else {
        false
    }
}

/// 构造前端状态快照。
pub fn status_json() -> Value {
    let st = state().lock().unwrap();
    let now = now_ms();
    let groups: Vec<Value> = st
        .config
        .groups
        .iter()
        .map(|g| {
            let active = RotationState::active_indices(g);
            let expanded = RotationState::expanded(g, &active);
            let win = st.windows.get(&g.name);
            let (current_idx, next_in_secs) = match win {
                Some(w) => {
                    let window_ms = (g.rotate_minutes as i64) * 60_000;
                    if window_ms > 0 {
                        let slot = (((now - w.started_at_ms) / window_ms + w.offset)
                            .rem_euclid(expanded.len() as i64))
                            as usize;
                        let idx = expanded.get(slot).copied().unwrap_or(0);
                        let elapsed = (now - w.started_at_ms) % window_ms;
                        (idx, ((window_ms - elapsed) / 1000).max(0))
                    } else {
                        (expanded.first().copied().unwrap_or(0), 0)
                    }
                }
                None => (expanded.first().copied().unwrap_or(0), 0),
            };
            let current = g.members.get(current_idx);
            let cooldowns: Vec<Value> = g
                .members
                .iter()
                .filter_map(|m| {
                    let key = member_key(&m.provider, &m.model);
                    st.cooldowns
                        .get(&g.name)
                        .and_then(|cm| cm.get(&key))
                        .map(|&until| {
                            json!({
                                "provider": m.provider,
                                "model": m.model,
                                "remainingSecs": ((until - now) / 1000).max(0),
                            })
                        })
                })
                .collect();
            json!({
                "name": g.name,
                "enabled": g.enabled,
                "rotateMinutes": g.rotate_minutes,
                "cooldownMinutes": g.cooldown_minutes,
                "stickyWithinWindow": g.sticky_within_window,
                "hedgeAfterSeconds": g.hedge_after_seconds,
                "failStatuses": g.fail_statuses,
                "current": current.map(|m| json!({"provider": m.provider, "model": m.model})),
                "nextRotationInSecs": next_in_secs,
                "members": g.members.iter().map(|m| json!({
                    "provider": m.provider,
                    "model": m.model,
                    "weight": m.weight,
                    "enabled": m.enabled,
                })).collect::<Vec<_>>(),
                "cooldowns": cooldowns,
            })
        })
        .collect();
    json!({
        "enabled": st.config.enabled,
        "groups": groups,
    })
}

/// 通用：从字节体解析 JSON 对象（与 `http::parse_body` 同语义，供本模块内部用）。
pub fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    serde_json::from_slice::<T>(bytes).ok()
}

#[allow(dead_code)]
fn _epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
