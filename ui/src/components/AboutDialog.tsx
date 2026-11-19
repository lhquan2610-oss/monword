import React from 'react';
import { RefreshCw, ShieldCheck, X } from 'lucide-react';
import type { PlatformEdition } from '../types';
import { APP_CODENAME, APP_VERSION } from '../utils/updater';

interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
  onOpenUpdate: () => void;
  edition: PlatformEdition;
  isNativeWin: boolean;
  currentVersion: string;
}

export const AboutDialog: React.FC<AboutDialogProps> = ({
  open,
  onClose,
  onOpenUpdate,
  edition,
  isNativeWin,
  currentVersion,
}) => {
  if (!open) return null;

  const platformName = isNativeWin
    ? 'Windows x64 (Rust + WebView2 · Hỗ trợ Live OTA)'
    : edition === 'mobile'
      ? 'Mobile Web / PWA (Hỗ trợ Live OTA)'
      : 'Web Edition (monword.vercel.app)';

  return (
    <div className="fixed inset-0 z-50 bg-black/55 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl max-w-md w-full p-6 text-center">
        <div className="flex justify-end -mt-2 -mr-2">
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-slate-200"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="mx-auto w-28 h-28 flex items-center justify-center mb-2">
          <img
            src="/monword-logo.png"
            alt="MonWord Logo"
            className="w-full h-full object-contain drop-shadow-sm select-none"
          />
        </div>

        <div className="text-xs font-bold text-blue-600 dark:text-blue-400 mb-1">
          Phiên bản v{currentVersion || APP_VERSION} · {APP_CODENAME}
        </div>
        <div className="text-[11px] text-slate-500 dark:text-slate-400 mb-3">{platformName}</div>

        <p className="text-xs text-slate-600 dark:text-slate-300 leading-relaxed mb-4">
          Ứng dụng chuẩn hóa thể thức văn bản hành chính Nghị định 30/2020/NĐ-CP &amp; Văn bản Đảng thuộc{' '}
          <strong>Hệ sinh thái Văn phòng Mon</strong> (MonPDF · MonWord · MonEx).
        </p>

        <div className="rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/60 p-3 text-left text-[11px] text-slate-600 dark:text-slate-400 space-y-1 mb-4">
          <div className="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400 font-bold mb-1">
            <ShieldCheck className="w-4 h-4" />
            <span>Tiêu chuẩn bảo mật cục bộ 100%</span>
          </div>
          <div>• Xử lý hoàn toàn trên máy tính hoặc trình duyệt người dùng, không gửi file lên server.</div>
          <div>• Bản Windows hỗ trợ lõi Rust OpenXML + Word COM Hybrid, sao lưu tự động `.chuanhoa_backup`.</div>
          <div>• Bản Web/Mobile xử lý trực tiếp `.docx`/`.docm` tại trình duyệt &amp; cập nhật Live OTA.</div>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => {
              onClose();
              onOpenUpdate();
            }}
            className="flex-1 py-2 rounded-xl bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-semibold flex items-center justify-center gap-1.5"
          >
            <RefreshCw className="w-3.5 h-3.5" /> Kiểm tra cập nhật
          </button>
          <button
            type="button"
            onClick={onClose}
            className="flex-1 py-2 rounded-xl bg-blue-600 hover:bg-blue-700 text-white text-xs font-semibold"
          >
            Đóng
          </button>
        </div>
      </div>
    </div>
  );
};
