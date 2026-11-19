import { useEffect, useState } from 'react';
import { bridge } from '../bridge';
import type { PlatformEdition, UILayoutOverride } from '../types';

const EDITION_STORAGE_KEY = 'monword_ui_edition_override';

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

  let edition: PlatformEdition = 'web';
  if (override !== 'auto') {
    edition = override;
  } else if (isNativeWin && width >= 640) {
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
