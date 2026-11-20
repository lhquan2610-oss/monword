import { Toast } from './Toast';
import React from 'react';
import {
  Download,
  ExternalLink,
  Info,
  Moon,
  Play,
  RefreshCw,
  SearchCheck,
  ShieldCheck,
  Sun,
  FolderOpen,
} from 'lucide-react';
import type {
  AuditReport,
  FileProcessResult,
  FileQueueItem,
  NavTab,
  PlatformEdition,
  ProcessConfig,
  ProgressPayload,
  UILayoutOverride,
  UpdateCheckResult,
} from '../types';
import { A4LivePreview } from './A4LivePreview';
import { ConfigPanels, type ConfigPanelsProps } from './ConfigPanels';
import { FileQueuePanel } from './FileQueuePanel';
import { PlatformSwitcher, VersionOtaPill } from './PlatformSwitcher';
import { WorkspaceHome } from './WorkspaceHome';
import { WorkspaceNavigation } from './WorkspaceNavigation';

interface WebWorkspaceProps {
  darkMode: boolean;
  onToggleDark: () => void;
  edition: PlatformEdition;
  override: UILayoutOverride;
  onChangeOverride: (next: UILayoutOverride) => void;
  currentVersion: string;
  updateInfo: UpdateCheckResult | null;
  onOpenUpdate: () => void;
  onOpenAbout: () => void;
  activeTab: NavTab;
  setActiveTab: (tab: NavTab) => void;
  configPanelsProps: ConfigPanelsProps;
  config: ProcessConfig;
  files: FileQueueItem[];
  audits: Record<string, AuditReport>;
  results: Record<string, FileProcessResult>;
  isDragging: boolean;
  onWebDragOver: (e: React.DragEvent) => void;
  onWebDragLeave: () => void;
  onWebDrop: (e: React.DragEvent) => void;
  onPickFiles: () => void;
  onRemoveFile: (path: string) => void;
  onClearAll: () => void;
  onOpenReport: (report: AuditReport) => void;
  onDownloadOutput: (blobUrl: string, fileName: string) => void;
  onShareOutput: (res: FileProcessResult) => void;
  onAuditFiles: () => void;
  onProcessFiles: () => void;
  busy: boolean;
  progress: ProgressPayload | null;
  toast: { type: 'ok' | 'err'; text: string } | null;
  onOpenDesktopDownload: () => void;
}

export const WebWorkspace: React.FC<WebWorkspaceProps> = ({
  darkMode,
  onToggleDark,
  edition,
  override,
  onChangeOverride,
  currentVersion,
  updateInfo,
  onOpenUpdate,
  onOpenAbout,
  activeTab,
  setActiveTab,
  configPanelsProps,
  config,
  files,
  audits,
  results,
  isDragging,
  onWebDragOver,
  onWebDragLeave,
  onWebDrop,
  onPickFiles,
  onRemoveFile,
  onClearAll,
  onOpenReport,
  onDownloadOutput,
  onShareOutput,
  onAuditFiles,
  onProcessFiles,
  busy,
  progress,
  toast,
  onOpenDesktopDownload,
}) => {
  return (
    <div
      onDragOver={onWebDragOver}
      onDragLeave={onWebDragLeave}
      onDrop={onWebDrop}
      className="min-h-screen flex flex-col mw-canvas"
    >
      {/* Glass Web Header */}
      <header className="sticky top-0 z-30 border-b border-slate-200/80 dark:border-slate-800 bg-white/85 dark:bg-slate-900/85 backdrop-blur-md">
        <div className="max-w-7xl mx-auto px-4 h-14 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2.5 min-w-0">
            <img
              src="/monword-icon.png"
              alt="MonWord"
              className="w-9 h-9 object-contain drop-shadow-sm shrink-0"
            />
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="text-base font-extrabold tracking-tight">
                  <span className="text-[#35283C] dark:text-slate-100">Mon</span>
                  <span className="text-[#2563EB] dark:text-blue-400">Word</span>
                </span>
                <VersionOtaPill
                  currentVersion={currentVersion}
                  updateInfo={updateInfo}
                  onOpenUpdate={onOpenUpdate}
                  editionLabel="Web Auto-OTA"
                />
                <span className="hidden lg:inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-50 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
                  <ShieldCheck className="w-3 h-3" /> Xử lý 100% tại trình duyệt
                </span>
              </div>
              <p className="text-[11px] text-slate-500 dark:text-slate-400 truncate hidden sm:block">
                Chuẩn hóa văn bản NĐ 30/2020 &amp; Văn bản Đảng · monword.vercel.app
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <PlatformSwitcher
              edition={edition}
              override={override}
              onChangeOverride={onChangeOverride}
              compact
            />

            <button
              type="button"
              onClick={onOpenDesktopDownload}
              className="mw-button mw-button-secondary hidden md:inline-flex"
              title="Tải MonWord Desktop (.exe) hỗ trợ .doc cũ, Word COM & Registry"
            >
              <Download className="w-3.5 h-3.5" /> Bản Windows (.exe)
            </button>

            <button
              type="button"
              onClick={onOpenAbout}
              className="p-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300"
              title="Thông tin phiên bản & bảo mật"
            >
              <Info className="w-4 h-4" />
            </button>

            <button
              type="button"
              onClick={onToggleDark}
              className="p-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300"
              title="Đổi giao diện Sáng / Tối"
            >
              {darkMode ? <Sun className="w-4 h-4 text-amber-400" /> : <Moon className="w-4 h-4" />}
            </button>
          </div>
        </div>
      </header>

      <div className="mx-auto grid w-full max-w-screen-2xl flex-1 items-start gap-5 px-4 py-5 lg:grid-cols-[13rem_minmax(0,1fr)] lg:px-6">
        <aside className="mw-panel p-2 lg:sticky lg:top-20 lg:p-3">
          <WorkspaceNavigation activeTab={activeTab} onSelect={setActiveTab} />
        </aside>

        <main className="min-w-0 space-y-4">
          {toast && <Toast {...toast} />}
          {activeTab === 'home' ? (
            <WorkspaceHome
              files={files}
              onPickFiles={onPickFiles}
              onTask={setActiveTab}
              onShowFiles={() => setActiveTab('oneClick')}
              progress={busy ? progress : null}
              isDragging={isDragging}
            />
          ) : (
            <>
              <div role="group" aria-label="Thao tác tài liệu" className="mw-panel flex flex-wrap items-center justify-between gap-3 p-3">
                <div className="flex flex-wrap items-center gap-3 text-xs">
                  <span className="font-bold text-slate-700 dark:text-slate-200">
                    {files.length} tệp trong hàng đợi
                  </span>
                  {busy && progress && (
                    <span role="status" className="inline-flex items-center gap-1 text-slate-700 dark:text-slate-200 font-semibold">
                      <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                      Đang xử lý {progress.current}/{progress.total}
                    </span>
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <button type="button" onClick={onPickFiles} className="mw-button mw-button-secondary">
                    <FolderOpen className="size-4" aria-hidden="true" /> Mở tài liệu
                  </button>
                  <button
                    type="button"
                    disabled={busy || files.length === 0}
                    onClick={onAuditFiles}
                    className="mw-button mw-button-secondary"
                  >
                    <SearchCheck className="w-4 h-4 text-indigo-500" />
                    Kiểm tra lỗi
                  </button>
                  <button
                    type="button"
                    disabled={busy || files.length === 0}
                    onClick={onProcessFiles}
                    className="mw-button mw-button-primary"
                  >
                    <Play className="w-3.5 h-3.5 fill-current" />
                    Chuẩn hóa nhanh
                  </button>
                </div>
              </div>
              {/* 2-Column Web Studio (full width for Live A4 Preview Studio) */}
              {activeTab === 'livePreview' ? (
                <div className="min-w-0">
                  <ConfigPanels {...configPanelsProps} />
                </div>
              ) : (
                <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,1fr)_19rem] gap-4 items-start">
                  <div className="min-w-0 space-y-4">
                    <ConfigPanels {...configPanelsProps} />
                  </div>
                  <div className="min-w-0 space-y-4">
                    <FileQueuePanel
                      files={files}
                      audits={audits}
                      results={results}
                      isDragging={isDragging}
                      isBrowserMode
                      onPickFiles={onPickFiles}
                      onPickFolder={onPickFiles}
                      onRemoveFile={onRemoveFile}
                      onClearAll={onClearAll}
                      onOpenReport={onOpenReport}
                      onOpenOutput={() => {}}
                      onDownloadOutput={onDownloadOutput}
                      onShareOutput={onShareOutput}
                      onRestoreBackup={() => {}}
                    />
                    <A4LivePreview
                      config={config}
                      fileCount={files.length}
                      onOpenFullPreview={() => setActiveTab('livePreview')}
                    />
                  </div>
                </div>
              )}
            </>
          )}
        </main>
      </div>

      {/* Ecosystem Footer */}
      <footer className="border-t border-slate-200 dark:border-slate-800 bg-white/80 dark:bg-slate-900/80 py-3 px-4 text-xs text-slate-500 dark:text-slate-400">
        <div className="max-w-7xl mx-auto flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <span className="font-semibold text-slate-700 dark:text-slate-200">
              Hệ sinh thái Văn phòng Mon:
            </span>
            <a
              href="https://monpdf.vn"
              target="_blank"
              rel="noreferrer"
              className="text-rose-600 hover:underline inline-flex items-center gap-0.5 font-medium"
            >
              MonPDF <ExternalLink className="w-3 h-3" />
            </a>
            <span>·</span>
            <span className="text-blue-600 font-bold">MonWord v{currentVersion}</span>
            <span>·</span>
            <span className="text-emerald-600 font-medium">MonEx</span>
          </div>
          <div>Auto-Updated via Vercel Edge &amp; Live OTA</div>
        </div>
      </footer>
    </div>
  );
};
