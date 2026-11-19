import React from 'react';
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  ExternalLink,
  Info,
  Moon,
  Play,
  RefreshCw,
  SearchCheck,
  ShieldCheck,
  Sun,
  UploadCloud,
} from 'lucide-react';
import type {
  AuditReport,
  FileProcessResult,
  FileQueueItem,
  NavTab,
  PlatformEdition,
  PresetInfo,
  ProcessConfig,
  ProgressPayload,
  UILayoutOverride,
  UpdateCheckResult,
} from '../types';
import { A4LivePreview } from './A4LivePreview';
import { ConfigPanels, type ConfigPanelsProps } from './ConfigPanels';
import { FileQueuePanel } from './FileQueuePanel';
import { PlatformSwitcher, VersionOtaPill } from './PlatformSwitcher';

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
  navItems: { id: NavTab; label: string; group: 'doc' | 'sys'; icon: React.ReactNode }[];
  configPanelsProps: ConfigPanelsProps;
  presets: PresetInfo[];
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
  navItems,
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
      className="min-h-screen flex flex-col bg-gradient-to-b from-slate-50 via-blue-50/25 to-slate-100 dark:from-slate-950 dark:via-slate-900 dark:to-slate-950 text-slate-900 dark:text-slate-100"
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
              className="hidden md:inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-slate-900 dark:bg-blue-600 hover:bg-slate-800 dark:hover:bg-blue-700 text-white text-xs font-semibold shadow-sm"
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

      {/* Main Container */}
      <div className="flex-1 max-w-7xl w-full mx-auto px-4 py-5 space-y-4">
        {toast && (
          <div
            className={`px-4 py-2.5 rounded-xl border text-xs font-semibold flex items-center gap-2 shadow-sm ${
              toast.type === 'ok'
                ? 'bg-emerald-50 border-emerald-200 text-emerald-800 dark:bg-emerald-950/70 dark:border-emerald-800 dark:text-emerald-200'
                : 'bg-rose-50 border-rose-200 text-rose-800 dark:bg-rose-950/70 dark:border-rose-800 dark:text-rose-200'
            }`}
          >
            {toast.type === 'ok' ? <CheckCircle2 className="w-4 h-4 shrink-0" /> : <AlertTriangle className="w-4 h-4 shrink-0" />}
            <span>{toast.text}</span>
          </div>
        )}

        {/* Top Web Dropzone Hero + Primary Action */}
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-stretch">
          <div
            onClick={onPickFiles}
            className={`lg:col-span-7 rounded-2xl border-2 border-dashed p-5 flex flex-col sm:flex-row items-center justify-between gap-4 cursor-pointer transition-all ${
              isDragging
                ? 'border-blue-600 bg-blue-50/90 dark:bg-blue-950/40 ring-4 ring-blue-500/20'
                : 'border-blue-300/80 dark:border-blue-800 bg-white/90 dark:bg-slate-900 hover:border-blue-500 shadow-sm'
            }`}
          >
            <div className="flex items-center gap-4 text-center sm:text-left">
              <div className="w-12 h-12 rounded-2xl bg-blue-600 text-white flex items-center justify-center shadow-md shrink-0">
                <UploadCloud className="w-6 h-6" />
              </div>
              <div>
                <h1 className="text-sm sm:text-base font-extrabold text-slate-900 dark:text-slate-100">
                  Kéo thả hoặc chọn tệp Word (.docx, .docm) để chuẩn hóa tức thì
                </h1>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-1">
                  Toàn bộ thao tác căn lề NĐ 30/2020, sửa phông chữ, bảng biểu &amp; tiếng Việt chạy ngay trên trình duyệt của bạn.
                </p>
              </div>
            </div>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onPickFiles();
              }}
              className="px-4 py-2 rounded-xl bg-blue-600 hover:bg-blue-700 text-white text-xs font-bold shadow-sm shrink-0"
            >
              Chọn tệp .docx
            </button>
          </div>

          <div className="lg:col-span-5 rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm flex flex-col justify-between gap-3">
            <div className="flex items-center justify-between text-xs">
              <span className="font-bold text-slate-700 dark:text-slate-200">
                Thao tác Web ({files.length} tệp trong hàng đợi)
              </span>
              {busy && progress && (
                <span className="inline-flex items-center gap-1 text-blue-600 font-semibold">
                  <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                  {progress.current}/{progress.total}
                </span>
              )}
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                disabled={busy || files.length === 0}
                onClick={onAuditFiles}
                className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-2 rounded-xl border border-slate-300 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 hover:bg-slate-100 text-xs font-bold disabled:opacity-50"
              >
                <SearchCheck className="w-4 h-4 text-indigo-500" />
                Kiểm tra lỗi
              </button>
              <button
                type="button"
                disabled={busy || files.length === 0}
                onClick={onProcessFiles}
                className="flex-[1.5] inline-flex items-center justify-center gap-1.5 px-4 py-2 rounded-xl bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-700 hover:to-indigo-700 text-white text-xs font-bold shadow-sm disabled:opacity-50"
              >
                <Play className="w-3.5 h-3.5 fill-current" />
                CHUẨN HÓA &amp; TẢI VỀ
              </button>
            </div>
          </div>
        </div>

        {/* Horizontal Navigation Tabs */}
        <div className="flex items-center gap-1.5 overflow-x-auto pb-1">
          {navItems.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => setActiveTab(item.id)}
              className={`inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl text-xs font-semibold whitespace-nowrap border transition-all ${
                activeTab === item.id
                  ? 'bg-blue-600 text-white border-blue-600 shadow-sm'
                  : 'bg-white dark:bg-slate-900 text-slate-600 dark:text-slate-300 border-slate-200 dark:border-slate-800 hover:border-blue-300'
              }`}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          ))}
        </div>

        {/* 2-Column Web Studio */}
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start">
          <div className="lg:col-span-8 space-y-4">
            <ConfigPanels {...configPanelsProps} />
          </div>
          <div className="lg:col-span-4 space-y-4">
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
            <A4LivePreview config={config} />
          </div>
        </div>
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
