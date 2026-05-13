/* ttlcd web UI — app.js */
'use strict';

// ── State ─────────────────────────────────────────────────────────────────────
let currentFile      = null;
let previewToken     = 0;
let lastFrameSeq     = null;  // null = haven't received status yet
let lastPreviewedSeq = -2;    // -2 = never fetched preview

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
const infoQuality      = document.getElementById('info-quality');
const infoHeadless     = document.getElementById('info-headless');
const infoHost         = document.getElementById('info-host');
const hdrVersion       = document.getElementById('hdr-version');
const panelPreviewImg  = document.getElementById('panel-preview-img');
const panelPreviewEmpty= document.getElementById('panel-preview-empty');
const returnBtn        = document.getElementById('return-btn');
const logClearBtn      = document.getElementById('log-clear-btn');

// Live stat elements
const statCpu       = document.getElementById('stat-cpu');
const statRam       = document.getElementById('stat-ram');
const statLoad      = document.getElementById('stat-load');
const statUptime    = document.getElementById('stat-uptime');
const statTempBlock = document.getElementById('stat-temp-block');
const statTemp      = document.getElementById('stat-temp');
const barCpu        = document.getElementById('bar-cpu');
const barRam        = document.getElementById('bar-ram');
const barTemp       = document.getElementById('bar-temp');

// Text form refs
const txtTitle    = document.getElementById('txt-title');
const txtSubtitle = document.getElementById('txt-subtitle');
const txtLines    = document.getElementById('txt-lines');
const txtProgress = document.getElementById('txt-progress');
const txtColor    = document.getElementById('txt-color');
const txtShowTime = document.getElementById('txt-show-time');
const txtDuration    = document.getElementById('txt-duration');
const txtSendBtn     = document.getElementById('txt-send-btn');
const txtPreviewBtn  = document.getElementById('txt-preview-btn');
const txtPreviewWrap = document.getElementById('txt-preview-wrap');
const txtPreviewImg  = document.getElementById('txt-preview-img');
const txtStatus      = document.getElementById('txt-status');
const progPctLbl     = document.getElementById('prog-pct-label');

// Preset refs
const presetSelect    = document.getElementById('preset-select');
const presetApplyBtn  = document.getElementById('preset-apply-btn');
const presetSaveBtn   = document.getElementById('preset-save-btn');
const presetDelBtn    = document.getElementById('preset-del-btn');
const presetClearBtn  = document.getElementById('preset-clear-btn');

// Quick notify refs
const quickStatus = document.getElementById('quick-status');

// Slideshow refs
const ssDir      = document.getElementById('ss-dir');
const ssInterval = document.getElementById('ss-interval');
const ssStartBtn = document.getElementById('ss-start-btn');
const ssPrevBtn  = document.getElementById('ss-prev-btn');
const ssNextBtn  = document.getElementById('ss-next-btn');
const ssStatus   = document.getElementById('ss-status');

// Rotation refs
const rotModes        = document.getElementById('rot-modes');
const rotInterval     = document.getElementById('rot-interval');
const rotStartBtn     = document.getElementById('rot-start-btn');
const rotStopBtn      = document.getElementById('rot-stop-btn');
const rotStatus       = document.getElementById('rot-status');
const rotIndicator    = document.getElementById('rot-indicator');
const rotModeList     = document.getElementById('rot-mode-list');
const rotIntervalDisp = document.getElementById('rot-interval-display');

// Panel settings refs
const setQuality   = document.getElementById('set-quality');
const setQualityLbl= document.getElementById('set-quality-lbl');
const setOrient    = document.getElementById('set-orient');
const setApplyBtn  = document.getElementById('set-apply-btn');
const setStatus    = document.getElementById('set-status');


// Schedule refs
const schedInSecs = document.getElementById('sched-in-secs');
const schedLabel  = document.getElementById('sched-label');
const schedBtn    = document.getElementById('sched-btn');
const schedStatus = document.getElementById('sched-status');

// Countdown refs
const cdSeconds  = document.getElementById('cd-seconds');
const cdLabel    = document.getElementById('cd-label');
const cdStartBtn = document.getElementById('cd-start-btn');
const cdCancelBtn= document.getElementById('cd-cancel-btn');
const cdStatus   = document.getElementById('cd-status');

// ── Logging ───────────────────────────────────────────────────────────────────
function logEntry(msg, level = 'info') {
  const now = new Date();
  const ts  = now.toTimeString().slice(0, 8);
  const li  = document.createElement('li');
  li.className = level;
  li.innerHTML = `<span class="ts">${ts}</span><span class="msg">${escHtml(msg)}</span>`;
  logEl.prepend(li);
  if (logEl.children.length > 80) logEl.lastChild.remove();
}

logClearBtn.addEventListener('click', () => { logEl.innerHTML = ''; logEntry('Log cleared'); });

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
    infoQuality.textContent    = data.quality != null ? `${data.quality}%` : '—';
    infoHeadless.textContent   = data.headless ? 'yes (USB skipped)' : 'no';
    hdrVersion.textContent     = `v${ver}`;

    // Sync the settings sliders/selects with current daemon state
    if (data.quality != null) {
      setQuality.value       = data.quality;
      setQualityLbl.textContent = `${data.quality}%`;
    }
    if (data.rotate_deg != null) {
      setOrient.value = String(data.rotate_deg);
    }

    modeBtns.forEach(btn => {
      btn.classList.toggle('active', btn.dataset.mode === data.mode);
    });

    // Return button for transient modes
    returnBtn.style.display = data.can_clear ? '' : 'none';

    // Rotation indicator
    updateRotationIndicator(data.rotation_modes ?? [], data.rotation_interval ?? 0, data.mode);

    // Smarter preview: only fetch when a new frame is available
    const newSeq = data.frame_seq ?? null;
    if (newSeq !== lastFrameSeq) {
      lastFrameSeq = newSeq;
      fetchPreview();
    }
  } catch (_) { /* busy */ }
}

function updateRotationIndicator(modes, interval, currentMode) {
  if (interval > 0 && modes.length > 0) {
    rotIndicator.style.display  = '';
    rotIntervalDisp.textContent = interval;

    // Render mode chips
    rotModeList.innerHTML = '';
    modes.forEach(m => {
      const chip = document.createElement('span');
      chip.className = 'rot-chip' + (m === currentMode ? ' active-rot' : '');
      chip.textContent = m.replace(/_/g, ' ');
      rotModeList.appendChild(chip);
    });
  } else {
    rotIndicator.style.display = 'none';
  }
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

    statLoad.textContent   = `${d.load1.toFixed(2)}  ${d.load5.toFixed(2)}  ${d.load15.toFixed(2)}`;
    statUptime.textContent = fmtUptime(d.uptime);

    if (d.cpu_temp != null) {
      statTempBlock.style.display = '';
      statTemp.textContent = d.cpu_temp.toFixed(1) + '°C';
      const tempPct = Math.min(d.cpu_temp, 100);
      barTemp.style.width = tempPct.toFixed(1) + '%';
      barTemp.style.background =
        d.cpu_temp > 80 ? 'var(--red)' :
        d.cpu_temp > 65 ? 'var(--yellow)' : 'var(--green)';
      statTemp.style.color =
        d.cpu_temp > 80 ? 'var(--red)' :
        d.cpu_temp > 65 ? 'var(--yellow)' : 'var(--green)';
    } else {
      statTempBlock.style.display = 'none';
    }
  } catch (_) { /* busy */ }
}

async function fetchPreview() {
  if (lastFrameSeq !== null && lastPreviewedSeq === lastFrameSeq) return;
  const token = ++previewToken;
  try {
    const r = await fetch('/api/preview');
    if (r.status === 204 || !r.ok) return;
    if (token !== previewToken) return;
    const blob = await r.blob();
    if (token !== previewToken) return;
    const url = URL.createObjectURL(blob);
    const old = panelPreviewImg.src;
    panelPreviewImg.src = url;
    panelPreviewImg.classList.add('visible');
    panelPreviewEmpty.style.display = 'none';
    if (old && old.startsWith('blob:')) URL.revokeObjectURL(old);
    lastPreviewedSeq = lastFrameSeq;
  } catch (_) { /* busy */ }
}

setInterval(fetchStatus,  2000);
setInterval(fetchSysinfo, 3000);
setInterval(fetchPreview, 5000);  // fallback if SSE is absent

fetchStatus();
fetchSysinfo();
setTimeout(fetchPreview, 800);

// ── Server-Sent Events (real-time frame push) ─────────────────────────────────
let evtSource = null;

function connectEvents() {
  if (evtSource) { evtSource.close(); evtSource = null; }
  try {
    evtSource = new EventSource('/api/events');
    evtSource.addEventListener('frame', e => {
      const seq = parseInt(e.data, 10);
      if (!isNaN(seq) && seq !== lastFrameSeq) {
        lastFrameSeq = seq;
        fetchPreview();
      }
    });
    evtSource.onerror = () => {
      // On error, back off for 10 s and reconnect (SSE auto-reconnects anyway,
      // but this ensures we don't hammer on initial failures)
      if (evtSource) { evtSource.close(); evtSource = null; }
      setTimeout(connectEvents, 10_000);
    };
  } catch (_) {
    // Browser doesn't support EventSource (very unlikely), fall back to polling
  }
}

connectEvents();

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
      } else {
        logEntry(`Mode change failed: ${await r.text()}`, 'err');
      }
    } catch (e) {
      logEntry(`Mode change error: ${e.message}`, 'err');
    }
  });
});

// ── Return to previous mode ───────────────────────────────────────────────────
returnBtn.addEventListener('click', async () => {
  try {
    const r = await fetch('/api/clear', { method: 'POST' });
    if (r.ok) { logEntry('Returned to previous mode', 'ok'); fetchStatus(); }
    else       { logEntry(`Return failed: ${await r.text()}`, 'err'); }
  } catch (e) { logEntry(`Return error: ${e.message}`, 'err'); }
});

// ── File drop / upload ────────────────────────────────────────────────────────
dropZone.addEventListener('click', () => fileInput.click());
fileInput.addEventListener('change', e => { const f = e.target.files[0]; if (f) setFile(f); });

dropZone.addEventListener('dragover', e => { e.preventDefault(); dropZone.classList.add('drag-over'); });
['dragleave', 'dragend'].forEach(ev => dropZone.addEventListener(ev, () => dropZone.classList.remove('drag-over')));
dropZone.addEventListener('drop', e => {
  e.preventDefault(); dropZone.classList.remove('drag-over');
  const f = e.dataTransfer.files[0];
  if (f && f.type.startsWith('image/')) setFile(f);
  else logEntry('Dropped file is not an image', 'err');
});

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
  currentFile = null; fileInput.value = '';
  previewWrap.classList.add('hidden'); dropZone.style.display = '';
  uploadStatusEl.textContent = '';
});

sendBtn.addEventListener('click', async () => {
  if (!currentFile) return;
  sendBtn.disabled = true;
  uploadStatusEl.textContent = '⏳ Sending…'; uploadStatusEl.className = '';
  const fd = new FormData(); fd.append('image', currentFile);
  try {
    const r = await fetch('/api/image', { method: 'POST', body: fd });
    if (r.ok) {
      uploadStatusEl.textContent = '✓ Image sent'; uploadStatusEl.className = 'status-ok';
      logEntry(`Image sent: ${currentFile.name}`, 'ok'); fetchStatus();
    } else {
      const err = await r.text();
      uploadStatusEl.textContent = `✗ ${err}`; uploadStatusEl.className = 'status-err';
      logEntry(`Upload failed: ${err}`, 'err');
    }
  } catch (e) {
    uploadStatusEl.textContent = `✗ ${e.message}`; uploadStatusEl.className = 'status-err';
    logEntry(`Upload error: ${e.message}`, 'err');
  } finally { sendBtn.disabled = false; }
});

document.addEventListener('keydown', e => {
  if (e.key === 'Enter' && currentFile && !sendBtn.disabled) sendBtn.click();
});

// ── Keyboard shortcuts ────────────────────────────────────────────────────────
// Only fire when focus is NOT inside a text input / textarea / select.
function isFocusedOnInput() {
  const tag = document.activeElement?.tagName?.toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select';
}

document.addEventListener('keydown', e => {
  if (isFocusedOnInput()) return;

  // Escape → return to previous mode (same as the ← Return button)
  if (e.key === 'Escape' && returnBtn.style.display !== 'none') {
    e.preventDefault();
    returnBtn.click();
    return;
  }

  // 1–9 → switch to the nth mode button (by DOM order)
  const num = parseInt(e.key, 10);
  if (!isNaN(num) && num >= 1 && num <= 9 && !e.ctrlKey && !e.metaKey && !e.altKey) {
    const btn = [...modeBtns][num - 1];
    if (btn) { e.preventDefault(); btn.click(); }
  }
});

// ── Custom text form ──────────────────────────────────────────────────────────
txtProgress.addEventListener('input', () => {
  const v = parseInt(txtProgress.value, 10);
  progPctLbl.textContent = v < 0 ? '(off)' : `${v}%`;
});
progPctLbl.textContent = '(off)';

function buildTextPayload() {
  const title    = txtTitle.value.trim();
  const subtitle = txtSubtitle.value.trim();
  const lines    = txtLines.value.split('\n').map(l => l.trim()).filter(Boolean);
  const progVal  = parseInt(txtProgress.value, 10);
  const durVal   = parseInt(txtDuration ? txtDuration.value : '0', 10);
  const payload  = { color: txtColor.value, show_time: txtShowTime.checked };
  if (title)          payload.title    = title;
  if (subtitle)       payload.subtitle = subtitle;
  if (lines.length)   payload.lines    = lines;
  if (progVal >= 0)   payload.progress = progVal / 100;
  if (durVal > 0)     payload.duration_secs = durVal;
  return payload;
}

async function sendTextPayload(payload) {
  return fetch('/api/text', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

txtSendBtn.addEventListener('click', async () => {
  const title  = txtTitle.value.trim();
  const lines  = txtLines.value.split('\n').map(l => l.trim()).filter(Boolean);
  if (!title && !txtSubtitle.value.trim() && lines.length === 0) {
    txtStatus.textContent = 'Enter at least a title or some lines.';
    txtStatus.className   = 'status-err'; return;
  }
  txtSendBtn.disabled = true; txtStatus.textContent = '⏳ Sending…'; txtStatus.className = '';
  try {
    const r = await sendTextPayload(buildTextPayload());
    if (r.ok) {
      txtStatus.textContent = '✓ Text sent'; txtStatus.className = 'status-ok';
      logEntry(`Text sent: "${title || '(no title)'}"`, 'ok'); fetchStatus(); refreshHistory();
    } else {
      const err = await r.text();
      txtStatus.textContent = `✗ ${err}`; txtStatus.className = 'status-err';
      logEntry(`Text send failed: ${err}`, 'err');
    }
  } catch (e) {
    txtStatus.textContent = `✗ ${e.message}`; txtStatus.className = 'status-err';
    logEntry(`Text send error: ${e.message}`, 'err');
  } finally { txtSendBtn.disabled = false; }
});

// ── Custom text preview ──────────────────────────────────────────────────────
let lastPreviewUrl = null;

txtPreviewBtn.addEventListener('click', async () => {
  txtPreviewBtn.disabled = true;
  try {
    const r = await fetch('/api/render', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(buildTextPayload()),
    });
    if (!r.ok) {
      logEntry(`Preview error: ${await r.text()}`, 'err');
      return;
    }
    const blob = await r.blob();
    const url  = URL.createObjectURL(blob);
    if (lastPreviewUrl) URL.revokeObjectURL(lastPreviewUrl);
    lastPreviewUrl     = url;
    txtPreviewImg.src  = url;
    txtPreviewWrap.style.display = '';
  } catch (e) {
    logEntry(`Preview error: ${e.message}`, 'err');
  } finally {
    txtPreviewBtn.disabled = false;
  }
});

// ── Quick notify ──────────────────────────────────────────────────────────────
const QUICK_DEFS = {
  ok:     { title: 'OK',        color: 'green',  progress: 1.0 },
  fail:   { title: 'FAILED',    color: 'red',    progress: 0.0 },
  warn:   { title: 'Warning',   color: 'yellow', progress: null },
  run:    { title: 'Running…',  color: 'blue',   progress: 0.5 },
  deploy: { title: 'Deploying', color: 'teal',   progress: null },
  done:   { title: 'Done',      color: 'mauve',  progress: 1.0 },
  info:   { title: 'Info',      color: 'white',  progress: null },
  reboot: { title: 'Rebooting', color: 'peach',  progress: null },
};

document.querySelectorAll('.quick-btn').forEach(btn => {
  btn.addEventListener('click', async () => {
    const key = btn.dataset.quick;
    const def = QUICK_DEFS[key];
    if (!def) return;

    // Fill form
    txtTitle.value = def.title; txtSubtitle.value = ''; txtLines.value = '';
    txtColor.value = def.color; txtShowTime.checked = true;
    if (def.progress != null) {
      txtProgress.value = Math.round(def.progress * 100);
      progPctLbl.textContent = `${Math.round(def.progress * 100)}%`;
    } else {
      txtProgress.value = -1; progPctLbl.textContent = '(off)';
    }

    // Send
    const payload = { title: def.title, color: def.color, show_time: true };
    if (def.progress != null) payload.progress = def.progress;
    btn.disabled = true; quickStatus.textContent = '⏳ Sending…'; quickStatus.className = '';
    try {
      const r = await sendTextPayload(payload);
      if (r.ok) {
        quickStatus.textContent = `✓ "${def.title}" sent`; quickStatus.className = 'status-ok';
        logEntry(`Quick: "${def.title}" (${def.color})`, 'ok'); fetchStatus(); refreshHistory();
      } else {
        const err = await r.text();
        quickStatus.textContent = `✗ ${err}`; quickStatus.className = 'status-err';
        logEntry(`Quick failed: ${err}`, 'err');
      }
    } catch (e) {
      quickStatus.textContent = `✗ ${e.message}`; quickStatus.className = 'status-err';
    } finally { btn.disabled = false; }
  });
});

// ── Preset save / load ────────────────────────────────────────────────────────
async function refreshPresets() {
  try {
    const r = await fetch('/api/presets');
    if (!r.ok) return;
    const list = await r.json();
    const prev = presetSelect.value;
    while (presetSelect.options.length > 1) presetSelect.remove(1);
    list.forEach(entry => {
      const opt = document.createElement('option');
      opt.value = opt.textContent = entry.name;
      presetSelect.appendChild(opt);
    });
    if (prev && [...presetSelect.options].some(o => o.value === prev)) presetSelect.value = prev;
  } catch (_) { /* ignore */ }
}

/** Load preset payload into the text form fields (does not send to panel). */
function loadPresetIntoForm(p) {
  txtTitle.value    = p.title    ?? '';
  txtSubtitle.value = p.subtitle ?? '';
  txtLines.value    = (p.lines   ?? []).join('\n');
  if (p.color) {
    const lc = p.color.toLowerCase();
    const opt = [...txtColor.options].find(o => o.value === lc);
    if (opt) txtColor.value = lc;
  }
  txtShowTime.checked = p.show_time !== false;
  if (p.progress != null) {
    const pct = Math.round(p.progress * 100);
    txtProgress.value       = pct;
    progPctLbl.textContent  = `${pct}%`;
  } else {
    txtProgress.value      = -1;
    progPctLbl.textContent = '(off)';
  }
}

presetSaveBtn.addEventListener('click', async () => {
  const name = prompt('Preset name:');
  if (!name?.trim()) return;
  try {
    const r = await fetch('/api/presets', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: name.trim(), payload: buildTextPayload() }),
    });
    if (r.ok) {
      logEntry(`Preset saved: "${name.trim()}"`, 'ok');
      await refreshPresets(); presetSelect.value = name.trim();
    } else { logEntry(`Preset save failed: ${await r.text()}`, 'err'); }
  } catch (e) { logEntry(`Preset save error: ${e.message}`, 'err'); }
});

presetApplyBtn.addEventListener('click', async () => {
  const name = presetSelect.value;
  if (!name) return;
  try {
    // First: fetch the preset data so we can populate the form
    const gr = await fetch(`/api/presets/${encodeURIComponent(name)}`);
    if (gr.ok) {
      const p = await gr.json();
      loadPresetIntoForm(p);
    }
    // Then: apply to panel
    const r = await fetch(`/api/presets/${encodeURIComponent(name)}`, { method: 'POST' });
    if (r.ok) { logEntry(`Applied preset: "${name}"`, 'ok'); fetchStatus(); refreshHistory(); }
    else       { logEntry(`Apply preset failed: ${await r.text()}`, 'err'); }
  } catch (e) { logEntry(`Apply preset error: ${e.message}`, 'err'); }
});

presetDelBtn.addEventListener('click', async () => {
  const name = presetSelect.value;
  if (!name || !confirm(`Delete preset "${name}"?`)) return;
  try {
    const r = await fetch(`/api/presets/${encodeURIComponent(name)}`, { method: 'DELETE' });
    if (r.ok) { logEntry(`Deleted preset: "${name}"`, 'ok'); await refreshPresets(); }
    else       { logEntry(`Delete failed: ${await r.text()}`, 'err'); }
  } catch (e) { logEntry(`Delete error: ${e.message}`, 'err'); }
});

presetClearBtn.addEventListener('click', async () => {
  if (!confirm('Delete ALL presets? This cannot be undone.')) return;
  try {
    const r = await fetch('/api/presets', { method: 'DELETE' });
    if (r.ok) {
      logEntry(`All presets cleared: ${await r.text()}`, 'ok');
      await refreshPresets();
    } else { logEntry(`Clear presets failed: ${await r.text()}`, 'err'); }
  } catch (e) { logEntry(`Clear presets error: ${e.message}`, 'err'); }
});

// ── Panel settings (quality + orient) ────────────────────────────────────────
setQuality.addEventListener('input', () => {
  setQualityLbl.textContent = `${setQuality.value}%`;
});

setApplyBtn.addEventListener('click', async () => {
  const quality = parseInt(setQuality.value, 10);
  const rotate  = parseInt(setOrient.value, 10);
  setApplyBtn.disabled = true;
  setStatus.textContent = '⏳ Applying…'; setStatus.className = '';
  try {
    const [qr, or] = await Promise.all([
      fetch('/api/quality', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ quality }),
      }),
      fetch('/api/orient', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ rotate }),
      }),
    ]);
    if (qr.ok && or.ok) {
      setStatus.textContent = `✓ Quality=${quality}% · Rotate=${rotate}°`;
      setStatus.className = 'status-ok';
      logEntry(`Settings applied: quality=${quality}% rotate=${rotate}°`, 'ok');
      fetchStatus();
    } else {
      const err = (!qr.ok ? await qr.text() : '') + (!or.ok ? ` ${await or.text()}` : '');
      setStatus.textContent = `✗ ${err.trim()}`; setStatus.className = 'status-err';
      logEntry(`Settings error: ${err.trim()}`, 'err');
    }
  } catch (e) {
    setStatus.textContent = `✗ ${e.message}`; setStatus.className = 'status-err';
    logEntry(`Settings error: ${e.message}`, 'err');
  } finally { setApplyBtn.disabled = false; }
});

// ── Slideshow ─────────────────────────────────────────────────────────────────
ssStartBtn.addEventListener('click', async () => {
  const dir      = ssDir.value.trim();
  const interval = parseInt(ssInterval.value, 10) || 5;
  if (!dir) { ssStatus.textContent = 'Enter a directory path.'; ssStatus.className = 'status-err'; return; }
  ssStartBtn.disabled = true; ssStatus.textContent = '⏳ Starting…'; ssStatus.className = '';
  try {
    const r = await fetch('/api/slideshow', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ dir, interval }),
    });
    if (r.ok) {
      ssStatus.textContent = `✓ ${await r.text()}`; ssStatus.className = 'status-ok';
      logEntry(`Slideshow started: ${dir}`, 'ok'); fetchStatus();
    } else {
      const err = await r.text();
      ssStatus.textContent = `✗ ${err}`; ssStatus.className = 'status-err';
      logEntry(`Slideshow error: ${err}`, 'err');
    }
  } catch (e) {
    ssStatus.textContent = `✗ ${e.message}`; ssStatus.className = 'status-err';
    logEntry(`Slideshow error: ${e.message}`, 'err');
  } finally { ssStartBtn.disabled = false; }
});

async function slideshowNav(dir) {
  try {
    const r = await fetch(`/api/slideshow/${dir}`, { method: 'POST' });
    if (r.ok) {
      ssStatus.textContent = `✓ ${await r.text()}`; ssStatus.className = 'status-ok';
      logEntry(`Slideshow ${dir}`, 'ok');
    } else {
      ssStatus.textContent = `✗ ${await r.text()}`; ssStatus.className = 'status-err';
    }
  } catch (e) {
    ssStatus.textContent = `✗ ${e.message}`; ssStatus.className = 'status-err';
  }
}

if (ssPrevBtn) ssPrevBtn.addEventListener('click', () => slideshowNav('prev'));
if (ssNextBtn) ssNextBtn.addEventListener('click', () => slideshowNav('next'));

// ── Auto-rotation ─────────────────────────────────────────────────────────────
rotStartBtn.addEventListener('click', async () => {
  const modesRaw = rotModes.value.trim();
  const interval = parseInt(rotInterval.value, 10) || 30;
  if (!modesRaw) { rotStatus.textContent = 'Enter at least one mode.'; rotStatus.className = 'status-err'; return; }
  const modes = modesRaw.split(',').map(m => m.trim()).filter(Boolean);
  rotStartBtn.disabled = true; rotStatus.textContent = '⏳ Starting…'; rotStatus.className = '';
  try {
    const r = await fetch('/api/rotation', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ modes, interval }),
    });
    if (r.ok) {
      rotStatus.textContent = `✓ ${await r.text()}`; rotStatus.className = 'status-ok';
      logEntry(`Rotation started: [${modes.join(', ')}] every ${interval}s`, 'ok'); fetchStatus();
    } else {
      const err = await r.text();
      rotStatus.textContent = `✗ ${err}`; rotStatus.className = 'status-err';
      logEntry(`Rotation error: ${err}`, 'err');
    }
  } catch (e) {
    rotStatus.textContent = `✗ ${e.message}`; rotStatus.className = 'status-err';
  } finally { rotStartBtn.disabled = false; }
});

rotStopBtn.addEventListener('click', async () => {
  try {
    const r = await fetch('/api/rotation', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ modes: [], interval: 0 }),
    });
    if (r.ok) {
      rotStatus.textContent = '■ Rotation stopped'; rotStatus.className = 'status-ok';
      logEntry('Rotation stopped', 'ok'); fetchStatus();
    } else {
      rotStatus.textContent = `✗ ${await r.text()}`; rotStatus.className = 'status-err';
    }
  } catch (e) {
    rotStatus.textContent = `✗ ${e.message}`; rotStatus.className = 'status-err';
  }
});

// ── Schedule notification ─────────────────────────────────────────────────────
if (schedBtn) {
  schedBtn.addEventListener('click', async () => {
    const inSecs = parseInt(schedInSecs.value, 10);
    const label  = schedLabel ? schedLabel.value.trim() : '';
    if (!inSecs || inSecs < 1) {
      schedStatus.textContent = 'Enter a delay in seconds.';
      schedStatus.className   = 'status-err'; return;
    }
    const payload = buildTextPayload();
    if (!payload.title && !payload.lines?.length) {
      schedStatus.textContent = 'Fill in the Custom Text form first.';
      schedStatus.className   = 'status-err'; return;
    }
    schedBtn.disabled = true;
    schedStatus.textContent = '⏳ Scheduling…'; schedStatus.className = '';
    try {
      const body = { in_secs: inSecs, payload };
      if (label) body.label = label;
      const r = await fetch('/api/schedule', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (r.ok) {
        schedStatus.textContent = `✓ ${await r.text()}`; schedStatus.className = 'status-ok';
        logEntry(`Scheduled in ${inSecs}s${label ? ': "' + label + '"' : ''}`, 'ok');
      } else {
        const err = await r.text();
        schedStatus.textContent = `✗ ${err}`; schedStatus.className = 'status-err';
        logEntry(`Schedule error: ${err}`, 'err');
      }
    } catch (e) {
      schedStatus.textContent = `✗ ${e.message}`; schedStatus.className = 'status-err';
      logEntry(`Schedule error: ${e.message}`, 'err');
    } finally { schedBtn.disabled = false; }
  });
}

// ── Countdown timer ──────────────────────────────────────────────────────────
cdStartBtn.addEventListener('click', async () => {
  const secs  = parseInt(cdSeconds.value, 10);
  const label = cdLabel.value.trim();
  if (!secs || secs < 1) {
    cdStatus.textContent = 'Enter a duration in seconds.';
    cdStatus.className   = 'status-err'; return;
  }
  cdStartBtn.disabled = true;
  cdStatus.textContent = '⏳ Starting…'; cdStatus.className = '';
  try {
    const body = { seconds: secs };
    if (label) body.label = label;
    const r = await fetch('/api/countdown', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (r.ok) {
      const msg = label ? `Countdown: ${secs}s "${label}"` : `Countdown: ${secs}s`;
      cdStatus.textContent = `✓ ${msg}`; cdStatus.className = 'status-ok';
      logEntry(msg, 'ok'); fetchStatus(); refreshHistory();
    } else {
      const err = await r.text();
      cdStatus.textContent = `✗ ${err}`; cdStatus.className = 'status-err';
      logEntry(`Countdown error: ${err}`, 'err');
    }
  } catch (e) {
    cdStatus.textContent = `✗ ${e.message}`; cdStatus.className = 'status-err';
    logEntry(`Countdown error: ${e.message}`, 'err');
  } finally { cdStartBtn.disabled = false; }
});

cdCancelBtn.addEventListener('click', async () => {
  try {
    const r = await fetch('/api/clear', { method: 'POST' });
    if (r.ok) {
      cdStatus.textContent = '✓ Countdown cancelled'; cdStatus.className = 'status-ok';
      logEntry('Countdown cancelled', 'ok'); fetchStatus();
    } else {
      cdStatus.textContent = `✗ ${await r.text()}`; cdStatus.className = 'status-err';
    }
  } catch (e) {
    cdStatus.textContent = `✗ ${e.message}`; cdStatus.className = 'status-err';
  }
});

// ── History ───────────────────────────────────────────────────────────────────
const historyListEl  = document.getElementById('history-list');
const histRefreshBtn = document.getElementById('hist-refresh-btn');
const histClearBtn   = document.getElementById('hist-clear-btn');

async function refreshHistory() {
  try {
    const r = await fetch('/api/history?limit=20');
    if (!r.ok) return;
    const items = await r.json();
    historyListEl.innerHTML = '';
    if (items.length === 0) {
      const li = document.createElement('li');
      li.className = 'hist-empty';
      li.textContent = 'No notifications yet.';
      historyListEl.appendChild(li);
      return;
    }
    items.forEach(item => {
      const li  = document.createElement('li');
      const ts  = new Date(item.timestamp * 1000).toTimeString().slice(0, 8);
      const kind = item.kind === 'alert' ? '🔔' : '📝';
      li.innerHTML =
        `<span class="hist-ts">${ts}</span>` +
        `<span class="hist-kind">${kind}</span>` +
        `<span class="hist-msg">${escHtml(item.summary)}</span>`;
      historyListEl.appendChild(li);
    });
  } catch (_) { /* busy */ }
}

if (histRefreshBtn) {
  histRefreshBtn.addEventListener('click', refreshHistory);
}

if (histClearBtn) {
  histClearBtn.addEventListener('click', async () => {
    if (!confirm('Clear all notification history?')) return;
    try {
      const r = await fetch('/api/history', { method: 'DELETE' });
      if (r.ok) {
        logEntry(`History cleared: ${await r.text()}`, 'ok');
        await refreshHistory();
      } else {
        logEntry(`History clear failed: ${await r.text()}`, 'err');
      }
    } catch (e) {
      logEntry(`History clear error: ${e.message}`, 'err');
    }
  });
}

// ── Init ──────────────────────────────────────────────────────────────────────
refreshPresets();
refreshHistory();

// Sync quality slider label on load
setQualityLbl.textContent = `${setQuality.value}%`;

logEntry('ttlcd UI ready', 'ok');
