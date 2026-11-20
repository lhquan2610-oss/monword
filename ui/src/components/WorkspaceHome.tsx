import {
  ArrowRight,
  ExternalLink,
  FileCheck2,
  FileText,
  FolderOpen,
  FolderPlus,
  History,
  LayoutTemplate,
  SearchCheck,
} from 'lucide-react';
import type { FileQueueItem, NavTab, ProgressPayload, RecentDocEntry } from '../types';

const RECENT_STATUS_META: Record<
  RecentDocEntry['status'],
  { label: string; className: string }
> = {
  standardized: {
    label: 'Đã chuẩn hóa',
    className:
      'bg-emerald-100 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-300',
  },
  checked: {
    label: 'Đã kiểm tra',
    className:
      'bg-blue-100 text-blue-800 dark:bg-blue-950/60 dark:text-blue-300',
  },
  selected: {
    label: 'Đã chọn',
    className:
      'bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300',
  },
};

function formatRecentTime(iso: string): string {
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    return d.toLocaleString('vi-VN', {
      hour: '2-digit',
      minute: '2-digit',
      day: '2-digit',
      month: '2-digit',
    });
  } catch {
    return '';
  }
}

export interface WorkspaceHomeProps {
  files: FileQueueItem[];
  recentDocs?: RecentDocEntry[];
  onPickFiles: () => void;
  onPickFolder?: () => void;
  onOpenOutput?: (path: string, reveal?: boolean) => void;
  onTask: (tab: NavTab) => void;
  onShowFiles: () => void;
  progress: ProgressPayload | null;
  isDragging?: boolean;
  mobile?: boolean;
}

/**
 * Màn hình Trang chủ tối giản: ưu tiên tác vụ Chuẩn hóa văn bản ở vị trí trung tâm,
 * kèm tác vụ phụ (Kiểm tra văn bản, Mẫu văn bản) và danh sách Tài liệu gần đây.
 */
export function WorkspaceHome({
  files,
  recentDocs = [],
  onPickFiles,
  onPickFolder,
  onOpenOutput,
  onTask,
  onShowFiles,
  progress,
  isDragging,
  mobile,
}: WorkspaceHomeProps) {
  const recentPreview = recentDocs.slice(0, 4);
  const sessionPathSet = new Set(files.map((f) => f.path.toLowerCase()));

  return (
    <div className="mx-auto max-w-4xl space-y-5 py-2">
      <header className="space-y-1">
        <p className="text-xs font-semibold text-[var(--mw-accent)]">
          Hôm nay bạn muốn làm gì?
        </p>
        <h2
          id="monword-home-title"
          className="text-xl font-bold tracking-tight text-[var(--mw-ink)] sm:text-2xl"
        >
          Chuẩn hóa và kiểm tra văn bản Word
        </h2>
        <p className="text-sm text-[var(--mw-muted)]">
          Đưa văn bản Word về đúng phông chữ, cỡ chữ, lề trang và thể thức quy định.
        </p>
      </header>

      {/* 1. Tác vụ chính: Chuẩn hóa văn bản */}
      <section
        aria-labelledby="monword-hero-standardize"
        className="mw-panel border-[var(--mw-accent)]/35 p-5 sm:p-6 space-y-4 shadow-sm"
      >
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-start gap-3.5 min-w-0">
            <div className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-[var(--mw-tint)] text-[var(--mw-accent)]">
              <FileCheck2 className="size-6" aria-hidden="true" />
            </div>
            <div className="min-w-0 space-y-1">
              <h3
                id="monword-hero-standardize"
                className="text-lg font-bold text-[var(--mw-ink)]"
              >
                Chuẩn hóa văn bản
              </h3>
              <p className="text-xs sm:text-sm leading-5 text-[var(--mw-muted)]">
                Tự động sửa phông chữ, cỡ chữ, giãn dòng, căn lề và bố cục chuẩn quy định. Giữ nguyên nội dung và lưu thành tệp mới an toàn.
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2 shrink-0">
            <button
              type="button"
              onClick={() => onTask('standardizer')}
              className="mw-button mw-button-primary px-4 py-2 text-sm"
            >
              Chuẩn hóa văn bản
              <ArrowRight className="size-4" aria-hidden="true" />
            </button>
          </div>
        </div>

        {/* Vùng chọn / kéo thả tệp ngay trong tác vụ chính */}
        <div
          aria-label="Chọn hoặc kéo thả tài liệu"
          className={`mw-empty flex flex-wrap items-center justify-between gap-3 px-4 py-3${
            isDragging ? ' border-[var(--mw-focus)] bg-[var(--mw-tint)]' : ''
          }`}
        >
          <div className="flex items-center gap-2.5 min-w-0">
            <FolderOpen className="size-4 shrink-0 text-[var(--mw-muted)]" aria-hidden="true" />
            <p className="text-xs text-[var(--mw-muted)]">
              {mobile
                ? 'Chọn nhanh tệp Word (.docx, .docm) từ thiết bị để chuẩn hóa ngay'
                : onPickFolder
                  ? 'Kéo thả tài liệu Word (.docx, .docm, .doc) vào đây hoặc chọn từ máy'
                  : 'Kéo thả tài liệu Word (.docx, .docm) vào đây hoặc chọn từ máy'}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2 shrink-0">
            <button type="button" onClick={onPickFiles} className="mw-button mw-button-secondary">
              Chọn tài liệu từ máy
            </button>
            {onPickFolder && (
              <button type="button" onClick={onPickFolder} className="mw-button mw-button-secondary">
                <FolderPlus className="size-4" aria-hidden="true" /> Chọn thư mục
              </button>
            )}
          </div>
        </div>

        {files.length > 0 && (
          <div
            aria-labelledby="monword-session-title"
            className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-[var(--mw-border)] bg-[var(--mw-canvas)] px-3.5 py-2.5"
          >
            <div className="flex items-center gap-2 min-w-0">
              <FileText className="size-4 shrink-0 text-[var(--mw-accent)]" aria-hidden="true" />
              <div className="min-w-0">
                <span id="monword-session-title" className="text-xs font-bold text-[var(--mw-ink)]">
                  Đang chọn {files.length} tài liệu:{' '}
                </span>
                <span className="truncate text-xs text-[var(--mw-muted)]">
                  {files.slice(0, 2).map((f) => f.name).join(', ')}
                  {files.length > 2 ? ` và ${files.length - 2} tệp khác` : ''}
                </span>
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <button type="button" onClick={onShowFiles} className="mw-button mw-button-secondary text-xs">
                Xem danh sách
              </button>
              <button
                type="button"
                onClick={() => onTask('standardizer')}
                className="mw-button mw-button-primary text-xs"
              >
                Tiếp tục chuẩn hóa
                <ArrowRight className="size-3.5" aria-hidden="true" />
              </button>
            </div>
          </div>
        )}

        {progress && (
          <p role="status" className="text-xs font-semibold text-[var(--mw-accent)]">
            {progress.phase === 'audit' ? 'Đang kiểm tra' : 'Đang xử lý'} {progress.current}/{progress.total}
          </p>
        )}
      </section>

      {/* 2 & 3. Tác vụ phụ: Kiểm tra văn bản & Mẫu văn bản */}
      <section aria-label="Tác vụ bổ trợ" className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div className="mw-panel flex flex-col justify-between gap-3 p-4">
          <div className="flex items-start gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-[var(--mw-tint)] text-[var(--mw-accent)]">
              <SearchCheck className="size-4.5" aria-hidden="true" />
            </div>
            <div className="min-w-0 space-y-0.5">
              <h3 className="text-sm font-bold text-[var(--mw-ink)]">Kiểm tra văn bản</h3>
              <p className="text-xs leading-5 text-[var(--mw-muted)]">
                Rà soát lỗi thể thức và trình bày mà không làm thay đổi tệp gốc.
              </p>
            </div>
          </div>
          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => onTask('complianceAudit')}
              className="mw-button mw-button-secondary"
            >
              Kiểm tra văn bản
              <ArrowRight className="size-3.5" aria-hidden="true" />
            </button>
          </div>
        </div>

        <div className="mw-panel flex flex-col justify-between gap-3 p-4">
          <div className="flex items-start gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-[var(--mw-tint)] text-[var(--mw-accent)]">
              <LayoutTemplate className="size-4.5" aria-hidden="true" />
            </div>
            <div className="min-w-0 space-y-0.5">
              <h3 className="text-sm font-bold text-[var(--mw-ink)]">Mẫu văn bản</h3>
              <p className="text-xs leading-5 text-[var(--mw-muted)]">
                Tạo công văn, quyết định, tờ trình, báo cáo từ khung mẫu chuẩn.
              </p>
            </div>
          </div>
          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => onTask('templateEngine')}
              className="mw-button mw-button-secondary"
            >
              Mẫu văn bản
              <ArrowRight className="size-3.5" aria-hidden="true" />
            </button>
          </div>
        </div>
      </section>

      {/* 4. Tài liệu gần đây */}
      <section aria-labelledby="monword-recent-title" className="mw-panel overflow-hidden">
        <div className="flex items-center justify-between gap-2 border-b border-[var(--mw-border)] px-4 py-3">
          <div className="flex items-center gap-2">
            <History className="size-4 text-[var(--mw-accent)]" aria-hidden="true" />
            <h3 id="monword-recent-title" className="text-sm font-bold text-[var(--mw-ink)]">
              Tài liệu gần đây
            </h3>
          </div>
          <button
            type="button"
            onClick={() => onTask('history')}
            className="mw-button mw-button-secondary text-xs"
          >
            Xem tất cả
          </button>
        </div>

        {recentPreview.length === 0 ? (
          <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-4 text-xs text-[var(--mw-muted)]">
            <span>Chưa có tài liệu nào được xử lý gần đây.</span>
            <button
              type="button"
              onClick={() => onTask('history')}
              className="mw-button mw-button-secondary text-xs"
            >
              Mở lịch sử
            </button>
          </div>
        ) : (
          <div className="divide-y divide-[var(--mw-border)]">
            {recentPreview.map((doc) => {
              const badge =
                doc.operation === 'template'
                  ? {
                      label: 'Tạo từ mẫu',
                      className:
                        'bg-emerald-100 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-300',
                    }
                  : RECENT_STATUS_META[doc.status];
              const isWebPath = doc.path.startsWith('web://');
              const canOpenFile = !isWebPath && Boolean(doc.outputPath || doc.path) && Boolean(onOpenOutput);
              const inSession = sessionPathSet.has(doc.path.toLowerCase());
              const timeLabel = formatRecentTime(doc.updatedAt);

              return (
                <div
                  key={`${doc.path}::${doc.operation || doc.status}`}
                  className="flex flex-wrap items-center justify-between gap-3 px-4 py-3"
                >
                  <div className="flex items-center gap-2.5 min-w-0 flex-1 basis-56">
                    <FileText className="size-4 shrink-0 text-[var(--mw-accent)]" aria-hidden="true" />
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="truncate text-xs sm:text-sm font-semibold text-[var(--mw-ink)]">
                          {doc.name}
                        </span>
                        <span className={`rounded-full px-2 py-0.5 text-[10px] font-bold ${badge.className}`}>
                          {badge.label}
                        </span>
                      </div>
                      <p className="truncate text-[11px] text-[var(--mw-muted)]">
                        {timeLabel ? `${timeLabel}` : ''}
                        {doc.summary && !doc.summary.includes('/100')
                          ? `${timeLabel ? ' · ' : ''}${doc.summary}`
                          : ''}
                      </p>
                    </div>
                  </div>

                  <div className="flex items-center gap-2 shrink-0">
                    {canOpenFile && (
                      <button
                        type="button"
                        onClick={() => onOpenOutput?.(doc.outputPath || doc.path)}
                        className="mw-button mw-button-secondary text-xs"
                      >
                        <ExternalLink className="size-3.5" aria-hidden="true" />
                        Mở tệp
                      </button>
                    )}
                    {inSession ? (
                      <button
                        type="button"
                        onClick={() => onTask('standardizer')}
                        className="mw-button mw-button-secondary text-xs"
                      >
                        Chuẩn hóa
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => onTask('history')}
                        className="mw-button mw-button-secondary text-xs"
                      >
                        Xem lịch sử
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}
