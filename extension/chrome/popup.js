// QDM Extension Popup v1.2.0
const QDM_HOST = 'http://127.0.0.1:8597';

const dot        = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const mainContent= document.getElementById('mainContent');
const footer     = document.getElementById('footer');
const openBtn    = document.getElementById('openBtn');

// Retrieve the session token stored by the background service worker
async function getToken() {
  try {
    const r = await chrome.storage.session.get('qdmToken');
    return r.qdmToken || null;
  } catch { return null; }
}

function authHeaders(token, extra) {
  const h = { 'Content-Type': 'application/json', ...extra };
  if (token) h['X-QDM-Token'] = token;
  return h;
}

async function init() {
  let running = false;
  let mediaItems = [];
  let token = await getToken();

  try {
    const syncRes = await fetch(`${QDM_HOST}/sync`, { signal: AbortSignal.timeout(3000) });
    if (syncRes.ok) {
      running = true;
      const syncData = await syncRes.json();
      // Update token if QDM restarted (new token)
      if (syncData.token && syncData.token !== token) {
        token = syncData.token;
        chrome.storage.session.set({ qdmToken: token });
      }
      // Fetch media list with token
      try {
        const mediaRes = await fetch(`${QDM_HOST}/media`, {
          headers: authHeaders(token),
          signal: AbortSignal.timeout(3000),
        });
        if (mediaRes.ok) {
          const data = await mediaRes.json();
          mediaItems = data.items || [];
        }
      } catch {}
    }
  } catch {}

  if (running) {
    if (dot)        dot.className = 'dot on';
    if (statusText) { statusText.className = 'status-text on'; statusText.textContent = 'QDM is running'; }
    if (openBtn)    openBtn.classList.remove('hidden');
    if (footer)     footer.style.display = 'flex';
    const urlSec = document.getElementById('urlInputSection');
    if (urlSec) urlSec.style.display = 'block';
    setupInterceptToggle();
    setupUrlInput(token);
    renderMediaList(mediaItems, token);
  } else {
    if (dot)        dot.className = 'dot off';
    if (statusText) { statusText.className = 'status-text off'; statusText.textContent = 'QDM is not running'; }
    if (mainContent) mainContent.innerHTML = `
      <div class="not-running">
        <div class="nr-icon">⚡</div>
        <p>QDM is not running.</p>
        <p style="margin-top:6px">Download and start the<br>
        <a href="https://github.com/PBhadoo/QDM/releases" target="_blank">QDM app</a> to use this extension.</p>
      </div>`;
  }
}

// Ask the background service worker for YouTube cookies in Netscape format.
// The SW runs in a privileged context and has reliable chrome.cookies access.
function getYtCookiesNetscape() {
  return new Promise(resolve => {
    chrome.runtime.sendMessage({ action: 'get-yt-cookies' }, res => {
      resolve(res?.ytdlpCookies || null);
    });
  });
}

function setupInterceptToggle() {
  const row = document.getElementById('interceptRow');
  const check = document.getElementById('interceptCheck');
  if (!row || !check) return;
  row.style.display = 'flex';

  // Read current state from background
  chrome.runtime.sendMessage({ action: 'get-status' }, (res) => {
    if (res && typeof res.interceptEnabled === 'boolean') {
      check.checked = res.interceptEnabled;
    }
  });

  check.addEventListener('change', () => {
    chrome.runtime.sendMessage({ action: 'set-intercept', enabled: check.checked });
  });
}

function setupUrlInput(token) {
  const input = document.getElementById('urlInput');
  const btn = document.getElementById('urlDlBtn');
  const status = document.getElementById('urlStatus');
  if (!input || !btn || !status) return;

  // Pre-fill from active tab URL if it looks like a yt-dlp-supported page
  chrome.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
    if (tab && tab.url && isYtDlpUrl(tab.url)) {
      input.value = tab.url;
    }
  });

  async function doDownload() {
    const url = input.value.trim();
    if (!url || !url.startsWith('http')) {
      status.style.color = '#e17055';
      status.textContent = 'Please enter a valid URL';
      return;
    }
    btn.disabled = true;
    status.style.color = '#55556a';
    status.textContent = 'Fetching cookies…';

    // Get cookies — YouTube gets Netscape format for yt-dlp; others get plain cookie string
    let cookie, ytdlpCookies;
    if (isYtDlpUrl(url)) {
      ytdlpCookies = (await getYtCookiesNetscape()) || null;
    } else {
      try {
        const cookies = await chrome.cookies.getAll({ url });
        cookie = cookies.length ? cookies.map(c => `${c.name}=${c.value}`).join('; ') : undefined;
      } catch {}
    }

    // Get current tab for referer
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true }).catch(() => [null]);

    status.textContent = 'Sending to QDM…';
    try {
      const res = await fetch(`${QDM_HOST}/download`, {
        method: 'POST',
        headers: authHeaders(token),
        body: JSON.stringify({
          url,
          file: '',
          tabUrl: tab?.url || '',
          tabId: (tab?.id || -1).toString(),
          cookie,
          ytdlpCookies,
          requestHeaders: tab?.url ? { Referer: tab.url } : {},
          contentLength: 0,
          contentType: '',
        }),
        signal: AbortSignal.timeout(5000),
      });
      if (res.ok) {
        status.style.color = '#00b894';
        status.textContent = 'Sent to QDM!';
        input.value = '';
      } else if (res.status === 401) {
        status.style.color = '#e17055';
        status.textContent = 'Token mismatch — reload popup';
      } else {
        status.style.color = '#fdcb6e';
        status.textContent = 'QDM responded: ' + res.status;
      }
    } catch (err) {
      status.style.color = '#e17055';
      status.textContent = 'Failed to reach QDM';
    }
    btn.disabled = false;
  }

  btn.addEventListener('click', doDownload);
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter') doDownload(); });
}

function isYtDlpUrl(url) {
  return url.includes('youtube.com/watch') || url.includes('youtu.be/') ||
    url.includes('youtube.com/shorts/') || url.includes('music.youtube.com/watch');
}

function fmtSize(b) {
  if (!b || b <= 0) return '';
  const k = 1024, s = ['B','KB','MB','GB'];
  const i = Math.floor(Math.log(b) / Math.log(k));
  return parseFloat((b / Math.pow(k, i)).toFixed(1)) + ' ' + s[i];
}

function esc(s) { return (s||'').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }

function renderMediaList(items, token) {
  if (!mainContent) return;
  mainContent.innerHTML = '';
  const section = document.createElement('div');
  section.className = 'section';

  const header = document.createElement('div');
  header.className = 'section-header';
  header.innerHTML = `
    <span class="section-label">Detected Media</span>
    ${items.length ? `<span class="count-badge">${items.length}</span>` : ''}`;
  section.appendChild(header);

  if (!items.length) {
    section.innerHTML += `
      <div class="empty">
        <div class="empty-icon">🎬</div>
        <p>No media detected yet.</p>
        <p>Browse any video page and media will appear here.</p>
      </div>`;
    mainContent.appendChild(section);
    return;
  }

  const list = document.createElement('div');
  list.className = 'video-list';

  items.forEach(item => {
    // MediaItem fields: id, name, description, media_type, size, url, content_type
    const name = item.name || item.url || 'Unknown';
    const desc = item.description || '';
    const mtype = item.media_type || item.mediaType || 'video';
    const size = fmtSize(item.size > 0 ? item.size : 0);
    const isAudio = mtype === 'audio' || (item.content_type||'').startsWith('audio/');
    const isHls = mtype === 'hls' || name.includes('.m3u8');
    const icon = isHls ? '📡' : isAudio ? '🎵' : '🎬';

    const badges = [];
    if (isHls) badges.push(['hls', 'HLS Stream']);
    else if (isAudio) badges.push(['audio', 'Audio']);
    else badges.push(['video', 'Video']);
    if (desc) {
      desc.split('•').forEach(p => {
        const t = p.trim();
        if (t) {
          const cls = /^\d/.test(t) ? 'size' : 'quality';
          badges.push([cls, t]);
        }
      });
    }
    if (size && !badges.some(b => b[1] === size)) badges.push(['size', size]);

    const el = document.createElement('div');
    el.className = 'video-item';
    el.innerHTML = `
      <div class="vi-icon">${icon}</div>
      <div class="vi-info">
        <div class="vi-name" title="${esc(name)}">${esc(name)}</div>
        <div class="vi-meta">
          ${badges.map(([cls, text]) => `<span class="badge ${cls}">${esc(text)}</span>`).join('')}
        </div>
      </div>
      <button class="dl-btn" title="Download with QDM">⚡</button>`;

    el.querySelector('.dl-btn').addEventListener('click', async (e) => {
      e.stopPropagation();
      const btn = e.currentTarget;
      const isYt = item.url && (item.url.includes('youtube.com') || item.url.includes('youtu.be'));
      const ytdlpCookies = isYt ? (await getYtCookiesNetscape()) : null;
      try {
        const res = await fetch(`${QDM_HOST}/vid`, {
          method: 'POST',
          headers: authHeaders(token),
          body: JSON.stringify({
            url: item.url,
            file: item.name || '',
            tabUrl: item.tab_url || item.tabUrl || '',
            ytdlpCookies,
          }),
        });
        btn.textContent = res.ok ? '✓' : '✗';
        btn.className = `dl-btn ${res.ok ? 'done' : ''}`;
      } catch { btn.textContent = '✗'; }
    });

    list.appendChild(el);
  });

  section.appendChild(list);
  mainContent.appendChild(section);
}

// Clear button
document.getElementById('clearBtn')?.addEventListener('click', async () => {
  const token = await getToken();
  try { await fetch(`${QDM_HOST}/clear`, { method: 'POST', headers: authHeaders(token), body: '{}' }); } catch {}
  if (mainContent) mainContent.innerHTML = `<div class="section"><div class="empty"><div class="empty-icon">🎬</div><p>Cleared.</p></div></div>`;
  document.getElementById('footer')?.style && (document.getElementById('footer').style.display = 'none');
});

// Open QDM button — tell the app to show/focus its window
document.getElementById('openQdmBtn')?.addEventListener('click', () => {
  fetch(`${QDM_HOST}/show`).catch(() => {});
});

init();
