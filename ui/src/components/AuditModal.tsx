import React from 'react';
import { AlertCircle, AlertTriangle, Info, ShieldAlert, X } from 'lucide-react';
import { AuditReport } from '../types';

interface Props {
  report: AuditReport | null;
  onClose: () => void;
}

export const AuditModal: React.FC<Props> = ({ report, onClose }) => {
  if (!report) return null;

  const scoreColor =
    report.score >= 85
      ? 'text-emerald-600 dark:text-emerald-400'
      : report.score >= 60
      ? 'text-amber-600 dark:text-amber-400'
      : 'text-rose-600 dark:text-rose-400';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/60 backdrop-blur-sm p-4">
      <div className="w-full max-w-xl rounded-2xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 shadow-2xl overflow-hidden">
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-900/80">
          <div>
            <h3 className="text-sm font-bold text-slate-900 dark:text-slate-100">
              Báo cáo Kiểm tra Thể thức & Bảo mật
            </h3>
            <p className="text-xs text-slate-500 dark:text-slate-400 truncate max-w-md">
              {report.fileName}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1.5 rounded-lg text-slate-500 hover:bg-slate-200 dark:hover:bg-slate-800"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-5 space-y-4 max-h-[70vh] overflow-y-auto">
          <div className="flex items-center justify-between rounded-xl p-4 bg-slate-50 dark:bg-slate-950/60 border border-slate-200 dark:border-slate-800">
            <div>
              <div className="text-xs uppercase tracking-wider font-semibold text-slate-500">
                Điểm tuân thủ NĐ30
              </div>
              <div className={`text-3xl font-extrabold ${scoreColor}`}>{report.score}/100</div>
            </div>
            <div className="grid grid-cols-3 gap-3 text-center text-xs">
              <div className="px-2.5 py-1.5 rounded-lg bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800">
                <div className="font-bold text-slate-800 dark:text-slate-100">{report.paragraphCount}</div>
                <div className="text-[11px] text-slate-500">Đoạn văn</div>
              </div>
              <div className="px-2.5 py-1.5 rounded-lg bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800">
                <div className="font-bold text-slate-800 dark:text-slate-100">{report.tableCount}</div>
                <div className="text-[11px] text-slate-500">Bảng</div>
              </div>
              <div className="px-2.5 py-1.5 rounded-lg bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800">
                <div className="font-bold text-slate-800 dark:text-slate-100">{report.sectionCount}</div>
                <div className="text-[11px] text-slate-500">Section</div>
              </div>
            </div>
          </div>

          <div className="flex flex-wrap gap-2 text-xs">
            {report.hasMacros && (
              <span className="inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-rose-100 text-rose-700 dark:bg-rose-950 dark:text-rose-300 font-medium">
                <ShieldAlert className="w-3.5 h-3.5" /> Chứa VBA Macro
              </span>
            )}
            {report.hasLegacyEncoding && (
              <span className="inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300 font-medium">
                Bảng mã cũ TCVN3 / VNI
              </span>
            )}
            {report.isProtected && (
              <span className="inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-indigo-100 text-indigo-700 dark:bg-indigo-950 dark:text-indigo-300 font-medium">
                Đang bật Restrict Editing
              </span>
            )}
          </div>

          <div className="space-y-2">
            <h4 className="text-xs font-semibold uppercase tracking-wider text-slate-500">
              Chi tiết phát hiện ({report.issues.length})
            </h4>
            {report.issues.length === 0 ? (
              <div className="p-4 rounded-xl bg-emerald-50 dark:bg-emerald-950/30 border border-emerald-200 dark:border-emerald-900 text-xs text-emerald-700 dark:text-emerald-300">
                Tuyệt vời! Không phát hiện lỗi định dạng hay rủi ro bảo mật đáng kể nào.
              </div>
            ) : (
              report.issues.map((issue, idx) => (
                <div
                  key={idx}
                  className="flex items-start gap-2.5 p-3 rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-950/40"
                >
                  {issue.severity === 'error' ? (
                    <AlertCircle className="w-4 h-4 text-rose-500 shrink-0 mt-0.5" />
                  ) : issue.severity === 'warning' ? (
                    <AlertTriangle className="w-4 h-4 text-amber-500 shrink-0 mt-0.5" />
                  ) : (
                    <Info className="w-4 h-4 text-sky-500 shrink-0 mt-0.5" />
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-xs font-semibold text-slate-800 dark:text-slate-200">
                        {issue.category}
                      </span>
                      <span className="text-[11px] font-mono px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300">
                        ×{issue.count}
                      </span>
                    </div>
                    <p className="text-xs text-slate-600 dark:text-slate-400 mt-0.5">
                      {issue.message}
                    </p>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        <div className="px-5 py-3 border-t border-slate-200 dark:border-slate-800 flex justify-end bg-slate-50 dark:bg-slate-900/80">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-xs font-semibold hover:opacity-90"
          >
            Đóng
          </button>
        </div>
      </div>
    </div>
  );
};
