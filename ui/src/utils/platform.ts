import { useEffect, useState } from 'react';
import { bridge } from '../bridge';
import type { PlatformEdition, UILayoutOverride } from '../types';

const EDITION_STORAGE_KEY = 'monword_ui_edition_override';
const ADMIN_TOOLS_STORAGE_KEY = 'monword_admin_tools';
export const DEFAULT_DOC_INFO_KEY = 'monword_default_doc_info_v1';

export interface DefaultDocInfo {
  defaultOrgProfileId: string;
  defaultSignerTitle: string;
  defaultSignerName: string;
}

export function loadDefaultDocInfo(): DefaultDocInfo {
  if (typeof window === 'undefined') {
    return { defaultOrgProfileId: '', defaultSignerTitle: '', defaultSignerName: '' };
  }
  try {
    const raw = localStorage.getItem(DEFAULT_DOC_INFO_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<DefaultDocInfo>;
      return {
        defaultOrgProfileId: parsed.defaultOrgProfileId ?? '',
        defaultSignerTitle: parsed.defaultSignerTitle ?? '',
        defaultSignerName: parsed.defaultSignerName ?? '',
      };
    }
  } catch {
    // ignore
  }
  return { defaultOrgProfileId: '', defaultSignerTitle: '', defaultSignerName: '' };
}

export function saveDefaultDocInfo(info: DefaultDocInfo): void {
  if (typeof window === 'undefined') return;
  try {
    localStorage.setItem(DEFAULT_DOC_INFO_KEY, JSON.stringify(info));
  } catch {
    // ignore
  }
}

export function isAdminToolsEnabled(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    const param = new URLSearchParams(window.location.search).get('admin');
    if (param === '1' || param === 'true') return true;
    if (param === '0' || param === 'false') return false;
    return localStorage.getItem(ADMIN_TOOLS_STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

export interface PlatformSnapshot {
  edition: PlatformEdition;
  override: UILayoutOverride;
  isNativeWin: boolean;
  isMobileHardware: boolean;
  label: string;
}

export function detectPlatformSnapshot(override: UILayoutOverride = 'auto'): PlatformSnapshot {
  const isNativeWin = bridge.isNativeHost();
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent || '' : '';
  const width = typeof window !== 'undefined' ? window.innerWidth || 1280 : 1280;
  const isAndroidApk =
    typeof window !== 'undefined' &&
    (Boolean((window as unknown as { Capacitor?: { isNativePlatform?: () => boolean } }).Capacitor?.isNativePlatform?.()) ||
      /Capacitor/i.test(ua));
  const isMobileUA = /Android|iPhone|iPad|iPod/i.test(ua);
  const isMobileHardware = isAndroidApk || isMobileUA || width < 768;

  const urlEdition = (() => {
    if (typeof window === 'undefined') return null;
    try {
      const param = new URLSearchParams(window.location.search).get('edition');
      if (param === 'win' || param === 'web' || param === 'mobile') {
        return param;
      }
    } catch {
      // ignore
    }
    return null;
  })();

  let edition: PlatformEdition = 'web';
  if (override !== 'auto') {
    edition = override;
  } else if (urlEdition) {
    edition = urlEdition;
  } else if (isNativeWin) {
    edition = 'win';
  } else if (isMobileHardware || width < 768) {
    edition = 'mobile';
  } else {
    edition = 'web';
  }

  const label =
    edition === 'win'
      ? isNativeWin
        ? 'Windows Desktop (.exe)'
        : 'Xem thử bản Windows'
      : edition === 'mobile'
        ? isAndroidApk
          ? 'Mobile App (Android)'
          : 'Mobile / PWA'
        : 'Web App (monword.vercel.app)';

  return {
    edition,
    override,
    isNativeWin,
    isMobileHardware,
    label,
  };
}

let cachedOverride: UILayoutOverride = (() => {
  try {
    const raw = localStorage.getItem(EDITION_STORAGE_KEY);
    if (raw === 'win' || raw === 'web' || raw === 'mobile' || raw === 'auto') {
      return raw;
    }
  } catch {
    // ignore
  }
  return 'auto';
})();

const listeners = new Set<() => void>();

export function setPlatformOverride(next: UILayoutOverride): void {
  cachedOverride = next;
  try {
    localStorage.setItem(EDITION_STORAGE_KEY, next);
  } catch {
    // ignore
  }
  listeners.forEach((fn) => fn());
}

export function usePlatformEdition(): PlatformSnapshot & {
  setOverride: (mode: UILayoutOverride) => void;
} {
  const [snap, setSnap] = useState<PlatformSnapshot>(() => detectPlatformSnapshot(cachedOverride));

  useEffect(() => {
    const update = () => setSnap(detectPlatformSnapshot(cachedOverride));
    listeners.add(update);
    window.addEventListener('resize', update, { passive: true });
    window.addEventListener('orientationchange', update, { passive: true });
    return () => {
      listeners.delete(update);
      window.removeEventListener('resize', update);
      window.removeEventListener('orientationchange', update);
    };
  }, []);

  return {
    ...snap,
    setOverride: setPlatformOverride,
  };
}
