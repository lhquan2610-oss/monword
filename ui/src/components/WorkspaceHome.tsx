import {
  ArrowRight,
  FileCheck2,
  FileText,
  FolderOpen,
  FolderPlus,
  History,
  LayoutTemplate,
  SearchCheck,
} from 'lucide-react';
import type { FileQueueItem, NavTab, ProgressPayload } from '../types';

const PRIMARY_HOME_TASKS = [
  {
    id: 'standardizer' as NavTab,
    title: 'Chuẩn hóa văn bản',
    description: 'Sửa phông chữ, cỡ chữ, căn lề và khoảng cách theo đúng quy chuẩn.',
    actionLabel: 'Bắt đầu',
    icon: FileCheck2,
  },
  {
    id: 'complianceAudit' as NavTab,
    title: 'Kiểm tra văn bản',
    description: 'Kiểm tra lỗi trình bày và thể thức mà không làm thay đổi tệp gốc.',
    actionLabel: 'Bắt đầu',
    icon: SearchCheck,
  },
  {
    id: 'templateEngine' as NavTab,
    title: 'Tạo văn bản từ mẫu',
    description: 'Soạn công văn, quyết định, tờ trình, báo cáo từ mẫu chuẩn có sẵn.',
    actionLabel: 'Bắt đầu',
    icon: LayoutTemplate,
  },
  {
    id: 'history' as NavTab,
    title: 'Mở tài liệu gần đây',
    description: 'Xem lại, mở hoặc khôi phục các tài liệu bạn đã xử lý gần đây.',
    actionLabel: 'Mở danh sách',
    icon: History,
  },
] as const;

interface WorkspaceHomeProps {
  files: FileQueueItem[];
  onPickFiles: () => void;
  onPickFolder?: () => void;
  onTask: (tab: NavTab) => void;
  onShowFiles: () => void;
  progress: ProgressPayload | null;
  isDragging?: boolean;
  mobile?: boolean;
}

export function WorkspaceHome({
  files,
  onPickFiles,
  onPickFolder,
  onTask,
  onShowFiles,
  progress,
  isDragging,
  mobile,
}: WorkspaceHomeProps) {
  return (
    <div className="mx-auto max-w-4xl space-y-6 py-2">
      <header className="space-y-1.5">
        <h2 id="monword-home-title" className="text-xl font-bold tracking-tight text-[var(--mw-ink)] sm:text-2xl">
          Hôm nay bạn muốn làm gì?
        </h2>
        <p className="text-sm text-[var(--mw-muted)]">
          Chọn công việc bạn cần thực hiện. MonWord sẽ hướng dẫn từng bước.
        </p>
      </header>

      <section aria-labelledby="monword-home-title" className="grid grid-cols-1 gap-4 md:grid-cols-2">
        {PRIMARY_HOME_TASKS.map(({ id, title, description, actionLabel, icon: Icon }) => (
          <div
            key={id}
            className="mw-panel flex flex-col justify-between gap-4 p-5 transition-colors hover:border-[var(--mw-focus)]"
          >
            <div className="flex items-start gap-3.5">
              <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-[var(--mw-tint)] text-[var(--mw-accent)]">
                <Icon className="size-5" aria-hidden="true" />
              </div>
              <div className="min-w-0 space-y-1">
                <h3 className="text-base font-bold text-[var(--mw-ink)]">{title}</h3>
                <p className="text-xs leading-5 text-[var(--mw-muted)]">{description}</p>
              </div>
            </div>
            <div className="flex items-center justify-end pt-1">
              <button
                type="button"
                onClick={() => onTask(id)}
                className="mw-button mw-button-primary"
              >
                {actionLabel}
                <ArrowRight className="size-4" aria-hidden="true" />
              </button>
            </div>
          </div>
        ))}
      </section>

      {files.length > 0 && (
        <section
          aria-labelledby="monword-session-title"
          className="mw-panel flex flex-wrap items-center justify-between gap-3 px-4 py-3"
        >
          <div className="flex items-center gap-2.5 min-w-0">
            <FileText className="size-4 shrink-0 text-[var(--mw-accent)]" aria-hidden="true" />
            <div className="min-w-0">
              <div id="monword-session-title" className="text-xs font-bold text-[var(--mw-ink)]">
                Đang chọn {files.length} tài liệu sẵn sàng xử lý
              </div>
              <div className="truncate text-xs text-[var(--mw-muted)]">
                {files.slice(0, 2).map((f) => f.name).join(', ')}
                {files.length > 2 ? ` và ${files.length - 2} tệp khác` : ''}
              </div>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={onShowFiles}
              className="mw-button mw-button-secondary"
            >
              Xem danh sách
            </button>
            <button
              type="button"
              onClick={() => onTask('standardizer')}
              className="mw-button mw-button-primary"
            >
              Tiếp tục chuẩn hóa
              <ArrowRight className="size-4" aria-hidden="true" />
            </button>
          </div>
        </section>
      )}

      {progress && (
        <p role="status" className="text-sm font-medium text-[var(--mw-muted)]">
          {progress.phase === 'audit' ? 'Đang kiểm tra' : 'Đang xử lý'} {progress.current}/{progress.total}
        </p>
      )}

      <section
        aria-label="Chọn hoặc kéo thả tài liệu"
        className={`mw-empty flex flex-wrap items-center justify-between gap-3 px-4 py-3.5${
          isDragging ? ' border-[var(--mw-focus)] bg-[var(--mw-tint)]' : ''
        }`}
      >
        <div className="flex items-center gap-3 min-w-0">
          <FolderOpen className="size-4 shrink-0 text-[var(--mw-muted)]" aria-hidden="true" />
          <p className="text-xs sm:text-sm text-[var(--mw-muted)]">
            {mobile
              ? 'Hoặc chọn nhanh tệp Word (.docx, .docm) từ thiết bị'
              : 'Hoặc kéo thả tài liệu Word (.docx, .docm, .doc) vào đây'}
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
      </section>
    </div>
  );
}
