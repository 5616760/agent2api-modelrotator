// 模型轮换（model-rotator 融合）设置页。
//
// 对应后端 core/model_rotation.rs 与 api/model_rotation.rs：
//   GET  /api/model-rotation/config   读配置
//   PUT  /api/model-rotation/config   整体替换配置
//   GET  /api/model-rotation/status   运行期状态（当前模型 / 下次轮换 / 冷却）
//   POST /api/model-rotation/switch    手动切换某组
//   POST /api/model-rotation/refresh   清某组冷却
//   POST /api/model-rotation/clear-cooldown  清某成员冷却
//
// 鉴权走面板会话 cookie（access.rs 注入），与面板其它 fetch 同源，无需手动带 token。
// 对外契约：window.wbModelRotation = { load }（app.js 切页时调）。

(function () {
  let config = null; // 当前配置（GET 后缓存，Save 前据此重建）
  let statusTimer = null;

  function api(method, path, body) {
    const opts = { method, headers: { 'Content-Type': 'application/json' }, credentials: 'include' };
    if (body !== undefined) opts.body = JSON.stringify(body);
    return fetch(path, opts).then((r) => {
      if (!r.ok) return r.json().then((e) => Promise.reject(new Error(e.error || e.message || r.status)));
      return r.json().catch(() => ({}));
    });
  }

  function el(id) { return document.getElementById(id); }
  function toast(msg, kind) {
    const t = el('toast');
    if (!t) return;
    t.textContent = msg;
    t.className = 'show' + (kind ? ' ' + kind : '');
    clearTimeout(t._t);
    t._t = setTimeout(() => (t.className = ''), 2600);
  }

  // ── 配置表单 ──
  function renderConfig() {
    const root = el('mr-groups');
    if (!root) return;
    if (!config) { root.innerHTML = '<p class="muted">加载中…</p>'; return; }
    const enabled = config.enabled;
    let html = '<label class="mr-global"><input type="checkbox" id="mr-enabled" ' +
      (enabled ? 'checked' : '') + '> 启用模型轮换（关闭后虚拟模型名直接放行，不拦截）</label>';
    (config.groups || []).forEach((g, gi) => {
      html += groupCard(g, gi);
    });
    html += '<div class="mr-actions"><button class="btn primary" id="mr-add-group">+ 新增轮换组</button>' +
      '<button class="btn primary" id="mr-save">保存配置</button></div>';
    root.innerHTML = html;
    bindConfig();
  }

  function groupCard(g, gi) {
    const members = (g.members || []).map((m, mi) =>
      '<div class="mr-member" data-g="' + gi + '" data-m="' + mi + '">' +
        '<input class="mr-m-provider" placeholder="provider" value="' + esc(m.provider) + '">' +
        '<input class="mr-m-model" placeholder="model" value="' + esc(m.model) + '">' +
        '<label>权重<input class="mr-m-weight" type="number" min="1" value="' + (m.weight || 1) + '"></label>' +
        '<label><input class="mr-m-enabled" type="checkbox" ' + (m.enabled !== false ? 'checked' : '') + '> 启用</label>' +
        '<button class="btn ghost mr-m-del">删</button>' +
      '</div>'
    ).join('');
    return (
      '<div class="mr-group card" data-g="' + gi + '">' +
        '<div class="mr-group-head">' +
          '<input class="mr-g-name" placeholder="虚拟模型名（如 AutoRotate）" value="' + esc(g.name) + '">' +
          '<label><input class="mr-g-enabled" type="checkbox" ' + (g.enabled !== false ? 'checked' : '') + '> 启用</label>' +
          '<button class="btn ghost mr-g-del">删除组</button>' +
        '</div>' +
        '<div class="mr-g-grid">' +
          '<label>时间窗(分钟)<input class="mr-g-rotate" type="number" min="0" value="' + (g.rotate_minutes || 30) + '"></label>' +
          '<label>冷却(分钟)<input class="mr-g-cooldown" type="number" min="0" value="' + (g.cooldown_minutes || 10) + '"></label>' +
          '<label>看门狗(秒)<input class="mr-g-hedge" type="number" min="0" step="0.1" value="' + (g.hedge_after_seconds || 0) + '"></label>' +
          '<label><input class="mr-g-sticky" type="checkbox" ' + (g.sticky_within_window !== false ? 'checked' : '') + '> 窗内会话粘性</label>' +
          '<label class="mr-g-fail">失败状态码<input class="mr-g-failstatus" placeholder="429,500,502,503,504" value="' + (g.fail_statuses || []).join(',') + '"></label>' +
        '</div>' +
        '<div class="mr-members"><div class="mr-members-head">成员池 <button class="btn ghost mr-m-add">+ 成员</button></div>' +
          members + '</div>' +
        '<div class="mr-group-ops">' +
          '<button class="btn" data-switch="' + esc(g.name) + '">手动切换</button>' +
          '<button class="btn" data-refresh="' + esc(g.name) + '">清冷却</button>' +
        '</div>' +
      '</div>'
    );
  }

  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c])); }

  function bindConfig() {
    const root = el('mr-groups');
    root.querySelector('#mr-enabled').addEventListener('change', (e) => (config.enabled = e.target.checked));
    root.querySelector('#mr-add-group').addEventListener('click', () => {
      config.groups = config.groups || [];
      config.groups.push({ name: 'AutoRotate', enabled: true, rotate_minutes: 30, cooldown_minutes: 10, sticky_within_window: true, hedge_after_seconds: 0, fail_statuses: [429, 500, 502, 503, 504], members: [{ provider: '', model: '', weight: 1, enabled: true }] });
      renderConfig();
    });
    root.querySelector('#mr-save').addEventListener('click', save);
    root.querySelectorAll('.mr-g-del').forEach((b) => b.addEventListener('click', (e) => {
      const gi = +e.target.closest('.mr-group').dataset.g; config.groups.splice(gi, 1); renderConfig();
    }));
    root.querySelectorAll('.mr-m-add').forEach((b) => b.addEventListener('click', (e) => {
      const gi = +e.target.closest('.mr-group').dataset.g;
      config.groups[gi].members = config.groups[gi].members || [];
      config.groups[gi].members.push({ provider: '', model: '', weight: 1, enabled: true }); renderConfig();
    }));
    root.querySelectorAll('.mr-m-del').forEach((b) => b.addEventListener('click', (e) => {
      const card = e.target.closest('.mr-member'); const gi = +card.dataset.g; const mi = +card.dataset.m;
      config.groups[gi].members.splice(mi, 1); renderConfig();
    }));
    root.querySelectorAll('[data-switch]').forEach((b) => b.addEventListener('click', () => switchGroup(b.dataset.switch)));
    root.querySelectorAll('[data-refresh]').forEach((b) => b.addEventListener('click', () => refresh(b.dataset.refresh)));
  }

  // 从 DOM 重建 config 对象（Save 前）。
  function collectConfig() {
    const root = el('mr-groups');
    config.enabled = root.querySelector('#mr-enabled').checked;
    config.groups = [...root.querySelectorAll('.mr-group')].map((card) => {
      const gi = +card.dataset.g;
      const members = [...card.querySelectorAll('.mr-member')].map((m) => ({
        provider: m.querySelector('.mr-m-provider').value.trim(),
        model: m.querySelector('.mr-m-model').value.trim(),
        weight: Math.max(1, parseInt(m.querySelector('.mr-m-weight').value, 10) || 1),
        enabled: m.querySelector('.mr-m-enabled').checked,
      }));
      return {
        name: card.querySelector('.mr-g-name').value.trim(),
        enabled: card.querySelector('.mr-g-enabled').checked,
        rotate_minutes: parseInt(card.querySelector('.mr-g-rotate').value, 10) || 0,
        cooldown_minutes: parseInt(card.querySelector('.mr-g-cooldown').value, 10) || 0,
        sticky_within_window: card.querySelector('.mr-g-sticky').checked,
        hedge_after_seconds: parseFloat(card.querySelector('.mr-g-hedge').value) || 0,
        fail_statuses: card.querySelector('.mr-g-failstatus').value.split(',').map((s) => parseInt(s.trim(), 10)).filter((n) => !isNaN(n)),
        members,
      };
    });
    return config;
  }

  function save() {
    const cfg = collectConfig();
    api('PUT', '/api/model-rotation/config', cfg)
      .then(() => { toast('配置已保存，对下一个请求立即生效', 'ok'); load(); })
      .catch((e) => toast('保存失败：' + e.message, 'err'));
  }

  function switchGroup(name) {
    api('POST', '/api/model-rotation/switch', { group: name })
      .then(() => { toast('已手动切换到下一个成员', 'ok'); load(); })
      .catch((e) => toast('切换失败：' + e.message, 'err'));
  }
  function refresh(name) {
    api('POST', '/api/model-rotation/refresh', { group: name })
      .then(() => { toast('已清空该组冷却', 'ok'); load(); })
      .catch((e) => toast('清冷却失败：' + e.message, 'err'));
  }
  function clearCooldown(name, provider, model) {
    api('POST', '/api/model-rotation/clear-cooldown', { group: name, provider, model })
      .then(() => load())
      .catch((e) => toast('清冷却失败：' + e.message, 'err'));
  }

  // ── 状态面板 ──
  function renderStatus() {
    const box = el('mr-status');
    if (!box) return;
    api('GET', '/api/model-rotation/status')
      .then((s) => {
        let html = '<div class="mr-stat-line">全局：' + (s.enabled ? '<b>已启用</b>' : '<b>已关闭</b>') + '</div>';
        (s.groups || []).forEach((g) => {
          const cur = g.current ? g.current.provider + ' / ' + g.current.model : '—';
          const cools = (g.cooldowns || []).map((c) => c.provider + ':' + c.model + '（剩 ' + c.remainingSecs + 's）').join('，') || '无';
          html += '<div class="mr-stat-group">' +
            '<div class="mr-stat-title">' + esc(g.name) + (g.enabled ? '' : '（停用）') + '</div>' +
            '<div>当前模型：<b>' + esc(cur) + '</b>　下次轮换：' + (g.nextRotationInSecs || 0) + 's</div>' +
            '<div>冷却中：' + esc(cools) + '</div></div>';
        });
        box.innerHTML = html;
      })
      .catch(() => { box.innerHTML = '<p class="muted">状态读取失败</p>'; });
  }

  function startPolling() {
    stopPolling();
    renderStatus();
    statusTimer = setInterval(renderStatus, 5000);
  }
  function stopPolling() { if (statusTimer) { clearInterval(statusTimer); statusTimer = null; } }

  function load() {
    api('GET', '/api/model-rotation/config')
      .then((cfg) => { config = cfg || { enabled: false, groups: [] }; renderConfig(); startPolling(); })
      .catch((e) => { const r = el('mr-groups'); if (r) r.innerHTML = '<p class="muted">配置读取失败：' + esc(e.message) + '</p>'; });
  }

  window.wbModelRotation = { load, stop: stopPolling };
})();
