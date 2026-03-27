/**
 * QDM Content Script v1.2.2
 *
 * 1. Injects inject_v2.js into the page world to intercept dynamic video sources
 * 2. IDM-style hover download banner on ALL video/audio elements (including blob: YouTube)
 * 3. Banner click → asks background.js for the captured URL → downloads via QDM
 * 4. Link grabbing on request
 */
console.log('[QDM] content_v2.js v1.2.2 loaded at', new Date().toISOString());

(() => {
  const QDM_PORT = 8597;
  const QDM_HOST = `http://127.0.0.1:${QDM_PORT}`;
  const BANNER_ID = 'qdm-download-banner';

  let isQdmRunning = false;
  let bannerTimeout = null;

  // ── Inject page-world script ─────────────────────────────────────────────────
  function injectPageScript() {
    if (document.getElementById('qdm-inject')) return;
    try {
      const script = document.createElement('script');
      script.id = 'qdm-inject';
      script.src = chrome.runtime.getURL('inject.js');
      (document.head || document.documentElement).appendChild(script);
      script.remove();
    } catch {}
  }
  injectPageScript();

  // ── Listen for page-world messages (from inject.js) ──────────────────────────
  window.addEventListener('message', (e) => {
    if (!e.data || !e.data._qdm) return;
    const { action, url, title, mimeType, tabUrl } = e.data;
    if (action === 'page-media' && url) {
      chrome.runtime.sendMessage({
        action: 'page-media',
        url,
        title: title || document.title,
        mimeType: mimeType || '',
        tabUrl: tabUrl || location.href,
      });
    }
  });

  // ── QDM health check ─────────────────────────────────────────────────────────
  // One-shot check on page load.  Ongoing keepalive is handled by the background
  // service worker alarm (chrome.alarms) — a setInterval in the content script
  // runs in the page context and does nothing for SW lifecycle.
  async function checkQDM() {
    try {
      const res = await fetch(`${QDM_HOST}/sync`, { method: 'GET', signal: AbortSignal.timeout(2000) });
      isQdmRunning = res.ok;
    } catch { isQdmRunning = false; }
  }
  checkQDM();

  // ── Video hover banner ───────────────────────────────────────────────────────
  function createBanner() {
    let b = document.getElementById(BANNER_ID);
    if (b) return b;
    b = document.createElement('div');
    b.id = BANNER_ID;
    b.innerHTML = `<div style="display:flex;align-items:center;gap:6px;background:linear-gradient(135deg,#6c5ce7,#a855f7);color:#fff;padding:6px 14px;border-radius:8px;font-family:-apple-system,'Segoe UI',sans-serif;font-size:12px;font-weight:700;box-shadow:0 4px 20px rgba(108,92,231,0.6);cursor:pointer;user-select:none;white-space:nowrap"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>Download with QDM</div>`;
    Object.assign(b.style, {
      position: 'fixed', zIndex: '2147483647', pointerEvents: 'auto',
      opacity: '0', transform: 'scale(0.9)', transition: 'opacity .15s,transform .15s',
    });

    b.addEventListener('click', async (e) => {
      e.stopPropagation();
      e.preventDefault();
      hideBanner();

      // First try: direct URL from element (non-blob)
      const directUrl = b._directUrl;
      if (directUrl) {
        try {
          await fetch(`${QDM_HOST}/download`, {
            method: 'POST', headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ url: directUrl, file: '', tabUrl: location.href }),
          });
        } catch {}
        return;
      }

      // Second try: ask background.js for the captured URL for this tab
      try {
        chrome.runtime.sendMessage({ action: 'banner-download' }, (resp) => {
          if (chrome.runtime.lastError || !resp?.found) {
            // Nothing captured yet — flash banner red briefly
            b.style.opacity = '0.4';
            setTimeout(() => { b.style.opacity = '0'; }, 600);
          }
        });
      } catch {}
    });

    b.addEventListener('mouseenter', () => clearTimeout(bannerTimeout));
    b.addEventListener('mouseleave', () => scheduleBannerHide());
    document.documentElement.appendChild(b);
    return b;
  }

  function showBanner(target, directUrl) {
    if (!isQdmRunning) return;
    const b = createBanner();
    b._directUrl = directUrl || null;

    const r = target.getBoundingClientRect();
    // Position top-right of element, using fixed positioning (survives scroll)
    const top = Math.max(8, r.top + 8);
    const right = Math.max(8, window.innerWidth - r.right + 4);
    b.style.top = `${top}px`;
    b.style.right = `${right}px`;
    b.style.left = 'auto';
    requestAnimationFrame(() => { b.style.opacity = '1'; b.style.transform = 'scale(1)'; });
    clearTimeout(bannerTimeout);
    scheduleBannerHide();
  }

  function hideBanner() {
    const b = document.getElementById(BANNER_ID);
    if (b) { b.style.opacity = '0'; b.style.transform = 'scale(0.9)'; }
  }

  function scheduleBannerHide() {
    clearTimeout(bannerTimeout);
    bannerTimeout = setTimeout(hideBanner, 3000);
  }

  function getDirectSrc(el) {
    const candidates = [el.currentSrc, el.src];
    for (const s of el.querySelectorAll('source')) candidates.push(s.src);
    // Return real HTTP URL (not blob: or data:)
    return candidates.find(s => s && (s.startsWith('http://') || s.startsWith('https://'))) || null;
  }

  function attachBanner(el) {
    if (el._qdm) return;
    el._qdm = true;
    el.addEventListener('mouseenter', () => {
      const url = getDirectSrc(el);
      showBanner(el, url); // url may be null for YouTube blob: — that's fine
    });
    el.addEventListener('mouseleave', scheduleBannerHide);
  }

  // ── DOM scanning ─────────────────────────────────────────────────────────────
  function scanMedia() {
    document.querySelectorAll('video, audio').forEach(attachBanner);
  }

  // ── Listen for media-captured notification from background.js ────────────────
  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.action === 'media-captured') {
      // Media was captured for this tab — ensure all video elements have banners attached
      scanMedia();
    }
    if (msg.action === 'grab-links') {
      // handled below via the other listener
    }
  });

  // ── Init ─────────────────────────────────────────────────────────────────────
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', scanMedia, { once: true });
  } else {
    scanMedia();
  }

  // SPA navigation observer — covers YouTube, React, Vue apps
  let lastHref = location.href;
  const navObserver = new MutationObserver(() => {
    if (location.href !== lastHref) {
      lastHref = location.href;
      setTimeout(scanMedia, 1500);
    }
    scanMedia();
  });
  navObserver.observe(document.documentElement, { childList: true, subtree: true });

  // ── Link grabbing ─────────────────────────────────────────────────────────────
  chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
    if (msg.action === 'grab-links') {
      const links = [], seen = new Set();
      document.querySelectorAll('a[href]').forEach(a => {
        const url = a.href;
        if (url && !seen.has(url) && (url.startsWith('http://') || url.startsWith('https://'))) {
          seen.add(url);
          links.push({ url, file: a.download || a.textContent?.trim().slice(0, 100) || '' });
        }
      });
      chrome.runtime.sendMessage({ action: 'batch-links', links });
      sendResponse({ count: links.length });
    }
    return true;
  });
})();
