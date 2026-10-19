//! 模型轮换（model-rotator 融合）管理 API。
//!
//! 端点（全部挂 `protected`，需面板/API Key 鉴权）：
//!   GET  /api/model-rotation/config          读当前配置
//!   PUT  /api/model-rotation/config          整体替换配置（组名需唯一）
//!   GET  /api/model-rotation/status          运行期状态（当前模型 / 下次轮换 / 冷却）
//!   POST /api/model-rotation/switch           手动切换某组到下一个成员
//!   POST /api/model-rotation/refresh          清空某组冷却
//!   POST /api/model-rotation/clear-cooldown   清某组某成员冷却
//!
//! 与 `routing_api` 同一模式：部分字段由前端保证、保存后立即对下一个请求生效
//! （转发层每次请求都走 `model_rotation::resolve_for_request`），不重启进程。

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::response::Response;
use serde_json::{json, Value};

use crate::server::core::model_rotation as mr;
use crate::server::errors;
use crate::server::http::{ok_json, parse_body};
use crate::server::ServerState;

/// GET /api/model-rotation/config
pub async fn get_config(State(_state): State<ServerState>) -> Response {
    ok_json(serde_json::to_value(mr::current_config()).unwrap_or(Value::Null))
}

/// PUT /api/model-rotation/config —— body 为完整 `ModelRotationConfig`。
pub async fn put_config(State(_state): State<ServerState>, body: Bytes) -> Response {
    let next: mr::ModelRotationConfig = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => return errors::management_error(400, format!("配置解析失败：{error}")),
    };
    if mr::update_config(next) {
        ok_json(serde_json::to_value(mr::current_config()).unwrap_or(Value::Null))
    } else {
        errors::management_error(400, "配置校验失败（组名需唯一，且结构合法）")
    }
}

/// GET /api/model-rotation/status
pub async fn get_status(State(_state): State<ServerState>) -> Response {
    ok_json(mr::status_json())
}

/// POST /api/model-rotation/switch —— body `{"group": "<虚拟模型名>"}`。
pub async fn post_switch(State(_state): State<ServerState>, body: Bytes) -> Response {
    let payload = match parse_body(&body) {
        Ok(value) => value,
        Err(error) => return errors::management_error(400, error.message),
    };
    let group = match payload.get("group").and_then(Value::as_str) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return errors::management_error(400, "缺少 group 字段"),
    };
    if mr::manual_switch(&group) {
        ok_json(json!({ "ok": true, "group": group }))
    } else {
        errors::management_error(404, "未找到该轮换组")
    }
}

/// POST /api/model-rotation/refresh —— body `{"group": "<虚拟模型名>"}`。
pub async fn post_refresh(State(_state): State<ServerState>, body: Bytes) -> Response {
    let payload = match parse_body(&body) {
        Ok(value) => value,
        Err(error) => return errors::management_error(400, error.message),
    };
    let group = match payload.get("group").and_then(Value::as_str) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return errors::management_error(400, "缺少 group 字段"),
    };
    mr::refresh_cooldowns(&group);
    ok_json(json!({ "ok": true, "group": group }))
}

/// POST /api/model-rotation/clear-cooldown
/// body `{"group": "...", "provider": "...", "model": "..."}`。
pub async fn post_clear_cooldown(State(_state): State<ServerState>, body: Bytes) -> Response {
    let payload = match parse_body(&body) {
        Ok(value) => value,
        Err(error) => return errors::management_error(400, error.message),
    };
    let group = match payload.get("group").and_then(Value::as_str) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return errors::management_error(400, "缺少 group 字段"),
    };
    let provider = payload.get("provider").and_then(Value::as_str).unwrap_or("");
    let model = payload.get("model").and_then(Value::as_str).unwrap_or("");
    if provider.is_empty() || model.is_empty() {
        return errors::management_error(400, "缺少 provider / model 字段");
    }
    let key = format!("{provider}:{model}");
    mr::clear_member_cooldown(&group, &key);
    ok_json(json!({ "ok": true, "group": group, "member": key }))
}

/// 兜底：未知子路径统一 404（与 `/api/scheduled-tasks/{*rest}` 同一取舍，
/// 避免落到 axum 的 405 兜底）。
pub async fn not_found() -> Response {
    errors::not_found_response("GET", "/api/model-rotation")
}
