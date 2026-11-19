import React, { useEffect, useState } from 'react';
import { Clock, Loader2, RefreshCw } from 'lucide-react';
import type { ProcessConfig, ProgressPayload, UpdateCheckResult } from '../types';

function formatClock(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

interface StatusBarProps {
  fileCount: number;
  config: ProcessConfig;
  activePresetLabel: string;
  busy: boolean;
  progress: ProgressPayload | null;
  currentVersion: string;
  updateInfo: UpdateCheckResult | null;
  onOpenUpdate: () => void;
  onOpenAbout: () => void;
}

export const StatusBar: React.FC<StatusBarProps> = ({
  fileCount,
  config,
  activePresetLabel,
  busy,
  progress,
  currentVersion,
  updateInfo,
  onOpenUpdate,
  onOpenAbout,
}) => {
  const [nowStr, setNowStr] = useState(() => formatClock(new Date()));

  useEffect(() => {
    const id = setInterval(() => setNowStr(formatClock(new Date())), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <div className="h-6 shrink-0 px-3 border-t border-slate-200 dark:border-slate-800 bg-slate-100/95 dark:bg-slate-900 flex items-center justify-between text-[11px] text-slate-600 dark:text-slate-400 select-none gap-2 whitespace-nowrap">
      <div className="flex items-center gap-2 min-w-0">
        <span>
          {fileCount > 0 ? `Hàng đợi: ${fileCount} tệp` : 'Sẵn sàng'}
        </span>
        <span className="text-slate-300 dark:text-slate-700">|</span>
        <span className="truncate hidden sm:inline">Bộ chuẩn: {activePresetLabel}</span>
        <span className="hidden md:inline text-slate-300 dark:text-slate-700">|</span>
        <span className="hidden md:inline">
          {config.format.paperSize.toUpperCase()} · Lề {config.format.marginTopCm}-{config.format.marginBottomCm}-
          {config.format.marginLeftCm}-{config.format.marginRightCm} cm
        </span>
      </div>

      <div className="flex items-center gap-1.5 text-blue-600 dark:text-blue-400 font-medium truncate">
        {busy && (
          <>
            <Loader2 className="w-3 h-3 animate-spin shrink-0" />
            <span>
              {progress
                ? `${progress.phase === 'audit' ? 'Đang quét' : 'Đang chuẩn hóa'} ${progress.current}/${progress.total}`
                : 'Đang xử lý tài liệu...'}
            </span>
          </>
        )}
      </div>

      <div className="flex items-center gap-2.5 shrink-0">
        <button
          type="button"
          onClick={onOpenUpdate}
          className={`inline-flex items-center gap-1 font-semibold hover:underline cursor-pointer ${
            updateInfo?.hasUpdate ? 'text-emerald-600 dark:text-emerald-400' : 'text-blue-600 dark:text-blue-400'
          }`}
          title="Mở Trung tâm Live OTA"
        >
          <RefreshCw className="w-2.5 h-2.5" />
          <span>{updateInfo?.hasUpdate ? `Cập nhật v${updateInfo.latestVersion}` : `Live OTA v${currentVersion}`}</span>
        </button>

        <span className="text-slate-300 dark:text-slate-700 hidden sm:inline">|</span>
        <button
          type="button"
          onClick={onOpenAbout}
          className="hidden sm:inline hover:text-slate-900 dark:hover:text-slate-200 cursor-pointer"
        >
          Hệ sinh thái Mon
        </button>
        <span className="text-slate-300 dark:text-slate-700">|</span>
        <div className="inline-flex items-center gap-1 tabular-nums font-medium text-slate-700 dark:text-slate-300">
          <Clock className="w-3 h-3 text-blue-600 dark:text-blue-400" />
          <span>{nowStr}</span>
        </div>
      </div>
    </div>
  );
};
