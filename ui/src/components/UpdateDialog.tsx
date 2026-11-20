import { Dialog } from './Dialog';
import React, { useCallback, useEffect, useState } from 'react';
import {
  ArrowUpCircle,
  BellRing,
  CheckCircle2,
  Download,
  ExternalLink,
  Loader2,
  RefreshCw,
  RotateCcw,
  Sparkles,
  WifiOff,
  X,
} from 'lucide-react';
import { bridge } from '../bridge';
import type { PlatformEdition, UpdateCheckResult, UpdateProgressPayload } from '../types';
import {
  APP_CODENAME,
  APP_VERSION,
  GITHUB_RELEASES_URL,
  applyMobileOtaUpdate,
  applyWebUpdate,
  checkSoftwareUpdate,
} from '../utils/updater';

interface UpdateDialogProps {
  open: boolean;
  onClose: () => void;
  edition: PlatformEdition;
  isNativeWin: boolean;
  currentVersion: string;
  initialResult: UpdateCheckResult | null;
  onUpdateChecked: (res: UpdateCheckResult) => void;
  onToast: (type: 'ok' | 'err', text: string) => void;
  progress: UpdateProgressPayload | null;
}

export const UpdateDialog: React.FC<UpdateDialogProps> = ({
  open,
  onClose,
  edition,
  isNativeWin,
  currentVersion,
  initialResult,
  onUpdateChecked,
  onToast,
  progress,
}) => {
  const [checking, setChecking] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [result, setResult] = useState<UpdateCheckResult | null>(initialResult);

  useEffect(() => {
    setResult(initialResult);
  }, [initialResult]);

  const runCheck = useCallback(
    async (notify = false) => {
      setChecking(true);
      try {
        const res = await checkSoftwareUpdate();
        setResult(res);
        onUpdateChecked(res);
        if (notify) {
          if (res.offline) {
            onToast('ok', `Đang ở v${res.currentVersion}. Máy chủ Live OTA (monword.vercel.app) chưa phản hồi hoặc đang ngoại tuyến.`);
          } else if (res.hasUpdate) {
            onToast('ok', `Đã phát hiện phiên bản mới MonWord v${res.latestVersion}!`);
          } else {
            onToast('ok', `Bạn đang dùng phiên bản mới nhất MonWord v${res.currentVersion}.`);
          }
        }
      } catch (e) {
        if (notify) onToast('err', `Lỗi kiểm tra cập nhật: ${(e as Error).message}`);
      } finally {
        setChecking(false);
      }
    },
    [onUpdateChecked, onToast],
  );

  useEffect(() => {
    if (open) {
      void runCheck(false);
    } else {
      setUpdating(false);
    }
  }, [open, runCheck]);

  if (!open) return null;

  const curVer = result?.currentVersion || currentVersion || APP_VERSION;
  const latVer = result?.latestVersion || curVer;
  const hasUpdate = Boolean(result?.hasUpdate);

  const handleApplyLiveOta = async () => {
    setUpdating(true);
    try {
      if (isNativeWin) {
        const res = await bridge.invoke<{ version?: string }>('apply_live_ota');
        onToast('ok', `Đã đồng bộ Live OTA v${res?.version || latVer}! Đang tự động nạp lại giao diện...`);
      } else if (edition === 'mobile') {
        onToast('ok', 'Đang kích hoạt Live OTA trên Mobile (không cần cài lại app)...');
        await applyMobileOtaUpdate(latVer);
      } else {
        onToast('ok', 'Đang làm mới Cache và đồng bộ Web App...');
        await applyWebUpdate(latVer);
      }
    } catch (e) {
      onToast('err', `Cập nhật Live OTA thất bại: ${(e as Error).message}`);
      setUpdating(false);
    }
  };

  const handleResetDesktopOta = async () => {
    try {
      await bridge.invoke('reset_ota_cache');
      onToast('ok', 'Đã khôi phục về gói giao diện gốc đóng gói trong MonWord.exe.');
      window.location.reload();
    } catch (e) {
      onToast('err', (e as Error).message);
    }
  };

  const handleOpenReleases = async () => {
    const url = result?.downloadUrl || GITHUB_RELEASES_URL;
    if (isNativeWin) {
      await bridge.invoke('open_external_url', { url }).catch(() => window.open(url, '_blank'));
    } else {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  };

  return (
    <Dialog label="Trung tâm Phiên bản & Live OTA" widthClass="max-w-lg" onClose={onClose}>
      <div>
        {/* Header */}
        <div
          className={`px-5 py-3.5 border-b flex items-center justify-between ${
            hasUpdate
              ? 'mw-warning border-current'
              : 'bg-slate-50 dark:bg-slate-800/70 border-slate-200 dark:border-slate-800'
          }`}
        >
          <div className="flex items-center gap-2">
            {hasUpdate ? (
              <BellRing className="w-5 h-5 text-amber-300" />
            ) : (
              <ArrowUpCircle className="w-5 h-5 text-blue-600 dark:text-blue-400" />
            )}
            <div>
              <h2 className="text-sm font-bold leading-tight">
                {hasUpdate ? `Có bản cập nhật mới MonWord v${latVer}!` : 'Trung tâm Phiên bản & Live OTA'}
              </h2>
              <p className={`text-[11px] ${hasUpdate ? 'text-current' : 'text-slate-500 dark:text-slate-400'}`}>
                {result?.codename || APP_CODENAME}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Đóng hộp thoại"
            className="mw-button mw-button-tertiary mw-icon-button"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="p-5 space-y-4 max-h-[75vh] overflow-y-auto">
          <div
            className={`p-4 rounded-xl border ${
              hasUpdate
                ? 'bg-blue-50/80 dark:bg-blue-950/30 border-blue-200 dark:border-blue-800'
                : 'bg-emerald-50/70 dark:bg-emerald-950/25 border-emerald-200 dark:border-emerald-900'
            }`}
          >
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-3 min-w-0">
                <div
                  className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 ${
                    checking
                      ? 'bg-blue-100 dark:bg-blue-900/50 text-blue-600'
                      : hasUpdate
                        ? 'bg-blue-600 text-white'
                        : 'bg-emerald-100 dark:bg-emerald-900/50 text-emerald-600'
                  }`}
                >
                  {checking ? (
                    <Loader2 className="w-5 h-5 animate-spin" />
                  ) : hasUpdate ? (
                    <Sparkles className="w-5 h-5" />
                  ) : (
                    <CheckCircle2 className="w-5 h-5" />
                  )}
                </div>

                <div className="min-w-0">
                  <div className="text-xs font-bold text-slate-900 dark:text-slate-100 truncate">
                    {checking
                      ? 'Đang kết nối máy chủ monword.vercel.app...'
                      : hasUpdate
                        ? `MonWord v${latVer} đã sẵn sàng cập nhật`
                        : `Đang dùng bản mới nhất (v${curVer})`}
                  </div>
                  <div className="text-[11px] text-slate-600 dark:text-slate-400 mt-0.5 flex flex-wrap items-center gap-1.5">
                    <span>Hiện tại: <strong>v{curVer}</strong></span>
                    <span>•</span>
                    <span>Mới nhất: <strong className="text-blue-600 dark:text-blue-400">v{latVer}</strong></span>
                    {result?.offline && (
                      <span className="inline-flex items-center gap-1 text-amber-600 dark:text-amber-400 font-medium">
                        <WifiOff className="w-3 h-3" /> Ngoại tuyến / Chưa publish
                      </span>
                    )}
                  </div>
                </div>
              </div>

              <button
                type="button"
                disabled={checking || updating}
                onClick={() => void runCheck(true)}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:bg-slate-100 text-xs font-semibold shrink-0 disabled:opacity-50"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${checking ? 'animate-spin text-blue-500' : ''}`} />
                Kiểm tra
              </button>
            </div>

            {updating && progress && (
              <div className="mt-3 pt-3 border-t border-blue-200/70 dark:border-blue-800/50 space-y-1.5">
                <div className="flex justify-between text-[11px] font-medium text-blue-700 dark:text-blue-300">
                  <span>{progress.status}</span>
                  <span>{progress.percent}%</span>
                </div>
                <div className="w-full h-2 rounded-full bg-blue-200 dark:bg-blue-950 overflow-hidden">
                  <div
                    className="h-full bg-blue-600 transition-all duration-200 rounded-full"
                    style={{ width: `${Math.max(8, progress.percent)}%` }}
                  />
                </div>
              </div>
            )}

            <div className="mt-3 pt-3 border-t border-slate-200/70 dark:border-slate-700/60 flex flex-wrap items-center justify-between gap-2">
              <span className="text-[11px] text-slate-500 dark:text-slate-400">
                Nền tảng: <strong>{isNativeWin ? 'Windows Desktop (.exe)' : edition === 'mobile' ? 'Mobile (PWA/APK)' : 'Web (monword.vercel.app)'}</strong>
              </span>

              <button
                type="button"
                disabled={updating}
                onClick={() => void handleApplyLiveOta()}
                className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-700 text-white text-xs font-semibold shadow-sm disabled:opacity-50"
              >
                {updating ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Download className="w-3.5 h-3.5" />}
                <span>
                  {isNativeWin
                    ? hasUpdate
                      ? `Cập nhật Live OTA v${latVer}`
                      : 'Đồng bộ Live OTA mới nhất'
                    : edition === 'mobile'
                      ? 'Cập nhật Mobile Live OTA'
                      : 'Làm mới & Cập nhật Web'}
                </span>
              </button>
            </div>
          </div>

          {result?.releaseNotes && result.releaseNotes.length > 0 && (
            <div className="rounded-xl border border-slate-200 dark:border-slate-800 p-3.5 bg-slate-50/70 dark:bg-slate-950/40 space-y-1.5">
              <div className="text-xs font-bold text-slate-800 dark:text-slate-200">
                Ghi chú phát hành v{latVer} ({result.releaseDate})
              </div>
              <ul className="space-y-1 text-[11px] text-slate-600 dark:text-slate-400 list-disc pl-4">
                {result.releaseNotes.map((note, idx) => (
                  <li key={idx}>{note}</li>
                ))}
              </ul>
            </div>
          )}

          <div className="rounded-xl border border-slate-200 dark:border-slate-800 p-3.5 text-[11px] text-slate-600 dark:text-slate-400 space-y-1">
            <div className="font-semibold text-slate-800 dark:text-slate-200">Cách Live OTA hoạt động trên 3 phiên bản:</div>
            <div>• <strong>Windows (.exe):</strong> tải gói giao diện &amp; bộ chuẩn mới từ <span className="font-mono text-blue-600">monword.vercel.app</span> vào <span className="font-mono">%LOCALAPPDATA%\MonWord\ota-dist</span> mà không cần tải lại file `.exe`.</div>
            <div>• <strong>Web:</strong> tự dọn Cache Storage / Service Worker khi phát hiện <span className="font-mono">version.json</span> mới.</div>
            <div>• <strong>Mobile:</strong> đồng bộ thẳng giao diện cloud mới nhất không cần cài lại ứng dụng.</div>
          </div>
        </div>

        {/* Footer */}
        <div className="px-5 py-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-900 flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            {isNativeWin && result?.otaActive && (
              <button
                type="button"
                onClick={() => void handleResetDesktopOta()}
                className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg border border-amber-300 dark:border-amber-800 text-amber-700 dark:text-amber-300 text-xs font-semibold hover:bg-amber-50"
              >
                <RotateCcw className="w-3.5 h-3.5" /> Khôi phục bản gốc EXE
              </button>
            )}
            <button
              type="button"
              onClick={() => void handleOpenReleases()}
              className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 text-slate-600 dark:text-slate-300 text-xs font-semibold hover:bg-slate-100"
            >
              <ExternalLink className="w-3.5 h-3.5" /> GitHub Releases
            </button>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="mw-button mw-button-secondary"
          >
            Đóng
          </button>
        </div>
      </div>
    </Dialog>
  );
};
