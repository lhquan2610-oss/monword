import { Dialog } from './Dialog';
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
    ? 'Bản Windows Desktop (.exe)'
    : edition === 'mobile'
      ? 'Bản Điện thoại (Mobile Web / PWA)'
      : 'Bản Trình duyệt Web (monword.vercel.app)';

  return (
    <Dialog label="Giới thiệu MonWord" widthClass="max-w-md" onClose={onClose}>
      <div className="p-6 text-center">
        <div className="flex justify-end -mt-2 -mr-2">
          <button
            type="button"
            onClick={onClose}
            aria-label="Đóng hộp thoại"
            className="mw-button mw-button-tertiary mw-icon-button"
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
        <div className="text-[11px] font-medium text-slate-600 dark:text-slate-300 mb-0.5">
          Nhà phát hành: Hệ sinh thái Văn phòng Mon (Mon Office Ecosystem)
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
          <div>• Bản Windows hỗ trợ đầy đủ tệp .docx, .docm, .doc và tự động sao lưu bản gốc trước khi xử lý.</div>
          <div>• Bản Web và Điện thoại xử lý trực tiếp tệp .docx, .docm ngay trên trình duyệt.</div>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => {
              onClose();
              onOpenUpdate();
            }}
            className="mw-button mw-button-primary flex-1"
          >
            <RefreshCw className="w-3.5 h-3.5" /> Kiểm tra cập nhật
          </button>
          <button
            type="button"
            onClick={onClose}
            className="mw-button mw-button-secondary flex-1"
          >
            Đóng
          </button>
        </div>
      </div>
    </Dialog>
  );
};
