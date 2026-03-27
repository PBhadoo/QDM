/**
 * QDM inject_v2.js v1.2.2 — runs in the PAGE world (not isolated)
 * Intercepts dynamic video/audio source assignment and MSE streams
 * that aren't visible to webRequest (blob: URLs, programmatic src changes).
 *
 * Posts messages to content.js via window.postMessage({ _qdm: true, ... })
 */
console.log('[QDM] inject_v2.js v1.2.2 loaded at', new Date().toISOString());
(() => {
  const send = (data) => window.postMessage({ _qdm: true, ...data }, '*');

  // ── Intercept HTMLVideoElement / HTMLAudioElement src assignment ────────────
  const mediaSrcDescriptor = Object.getOwnPropertyDescriptor(HTMLMediaElement.prototype, 'src');
  if (mediaSrcDescriptor) {
    Object.defineProperty(HTMLMediaElement.prototype, 'src', {
      set(value) {
        if (value && !value.startsWith('blob:') && !value.startsWith('data:')) {
          const title = document.title || '';
          send({ action: 'page-media', url: value, title, tabUrl: location.href });
        }
        if (mediaSrcDescriptor.set) mediaSrcDescriptor.set.call(this, value);
      },
      get() { return mediaSrcDescriptor.get ? mediaSrcDescriptor.get.call(this) : undefined; },
    });
  }

  // ── Intercept MediaSource.addSourceBuffer (detect MSE codec/container) ──────
  const origAddSourceBuffer = MediaSource.prototype.addSourceBuffer;
  MediaSource.prototype.addSourceBuffer = function(mimeType) {
    if (mimeType) {
      send({ action: 'mse-type', mimeType, tabUrl: location.href });
    }
    return origAddSourceBuffer.call(this, mimeType);
  };

  // ── Intercept fetch() for media responses ───────────────────────────────────
  const origFetch = window.fetch;
  window.fetch = function(...args) {
    const url = typeof args[0] === 'string' ? args[0] : (args[0]?.url || '');
    const result = origFetch.apply(this, args);
    if (url && (url.includes('.m3u8') || url.includes('.mpd') || url.includes('.f4m'))) {
      result.then(res => {
        const ct = res.headers.get('content-type') || '';
        send({ action: 'page-media', url, mimeType: ct, title: document.title, tabUrl: location.href });
      }).catch(() => {});
    }
    return result;
  };

  // ── Intercept XMLHttpRequest for streaming manifest requests ─────────────────
  const origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(method, url, ...rest) {
    this._qdmUrl = typeof url === 'string' ? url : String(url);
    return origOpen.call(this, method, url, ...rest);
  };

  const origSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function(...args) {
    const url = this._qdmUrl || '';
    if (url && (url.includes('.m3u8') || url.includes('.mpd') || url.includes('.f4m'))) {
      this.addEventListener('load', function() {
        const ct = this.getResponseHeader('content-type') || '';
        send({ action: 'page-media', url, mimeType: ct, title: document.title, tabUrl: location.href });
      }, { once: true });
    }
    return origSend.apply(this, args);
  };

  // ── Scan existing video/audio elements ───────────────────────────────────────
  function scanElements() {
    document.querySelectorAll('video[src], audio[src]').forEach(el => {
      const src = el.src || el.currentSrc;
      if (src && !src.startsWith('blob:') && !src.startsWith('data:')) {
        send({ action: 'page-media', url: src, title: document.title, tabUrl: location.href });
      }
    });
  }

  if (document.readyState !== 'loading') {
    scanElements();
  } else {
    document.addEventListener('DOMContentLoaded', scanElements, { once: true });
  }
})();
