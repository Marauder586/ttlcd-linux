/* ttlcd web UI — app.js */
'use strict';

// ─── Theme ────────────────────────────────────────────────────────────────────
(function () {
  const saved = localStorage.getItem('ttlcd-theme') || 'dark';
  if (saved === 'latte') document.documentElement.setAttribute('data-theme', 'latte');
})();

// ─── DOM refs ─────────────────────────────────────────────────────────────────
const $ = id => document.getElementById(id);

// Header
const hdrVersion    = $('hdr-version');
const currentMode   = $('current-mode');
const returnBtn     = $('return-btn');
const themeToggle   = $('theme-toggle-btn');

// Mode grid
const modeBtns      = document.querySelectorAll('.mode-btn');

// Preview
const panelImg      = $('panel-preview-img');
const panelEmpty    = $('panel-preview-empty');
const freezeBtn     = $('freeze-btn');

// Quick notify
const quickBtns     = document.querySelectorAll('.quick-btn');
const quickStatus   = $('quick-status');
const testPatternBtn= $('test-pattern-btn');

// Custom text
const txtTitle      = $('txt-title');
const txtSubtitle   = $('txt-subtitle');
const txtLines      = $('txt-lines');
const txtProgress   = $('txt-progress');
const progPctLabel  = $('prog-pct-label');
const txtColor      = $('txt-color');
const txtShowTime   = $('txt-show-time');
const txtDuration   = $('txt-duration');
const txtSendBtn    = $('txt-send-btn');
const txtPreviewBtn = $('txt-preview-btn');
const txtStatus     = $('txt-status');
const txtPreviewWrap= $('txt-preview-wrap');
const txtPreviewImg = $('txt-preview-img');

// Presets
const presetSelect  = $('preset-select');
const presetApplyBtn= $('preset-apply-btn');
const presetSaveBtn = $('preset-save-btn');
const presetDelBtn  = $('preset-del-btn');
const presetClearBtn= $('preset-clear-btn');
const presetImport  = $('preset-import-input');

// Image upload
const dropZone      = $('drop-zone');
const fileInput     = $('file-input');
const previewWrap   = $('preview-wrap');
const previewImg    = $('preview');
const sendBtn       = $('send-btn');
const cancelBtn     = $('cancel-btn');
const uploadStatus  = $('upload-status');

// Slideshow
const ssDir         = $('ss-dir');
const ssInterval    = $('ss-interval');
const ssStartBtn    = $('ss-start-btn');
const ssPrevBtn     = $('ss-prev-btn');
const ssNextBtn     = $('ss-next-btn');
const ssShuffleBtn  = $('ss-shuffle-btn');
const ssStatus      = $('ss-status');

// Countdown
const cdHours       = $('cd-hours');
const cdMinutes     = $('cd-minutes');
const cdSecsI       = $('cd-secs-i');
const cdLabel       = $('cd-label');
const cdStartBtn    = $('cd-start-btn');
const cdCancelBtn   = $('cd-cancel-btn');
const cdStatus      = $('cd-status');

// Banner
const banLine1      = $('ban-line1');
const banLine2      = $('ban-line2');
const banLine3      = $('ban-line3');
const banLabel      = $('ban-label');
const banColor      = $('ban-color');
const banSendBtn    = $('ban-send-btn');
const banStatus     = $('ban-status');

// Auto-rotation
const rotModes      = $('rot-modes');
const rotInterval   = $('rot-interval');
const rotStartBtn   = $('rot-start-btn');
const rotStopBtn    = $('rot-stop-btn');
const rotStatus     = $('rot-status');
const rotIndicator  = $('rot-indicator');
const rotModeList   = $('rot-mode-list');
const rotIntervalDisp=$('rot-interval-display');

// Settings
const setQuality    = $('set-quality');
const setQualityLbl = $('set-quality-lbl');
const setOrient     = $('set-orient');
const setBrightness = $('set-brightness');
const setBrightLbl  = $('set-brightness-lbl');
const setApplyBtn   = $('set-apply-btn');
const setStatus     = $('set-status');

// System snapshot
const statCpu       = $('stat-cpu');
const statRam       = $('stat-ram');
const statLoad      = $('stat-load');
const statUptime    = $('stat-uptime');
const statTempBlock = $('stat-temp-block');
const statTemp      = $('stat-temp');
const statNetBlock  = $('stat-net-block');
const statNet       = $('stat-net');
const barCpu        = $('bar-cpu');
const barRam        = $('bar-ram');
const barTemp       = $('bar-temp');
const sparklineWrap = $('sparkline-wrap');
const cpuSparkline  = $('cpu-sparkline');
const netIfacesWrap = $('net-ifaces-wrap');
const netIfacesTable= $('net-ifaces-table');

// Panel info
const infoVersion   = $('info-version');
const infoMode      = $('info-mode');
const infoRotate    = $('info-rotate');
const infoQuality   = $('info-quality');
const infoHeadless  = $('info-headless');
const infoHost      = $('info-host');
const infoSerial    = $('info-serial');
const infoFirmware  = $('info-firmware');
const infoPending   = $('info-pending');
const infoBrightness= $('info-brightness');
const infoFps       = $('info-fps');
const infoDaemonUp  = $('info-daemon-uptime');
const infoFrameSeq  = $('info-frame-seq');
const infoFrozen    = $('info-frozen');
const infoSw        = $('info-sw');

// Schedule
const schedInSecs   = $('sched-in-secs');
const schedLabelEl  = $('sched-label');
const schedBtn      = $('sched-btn');
const schedStatus   = $('sched-status');

// History
const histRefreshBtn= $('hist-refresh-btn');
const histClearBtn  = $('hist-clear-btn');
const historyList   = $('history-list');

// Log
const logEl         = $('log');
const logClearBtn   = $('log-clear-btn');

// ─── App state ────────────────────────────────────────────────────────────────
let lastFrameSeq    = -1;
let lastMode        = '';
let isFrozen        = false;
let imageFile       = null;
let rotationActive  = false;
let statusPollTimer = null;
let snapshotTimer   = null;
let sparklineData   = [];

// Stopwatch local timer
let swSyncState = {
  running: false,
  base_elapsed_ms: 0,
  started_at_unix_ms: 0,
  elapsed_ms: 0,
  laps: [],
};
let swLocalTimer = null;

// ─── Logging ──────────────────────────────────────────────────────────────────
function log(msg, cls = '') {
  const li = document.createElement('li');
  li.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
  if (cls) li.className = cls;
  logEl.prepend(li);
  while (logEl.children.length > 100) logEl.lastChild.remove();
}

// ─── Status display helper ────────────────────────────────────────────────────
function setStatusMsg(el, msg, ok = true, timeout = 3500) {
  if (!el) return;
  el.textContent = msg;
  el.style.color = ok ? 'var(--green)' : 'var(--red)';
  setTimeout(() => { if (el.textContent === msg) el.textContent = ''; }, timeout);
}

// ─── Fetch helpers ────────────────────────────────────────────────────────────
async function api(method, path, body) {
  const opts = { method, headers: {} };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  const r = await fetch(path, opts);
  return r;
}

async function apiJson(method, path, body) {
  const r = await api(method, path, body);
  if (!r.ok) throw new Error(await r.text());
  return r.json();
}

// ─── Theme toggle ─────────────────────────────────────────────────────────────
themeToggle.addEventListener('click', () => {
  const current = document.documentElement.getAttribute('data-theme');
  if (current === 'latte') {
    document.documentElement.removeAttribute('data-theme');
    localStorage.setItem('ttlcd-theme', 'dark');
    themeToggle.textContent = '🌙';
  } else {
    document.documentElement.setAttribute('data-theme', 'latte');
    localStorage.setItem('ttlcd-theme', 'latte');
    themeToggle.textContent = '☀️';
  }
});
// Restore icon
if (localStorage.getItem('ttlcd-theme') === 'latte') themeToggle.textContent = '☀️';

// ─── Mode switching ───────────────────────────────────────────────────────────
modeBtns.forEach(btn => {
  btn.addEventListener('click', async () => {
    const mode = btn.dataset.mode;
    try {
      await api('POST', '/api/mode', { mode });
      log(`Mode → ${mode}`);
      updateActiveModeButton(mode);
    } catch (e) {
      log(`Mode change failed: ${e.message}`, 'err');
    }
  });
});

function updateActiveModeButton(mode) {
  modeBtns.forEach(b => b.classList.toggle('active', b.dataset.mode === mode));
  currentMode.textContent = mode.replace(/_/g, ' ');
  lastMode = mode;
}

// ─── Return button ────────────────────────────────────────────────────────────
returnBtn.addEventListener('click', async () => {
  try {
    await api('POST', '/api/clear');
    log('Returned to previous mode');
    returnBtn.style.display = 'none';
  } catch (e) {
    log(`Clear failed: ${e.message}`, 'err');
  }
});

// ─── Connection status ───────────────────────────────────────────────────────
let connDot = null;
function setConnected(state) {
  // state: 'online' | 'offline' | 'stale'
  if (!connDot) return;
  connDot.className = 'conn-dot' + (state === 'online' ? '' : ` ${state}`);
  connDot.title = state === 'online' ? 'Connected' : state === 'stale' ? 'Reconnecting…' : 'Offline';
}

(function injectConnDot() {
  const bar = $('status-bar');
  if (!bar) return;
  connDot = document.createElement('span');
  connDot.className = 'conn-dot offline';
  connDot.title = 'Connecting…';
  bar.insertBefore(connDot, bar.firstChild);
})();


// ─── Toast notifications ──────────────────────────────────────────────────────
function showToast(msg, type = 'info', durationMs = 4000) {
  const container = document.getElementById('toast-container') || (() => {
    const el = document.createElement('div');
    el.id = 'toast-container';
    document.body.appendChild(el);
    return el;
  })();
  const toast = document.createElement('div');
  toast.className = 'toast toast-' + type;
  toast.textContent = msg;
  container.appendChild(toast);
  // Animate in
  requestAnimationFrame(() => toast.classList.add('toast-visible'));
  setTimeout(() => {
    toast.classList.remove('toast-visible');
    toast.addEventListener('transitionend', () => toast.remove(), { once: true });
  }, durationMs);
}

// ─── Preview refresh via SSE ──────────────────────────────────────────────────
function initSSE() {
  const es = new EventSource('/api/events');
  let reconnectDelay = 2000;

  es.addEventListener('frame', e => {
    setConnected('online');
    try {
      const data = JSON.parse(e.data);
      const seq  = data.seq;
      isFrozen   = data.frozen || false;
      updateFreezeButton();
      if (seq !== lastFrameSeq) {
        lastFrameSeq = seq;
        refreshPreview();
      }
    } catch (_) { refreshPreview(); }
  });

  es.addEventListener('mode', e => {
    setConnected('online');
    try {
      const data = JSON.parse(e.data);
      if (data.mode) {
        updateActiveModeButton(data.mode);
        // Show toast for mode changes
        const modeLabel = data.mode.replace(/_/g, ' ');
        showToast('Mode → ' + modeLabel, 'info', 2500);
      }
    } catch (_) {}
  });

  es.addEventListener('alert', e => {
    setConnected('online');
    try {
      const data = JSON.parse(e.data);
      if (data.summary) showToast(data.summary, 'alert', 6000);
    } catch (_) {}
  });

  es.onerror = () => {
    setConnected('offline');
    es.close();
    setTimeout(initSSE, reconnectDelay);
    reconnectDelay = Math.min(reconnectDelay * 1.5, 30000);
  };

  es.onopen = () => {
    reconnectDelay = 2000;
    setConnected('online');
  };
}

function refreshPreview() {
  const url = `/api/preview?t=${Date.now()}`;
  const img = new Image();
  img.onload = () => {
    panelImg.src = img.src;
    panelImg.style.display = 'block';
    panelEmpty.style.display = 'none';
  };
  img.src = url;
}

// ─── Status polling ───────────────────────────────────────────────────────────
async function fetchStatus() {
  try {
    const d = await apiJson('GET', '/api/status');
    // Header
    if (hdrVersion) hdrVersion.textContent = `v${d.version}`;

    // USB connection status dot
    const usbDot = document.getElementById('usb-status-dot');
    if (usbDot) {
      usbDot.className = 'usb-dot';
      if (d.headless) {
        usbDot.classList.add('usb-dot-headless');
        usbDot.title = 'Headless mode — USB panel not connected';
      } else if (d.panel_connected === false) {
        usbDot.classList.add('usb-dot-error');
        usbDot.title = 'USB panel disconnected — reconnecting…';
      } else {
        usbDot.classList.add('usb-dot-ok');
        usbDot.title = 'USB panel connected';
      }
    }

    // FPS badge in header
    const fpsBadge = document.getElementById('fps-badge');
    if (fpsBadge && d.render_fps != null) {
      fpsBadge.textContent = d.render_fps.toFixed(1) + ' fps';
      fpsBadge.style.display = '';
    }
    // Mode
    if (d.mode !== lastMode) {
      updateActiveModeButton(d.mode);
      // Show Return button for transient modes
      const transient = ['custom_text','custom_image','countdown'];
      returnBtn.style.display = (d.can_clear && transient.some(t => d.mode === t)) ? '' : 'none';
    }
    // Rotation indicator
    if (d.rotation_interval > 0 && d.rotation_modes?.length) {
      rotationActive = true;
      rotIndicator.style.display = 'flex';
      rotModeList.textContent = d.rotation_modes.join(', ');
      rotIntervalDisp.textContent = d.rotation_interval;
    } else {
      rotationActive = false;
      rotIndicator.style.display = 'none';
    }
    // Panel info table
    if (infoVersion)  infoVersion.textContent  = d.version;
    if (infoMode)     infoMode.textContent      = d.mode;
    if (infoRotate)   infoRotate.textContent    = `${d.rotate_deg}°`;
    if (infoQuality)  infoQuality.textContent   = d.quality;
    if (infoHeadless) infoHeadless.textContent  = d.headless ? 'Yes' : 'No';
    if (infoPending)  infoPending.textContent   = d.pending_scheduled ?? '0';
    if (infoBrightness) infoBrightness.textContent = `${Math.round(d.brightness * 100)}%`;
    if (infoFps)      infoFps.textContent       = d.render_fps?.toFixed(1) ?? '—';
    if (infoDaemonUp) infoDaemonUp.textContent  = fmtUptime(d.daemon_uptime_secs ?? 0);
    if (infoFrameSeq) infoFrameSeq.textContent  = d.frame_seq;
    if (infoFrozen)   infoFrozen.textContent    = d.frozen ? '❄ Yes' : 'No';
    if (infoSw)       infoSw.textContent        = d.stopwatch_running
      ? `▶ ${fmtSwTime(d.stopwatch_elapsed_ms)}`
      : (d.stopwatch_elapsed_ms > 0 ? `⏸ ${fmtSwTime(d.stopwatch_elapsed_ms)}` : 'Ready');
    // Frozen state
    isFrozen = d.frozen || false;
    updateFreezeButton();
    // Settings sliders (sync once)
    if (setQuality.dataset.synced !== '1') {
      setQuality.value = d.quality;
      setQualityLbl.textContent = d.quality;
      setQuality.dataset.synced = '1';
    }
    if (setOrient.dataset.synced !== '1') {
      setOrient.value = String(d.rotate_deg);
      setOrient.dataset.synced = '1';
    }
    if (setBrightness.dataset.synced !== '1') {
      setBrightness.value = Math.round(d.brightness * 100);
      setBrightLbl.textContent = `${Math.round(d.brightness * 100)}%`;
      setBrightness.dataset.synced = '1';
    }
  } catch (e) {
    // silent — daemon may be starting
  }
}

function fmtUptime(secs) {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  return `${m}m ${s}s`;
}

// ─── Panel hardware info ──────────────────────────────────────────────────────
async function fetchPanelInfo() {
  try {
    const d = await apiJson('GET', '/api/panel/info');
    if (infoHost)     infoHost.textContent     = window.location.host;
    if (infoSerial)   infoSerial.textContent   = d.serial   ?? (d.headless ? '(headless)' : '—');
    if (infoFirmware) infoFirmware.textContent = d.firmware_byte != null
      ? `0x${d.firmware_byte.toString(16).padStart(2,'0')}` : '—';
  } catch (_) {}
}

async function fetchUsbDevices() {
  try {
    const d = await apiJson('GET', '/api/usb/devices');
    const countEl = document.getElementById('info-usb-count');
    const listEl  = document.getElementById('info-usb-list');
    if (countEl) countEl.textContent = `${d.count} device${d.count !== 1 ? 's' : ''}`;
    if (listEl) {
      listEl.innerHTML = (d.devices || []).map(dev => {
        const label = dev.product || `${dev.vid}:${dev.pid}`;
        const serial = dev.serial ? ` (${dev.serial})` : '';
        const match = dev.vid === '264a' && dev.pid === '2343';
        const style = match ? 'color:var(--green);font-weight:600' : '';
        return `<div style="${style}">${dev.vid}:${dev.pid} — ${label}${serial}</div>`;
      }).join('');
    }
  } catch (_) {}
}

(function initListUsbBtn() {
  const btn = document.getElementById('list-usb-btn');
  if (!btn) return;
  btn.addEventListener('click', async () => {
    btn.disabled = true;
    await fetchUsbDevices();
    btn.disabled = false;
  });
})();

// ─── System snapshot ──────────────────────────────────────────────────────────
async function fetchSnapshot() {
  try {
    const d = await apiJson('GET', '/api/sys/snapshot');
    const cpu = d.cpu_pct ?? 0;
    const ram = d.mem_used != null && d.mem_total
      ? (d.mem_used / d.mem_total * 100) : 0;

    statCpu.textContent     = `${cpu.toFixed(1)}%`;
    statRam.textContent     = `${ram.toFixed(1)}%`;
    barCpu.style.width      = `${cpu}%`;
    barCpu.style.background = cpu > 85 ? 'var(--red)' : cpu > 60 ? 'var(--yellow)' : 'var(--blue)';
    barRam.style.width      = `${ram}%`;
    barRam.style.background = ram > 85 ? 'var(--red)' : ram > 60 ? 'var(--yellow)' : 'var(--green)';

    if (d.load_avg) statLoad.textContent = d.load_avg.join(' / ');
    if (d.uptime_secs != null) statUptime.textContent = fmtUptime(d.uptime_secs);

    // Temperature
    if (d.temperatures?.length) {
      const highest = d.temperatures.reduce((a, b) => (b.current > a.current ? b : a));
      statTempBlock.style.display = '';
      statTemp.textContent = `${highest.current.toFixed(1)}°C`;
      const tPct = Math.min(highest.current / (highest.critical || 105) * 100, 100);
      barTemp.style.width      = `${tPct}%`;
      barTemp.style.background = tPct > 80 ? 'var(--red)' : tPct > 60 ? 'var(--peach)' : 'var(--teal)';
    }

    // Network interfaces
    if (d.network?.length) {
      statNetBlock.style.display = '';
      // Total current Rx/Tx across all interfaces
      const totalRx = d.network.reduce((s, n) => s + (n.rx_bps ?? 0), 0);
      const totalTx = d.network.reduce((s, n) => s + (n.tx_bps ?? 0), 0);
      statNet.textContent = `↓${fmtBytes(totalRx)}/s ↑${fmtBytes(totalTx)}/s`;

      // Network interface table
      netIfacesWrap.style.display = '';
      const tbody = netIfacesTable.querySelector('tbody');
      tbody.innerHTML = '';
      d.network.forEach(iface => {
        const tr = document.createElement('tr');
        tr.innerHTML = `
          <td class="mono">${escHtml(iface.name)}</td>
          <td class="mono">${fmtBytes(iface.rx_bps ?? 0)}/s</td>
          <td class="mono">${fmtBytes(iface.tx_bps ?? 0)}/s</td>
          <td class="mono">${fmtBytes(iface.total_rx ?? 0)}</td>
          <td class="mono">${fmtBytes(iface.total_tx ?? 0)}</td>
        `;
        tbody.appendChild(tr);
      });
    }

    // Disk usage
    if (d.disks?.length) {
      updateDiskTable(d.disks);
    }

    // Top processes
    if (d.processes?.length) {
      updateProcessTable(d.processes);
    }

    // CPU sparkline
    await updateSparkline();

  } catch (_) {}
}

function updateDiskTable(disks) {
  let wrap = $('disk-usage-wrap');
  if (!wrap) {
    wrap = document.createElement('div');
    wrap.id = 'disk-usage-wrap';
    wrap.style.marginTop = '0.75rem';
    wrap.innerHTML = `<div class="sub-label">Disk Usage</div>
      <table class="iface-table" id="disk-table">
        <thead><tr><th>Mount</th><th>Used</th><th>Total</th><th>Used%</th></tr></thead>
        <tbody></tbody>
      </table>`;
    netIfacesWrap?.parentElement?.insertBefore(wrap, netIfacesWrap);
  }
  wrap.style.display = '';
  const tbody = wrap.querySelector('tbody');
  if (!tbody) return;
  tbody.innerHTML = '';
  disks.forEach(disk => {
    if (!disk.total_bytes || disk.total_bytes === 0) return;
    const pct = (disk.used_bytes / disk.total_bytes * 100);
    const tr  = document.createElement('tr');
    tr.innerHTML = `
      <td class="mono">${escHtml(disk.name || disk.mount_point || '?')}</td>
      <td class="mono">${fmtBytes(disk.used_bytes ?? 0)}</td>
      <td class="mono">${fmtBytes(disk.total_bytes)}</td>
      <td>
        <div class="mini-bar-track" style="width:80px;display:inline-block">
          <div class="mini-bar" style="width:${pct.toFixed(0)}%;background:${pct>85?'var(--red)':pct>70?'var(--yellow)':'var(--teal)'}"></div>
        </div>
        <span class="mono" style="font-size:0.75rem;margin-left:4px">${pct.toFixed(0)}%</span>
      </td>
    `;
    tbody.appendChild(tr);
  });
}

function updateProcessTable(procs) {
  let wrap = $('proc-wrap');
  if (!wrap) {
    wrap = document.createElement('div');
    wrap.id = 'proc-wrap';
    wrap.style.marginTop = '0.75rem';
    wrap.innerHTML = `<div class="sub-label">Top Processes <span class="hint-inline">(CPU)</span></div>
      <table class="iface-table" id="proc-table">
        <thead><tr><th>PID</th><th>Name</th><th>CPU%</th><th>RAM</th></tr></thead>
        <tbody></tbody>
      </table>`;
    const sparkWrap = $('sparkline-wrap');
    sparkWrap?.parentElement?.insertBefore(wrap, sparkWrap?.nextSibling);
  }
  wrap.style.display = '';
  const tbody = wrap.querySelector('tbody');
  if (!tbody) return;
  tbody.innerHTML = '';
  procs.slice(0, 6).forEach(p => {
    const tr = document.createElement('tr');
    tr.innerHTML = `
      <td class="mono" style="color:var(--overlay0)">${p.pid}</td>
      <td class="mono">${escHtml((p.name || '').slice(0, 20))}</td>
      <td class="mono" style="color:${(p.cpu_pct||0)>50?'var(--red)':(p.cpu_pct||0)>20?'var(--yellow)':'var(--green)'}">${(p.cpu_pct ?? 0).toFixed(1)}%</td>
      <td class="mono">${fmtBytes(p.mem_bytes ?? 0)}</td>
    `;
    tbody.appendChild(tr);
  });
}

async function updateSparkline() {
  try {
    const d = await apiJson('GET', '/api/cpu/history');
    if (!d.samples?.length) return;
    sparklineData = d.samples;
    sparklineWrap.style.display = '';
    drawSparkline(cpuSparkline, sparklineData);
  } catch (_) {}
}

function drawSparkline(canvas, data) {
  const ctx    = canvas.getContext('2d');
  const W      = canvas.width;
  const H      = canvas.height;
  const n      = data.length;
  const barW   = W / Math.max(n, 1);
  ctx.clearRect(0, 0, W, H);

  // Background
  ctx.fillStyle = 'rgba(49,50,68,0.4)';
  ctx.fillRect(0, 0, W, H);

  // Gridlines at 25%, 50%, 75%
  ctx.strokeStyle = 'rgba(108,112,134,0.25)';
  ctx.lineWidth = 1;
  [0.25, 0.5, 0.75].forEach(f => {
    const y = H - H * f;
    ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(W, y); ctx.stroke();
  });

  // Bars
  data.forEach((v, i) => {
    const h    = Math.round(H * Math.min(v / 100, 1));
    const r    = v > 85 ? '#f38ba8' : v > 60 ? '#f9e2af' : '#89b4fa';
    ctx.fillStyle = r;
    ctx.fillRect(i * barW, H - h, barW - 1, h);
  });
}

function fmtBytes(b) {
  if (b < 1024)       return `${b}B`;
  if (b < 1048576)    return `${(b/1024).toFixed(1)}K`;
  if (b < 1073741824) return `${(b/1048576).toFixed(1)}M`;
  return `${(b/1073741824).toFixed(2)}G`;
}

function escHtml(s) {
  return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

// ─── Quick notify ─────────────────────────────────────────────────────────────
const QUICK_PRESETS = {
  ok:     { title:'✓ OK',        lines:['All systems operational'], color:'green', duration:8 },
  fail:   { title:'✗ FAILED',    lines:['Operation failed'],        color:'red',   duration:15 },
  warn:   { title:'⚠ Warning',   lines:['Check required'],          color:'yellow',duration:12 },
  run:    { title:'⟳ Running',   lines:['Operation in progress…'],  color:'teal',  duration:0 },
  deploy: { title:'↑ Deploying', lines:['Deploy in progress…'],     color:'blue',  duration:0 },
  done:   { title:'✓ Done',      lines:['Task completed'],          color:'green', duration:10 },
  info:   { title:'ℹ Info',      lines:['FYI'],                     color:'blue',  duration:8 },
  reboot: { title:'↺ Rebooting', lines:['System restart initiated'],color:'peach', duration:0 },
};

quickBtns.forEach(btn => {
  const key = btn.dataset.quick;
  if (!key) return;
  btn.addEventListener('click', async () => {
    const p = QUICK_PRESETS[key];
    try {
      await api('POST', '/api/text', {
        title: p.title, lines: p.lines, color: p.color,
        duration_secs: p.duration || undefined,
        show_time: true,
      });
      setStatusMsg(quickStatus, `Sent: ${p.title}`, true);
      log(`Quick: ${p.title}`);
    } catch (e) {
      setStatusMsg(quickStatus, `Error: ${e.message}`, false);
    }
  });
});

// Test pattern
testPatternBtn.addEventListener('click', async () => {
  try {
    await api('POST', '/api/test');
    setStatusMsg(quickStatus, '🎨 Test card sent (5s)', true);
    log('Test pattern sent');
  } catch (e) {
    setStatusMsg(quickStatus, `Error: ${e.message}`, false);
  }
});

// ─── Custom text ──────────────────────────────────────────────────────────────
txtProgress.addEventListener('input', () => {
  const v = parseInt(txtProgress.value, 10);
  progPctLabel.textContent = v < 0 ? 'off' : `${v}%`;
});

function buildTextPayload() {
  const lines = txtLines.value.split('\n').map(l => l.trim()).filter(Boolean);
  const prog  = parseInt(txtProgress.value, 10);
  return {
    title:        txtTitle.value.trim() || undefined,
    subtitle:     txtSubtitle.value.trim() || undefined,
    lines:        lines.length ? lines : undefined,
    progress:     prog >= 0 ? prog : undefined,
    color:        txtColor.value,
    show_time:    txtShowTime.checked,
    duration_secs: parseInt(txtDuration.value,10) > 0
      ? parseInt(txtDuration.value,10) : undefined,
  };
}

txtSendBtn.addEventListener('click', async () => {
  try {
    await api('POST', '/api/text', buildTextPayload());
    setStatusMsg(txtStatus, '✓ Sent', true);
    log(`Text push: "${txtTitle.value || '(no title)'}"`);
  } catch (e) {
    setStatusMsg(txtStatus, `Error: ${e.message}`, false);
  }
});

txtPreviewBtn.addEventListener('click', async () => {
  try {
    const r = await api('POST', '/api/render', buildTextPayload());
    if (!r.ok) throw new Error(await r.text());
    const blob = await r.blob();
    const url  = URL.createObjectURL(blob);
    txtPreviewImg.src = url;
    txtPreviewWrap.style.display = '';
    setStatusMsg(txtStatus, '👁 Preview rendered', true);
  } catch (e) {
    setStatusMsg(txtStatus, `Error: ${e.message}`, false);
  }
});

// ─── Presets ──────────────────────────────────────────────────────────────────
async function fetchPresets() {
  try {
    const list = await apiJson('GET', '/api/presets');
    presetSelect.innerHTML = '<option value="">— saved presets —</option>';
    list.forEach(p => {
      const opt = document.createElement('option');
      opt.value       = p.name;
      opt.textContent = p.name;
      presetSelect.appendChild(opt);
    });
  } catch (_) {}
}

presetApplyBtn.addEventListener('click', async () => {
  const name = presetSelect.value;
  if (!name) return;
  try {
    // Load payload into form
    const p = await apiJson('GET', `/api/presets/${encodeURIComponent(name)}`);
    txtTitle.value    = p.title    || '';
    txtSubtitle.value = p.subtitle || '';
    txtLines.value    = (p.lines   || []).join('\n');
    txtProgress.value = p.progress ?? -1;
    progPctLabel.textContent = (p.progress ?? -1) < 0 ? 'off' : `${p.progress}%`;
    if (p.color) txtColor.value = p.color;
    txtShowTime.checked = p.show_time !== false;
    // Apply to panel
    await api('POST', `/api/presets/${encodeURIComponent(name)}/apply`);
    log(`Preset applied: ${name}`);
  } catch (e) {
    log(`Preset apply error: ${e.message}`, 'err');
  }
});

presetSaveBtn.addEventListener('click', async () => {
  const name = presetSelect.value || prompt('Preset name:');
  if (!name?.trim()) return;
  try {
    await api('POST', '/api/presets', { name: name.trim(), payload: buildTextPayload() });
    log(`Preset saved: ${name}`);
    await fetchPresets();
    presetSelect.value = name;
  } catch (e) {
    log(`Preset save error: ${e.message}`, 'err');
  }
});

presetDelBtn.addEventListener('click', async () => {
  const name = presetSelect.value;
  if (!name) return;
  if (!confirm(`Delete preset "${name}"?`)) return;
  try {
    await api('DELETE', `/api/presets/${encodeURIComponent(name)}`);
    log(`Preset deleted: ${name}`);
    await fetchPresets();
  } catch (e) {
    log(`Preset delete error: ${e.message}`, 'err');
  }
});

presetClearBtn.addEventListener('click', async () => {
  if (!confirm('Delete ALL presets?')) return;
  try {
    await api('DELETE', '/api/presets');
    log('All presets cleared');
    await fetchPresets();
  } catch (e) {
    log(`Preset clear error: ${e.message}`, 'err');
  }
});

presetImport.addEventListener('change', async () => {
  const file = presetImport.files?.[0];
  if (!file) return;
  const text = await file.text();
  const fd   = new FormData();
  fd.append('file', new Blob([text], { type:'application/json' }), file.name);
  try {
    const r = await fetch('/api/presets/import', { method:'POST', body: fd });
    if (!r.ok) throw new Error(await r.text());
    log(`Presets imported from ${file.name}`);
    await fetchPresets();
  } catch (e) {
    log(`Preset import error: ${e.message}`, 'err');
  }
  presetImport.value = '';
});

// ─── Image upload ─────────────────────────────────────────────────────────────
function handleFiles(files) {
  if (!files?.[0]) return;
  imageFile = files[0];
  const reader = new FileReader();
  reader.onload = e => {
    previewImg.src    = e.target.result;
    previewWrap.classList.remove('hidden');
    previewWrap.style.display = '';
  };
  reader.readAsDataURL(imageFile);
}

dropZone.addEventListener('click', () => fileInput.click());
fileInput.addEventListener('change', () => handleFiles(fileInput.files));

dropZone.addEventListener('dragover', e => {
  e.preventDefault();
  dropZone.classList.add('drag-over');
});
dropZone.addEventListener('dragleave', () => dropZone.classList.remove('drag-over'));
dropZone.addEventListener('drop', e => {
  e.preventDefault();
  dropZone.classList.remove('drag-over');
  handleFiles(e.dataTransfer.files);
});

sendBtn.addEventListener('click', async () => {
  if (!imageFile) return;
  const fd = new FormData();
  fd.append('file', imageFile);
  uploadStatus.textContent = 'Uploading…';
  try {
    const r = await fetch('/api/image', { method: 'POST', body: fd });
    if (!r.ok) throw new Error(await r.text());
    uploadStatus.textContent = '✓ Image sent';
    log(`Image sent: ${imageFile.name}`);
    setTimeout(() => { uploadStatus.textContent = ''; }, 3000);
  } catch (e) {
    uploadStatus.textContent = `Error: ${e.message}`;
    log(`Image upload error: ${e.message}`, 'err');
  }
});

cancelBtn.addEventListener('click', () => {
  imageFile = null;
  previewWrap.classList.add('hidden');
  previewWrap.style.display = 'none';
  fileInput.value = '';
});

// ─── Image from URL ──────────────────────────────────────────────────────────
document.addEventListener('click', async function(e) {
  if (e.target.id !== 'img-url-send-btn') return;
  var urlInput = document.getElementById('img-url-input');
  var letterboxSel = document.getElementById('img-url-letterbox');
  var statusEl = document.getElementById('img-url-status');
  var url = (urlInput?.value || '').trim();
  if (!url) { setStatusMsg(statusEl, 'Enter a URL first', false); return; }
  var letterbox = letterboxSel?.value !== 'false';
  setStatusMsg(statusEl, '⏳ Fetching image…', true, 30000);
  try {
    await api('POST', '/api/image/url', { url: url, letterbox: letterbox });
    setStatusMsg(statusEl, '✓ Image displayed', true);
    log('Image from URL: ' + url.substring(0, 60));
  } catch (e) {
    setStatusMsg(statusEl, 'Error: ' + e.message, false);
    log('Image URL error: ' + e.message, 'err');
  }
});

// ─── Slideshow ────────────────────────────────────────────────────────────────
ssStartBtn.addEventListener('click', async () => {
  const dir = ssDir.value.trim();
  if (!dir) { setStatusMsg(ssStatus, 'Enter a directory path', false); return; }
  try {
    await api('POST', '/api/slideshow', {
      dir,
      interval: parseInt(ssInterval.value, 10) || 5,
    });
    setStatusMsg(ssStatus, '▶ Slideshow started', true);
    log(`Slideshow started: ${dir}`);
  } catch (e) {
    setStatusMsg(ssStatus, `Error: ${e.message}`, false);
  }
});

ssPrevBtn.addEventListener('click', async () => {
  try { await api('POST', '/api/slideshow/prev'); } catch (_) {}
});
ssNextBtn.addEventListener('click', async () => {
  try { await api('POST', '/api/slideshow/next'); } catch (_) {}
});
ssShuffleBtn.addEventListener('click', async () => {
  try {
    await api('POST', '/api/slideshow/shuffle');
    setStatusMsg(ssStatus, '🔀 Shuffled', true);
  } catch (e) {
    setStatusMsg(ssStatus, `Error: ${e.message}`, false);
  }
});

// ─── Countdown timer ──────────────────────────────────────────────────────────
// Sync hidden total field from h/m/s pickers
function syncCdSeconds() {
  const h = parseInt(cdHours.value, 10)   || 0;
  const m = parseInt(cdMinutes.value, 10) || 0;
  const s = parseInt(cdSecsI.value, 10)   || 0;
  $('cd-seconds').value = h * 3600 + m * 60 + s;
}
[cdHours, cdMinutes, cdSecsI].forEach(el => el.addEventListener('input', syncCdSeconds));

cdStartBtn.addEventListener('click', async () => {
  syncCdSeconds();
  const secs  = parseInt($('cd-seconds').value, 10);
  if (!secs) { setStatusMsg(cdStatus, 'Set a time > 0', false); return; }
  const label = cdLabel.value.trim() || undefined;
  try {
    await api('POST', '/api/countdown', { seconds: secs, label });
    setStatusMsg(cdStatus, `⏳ ${fmtUptime(secs)} countdown started`, true);
    log(`Countdown started: ${secs}s`);
  } catch (e) {
    setStatusMsg(cdStatus, `Error: ${e.message}`, false);
  }
});

cdCancelBtn.addEventListener('click', async () => {
  try {
    await api('DELETE', '/api/countdown');
    setStatusMsg(cdStatus, '✕ Countdown cancelled', true);
    log('Countdown cancelled');
  } catch (e) {
    setStatusMsg(cdStatus, `Error: ${e.message}`, false);
  }
});

// Countdown preset bar (added below the form via JS since it's not in HTML)
(function buildCdPresets() {
  const presets = [
    { label:'5m',  secs: 300 },
    { label:'10m', secs: 600 },
    { label:'15m', secs: 900 },
    { label:'25m 🍅', secs: 1500 },
    { label:'1h',  secs: 3600 },
  ];
  const bar = document.createElement('div');
  bar.className = 'cd-preset-bar';
  presets.forEach(p => {
    const btn = document.createElement('button');
    btn.className   = 'cd-preset-btn';
    btn.textContent = p.label;
    btn.title       = `Start ${p.label} countdown`;
    btn.addEventListener('click', async () => {
      try {
        await api('POST', '/api/countdown', { seconds: p.secs });
        setStatusMsg(cdStatus, `⏳ ${p.label} countdown started`, true);
        log(`Preset countdown started: ${p.label}`);
      } catch (e) {
        setStatusMsg(cdStatus, `Error: ${e.message}`, false);
      }
    });
    bar.appendChild(btn);
  });
  cdCancelBtn.parentElement.insertAdjacentElement('afterend', bar);
})();

// ─── Big text banner ──────────────────────────────────────────────────────────
banSendBtn.addEventListener('click', async () => {
  const lines = [banLine1.value, banLine2.value, banLine3.value].filter(Boolean);
  if (!lines.length) { setStatusMsg(banStatus, 'Enter at least one line', false); return; }
  try {
    await api('POST', '/api/banner', {
      lines,
      label:     banLabel.value.trim() || undefined,
      color:     banColor.value,
      show_time: true,
      duration:  0,
    });
    setStatusMsg(banStatus, '✓ Banner shown', true);
    log(`Banner: ${lines[0]}`);
  } catch (e) {
    setStatusMsg(banStatus, `Error: ${e.message}`, false);
  }
});

// ─── Auto-rotation ────────────────────────────────────────────────────────────
rotStartBtn.addEventListener('click', async () => {
  const modes    = rotModes.value.split(',').map(m => m.trim()).filter(Boolean);
  const interval = parseInt(rotInterval.value, 10) || 30;
  if (!modes.length) { setStatusMsg(rotStatus, 'Enter at least one mode', false); return; }
  try {
    await api('POST', '/api/rotation', { modes, interval });
    setStatusMsg(rotStatus, '▶ Rotation active', true);
    log(`Auto-rotation started: ${modes.join(', ')} every ${interval}s`);
  } catch (e) {
    setStatusMsg(rotStatus, `Error: ${e.message}`, false);
  }
});

rotStopBtn.addEventListener('click', async () => {
  try {
    await api('POST', '/api/rotation', { modes: [], interval: 0 });
    setStatusMsg(rotStatus, '■ Rotation stopped', true);
    log('Auto-rotation stopped');
  } catch (e) {
    setStatusMsg(rotStatus, `Error: ${e.message}`, false);
  }
});

// ─── Panel settings ───────────────────────────────────────────────────────────
setQuality.addEventListener('input', () => {
  setQualityLbl.textContent = setQuality.value;
});

setBrightness.addEventListener('input', () => {
  setBrightLbl.textContent = `${setBrightness.value}%`;
});

setApplyBtn.addEventListener('click', async () => {
  try {
    await api('POST', '/api/quality',    { quality: parseInt(setQuality.value, 10) });
    await api('POST', '/api/orient',     { degrees: parseInt(setOrient.value, 10) });
    await api('POST', '/api/brightness', { brightness: parseInt(setBrightness.value, 10) / 100 });
    // Re-enable sync on next poll so sliders reflect server state
    setQuality.dataset.synced    = '1';
    setOrient.dataset.synced     = '1';
    setBrightness.dataset.synced = '1';
    setStatusMsg(setStatus, '✓ Settings applied', true);
    log(`Settings: quality=${setQuality.value} rotate=${setOrient.value}° brightness=${setBrightness.value}%`);
  } catch (e) {
    setStatusMsg(setStatus, `Error: ${e.message}`, false);
  }
});

// ─── Freeze / unfreeze ────────────────────────────────────────────────────────
function updateFreezeButton() {
  if (!freezeBtn) return;
  if (isFrozen) {
    freezeBtn.textContent = '▶ Unfreeze';
    freezeBtn.classList.add('active');
  } else {
    freezeBtn.textContent = '❄ Freeze';
    freezeBtn.classList.remove('active');
  }
}

freezeBtn.addEventListener('click', async () => {
  try {
    if (isFrozen) {
      await api('POST', '/api/display/unfreeze');
      isFrozen = false;
      log('Display unfrozen');
    } else {
      await api('POST', '/api/display/freeze');
      isFrozen = true;
      log('Display frozen');
    }
    updateFreezeButton();
  } catch (e) {
    log(`Freeze toggle error: ${e.message}`, 'err');
  }
});

// ─── Schedule notification ────────────────────────────────────────────────────
schedBtn.addEventListener('click', async () => {
  const secs = parseInt(schedInSecs.value, 10);
  if (!secs || secs < 1) { setStatusMsg(schedStatus, 'Enter delay > 0', false); return; }
  try {
    await api('POST', '/api/schedule', {
      delay_secs: secs,
      label:      schedLabelEl.value.trim() || undefined,
      payload:    buildTextPayload(),
    });
    setStatusMsg(schedStatus, `✓ Scheduled in ${fmtUptime(secs)}`, true);
    log(`Notification scheduled: +${secs}s`);
  } catch (e) {
    setStatusMsg(schedStatus, `Error: ${e.message}`, false);
  }
});

// ─── Notification history ─────────────────────────────────────────────────────
var histKindFilter = '';

async function fetchHistory(kind) {
  if (kind !== undefined) histKindFilter = kind;
  try {
    var url = '/api/history?limit=30';
    if (histKindFilter) url += '&kind=' + histKindFilter;
    var d = await apiJson('GET', url);
    var entries = Array.isArray(d) ? d : (d.entries || []);
    historyList.innerHTML = '';
    if (!entries.length) {
      var li = document.createElement('li');
      li.textContent = histKindFilter ? ('No ' + histKindFilter + ' entries') : 'No history yet';
      li.style.color = 'var(--overlay0)';
      historyList.appendChild(li);
    } else {
      entries.forEach(function(entry) {
        var li = document.createElement('li');
        var ts   = new Date((entry.timestamp != null ? entry.timestamp : (entry.ts_secs || 0)) * 1000).toLocaleTimeString();
        var ekind = entry.kind || 'text';
        var msg  = (entry.summary != null) ? entry.summary : (entry.message || '');
        li.innerHTML = '<span class="hist-ts">' + ts + '</span>'
          + '<span class="hist-kind hist-' + ekind + '">' + ekind + '</span>'
          + '<span class="hist-msg">' + escHtml(msg) + '</span>';
        historyList.appendChild(li);
      });
    }
    document.querySelectorAll('.hist-filter-btn').forEach(function(b) {
      b.classList.toggle('active', b.dataset.kind === histKindFilter);
    });
    // Update CSV download link to match current filter
    var csvLink = document.querySelector('a[download="ttlcd-history.csv"]');
    if (csvLink) {
      var csvUrl = '/api/history?format=csv&limit=500';
      if (histKindFilter) csvUrl += '&kind=' + histKindFilter;
      csvLink.href = csvUrl;
    }
  } catch (_) {}
}

histRefreshBtn.addEventListener('click', function() { fetchHistory(); });
histClearBtn.addEventListener('click', async () => {
  try {
    await api('DELETE', '/api/history');
    log('History cleared');
    await fetchHistory();
  } catch (e) {
    log(`History clear error: ${e.message}`, 'err');
  }
});

logClearBtn.addEventListener('click', () => { logEl.innerHTML = ''; });

// ─── Stopwatch ────────────────────────────────────────────────────────────────
function fmtSwTime(ms) {
  ms = Math.max(0, Math.floor(ms));
  const h  = Math.floor(ms / 3600000);
  const m  = Math.floor((ms % 3600000) / 60000);
  const s  = Math.floor((ms % 60000) / 1000);
  const cs = Math.floor((ms % 1000) / 10);
  const pad2 = n => String(n).padStart(2,'0');
  if (h > 0) return `${h}:${pad2(m)}:${pad2(s)}.${pad2(cs)}`;
  return `${pad2(m)}:${pad2(s)}.${pad2(cs)}`;
}

function swCurrentMs() {
  if (swSyncState.running && swSyncState.started_at_unix_ms > 0) {
    return swSyncState.base_elapsed_ms + (Date.now() - swSyncState.started_at_unix_ms);
  }
  return swSyncState.elapsed_ms;
}

function updateSwDisplay() {
  const swDisplay    = $('sw-display');
  const swStatusLbl  = $('sw-status-label');
  if (!swDisplay) return;
  swDisplay.textContent = fmtSwTime(swCurrentMs());
  if (swStatusLbl) {
    swStatusLbl.textContent = swSyncState.running ? 'RUNNING'
      : (swSyncState.elapsed_ms > 0 ? 'PAUSED' : 'READY');
    swStatusLbl.className = 'sw-status-label ' + (swSyncState.running ? 'sw-running'
      : (swSyncState.elapsed_ms > 0 ? 'sw-paused' : ''));
  }
}

function startLocalSwTimer() {
  if (swLocalTimer) return;
  swLocalTimer = setInterval(updateSwDisplay, 50);
}

function stopLocalSwTimer() {
  if (swLocalTimer) { clearInterval(swLocalTimer); swLocalTimer = null; }
  updateSwDisplay();
}

async function fetchStopwatch() {
  try {
    const d = await apiJson('GET', '/api/stopwatch');
    swSyncState = {
      running:            d.running,
      base_elapsed_ms:    d.base_elapsed_ms,
      started_at_unix_ms: d.started_at_unix_ms,
      elapsed_ms:         d.elapsed_ms,
      laps:               d.laps || [],
    };
    if (d.running) startLocalSwTimer(); else stopLocalSwTimer();
    renderSwLaps(d.laps || []);
  } catch (_) {}
}

function renderSwLaps(laps) {
  const table = $('sw-laps-table');
  const tbody = document.querySelector('#sw-laps-table tbody');
  if (!tbody) return;
  tbody.innerHTML = '';
  laps.forEach((cumMs, idx) => {
    const prev  = idx > 0 ? laps[idx-1] : 0;
    const delta = cumMs - prev;
    const tr    = document.createElement('tr');
    if (idx === laps.length - 1) tr.className = 'sw-latest-lap';
    tr.innerHTML = `<td>${idx + 1}</td>
      <td class="mono">${fmtSwTime(cumMs)}</td>
      <td class="mono sw-delta">+${fmtSwTime(delta)}</td>`;
    tbody.prepend(tr);
  });
  if (table) table.style.display = laps.length ? '' : 'none';
}

// Stopwatch buttons — injected section in index.html (IDs: sw-start-btn, etc.)
document.addEventListener('click', async e => {
  const id = e.target.closest('button')?.id;
  if (!id) return;
  switch (id) {
    case 'sw-start-btn':
      try {
        await api('POST', swSyncState.running ? '/api/stopwatch/stop' : '/api/stopwatch/start');
        await fetchStopwatch();
      } catch (err) { log(`Stopwatch error: ${err.message}`, 'err'); }
      break;
    case 'sw-lap-btn':
      try {
        const r = await apiJson('POST', '/api/stopwatch/lap');
        log(`Lap ${r.lap}: ${fmtSwTime(r.elapsed_ms)}`);
        await fetchStopwatch();
      } catch (err) { log(`Lap error: ${err.message}`, 'err'); }
      break;
    case 'sw-reset-btn':
      try {
        await api('POST', '/api/stopwatch/reset');
        swSyncState = { running:false, base_elapsed_ms:0, started_at_unix_ms:0, elapsed_ms:0, laps:[] };
        stopLocalSwTimer();
        renderSwLaps([]);
        log('Stopwatch reset');
      } catch (err) { log(`Reset error: ${err.message}`, 'err'); }
      break;
  }
});

// Inject stopwatch section if not already present
(function injectStopwatchSection() {
  if ($('sw-display')) return; // already present
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Stopwatch</h2>
    <div class="sw-main">
      <div class="stopwatch-display" id="sw-display">00:00.00</div>
      <div class="stopwatch-status" id="sw-status-label">READY</div>
    </div>
    <div class="form-actions" style="margin-top:0.75rem">
      <button id="sw-start-btn" class="primary">▶ Start</button>
      <button id="sw-lap-btn">⊙ Lap</button>
      <button id="sw-reset-btn">↺ Reset</button>
    </div>
    <table id="sw-laps-table" class="sw-laps-table" style="margin-top:0.75rem;width:100%;display:none">
      <thead><tr><th>#</th><th>Cumulative</th><th>Split</th></tr></thead>
      <tbody></tbody>
    </table>
  `;
  // Insert after panel preview section
  const previewSection = freezeBtn.closest('section');
  previewSection?.insertAdjacentElement('afterend', section);
})();

// ─── Alert thresholds ─────────────────────────────────────────────────────────
(function injectThresholdsSection() {
  if ($('thr-cpu')) return;
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Alert Thresholds</h2>
    <p class="card-hint">Panel flashes a custom text alert when CPU or temperature exceeds these limits.</p>
    <div class="form-row three-col">
      <div class="form-group">
        <label for="thr-cpu">CPU % trigger <span class="hint-inline">(0 = off)</span></label>
        <input type="number" id="thr-cpu" min="0" max="100" value="0" step="1">
      </div>
      <div class="form-group">
        <label for="thr-temp">Temp °C trigger <span class="hint-inline">(0 = off)</span></label>
        <input type="number" id="thr-temp" min="0" max="200" value="0" step="1">
      </div>
      <div class="form-group">
        <label for="thr-cooldown">Cooldown (s)</label>
        <input type="number" id="thr-cooldown" min="0" value="300" step="30">
      </div>
    </div>
    <div class="form-actions">
      <button id="thr-save-btn" class="primary">💾 Save Thresholds</button>
      <span id="thr-status"></span>
    </div>
  `;
  const settingsSection = setApplyBtn.closest('section');
  settingsSection?.insertAdjacentElement('afterend', section);
})();

async function fetchThresholds() {
  try {
    const d = await apiJson('GET', '/api/alerts/thresholds');
    const thrCpu = $('thr-cpu'), thrTemp = $('thr-temp'), thrCooldown = $('thr-cooldown');
    if (thrCpu)      thrCpu.value      = d.cpu_pct       ?? 0;
    if (thrTemp)     thrTemp.value     = d.temp_c        ?? 0;
    if (thrCooldown) thrCooldown.value = d.cooldown_secs ?? 300;
  } catch (_) {}
}

document.addEventListener('click', async e => {
  if (e.target.id !== 'thr-save-btn') return;
  try {
    await api('POST', '/api/alerts/thresholds', {
      cpu_pct:       parseFloat($('thr-cpu')?.value || 0),
      temp_c:        parseFloat($('thr-temp')?.value || 0),
      cooldown_secs: parseInt($('thr-cooldown')?.value || 300, 10),
    });
    setStatusMsg($('thr-status'), '✓ Saved', true);
    log('Alert thresholds updated');
  } catch (e) {
    setStatusMsg($('thr-status'), `Error: ${e.message}`, false);
  }
});

// ─── Night dim schedule ───────────────────────────────────────────────────────
(function injectDimSection() {
  if ($('dim-start')) return;
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Night Dim Schedule</h2>
    <p class="card-hint">Automatically dim the panel during a specified hour range (e.g. 22:00 – 07:00).</p>
    <div class="form-row three-col">
      <div class="form-group">
        <label for="dim-start">Start hour <span class="hint-inline">(0–23)</span></label>
        <input type="number" id="dim-start" min="0" max="23" value="22">
      </div>
      <div class="form-group">
        <label for="dim-end">End hour <span class="hint-inline">(0–23)</span></label>
        <input type="number" id="dim-end" min="0" max="23" value="7">
      </div>
      <div class="form-group">
        <label for="dim-level">Dim level <span class="hint-inline">(0–100%)</span></label>
        <input type="number" id="dim-level" min="0" max="100" value="20">
      </div>
    </div>
    <div class="form-actions">
      <button id="dim-set-btn" class="primary">🌙 Set Schedule</button>
      <button id="dim-clear-btn">✕ Clear</button>
      <span id="dim-status"></span>
    </div>
    <div id="dim-active-info" style="display:none;margin-top:0.5rem;font-size:0.85rem;color:var(--subtext)"></div>
  `;
  $('thr-save-btn')?.closest('section')?.insertAdjacentElement('afterend', section);
})();

async function fetchDimSchedule() {
  try {
    const d  = await apiJson('GET', '/api/dim/schedule');
    const sched = d.schedule;
    const info  = $('dim-active-info');
    if (sched) {
      const dimStart = $('dim-start'), dimEnd = $('dim-end'), dimLevel = $('dim-level');
      if (dimStart) dimStart.value = sched.start_hour;
      if (dimEnd)   dimEnd.value   = sched.end_hour;
      if (dimLevel) dimLevel.value = Math.round(sched.dim_level * 100);
      if (info) {
        info.style.display = '';
        info.textContent   = `Active: ${sched.start_hour}:00 → ${sched.end_hour}:00 @ ${Math.round(sched.dim_level*100)}%`;
      }
    } else if (info) {
      info.style.display = 'none';
    }
  } catch (_) {}
}

document.addEventListener('click', async e => {
  const id = e.target.id;
  if (id === 'dim-set-btn') {
    try {
      await api('POST', '/api/dim/schedule', {
        start_hour: parseInt($('dim-start')?.value || 22, 10),
        end_hour:   parseInt($('dim-end')?.value   || 7,  10),
        dim_level:  parseInt($('dim-level')?.value || 20, 10) / 100,
      });
      setStatusMsg($('dim-status'), '✓ Schedule set', true);
      log('Dim schedule updated');
      await fetchDimSchedule();
    } catch (err) {
      setStatusMsg($('dim-status'), `Error: ${err.message}`, false);
    }
  } else if (id === 'dim-clear-btn') {
    try {
      await api('DELETE', '/api/dim/schedule');
      setStatusMsg($('dim-status'), '✓ Cleared', true);
      log('Dim schedule cleared');
      await fetchDimSchedule();
    } catch (err) {
      setStatusMsg($('dim-status'), `Error: ${err.message}`, false);
    }
  }
});

// ─── Custom metrics dashboard ─────────────────────────────────────────────────
(function injectMetricsSection() {
  if ($('met-title')) return;
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Custom Metrics Dashboard</h2>
    <p class="card-hint">Push up to 8 key/value metrics to the panel's Metrics display mode.</p>
    <div class="form-group" style="margin-bottom:0.6rem">
      <label for="met-title">Dashboard title</label>
      <input type="text" id="met-title" placeholder="CI Dashboard" maxlength="28">
    </div>
    <div id="met-rows"></div>
    <div class="form-actions" style="margin-top:0.5rem">
      <button id="met-add-btn">+ Add metric</button>
      <button id="met-push-btn" class="primary">📊 Push to panel</button>
      <button id="met-clear-btn" class="danger-btn">🗑 Clear</button>
      <span id="met-status"></span>
    </div>
  `;
  $('dim-set-btn')?.closest('section')?.insertAdjacentElement('afterend', section);
})();

function addMetricRow(label = '', value = '', color = 'blue') {
  const rows = $('met-rows');
  if (!rows) return;
  if (rows.children.length >= 8) return;
  const div = document.createElement('div');
  div.className = 'metric-row';
  div.innerHTML = `
    <input type="text"  class="met-label" placeholder="Label" value="${escHtml(label)}" maxlength="14" style="flex:1">
    <input type="text"  class="met-value" placeholder="Value" value="${escHtml(value)}" maxlength="14" style="flex:1">
    <select class="met-color">
      ${['blue','green','red','yellow','teal','mauve','peach','white'].map(c =>
        `<option value="${c}"${c===color?' selected':''}>${c}</option>`).join('')}
    </select>
    <button class="met-del-btn danger-btn" title="Remove">✕</button>
  `;
  div.querySelector('.met-del-btn').addEventListener('click', () => div.remove());
  rows.appendChild(div);
}

document.addEventListener('click', async e => {
  const id = e.target.id;
  if (id === 'met-add-btn') {
    addMetricRow();
  } else if (id === 'met-push-btn') {
    const rows  = $('met-rows');
    const title = $('met-title')?.value.trim() || '';
    const metrics = [];
    rows?.querySelectorAll('.metric-row').forEach(row => {
      const label = row.querySelector('.met-label')?.value.trim();
      const value = row.querySelector('.met-value')?.value.trim();
      const color = row.querySelector('.met-color')?.value;
      if (label && value) metrics.push({ label, value, color });
    });
    try {
      await api('POST', '/api/metrics', { title, metrics, show: true });
      setStatusMsg($('met-status'), `✓ ${metrics.length} metric(s) pushed`, true);
      log(`Metrics pushed: ${metrics.length} items`);
    } catch (err) {
      setStatusMsg($('met-status'), `Error: ${err.message}`, false);
    }
  } else if (id === 'met-clear-btn') {
    try {
      await api('DELETE', '/api/metrics');
      const rows = $('met-rows');
      if (rows) rows.innerHTML = '';
      setStatusMsg($('met-status'), '✓ Cleared', true);
      log('Metrics cleared');
    } catch (err) {
      setStatusMsg($('met-status'), `Error: ${err.message}`, false);
    }
  }
});

async function fetchMetrics() {
  try {
    const d   = await apiJson('GET', '/api/metrics');
    const met  = d.metrics || [];
    const rows = $('met-rows');
    if (!rows || rows.children.length > 0) return; // don't overwrite user edits
    if ($('met-title')) $('met-title').value = d.title || '';
    met.forEach(m => addMetricRow(m.label, m.value, m.color || 'blue'));
  } catch (_) {}
}


// ─── HTTP JSON Metrics Source section ────────────────────────────────────────
(function injectMetricsSourceSection() {
  if (document.getElementById('ms-url-input')) return;
  var sec = document.createElement('section');
  sec.id = 'metrics-source-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>📡 HTTP JSON Metrics Source</h2>',
    '<p class="card-hint">Automatically fetch metrics from any JSON HTTP endpoint and display them on the panel.',
    ' Use dot-notation JSON paths: <code>cpu.usage_pct</code>, <code>data.0.value</code>.</p>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">',
    '  <div class="form-group" style="flex:3"><label>URL</label>',
    '    <input type="url" id="ms-url-input" placeholder="https://myserver/api/stats" style="width:100%"></div>',
    '  <div class="form-group" style="flex:1"><label>Interval (s)</label>',
    '    <input type="number" id="ms-interval-input" value="30" min="5" max="3600" style="width:100%"></div>',
    '</div>',
    '<div class="form-group" style="margin:0.4rem 0">',
    '  <label>Dashboard title (optional)</label>',
    '  <input type="text" id="ms-title-input" placeholder="My Server" maxlength="28" style="width:100%">',
    '</div>',
    '<div id="ms-fields-list" style="margin-bottom:0.5rem"></div>',
    '<div class="form-row" style="gap:0.5rem;margin-bottom:0.5rem">',
    '  <button id="ms-add-field-btn" class="btn-secondary">+ Add field</button>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap">',
    '  <button id="ms-set-btn"   class="btn-primary"    style="flex:2">Set & Start Auto-fetch</button>',
    '  <button id="ms-fetch-btn" class="btn-secondary"  style="flex:1">Fetch Now</button>',
    '  <button id="ms-clear-btn" class="btn-danger"     style="flex:1">Clear</button>',
    '</div>',
    '<div id="ms-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>',
  ].join('\n');

  // Insert after the custom metrics section
  var metSec = document.getElementById('metrics-source-section');
  var afterSec = document.querySelector('[id="met-push-btn"]')?.closest('section');
  if (afterSec && afterSec.nextSibling) {
    afterSec.parentNode.insertBefore(sec, afterSec.nextSibling);
  } else if (afterSec) {
    afterSec.parentNode.appendChild(sec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  function addFieldRow(label, path, fmt, color) {
    var list = document.getElementById('ms-fields-list');
    if (!list || list.children.length >= 8) return;
    var div = document.createElement('div');
    div.className = 'form-row';
    div.style.gap = '0.4rem';
    div.style.marginBottom = '0.3rem';
    div.innerHTML = [
      '<input type="text" class="ms-field-label" placeholder="Label" value="' + escHtml(label||'') + '" maxlength="14" style="flex:1">',
      '<input type="text" class="ms-field-path"  placeholder="JSON path (e.g. cpu.pct)" value="' + escHtml(path||'') + '" style="flex:2">',
      '<input type="text" class="ms-field-fmt"   placeholder="{}" value="' + escHtml(fmt||'{}') + '" style="flex:1;min-width:50px">',
      '<select class="ms-field-color" style="flex:1">',
      ['blue','green','red','yellow','teal','mauve','peach','white'].map(c =>
        '<option value="' + c + '"' + (c===(color||'blue')?' selected':'') + '>' + c + '</option>'
      ).join(''),
      '</select>',
      '<button class="ms-remove-btn btn-danger" title="Remove">✕</button>',
    ].join('');
    div.querySelector('.ms-remove-btn').addEventListener('click', () => div.remove());
    list.appendChild(div);
  }

  document.getElementById('ms-add-field-btn').addEventListener('click', () => addFieldRow('', '', '{}', 'blue'));

  document.getElementById('ms-set-btn').addEventListener('click', async function() {
    var url      = document.getElementById('ms-url-input').value.trim();
    var title    = document.getElementById('ms-title-input').value.trim();
    var interval = parseInt(document.getElementById('ms-interval-input').value, 10) || 30;
    if (!url) { showToast('Enter a URL first', 'alert', 2000); return; }
    var fields = [];
    document.querySelectorAll('#ms-fields-list .form-row').forEach(function(row) {
      var label = row.querySelector('.ms-field-label')?.value.trim();
      var path  = row.querySelector('.ms-field-path')?.value.trim();
      var fmt   = row.querySelector('.ms-field-fmt')?.value.trim() || '{}';
      var color = row.querySelector('.ms-field-color')?.value || 'blue';
      if (label && path) fields.push({ label, path, format: fmt, color });
    });
    if (!fields.length) { showToast('Add at least one field', 'alert', 2000); return; }
    try {
      await apiJson('POST', '/api/metrics/source', { url, title, interval_secs: interval, fields });
      showToast('Metrics source started: ' + url, 'ok', 3000);
      document.getElementById('ms-status').textContent = 'Auto-fetch active every ' + interval + 's';
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });

  document.getElementById('ms-fetch-btn').addEventListener('click', async function() {
    try {
      var d = await apiJson('POST', '/api/metrics/source/fetch');
      showToast('Fetched ' + (d.count || 0) + ' metric(s)', 'ok', 2500);
      document.getElementById('ms-status').textContent = 'Last fetch: ' + new Date().toLocaleTimeString();
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });

  document.getElementById('ms-clear-btn').addEventListener('click', async function() {
    try {
      await api('DELETE', '/api/metrics/source');
      document.getElementById('ms-status').textContent = 'Cleared.';
      showToast('Metrics source cleared', 'ok', 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });

  // Load existing config on boot
  (async function() {
    try {
      var d = await apiJson('GET', '/api/metrics/source');
      if (!d.source) return;
      var src = d.source;
      var urlInput = document.getElementById('ms-url-input');
      if (urlInput) urlInput.value = src.url || '';
      var titleInput = document.getElementById('ms-title-input');
      if (titleInput) titleInput.value = src.title || '';
      var intInput = document.getElementById('ms-interval-input');
      if (intInput) intInput.value = src.interval_secs || 30;
      (src.fields || []).forEach(function(f) {
        addFieldRow(f.label, f.path, f.format, f.color);
      });
      if (src.url) {
        document.getElementById('ms-status').textContent = 'Auto-fetch active every ' + (src.interval_secs||30) + 's';
      }
    } catch (_) {}
  })();
})();

// ─── Display profiles ─────────────────────────────────────────────────────────
(function injectProfilesSection() {
  if ($('prof-name')) return;
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Display Profiles</h2>
    <p class="card-hint">Save and restore complete panel configurations (mode, quality, rotation, brightness).</p>
    <div class="form-row two-col">
      <div class="form-group">
        <label for="prof-name">Profile name</label>
        <input type="text" id="prof-name" placeholder="e.g. night-mode" maxlength="40">
      </div>
      <div class="form-group">
        <label for="prof-select">Saved profiles</label>
        <select id="prof-select"><option value="">— select profile —</option></select>
      </div>
    </div>
    <div class="form-actions">
      <button id="prof-save-btn" class="primary">💾 Save Current</button>
      <button id="prof-apply-btn">▶ Apply</button>
      <button id="prof-del-btn" class="danger-btn">🗑 Delete</button>
      <span id="prof-status"></span>
    </div>
  `;
  $('met-push-btn')?.closest('section')?.insertAdjacentElement('afterend', section);
})();

async function fetchProfiles() {
  try {
    const profiles = await apiJson('GET', '/api/profiles');
    const sel = $('prof-select');
    if (!sel) return;
    const prev = sel.value;
    sel.innerHTML = '<option value="">— select profile —</option>';
    profiles.forEach(p => {
      const opt = document.createElement('option');
      opt.value       = p.name;
      opt.textContent = `${p.name} (${p.mode}, q${p.quality})`;
      sel.appendChild(opt);
    });
    if (prev) sel.value = prev;
  } catch (_) {}
}

document.addEventListener('click', async e => {
  const id = e.target.id;
  if (id === 'prof-save-btn') {
    const name = $('prof-name')?.value.trim() || prompt('Profile name:');
    if (!name) return;
    try {
      await api('POST', '/api/profiles', { name });
      setStatusMsg($('prof-status'), `✓ Saved "${name}"`, true);
      log(`Profile saved: ${name}`);
      await fetchProfiles();
      const sel = $('prof-select');
      if (sel) sel.value = name;
    } catch (err) {
      setStatusMsg($('prof-status'), `Error: ${err.message}`, false);
    }
  } else if (id === 'prof-apply-btn') {
    const name = $('prof-select')?.value;
    if (!name) { setStatusMsg($('prof-status'), 'Select a profile first', false); return; }
    try {
      await api('POST', `/api/profiles/${encodeURIComponent(name)}/apply`);
      setStatusMsg($('prof-status'), `✓ Applied "${name}"`, true);
      log(`Profile applied: ${name}`);
      // Reset slider sync so they pick up new values
      setQuality.dataset.synced    = '';
      setOrient.dataset.synced     = '';
      setBrightness.dataset.synced = '';
    } catch (err) {
      setStatusMsg($('prof-status'), `Error: ${err.message}`, false);
    }
  } else if (id === 'prof-del-btn') {
    const name = $('prof-select')?.value;
    if (!name) return;
    if (!confirm(`Delete profile "${name}"?`)) return;
    try {
      await api('DELETE', `/api/profiles/${encodeURIComponent(name)}`);
      setStatusMsg($('prof-status'), `✓ Deleted "${name}"`, true);
      log(`Profile deleted: ${name}`);
      await fetchProfiles();
    } catch (err) {
      setStatusMsg($('prof-status'), `Error: ${err.message}`, false);
    }
  }
});

// ─── Notes / bulletin board section ─────────────────────────────────────────
(function injectNotesSection() {
  if ($('notes-title-input')) return;
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Notes / Bulletin Board</h2>
    <p class="card-hint">Push a list of text items to the panel. Useful for sprint boards, checklists, or reminders.</p>
    <div class="form-group" style="margin-bottom:0.6rem">
      <label for="notes-title-input">Board title</label>
      <input type="text" id="notes-title-input" placeholder="Sprint 42, TODO, Meeting Agenda…" maxlength="28">
    </div>
    <div id="notes-items-wrap"></div>
    <div class="form-actions" style="margin-top:0.5rem">
      <button id="notes-add-btn">+ Add note</button>
      <button id="notes-push-btn" class="primary">📝 Push to panel</button>
      <button id="notes-append-btn">➕ Append</button>
      <button id="notes-clear-btn" class="danger-btn">🗑 Clear</button>
      <span id="notes-status"></span>
    </div>
  `;
  // Insert after weather section (or fallback to metrics section)
  const weatherBtn = document.getElementById('weather-fetch-btn');
  weatherBtn?.closest('section')?.insertAdjacentElement('afterend', section);
})();

function addNoteRow(text = '') {
  const wrap = $('notes-items-wrap');
  if (!wrap) return;
  if (wrap.children.length >= 10) return;
  const div = document.createElement('div');
  div.className = 'notes-item-row';
  div.style.cssText = 'display:flex;gap:0.4rem;align-items:center;margin-bottom:0.3rem';
  div.innerHTML = `
    <input type="text" class="note-text" placeholder="Note text…" value="${escHtml(text)}" maxlength="40"
           style="flex:1;padding:0.3rem 0.5rem;font-size:0.85rem">
    <button class="note-del danger-btn" title="Remove">✕</button>
  `;
  div.querySelector('.note-del').addEventListener('click', () => div.remove());
  wrap.appendChild(div);
}

document.addEventListener('click', async e => {
  const id = e.target.id;
  if (id === 'notes-add-btn') {
    addNoteRow();
  } else if (id === 'notes-push-btn' || id === 'notes-append-btn') {
    const title  = $('notes-title-input')?.value.trim() || '';
    const append = id === 'notes-append-btn';
    const items  = Array.from($('notes-items-wrap')?.querySelectorAll('.note-text') || [])
      .map(el => el.value.trim()).filter(Boolean);
    try {
      await api('POST', '/api/notes', { title, items, append, show: true });
      setStatusMsg($('notes-status'), `✓ ${items.length} note(s) pushed`, true);
      log(`Notes: ${items.length} item(s)${append ? ' (appended)' : ''}`);
    } catch (err) {
      setStatusMsg($('notes-status'), `Error: ${err.message}`, false);
    }
  } else if (id === 'notes-clear-btn') {
    try {
      await api('DELETE', '/api/notes');
      const wrap = $('notes-items-wrap');
      if (wrap) wrap.innerHTML = '';
      if ($('notes-title-input')) $('notes-title-input').value = '';
      setStatusMsg($('notes-status'), '✓ Cleared', true);
      log('Notes cleared');
    } catch (err) {
      setStatusMsg($('notes-status'), `Error: ${err.message}`, false);
    }
  }
});

async function fetchCachedNotes() {
  try {
    const d = await apiJson('GET', '/api/notes');
    if (!d.items?.length) return;
    const titleEl = $('notes-title-input');
    if (titleEl && !titleEl.value) titleEl.value = d.title || '';
    const wrap = $('notes-items-wrap');
    if (wrap && !wrap.children.length) {
      d.items.forEach(item => addNoteRow(item));
    }
  } catch (_) {}
}

// ─── Weather section ─────────────────────────────────────────────────────────
(function injectWeatherSection() {
  if ($('weather-city')) return;
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Weather Dashboard</h2>
    <p class="card-hint">Fetches current conditions from <a href="https://wttr.in" target="_blank" rel="noopener">wttr.in</a> (public API, no key needed) and pushes them to the panel.</p>
    <div class="form-row two-col">
      <div class="form-group">
        <label for="weather-city">City / location</label>
        <input type="text" id="weather-city" placeholder="London, Paris, Tokyo…" value="">
      </div>
      <div class="form-group">
        <label>Auto-refresh</label>
        <select id="weather-refresh-interval">
          <option value="0">Manual only</option>
          <option value="300">Every 5 min</option>
          <option value="600" selected>Every 10 min</option>
          <option value="1800">Every 30 min</option>
          <option value="3600">Every 1 hour</option>
        </select>
      </div>
    </div>
    <div class="form-actions" style="margin-top:0.6rem;flex-wrap:wrap;gap:0.5rem">
      <button id="weather-fetch-btn" class="primary">🌤 Fetch &amp; Push Weather</button>
      <button id="weather-server-auto-btn" title="Enable server-side auto-fetch (persists when browser is closed)">🔄 Server Auto-fetch</button>
      <button id="weather-auto-stop-btn" title="Stop server-side auto-fetch">⏹ Stop Server Auto</button>
      <button id="weather-clear-btn">✕ Clear</button>
      <span id="weather-status"></span>
    </div>
    <div id="weather-display" class="weather-display" style="display:none;margin-top:0.75rem">
      <div class="weather-temp" id="weather-temp">—</div>
      <div class="weather-cond" id="weather-cond">—</div>
      <div class="weather-details" id="weather-details"></div>
    </div>
  `;
  $('prof-save-btn')?.closest('section')?.insertAdjacentElement('afterend', section);
})();

let weatherRefreshTimer = null;

async function fetchAndPushWeather(silent = false) {
  const city = ($('weather-city')?.value.trim()) || 'London';
  const statusEl = $('weather-status');
  if (!silent) setStatusMsg(statusEl, '⏳ Fetching weather…', true, 60000);

  try {
    // wttr.in returns JSON with current_condition, weather (forecast)
    const url = `https://wttr.in/${encodeURIComponent(city)}?format=j1`;
    const r   = await fetch(url);
    if (!r.ok) throw new Error(`wttr.in returned ${r.status}`);
    const data = await r.json();

    const cur  = data.current_condition?.[0];
    const cast = data.weather?.[0];
    if (!cur)  throw new Error('No current_condition in response');

    const payload = {
      city:          city,
      temp_c:        parseFloat(cur.temp_C),
      feels_like_c:  parseFloat(cur.FeelsLikeC),
      condition:     cur.weatherDesc?.[0]?.value || '',
      humidity_pct:  parseInt(cur.humidity, 10),
      wind_kmh:      parseFloat(cur.windspeedKmph),
      wind_dir:      cur.winddir16Point || '',
      visibility_km: parseFloat(cur.visibility),
      high_c:        cast ? parseFloat(cast.maxtempC) : undefined,
      low_c:         cast ? parseFloat(cast.mintempC) : undefined,
      show:          true,
    };

    await api('POST', '/api/weather', payload);

    // Update local display
    const tempEl    = $('weather-temp');
    const condEl    = $('weather-cond');
    const detailsEl = $('weather-details');
    const dispEl    = $('weather-display');
    if (tempEl)    tempEl.textContent = `${payload.temp_c?.toFixed(0)}°C`;
    if (condEl)    condEl.textContent = payload.condition;
    if (detailsEl) detailsEl.innerHTML = [
      `💧 ${payload.humidity_pct}%`,
      `💨 ${payload.wind_kmh?.toFixed(0)} km/h ${payload.wind_dir}`,
      `▲ ${payload.high_c?.toFixed(0)}° ▼ ${payload.low_c?.toFixed(0)}°`,
    ].map(s => `<span>${s}</span>`).join('');
    if (dispEl) dispEl.style.display = '';

    if (!silent) {
      setStatusMsg(statusEl, `✓ Weather pushed for ${city}`, true);
      log(`Weather: ${city} ${payload.temp_c?.toFixed(0)}°C ${payload.condition}`);
    }
  } catch (e) {
    if (!silent) setStatusMsg(statusEl, `Error: ${e.message}`, false);
  }
}

document.addEventListener('click', async e => {
  const id = e.target.id;
  if (id === 'weather-fetch-btn') {
    await fetchAndPushWeather(false);
    // Set up auto-refresh if requested
    const interval = parseInt($('weather-refresh-interval')?.value || '0', 10);
    if (weatherRefreshTimer) { clearInterval(weatherRefreshTimer); weatherRefreshTimer = null; }
    if (interval > 0) {
      weatherRefreshTimer = setInterval(() => fetchAndPushWeather(true), interval * 1000);
      log(`Weather auto-refresh every ${interval}s`);
    }
  } else if (id === 'weather-clear-btn') {
    try {
      await api('DELETE', '/api/weather');
      if (weatherRefreshTimer) { clearInterval(weatherRefreshTimer); weatherRefreshTimer = null; }
      const dispEl = $('weather-display');
      if (dispEl) dispEl.style.display = 'none';
      setStatusMsg($('weather-status'), '✓ Weather cleared', true);
      log('Weather cleared');
    } catch (err) {
      setStatusMsg($('weather-status'), `Error: ${err.message}`, false);
    }
  } else if (id === 'weather-server-auto-btn') {
    const city     = $('weather-city')?.value.trim() || '';
    const interval = parseInt($('weather-refresh-interval')?.value || '1800', 10);
    if (!city) { setStatusMsg($('weather-status'), 'Enter a city first', false); return; }
    const secs = Math.max(interval, 60) || 1800;
    try {
      await apiJson('PUT', '/api/weather/auto', { location: city, interval_secs: secs });
      // Also trigger an immediate server-side fetch
      await apiJson('POST', '/api/weather/fetch', { location: city, switch_mode: true });
      setStatusMsg($('weather-status'), `✓ Server auto-fetch enabled for ${city} every ${secs}s`, true);
      fetchStatus();
    } catch (err) {
      setStatusMsg($('weather-status'), `Error: ${err.message}`, false);
    }
  } else if (id === 'weather-auto-stop-btn') {
    try {
      await api('DELETE', '/api/weather/auto');
      setStatusMsg($('weather-status'), '✓ Server auto-fetch stopped', true);
    } catch (err) {
      setStatusMsg($('weather-status'), `Error: ${err.message}`, false);
    }
  }
});

// Load cached weather on boot
async function fetchCachedWeather() {
  try {
    const d = await apiJson('GET', '/api/weather');
    if (!d.city && !d.temp_c) return;
    const cityEl = $('weather-city');
    if (cityEl && !cityEl.value) cityEl.value = d.city || '';
    const tempEl = $('weather-temp'), condEl = $('weather-cond'), dispEl = $('weather-display');
    if (tempEl) tempEl.textContent = d.temp_c != null ? `${d.temp_c.toFixed(0)}°C` : '—';
    if (condEl) condEl.textContent = d.condition || '—';
    if (dispEl) dispEl.style.display = '';
  } catch (_) {}
}



// ─── QR Code section ─────────────────────────────────────────────────────────
(function injectQrSection() {
  if (document.getElementById('qr-payload')) return;
  var sec = document.createElement('section');
  sec.id = 'qr-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>QR Code</h2>',
    '<p class="card-hint">Generate a scannable QR code on the panel — great for sharing URLs, Wi-Fi passwords, or any short text.</p>',
    '<div class="form-row two-col">',
      '<div class="form-group"><label for="qr-payload">Payload (URL or text)</label>',
        '<input type="text" id="qr-payload" placeholder="https://example.com" maxlength="300"></div>',
      '<div class="form-group"><label for="qr-title">Title <span class="hint-inline">(opt.)</span></label>',
        '<input type="text" id="qr-title" placeholder="QR Code" maxlength="32"></div>',
    '</div>',
    '<div class="form-actions">',
      '<button id="qr-show-btn" class="primary">🔲 Show QR Code</button>',
      '<button id="qr-clear-btn">✕ Clear</button>',
      '<span id="qr-status"></span>',
    '</div>',
    '<div id="qr-preview-wrap" style="display:none;margin-top:0.75rem">',
      '<div class="sub-label">Last payload</div>',
      '<div id="qr-payload-display" class="mono" style="font-size:0.82rem;color:var(--teal);word-break:break-all;margin-top:0.3rem"></div>',
    '</div>',
  ].join('');

  // Insert after the notes section (or before history)
  var notesSection = document.querySelector('section.card h2');
  // Find notes card by searching all h2s
  var inserted = false;
  document.querySelectorAll('section.card h2').forEach(function(h2) {
    if (h2.textContent.trim() === 'Notes / Bulletin Board' && !inserted) {
      h2.closest('section').insertAdjacentElement('afterend', sec);
      inserted = true;
    }
  });
  if (!inserted) {
    var histCard = historyList && historyList.closest('.card');
    if (histCard) histCard.insertAdjacentElement('beforebegin', sec);
  }

  document.getElementById('qr-show-btn').addEventListener('click', async function() {
    var payload = (document.getElementById('qr-payload').value || '').trim();
    var title   = (document.getElementById('qr-title').value || '').trim();
    var statusEl = document.getElementById('qr-status');
    if (!payload) { setStatusMsg(statusEl, 'Enter a payload first', false); return; }
    try {
      await api('POST', '/api/qr', { payload: payload, title: title, show: true });
      setStatusMsg(statusEl, '✓ QR code on panel', true);
      var wrap = document.getElementById('qr-preview-wrap');
      var disp = document.getElementById('qr-payload-display');
      if (wrap) wrap.style.display = '';
      if (disp) disp.textContent = payload;
      log('QR code: ' + payload.substring(0, 60));
    } catch (e) { setStatusMsg(statusEl, 'Error: ' + e.message, false); }
  });

  document.getElementById('qr-clear-btn').addEventListener('click', async function() {
    var statusEl = document.getElementById('qr-status');
    try {
      await api('DELETE', '/api/qr');
      setStatusMsg(statusEl, '✓ Cleared', true);
      var wrap = document.getElementById('qr-preview-wrap');
      if (wrap) wrap.style.display = 'none';
      document.getElementById('qr-payload').value = '';
    } catch (e) { setStatusMsg(statusEl, 'Error: ' + e.message, false); }
  });
})();

async function fetchQrState() {
  try {
    var d = await apiJson('GET', '/api/qr');
    if (d.payload) {
      var inp = document.getElementById('qr-payload');
      if (inp && !inp.value) inp.value = d.payload;
      var titleInp = document.getElementById('qr-title');
      if (titleInp && !titleInp.value && d.title !== 'QR Code') titleInp.value = d.title;
      var wrap = document.getElementById('qr-preview-wrap');
      var disp = document.getElementById('qr-payload-display');
      if (wrap && d.payload) { wrap.style.display = ''; }
      if (disp && d.payload) { disp.textContent = d.payload; }
    }
  } catch (_) {}
}

// ─── History filter bar (────────────────────────────────────────────────
(function injectHistoryFilters() {
  if (document.getElementById('hist-filter-bar')) return;
  var par = historyList && historyList.parentElement;
  if (!par) return;
  var bar = document.createElement('div');
  bar.id = 'hist-filter-bar';
  bar.className = 'hist-filter-bar';
  bar.innerHTML = '<span class="hist-filter-label">Filter:</span>'
    + ['','text','alert','image','system'].map(function(k) {
        return '<button class="hist-filter-btn small-btn" data-kind="' + k + '">'+(k||'all')+'</button>';
      }).join('');
  par.insertBefore(bar, historyList);
  bar.addEventListener('click', function(e) {
    var btn = e.target.closest('.hist-filter-btn');
    if (btn) fetchHistory(btn.dataset.kind);
  });
})();

// ─── Webhook test UI ───────────────────────────────────────────────────────────────
(function injectWebhookSection() {
  if (document.getElementById('wh-section')) return;
  var sec = document.createElement('section');
  sec.id = 'wh-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>Webhook</h2>',
    '<p class="card-hint">Test the webhook endpoint or configure a forward URL for outbound delivery.</p>',
    '<div class="form-row two-col">',
      '<div class="form-group"><label for="wh-status">Status</label>',
        '<select id="wh-status">',
          '<option value="success">success</option>',
          '<option value="failure">failure</option>',
          '<option value="warning">warning</option>',
          '<option value="running">running</option>',
          '<option value="info">info</option>',
        '</select></div>',
      '<div class="form-group"><label for="wh-message">Message</label>',
        '<input type="text" id="wh-message" placeholder="Test webhook message" maxlength="80"></div>',
    '</div>',
    '<div class="form-actions">',
      '<button id="wh-test-btn" class="primary">⚡ Send Test Webhook</button>',
      '<span id="wh-test-status"></span>',
    '</div>',
    '<hr style="border-color:var(--surface0);margin:0.75rem 0">',
    '<p class="card-hint">Forward URL — if set, all incoming webhooks are also forwarded here.</p>',
    '<div class="form-row two-col">',
      '<div class="form-group"><label for="wh-forward-url">Forward URL</label>',
        '<input type="text" id="wh-forward-url" placeholder="https://hooks.example.com/..." maxlength="200"></div>',
    '</div>',
    '<div class="form-actions">',
      '<button id="wh-url-save-btn" class="primary">💾 Save URL</button>',
      '<button id="wh-url-clear-btn" class="danger-btn">✕ Clear</button>',
      '<span id="wh-url-status"></span>',
    '</div>',
    '<div id="wh-url-display" style="display:none;margin-top:0.4rem;font-size:0.82rem;color:var(--overlay0)">',
      'Forwarding to: <span id="wh-url-current" class="mono" style="color:var(--teal)"></span>',
    '</div>',
    '<p class="card-hint" style="margin-top:0.75rem">Endpoint: <span class="mono" id="wh-endpoint-url" style="color:var(--blue)"></span></p>',
  ].join('');

  var histCard = historyList && historyList.closest('.card');
  if (histCard) histCard.insertAdjacentElement('beforebegin', sec);

  var epEl = document.getElementById('wh-endpoint-url');
  if (epEl) epEl.textContent = window.location.origin + '/api/webhook';

  document.getElementById('wh-test-btn').addEventListener('click', async function() {
    var status  = document.getElementById('wh-status').value;
    var message = document.getElementById('wh-message').value || 'Test from ttlcd web UI';
    var statusEl = document.getElementById('wh-test-status');
    try {
      await api('POST', '/api/webhook', {status: status, message: message});
      setStatusMsg(statusEl, '✓ Webhook sent', true);
      log('Webhook test: ' + status + ' — ' + message);
    } catch (e) { setStatusMsg(statusEl, 'Error: ' + e.message, false); }
  });

  document.getElementById('wh-url-save-btn').addEventListener('click', async function() {
    var url = (document.getElementById('wh-forward-url').value || '').trim();
    var statusEl = document.getElementById('wh-url-status');
    if (!url) { setStatusMsg(statusEl, 'Enter a URL first', false); return; }
    try {
      await api('POST', '/api/webhook/config', {url: url});
      setStatusMsg(statusEl, '✓ URL saved', true);
      updateWebhookUrlDisplay(url);
    } catch (e) { setStatusMsg(statusEl, 'Error: ' + e.message, false); }
  });

  document.getElementById('wh-url-clear-btn').addEventListener('click', async function() {
    var statusEl = document.getElementById('wh-url-status');
    try {
      await api('DELETE', '/api/webhook/config');
      setStatusMsg(statusEl, '✓ Cleared', true);
      updateWebhookUrlDisplay(null);
      document.getElementById('wh-forward-url').value = '';
    } catch (e) { setStatusMsg(statusEl, 'Error: ' + e.message, false); }
  });
})();

function updateWebhookUrlDisplay(url) {
  var disp = document.getElementById('wh-url-display');
  var span = document.getElementById('wh-url-current');
  if (!disp || !span) return;
  if (url) { disp.style.display = ''; span.textContent = url; }
  else      { disp.style.display = 'none'; }
}

async function fetchWebhookConfig() {
  try {
    var d = await apiJson('GET', '/api/webhook/config');
    if (d.url) {
      var inp = document.getElementById('wh-forward-url');
      if (inp && !inp.value) inp.value = d.url;
      updateWebhookUrlDisplay(d.url);
    }
  } catch (_) {}
}

// ─── FPS sparkline in panel info ────────────────────────────────────────────────────────────────
(function injectFpsSparkline() {
  if (document.getElementById('fps-sparkline-row')) return;
  var tbl = document.getElementById('info-table');
  if (!tbl) return;
  var fpsRow = null;
  tbl.querySelectorAll('tr').forEach(function(r) {
    if (r.querySelector('#info-fps')) fpsRow = r;
  });
  if (!fpsRow) return;
  var row = document.createElement('tr');
  row.id = 'fps-sparkline-row';
  row.innerHTML = '<td class="info-label">FPS graph</td><td>'
    + '<canvas id="fps-sparkline-canvas" width="200" height="28" style="vertical-align:middle;border-radius:3px"></canvas>'
    + '</td>';
  fpsRow.insertAdjacentElement('afterend', row);
})();

async function fetchFpsHistory() {
  try {
    var d = await apiJson('GET', '/api/fps/history');
    if (!d.samples || !d.samples.length) return;
    drawFpsSparkline(document.getElementById('fps-sparkline-canvas'), d.samples);
  } catch (_) {}
}

function drawFpsSparkline(canvas, data) {
  if (!canvas || !data.length) return;
  var ctx = canvas.getContext('2d');
  var W = canvas.width, H = canvas.height;
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = 'rgba(49,50,68,0.6)';
  ctx.fillRect(0, 0, W, H);
  var max = 0;
  data.forEach(function(v) { if (v > max) max = v; });
  if (max < 1) max = 1;
  var bw = W / data.length;
  data.forEach(function(v, i) {
    var h = Math.round(H * (v / max));
    ctx.fillStyle = v < 0.5 ? '#f38ba8' : v < 1.5 ? '#f9e2af' : '#a6e3a1';
    ctx.fillRect(i * bw, H - h, Math.max(bw - 1, 1), h);
  });
}

// ─── Net sparkline ─────────────────────────────────────────────────────────────────────────
(function injectNetSparkline() {
  if (document.getElementById('net-sparkline-wrap')) return;
  var sparkWrap = document.getElementById('sparkline-wrap');
  if (!sparkWrap) return;
  var div = document.createElement('div');
  div.id = 'net-sparkline-wrap';
  div.style.cssText = 'margin-top:0.5rem;display:none';
  div.innerHTML = '<div class="sub-label">Net history <span class="hint-inline">(→3 Rx | ↑ Tx, last 60 s)</span></div>'
    + '<div style="display:flex;gap:4px">'
    + '<div style="flex:1"><div class="sub-label" style="font-size:0.7rem">↓ Rx</div>'
    +   '<canvas id="net-rx-sparkline" class="cpu-sparkline" width="210" height="45"></canvas></div>'
    + '<div style="flex:1"><div class="sub-label" style="font-size:0.7rem">↑ Tx</div>'
    +   '<canvas id="net-tx-sparkline" class="cpu-sparkline" width="210" height="45"></canvas></div>'
    + '</div>';
  sparkWrap.insertAdjacentElement('afterend', div);
})();

async function updateNetSparkline() {
  try {
    var d = await apiJson('GET', '/api/net/history');
    if (!d.samples || !d.samples.length) return;
    var wrap = document.getElementById('net-sparkline-wrap');
    if (wrap) wrap.style.display = '';
    drawLineSparkline(
      document.getElementById('net-rx-sparkline'),
      d.samples.map(function(s) { return s.rx_bps || 0; }), '#89b4fa');
    drawLineSparkline(
      document.getElementById('net-tx-sparkline'),
      d.samples.map(function(s) { return s.tx_bps || 0; }), '#a6e3a1');
  } catch (_) {}
}

function drawLineSparkline(canvas, data, color) {
  if (!canvas || !data.length) return;
  var ctx = canvas.getContext('2d');
  var W = canvas.width, H = canvas.height;
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = 'rgba(49,50,68,0.4)';
  ctx.fillRect(0, 0, W, H);
  var max = 0;
  data.forEach(function(v) { if (v > max) max = v; });
  if (max < 1) max = 1;
  ctx.beginPath();
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  var first = true;
  data.forEach(function(v, i) {
    var x = (data.length > 1) ? (i / (data.length - 1)) * W : W / 2;
    var y = H - (v / max) * (H - 4) - 2;
    if (first) { ctx.moveTo(x, y); first = false; } else ctx.lineTo(x, y);
  });
  ctx.stroke();
}


// ─── Mode schedule section ───────────────────────────────────────────────────
(function injectModeScheduleSection() {
  if (document.getElementById('sched-hour')) return;
  var sec = document.createElement('section');
  sec.id = 'mode-schedule-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>Mode Schedule</h2>',
    '<p class="card-hint">Automatically switch display modes at specific times of day. The daemon applies the most recently past slot on startup and every 30 seconds.</p>',
    '<div class="form-row three-col">',
      '<div class="form-group"><label for="sched-hour">Hour <span class="hint-inline">(0–23)</span></label>',
        '<input type="number" id="sched-hour" min="0" max="23" value="9"></div>',
      '<div class="form-group"><label for="sched-minute">Minute <span class="hint-inline">(0–59)</span></label>',
        '<input type="number" id="sched-minute" min="0" max="59" value="0"></div>',
      '<div class="form-group"><label for="sched-mode">Mode</label>',
        '<select id="sched-mode"></select></div>',
    '</div>',
    '<div class="form-row two-col">',
      '<div class="form-group"><label for="sched-label">Label <span class="hint-inline">(opt.)</span></label>',
        '<input type="text" id="sched-label" placeholder="Work hours" maxlength="40"></div>',
    '</div>',
    '<div class="form-actions">',
      '<button id="sched-add-btn" class="primary">+ Add Slot</button>',
      '<button id="sched-clear-btn" class="danger-btn">✕ Clear All</button>',
      '<span id="sched-status"></span>',
    '</div>',
    '<ul id="sched-list" class="sched-list" style="margin-top:0.75rem;list-style:none;padding:0"></ul>',
  ].join('');

  // Insert after dim schedule section
  var dimSection = document.getElementById('dim-start')?.closest('section');
  if (dimSection) dimSection.insertAdjacentElement('afterend', sec);
  else document.body.appendChild(sec);

  // Populate mode select from /api/modes (all available modes)
  (async function() {
    var modeSelect = document.getElementById('sched-mode');
    if (!modeSelect) return;
    try {
      var data = await apiJson('GET', '/api/modes');
      var modes = Array.isArray(data) ? data.map(m => m.name || m) : [];
      if (modes.length === 0) modes = ['clock','sysinfo','cpu_detail','network','idle'];
      modes.forEach(function(m) {
        var opt = document.createElement('option');
        opt.value = m;
        opt.textContent = m.replace(/_/g, ' ');
        modeSelect.appendChild(opt);
      });
    } catch (_) {
      // Fallback minimal list
      ['clock','sysinfo','cpu_detail','network','idle'].forEach(function(m) {
        var opt = document.createElement('option');
        opt.value = m;
        opt.textContent = m.replace(/_/g, ' ');
        modeSelect.appendChild(opt);
      });
    }
  })();

  document.getElementById('sched-add-btn').addEventListener('click', async function() {
    var hour   = parseInt(document.getElementById('sched-hour')?.value || 0, 10);
    var minute = parseInt(document.getElementById('sched-minute')?.value || 0, 10);
    var mode   = document.getElementById('sched-mode')?.value;
    var label  = (document.getElementById('sched-label')?.value || '').trim();
    var statusEl = document.getElementById('sched-status');
    if (!mode) { setStatusMsg(statusEl, 'Select a mode', false); return; }
    try {
      await api('POST', '/api/schedule/mode', { hour, minute, mode, label });
      setStatusMsg(statusEl, '✓ Slot added', true);
      log('Schedule: ' + String(hour).padStart(2,'0') + ':' + String(minute).padStart(2,'0') + ' → ' + mode);
      await fetchModeSchedule();
    } catch (e) { setStatusMsg(statusEl, 'Error: ' + e.message, false); }
  });

  document.getElementById('sched-clear-btn').addEventListener('click', async function() {
    var statusEl = document.getElementById('sched-status');
    try {
      await api('DELETE', '/api/schedule/mode');
      setStatusMsg(statusEl, '✓ Cleared', true);
      await fetchModeSchedule();
    } catch (e) { setStatusMsg(statusEl, 'Error: ' + e.message, false); }
  });

  document.getElementById('sched-list').addEventListener('click', async function(e) {
    var btn = e.target.closest('[data-sched-remove]');
    if (!btn) return;
    var idx = parseInt(btn.dataset.schedRemove, 10);
    try {
      await fetch('/api/schedule/mode/' + idx, { method: 'DELETE' });
      await fetchModeSchedule();
    } catch (_) {}
  });
})();

async function fetchModeSchedule() {
  try {
    var entries = await apiJson('GET', '/api/schedule/mode');
    var list = document.getElementById('sched-list');
    if (!list) return;
    list.innerHTML = '';
    if (!entries.length) {
      var li = document.createElement('li');
      li.textContent = 'No schedule entries yet.';
      li.style.color = 'var(--overlay0)';
      li.style.fontSize = '0.85rem';
      list.appendChild(li);
      return;
    }
    entries.forEach(function(entry, idx) {
      var li = document.createElement('li');
      li.className = 'sched-entry';
      var h = String(entry.hour).padStart(2,'0');
      var m = String(entry.minute).padStart(2,'0');
      li.innerHTML = '<span class="sched-time mono">' + h + ':' + m + '</span>'
        + '<span class="sched-mode">' + entry.mode.replace(/_/g,' ') + '</span>'
        + (entry.label ? '<span class="sched-label">' + escHtml(entry.label) + '</span>' : '')
        + '<button class="sched-remove small-btn" data-sched-remove="' + idx + '" title="Remove">✕</button>';
      list.appendChild(li);
    });
  } catch (_) {}
}


// ─── Pomodoro section ────────────────────────────────────────────────────────
(function injectPomodoroSection() {
  if (document.getElementById('pomo-start-btn')) return;
  var sec = document.createElement('section');
  sec.id = 'pomodoro-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>🍅 Pomodoro Timer</h2>',
    '<p class="card-hint">Focus timer: 25 min work → 5 min break → repeat. After 4 sessions, take a 15 min long break.</p>',
    '<div id="pomo-status-row" style="display:flex;align-items:center;gap:1rem;flex-wrap:wrap;margin-bottom:0.75rem">',
    '  <div id="pomo-phase-badge" class="badge" style="font-size:1rem;padding:0.3rem 0.7rem">—</div>',
    '  <div id="pomo-time" style="font-family:var(--mono-font);font-size:2rem;color:var(--teal);min-width:5rem">--:--</div>',
    '  <div id="pomo-sessions" style="font-size:0.85rem;color:var(--overlay0)"></div>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap">',
    '  <button id="pomo-start-btn"  class="btn-primary" style="min-width:6rem">▶ Start</button>',
    '  <button id="pomo-stop-btn"   class="btn-secondary" style="min-width:6rem" title="Stop and return to previous mode">⏹ Stop</button>',
    '  <button id="pomo-pause-btn"  class="btn-secondary" style="min-width:6rem">⏸ Pause</button>',
    '  <button id="pomo-skip-btn"   class="btn-secondary" style="min-width:6rem" title="Skip to next phase">⏭ Skip</button>',
    '  <button id="pomo-reset-btn"  class="btn-danger"    style="min-width:6rem" title="Reset all sessions">↺ Reset</button>',
    '</div>',
    '<details style="margin-top:0.75rem">',
    '  <summary style="cursor:pointer;color:var(--subtext0);font-size:0.85rem">Configure durations</summary>',
    '  <div class="form-row two-col" style="margin-top:0.5rem">',
    '    <div class="form-group"><label>Work (min)</label><input type="number" id="pomo-work-mins"  value="25" min="1" max="120" style="width:80px"></div>',
    '    <div class="form-group"><label>Short break (min)</label><input type="number" id="pomo-short-mins" value="5"  min="1" max="60"  style="width:80px"></div>',
    '    <div class="form-group"><label>Long break (min)</label><input type="number"  id="pomo-long-mins"  value="15" min="1" max="120" style="width:80px"></div>',
    '    <div class="form-group"><label>Sessions / set</label><input type="number"    id="pomo-interval"   value="4"  min="1" max="12"  style="width:80px"></div>',
    '  </div>',
    '  <button id="pomo-config-btn" class="btn-secondary" style="margin-top:0.4rem">Apply Config</button>',
    '</details>'
  ].join('\n');

  // Insert before the mode-schedule section
  var schedSec = document.getElementById('mode-schedule-section');
  if (schedSec) {
    schedSec.parentNode.insertBefore(sec, schedSec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  // Button handlers
  document.getElementById('pomo-stop-btn').addEventListener('click', async function() {
    await apiJson('POST', '/api/pomodoro/stop');
    fetchPomodoroState();
    fetchStatus();
  });
  document.getElementById('pomo-start-btn').addEventListener('click', async function() {
    await apiJson('POST', '/api/pomodoro/start');
    fetchPomodoroState();
  fetchWorldClock();
  });
  document.getElementById('pomo-pause-btn').addEventListener('click', async function() {
    await apiJson('POST', '/api/pomodoro/pause');
    fetchPomodoroState();
  fetchWorldClock();
  });
  document.getElementById('pomo-skip-btn').addEventListener('click', async function() {
    await apiJson('POST', '/api/pomodoro/skip');
    fetchPomodoroState();
  fetchWorldClock();
  });
  document.getElementById('pomo-reset-btn').addEventListener('click', async function() {
    if (!confirm('Reset all Pomodoro sessions?')) return;
    await apiJson('POST', '/api/pomodoro/reset');
    fetchPomodoroState();
  fetchWorldClock();
  });
  document.getElementById('pomo-config-btn').addEventListener('click', async function() {
    var body = {
      work_mins:           parseInt(document.getElementById('pomo-work-mins').value)  || 25,
      short_break_mins:    parseInt(document.getElementById('pomo-short-mins').value) || 5,
      long_break_mins:     parseInt(document.getElementById('pomo-long-mins').value)  || 15,
      long_break_interval: parseInt(document.getElementById('pomo-interval').value)   || 4,
    };
    await apiJson('POST', '/api/pomodoro/config', body);
    fetchPomodoroState();
  fetchWorldClock();
  });
})();

async function fetchPomodoroState() {
  try {
    var d = await apiJson('GET', '/api/pomodoro');
    var badge = document.getElementById('pomo-phase-badge');
    var timeEl = document.getElementById('pomo-time');
    var sessEl = document.getElementById('pomo-sessions');
    if (!badge) return;

    var phaseLabel = { work: '🍅 Focus', short_break: '☕ Short Break', long_break: '🌿 Long Break' }[d.phase] || d.phase;
    badge.textContent = phaseLabel;
    badge.style.background = d.phase === 'work' ? 'var(--red)' : d.phase === 'short_break' ? 'var(--green)' : 'var(--teal)';
    badge.style.color = 'var(--base)';

    var rem = Math.max(0, d.remaining_secs);
    timeEl.textContent = String(Math.floor(rem / 60)).padStart(2,'0') + ':' + String(rem % 60).padStart(2,'0');
    timeEl.style.color = d.running ? 'var(--teal)' : 'var(--overlay0)';

    var dots = '';
    for (var i = 0; i < d.long_break_interval; i++) {
      var filled = i < d.sessions_in_set;
      var active = d.phase === 'work' && i === d.sessions_in_set;
      dots += '<span style="display:inline-block;width:12px;height:12px;border-radius:50%;margin:0 3px;background:' +
        (filled ? 'var(--red)' : active ? 'var(--overlay0)' : 'var(--surface0)') + '"></span>';
    }
    sessEl.innerHTML = dots + ' &nbsp;' + d.completed_sessions + ' completed';

    // Update config inputs if not focused
    var fields = ['pomo-work-mins','pomo-short-mins','pomo-long-mins','pomo-interval'];
    var vals = [d.work_secs/60, d.short_break_secs/60, d.long_break_secs/60, d.long_break_interval];
    fields.forEach(function(id, i) {
      var el = document.getElementById(id);
      if (el && document.activeElement !== el) el.value = vals[i];
    });
  } catch (_) {}
}



// ─── Ticker section ───────────────────────────────────────────────────────────
(function injectTickerSection() {
  if (document.getElementById('ticker-send-btn')) return;
  var sec = document.createElement('section');
  sec.id = 'ticker-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>📡 Scrolling Ticker</h2>',
    '<p class="card-hint">Display a scrolling text message on the panel. Useful for long announcements or notifications.</p>',
    '<div class="form-row two-col" style="align-items:flex-end">',
    '  <div class="form-group" style="flex:3"><label>Ticker Text</label>',
    '    <input type="text" id="ticker-text" placeholder="Your scrolling message here…" style="width:100%"></div>',
    '  <div class="form-group" style="flex:1"><label>Color</label>',
    '    <select id="ticker-color"><option value="blue">Blue</option><option value="green">Green</option>',
    '      <option value="yellow">Yellow</option><option value="red">Red</option>',
    '      <option value="teal">Teal</option><option value="mauve">Mauve</option>',
    '      <option value="peach">Peach</option><option value="white">White</option></select></div>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;margin-top:0.5rem">',
    '  <button id="ticker-send-btn" class="btn-primary">Send to Panel</button>',
    '  <button id="ticker-stop-btn" class="btn-secondary">Stop Ticker</button>',
    '</div>'
  ].join('\n');

  // Insert before world clock section
  var wcSec = document.getElementById('world-clock-section');
  if (wcSec) {
    wcSec.parentNode.insertBefore(sec, wcSec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  document.getElementById('ticker-send-btn').addEventListener('click', async function() {
    var text  = document.getElementById('ticker-text').value.trim();
    var color = document.getElementById('ticker-color').value;
    if (!text) return;
    await apiJson('POST', '/api/ticker', { text: text, color: color, switch_mode: true });
    fetchStatus();
    showToast('Ticker set: ' + text.substring(0, 40) + (text.length > 40 ? '…' : ''), 'ok', 2500);
  });

  document.getElementById('ticker-stop-btn').addEventListener('click', async function() {
    await apiJson('DELETE', '/api/ticker');
    fetchStatus();
  });
})();

// ─── World clock section ──────────────────────────────────────────────────────
(function injectWorldClockSection() {
  if (document.getElementById('wc-add-btn')) return;
  var sec = document.createElement('section');
  sec.id = 'world-clock-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>🌍 World Clock</h2>',
    '<p class="card-hint">Show up to 4 cities with configurable UTC offsets. Leave empty for defaults (UTC, New York, London, Tokyo).</p>',
    '<ul id="wc-list" class="sched-list" style="margin-bottom:0.75rem"></ul>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">',
    '  <div class="form-group" style="flex:2"><label>City name</label>',
    '    <input type="text" id="wc-city" placeholder="e.g. Berlin" style="width:100%"></div>',
    '  <div class="form-group" style="flex:1"><label>UTC offset (min)</label>',
    '    <input type="number" id="wc-offset" value="0" min="-840" max="840" style="width:100%"></div>',
    '  <button id="wc-add-btn" class="btn-primary">Add City</button>',
    '  <button id="wc-clear-btn" class="btn-danger" title="Reset to defaults">Reset</button>',
    '</div>'
  ].join('\n');

  // Insert after pomodoro section
  var pomoSec = document.getElementById('pomodoro-section');
  if (pomoSec && pomoSec.nextSibling) {
    pomoSec.parentNode.insertBefore(sec, pomoSec.nextSibling);
  } else if (pomoSec) {
    pomoSec.parentNode.appendChild(sec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  document.getElementById('wc-add-btn').addEventListener('click', async function() {
    var city   = document.getElementById('wc-city').value.trim();
    var offset = parseInt(document.getElementById('wc-offset').value) || 0;
    if (!city) return;
    var existing = await apiJson('GET', '/api/world_clock');
    var cities = (existing.cities || []).slice(0, 3);
    cities.push({ city: city, utc_offset_mins: offset });
    await apiJson('POST', '/api/world_clock', cities);
    document.getElementById('wc-city').value = '';
    fetchWorldClock();
  });

  document.getElementById('wc-clear-btn').addEventListener('click', async function() {
    await apiJson('DELETE', '/api/world_clock');
    fetchWorldClock();
  });
})();

async function fetchWorldClock() {
  try {
    var d = await apiJson('GET', '/api/world_clock');
    var list = document.getElementById('wc-list');
    if (!list) return;
    list.innerHTML = '';
    if (!d.cities || !d.cities.length) {
      var li = document.createElement('li');
      li.textContent = 'Using defaults: UTC, New York, London, Tokyo';
      li.style.color = 'var(--overlay0)';
      li.style.fontSize = '0.85rem';
      list.appendChild(li);
      return;
    }
    d.cities.forEach(function(c, idx) {
      var li = document.createElement('li');
      li.className = 'sched-entry';
      var h = Math.floor(Math.abs(c.utc_offset_mins) / 60);
      var m = Math.abs(c.utc_offset_mins) % 60;
      var sign = c.utc_offset_mins >= 0 ? '+' : '-';
      var offsetStr = 'UTC' + sign + h + (m ? ':' + String(m).padStart(2,'0') : '');
      li.innerHTML = '<span class="sched-mode" style="color:var(--mauve)">' + escHtml(c.city) + '</span>'
        + '<span class="sched-time">' + offsetStr + '</span>'
        + '<button class="sched-remove" data-wc-remove="' + idx + '" title="Remove">✕</button>';
      list.appendChild(li);
    });

    list.addEventListener('click', async function(e) {
      var btn = e.target.closest('[data-wc-remove]');
      if (!btn) return;
      var removeIdx = parseInt(btn.dataset.wcRemove);
      var cities = d.cities.filter(function(_, i) { return i !== removeIdx; });
      await apiJson('POST', '/api/world_clock', cities);
      fetchWorldClock();
    }, { once: true });
  } catch (_) {}
}


// ─── HTTP Status Monitor section ─────────────────────────────────────────────
(function injectHttpStatusSection() {
  if (document.getElementById('hs-add-btn')) return;
  var sec = document.createElement('section');
  sec.id = 'http-status-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>🌐 HTTP Monitor</h2>',
    '<p class="card-hint">Ping up to 10 URLs; status shown on panel with response codes and latency. Checks run every 30 s.</p>',
    '<ul id="hs-list" class="sched-list" style="margin-bottom:0.75rem"></ul>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">',
    '  <div class="form-group" style="flex:1"><label>Label</label>',
    '    <input type="text" id="hs-label" placeholder="My Site" style="width:100%"></div>',
    '  <div class="form-group" style="flex:2"><label>URL</label>',
    '    <input type="url" id="hs-url" placeholder="https://example.com" style="width:100%"></div>',
    '  <button id="hs-add-btn" class="btn-primary">Add</button>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;margin-top:0.5rem">',
    '  <button id="hs-mode-btn" class="btn-primary" style="flex:1">Switch to HTTP Monitor Mode</button>',
    '  <button id="hs-check-btn" class="btn-secondary" style="flex:1" title="Trigger immediate check">Check Now</button>',
    '</div>',
    '<div id="hs-results" style="margin-top:0.5rem;font-size:0.85rem;color:var(--overlay0)"></div>'
  ].join('\n');

  // Insert after world-clock section
  var wcSec = document.getElementById('world-clock-section');
  if (wcSec && wcSec.nextSibling) {
    wcSec.parentNode.insertBefore(sec, wcSec.nextSibling);
  } else if (wcSec) {
    wcSec.parentNode.appendChild(sec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  document.getElementById('hs-add-btn').addEventListener('click', async function() {
    var label = document.getElementById('hs-label').value.trim();
    var url   = document.getElementById('hs-url').value.trim();
    if (!label || !url) return;
    try {
      await apiJson('POST', '/api/http_status', { label, url });
      document.getElementById('hs-label').value = '';
      document.getElementById('hs-url').value = '';
      fetchHttpStatus();
      showToast('Monitor added: ' + label, 'ok', 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });

  document.getElementById('hs-mode-btn').addEventListener('click', async function() {
    await api('POST', '/api/mode', { mode: 'http_status' });
    fetchStatus();
  });

  document.getElementById('hs-check-btn').addEventListener('click', function() {
    showToast('HTTP check triggered (runs in background)', 'ok', 2000);
    fetchHttpStatus();
  });
})();

async function fetchHttpStatus() {
  try {
    var d = await apiJson('GET', '/api/http_status');
    var list = document.getElementById('hs-list');
    var resultsDiv = document.getElementById('hs-results');
    if (!list) return;
    list.innerHTML = '';
    if (!d.monitors || !d.monitors.length) {
      var li = document.createElement('li');
      li.textContent = 'No monitors configured yet.';
      li.style.color = 'var(--overlay0)';
      li.style.fontSize = '0.85rem';
      list.appendChild(li);
      if (resultsDiv) resultsDiv.textContent = '';
      return;
    }
    d.monitors.forEach(function(m, idx) {
      var res = (d.results || [])[idx];
      var ok = res ? res.ok : null;
      var code = res ? res.status_code : null;
      var lat  = res ? res.latency_ms  : null;
      var statusText = ok === null ? '…' : (ok ? '✓' : '✗');
      var statusColor = ok === null ? 'var(--overlay0)' : (ok ? 'var(--green)' : 'var(--red)');
      var li = document.createElement('li');
      li.className = 'sched-entry';
      li.innerHTML = '<span class="sched-mode" style="color:' + statusColor + '">' + statusText + ' ' + escHtml(m.label) + '</span>'
        + '<span class="sched-time" style="font-size:0.8rem;color:var(--overlay0)">' + escHtml(m.url.slice(0,40)) + '</span>'
        + (res ? '<span style="margin-left:0.5rem;font-size:0.8rem;color:' + statusColor + '">' + (code || 'timeout') + (lat ? ' ' + lat + 'ms' : '') + '</span>' : '')
        + '<button class="sched-remove" data-hs-remove="' + idx + '" title="Remove">✕</button>';
      list.appendChild(li);
    });

    list.addEventListener('click', async function(e) {
      var btn = e.target.closest('[data-hs-remove]');
      if (!btn) return;
      var idx = parseInt(btn.dataset.hsRemove);
      await api('DELETE', '/api/http_status/' + idx);
      fetchHttpStatus();
    }, { once: true });
  } catch (_) {}
}

// ─── Shell Command section ────────────────────────────────────────────────────
(function injectShellCmdSection() {
  if (document.getElementById('sc-run-btn')) return;
  var sec = document.createElement('section');
  sec.id = 'shell-cmd-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>💻 Shell Command</h2>',
    '<p class="card-hint">Execute a shell command and display stdout on the panel. Refreshes on the configured interval.</p>',
    '<div class="form-group"><label>Command</label>',
    '  <input type="text" id="sc-cmd" placeholder="uptime && df -h | head -4" style="width:100%"></div>',
    '<div class="form-row" style="gap:0.5rem;align-items:flex-end">',
    '  <div class="form-group" style="flex:1"><label>Refresh (seconds; 0 = manual)</label>',
    '    <input type="number" id="sc-interval" value="10" min="0" style="width:100%"></div>',
    '  <button id="sc-run-btn" class="btn-primary">Set &amp; Run</button>',
    '  <button id="sc-now-btn" class="btn-secondary" title="Force re-run now">↺ Now</button>',
    '  <button id="sc-clear-btn" class="btn-danger">✕</button>',
    '</div>',
    '<div id="sc-output" style="font-family:monospace;font-size:0.8rem;background:var(--crust);border-radius:6px;padding:0.6rem;margin-top:0.5rem;white-space:pre-wrap;max-height:150px;overflow-y:auto;color:var(--text)"></div>'
  ].join('\n');

  // Insert after http-status section
  var hsSec = document.getElementById('http-status-section');
  if (hsSec && hsSec.nextSibling) {
    hsSec.parentNode.insertBefore(sec, hsSec.nextSibling);
  } else if (hsSec) {
    hsSec.parentNode.appendChild(sec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  document.getElementById('sc-run-btn').addEventListener('click', async function() {
    var cmd      = document.getElementById('sc-cmd').value.trim();
    var interval = parseInt(document.getElementById('sc-interval').value) || 10;
    if (!cmd) return;
    try {
      await apiJson('POST', '/api/shell_cmd', { cmd, interval_secs: interval, switch_mode: true });
      showToast('Command set, running…', 'ok', 2000);
      setTimeout(fetchShellCmd, 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });

  document.getElementById('sc-now-btn').addEventListener('click', async function() {
    try {
      await apiJson('POST', '/api/shell_cmd/run');
      showToast('Re-running command…', 'ok', 1500);
      setTimeout(fetchShellCmd, 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });

  document.getElementById('sc-clear-btn').addEventListener('click', async function() {
    await api('DELETE', '/api/shell_cmd');
    document.getElementById('sc-output').textContent = '';
    document.getElementById('sc-cmd').value = '';
    fetchStatus();
    showToast('Shell command cleared', 'ok', 1500);
  });
})();

async function fetchShellCmd() {
  try {
    var d = await apiJson('GET', '/api/shell_cmd');
    var out = document.getElementById('sc-output');
    var cmdInput = document.getElementById('sc-cmd');
    if (!out) return;
    if (cmdInput && d.cmd) cmdInput.value = d.cmd;
    if (d.output) {
      out.style.color = d.exit_ok ? 'var(--text)' : 'var(--red)';
      out.textContent = d.output;
    } else {
      out.textContent = d.cmd ? '(no output yet)' : '(no command set)';
    }
  } catch (_) {}
}

// ─── Ping Monitor section ────────────────────────────────────────────────────
(function injectPingSection() {
  if (document.getElementById('pm-add-btn')) return;
  var sec = document.createElement('section');
  sec.id = 'ping-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>📶 Ping Monitor</h2>',
    '<p class="card-hint">ICMP ping up to 12 hosts; displays RTT and packet loss on panel. Checks run every 10 s.</p>',
    '<ul id="pm-list" class="sched-list" style="margin-bottom:0.75rem"></ul>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">',
    '  <div class="form-group" style="flex:1"><label>Label</label>',
    '    <input type="text" id="pm-label" placeholder="Router" style="width:100%"></div>',
    '  <div class="form-group" style="flex:2"><label>Host / IP</label>',
    '    <input type="text" id="pm-host" placeholder="192.168.1.1" style="width:100%"></div>',
    '  <button id="pm-add-btn" class="btn-primary">Add</button>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;margin-top:0.5rem">',
    '  <button id="pm-mode-btn" class="btn-primary" style="flex:1">Switch to Ping Monitor Mode</button>',
    '</div>',
    '<div id="pm-results" style="margin-top:0.5rem;font-size:0.85rem;color:var(--overlay0)"></div>'
  ].join('\n');

  var scSec = document.getElementById('shell-cmd-section');
  if (scSec && scSec.nextSibling) {
    scSec.parentNode.insertBefore(sec, scSec.nextSibling);
  } else if (scSec) {
    scSec.parentNode.appendChild(sec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  document.getElementById('pm-add-btn').addEventListener('click', async function() {
    var label = document.getElementById('pm-label').value.trim();
    var host  = document.getElementById('pm-host').value.trim();
    if (!label || !host) return;
    try {
      await apiJson('POST', '/api/ping_monitor', { label, host });
      document.getElementById('pm-label').value = '';
      document.getElementById('pm-host').value = '';
      fetchPing();
      showToast('Ping host added: ' + label, 'ok', 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });

  document.getElementById('pm-mode-btn').addEventListener('click', async function() {
    await api('POST', '/api/mode', { mode: 'ping_monitor' });
    fetchStatus();
  });
})();

async function fetchPing() {
  try {
    var d = await apiJson('GET', '/api/ping_monitor');
    var list = document.getElementById('pm-list');
    if (!list) return;
    list.innerHTML = '';
    if (!d.hosts || !d.hosts.length) {
      var li = document.createElement('li');
      li.textContent = 'No hosts configured yet.';
      li.style.color = 'var(--overlay0)';
      li.style.fontSize = '0.85rem';
      list.appendChild(li);
      return;
    }
    d.hosts.forEach(function(h, idx) {
      var res = (d.results || [])[idx];
      var lat = res ? res.latency_ms : null;
      var loss = res ? res.loss_pct : null;
      var ok = lat !== null && lat !== undefined;
      var statusColor = ok ? (lat < 50 ? 'var(--green)' : lat < 200 ? 'var(--yellow)' : 'var(--peach)') : 'var(--red)';
      var latStr = ok ? lat.toFixed(1) + ' ms' : 'timeout';
      var lossStr = (loss && loss > 0) ? ' ' + loss.toFixed(0) + '% loss' : '';
      var li = document.createElement('li');
      li.className = 'sched-entry';
      li.innerHTML = '<span class="sched-mode" style="color:' + statusColor + '">' + escHtml(h.label) + '</span>'
        + '<span class="sched-time" style="font-size:0.8rem;color:var(--overlay0)">' + escHtml(h.host) + '</span>'
        + (res ? '<span style="margin-left:0.5rem;font-size:0.8rem;color:' + statusColor + '">' + latStr + lossStr + '</span>' : '')
        + '<button class="sched-remove" data-pm-remove="' + idx + '" title="Remove">✕</button>';
      list.appendChild(li);
    });

    list.addEventListener('click', async function(e) {
      var btn = e.target.closest('[data-pm-remove]');
      if (!btn) return;
      var idx = parseInt(btn.dataset.pmRemove);
      await api('DELETE', '/api/ping_monitor/' + idx);
      fetchPing();
    }, { once: true });
  } catch (_) {}
}


// ─── Log tail section ─────────────────────────────────────────────────────────
(function injectLogTailSection() {
  if (document.getElementById('lt-path-input')) return;
  var sec = document.createElement('section');
  sec.id = 'log-tail-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>📄 Log Tail Viewer</h2>',
    '<p class="card-hint">Display the last N lines of any log file live on the panel. Updates every few seconds.</p>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">',
    '  <div class="form-group" style="flex:3"><label>File path</label>',
    '    <input type="text" id="lt-path-input" placeholder="/var/log/syslog" style="width:100%"></div>',
    '  <div class="form-group" style="flex:1"><label>Lines</label>',
    '    <input type="number" id="lt-n-input" value="16" min="1" max="50" style="width:100%"></div>',
    '  <div class="form-group" style="flex:1"><label>Interval (s)</label>',
    '    <input type="number" id="lt-interval-input" value="3" min="1" max="60" style="width:100%"></div>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;margin-top:0.4rem;flex-wrap:wrap">',
    '  <button id="lt-set-btn"   class="btn-primary" style="flex:2">Set & Switch to Log Tail</button>',
    '  <button id="lt-apply-btn" class="btn-secondary" style="flex:1">Apply (no switch)</button>',
    '  <button id="lt-clear-btn" class="btn-danger"    style="flex:1">Clear</button>',
    '</div>',
    '<div id="lt-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>',
    '<pre id="lt-preview" style="margin-top:0.5rem;background:var(--crust);color:var(--subtext1);font-size:0.75rem;',
    'max-height:160px;overflow-y:auto;padding:0.5rem;border-radius:4px;white-space:pre-wrap;word-break:break-all"></pre>',
  ].join('\n');

  var pmSec = document.getElementById('ping-section');
  if (pmSec && pmSec.nextSibling) {
    pmSec.parentNode.insertBefore(sec, pmSec.nextSibling);
  } else if (pmSec) {
    pmSec.parentNode.appendChild(sec);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  async function applyLogTail(switchMode) {
    var path  = document.getElementById('lt-path-input').value.trim();
    var n     = parseInt(document.getElementById('lt-n-input').value, 10) || 16;
    var secs  = parseInt(document.getElementById('lt-interval-input').value, 10) || 3;
    if (!path) { showToast('Enter a file path first', 'alert', 2000); return; }
    try {
      await apiJson('POST', '/api/log_tail', { path, n, interval_secs: secs, switch_mode: switchMode });
      showToast(switchMode ? 'Log tail started: ' + path : 'Log tail configured: ' + path, 'ok', 2500);
      setTimeout(fetchLogTail, 500);
      if (switchMode) fetchStatus();
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  }

  document.getElementById('lt-set-btn').addEventListener('click',   () => applyLogTail(true));
  document.getElementById('lt-apply-btn').addEventListener('click', () => applyLogTail(false));
  document.getElementById('lt-clear-btn').addEventListener('click', async function() {
    try {
      await api('DELETE', '/api/log_tail');
      document.getElementById('lt-status').textContent = 'Cleared.';
      document.getElementById('lt-preview').textContent = '';
      fetchStatus();
      showToast('Log tail cleared', 'ok', 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });
})();

async function fetchLogTail() {
  try {
    var d = await apiJson('GET', '/api/log_tail');
    var status = document.getElementById('lt-status');
    var preview = document.getElementById('lt-preview');
    if (!status || !preview) return;

    if (!d.path) {
      status.textContent = 'No file configured.';
      preview.textContent = '';
      return;
    }

    var okText = d.ok ? '✓ readable' : '✗ unreadable';
    var okColor = d.ok ? 'var(--green)' : 'var(--red)';
    status.innerHTML = '<span style="color:' + okColor + '">' + okText + '</span>'
      + ' &mdash; <code>' + escHtml(d.path) + '</code>'
      + ' (' + (d.lines ? d.lines.length : 0) + ' lines, every ' + d.interval_secs + 's)';

    // Update inputs if empty
    var pathInput = document.getElementById('lt-path-input');
    if (!pathInput.value) pathInput.value = d.path;

    if (d.lines && d.lines.length) {
      preview.textContent = d.lines.join('\n');
      // Scroll to bottom
      preview.scrollTop = preview.scrollHeight;
    } else {
      preview.textContent = d.ok ? '(no lines yet)' : '(file not readable)';
    }
  } catch (_) {}
}


// ─── RSS Feed section ─────────────────────────────────────────────────────────
(function injectRssFeedSection() {
  if (document.getElementById('rss-url-input')) return;
  var sec = document.createElement('section');
  sec.id = 'rss-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>📰 RSS / Atom Feed</h2>',
    '<p class="card-hint">Display the latest headlines from any RSS or Atom feed URL on the panel.</p>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">',
    '  <div class="form-group" style="flex:4"><label>Feed URL</label>',
    '    <input type="url" id="rss-url-input" placeholder="https://feeds.bbci.co.uk/news/rss.xml" style="width:100%"></div>',
    '  <div class="form-group" style="flex:1"><label>Interval (s)</label>',
    '    <input type="number" id="rss-interval-input" value="300" min="30" max="86400" style="width:100%"></div>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;margin-top:0.4rem;flex-wrap:wrap">',
    '  <button id="rss-set-btn"   class="btn-primary"   style="flex:2">Set & Switch to RSS Feed</button>',
    '  <button id="rss-fetch-btn" class="btn-secondary" style="flex:1">⟳ Fetch Now</button>',
    '  <button id="rss-clear-btn" class="btn-danger"    style="flex:1">Clear</button>',
    '</div>',
    '<div id="rss-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>',
    '<ul id="rss-items" style="margin-top:0.5rem;list-style:none;padding:0;font-size:0.82rem;max-height:160px;overflow-y:auto"></ul>',
  ].join('\n');

  var ltSec = document.getElementById('log-tail-section');
  if (ltSec && ltSec.nextSibling) {
    ltSec.parentNode.insertBefore(sec, ltSec.nextSibling);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  async function setRss(switchMode) {
    var url  = document.getElementById('rss-url-input').value.trim();
    var secs = parseInt(document.getElementById('rss-interval-input').value, 10) || 300;
    if (!url) { showToast('Enter a feed URL first', 'alert', 2000); return; }
    try {
      await apiJson('POST', '/api/rss', { url, interval_secs: secs, switch_mode: switchMode });
      showToast(switchMode ? 'RSS feed started' : 'RSS feed configured', 'ok', 2500);
      setTimeout(fetchRssFeed, 400);
      if (switchMode) fetchStatus();
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  }

  document.getElementById('rss-set-btn').addEventListener('click',   () => setRss(true));
  document.getElementById('rss-fetch-btn').addEventListener('click', async function() {
    try {
      await api('POST', '/api/rss/fetch');
      showToast('RSS feed fetched', 'ok', 2000);
      setTimeout(fetchRssFeed, 400);
    } catch (e) { showToast('Fetch error: ' + e.message, 'alert', 3000); }
  });
  document.getElementById('rss-clear-btn').addEventListener('click', async function() {
    try {
      await api('DELETE', '/api/rss');
      document.getElementById('rss-status').textContent = 'Cleared.';
      document.getElementById('rss-items').innerHTML = '';
      fetchStatus();
      showToast('RSS feed cleared', 'ok', 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });
})();

async function fetchRssFeed() {
  try {
    var d = await apiJson('GET', '/api/rss');
    var status = document.getElementById('rss-status');
    var items  = document.getElementById('rss-items');
    if (!status || !items) return;

    if (!d.url) {
      status.textContent = 'No feed configured.';
      items.innerHTML = '';
      return;
    }

    var okText  = d.ok ? '✓ live'    : '✗ unavailable';
    var okColor = d.ok ? 'var(--green)' : 'var(--red)';
    status.innerHTML = '<span style="color:' + okColor + '">' + okText + '</span>'
      + ' &mdash; <a href="' + escHtml(d.url) + '" target="_blank" rel="noopener" style="color:var(--blue)">'
      + escHtml(d.title || d.url) + '</a>'
      + ' (' + (d.items ? d.items.length : 0) + ' items)';

    // Update inputs if empty
    var urlInput = document.getElementById('rss-url-input');
    if (!urlInput.value) urlInput.value = d.url;

    if (d.items && d.items.length) {
      items.innerHTML = d.items.slice(0,6).map(function(item) {
        return '<li style="padding:0.25rem 0;border-bottom:1px solid var(--surface0)">'
          + '<span style="color:var(--text)">' + escHtml(item[0]) + '</span>'
          + (item[1] ? ' <span style="color:var(--overlay0);font-size:0.75rem">' + escHtml(item[1]) + '</span>' : '')
          + '</li>';
      }).join('');
    } else {
      items.innerHTML = '<li style="color:var(--overlay0)">' + (d.ok ? '(no items yet)' : '(feed unavailable)') + '</li>';
    }
  } catch (_) {}
}

// ─── Service Status section ───────────────────────────────────────────────────
(function injectServiceStatusSection() {
  if (document.getElementById('svc-units-input')) return;
  var sec = document.createElement('section');
  sec.id = 'service-section';
  sec.className = 'card';
  sec.innerHTML = [
    '<h2>⚙ Service Status Monitor</h2>',
    '<p class="card-hint">Watch systemd unit states on the panel. Enter unit names (one per line or comma-separated).</p>',
    '<div class="form-row" style="gap:0.5rem;flex-wrap:wrap;align-items:flex-end">',
    '  <div class="form-group" style="flex:3"><label>Units <span class="hint-inline">(e.g. nginx.service, sshd)</span></label>',
    '    <textarea id="svc-units-input" rows="3" placeholder="nginx.service&#10;sshd.service&#10;postgresql.service" style="width:100%;resize:vertical;background:var(--surface0);color:var(--text);border:1px solid var(--overlay0);border-radius:4px;padding:0.4rem"></textarea></div>',
    '  <div class="form-group" style="flex:1"><label>Poll interval (s)</label>',
    '    <input type="number" id="svc-interval-input" value="10" min="5" max="3600" style="width:100%"></div>',
    '</div>',
    '<div class="form-row" style="gap:0.5rem;margin-top:0.4rem;flex-wrap:wrap">',
    '  <button id="svc-set-btn"   class="btn-primary"   style="flex:2">Set & Switch to Services</button>',
    '  <button id="svc-apply-btn" class="btn-secondary" style="flex:1">Apply (no switch)</button>',
    '  <button id="svc-clear-btn" class="btn-danger"    style="flex:1">Clear</button>',
    '</div>',
    '<div id="svc-status" style="margin-top:0.5rem;font-size:0.82rem;color:var(--overlay0)"></div>',
    '<table id="svc-table" style="margin-top:0.5rem;width:100%;font-size:0.82rem;border-collapse:collapse"></table>',
  ].join('\n');

  var rssSec = document.getElementById('rss-section');
  if (rssSec && rssSec.nextSibling) {
    rssSec.parentNode.insertBefore(sec, rssSec.nextSibling);
  } else {
    document.querySelector('main').appendChild(sec);
  }

  function parseUnits(raw) {
    return raw.split(/[\n,]+/).map(s => s.trim()).filter(Boolean);
  }

  async function setServices(switchMode) {
    var raw   = document.getElementById('svc-units-input').value;
    var units = parseUnits(raw);
    var secs  = parseInt(document.getElementById('svc-interval-input').value, 10) || 10;
    if (!units.length) { showToast('Enter at least one unit name', 'alert', 2000); return; }
    try {
      await apiJson('POST', '/api/services', { units, interval_secs: secs, switch_mode: switchMode });
      showToast(switchMode ? 'Services monitor started' : 'Services configured', 'ok', 2500);
      setTimeout(fetchServices, 400);
      if (switchMode) fetchStatus();
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  }

  document.getElementById('svc-set-btn').addEventListener('click',   () => setServices(true));
  document.getElementById('svc-apply-btn').addEventListener('click', () => setServices(false));
  document.getElementById('svc-clear-btn').addEventListener('click', async function() {
    try {
      await api('DELETE', '/api/services');
      document.getElementById('svc-status').textContent = 'Cleared.';
      document.getElementById('svc-table').innerHTML = '';
      fetchStatus();
      showToast('Services cleared', 'ok', 2000);
    } catch (e) { showToast('Error: ' + e.message, 'alert', 3000); }
  });
})();

async function fetchServices() {
  try {
    var d = await apiJson('GET', '/api/services');
    var status = document.getElementById('svc-status');
    var table  = document.getElementById('svc-table');
    if (!status || !table) return;

    if (!d.units || !d.units.length) {
      status.textContent = 'No units configured.';
      table.innerHTML = '';
      return;
    }

    var active  = d.statuses.filter(s => s.state === 'active').length;
    var failed  = d.statuses.filter(s => s.state === 'failed').length;
    status.innerHTML = '<span style="color:var(--green)">' + active + ' active</span>'
      + '  <span style="color:var(--red)">' + failed + ' failed</span>'
      + '  of ' + d.units.length + ' units'
      + ' (poll every ' + d.interval + 's)';

    // Update textarea if empty
    var ta = document.getElementById('svc-units-input');
    if (!ta.value && d.units.length) ta.value = d.units.join('\n');

    var stateColor = {active:'var(--green)',inactive:'var(--overlay0)',failed:'var(--red)'};
    var stateIcon  = {active:'●',inactive:'○',failed:'✖'};
    table.innerHTML = '<tr style="border-bottom:1px solid var(--surface0)">'
      + '<th style="text-align:left;padding:0.2rem 0.3rem;color:var(--overlay0)">Unit</th>'
      + '<th style="text-align:left;padding:0.2rem 0.3rem;color:var(--overlay0)">State</th>'
      + '<th style="text-align:left;padding:0.2rem 0.3rem;color:var(--overlay0)">Sub</th>'
      + '</tr>'
      + (d.statuses || []).map(function(s) {
          var c = stateColor[s.state] || 'var(--yellow)';
          var ic = stateIcon[s.state] || '?';
          return '<tr style="border-bottom:1px solid var(--surface0)">'
            + '<td style="padding:0.2rem 0.3rem;color:var(--text)">' + escHtml(s.name.replace(/\.service$/, '')) + '</td>'
            + '<td style="padding:0.2rem 0.3rem;color:' + c + '">' + ic + ' ' + escHtml(s.state) + '</td>'
            + '<td style="padding:0.2rem 0.3rem;color:var(--subtext1)">' + escHtml(s.sub) + '</td>'
            + '</tr>';
        }).join('');
  } catch (_) {}
}

// ─── Keyboard shortcuts ───────────────────────────────────────────────────────
document.addEventListener('keydown', e => {
  // Don't fire shortcuts when typing in a text input
  const tag = document.activeElement.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
  if (e.metaKey || e.ctrlKey) return;

  const modeList = Array.from(modeBtns).map(b => b.dataset.mode);

  switch (e.key) {
    case ' ':
      e.preventDefault();
      document.getElementById('sw-start-btn')?.click();
      break;
    case 'l':
      document.getElementById('sw-lap-btn')?.click();
      break;
    case 'f':
      freezeBtn.click();
      break;
    case 'w':
      document.getElementById('weather-fetch-btn')?.click();
      break;
    case 'D':  // Shift+D = Dashboard mode
      api('POST', '/api/mode', { mode: 'dashboard' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'H':  // Shift+H = HTTP Monitor mode
      api('POST', '/api/mode', { mode: 'http_status' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'L':  // Shift+L = Log tail mode
      api('POST', '/api/mode', { mode: 'log_tail' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'F':  // Shift+F = CPU Freq mode
      api('POST', '/api/mode', { mode: 'cpu_freq' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'P':  // Shift+P = Pomodoro start/pause toggle
      (async () => {
        try {
          var d = await apiJson('GET', '/api/pomodoro');
          await apiJson('POST', d.running ? '/api/pomodoro/pause' : '/api/pomodoro/start');
          fetchPomodoroState();
        } catch (_) {}
      })();
      break;
    case 'S':  // Shift+S = Pomodoro skip phase
      (async () => {
        try {
          await apiJson('POST', '/api/pomodoro/skip');
          fetchPomodoroState();
          showToast('Pomodoro phase skipped', 'ok', 2000);
        } catch (_) {}
      })();
      break;
    case 'I':  // Shift+I = Hostname/IP
      api('POST', '/api/mode', { mode: 'hostname_ip' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'R':  // Shift+R = RSS Feed
      api('POST', '/api/mode', { mode: 'rss_feed' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'V':  // Shift+V = Services
      api('POST', '/api/mode', { mode: 'service_status' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'U':  // Shift+U = Uptime Fancy
      api('POST', '/api/mode', { mode: 'uptime_fancy' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'C':  // Shift+C = Net Connections
      api('POST', '/api/mode', { mode: 'net_connections' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'O':  // Shift+O = OS Info
      api('POST', '/api/mode', { mode: 'os_info' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'K':  // Shift+K = Docker Status
      api('POST', '/api/mode', { mode: 'docker_status' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'W':  // Shift+W = Logged-in Users
      api('POST', '/api/mode', { mode: 'logged_in_users' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'T':  // Shift+T = Temp Graph
      api('POST', '/api/mode', { mode: 'temp_graph' }).then(() => fetchStatus()).catch(() => {});
      break;
    case 'Escape':
      if (returnBtn.style.display !== 'none') returnBtn.click();
      break;
    case 'ArrowRight':
    case 'n': {
      e.preventDefault();
      api('POST', '/api/mode/next').then(() => fetchStatus()).catch(() => {});
      break;
    }
    case 'ArrowLeft':
    case 'p': {
      e.preventDefault();
      api('POST', '/api/mode/prev').then(() => fetchStatus()).catch(() => {});
      break;
    }
    default:
      if (e.key >= '1' && e.key <= '9') {
        const idx = parseInt(e.key, 10) - 1;
        if (modeList[idx]) {
          api('POST', '/api/mode', { mode: modeList[idx] }).then(() => {
            updateActiveModeButton(modeList[idx]);
          }).catch(() => {});
        }
      }
  }
});

// ─── Settings export / import ─────────────────────────────────────────────────
(function injectSettingsIO() {
  if ($('settings-import-btn')) return;
  const section = document.createElement('section');
  section.className = 'card';
  section.innerHTML = `
    <h2>Settings Backup</h2>
    <p class="card-hint">Export or import all daemon settings: presets, profiles, thresholds, dim schedule, notes, and metrics.</p>
    <div class="form-actions">
      <a id="settings-export-btn" href="/api/settings/export" download="ttlcd-settings.json"
         class="btn-link" style="text-decoration:none;display:inline-block">⬇ Export All Settings</a>
      <label class="btn-link" style="cursor:pointer;display:inline-block">
        ⬆ Import Settings
        <input type="file" id="settings-import-input" accept=".json" hidden>
      </label>
      <span id="settings-io-status"></span>
    </div>
  `;
  // Insert before the log section
  document.querySelector('.log-card')?.insertAdjacentElement('beforebegin', section);

  document.getElementById('settings-import-input')?.addEventListener('change', async function() {
    const file = this.files?.[0];
    if (!file) return;
    const statusEl = $('settings-io-status');
    try {
      const text = await file.text();
      const json = JSON.parse(text);
      const r    = await api('POST', '/api/settings/import', json);
      if (!r.ok) throw new Error(await r.text());
      const d = await r.json();
      setStatusMsg(statusEl, `✓ Imported: ${(d.applied||[]).join(', ')}`, true);
      log(`Settings imported from ${file.name}: ${d.count} section(s)`);
      // Refresh UI state
      setQuality.dataset.synced    = '';
      setOrient.dataset.synced     = '';
      setBrightness.dataset.synced = '';
      await fetchPresets();
      await fetchProfiles();
      await fetchThresholds();
      await fetchDimSchedule();
      await fetchCachedNotes();
      await fetchMetrics();
    } catch (e) {
      setStatusMsg($('settings-io-status'), `Error: ${e.message}`, false);
      log(`Settings import error: ${e.message}`, 'err');
    }
    this.value = '';
  });
})();

// ─── Polling intervals ────────────────────────────────────────────────────────
function startPolling() {
  fetchStatus();
  fetchPanelInfo();
  fetchHistory();
  fetchPresets();
  fetchStopwatch();
  fetchThresholds();
  fetchDimSchedule();
  fetchMetrics();
  fetchProfiles();
  fetchSnapshot();
  fetchCachedWeather();
  fetchCachedNotes();
  fetchWebhookConfig();
  fetchFpsHistory();
  updateNetSparkline();
  fetchQrState();
  fetchModeSchedule();
  fetchPomodoroState();
  fetchWorldClock();
  fetchHttpStatus();
  fetchShellCmd();
  fetchPing();
  fetchLogTail();
  fetchRssFeed();
  fetchServices();

  statusPollTimer  = setInterval(fetchStatus,    3000);
  snapshotTimer    = setInterval(fetchSnapshot,  5000);
  setInterval(fetchHistory,       30000);
  setInterval(fetchStopwatch,     2000);
  setInterval(fetchPomodoroState, 3000);
  setInterval(fetchFpsHistory,    5000);
  setInterval(updateNetSparkline, 10000);
  setInterval(fetchShellCmd,      8000);
  setInterval(fetchHttpStatus,    35000);
  setInterval(fetchPing,          15000);
  setInterval(fetchLogTail,  8000);
  setInterval(fetchRssFeed, 30000);
  setInterval(fetchServices, 15000);
}

// ─── Progress display section ────────────────────────────────────────────────

(function injectProgressSection() {
  const setBtn   = document.getElementById('prog-set-btn');
  const clearBtn = document.getElementById('prog-clear-btn');
  const statusEl = document.getElementById('prog-status');

  if (setBtn) {
    setBtn.addEventListener('click', async () => {
      const label     = document.getElementById('prog-label')?.value || 'Progress';
      const sublabel  = document.getElementById('prog-sublabel')?.value || '';
      const value     = parseFloat(document.getElementById('prog-value')?.value || '0');
      const total     = parseFloat(document.getElementById('prog-total')?.value || '100');
      const color     = document.getElementById('prog-color')?.value || 'blue';
      const startTimer = document.getElementById('prog-timer-check')?.checked || false;
      try {
        const r = await api('POST', '/api/progress', { label, sublabel, value, total, color, start_timer: startTimer });
        if (statusEl) statusEl.textContent = `${r.pct ? r.pct.toFixed(1) : 0}% — showing on panel`;
        fetchStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  if (clearBtn) {
    clearBtn.addEventListener('click', async () => {
      try {
        await api('DELETE', '/api/progress');
        if (statusEl) statusEl.textContent = 'Cleared';
        fetchStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }
})();

// ─── Docker status section ────────────────────────────────────────────────────

async function fetchDocker() {
  try {
    const data = await api('GET', '/api/docker');
    const tbody = document.getElementById('docker-tbody');
    const statusLine = document.getElementById('docker-status-line');
    if (!tbody) return;
    if (!data || typeof data !== 'object') {
      tbody.innerHTML = '<tr><td colspan="3" style="color:var(--red)">Error fetching Docker info</td></tr>';
      return;
    }
    const containers = data.containers || [];
    statusLine.textContent = containers.length === 0
      ? 'Docker not available or no containers'
      : `${data.running}/${data.total} running`;
    if (containers.length === 0) {
      tbody.innerHTML = '<tr><td colspan="3" style="opacity:0.5">No containers found</td></tr>';
      return;
    }
    tbody.innerHTML = containers.map(c => {
      const color = c.running ? 'var(--green)' : 'var(--red)';
      const dot   = c.running ? '🟢' : '🔴';
      const name  = (c.name || '').replace(/^\//, '');
      const image = (c.image || '').split('/').pop().split(':')[0];
      const status = c.status || 'unknown';
      return `<tr>
        <td>${dot} <strong>${name}</strong></td>
        <td style="opacity:0.75">${image}</td>
        <td style="color:${color}">${status}</td>
      </tr>`;
    }).join('');
  } catch (e) {
    const tbody = document.getElementById('docker-tbody');
    if (tbody) tbody.innerHTML = `<tr><td colspan="3" style="color:var(--red)">Error: ${e.message}</td></tr>`;
  }
}

(function injectDockerSection() {
  const refreshBtn = document.getElementById('docker-refresh-btn');
  if (refreshBtn) refreshBtn.addEventListener('click', fetchDocker);
  fetchDocker();
})();

// ─── Active sessions section ──────────────────────────────────────────────────

async function fetchSessions() {
  try {
    const data = await api('GET', '/api/sessions');
    const tbody = document.getElementById('sessions-tbody');
    const statusLine = document.getElementById('sessions-status-line');
    if (!tbody) return;
    const sessions = data.sessions || [];
    statusLine.textContent = sessions.length === 0 ? 'No active sessions' : `${sessions.length} session${sessions.length !== 1 ? 's' : ''}`;
    if (sessions.length === 0) {
      tbody.innerHTML = '<tr><td colspan="5" style="opacity:0.5">No users logged in</td></tr>';
      return;
    }
    tbody.innerHTML = sessions.map(s => {
      const idleColor = (s.idle === '.') ? 'var(--green)' : 'var(--yellow)';
      const idleStr   = (s.idle === '.') ? 'active' : s.idle;
      const fromStr   = s.from || 'local';
      return `<tr>
        <td><strong>${s.username}</strong></td>
        <td style="font-family:monospace">${s.tty}</td>
        <td>${s.login_at}</td>
        <td style="color:${idleColor}">${idleStr}</td>
        <td style="opacity:0.75">${fromStr}</td>
      </tr>`;
    }).join('');
  } catch (e) {
    const tbody = document.getElementById('sessions-tbody');
    if (tbody) tbody.innerHTML = `<tr><td colspan="5" style="color:var(--red)">Error: ${e.message}</td></tr>`;
  }
}

(function injectSessionsSection() {
  const refreshBtn = document.getElementById('sessions-refresh-btn');
  if (refreshBtn) refreshBtn.addEventListener('click', fetchSessions);
  fetchSessions();
})();

// ─── ntfy.sh push notifications section ──────────────────────────────────────

async function fetchNtfyStatus() {
  try {
    const data = await api('GET', '/api/ntfy/config');
    const line = document.getElementById('ntfy-status-line');
    const urlInput = document.getElementById('ntfy-url-input');
    if (!line) return;
    if (data.configured) {
      line.innerHTML = `🔔 <strong>Active</strong> — sending to <code>${data.url}</code>`;
      line.style.color = 'var(--green)';
      if (urlInput && !urlInput.value) urlInput.value = data.url;
    } else {
      line.innerHTML = '🔕 Not configured — no push notifications will be sent.';
      line.style.color = 'var(--overlay0)';
    }
  } catch (_) {}
}

(function injectNtfySection() {
  const setBtn   = document.getElementById('ntfy-set-btn');
  const testBtn  = document.getElementById('ntfy-test-btn');
  const clearBtn = document.getElementById('ntfy-clear-btn');
  const urlInput = document.getElementById('ntfy-url-input');
  const statusEl = document.getElementById('ntfy-action-status');

  if (setBtn) {
    setBtn.addEventListener('click', async () => {
      const url = urlInput ? urlInput.value.trim() : '';
      if (!url) { if (statusEl) statusEl.textContent = 'Enter a URL first'; return; }
      try {
        const r = await api('POST', '/api/ntfy/config', { url });
        if (statusEl) statusEl.textContent = r.message || 'Saved';
        await fetchNtfyStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  if (testBtn) {
    testBtn.addEventListener('click', async () => {
      try {
        const r = await api('POST', '/api/ntfy/test');
        if (statusEl) statusEl.textContent = r.message || 'Test sent';
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  if (clearBtn) {
    clearBtn.addEventListener('click', async () => {
      try {
        await api('DELETE', '/api/ntfy/config');
        if (statusEl) statusEl.textContent = 'Cleared';
        if (urlInput) urlInput.value = '';
        await fetchNtfyStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  fetchNtfyStatus();
})();

// ─── API authentication section ───────────────────────────────────────────────

async function fetchAuthStatus() {
  try {
    const data = await api('GET', '/api/auth/status');
    const line = document.getElementById('auth-status-line');
    if (!line) return;
    if (data.auth_required) {
      line.innerHTML = '🔒 <strong>Authentication is active</strong> — Bearer token required for all API calls.';
      line.style.color = 'var(--green)';
    } else {
      line.innerHTML = '🔓 Authentication is <em>disabled</em> — API is open.';
      line.style.color = 'var(--yellow)';
    }
  } catch (_) {}
}

(function injectAuthSection() {
  const setBtn   = document.getElementById('auth-set-btn');
  const clearBtn = document.getElementById('auth-clear-btn');
  const keyInput = document.getElementById('auth-key-input');
  const statusEl = document.getElementById('auth-action-status');

  if (setBtn) {
    setBtn.addEventListener('click', async () => {
      const key = keyInput ? keyInput.value.trim() : '';
      if (!key) { if (statusEl) statusEl.textContent = 'Enter a key first'; return; }
      try {
        const r = await api('POST', '/api/auth/key', { key });
        if (statusEl) statusEl.textContent = r.message || 'Key set';
        if (keyInput) keyInput.value = '';
        await fetchAuthStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  if (clearBtn) {
    clearBtn.addEventListener('click', async () => {
      try {
        await api('DELETE', '/api/auth/key');
        if (statusEl) statusEl.textContent = 'Key cleared — auth disabled';
        await fetchAuthStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  fetchAuthStatus();
})();




// ─── MJPEG live stream toggle ─────────────────────────────────────────────────
(function () {
  const toggleBtn = document.getElementById('mjpeg-toggle-btn');
  if (!toggleBtn) return;

  let mjpegActive = false;

  toggleBtn.addEventListener('click', () => {
    mjpegActive = !mjpegActive;
    if (mjpegActive) {
      // Switch to embedded MJPEG stream
      panelImg.src = `/api/stream?t=${Date.now()}`;
      panelImg.style.display = 'block';
      panelEmpty.style.display = 'none';
      toggleBtn.textContent = '📷 Poll';
      toggleBtn.title = 'Switch back to JPEG polling mode';
      toggleBtn.style.color = 'var(--green)';
      log('Switched to embedded MJPEG stream');
    } else {
      // Switch back to JPEG polling
      panelImg.src = '';
      toggleBtn.textContent = '📺 MJPEG';
      toggleBtn.title = 'Toggle between JPEG polling and embedded MJPEG stream';
      toggleBtn.style.color = '';
      log('Switched back to JPEG polling');
      // Refresh immediately
      refreshPreview();
    }
  });
})();

// ─── Mode search / filter ─────────────────────────────────────────────────────
(function () {
  const searchInput = document.getElementById('mode-search');
  const countEl     = document.getElementById('mode-filter-count');
  if (!searchInput) return;

  function applyFilter() {
    const q = searchInput.value.trim().toLowerCase();
    const btns = document.querySelectorAll('#mode-grid .mode-btn');
    let visible = 0;
    btns.forEach(btn => {
      const mode  = (btn.dataset.mode  || '').toLowerCase();
      const label = (btn.querySelector('span')?.textContent || '').toLowerCase();
      const title = (btn.title || '').toLowerCase();
      const match = !q || mode.includes(q) || label.includes(q) || title.includes(q);
      btn.classList.toggle('hidden-by-filter', !match);
      if (match) visible++;
    });
    if (q && countEl) {
      countEl.textContent = `${visible} of ${btns.length} modes shown`;
    } else if (countEl) {
      countEl.textContent = '';
    }
  }

  searchInput.addEventListener('input', applyFilter);

  // Clear filter on Escape when search is focused
  searchInput.addEventListener('keydown', e => {
    if (e.key === 'Escape') {
      searchInput.value = '';
      applyFilter();
      searchInput.blur();
    }
  });

  // Focus search on '/' key (when not in an input)
  document.addEventListener('keydown', e => {
    if (e.key === '/' && document.activeElement.tagName !== 'INPUT' &&
        document.activeElement.tagName !== 'TEXTAREA' &&
        document.activeElement.tagName !== 'SELECT') {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
    }
  });
})();


// ─── Settings export / import ─────────────────────────────────────────────────
(function () {
  const importInput = document.getElementById('settings-import-input');
  if (!importInput) return;

  importInput.addEventListener('change', async () => {
    const file = importInput.files && importInput.files[0];
    if (!file) return;
    const statusEl = document.getElementById('set-status');
    try {
      const text = await file.text();
      const data = JSON.parse(text);
      const r = await api('POST', '/api/settings/import', data);
      if (statusEl) statusEl.textContent = `Imported: ${r.applied || 0} settings applied`;
      log(`Settings imported from ${file.name}`);
      // Reload status to reflect new settings
      await fetchStatus();
    } catch (e) {
      if (statusEl) statusEl.textContent = `Import failed: ${e.message}`;
    }
    importInput.value = '';
  });
})();


// ─── Alert thresholds ─────────────────────────────────────────────────────────

async function fetchThresholds() {
  try {
    const data = await api('GET', '/api/alerts/thresholds');
    const line  = document.getElementById('threshold-status-line');
    if (line) {
      const parts = [];
      if (data.cpu_pct > 0) parts.push(`CPU > ${data.cpu_pct}%`);
      if (data.temp_c > 0)  parts.push(`Temp > ${data.temp_c}°C`);
      if (parts.length > 0) {
        line.innerHTML = `⚠ Active thresholds: ${parts.join(', ')} — cooldown ${data.cooldown_secs}s`;
        line.style.color = 'var(--yellow)';
      } else {
        line.innerHTML = '✓ No thresholds configured (all disabled)';
        line.style.color = 'var(--overlay0)';
      }
    }
    // Populate form fields
    const cpuEl  = document.getElementById('thresh-cpu');
    const tempEl = document.getElementById('thresh-temp');
    const cdEl   = document.getElementById('thresh-cooldown');
    if (cpuEl  && data.cpu_pct   !== undefined) cpuEl.value  = data.cpu_pct;
    if (tempEl && data.temp_c    !== undefined) tempEl.value = data.temp_c;
    if (cdEl   && data.cooldown_secs !== undefined) cdEl.value = data.cooldown_secs;
  } catch (_) {}
}

(function injectThresholdsSection() {
  const saveBtn = document.getElementById('thresh-save-btn');
  const statusEl = document.getElementById('thresh-action-status');
  if (!saveBtn) return;

  saveBtn.addEventListener('click', async () => {
    const cpu_pct      = parseFloat(document.getElementById('thresh-cpu')?.value || '0');
    const temp_c       = parseFloat(document.getElementById('thresh-temp')?.value || '0');
    const cooldown_secs = parseInt(document.getElementById('thresh-cooldown')?.value || '120', 10);
    try {
      await api('POST', '/api/alerts/thresholds', { cpu_pct, temp_c, cooldown_secs });
      if (statusEl) statusEl.textContent = 'Thresholds saved';
      await fetchThresholds();
    } catch (e) {
      if (statusEl) statusEl.textContent = `Error: ${e.message}`;
    }
  });

  fetchThresholds();
})();

// ─── Screensaver / idle timeout section ──────────────────────────────────────

async function fetchScreensaverStatus() {
  try {
    const data = await api('GET', '/api/screensaver');
    const line = document.getElementById('screensaver-status-line');
    if (!line) return;
    if (data.enabled) {
      const since = data.seconds_since_activity;
      const remaining = Math.max(0, data.timeout_secs - since);
      if (data.active) {
        line.innerHTML = `💤 <strong>Screensaver active</strong> — showing <em>${data.mode}</em>. Any API call wakes the display.`;
        line.style.color = 'var(--blue)';
      } else {
        line.innerHTML = `✅ Screensaver enabled — activates after <strong>${data.timeout_secs}s</strong> of inactivity `
          + `(mode: <em>${data.mode}</em>, ${remaining}s remaining).`;
        line.style.color = 'var(--green)';
      }
    } else {
      line.innerHTML = '⚪ Screensaver is <em>disabled</em>.';
      line.style.color = 'var(--overlay0)';
    }
    // Update form fields
    const timeoutInput = document.getElementById('ss-timeout-input');
    const modeSelect   = document.getElementById('ss-mode-select');
    if (timeoutInput) timeoutInput.value = data.timeout_secs;
    if (modeSelect) modeSelect.value = data.mode;
  } catch (_) {}
}

(function injectScreensaverSection() {
  const saveBtn    = document.getElementById('ss-save-btn');
  const wakeBtn    = document.getElementById('ss-wake-btn');
  const disableBtn = document.getElementById('ss-disable-btn');
  const statusEl   = document.getElementById('ss-action-status');

  if (saveBtn) {
    saveBtn.addEventListener('click', async () => {
      const timeout = parseInt(document.getElementById('ss-timeout-input')?.value || '0', 10);
      const mode    = document.getElementById('ss-mode-select')?.value || 'matrix_rain';
      try {
        await api('POST', '/api/screensaver', { timeout_secs: timeout, mode });
        if (statusEl) statusEl.textContent = timeout > 0 ? `Saved — activates after ${timeout}s idle` : 'Disabled';
        await fetchScreensaverStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  if (wakeBtn) {
    wakeBtn.addEventListener('click', async () => {
      try {
        const r = await api('POST', '/api/screensaver/wake');
        if (statusEl) statusEl.textContent = r.was_active ? 'Woke from screensaver' : 'Not in screensaver';
        await fetchScreensaverStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  if (disableBtn) {
    disableBtn.addEventListener('click', async () => {
      try {
        await api('DELETE', '/api/screensaver');
        if (statusEl) statusEl.textContent = 'Screensaver disabled';
        await fetchScreensaverStatus();
      } catch (e) {
        if (statusEl) statusEl.textContent = `Error: ${e.message}`;
      }
    });
  }

  fetchScreensaverStatus();
  setInterval(fetchScreensaverStatus, 15000);  // refresh status every 15s
})();



// ─── Recently Used Modes tracking ────────────────────────────────────────────
(function initRecentModes() {
  const STORAGE_KEY = 'ttlcd-recent-modes';
  const MAX_RECENT  = 6;

  function loadRecent() {
    try { return JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]'); }
    catch (_) { return []; }
  }

  function saveRecent(list) {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(list.slice(0, MAX_RECENT))); }
    catch (_) {}
  }

  function pushRecent(mode) {
    let list = loadRecent().filter(m => m !== mode);
    list.unshift(mode);
    saveRecent(list);
    renderRecentBand();
  }

  // Create the "Recent" band above the mode grid
  const modeGrid = document.getElementById('mode-grid');
  if (!modeGrid) return;
  let recentBand = document.getElementById('recent-modes-band');
  if (!recentBand) {
    recentBand = document.createElement('div');
    recentBand.id = 'recent-modes-band';
    recentBand.className = 'recent-band';
    const favBand = document.getElementById('fav-modes-band');
    if (favBand) favBand.after(recentBand);
    else {
      const header = modeGrid.closest('.card')?.querySelector('.section-header-row');
      if (header) header.after(recentBand);
      else modeGrid.before(recentBand);
    }
  }

  function renderRecentBand() {
    const list = loadRecent();
    if (list.length === 0) {
      recentBand.innerHTML = '';
      recentBand.style.display = 'none';
      return;
    }
    recentBand.style.display = 'flex';
    recentBand.innerHTML = '<span class="recent-band-label">⏱ Recent:</span>';
    list.forEach(mode => {
      const orig = document.querySelector(`[data-mode="${mode}"]`);
      if (!orig) return;
      const btn = document.createElement('button');
      btn.className = 'mode-btn recent-btn';
      btn.dataset.mode = mode;
      btn.title = orig.title || mode;
      // Simplified label - just the text span content
      const span = orig.querySelector('span');
      const emoji = orig.textContent.replace(span?.textContent || '', '').trim().replace(/★|☆/g, '');
      btn.innerHTML = `${emoji}<span>${span?.textContent || mode}</span>`;
      btn.addEventListener('click', async () => {
        try {
          await api('POST', '/api/mode', { mode });
          log(`Mode → ${mode}`);
          updateActiveModeButton(mode);
          pushRecent(mode);
        } catch (e) { log(`Mode change failed: ${e.message}`, 'err'); }
      });
      recentBand.appendChild(btn);
    });

    // Clear button
    const clearBtn = document.createElement('button');
    clearBtn.className = 'recent-clear-btn';
    clearBtn.title = 'Clear recently used list';
    clearBtn.textContent = '✕';
    clearBtn.addEventListener('click', e => {
      e.stopPropagation();
      saveRecent([]);
      renderRecentBand();
    });
    recentBand.appendChild(clearBtn);
  }

  // Track mode switches: patch the modeBtns click handlers by listening globally
  document.addEventListener('click', e => {
    const btn = e.target.closest('.mode-btn[data-mode]');
    if (btn && !btn.classList.contains('recent-btn')) {
      pushRecent(btn.dataset.mode);
    }
  });

  // Also track API mode switches from fetchStatus
  const _origUpdateActiveModeButton = window.updateActiveModeButton;
  // Intercept mode changes from status polling
  const _origFetchStatus = window.fetchStatus;

  renderRecentBand();
})();



// ─── Mode category filter ─────────────────────────────────────────────────────
(function initCategoryFilter() {
  const catBtns  = document.querySelectorAll('.cat-btn[data-cat]');
  const modeBtns = document.querySelectorAll('.mode-btn[data-mode]');
  if (!catBtns.length) return;

  let activeCat = localStorage.getItem('ttlcd-active-cat') || 'all';

  function applyFilter() {
    modeBtns.forEach(btn => {
      const cat = btn.dataset.cat || '';
      if (activeCat === 'all' || cat === activeCat) {
        btn.classList.remove('hidden-by-cat');
      } else {
        btn.classList.add('hidden-by-cat');
      }
    });

    catBtns.forEach(b => {
      b.classList.toggle('active', b.dataset.cat === activeCat);
    });

    // Update count (combined with search filter count if applicable)
    const filterCount = document.getElementById('mode-filter-count');
    const visible = [...modeBtns].filter(b =>
      !b.classList.contains('hidden-by-cat') &&
      !b.classList.contains('hidden-by-filter')
    ).length;
    if (filterCount) {
      if (activeCat !== 'all') {
        filterCount.textContent = `Showing ${visible} mode${visible !== 1 ? 's' : ''} in ${activeCat}`;
      } else if (filterCount.textContent.startsWith('Showing')) {
        filterCount.textContent = '';
      }
    }

    try { localStorage.setItem('ttlcd-active-cat', activeCat); } catch (_) {}
  }

  catBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      activeCat = btn.dataset.cat;
      applyFilter();
    });
  });

  applyFilter();
})();


// ─── Web UI QR code button ─────────────────────────────────────────────────────
(function initWebQrBtn() {
  const btn = document.getElementById('web-qr-btn');
  if (!btn) return;
  btn.addEventListener('click', async () => {
    try {
      // Generate a URL pointing to this web UI
      const url = window.location.origin;
      const hostname = window.location.hostname;
      const port = window.location.port ? `:${window.location.port}` : '';
      const webUrl = `http://${hostname}${port}`;
      await api('POST', '/api/qr', {
        payload:  webUrl,
        title:    'ttlcd Web UI',
        display:  true
      });
      log(`QR code shown: ${webUrl}`);
      setStatusMsg(quickStatus, `✓ Web UI QR shown`, true);
    } catch (e) {
      log(`QR code failed: ${e.message}`, 'err');
      setStatusMsg(quickStatus, `QR error: ${e.message}`, false);
    }
  });
})();


// ─── Reset panel button ───────────────────────────────────────────────────────
(function initResetPanelBtn() {
  const btn = document.getElementById('reset-panel-btn');
  if (!btn) return;
  btn.addEventListener('click', async () => {
    if (!confirm('Reset panel to clock mode and stop rotation / screensaver?')) return;
    try {
      const data = await apiJson('POST', '/api/display/reset');
      log(`Panel reset: ${data.message || 'ok'}`);
      setStatusMsg(quickStatus, '✓ Panel reset to clock', true);
      await fetchStatus();
    } catch (e) {
      log(`Reset failed: ${e.message}`, 'err');
      setStatusMsg(quickStatus, `Reset error: ${e.message}`, false);
    }
  });
})();

// ─── Fullscreen panel preview ────────────────────────────────────────────────
(function initFullscreenPreview() {
  const overlay   = document.getElementById('fullscreen-overlay');
  const fsImg     = document.getElementById('fullscreen-img');
  const fsBtn     = document.getElementById('fullscreen-preview-btn');
  const closeBtn  = document.getElementById('fullscreen-close');
  const panelImg  = document.getElementById('panel-preview-img');
  if (!overlay || !fsBtn || !panelImg) return;

  let fsTimer = null;

  function showFullscreen() {
    if (!overlay) return;
    overlay.style.display = 'flex';
    // Set initial image
    fsImg.src = panelImg.src;
    // Auto-refresh at same rate as main preview
    fsTimer = setInterval(() => {
      fsImg.src = `/api/preview?_=${Date.now()}`;
    }, 1000);
    document.addEventListener('keydown', onEscKey, { once: true });
  }

  function hideFullscreen() {
    if (!overlay) return;
    overlay.style.display = 'none';
    if (fsTimer) { clearInterval(fsTimer); fsTimer = null; }
  }

  function onEscKey(e) {
    if (e.key === 'Escape') hideFullscreen();
  }

  fsBtn.addEventListener('click', showFullscreen);
  if (closeBtn) closeBtn.addEventListener('click', e => { e.stopPropagation(); hideFullscreen(); });
  overlay.addEventListener('click', hideFullscreen);
})();

// ─── Random mode button ────────────────────────────────────────────────────────
(function initRandomModeBtn() {
  const btn = document.getElementById('random-mode-btn');
  if (!btn) return;
  btn.addEventListener('click', async () => {
    try {
      const data = await apiJson('POST', '/api/mode/random');
      log(`Random mode → ${data.mode}`);
      setStatusMsg(quickStatus, `🎲 Switched to: ${data.mode}`, true);
      updateActiveModeButton(data.mode);
      await fetchStatus();
    } catch (e) {
      setStatusMsg(quickStatus, `Random mode error: ${e.message}`, false);
    }
  });
})();

// ─── USB Reconnect button ──────────────────────────────────────────────────────
(function initReconnectUsbBtn() {
  const btn = document.getElementById('reconnect-usb-btn');
  if (!btn) return;
  btn.addEventListener('click', async () => {
    try {
      btn.disabled = true;
      btn.textContent = '⏳ Reconnecting…';
      const data = await apiJson('POST', '/api/panel/reconnect');
      log(`USB reconnect: ${data.message || 'ok'}`);
      setStatusMsg(quickStatus, '✓ Reconnect triggered — check status dot', true);
      // Poll for status change
      setTimeout(fetchStatus, 3000);
      setTimeout(fetchStatus, 6000);
    } catch (e) {
      log(`Reconnect failed: ${e.message}`, 'err');
      setStatusMsg(quickStatus, `Reconnect error: ${e.message}`, false);
    } finally {
      btn.disabled = false;
      btn.textContent = '🔌 Reconnect';
    }
  });
})();

// ─── Boot ─────────────────────────────────────────────────────────────────────
initSSE();
startPolling();
setInterval(fetchDocker,   60000);
setInterval(fetchSessions, 30000);
log('ttlcd web UI connected');


// ─── Keyboard shortcuts for new modes (Shift+N, Shift+B) ─────────────────────
(function patchNewModeShortcuts() {
  // These are additive patches; the main keydown handler already handles Shift+T
  // We re-listen here to keep code modular
  document.addEventListener('keydown', e => {
    if (document.activeElement.tagName === 'INPUT' ||
        document.activeElement.tagName === 'TEXTAREA' ||
        document.activeElement.tagName === 'SELECT') return;
    if (!e.shiftKey) return;
    switch (e.key) {
      case 'A':
        api('POST', '/api/mode', { mode: 'analog_clock' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'X':
        api('POST', '/api/mode', { mode: 'led_clock' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'E':
        api('POST', '/api/mode', { mode: 'game_of_life' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'Z':
        api('POST', '/api/mode', { mode: 'status_overview' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'G':
        api('POST', '/api/mode', { mode: 'ping_graph' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'N':
        api('POST', '/api/mode', { mode: 'net_speed_graph' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'B':
        api('POST', '/api/mode', { mode: 'cpu_core_bars' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'V':
        api('POST', '/api/mode', { mode: 'speedometer_gauge' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'P':
        api('POST', '/api/mode', { mode: 'disk_pie' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'H':
        api('POST', '/api/mode', { mode: 'thermal_dashboard' }).then(() => fetchStatus()).catch(() => {});
        break;
      case 'R':
        apiJson('POST', '/api/mode/random').then(d => {
          log(`Random → ${d.mode}`);
          updateActiveModeButton(d.mode);
          fetchStatus();
        }).catch(() => {});
        break;
    }
  });
})();

// ─── Rotation mode picker (visual toggle buttons) ─────────────────────────────
(function initRotationPicker() {
  const picker  = document.getElementById('rot-mode-picker');
  const rotInput = document.getElementById('rot-modes');
  const selPreview = document.getElementById('rot-selected-preview');
  const selList    = document.getElementById('rot-selected-list');
  if (!picker || !rotInput) return;

  // Build toggle buttons from the mode grid
  const modeGrid  = document.getElementById('mode-grid');
  if (!modeGrid) return;

  const allBtns = Array.from(modeGrid.querySelectorAll('.mode-btn'));
  let selected = new Set();

  // Restore last selection from localStorage
  try {
    const saved = JSON.parse(localStorage.getItem('ttlcd-rot-modes') || '[]');
    saved.forEach(m => selected.add(m));
  } catch (_) {}

  // Parse from rot-modes input on load
  function parseInput() {
    const modes = (rotInput.value || '')
      .split(',').map(s => s.trim()).filter(Boolean);
    selected = new Set(modes);
    syncUI();
  }

  function syncUI() {
    // Update picker button states
    picker.querySelectorAll('.rot-pick-btn').forEach(b => {
      b.classList.toggle('rot-selected', selected.has(b.dataset.mode));
    });
    // Update text input
    rotInput.value = Array.from(selected).join(', ');
    // Preview
    if (selected.size > 0) {
      selList.textContent = Array.from(selected).join(' → ');
      selPreview.style.display = '';
    } else {
      selPreview.style.display = 'none';
    }
    // Persist
    try { localStorage.setItem('ttlcd-rot-modes', JSON.stringify(Array.from(selected))); } catch (_) {}
  }

  // Populate picker
  allBtns.forEach(btn => {
    const mode = btn.dataset.mode;
    if (!mode || mode === 'idle') return;
    const tb = document.createElement('button');
    tb.className = 'rot-pick-btn';
    tb.dataset.mode = mode;
    tb.title = `Toggle ${mode} in rotation`;
    // Use emoji + short label from the main button
    const span = btn.querySelector('span');
    const emoji = btn.textContent.replace(span ? span.textContent : '', '').trim();
    tb.innerHTML = `<span class="rpb-emoji">${emoji}</span><span class="rpb-label">${span ? span.textContent : mode}</span>`;
    tb.addEventListener('click', () => {
      if (selected.has(mode)) selected.delete(mode); else selected.add(mode);
      syncUI();
    });
    picker.appendChild(tb);
  });

  // Sync text input → selected set when user types in the input
  rotInput.addEventListener('change', parseInput);

  // Initial sync
  if (rotInput.value) parseInput(); else syncUI();

  // After rotation starts, reflect server state into picker
  const rotStartBtn = document.getElementById('rot-start-btn');
  if (rotStartBtn) {
    rotStartBtn.addEventListener('click', () => {
      // Re-read input in case user typed
      parseInput();
    });
  }
})();

// ─── Mode Favorites / Pinning ─────────────────────────────────────────────────
(function initModeFavorites() {
  const STORAGE_KEY = 'ttlcd-fav-modes';
  const modeGrid    = document.getElementById('mode-grid');
  if (!modeGrid) return;

  let favs = new Set();
  try { favs = new Set(JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]')); } catch (_) {}

  function saveFavs() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(Array.from(favs))); } catch (_) {}
  }

  // Add a ☆ star button to each mode-btn
  document.querySelectorAll('.mode-btn[data-mode]').forEach(btn => {
    const mode = btn.dataset.mode;
    if (!mode) return;
    const star = document.createElement('span');
    star.className = 'fav-star';
    star.title = 'Pin to favourites';
    star.textContent = favs.has(mode) ? '★' : '☆';
    star.setAttribute('role', 'button');
    star.setAttribute('aria-label', `Favourite ${mode}`);
    star.addEventListener('click', e => {
      e.stopPropagation();
      if (favs.has(mode)) { favs.delete(mode); star.textContent = '☆'; }
      else                { favs.add(mode);    star.textContent = '★'; }
      saveFavs();
      renderFavBand();
    });
    btn.appendChild(star);
  });

  // Render a "Pinned" band above the mode grid
  let favBand = document.getElementById('fav-modes-band');
  if (!favBand) {
    favBand = document.createElement('div');
    favBand.id = 'fav-modes-band';
    favBand.className = 'fav-band';
    const header = modeGrid.closest('.card')?.querySelector('.section-header-row');
    if (header) header.after(favBand);
    else modeGrid.before(favBand);
  }

  function renderFavBand() {
    if (favs.size === 0) {
      favBand.innerHTML = '';
      favBand.style.display = 'none';
      return;
    }
    favBand.style.display = 'flex';
    favBand.innerHTML = '<span class="fav-band-label">★ Pinned:</span>';
    favs.forEach(mode => {
      // Find original button
      const orig = modeGrid.querySelector(`[data-mode="${mode}"]`);
      if (!orig) return;
      const btn = document.createElement('button');
      btn.className = 'mode-btn fav-btn';
      btn.dataset.mode = mode;
      btn.title = orig.title || mode;
      btn.innerHTML = orig.innerHTML;  // copy emoji + label
      btn.addEventListener('click', async () => {
        try {
          await api('POST', '/api/mode', { mode });
          log(`Mode → ${mode}`);
          updateActiveModeButton(mode);
        } catch (e) { log(`Mode change failed: ${e.message}`, 'err'); }
      });
      favBand.appendChild(btn);
    });
  }

  renderFavBand();
})();


// ─── Mode button preview on hover ────────────────────────────────────────────
(function initModeHoverPreview() {
  const modeGrid = document.getElementById('mode-grid');
  if (!modeGrid) return;

  // Build a floating tooltip element
  const tip = document.createElement('div');
  tip.id = 'mode-hover-preview';
  tip.style.cssText = [
    'position:fixed','z-index:9999','display:none','pointer-events:none',
    'background:var(--surface0)','border:1px solid var(--surface2)',
    'border-radius:8px','padding:4px','box-shadow:0 4px 20px rgba(0,0,0,0.5)',
  ].join(';');
  const tipImg = document.createElement('img');
  tipImg.width = 160; tipImg.height = 160;
  tipImg.style.cssText = 'display:block;border-radius:4px;image-rendering:pixelated';
  tip.appendChild(tipImg);
  document.body.appendChild(tip);

  // Cache to avoid re-fetching during the same hover
  const cache = new Map();
  let hideTimer = null;
  let currentMode = null;

  async function showPreview(mode, anchorEl) {
    if (currentMode === mode) return;
    currentMode = mode;
    tip.style.display = 'block';
    tipImg.src = '';

    // Position the tooltip next to the button
    const rect = anchorEl.getBoundingClientRect();
    const tx = rect.right + 8;
    const ty = Math.max(8, rect.top - 60);
    tip.style.left = `${Math.min(tx, window.innerWidth - 180)}px`;
    tip.style.top  = `${ty}px`;

    // Fetch from cache or API
    if (cache.has(mode)) {
      tipImg.src = cache.get(mode);
      return;
    }
    try {
      const resp = await fetch(`/api/render/${encodeURIComponent(mode)}?quality=60`);
      if (resp.ok) {
        const blob = await resp.blob();
        const url  = URL.createObjectURL(blob);
        cache.set(mode, url);
        // Only display if still the same mode being hovered
        if (currentMode === mode) tipImg.src = url;
      }
    } catch (_) { /* silent */ }
  }

  function hidePreview() {
    tip.style.display = 'none';
    currentMode = null;
  }

  // Only load preview on hover-and-hold (100ms delay to avoid spamming)
  modeGrid.addEventListener('mouseover', e => {
    const btn = e.target.closest('.mode-btn[data-mode]');
    if (!btn) return;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => showPreview(btn.dataset.mode, btn), 120);
  });

  modeGrid.addEventListener('mouseout', e => {
    const btn = e.target.closest('.mode-btn[data-mode]');
    if (!btn) return;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(hidePreview, 80);
  });
})();

// ─── History export button ────────────────────────────────────────────────────
(function initHistoryExport() {
  // Add an Export button to the history section if it exists
  const histSection = document.getElementById('history-list')?.closest('.card');
  if (!histSection) return;
  const existingBtn = histSection.querySelector('.hist-export-btn');
  if (existingBtn) return; // Already added

  const exportBtn = document.createElement('button');
  exportBtn.className = 'btn-secondary hist-export-btn';
  exportBtn.textContent = '⬇ Export';
  exportBtn.title = 'Download history as JSON';
  exportBtn.style.cssText = 'font-size:0.75rem;padding:0.2rem 0.5rem;margin-left:0.5rem';

  exportBtn.addEventListener('click', async () => {
    try {
      const data = await apiJson('GET', '/api/history');
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement('a');
      a.href     = url;
      a.download = `ttlcd-history-${new Date().toISOString().slice(0,10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      alert('Export failed: ' + e.message);
    }
  });

  // Find a good place to insert it
  const h2 = histSection.querySelector('h2');
  if (h2) h2.appendChild(exportBtn);
})();

// ─── Favorites System ─────────────────────────────────────────────────────────
(function () {
  const STORAGE_KEY = 'ttlcd_favorites';

  function loadFavorites() {
    try { return new Set(JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]')); }
    catch { return new Set(); }
  }

  function saveFavorites(favSet) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...favSet]));
  }

  let favorites = loadFavorites();

  // Add a "⭐ Favorites" category button after the "All" button
  const catBar = document.getElementById('mode-categories');
  if (catBar) {
    const favBtn = document.createElement('button');
    favBtn.className = 'cat-btn';
    favBtn.dataset.cat = 'favorites';
    favBtn.textContent = '⭐ Favorites';
    catBar.insertBefore(favBtn, catBar.children[1]);
  }

  function applyFavoriteStar(btn) {
    const mode = btn.dataset.mode;
    if (!mode) return;
    // Remove old star if present
    const old = btn.querySelector('.fav-star');
    if (old) old.remove();
    // Add star span
    const star = document.createElement('span');
    star.className = 'fav-star';
    star.textContent = favorites.has(mode) ? '★' : '☆';
    star.title = favorites.has(mode) ? 'Remove from favorites' : 'Add to favorites';
    star.addEventListener('click', e => {
      e.stopPropagation();
      if (favorites.has(mode)) {
        favorites.delete(mode);
      } else {
        favorites.add(mode);
      }
      saveFavorites(favorites);
      applyFavoriteStar(btn);
      // If currently in favorites filter, re-apply
      const activeCat = document.querySelector('.cat-btn.active');
      if (activeCat && activeCat.dataset.cat === 'favorites') {
        applyFavoritesFilter();
      }
    });
    btn.appendChild(star);
  }

  function applyFavoritesFilter() {
    const btns = document.querySelectorAll('#mode-grid .mode-btn');
    btns.forEach(btn => {
      const inFav = favorites.has(btn.dataset.mode || '');
      btn.classList.toggle('hidden-by-filter', !inFav);
    });
    const countEl = document.getElementById('mode-filter-count');
    if (countEl) countEl.textContent = `${favorites.size} favorite${favorites.size !== 1 ? 's' : ''}`;
  }

  // Decorate all mode buttons with star
  function decorateAll() {
    document.querySelectorAll('#mode-grid .mode-btn').forEach(applyFavoriteStar);
  }
  decorateAll();

  // Intercept cat-btn click for "favorites"
  document.querySelectorAll('.cat-btn').forEach(btn => {
    if (btn.dataset.cat !== 'favorites') return;
    btn.addEventListener('click', () => {
      document.querySelectorAll('.cat-btn').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      applyFavoritesFilter();
    });
  });

  // After any dynamic injection of mode buttons, re-decorate
  const grid = document.getElementById('mode-grid');
  if (grid) {
    new MutationObserver(() => decorateAll()).observe(grid, { childList: true, subtree: false });
  }
})();


// ─── Recent Modes Tracker ─────────────────────────────────────────────────────
(function () {
  const STORAGE_KEY = 'ttlcd_recent_modes';
  const MAX_RECENT  = 8;

  function loadRecent() {
    try { return JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]'); }
    catch { return []; }
  }

  function saveRecent(arr) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(arr.slice(0, MAX_RECENT)));
  }

  function pushRecent(mode) {
    const arr = loadRecent().filter(m => m !== mode);
    arr.unshift(mode);
    saveRecent(arr);
    renderRecentBar();
  }

  function renderRecentBar() {
    const bar = document.getElementById('recent-modes-bar');
    if (!bar) return;
    const recent = loadRecent();
    bar.innerHTML = '';
    if (recent.length === 0) {
      bar.textContent = 'No recent modes yet';
      return;
    }
    recent.forEach(mode => {
      const chip = document.createElement('button');
      chip.className = 'recent-chip';
      chip.textContent = mode.replace(/_/g, ' ');
      chip.title = `Switch to ${mode}`;
      chip.addEventListener('click', () => {
        apiJson('POST', '/api/mode', { mode }).then(() => fetchStatus()).catch(() => {});
        pushRecent(mode);
      });
      bar.appendChild(chip);
    });
  }

  // Intercept mode button clicks to record recent mode
  document.addEventListener('click', e => {
    const btn = e.target.closest('.mode-btn');
    if (btn && btn.dataset.mode) {
      pushRecent(btn.dataset.mode);
    }
  });

  renderRecentBar();
})();


// ─── Flash Message ────────────────────────────────────────────────────────────
(function () {
  const btn    = document.getElementById('flash-send-btn');
  const txtEl  = document.getElementById('flash-text');
  const durEl  = document.getElementById('flash-duration');
  const colEl  = document.getElementById('flash-color');
  const status = document.getElementById('flash-status');
  if (!btn) return;

  btn.addEventListener('click', async () => {
    const text     = (txtEl?.value || '').trim();
    const duration = parseInt(durEl?.value || '10', 10);
    const color    = colEl?.value || '#89b4fa';
    if (!text) { if (status) { status.textContent = 'Enter a message first'; status.style.color = 'var(--red)'; } return; }

    btn.disabled = true;
    if (status) { status.textContent = 'Sending…'; status.style.color = 'var(--overlay0)'; }
    try {
      await apiJson('POST', '/api/flash', { text, duration, color });
      if (status) { status.textContent = `✓ Showing for ${duration}s`; status.style.color = 'var(--green)'; }
    } catch (e) {
      if (status) { status.textContent = 'Error: ' + e.message; status.style.color = 'var(--red)'; }
    } finally {
      btn.disabled = false;
      setTimeout(() => { if (status) status.textContent = ''; }, 4000);
    }
  });
})();


// ─── Keyboard Shortcuts Help Overlay ─────────────────────────────────────────
(function () {
  // Build overlay on first press of '?'
  let overlayEl = null;

  function buildOverlay() {
    const el = document.createElement('div');
    el.id = 'shortcuts-overlay';
    el.innerHTML = `
      <div class="shortcuts-card">
        <div class="shortcuts-header">
          <h2 style="margin:0">⌨ Keyboard Shortcuts</h2>
          <button id="shortcuts-close-btn" class="icon-btn" title="Close">✕</button>
        </div>
        <table class="shortcuts-table">
          <thead><tr><th>Key</th><th>Action</th></tr></thead>
          <tbody>
            <tr><td><kbd>/</kbd></td><td>Focus mode search</td></tr>
            <tr><td><kbd>Esc</kbd></td><td>Clear search / close overlay</td></tr>
            <tr><td><kbd>?</kbd></td><td>Toggle shortcuts overlay</td></tr>
            <tr><td><kbd>F</kbd></td><td>Freeze / unfreeze display</td></tr>
            <tr><td><kbd>Shift+R</kbd></td><td>Random mode</td></tr>
            <tr><td><kbd>Shift+V</kbd></td><td>Speedometer gauge</td></tr>
            <tr><td><kbd>Shift+P</kbd></td><td>Disk pie</td></tr>
            <tr><td><kbd>Shift+H</kbd></td><td>Thermal dashboard</td></tr>
            <tr><td><kbd>Space</kbd></td><td>Refresh preview</td></tr>
          </tbody>
        </table>
      </div>
    `;
    el.addEventListener('click', e => { if (e.target === el) closeOverlay(); });
    document.body.appendChild(el);
    document.getElementById('shortcuts-close-btn')?.addEventListener('click', closeOverlay);
    return el;
  }

  function openOverlay() {
    if (!overlayEl) overlayEl = buildOverlay();
    overlayEl.classList.add('visible');
  }
  function closeOverlay() {
    overlayEl?.classList.remove('visible');
  }

  document.addEventListener('keydown', e => {
    if (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'TEXTAREA') return;
    if (e.key === '?') { e.preventDefault(); overlayEl?.classList.contains('visible') ? closeOverlay() : openOverlay(); }
    if (e.key === 'Escape' && overlayEl?.classList.contains('visible')) closeOverlay();
  });
})();

