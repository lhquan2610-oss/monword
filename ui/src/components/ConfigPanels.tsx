import React, { useMemo } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  Clock,
  Download,
  FileCheck2,
  FileText,
  FolderOpen,
  FolderPlus,
  History,
  Plus,
  RefreshCw,
  RotateCcw,
  SearchCheck,
  Settings2,
  Shield,
  ShieldAlert,
  SlidersHorizontal,
  Terminal,
  Trash2,
  Unlock,
  Zap,
} from 'lucide-react';
import type {
  AddinItem,
  AuditReport,
  FileProcessResult,
  FileQueueItem,
  LockMode,
  NavTab,
  OperationLogEntry,
  OrientationMode,
  PageNumPosition,
  PaperSize,
  PresetId,
  PresetInfo,
  ProcessConfig,
  QuoteStyle,
  RecentDocEntry,
  SystemStatus,
  SystemTweaksRequest,
} from '../types';
import {
  ADVANCED_NAV_GROUPS,
  getAdvancedTabMeta,
  isAdvancedSubTab,
} from './WorkspaceNavigation';

export interface ConfigPanelsProps {
  activeTab: NavTab;
  presets: PresetInfo[];
  config: ProcessConfig;
  applyPreset: (presetId: PresetId) => void;
  updateFormat: <K extends keyof ProcessConfig['format']>(
    key: K,
    value: ProcessConfig['format'][K],
  ) => void;
  updateCleaner: <K extends keyof ProcessConfig['cleaner']>(
    key: K,
    value: ProcessConfig['cleaner'][K],
  ) => void;
  updatePageToc: <K extends keyof ProcessConfig['pageToc']>(
    key: K,
    value: ProcessConfig['pageToc'][K],
  ) => void;
  updateSecurity: <K extends keyof ProcessConfig['security']>(
    key: K,
    value: ProcessConfig['security'][K],
  ) => void;
  activityLogs: string[];
  onClearLogs: () => void;
  operationLogs?: OperationLogEntry[];
  onRefreshOpLogs?: () => void;
  onClearOpLogs?: () => void;
  isNativeWin: boolean;
  systemStatus: SystemStatus | null;
  addinApp: 'word' | 'excel';
  setAddinApp: (app: 'word' | 'excel') => void;
  addins: AddinItem[];
  loadingSystem: boolean;
  refreshSystemAndAddins: (targetApp?: 'word' | 'excel') => void;
  applySystemTweak: (req: SystemTweaksRequest) => void;
  handleAddAddin: () => void;
  handleManageAddin: (action: 'toggle' | 'delete', path: string) => void;
  handleInstallFonts: () => void;
  onOpenDesktopDownload: () => void;
  files?: FileQueueItem[];
  audits?: Record<string, AuditReport>;
  results?: Record<string, FileProcessResult>;
  recentDocs?: RecentDocEntry[];
  onNavigate?: (tab: NavTab) => void;
  onPickFiles?: () => void;
  onOpenOutput?: (path: string, revealInFolder?: boolean) => void;
  onDownloadOutput?: (blobUrl: string, fileName: string) => void;
  onRestoreBackup?: (originalPath: string, backupPath: string) => void;
  onClearRecent?: () => void;
  renderHeaderBuilder?: () => React.ReactNode;
  renderStandardizer?: () => React.ReactNode;
  renderTemplateEngine?: () => React.ReactNode;
  renderLayoutIntelligence?: () => React.ReactNode;
  renderComplianceAudit?: () => React.ReactNode;
  renderLivePreview?: () => React.ReactNode;
}

const STATUS_META: Record<
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
      year: 'numeric',
    });
  } catch {
    return '';
  }
}

const AdvancedSubNavBar: React.FC<{
  activeTab: NavTab;
  onNavigate: (tab: NavTab) => void;
}> = ({ activeTab, onNavigate }) => {
  const meta = getAdvancedTabMeta(activeTab);
  if (!meta) return null;

  return (
    <div className="mw-panel flex flex-wrap items-center justify-between gap-2 px-3.5 py-2.5">
      <div className="flex flex-wrap items-center gap-2 min-w-0">
        <button
          type="button"
          onClick={() => onNavigate('advancedSettings')}
          className="mw-button mw-button-secondary text-xs"
        >
          <ArrowLeft className="size-3.5" aria-hidden="true" />
          Quay lại Cài đặt và nâng cao
        </button>
        <span className="hidden sm:inline text-xs text-[var(--mw-muted)]">
          {meta.group.title} / <strong className="text-[var(--mw-ink)]">{meta.item.label}</strong>
        </span>
      </div>

      {meta.group.items.length > 1 && (
        <div className="flex flex-wrap items-center gap-1">
          {meta.group.items.map((sibling) => (
            <button
              key={sibling.id}
              type="button"
              onClick={() => onNavigate(sibling.id)}
              className={`px-2.5 py-1 rounded-md text-xs font-semibold transition-colors ${
                sibling.id === activeTab
                  ? 'bg-[var(--mw-accent)] text-[var(--mw-accent-ink)]'
                  : 'bg-[var(--mw-tint)] text-[var(--mw-ink)] hover:opacity-85'
              }`}
            >
              {sibling.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

interface HistoryViewItem extends RecentDocEntry {
  inSession: boolean;
  sessionResult?: FileProcessResult;
  sessionAudit?: AuditReport;
}

const HistoryPanel: React.FC<{
  files: FileQueueItem[];
  audits: Record<string, AuditReport>;
  results: Record<string, FileProcessResult>;
  recentDocs: RecentDocEntry[];
  onNavigate?: (tab: NavTab) => void;
  onPickFiles?: () => void;
  onOpenOutput?: (path: string, revealInFolder?: boolean) => void;
  onDownloadOutput?: (blobUrl: string, fileName: string) => void;
  onRestoreBackup?: (originalPath: string, backupPath: string) => void;
  onClearRecent?: () => void;
}> = ({
  files,
  audits,
  results,
  recentDocs,
  onNavigate,
  onPickFiles,
  onOpenOutput,
  onDownloadOutput,
  onRestoreBackup,
  onClearRecent,
}) => {
  const items = useMemo<HistoryViewItem[]>(() => {
    const map = new Map<string, HistoryViewItem>();

    for (const entry of recentDocs) {
      map.set(entry.path.toLowerCase(), {
        ...entry,
        inSession: false,
      });
    }

    for (const file of files) {
      const key = file.path.toLowerCase();
      const prev = map.get(key);
      const res = results[file.path];
      const audit = res?.auditAfter || audits[file.path];
      const status: RecentDocEntry['status'] = res?.success
        ? 'standardized'
        : audit
          ? 'checked'
          : prev?.status || 'selected';

      map.set(key, {
        path: file.path,
        name: file.name,
        ext: file.ext,
        updatedAt: prev?.updatedAt || new Date().toISOString(),
        status,
        outputPath: res?.success ? res.outputPath : prev?.outputPath,
        backupPath: res?.backupPath ?? prev?.backupPath ?? null,
        summary: res?.success
          ? res.statsSummary
          : audit
            ? `Điểm thể thức: ${audit.score}/100`
            : prev?.summary,
        inSession: true,
        sessionResult: res,
        sessionAudit: audit,
      });
    }

    return Array.from(map.values()).sort((a, b) =>
      b.updatedAt.localeCompare(a.updatedAt),
    );
  }, [audits, files, recentDocs, results]);

  return (
    <div className="mx-auto max-w-4xl space-y-5">
      <header className="mw-panel flex flex-wrap items-center justify-between gap-3 p-4 sm:p-5">
        <div className="flex items-start gap-3 min-w-0">
          <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-[var(--mw-tint)] text-[var(--mw-accent)]">
            <History className="size-5" aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-bold text-[var(--mw-ink)]">
              Lịch sử &amp; Tài liệu gần đây
            </h2>
            <p className="mt-0.5 text-xs text-[var(--mw-muted)]">
              Xem lại các tài liệu bạn đã mở, kiểm tra hoặc chuẩn hóa để tiếp tục xử lý hoặc khôi phục bản gốc.
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {onPickFiles && (
            <button
              type="button"
              onClick={onPickFiles}
              className="mw-button mw-button-primary"
            >
              Chọn tài liệu từ máy
            </button>
          )}
          {items.length > 0 && onClearRecent && (
            <button
              type="button"
              onClick={onClearRecent}
              className="mw-button mw-button-secondary"
            >
              <Trash2 className="size-3.5" aria-hidden="true" /> Xóa lịch sử
            </button>
          )}
        </div>
      </header>

      {items.length === 0 ? (
        <div className="mw-empty flex flex-col items-center justify-center gap-3 px-6 py-12 text-center">
          <FileText className="size-10 text-[var(--mw-muted)]" aria-hidden="true" />
          <div className="space-y-1">
            <h3 className="text-sm font-bold text-[var(--mw-ink)]">
              Chưa có tài liệu nào gần đây
            </h3>
            <p className="text-xs text-[var(--mw-muted)]">
              Hãy chọn một tài liệu Word từ máy tính để bắt đầu chuẩn hóa hoặc kiểm tra thể thức.
            </p>
          </div>
          {onPickFiles && (
            <button
              type="button"
              onClick={onPickFiles}
              className="mw-button mw-button-primary mt-1"
            >
              Chọn tài liệu từ máy
            </button>
          )}
        </div>
      ) : (
        <div className="mw-panel divide-y divide-[var(--mw-border)]">
          {items.map((item) => {
            const badge = STATUS_META[item.status];
            const isWebPath = item.path.startsWith('web://');
            const targetOpenPath = item.outputPath || (!isWebPath ? item.path : undefined);
            const timeLabel = formatRecentTime(item.updatedAt);

            return (
              <div
                key={item.path}
                className="flex flex-wrap items-center justify-between gap-3 p-4"
              >
                <div className="flex items-start gap-3 min-w-0 flex-1 basis-64">
                  <FileText className="size-5 shrink-0 text-[var(--mw-accent)] mt-0.5" aria-hidden="true" />
                  <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-sm font-bold text-[var(--mw-ink)]">
                        {item.name}
                      </span>
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-bold ${badge.className}`}
                      >
                        {badge.label}
                      </span>
                    </div>
                    {item.summary && (
                      <p className="text-xs text-[var(--mw-muted)]">{item.summary}</p>
                    )}
                    <div className="flex flex-wrap items-center gap-2 text-[11px] text-[var(--mw-muted)]">
                      {!isWebPath && (
                        <span title={item.path} className="truncate max-w-md">
                          {item.path}
                        </span>
                      )}
                      {timeLabel && (
                        <span className="inline-flex items-center gap-1">
                          <Clock className="size-3" aria-hidden="true" />
                          {timeLabel}
                        </span>
                      )}
                    </div>
                  </div>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  {item.inSession && onNavigate && (
                    <>
                      <button
                        type="button"
                        onClick={() => onNavigate('standardizer')}
                        className="mw-button mw-button-primary"
                      >
                        <FileCheck2 className="size-3.5" aria-hidden="true" />
                        Chuẩn hóa
                      </button>
                      <button
                        type="button"
                        onClick={() => onNavigate('complianceAudit')}
                        className="mw-button mw-button-secondary"
                      >
                        <SearchCheck className="size-3.5" aria-hidden="true" />
                        Kiểm tra
                      </button>
                    </>
                  )}

                  {item.sessionResult?.blobUrl && onDownloadOutput && (
                    <button
                      type="button"
                      onClick={() =>
                        onDownloadOutput(
                          item.sessionResult!.blobUrl!,
                          item.sessionResult!.outputName || item.sessionResult!.outputPath,
                        )
                      }
                      className="mw-button mw-button-secondary"
                    >
                      <Download className="size-3.5" aria-hidden="true" />
                      Tải tệp kết quả
                    </button>
                  )}

                  {!isWebPath && targetOpenPath && onOpenOutput && (
                    <>
                      <button
                        type="button"
                        onClick={() => onOpenOutput(targetOpenPath, false)}
                        className="mw-button mw-button-secondary"
                      >
                        {item.outputPath ? 'Mở tệp kết quả' : 'Mở tệp'}
                      </button>
                      <button
                        type="button"
                        onClick={() => onOpenOutput(targetOpenPath, true)}
                        className="mw-button mw-button-secondary"
                      >
                        <FolderOpen className="size-3.5" aria-hidden="true" />
                        Mở thư mục
                      </button>
                    </>
                  )}

                  {!isWebPath && item.backupPath && onRestoreBackup && (
                    <button
                      type="button"
                      onClick={() => onRestoreBackup(item.path, item.backupPath!)}
                      className="mw-button mw-button-secondary"
                    >
                      <RotateCcw className="size-3.5" aria-hidden="true" />
                      Khôi phục bản gốc
                    </button>
                  )}

                  {!item.inSession && isWebPath && onPickFiles && (
                    <button
                      type="button"
                      onClick={onPickFiles}
                      className="mw-button mw-button-secondary"
                    >
                      Chọn lại tệp để xử lý
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};

const AdvancedSettingsHub: React.FC<{
  onNavigate?: (tab: NavTab) => void;
}> = ({ onNavigate }) => {
  return (
    <div className="mx-auto max-w-4xl space-y-5">
      <header className="mw-panel p-4 sm:p-5">
        <div className="flex items-start gap-3">
          <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-[var(--mw-tint)] text-[var(--mw-accent)]">
            <SlidersHorizontal className="size-5" aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-bold text-[var(--mw-ink)]">
              Cài đặt và nâng cao
            </h2>
            <p className="mt-1 text-xs leading-5 text-[var(--mw-muted)]">
              Các công cụ cấu hình chi tiết, xử lý hàng loạt và thiết lập hệ thống dành cho người dùng có nhu cầu riêng hoặc quản trị viên.
            </p>
          </div>
        </div>
      </header>

      <div className="space-y-4">
        {ADVANCED_NAV_GROUPS.map((group) => (
          <section key={group.id} className="mw-panel p-4 sm:p-5 space-y-3">
            <div>
              <h3 className="text-sm font-bold text-[var(--mw-ink)]">{group.title}</h3>
              <p className="mt-0.5 text-xs text-[var(--mw-muted)]">{group.subtitle}</p>
            </div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              {group.items.map(({ id, label, description, icon: Icon }) => (
                <div
                  key={id}
                  className="flex flex-col justify-between gap-3 rounded-lg border border-[var(--mw-border)] bg-[var(--mw-canvas)] p-3.5"
                >
                  <div className="flex items-start gap-3">
                    <div className="flex size-8 shrink-0 items-center justify-center rounded-md bg-[var(--mw-tint)] text-[var(--mw-accent)]">
                      <Icon className="size-4" aria-hidden="true" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-xs font-bold text-[var(--mw-ink)]">{label}</div>
                      <p className="mt-0.5 text-xs leading-4 text-[var(--mw-muted)]">
                        {description}
                      </p>
                    </div>
                  </div>
                  <div className="flex justify-end">
                    <button
                      type="button"
                      onClick={() => onNavigate?.(id)}
                      className="mw-button mw-button-secondary text-xs"
                    >
                      Mở công cụ
                      <ArrowRight className="size-3.5" aria-hidden="true" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
};

export const ConfigPanels: React.FC<ConfigPanelsProps> = ({
  activeTab,
  presets,
  config,
  applyPreset,
  updateFormat,
  updateCleaner,
  updatePageToc,
  updateSecurity,
  activityLogs,
  onClearLogs,
  operationLogs = [],
  onRefreshOpLogs,
  onClearOpLogs,
  isNativeWin,
  systemStatus,
  addinApp,
  setAddinApp,
  addins,
  loadingSystem,
  refreshSystemAndAddins,
  applySystemTweak,
  handleAddAddin,
  handleManageAddin,
  handleInstallFonts,
  onOpenDesktopDownload,
  files = [],
  audits = {},
  results = {},
  recentDocs = [],
  onNavigate,
  onPickFiles,
  onOpenOutput,
  onDownloadOutput,
  onRestoreBackup,
  onClearRecent,
  renderHeaderBuilder,
  renderStandardizer,
  renderTemplateEngine,
  renderLayoutIntelligence,
  renderComplianceAudit,
  renderLivePreview,
}) => {
  const isSubTab = isAdvancedSubTab(activeTab);

  const renderPanelBody = (): React.ReactNode => {
    if (activeTab === 'history') {
      return (
        <HistoryPanel
          files={files}
          audits={audits}
          results={results}
          recentDocs={recentDocs}
          onNavigate={onNavigate}
          onPickFiles={onPickFiles}
          onOpenOutput={onOpenOutput}
          onDownloadOutput={onDownloadOutput}
          onRestoreBackup={onRestoreBackup}
          onClearRecent={onClearRecent}
        />
      );
    }
    if (activeTab === 'advancedSettings') {
      return <AdvancedSettingsHub onNavigate={onNavigate} />;
    }
    if (activeTab === 'headerBuilder' && renderHeaderBuilder) {
      return <>{renderHeaderBuilder()}</>;
    }
    if (activeTab === 'standardizer' && renderStandardizer) {
      return <>{renderStandardizer()}</>;
    }
    if (activeTab === 'templateEngine' && renderTemplateEngine) {
      return <>{renderTemplateEngine()}</>;
    }
    if (activeTab === 'layoutIntelligence' && renderLayoutIntelligence) {
      return <>{renderLayoutIntelligence()}</>;
    }
    if (activeTab === 'complianceAudit' && renderComplianceAudit) {
      return <>{renderComplianceAudit()}</>;
    }
    if (activeTab === 'livePreview' && renderLivePreview) {
      return <>{renderLivePreview()}</>;
    }

  if (activeTab === 'oneClick') {
    return (
      <div className="space-y-4">
        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm">
          <h2 className="text-base font-bold flex items-center gap-2">
            <Zap className="w-4 h-4 text-blue-600" />
            Chuẩn hóa Văn bản 1-Click theo Bộ Quy chuẩn
          </h2>
          <p className="text-xs text-slate-500 dark:text-slate-400 mt-1">
            Chọn nhanh mẫu chuẩn hóa phù hợp với loại tài liệu. Bạn có thể tinh chỉnh từng thông số ở các mục liên quan.
          </p>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-3 mt-4">
            {presets.map((preset) => {
              const selected = config.preset === preset.id;
              return (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() => applyPreset(preset.id)}
                  className={`text-left p-3.5 rounded-xl border transition-all cursor-pointer ${
                    selected
                      ? 'border-blue-500 bg-blue-50/70 dark:bg-blue-950/30 ring-2 ring-blue-500/20'
                      : 'border-slate-200 dark:border-slate-800 hover:border-slate-300 dark:hover:border-slate-700 bg-slate-50/50 dark:bg-slate-950/40'
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-bold text-slate-900 dark:text-slate-100">
                      {preset.name}
                    </span>
                    <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900/60 dark:text-blue-300 shrink-0">
                      {preset.badge}
                    </span>
                  </div>
                  <p className="text-[11px] text-slate-600 dark:text-slate-400 mt-1.5 leading-relaxed">
                    {preset.description}
                  </p>
                </button>
              );
            })}
          </div>
        </div>

        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm">
          <h3 className="text-xs font-bold uppercase tracking-wider text-slate-500 mb-3">
            Tóm tắt quy trình tự động sẽ thực thi
          </h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 text-xs">
            <div className="p-2.5 rounded-xl bg-slate-50 dark:bg-slate-950/50 border border-slate-200 dark:border-slate-800">
              <div className="font-semibold text-slate-800 dark:text-slate-200">
                1. Làm sạch &amp; Chuyển mã
              </div>
              <div className="text-[11px] text-slate-500 mt-0.5">
                {config.cleaner.enabled
                  ? `${config.cleaner.convertLegacyEncoding ? 'Chuyển TCVN3/VNI → Unicode, ' : ''}sửa dấu câu, ngày tháng NĐ30`
                  : 'Đang tắt'}
              </div>
            </div>
            <div className="p-2.5 rounded-xl bg-slate-50 dark:bg-slate-950/50 border border-slate-200 dark:border-slate-800">
              <div className="font-semibold text-slate-800 dark:text-slate-200">
                2. Định dạng Thông minh
              </div>
              <div className="text-[11px] text-slate-500 mt-0.5">
                {config.format.enabled
                  ? `${config.format.fontName} ${config.format.fontSizePt}pt · Lề ${config.format.marginTopCm}-${config.format.marginBottomCm}-${config.format.marginLeftCm}-${config.format.marginRightCm} cm`
                  : 'Đang tắt'}
              </div>
            </div>
            <div className="p-2.5 rounded-xl bg-slate-50 dark:bg-slate-950/50 border border-slate-200 dark:border-slate-800">
              <div className="font-semibold text-slate-800 dark:text-slate-200">
                3. Số trang &amp; Khổ giấy
              </div>
              <div className="text-[11px] text-slate-500 mt-0.5">
                {config.pageToc.pageNumbersEnabled
                  ? `Đánh số trang (${config.pageToc.pageNumFontSizePt}pt)${config.pageToc.skipFirstPage ? ', bỏ trang 1' : ''}`
                  : 'Giữ nguyên số trang'}
              </div>
            </div>
            <div className="p-2.5 rounded-xl bg-slate-50 dark:bg-slate-950/50 border border-slate-200 dark:border-slate-800">
              <div className="font-semibold text-slate-800 dark:text-slate-200">
                4. Bảo vệ &amp; Macro
              </div>
              <div className="text-[11px] text-slate-500 mt-0.5">
                {config.security.cleanMacros ? 'Gỡ sạch VBA Macro' : 'Giữ nguyên Macro'} ·{' '}
                {config.security.scrubPersonalInfo ? 'Xóa Metadata ẩn' : 'Giữ Metadata'}
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (activeTab === 'format') {
    return (
      <div className="space-y-4">
        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm space-y-4">
          <div className="flex items-center justify-between gap-2">
            <div>
              <h2 className="text-sm font-bold">Thiết lập Font chữ &amp; Đoạn văn</h2>
              <p className="text-xs text-slate-500">Áp dụng chuẩn xác trên OpenXML (Word &amp; Excel)</p>
            </div>
            <label className="inline-flex items-center gap-2 text-xs font-semibold cursor-pointer">
              <input
                type="checkbox"
                checked={config.format.enabled}
                onChange={(e) => updateFormat('enabled', e.target.checked)}
                className="rounded border-slate-300 text-blue-600"
              />
              Bật định dạng
            </label>
          </div>

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
            <div className="col-span-2">
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Font chữ</label>
              <select
                value={config.format.fontName}
                onChange={(e) => updateFormat('fontName', e.target.value)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value="Times New Roman">Times New Roman</option>
                <option value="Arial">Arial</option>
                <option value="Calibri">Calibri</option>
                <option value="Tahoma">Tahoma</option>
              </select>
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Cỡ chữ (pt)</label>
              <input
                type="number"
                step="0.5"
                min="8"
                max="36"
                value={config.format.fontSizePt}
                onChange={(e) => updateFormat('fontSizePt', Number(e.target.value) || 14)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Giãn dòng</label>
              <input
                type="number"
                step="0.05"
                min="1"
                max="3"
                value={config.format.lineSpacing}
                onChange={(e) => updateFormat('lineSpacing', Number(e.target.value) || 1.15)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Cách đoạn Trước (pt)</label>
              <input
                type="number"
                step="1"
                min="0"
                max="36"
                value={config.format.spaceBeforePt}
                onChange={(e) => updateFormat('spaceBeforePt', Number(e.target.value) || 0)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Cách đoạn Sau (pt)</label>
              <input
                type="number"
                step="1"
                min="0"
                max="36"
                value={config.format.spaceAfterPt}
                onChange={(e) => updateFormat('spaceAfterPt', Number(e.target.value) || 0)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Thụt đầu dòng (cm)</label>
              <input
                type="number"
                step="0.1"
                min="0"
                max="5"
                value={config.format.firstLineIndentCm}
                onChange={(e) => updateFormat('firstLineIndentCm', Number(e.target.value) || 0)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Khổ giấy</label>
              <select
                value={config.format.paperSize}
                onChange={(e) => updateFormat('paperSize', e.target.value as PaperSize)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value="a4">A4 (21 × 29.7 cm)</option>
                <option value="a3">A3 (29.7 × 42 cm)</option>
                <option value="letter">Letter</option>
              </select>
            </div>
          </div>

          <div className="pt-2 border-t border-slate-200 dark:border-slate-800">
            <div className="text-[11px] font-semibold text-slate-500 mb-2">Căn lề chuẩn (cm)</div>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
              {(
                [
                  ['marginTopCm', 'Trên'],
                  ['marginBottomCm', 'Dưới'],
                  ['marginLeftCm', 'Trái (Gáy)'],
                  ['marginRightCm', 'Phải'],
                ] as const
              ).map(([k, label]) => (
                <div key={k}>
                  <label className="block text-[11px] text-slate-500 mb-1">{label}</label>
                  <input
                    type="number"
                    step="0.1"
                    min="0.5"
                    max="6"
                    value={config.format[k]}
                    onChange={(e) => updateFormat(k, Number(e.target.value) || 2)}
                    className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                  />
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm space-y-3">
          <h3 className="text-sm font-bold">Định dạng Thông minh &amp; Bảng biểu</h3>

          <label className="flex items-start gap-2.5 p-2.5 rounded-xl bg-emerald-50/70 dark:bg-emerald-950/30 border border-emerald-200/80 dark:border-emerald-900 cursor-pointer">
            <input
              type="checkbox"
              checked={config.format.protectAdminBlocks}
              onChange={(e) => updateFormat('protectAdminBlocks', e.target.checked)}
              className="mt-0.5 rounded border-emerald-400 text-emerald-600"
            />
            <div>
              <div className="text-xs font-semibold text-emerald-900 dark:text-emerald-200">
                Bảo vệ Quốc hiệu - Tiêu ngữ, Kính gửi &amp; Khối Chữ ký
              </div>
              <div className="text-[11px] text-emerald-700 dark:text-emerald-400">
                Không ép thụt đầu dòng {config.format.firstLineIndentCm} cm hay phá căn giữa/phải tại các phần thể thức hành chính.
              </div>
            </div>
          </label>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <label className="flex items-start gap-2.5 p-2.5 rounded-xl border border-slate-200 dark:border-slate-800 cursor-pointer">
              <input
                type="checkbox"
                checked={config.format.smartTables}
                onChange={(e) => updateFormat('smartTables', e.target.checked)}
                className="mt-0.5 rounded border-slate-300 text-blue-600"
              />
              <div>
                <div className="text-xs font-semibold">Tối ưu Bảng biểu thông minh</div>
                <div className="text-[11px] text-slate-500">
                  Tự co vừa trang 100%, chống tách đôi dòng (cantSplit) và xóa thụt lề ô bảng.
                </div>
              </div>
            </label>

            <div className="p-2.5 rounded-xl border border-slate-200 dark:border-slate-800 space-y-2">
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-semibold">Cỡ chữ trong bảng (pt)</span>
                <input
                  type="number"
                  step="0.5"
                  min="8"
                  max="18"
                  value={config.format.tableFontSizePt}
                  onChange={(e) => updateFormat('tableFontSizePt', Number(e.target.value) || 13)}
                  className="w-20 px-2 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                />
              </div>
              <label className="flex items-center gap-2 text-xs cursor-pointer">
                <input
                  type="checkbox"
                  checked={config.format.repeatTableHeaders}
                  onChange={(e) => updateFormat('repeatTableHeaders', e.target.checked)}
                  className="rounded border-slate-300 text-blue-600"
                />
                Tự lặp dòng tiêu đề khi bảng sang trang
              </label>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (activeTab === 'pageToc') {
    return (
      <div className="space-y-4">
        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm space-y-3">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-sm font-bold">Đánh số trang &amp; Đầu/Chân trang</h2>
            <label className="inline-flex items-center gap-2 text-xs font-semibold cursor-pointer">
              <input
                type="checkbox"
                checked={config.pageToc.pageNumbersEnabled}
                onChange={(e) => updatePageToc('pageNumbersEnabled', e.target.checked)}
                className="rounded border-slate-300 text-blue-600"
              />
              Bật đánh số trang
            </label>
          </div>

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
            <div className="col-span-2">
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Vị trí số trang</label>
              <select
                value={config.pageToc.pageNumPosition}
                onChange={(e) => updatePageToc('pageNumPosition', e.target.value as PageNumPosition)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value="topCenter">Đầu trang - Giữa (Chuẩn NĐ30)</option>
                <option value="topRight">Đầu trang - Phải</option>
                <option value="bottomCenter">Chân trang - Giữa</option>
                <option value="bottomRight">Chân trang - Phải</option>
              </select>
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Cỡ số trang (pt)</label>
              <input
                type="number"
                value={config.pageToc.pageNumFontSizePt}
                onChange={(e) => updatePageToc('pageNumFontSizePt', Number(e.target.value) || 13)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Bắt đầu từ số</label>
              <input
                type="number"
                min="1"
                value={config.pageToc.startAt}
                onChange={(e) => updatePageToc('startAt', Number(e.target.value) || 1)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
          </div>

          <div className="flex flex-wrap gap-4 pt-1 text-xs">
            <label className="inline-flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.pageToc.skipFirstPage}
                onChange={(e) => updatePageToc('skipFirstPage', e.target.checked)}
                className="rounded border-slate-300 text-blue-600"
              />
              Không hiển thị ở trang đầu (NĐ30)
            </label>
            <label className="inline-flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.pageToc.mirrorOddEven}
                onChange={(e) => updatePageToc('mirrorOddEven', e.target.checked)}
                className="rounded border-slate-300 text-blue-600"
              />
              Chẵn/Lẻ đối xứng
            </label>
            <label className="inline-flex items-center gap-2 cursor-pointer text-rose-600 dark:text-rose-400">
              <input
                type="checkbox"
                checked={config.pageToc.removeExistingPageNumbers}
                onChange={(e) => updatePageToc('removeExistingPageNumbers', e.target.checked)}
                className="rounded border-slate-300 text-rose-600"
              />
              Xóa số trang cũ trước khi xử lý
            </label>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2 border-t border-slate-200 dark:border-slate-800">
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Header bổ sung (tùy chọn)</label>
              <input
                type="text"
                placeholder="VD: Tài liệu lưu hành nội bộ"
                value={config.pageToc.headerText}
                onChange={(e) => updatePageToc('headerText', e.target.value)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Footer bổ sung (tùy chọn)</label>
              <input
                type="text"
                placeholder="VD: Đơn vị ban hành..."
                value={config.pageToc.footerText}
                onChange={(e) => updatePageToc('footerText', e.target.value)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
          </div>
        </div>

        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm space-y-3">
          <h3 className="text-sm font-bold">Hướng giấy &amp; Xoay trang Thông minh</h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Chế độ hướng giấy</label>
              <select
                value={config.pageToc.orientationMode}
                onChange={(e) => updatePageToc('orientationMode', e.target.value as OrientationMode)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value="keep">Giữ nguyên hướng hiện tại</option>
                <option value="allPortrait">Toàn bộ khổ Dọc (Portrait)</option>
                <option value="allLandscape">Toàn bộ khổ Ngang (Landscape)</option>
                <option value="rotateRanges">Xoay ngang các trang chỉ định</option>
              </select>
            </div>

            {config.pageToc.orientationMode === 'rotateRanges' && (
              <div>
                <label className="block text-[11px] font-semibold text-slate-500 mb-1">
                  Trang cần xoay ngang (VD: 3,5,7-10)
                </label>
                <input
                  type="text"
                  placeholder="3,5,7-10"
                  value={config.pageToc.landscapeRanges}
                  onChange={(e) => updatePageToc('landscapeRanges', e.target.value)}
                  className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                />
              </div>
            )}
          </div>

          <div className="flex flex-wrap gap-4 text-xs">
            <label className="inline-flex items-center gap-2 cursor-pointer font-medium text-amber-700 dark:text-amber-300">
              <input
                type="checkbox"
                checked={config.pageToc.autoSwapBindingMargin}
                onChange={(e) => updatePageToc('autoSwapBindingMargin', e.target.checked)}
                className="rounded border-amber-400 text-amber-600"
              />
              Tự động đảo lề đóng gáy 3 cm lên mép trên khi xoay ngang
            </label>
            <label className="inline-flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.pageToc.resetPortraitAndRemoveSections}
                onChange={(e) => updatePageToc('resetPortraitAndRemoveSections', e.target.checked)}
                className="rounded border-slate-300 text-blue-600"
              />
              Reset về dọc + xóa toàn bộ Section Break cũ
            </label>
          </div>
        </div>

        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm space-y-3">
          <h3 className="text-sm font-bold">Heading chuẩn NĐ30 &amp; Mục lục Tự động</h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
            <label className="flex items-start gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.pageToc.applyNd30Headings}
                onChange={(e) => updatePageToc('applyNd30Headings', e.target.checked)}
                className="mt-0.5 rounded border-slate-300 text-blue-600"
              />
              <span>Cập nhật Style Heading 1–4 chuẩn Nghị định 30/2020</span>
            </label>
            <label className="flex items-start gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.pageToc.autoDetectHeadings}
                onChange={(e) => updatePageToc('autoDetectHeadings', e.target.checked)}
                className="mt-0.5 rounded border-slate-300 text-blue-600"
              />
              <span>Tự động nhận diện Chương / Mục / Điều / 1.1 để gắn Heading</span>
            </label>
            <label className="flex items-start gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.pageToc.insertToc}
                onChange={(e) => updatePageToc('insertToc', e.target.checked)}
                className="mt-0.5 rounded border-slate-300 text-blue-600"
              />
              <span>Chèn Mục lục tự động ở đầu văn bản</span>
            </label>
            <div className="flex items-center gap-2">
              <span>Số cấp Mục lục:</span>
              <select
                value={config.pageToc.tocLevels}
                onChange={(e) => updatePageToc('tocLevels', Number(e.target.value))}
                className="px-2 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value={2}>2 cấp</option>
                <option value={3}>3 cấp</option>
                <option value={4}>4 cấp</option>
              </select>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (activeTab === 'cleaner') {
    return (
      <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm space-y-4">
        <div className="flex items-center justify-between gap-2">
          <div>
            <h2 className="text-sm font-bold">Làm sạch Tiếng Việt &amp; Chuyển mã Font cũ</h2>
            <p className="text-xs text-slate-500">
              Sửa tự động các lỗi gõ văn bản hành chính phổ biến và chuyển bảng mã TCVN3 / VNI sang Unicode.
            </p>
          </div>
          <label className="inline-flex items-center gap-2 text-xs font-semibold cursor-pointer">
            <input
              type="checkbox"
              checked={config.cleaner.enabled}
              onChange={(e) => updateCleaner('enabled', e.target.checked)}
              className="rounded border-slate-300 text-blue-600"
            />
            Bật bộ làm sạch
          </label>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
          <label className="flex items-start gap-2.5 p-3 rounded-xl border border-amber-200 dark:border-amber-900 bg-amber-50/50 dark:bg-amber-950/30 cursor-pointer">
            <input
              type="checkbox"
              checked={config.cleaner.convertLegacyEncoding}
              onChange={(e) => updateCleaner('convertLegacyEncoding', e.target.checked)}
              className="mt-0.5 rounded border-amber-400 text-amber-600"
            />
            <div>
              <div className="font-semibold text-amber-900 dark:text-amber-200">
                Chuyển mã TCVN3 (.VnTime) &amp; VNI → Unicode
              </div>
              <div className="text-[11px] text-amber-700 dark:text-amber-400">
                Tự nhận diện run dùng font cũ và chuyển sang Times New Roman Unicode mà vẫn giữ định dạng.
              </div>
            </div>
          </label>

          <label className="flex items-start gap-2.5 p-3 rounded-xl border border-slate-200 dark:border-slate-800 cursor-pointer">
            <input
              type="checkbox"
              checked={config.cleaner.normalizeNd30Dates}
              onChange={(e) => updateCleaner('normalizeNd30Dates', e.target.checked)}
              className="mt-0.5 rounded border-slate-300 text-blue-600"
            />
            <div>
              <div className="font-semibold">Chuẩn hóa Ngày tháng NĐ30</div>
              <div className="text-[11px] text-slate-500">
                Ngày &lt;10 và tháng 1, 2 thêm số 0 (05/02); tháng 3–12 bỏ số 0 thừa (tháng 3).
              </div>
            </div>
          </label>

          <label className="flex items-start gap-2.5 p-3 rounded-xl border border-slate-200 dark:border-slate-800 cursor-pointer">
            <input
              type="checkbox"
              checked={config.cleaner.fixMultipleSpaces}
              onChange={(e) => updateCleaner('fixMultipleSpaces', e.target.checked)}
              className="mt-0.5 rounded border-slate-300 text-blue-600"
            />
            <div>
              <div className="font-semibold">Xóa khoảng trắng kép thừa</div>
              <div className="text-[11px] text-slate-500">Gộp nhiều khoảng trắng liên tiếp thành 1 khoảng trắng chuẩn.</div>
            </div>
          </label>

          <label className="flex items-start gap-2.5 p-3 rounded-xl border border-slate-200 dark:border-slate-800 cursor-pointer">
            <input
              type="checkbox"
              checked={config.cleaner.fixPunctuationSpacing}
              onChange={(e) => updateCleaner('fixPunctuationSpacing', e.target.checked)}
              className="mt-0.5 rounded border-slate-300 text-blue-600"
            />
            <div>
              <div className="font-semibold">Chuẩn khoảng trắng dấu câu</div>
              <div className="text-[11px] text-slate-500">Xóa cách trước dấu (, . ; : ! ?) và chèn cách sau (, ;) dính chữ.</div>
            </div>
          </label>

          <label className="flex items-start gap-2.5 p-3 rounded-xl border border-slate-200 dark:border-slate-800 cursor-pointer">
            <input
              type="checkbox"
              checked={config.cleaner.fixSoftBreaks}
              onChange={(e) => updateCleaner('fixSoftBreaks', e.target.checked)}
              className="mt-0.5 rounded border-slate-300 text-blue-600"
            />
            <div>
              <div className="font-semibold">Ngắt dòng mềm (Shift+Enter → Enter)</div>
              <div className="text-[11px] text-slate-500">Chuyển ngắt mềm thành đoạn mới để không bị giãn thưa chữ khi căn đều.</div>
            </div>
          </label>

          <label className="flex items-start gap-2.5 p-3 rounded-xl border border-slate-200 dark:border-slate-800 cursor-pointer">
            <input
              type="checkbox"
              checked={config.cleaner.removeEmptyParagraphs}
              onChange={(e) => updateCleaner('removeEmptyParagraphs', e.target.checked)}
              className="mt-0.5 rounded border-slate-300 text-blue-600"
            />
            <div>
              <div className="font-semibold">Xóa dòng Enter trống dư thừa</div>
              <div className="text-[11px] text-slate-500">Loại bỏ các đoạn rỗng thừa mà vẫn giữ nguyên ảnh và ngắt trang.</div>
            </div>
          </label>
        </div>

        <div className="pt-2 border-t border-slate-200 dark:border-slate-800">
          <label className="block text-[11px] font-semibold text-slate-500 mb-1.5">Quy chuẩn dấu ngoặc kép</label>
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-2 text-xs">
            {(
              [
                ['curlyNd30', '“ ” Ngoặc móc (VB Hành chính)'],
                ['straightDang', '" " Ngoặc thẳng (VB Đảng)'],
                ['keep', 'Giữ nguyên hiện trạng'],
              ] as [QuoteStyle, string][]
            ).map(([val, label]) => (
              <button
                key={val}
                type="button"
                onClick={() => updateCleaner('quoteStyle', val)}
                className={`p-2 rounded-lg border font-medium transition-all ${
                  config.cleaner.quoteStyle === val
                    ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300'
                    : 'border-slate-200 dark:border-slate-800'
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </div>
    );
  }

  if (activeTab === 'security') {
    return (
      <div className="space-y-4">
        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 sm:p-5 shadow-sm space-y-4">
          <h2 className="text-sm font-bold flex items-center gap-2">
            <Shield className="w-4 h-4 text-rose-600" />
            Office Macro Cleaner (.docm/.xlsm → .docx/.xlsx) &amp; Bảo vệ Tệp
          </h2>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
            <label className="flex items-start gap-2.5 p-3 rounded-xl border border-rose-200 dark:border-rose-900 bg-rose-50/50 dark:bg-rose-950/30 cursor-pointer">
              <input
                type="checkbox"
                checked={config.security.cleanMacros}
                onChange={(e) => updateSecurity('cleanMacros', e.target.checked)}
                className="mt-0.5 rounded border-rose-400 text-rose-600"
              />
              <div>
                <div className="font-bold text-rose-900 dark:text-rose-200">
                  Gỡ bỏ sạch VBA Macro &amp; Chuyển đuôi sạch
                </div>
                <div className="text-[11px] text-rose-700 dark:text-rose-400">
                  Xóa vbaProject.bin, vbaData.xml, cập nhật [Content_Types].xml &amp; lưu sang .docx/.xlsx an toàn.
                </div>
              </div>
            </label>

            <label className="flex items-start gap-2.5 p-3 rounded-xl border border-slate-200 dark:border-slate-800 cursor-pointer">
              <input
                type="checkbox"
                checked={config.security.unlockProtection}
                onChange={(e) => updateSecurity('unlockProtection', e.target.checked)}
                className="mt-0.5 rounded border-slate-300 text-blue-600"
              />
              <div>
                <div className="font-semibold flex items-center gap-1">
                  <Unlock className="w-3.5 h-3.5 text-emerald-600" />
                  Mở khóa Restrict Editing / Sheet Protection
                </div>
                <div className="text-[11px] text-slate-500">
                  Gỡ bỏ thẻ khóa chỉnh sửa trong Word và Sheet/Workbook Protection trong Excel.
                </div>
              </div>
            </label>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">
                Đặt khóa bảo vệ mới (Word)
              </label>
              <select
                value={config.security.lockMode}
                onChange={(e) => updateSecurity('lockMode', e.target.value as LockMode)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value="none">Không khóa</option>
                <option value="readOnly">Khóa Chỉ đọc (Read Only)</option>
                <option value="commentsOnly">Chỉ cho phép Bình luận</option>
                <option value="trackedChanges">Bắt buộc Track Changes</option>
              </select>
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Tác giả mới (Author)</label>
              <input
                type="text"
                placeholder="VD: Admin"
                value={config.security.author}
                onChange={(e) => updateSecurity('author', e.target.value)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
            <div>
              <label className="block text-[11px] font-semibold text-slate-500 mb-1">Tiêu đề tệp (Title)</label>
              <input
                type="text"
                placeholder="Để trống nếu giữ nguyên"
                value={config.security.title}
                onChange={(e) => updateSecurity('title', e.target.value)}
                className="w-full px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
            </div>
          </div>

          <label className="flex items-center gap-2 text-xs font-medium cursor-pointer">
            <input
              type="checkbox"
              checked={config.security.scrubPersonalInfo}
              onChange={(e) => updateSecurity('scrubPersonalInfo', e.target.checked)}
              className="rounded border-slate-300 text-blue-600"
            />
            Xóa sạch thông tin cá nhân ẩn (LastModifiedBy, Company, Manager, TotalTime)
          </label>
        </div>

        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-bold flex items-center gap-1.5 text-slate-700 dark:text-slate-200">
              <Terminal className="w-3.5 h-3.5 text-blue-500" />
              Nhật ký xử lý OpenXML &amp; Macro Cleaner ({activityLogs.length})
            </span>
            <button
              type="button"
              onClick={onClearLogs}
              className="text-[11px] text-slate-500 hover:text-slate-800 dark:hover:text-slate-200"
            >
              Xóa nhật ký
            </button>
          </div>
          <div className="h-44 overflow-y-auto rounded-xl bg-slate-950 text-slate-200 font-mono text-[11px] p-3 space-y-1">
            {activityLogs.length === 0 ? (
              <div className="text-slate-500">Chưa có nhật ký nào.</div>
            ) : (
              activityLogs.map((l, i) => (
                <div key={i} className="leading-relaxed break-all">
                  {l}
                </div>
              ))
            )}
          </div>

          {isNativeWin && (
            <div className="mt-4 pt-3 border-t border-slate-200 dark:border-slate-800 space-y-2">
              <div className="flex items-center justify-between gap-2">
                <div>
                  <span className="text-xs font-bold text-slate-700 dark:text-slate-200">
                    Nhật ký vận hành chuẩn hóa (Privacy-Safe · {operationLogs.length})
                  </span>
                  <p className="text-[10px] text-slate-500">
                    Chỉ lưu thời gian, tác vụ, tên tệp, module, trạng thái, ms &amp; mã lỗi — không lưu nội dung tài liệu.
                  </p>
                </div>
                <div className="flex items-center gap-2 shrink-0 text-[11px]">
                  {onRefreshOpLogs && (
                    <button
                      type="button"
                      onClick={onRefreshOpLogs}
                      className="inline-flex items-center gap-1 px-2 py-1 rounded bg-slate-100 dark:bg-slate-800 hover:bg-blue-600 hover:text-white font-medium"
                    >
                      <RefreshCw className="w-3 h-3" /> Làm mới
                    </button>
                  )}
                  {onClearOpLogs && (
                    <button
                      type="button"
                      onClick={onClearOpLogs}
                      className="text-slate-500 hover:text-rose-600"
                    >
                      Xóa log hệ thống
                    </button>
                  )}
                </div>
              </div>
              <div className="max-h-44 overflow-y-auto rounded-xl border border-slate-200 dark:border-slate-800 text-[11px]">
                {operationLogs.length === 0 ? (
                  <div className="p-3 text-slate-500 text-center">Chưa ghi nhận nhật ký hệ thống nào.</div>
                ) : (
                  <table className="w-full text-left border-collapse">
                    <thead className="bg-slate-50 dark:bg-slate-950 text-[10px] uppercase text-slate-500 sticky top-0">
                      <tr>
                        <th className="px-2 py-1.5">Thời gian</th>
                        <th className="px-2 py-1.5">Module / Tác vụ</th>
                        <th className="px-2 py-1.5">Tệp</th>
                        <th className="px-2 py-1.5">Trạng thái</th>
                        <th className="px-2 py-1.5 text-right">ms</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-100 dark:divide-slate-800 font-mono text-[10px]">
                      {operationLogs.slice(0, 60).map((entry, idx) => (
                        <tr key={idx} className="hover:bg-slate-50/80 dark:hover:bg-slate-800/40">
                          <td className="px-2 py-1 whitespace-nowrap text-slate-500">{entry.timestamp}</td>
                          <td className="px-2 py-1">{entry.module}.{entry.task}</td>
                          <td className="px-2 py-1 max-w-[140px] truncate" title={entry.fileName}>{entry.fileName}</td>
                          <td className="px-2 py-1">
                            <span
                              className={`px-1 py-0.5 rounded font-bold ${
                                entry.status === 'ok'
                                  ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300'
                                  : entry.status === 'cancelled'
                                  ? 'bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300'
                                  : 'bg-rose-100 text-rose-700 dark:bg-rose-950 dark:text-rose-300'
                              }`}
                            >
                              {entry.errorCode || entry.status}
                            </span>
                          </td>
                          <td className="px-2 py-1 text-right text-slate-500">{entry.elapsedMs}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  // TAB 6: SYSTEM & ADD-INS
  if (!isNativeWin) {
    return (
      <div className="rounded-2xl border border-indigo-200 dark:border-indigo-900 bg-white dark:bg-slate-900 p-5 shadow-sm space-y-3">
        <div className="flex items-center gap-2 text-indigo-700 dark:text-indigo-300 font-bold text-sm">
          <Settings2 className="w-4 h-4" />
          Tính năng Hệ thống Office dành cho bản Windows Desktop (.exe)
        </div>
        <p className="text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
          Chỉnh cấu hình Registry Microsoft Word (tắt Protected View, Start Screen), cập nhật mẫu chuẩn{' '}
          <span className="font-mono">Normal.dotm</span>, cài đặt Add-in và cài font chữ hàng loạt đòi hỏi quyền hệ điều hành Windows qua ứng dụng{' '}
          <strong>MonWord.exe</strong>.
        </p>
        <button
          type="button"
          onClick={onOpenDesktopDownload}
          className="inline-flex items-center gap-1.5 px-4 py-2 rounded-xl bg-blue-600 hover:bg-blue-700 text-white text-xs font-semibold shadow-sm"
        >
          <Download className="w-4 h-4" />
          Tải MonWord Desktop cho Windows (.exe)
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="rounded-2xl border border-indigo-200 dark:border-indigo-900 bg-indigo-50/70 dark:bg-indigo-950/30 p-3.5 text-xs flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 text-indigo-900 dark:text-indigo-200">
          <Settings2 className="w-4 h-4 shrink-0" />
          <span>
            Khu vực <strong>Cấu hình Hệ thống Office</strong> tách biệt hoàn toàn với danh sách File đang chọn.
            {systemStatus?.officeVersion ? ` (Phát hiện Office ${systemStatus.officeVersion})` : ''}
          </span>
        </div>
        <button
          type="button"
          onClick={() => refreshSystemAndAddins()}
          disabled={loadingSystem}
          className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-white dark:bg-slate-900 border border-indigo-200 dark:border-indigo-800 text-[11px] font-semibold shrink-0"
        >
          <RefreshCw className={`w-3 h-3 ${loadingSystem ? 'animate-spin' : ''}`} />
          Làm mới
        </button>
      </div>

      <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-5 shadow-sm space-y-3">
        <h3 className="text-sm font-bold">Cấu hình Nhanh Microsoft Office (HKCU Registry)</h3>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
          <div className="p-3 rounded-xl border border-slate-200 dark:border-slate-800 flex items-center justify-between gap-2">
            <div>
              <div className="font-semibold">Tắt Protected View</div>
              <div className="text-[11px] text-slate-500">
                Trạng thái: {systemStatus?.wordProtectedView ? 'Đang bật bảo vệ' : 'Đã tắt'}
              </div>
            </div>
            <button
              type="button"
              onClick={() =>
                applySystemTweak({
                  disableProtectedView: systemStatus?.wordProtectedView ?? true,
                })
              }
              className="px-2.5 py-1 rounded-lg bg-slate-100 dark:bg-slate-800 font-semibold hover:bg-blue-600 hover:text-white transition-colors"
            >
              {systemStatus?.wordProtectedView ? 'Tắt ngay' : 'Bật lại'}
            </button>
          </div>

          <div className="p-3 rounded-xl border border-slate-200 dark:border-slate-800 flex items-center justify-between gap-2">
            <div>
              <div className="font-semibold">Bỏ Start Screen khi mở Office</div>
              <div className="text-[11px] text-slate-500">
                Trạng thái: {systemStatus?.wordStartScreenDisabled ? 'Đã tắt' : 'Đang hiển thị'}
              </div>
            </div>
            <button
              type="button"
              onClick={() =>
                applySystemTweak({
                  disableStartScreen: !(systemStatus?.wordStartScreenDisabled ?? false),
                })
              }
              className="px-2.5 py-1 rounded-lg bg-slate-100 dark:bg-slate-800 font-semibold hover:bg-blue-600 hover:text-white transition-colors"
            >
              {systemStatus?.wordStartScreenDisabled ? 'Bật lại' : 'Tắt ngay'}
            </button>
          </div>

          <div className="p-3 rounded-xl border border-slate-200 dark:border-slate-800 flex items-center justify-between gap-2">
            <div>
              <div className="font-semibold">Tắt gạch chân Chính tả / Grammar</div>
              <div className="text-[11px] text-slate-500">
                Trạng thái: {systemStatus?.wordSpellCheckDisabled ? 'Đã tắt' : 'Đang bật'}
              </div>
            </div>
            <button
              type="button"
              onClick={() =>
                applySystemTweak({
                  disableSpellCheck: !(systemStatus?.wordSpellCheckDisabled ?? false),
                })
              }
              className="px-2.5 py-1 rounded-lg bg-slate-100 dark:bg-slate-800 font-semibold hover:bg-blue-600 hover:text-white transition-colors"
            >
              {systemStatus?.wordSpellCheckDisabled ? 'Bật lại' : 'Tắt ngay'}
            </button>
          </div>

          <div className="p-3 rounded-xl border border-blue-200 dark:border-blue-900 bg-blue-50/40 dark:bg-blue-950/20 flex items-center justify-between gap-2">
            <div>
              <div className="font-semibold text-blue-900 dark:text-blue-200">Đặt Normal.dotm chuẩn NĐ30</div>
              <div className="text-[11px] text-slate-500 truncate max-w-[180px]">
                {systemStatus?.normalDotmPath ? 'Đã tìm thấy Normal.dotm' : 'Chưa phát hiện tệp'}
              </div>
            </div>
            <button
              type="button"
              onClick={() => applySystemTweak({ applyNormalDotmNd30: true })}
              className="px-2.5 py-1 rounded-lg bg-blue-600 text-white font-semibold hover:bg-blue-700"
            >
              Áp dụng
            </button>
          </div>
        </div>

        <div className="p-3 rounded-xl border border-amber-300 dark:border-amber-800 bg-amber-50/70 dark:bg-amber-950/30 flex items-center justify-between gap-3">
          <div className="flex items-start gap-2 text-xs">
            <ShieldAlert className="w-4 h-4 text-amber-600 shrink-0 mt-0.5" />
            <div>
              <div className="font-bold text-amber-900 dark:text-amber-200">
                Cấu hình chạy Macro &amp; VBA toàn cục (Lưu ý bảo mật)
              </div>
              <div className="text-[11px] text-amber-800 dark:text-amber-400">
                Chỉ bật khi cần chạy Add-in nội bộ tin cậy. Mức an toàn mặc định là VBAWarnings=2. (Hiện tại:{' '}
                {systemStatus?.vbaWarningsLevel ?? 'Mặc định'})
              </div>
            </div>
          </div>
          <div className="flex gap-1.5 shrink-0">
            <button
              type="button"
              onClick={() => applySystemTweak({ enableVbaMacros: true })}
              className="px-2.5 py-1 rounded-lg bg-amber-600 text-white text-xs font-semibold hover:bg-amber-700"
            >
              Bật Macro
            </button>
            <button
              type="button"
              onClick={() => applySystemTweak({ enableVbaMacros: false })}
              className="px-2.5 py-1 rounded-lg bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 text-xs font-semibold"
            >
              Mức an toàn
            </button>
          </div>
        </div>
      </div>

      <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-5 shadow-sm space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-bold">Quản lý Add-in &amp; Font chữ</h3>
            <div className="inline-flex rounded-lg border border-slate-200 dark:border-slate-700 p-0.5 bg-slate-100 dark:bg-slate-800 text-xs">
              {(['word', 'excel'] as const).map((app) => (
                <button
                  key={app}
                  type="button"
                  onClick={() => {
                    setAddinApp(app);
                    refreshSystemAndAddins(app);
                  }}
                  className={`px-2.5 py-0.5 rounded-md font-semibold capitalize ${
                    addinApp === app ? 'bg-white dark:bg-slate-900 text-blue-600 shadow-sm' : 'text-slate-500'
                  }`}
                >
                  {app}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleAddAddin}
              className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-emerald-600 text-white text-xs font-semibold hover:bg-emerald-700"
            >
              <Plus className="w-3.5 h-3.5" /> Thêm Add-in
            </button>
            <button
              type="button"
              onClick={handleInstallFonts}
              className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-amber-500 text-white text-xs font-semibold hover:bg-amber-600"
            >
              <FolderPlus className="w-3.5 h-3.5" /> Cài Font chữ (.ttf/.otf)
            </button>
          </div>
        </div>

        <div className="rounded-xl border border-slate-200 dark:border-slate-800 divide-y divide-slate-200 dark:divide-slate-800 max-h-48 overflow-y-auto">
          {addins.length === 0 ? (
            <div className="p-4 text-center text-xs text-slate-500">
              Chưa phát hiện Add-in nào trong thư mục Startup/AddIns của {addinApp.toUpperCase()}.
            </div>
          ) : (
            addins.map((ad) => (
              <div key={ad.path} className="px-3 py-2 flex items-center justify-between gap-3 text-xs">
                <div className="min-w-0">
                  <div className="font-semibold flex items-center gap-2">
                    <span className="truncate">{ad.fileName}</span>
                    <span
                      className={`text-[10px] px-1.5 py-0.5 rounded font-bold ${
                        ad.enabled
                          ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300'
                          : 'bg-slate-200 text-slate-600 dark:bg-slate-800 dark:text-slate-400'
                      }`}
                    >
                      {ad.enabled ? 'Đang bật' : 'Đã tắt'}
                    </span>
                  </div>
                  <div className="text-[10px] text-slate-500 truncate">{ad.path}</div>
                </div>
                <div className="flex items-center gap-1.5 shrink-0">
                  <button
                    type="button"
                    onClick={() => handleManageAddin('toggle', ad.path)}
                    className="px-2 py-1 rounded bg-slate-100 dark:bg-slate-800 hover:bg-blue-600 hover:text-white font-medium"
                  >
                    {ad.enabled ? 'Tắt' : 'Bật'}
                  </button>
                  <button
                    type="button"
                    onClick={() => handleManageAddin('delete', ad.path)}
                    className="p-1 rounded text-slate-400 hover:text-rose-600"
                    title="Xóa Add-in"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
    );
  };

  return (
    <div className="space-y-4">
      {isSubTab && onNavigate && (
        <AdvancedSubNavBar activeTab={activeTab} onNavigate={onNavigate} />
      )}
      {renderPanelBody()}
    </div>
  );
};
