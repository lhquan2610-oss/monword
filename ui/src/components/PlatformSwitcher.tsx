import React from 'react';
import { Monitor, Globe, Smartphone, Sparkles, RefreshCw } from 'lucide-react';
import type { PlatformEdition, UILayoutOverride, UpdateCheckResult } from '../types';

interface PlatformSwitcherProps {
  edition: PlatformEdition;
  override: UILayoutOverride;
  onChangeOverride: (next: UILayoutOverride) => void;
  compact?: boolean;
}

export const PlatformSwitcher: React.FC<PlatformSwitcherProps> = ({
  edition,
  override,
  onChangeOverride,
  compact = false,
}) => {
  const items: { id: UILayoutOverride; label: string; icon: React.ReactNode; title: string }[] = [
    {
      id: 'auto',
      label: compact ? 'Auto' : 'Tự động',
      icon: <Sparkles className="w-3 h-3" />,
      title: `Tự nhận diện theo thiết bị (đang là: ${edition.toUpperCase()})`,
    },
    {
      id: 'win',
      label: 'Win',
      icon: <Monitor className="w-3 h-3" />,
      title: 'Giao diện Windows Desktop chuyên sâu 3 cột',
    },
    {
      id: 'web',
      label: 'Web',
      icon: <Globe className="w-3 h-3" />,
      title: 'Giao diện Web Trình duyệt (monword.vercel.app)',
    },
    {
      id: 'mobile',
      label: 'Mobile',
      icon: <Smartphone className="w-3 h-3" />,
      title: 'Giao diện Điện thoại & PWA',
    },
  ];

  return (
    <div
      className="inline-flex items-center gap-0.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-100 dark:bg-slate-800/90 p-0.5"
      role="group"
      aria-label="Chuyển giao diện theo phiên bản nền tảng"
    >
      {items.map((item) => {
        const active = override === item.id;
        return (
          <button
            key={item.id}
            type="button"
            title={item.title}
            onClick={() => onChangeOverride(item.id)}
            className={`inline-flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-semibold transition-all cursor-pointer ${
              active
                ? 'bg-white dark:bg-slate-900 text-blue-600 dark:text-blue-400 shadow-sm'
                : 'text-slate-600 dark:text-slate-300 hover:text-slate-900 dark:hover:text-white'
            }`}
          >
            {item.icon}
            <span className={compact && item.id === 'auto' ? 'hidden sm:inline' : ''}>
              {item.label}
            </span>
          </button>
        );
      })}
    </div>
  );
};

interface VersionPillProps {
  currentVersion: string;
  updateInfo: UpdateCheckResult | null;
  onOpenUpdate: () => void;
  editionLabel?: string;
}

export const VersionOtaPill: React.FC<VersionPillProps> = ({
  currentVersion,
  updateInfo,
  onOpenUpdate,
  editionLabel,
}) => {
  const hasNewUpdate = Boolean(updateInfo?.hasUpdate);
  const otaActive = Boolean(updateInfo?.otaActive);

  return (
    <button
      type="button"
      onClick={onOpenUpdate}
      title={
        hasNewUpdate
          ? `Có bản cập nhật mới v${updateInfo?.latestVersion}! Nhấp để cập nhật Live OTA`
          : `MonWord v${currentVersion} (${editionLabel || 'Live OTA sẵn sàng'}) · Nhấp để kiểm tra cập nhật`
      }
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-bold border transition-all cursor-pointer ${
        hasNewUpdate
          ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 border-emerald-500/40 hover:bg-emerald-500/25'
          : otaActive
            ? 'bg-blue-500/10 text-blue-700 dark:text-blue-300 border-blue-500/30 hover:bg-blue-500/20'
            : 'bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-300 border-slate-200 dark:border-slate-700 hover:bg-slate-200/70'
      }`}
    >
      <RefreshCw className={`w-2.5 h-2.5 ${hasNewUpdate ? 'text-emerald-600 dark:text-emerald-400' : ''}`} />
      <span>
        {hasNewUpdate
          ? `Cập nhật v${updateInfo?.latestVersion}`
          : `v${currentVersion}${otaActive ? ' · Live OTA' : ''}`}
      </span>
    </button>
  );
};
