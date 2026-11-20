import React from 'react';
import {
  CheckCircle2,
  Download,
  ExternalLink,
  FileSpreadsheet,
  FileText,
  FolderOpen,
  FolderPlus,
  Plus,
  RotateCcw,
  SearchCheck,
  Share2,
  Trash2,
  UploadCloud,
  XCircle,
} from 'lucide-react';
import { AuditReport, FileProcessResult, FileQueueItem } from '../types';

interface Props {
  files: FileQueueItem[];
  audits: Record<string, AuditReport>;
  results: Record<string, FileProcessResult>;
  isDragging: boolean;
  isBrowserMode?: boolean;
  onPickFiles: () => void;
  onPickFolder: () => void;
  onRemoveFile: (path: string) => void;
  onClearAll: () => void;
  onOpenReport: (report: AuditReport) => void;
  onOpenOutput: (path: string, reveal?: boolean) => void;
  onDownloadOutput?: (blobUrl: string, fileName: string) => void;
  onShareOutput?: (res: FileProcessResult) => void;
  onRestoreBackup: (originalPath: string, backupPath: string) => void;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export const FileQueuePanel: React.FC<Props> = ({
  files,
  audits,
  results,
  isDragging,
  isBrowserMode = false,
  onPickFiles,
  onPickFolder,
  onRemoveFile,
  onClearAll,
  onOpenReport,
  onOpenOutput,
  onDownloadOutput,
  onShareOutput,
  onRestoreBackup,
}) => {
  return (
    <div
      className={`mw-panel transition-colors flex flex-col min-h-[240px] max-h-[360px]${
        isDragging ? ' border-blue-500 bg-blue-50/70 dark:bg-blue-950/30 ring-2 ring-blue-400/50' : ''
      }`}
    >
      {/* Header Controls */}
      <div className="px-3 py-2.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          <span className="text-xs font-bold uppercase tracking-wider text-slate-600 dark:text-slate-300">
            Hàng đợi ({files.length})
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onPickFiles}
            className="mw-button mw-button-secondary"
          >
            <Plus className="w-3 h-3" /> Chọn File
          </button>
          {!isBrowserMode && (
            <button
              type="button"
              onClick={onPickFolder}
              className="mw-button mw-button-tertiary"
            >
              <FolderPlus className="w-3 h-3" /> Thư mục
            </button>
          )}
          {files.length > 0 && (
            <button
              type="button"
              onClick={onClearAll}
              title="Xóa toàn bộ hàng đợi"
              className="mw-button mw-button-danger mw-icon-button"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* File List / Dropzone */}
      <div className="flex-1 overflow-y-auto p-2.5 space-y-2">
        {files.length === 0 ? (
          <div className="h-full min-h-[165px] flex flex-col items-center justify-center mw-empty text-center px-4 py-6">
            <UploadCloud className="w-8 h-8 text-blue-500 mb-2" />
            <p className="text-xs font-semibold text-slate-700 dark:text-slate-200">
              Kéo thả File hoặc Thư mục vào đây
            </p>
            <p className="text-[11px] text-slate-500 dark:text-slate-400 mt-1">
              Hỗ trợ .docx, .docm, .doc, .xlsx, .xlsm (không giới hạn số lượng)
            </p>
          </div>
        ) : (
          files.map((item) => {
            const audit = audits[item.path];
            const res = results[item.path];
            const displayAudit = res?.auditAfter || audit;
            const isExcel = item.ext.startsWith('xls');

            return (
              <div
                key={item.path}
                className="group rounded-lg border border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-950/50 p-2 hover:border-blue-300 dark:hover:border-blue-700 transition-colors"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="flex items-start gap-2 min-w-0">
                    {isExcel ? (
                      <FileSpreadsheet className="w-4 h-4 text-emerald-600 shrink-0 mt-0.5" />
                    ) : (
                      <FileText className="w-4 h-4 text-blue-600 shrink-0 mt-0.5" />
                    )}
                    <div className="min-w-0">
                      <div className="text-xs font-semibold text-slate-800 dark:text-slate-100 truncate">
                        {item.name}
                      </div>
                      <div className="text-[10px] text-slate-500 dark:text-slate-400 truncate">
                        {formatSize(item.sizeBytes)} · .{item.ext}
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-1 shrink-0">
                    {displayAudit && (() => {
                      const errs = displayAudit.issues.filter((i) => i.severity === 'error').length;
                      const warns = displayAudit.issues.filter((i) => i.severity === 'warning').length;
                      const badgeCls =
                        errs > 0
                          ? 'bg-rose-100 text-rose-700 dark:bg-rose-950 dark:text-rose-300'
                          : warns > 0
                          ? 'bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300'
                          : 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300';
                      const label =
                        errs > 0 ? `${errs} lỗi` : warns > 0 ? `${warns} c/báo` : 'Đạt';
                      return (
                        <button
                          type="button"
                          onClick={() => onOpenReport(displayAudit)}
                          title="Xem chi tiết kiểm tra lỗi"
                          className={`inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] font-bold ${badgeCls}`}
                        >
                          <SearchCheck className="w-3 h-3" />
                          {label}
                        </button>
                      );
                    })()}

                    <button
                      type="button"
                      onClick={() => onRemoveFile(item.path)}
                      className="p-0.5 rounded text-slate-400 hover:text-rose-500 opacity-0 group-hover:opacity-100 transition-opacity"
                      title="Gỡ khỏi hàng đợi"
                    >
                      <XCircle className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                {/* Result status bar */}
                {res && (
                  <div className="mt-1.5 pt-1.5 border-t border-slate-200/80 dark:border-slate-800 text-[10px]">
                    {res.success ? (
                      <div className="space-y-1">
                        <div className="flex items-center justify-between text-emerald-700 dark:text-emerald-400 font-medium">
                          <span className="inline-flex items-center gap-1 truncate">
                            <CheckCircle2 className="w-3 h-3 shrink-0" />
                            {res.statsSummary || 'Đã chuẩn hóa xong'}
                          </span>
                        </div>
                        <div className="flex items-center gap-1.5">
                          {res.blobUrl && onDownloadOutput ? (
                            <>
                              <button
                                type="button"
                                onClick={() => onDownloadOutput(res.blobUrl!, res.outputName || res.outputPath)}
                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-blue-600 text-white hover:bg-blue-700 font-semibold"
                              >
                                <Download className="w-2.5 h-2.5" /> Tải về (.docx)
                              </button>
                              {res.outputBlob && onShareOutput && (
                                <button
                                  type="button"
                                  onClick={() => onShareOutput(res)}
                                  className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-emerald-50 dark:bg-emerald-950/60 text-emerald-700 dark:text-emerald-300 hover:underline font-medium"
                                >
                                  <Share2 className="w-2.5 h-2.5" /> Chia sẻ
                                </button>
                              )}
                            </>
                          ) : (
                            <>
                              <button
                                type="button"
                                onClick={() => onOpenOutput(res.outputPath, false)}
                                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-blue-50 dark:bg-blue-950/60 text-blue-700 dark:text-blue-300 hover:underline font-medium"
                              >
                                <ExternalLink className="w-2.5 h-2.5" /> Mở file
                              </button>
                              <button
                                type="button"
                                onClick={() => onOpenOutput(res.outputPath, true)}
                                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 hover:underline"
                              >
                                <FolderOpen className="w-2.5 h-2.5" /> Thư mục
                              </button>
                              {res.backupPath && (
                                <>
                                  <button
                                    type="button"
                                    onClick={() => onRestoreBackup(res.inputPath, res.backupPath!)}
                                    className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-amber-50 dark:bg-amber-950/50 text-amber-700 dark:text-amber-300 hover:underline font-medium"
                                    title="Khôi phục phiên bản ngay trước lần chỉnh sửa này"
                                  >
                                    <RotateCcw className="w-2.5 h-2.5" /> Hoàn tác
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => onRestoreBackup(res.inputPath, '__initial__')}
                                    className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 hover:underline font-medium"
                                    title="Khôi phục về bản gốc ban đầu (Initial Baseline)"
                                  >
                                    Bản gốc
                                  </button>
                                </>
                              )}
                            </>
                          )}
                        </div>
                      </div>
                    ) : (
                      <div className="text-rose-600 dark:text-rose-400 flex items-start gap-1">
                        <XCircle className="w-3 h-3 shrink-0 mt-0.5" />
                        <div className="min-w-0 space-y-0.5">
                          {res.errorCode && (
                            <span className="inline-block mr-1 px-1 py-0 rounded bg-rose-100 dark:bg-rose-950 text-[9px] font-mono font-bold">
                              {res.errorCode}
                            </span>
                          )}
                          <span>{res.error}</span>
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
