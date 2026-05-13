/* ttlcd web UI — app.js */
'use strict';

// ── State ─────────────────────────────────────────────────────────────────────
let currentFile  = null;
let previewToken = 0; // incremented when a new frame might be available

// ── DOM refs ──────────────────────────────────────────────────────────────────
const currentModeEl    = document.getElementById('current-mode');
const modeBtns         = document.querySelectorAll('.mode-btn');
const dropZone         = document.getElementById('drop-zone');
const fileInput        = document.getElementById('file-input');
const previewWrap      = document.getElementById('preview-wrap');
const previewEl        = document.getElementById('preview');
const sendBtn          = document.getElementById('send-btn');
const cancelBtn        = document.getElementById('cancel-btn');
const uploadStatusEl   = document.getElementById('upload-status');
const logEl            = document.getElementById('log');
const infoVersion      = document.getElementById('info-version');
const infoMode         = document.getElementById('info-mode');
const infoRotate       = document.getElementById('info-rotate');
const infoHeadless     = document.getElementById('info-headless');
const infoHost         = document.getElementById('info-host');
const hdrVersion       = document.getElementById('hdr-version');
const panelPreviewImg  = document.getElementById('panel-preview-img');
const panelPreviewEmpty= document.getElementById('panel-preview-empty');

// Live stat elements
const statCpu    = document.getElementById('stat-cpu');
const statRam    = document.getElementById('stat-ram');
const statLoad   = document.getElementById('stat-load');
const statUptime = document.getElementById('stat-uptime');
const barCpu     = document.getElementById('bar-cpu');
const barRam     = document.getElementById('bar-ram');


// Text form refs
const txtTitle    = document.getElementById('txt-title');
const txtSubtitle = document.getElementById('txt-subtitle');
const txtLines    = document.getElementById('txt-lines');
const txtProgress = document.getElementById('txt-progress');
const txtColor    = document.getElementById('txt-color');
const txtShowTime = document.getElementById('txt-show-time');
const txtSendBtn  = document.getElementById('txt-send-btn');
const txtStatus   = document.getElementById('txt-status');
const progPctLbl  = document.getElementById('prog-pct-label');

// ── Logging ───────────────────────────────────────────────────────────────────
function logEntry(msg, level = 'info') {
  const now = new Date();
  const ts  = now.toTimeString().slice(0, 8);
  const li  = document.createElement('li');
  li.className = level;
  li.innerHTML = `<span class="ts">${ts}</span><span class="msg">${escHtml(msg)}</span>`;
  logEl.prepend(li);
  if (logEl.children.length > 60) logEl.lastChild.remove();
}

function escHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

// ── Helpers ───────────────────────────────────────────────────────────────────
function fmtBytes(bytes) {
  if (bytes >= 1_073_741_824) return (bytes / 1_073_741_824).toFixed(1) + ' GB';
  if (bytes >= 1_048_576)     return (bytes / 1_048_576).toFixed(1)     + ' MB';
  if (bytes >= 1024)          return (bytes / 1024).toFixed(1)          + ' KB';
  return bytes + ' B';
}

function fmtUptime(secs) {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m ${secs % 60}s`;
}

// ── Status polling ────────────────────────────────────────────────────────────
async function fetchStatus() {
  try {
    const r = await fetch('/api/status');
    if (!r.ok) return;
    const data = await r.json();

    const ver = data.version ?? '—';
    currentModeEl.textContent  = data.mode ?? '—';
    infoVersion.textContent    = ver;
    infoMode.textContent       = data.mode ?? '—';
    infoRotate.textContent     = data.rotate_deg != null ? `${data.rotate_deg}°` : '0°';
    infoHeadless.textContent   = data.headless ? 'yes (USB skipped)' : 'no';
    hdrVersion.textContent     = `v${ver}`;

    modeBtns.forEach(btn => {
      btn.classList.toggle('active', btn.dataset.mode === data.mode);
    });
  } catch (_) { /* busy */ }
}

async function fetchSysinfo() {
  try {
    const r = await fetch('/api/sysinfo');
    if (!r.ok) return;
    const d = await r.json();

    infoHost.textContent = d.hostname || '—';

    statCpu.textContent = d.cpu_pct.toFixed(1) + '%';
    barCpu.style.width  = d.cpu_pct.toFixed(1) + '%';
    barCpu.style.background =
      d.cpu_pct > 80 ? 'var(--red)' :
      d.cpu_pct > 50 ? 'var(--yellow)' : 'var(--blue)';

    const memPct = d.mem_total > 0 ? (d.mem_used / d.mem_total * 100) : 0;
    statRam.textContent = `${fmtBytes(d.mem_used)} / ${fmtBytes(d.mem_total)}`;
    barRam.style.width  = memPct.toFixed(1) + '%';
    barRam.style.background =
      memPct > 85 ? 'var(--red)' :
      memPct > 65 ? 'var(--yellow)' : 'var(--green)';

    statLoad.textContent  = `${d.load1.toFixed(2)}  ${d.load5.toFixed(2)}  ${d.load15.toFixed(2)}`;
    statUptime.textContent = fmtUptime(d.uptime);
  } catch (_) { /* busy */ }
}

async function fetchPreview() {
  try {
    const token = ++previewToken;
    const r = await fetch('/api/preview');
    if (r.status === 204 || !r.ok) return; // no frame yet
    if (token !== previewToken) return;     // superseded

    const blob = await r.blob();
    if (token !== previewToken) return;

    const url = URL.createObjectURL(blob);
    const old = panelPreviewImg.src;
    panelPreviewImg.src = url;
    panelPreviewImg.classList.add('visible');
    panelPreviewEmpty.style.display = 'none';
    if (old.startsWith('blob:')) URL.revokeObjectURL(old);
  } catch (_) { /* busy */ }
}

setInterval(fetchStatus,  2000);
setInterval(fetchSysinfo, 3000);
setInterval(fetchPreview, 3000);

fetchStatus();
fetchSysinfo();
setTimeout(fetchPreview, 800); // slight delay so first frame is ready

// ── Mode switching ────────────────────────────────────────────────────────────
modeBtns.forEach(btn => {
  btn.addEventListener('click', async () => {
    const mode = btn.dataset.mode;
    logEntry(`Requesting mode: ${mode}`);
    try {
      const r = await fetch('/api/mode', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mode }),
      });
      if (r.ok) {
        logEntry(`Mode → ${mode}`, 'ok');
        fetchStatus();
        // Slight delay then refresh preview so new mode's first frame appears
        setTimeout(fetchPreview, 3500);
      } else {
        logEntry(`Mode change failed: ${await r.text()}`, 'err');
      }
    } catch (e) {
      logEntry(`Mode change error: ${e.message}`, 'err');
    }
  });
});

// ── File selection (click) ────────────────────────────────────────────────────
dropZone.addEventListener('click', () => fileInput.click());

fileInput.addEventListener('change', e => {
  const f = e.target.files[0];
  if (f) setFile(f);
});

// ── Drag & drop ───────────────────────────────────────────────────────────────
dropZone.addEventListener('dragover', e => {
  e.preventDefault();
  dropZone.classList.add('drag-over');
});
['dragleave', 'dragend'].forEach(ev =>
  dropZone.addEventListener(ev, () => dropZone.classList.remove('drag-over'))
);
dropZone.addEventListener('drop', e => {
  e.preventDefault();
  dropZone.classList.remove('drag-over');
  const f = e.dataTransfer.files[0];
  if (f && f.type.startsWith('image/')) setFile(f);
  else logEntry('Dropped file is not an image', 'err');
});

// ── Preview ───────────────────────────────────────────────────────────────────
function setFile(file) {
  currentFile = file;
  const url = URL.createObjectURL(file);
  previewEl.src = url;
  previewEl.onload = () => URL.revokeObjectURL(url);
  previewWrap.classList.remove('hidden');
  dropZone.style.display = 'none';
  uploadStatusEl.textContent = '';
  logEntry(`Selected: ${file.name} (${(file.size / 1024).toFixed(1)} KB)`);
}

cancelBtn.addEventListener('click', () => {
  currentFile = null;
  fileInput.value = '';
  previewWrap.classList.add('hidden');
  dropZone.style.display = '';
  uploadStatusEl.textContent = '';
});

// ── Send image ────────────────────────────────────────────────────────────────
sendBtn.addEventListener('click', async () => {
  if (!currentFile) return;

  sendBtn.disabled = true;
  uploadStatusEl.textContent = '⏳ Sending…';
  uploadStatusEl.className = '';

  const fd = new FormData();
  fd.append('image', currentFile);

  try {
    const r = await fetch('/api/image', { method: 'POST', body: fd });
    if (r.ok) {
      uploadStatusEl.textContent = '✓ Image sent to panel';
      uploadStatusEl.className   = 'status-ok';
      logEntry(`Image sent: ${currentFile.name}`, 'ok');
      fetchStatus();
      setTimeout(fetchPreview, 3000);
    } else {
      const err = await r.text();
      uploadStatusEl.textContent = `✗ ${err}`;
      uploadStatusEl.className   = 'status-err';
      logEntry(`Upload failed: ${err}`, 'err');
    }
  } catch (e) {
    uploadStatusEl.textContent = `✗ Network error: ${e.message}`;
    uploadStatusEl.className   = 'status-err';
    logEntry(`Upload error: ${e.message}`, 'err');
  } finally {
    sendBtn.disabled = false;
  }
});

// ── Keyboard: Enter → send ────────────────────────────────────────────────────
document.addEventListener('keydown', e => {
  if (e.key === 'Enter' && currentFile && !sendBtn.disabled) sendBtn.click();
});


// ── Custom text form ─────────────────────────────────────────────────────────

// Live progress percentage label
txtProgress.addEventListener('input', () => {
  const v = parseInt(txtProgress.value, 10);
  progPctLbl.textContent = v < 0 ? '(off)' : `${v}%`;
});
progPctLbl.textContent = '(off)';

txtSendBtn.addEventListener('click', async () => {
  const title    = txtTitle.value.trim();
  const subtitle = txtSubtitle.value.trim();
  const lines    = txtLines.value.split('\n').map(l => l.trim()).filter(Boolean);
  const progVal  = parseInt(txtProgress.value, 10);
  const color    = txtColor.value;
  const showTime = txtShowTime.checked;

  if (!title && !subtitle && lines.length === 0) {
    txtStatus.textContent = 'Enter at least a title or some lines.';
    txtStatus.className   = 'status-err';
    return;
  }

  const payload = { color, show_time: showTime };
  if (title)          payload.title    = title;
  if (subtitle)       payload.subtitle = subtitle;
  if (lines.length)   payload.lines    = lines;
  if (progVal >= 0)   payload.progress = progVal / 100;

  txtSendBtn.disabled = true;
  txtStatus.textContent = '⏳ Sending…';
  txtStatus.className   = '';

  try {
    const r = await fetch('/api/text', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    if (r.ok) {
      txtStatus.textContent = '✓ Text sent to panel';
      txtStatus.className   = 'status-ok';
      logEntry(`Text sent: "${title || '(no title)'}"`, 'ok');
      fetchStatus();
      setTimeout(fetchPreview, 3000);
    } else {
      const err = await r.text();
      txtStatus.textContent = `✗ ${err}`;
      txtStatus.className   = 'status-err';
      logEntry(`Text send failed: ${err}`, 'err');
    }
  } catch (e) {
    txtStatus.textContent = `✗ ${e.message}`;
    txtStatus.className   = 'status-err';
    logEntry(`Text send error: ${e.message}`, 'err');
  } finally {
    txtSendBtn.disabled = false;
  }
});

// ── Init log ──────────────────────────────────────────────────────────────────
logEntry('ttlcd UI ready', 'ok');
