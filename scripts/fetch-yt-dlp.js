#!/usr/bin/env node
/**
 * Downloads the latest yt-dlp binary into src-tauri/resources/
 * so it can be bundled inside the QDM installer.
 *
 * Run manually: node scripts/fetch-yt-dlp.js
 * Or via npm:   npm run fetch-yt-dlp
 */

const https = require('https');
const fs = require('fs');
const path = require('path');

const RESOURCES_DIR = path.join(__dirname, '..', 'src-tauri', 'resources');
const IS_WIN = process.platform === 'win32';
const IS_MAC = process.platform === 'darwin';

const DOWNLOAD_URL = IS_WIN
  ? 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe'
  : IS_MAC
  ? 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos'
  : 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp';

const OUTPUT_FILE = path.join(RESOURCES_DIR, IS_WIN ? 'yt-dlp.exe' : 'yt-dlp');

function download(url, dest, redirects = 0) {
  if (redirects > 10) { console.error('Too many redirects'); process.exit(1); }
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { 'User-Agent': 'QDM-fetch-yt-dlp/1.0' } }, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302 || res.statusCode === 307 || res.statusCode === 308) {
        res.resume();
        return download(res.headers.location, dest, redirects + 1).then(resolve).catch(reject);
      }
      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode} fetching ${url}`));
        return;
      }
      const total = parseInt(res.headers['content-length'] || '0', 10);
      let received = 0;
      const file = fs.createWriteStream(dest);
      res.on('data', (chunk) => {
        received += chunk.length;
        if (total > 0) {
          const pct = Math.floor((received / total) * 100);
          process.stdout.write(`\r  Downloading yt-dlp... ${pct}% (${(received / 1e6).toFixed(1)} MB)`);
        }
      });
      res.pipe(file);
      file.on('finish', () => { file.close(); resolve(); });
      file.on('error', reject);
    }).on('error', reject);
  });
}

(async () => {
  fs.mkdirSync(RESOURCES_DIR, { recursive: true });

  if (fs.existsSync(OUTPUT_FILE)) {
    const size = fs.statSync(OUTPUT_FILE).size;
    if (size > 1_000_000) {
      console.log(`yt-dlp already present (${(size / 1e6).toFixed(1)} MB), skipping download.`);
      console.log('  Delete src-tauri/resources/yt-dlp[.exe] to force re-download.');
      process.exit(0);
    }
  }

  console.log(`Fetching latest yt-dlp for ${process.platform}...`);
  console.log(`  URL: ${DOWNLOAD_URL}`);
  await download(DOWNLOAD_URL, OUTPUT_FILE);
  console.log(`\n  Saved to: ${OUTPUT_FILE}`);

  if (!IS_WIN) {
    fs.chmodSync(OUTPUT_FILE, 0o755);
    console.log('  chmod +x applied.');
  }

  console.log('Done.');
})().catch(e => { console.error(e.message); process.exit(1); });
