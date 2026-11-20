import { bridge } from '../bridge';
import type { UpdateCheckResult, VersionHistoryEntry } from '../types';

export const APP_VERSION = '4.0.0';
export const APP_CODENAME = 'MonWord 4.0 Enterprise';
export const GITHUB_REPO_SLUG = 'lequan2512/monword';
export const GITHUB_RELEASES_URL = `https://github.com/${GITHUB_REPO_SLUG}/releases/latest`;
export const LIVE_WEB_OTA_URL = 'https://monword.vercel.app';
export const WEB_AUTO_UPDATED_SESSION_KEY = 'monword_web_auto_updated_ver';
export const MOBILE_OTA_ENABLED_KEY = 'monword_mobile_ota_enabled';

export function normalizeVersion(v: string): string {
  return (v || '').trim().replace(/^v/i, '');
}

export function compareSemver(a: string, b: string): number {
  const pa = normalizeVersion(a)
    .split(/[.-]/)
    .map((n) => parseInt(n, 10) || 0);
  const pb = normalizeVersion(b)
    .split(/[.-]/)
    .map((n) => parseInt(n, 10) || 0);
  const len = Math.max(pa.length, pb.length, 3);
  for (let i = 0; i < len; i++) {
    const na = pa[i] || 0;
    const nb = pb[i] || 0;
    if (na > nb) return 1;
    if (na < nb) return -1;
  }
  return 0;
}

export async function checkSoftwareUpdate(): Promise<UpdateCheckResult> {
  if (bridge.isNativeHost()) {
    try {
      return await bridge.invoke<UpdateCheckResult>('check_ota_update');
    } catch {
      // fallback to browser fetch
    }
  }

  const currentVersion = APP_VERSION;
  let latestVersion = currentVersion;
  let codename = APP_CODENAME;
  let releaseDate = '19/11/2026';
  let releaseNotes: string[] = [];
  let downloadUrl = GITHUB_RELEASES_URL;
  let history: VersionHistoryEntry[] = [];
  let source: string | undefined;
  let offline = true;

  const candidates = [`./version.json?t=${Date.now()}`, `${LIVE_WEB_OTA_URL}/version.json?t=${Date.now()}`];

  for (const url of candidates) {
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 5000);
      const res = await fetch(url, { cache: 'no-store', signal: controller.signal });
      clearTimeout(timer);
      if (res.ok) {
        const data = await res.json();
        offline = false;
        source = url.split('?')[0];
        if (data?.version && compareSemver(data.version, latestVersion) >= 0) {
          latestVersion = normalizeVersion(data.version);
          if (data.codename) codename = data.codename;
          if (data.releaseDate) releaseDate = data.releaseDate;
          if (Array.isArray(data.releaseNotes)) releaseNotes = data.releaseNotes;
          if (data.downloadUrl) downloadUrl = data.downloadUrl;
          if (Array.isArray(data.history)) history = data.history;
          break;
        }
      }
    } catch {
      // ignore unreachable origin and try next
    }
  }

  return {
    currentVersion,
    latestVersion,
    codename,
    hasUpdate: compareSemver(latestVersion, currentVersion) > 0,
    releaseDate,
    releaseNotes,
    downloadUrl,
    isDirectExe: false,
    otaActive: false,
    history,
    source,
    offline,
  };
}

export async function purgeWebCachesSilently(): Promise<void> {
  try {
    if (typeof window !== 'undefined' && 'caches' in window) {
      const keys = await window.caches.keys();
      await Promise.all(keys.map((k) => window.caches.delete(k)));
    }
    if (typeof navigator !== 'undefined' && 'serviceWorker' in navigator) {
      const regs = await navigator.serviceWorker.getRegistrations();
      await Promise.all(regs.map((r) => r.unregister()));
    }
  } catch {
    // ignore
  }
}

export async function applyWebUpdate(targetVersion?: string): Promise<void> {
  try {
    if (targetVersion && typeof sessionStorage !== 'undefined') {
      sessionStorage.setItem(WEB_AUTO_UPDATED_SESSION_KEY, normalizeVersion(targetVersion));
    }
  } catch {
    // ignore
  }
  await purgeWebCachesSilently();
  try {
    await fetch(window.location.pathname || '/', { cache: 'reload' });
  } catch {
    // ignore
  }
  const url = new URL(window.location.href);
  url.searchParams.set('v', String(Date.now()));
  window.location.replace(url.toString());
}

export async function applyMobileOtaUpdate(targetVersion?: string): Promise<void> {
  try {
    if (targetVersion) {
      localStorage.setItem('monword_mobile_ota_ver', normalizeVersion(targetVersion));
    }
    localStorage.setItem(MOBILE_OTA_ENABLED_KEY, '1');
  } catch {
    // ignore
  }
  await purgeWebCachesSilently();
  const isLocalHost =
    window.location.protocol === 'capacitor:' ||
    window.location.hostname === 'localhost' ||
    window.location.hostname === '127.0.0.1';
  if (isLocalHost) {
    window.location.replace(`${LIVE_WEB_OTA_URL}/?ota=1&v=${Date.now()}`);
  } else {
    const url = new URL(window.location.href);
    url.searchParams.set('ota', String(Date.now()));
    window.location.replace(url.toString());
  }
}
