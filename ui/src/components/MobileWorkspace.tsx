import { Toast } from './Toast';
import React, { useState } from 'react';
import {
  Download,
  FileText,
  FolderOpen,
  Home,
  Info,
  Play,
  RefreshCw,
  SearchCheck,
  Settings2,
  Share2,
  SlidersHorizontal,
  Trash2,
  Zap,
} from 'lucide-react';
import type {
  AuditReport,
  FileProcessResult,
  FileQueueItem,
  NavTab,
  PlatformEdition,
  PresetId,
  PresetInfo,
  ProcessConfig,
  ProgressPayload,
  UILayoutOverride,
  UpdateCheckResult,
} from '../types';
import { ConfigPanels, type ConfigPanelsProps } from './ConfigPanels';
import { PlatformSwitcher, VersionOtaPill } from './PlatformSwitcher';
import { WorkspaceHome } from './WorkspaceHome';
import { WorkspaceNavigation } from './WorkspaceNavigation';

type MobileTab = 'home' | 'files' | 'rules' | 'ota';

interface MobileWorkspaceProps {
  edition: PlatformEdition;
  override: UILayoutOverride;
  onChangeOverride: (next: UILayoutOverride) => void;
  currentVersion: string;
  updateInfo: UpdateCheckResult | null;
  onOpenUpdate: () => void;
  onOpenAbout: () => void;
  presets: PresetInfo[];
  config: ProcessConfig;
  applyPreset: (presetId: PresetId) => void;
  configPanelsProps: ConfigPanelsProps;
  activeConfigTab: NavTab;
  setActiveConfigTab: (tab: NavTab) => void;
  files: FileQueueItem[];
  audits: Record<string, AuditReport>;
  results: Record<string, FileProcessResult>;
  onPickFiles: () => void;
  onRemoveFile: (path: string) => void;
  onOpenReport: (report: AuditReport) => void;
  onDownloadOutput: (blobUrl: string, fileName: string) => void;
  onShareOutput: (res: FileProcessResult) => void;
  onAuditFiles: () => void;
  onProcessFiles: () => void;
  busy: boolean;
  progress: ProgressPayload | null;
  toast: { type: 'ok' | 'err'; text: string } | null;
}

export const MobileWorkspace: React.FC<MobileWorkspaceProps> = ({
  edition,
  override,
  onChangeOverride,
  currentVersion,
  updateInfo,
  onOpenUpdate,
  onOpenAbout,
  presets,
  config,
  applyPreset,
  configPanelsProps,
  activeConfigTab,
  setActiveConfigTab,
  files,
  audits,
  results,
  onPickFiles,
  onRemoveFile,
  onOpenReport,
  onDownloadOutput,
  onShareOutput,
  onAuditFiles,
  onProcessFiles,
  busy,
  progress,
  toast,
}) => {
  const [mobileTab, setMobileTab] = useState<MobileTab>('home');

  return (
    <div className="min-h-screen flex flex-col mw-canvas pb-20">
      {/* Sticky Safe-Area Mobile Header */}
      <header className="sticky top-0 z-30 bg-white/95 dark:bg-slate-900/95 backdrop-blur border-b border-slate-200 dark:border-slate-800 px-3.5 py-2.5 flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <img src="/monword-icon.png" alt="MonWord" className="w-8 h-8 object-contain shrink-0" />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <span className="text-sm font-extrabold tracking-tight">
                <span className="text-[#35283C] dark:text-slate-100">Mon</span>
                <span className="text-[#2563EB] dark:text-blue-400">Word</span>
              </span>
              <VersionOtaPill
                currentVersion={currentVersion}
                updateInfo={updateInfo}
                onOpenUpdate={onOpenUpdate}
                editionLabel="Mobile Live OTA"
              />
            </div>
            <p className="text-[11px] text-slate-500 dark:text-slate-400 truncate">
              Chuẩn hóa văn bản trên điện thoại
            </p>
          </div>
        </div>

        <button
          type="button"
          onClick={onOpenAbout}
          className="p-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 text-slate-600 dark:text-slate-300 shrink-0"
          aria-label="Thông tin MonWord"
        >
          <Info className="w-4 h-4" />
        </button>
      </header>

      {/* Main Content */}
      <main className="flex-1 p-3.5 space-y-3.5 max-w-xl w-full mx-auto">
        {toast && <Toast {...toast} />}

        {/* TAB HOME */}
        {mobileTab === 'home' && (
          <>
            <WorkspaceHome
              files={files}
              onPickFiles={() => {
                onPickFiles();
                setMobileTab('files');
              }}
              onTask={(tab) => {
                setActiveConfigTab(tab);
                setMobileTab('rules');
              }}
              onShowFiles={() => setMobileTab('files')}
              progress={busy ? progress : null}
              mobile
            />

            {/* Quick 2x2 Presets Grid */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-xs font-bold text-slate-700 dark:text-slate-200 flex items-center gap-1.5">
                  <Zap className="w-3.5 h-3.5 text-blue-600" /> Chọn nhanh Bộ quy chuẩn
                </span>
                <span className="text-[11px] text-slate-500">{config.format.fontSizePt}pt · NĐ30</span>
              </div>
              <div className="grid grid-cols-2 gap-2.5">
                {presets.map((p) => {
                  const active = config.preset === p.id;
                  return (
                    <button
                      key={p.id}
                      type="button"
                      onClick={() => applyPreset(p.id)}
                      className={`p-3 rounded-2xl border text-left transition-all ${
                        active
                          ? 'bg-blue-50 dark:bg-blue-950/40 border-blue-500 ring-2 ring-blue-500/20'
                          : 'bg-white dark:bg-slate-900 border-slate-200 dark:border-slate-800'
                      }`}
                    >
                      <div className="text-xs font-bold text-slate-900 dark:text-slate-100 line-clamp-1">{p.name}</div>
                      <div className="text-[10px] font-semibold text-blue-600 dark:text-blue-400 mt-1">{p.badge}</div>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Platform Preview Switcher on Mobile */}
            <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-3 flex items-center justify-between gap-2">
              <span className="text-xs font-semibold text-slate-600 dark:text-slate-300">Chuyển giao diện:</span>
              <PlatformSwitcher
                edition={edition}
                override={override}
                onChangeOverride={onChangeOverride}
                compact
              />
            </div>
          </>
        )}

        {/* TAB FILES */}
        {mobileTab === 'files' && (
          <div className="space-y-3">
            <div className="flex items-center justify-between gap-2">
              <h2 className="text-sm font-bold">Tài liệu đã chọn ({files.length})</h2>
              <button
                type="button"
                onClick={onPickFiles}
                className="mw-button mw-button-secondary"
              >
                + Thêm tệp .docx
              </button>
            </div>

            {files.length === 0 ? (
              <div className="rounded-2xl border border-dashed border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 p-8 text-center space-y-2">
                <FileText className="w-8 h-8 text-blue-500 mx-auto" />
                <p className="text-xs font-semibold">Chưa có tệp Word nào</p>
                <p className="text-[11px] text-slate-500">Bấm "+ Thêm tệp .docx" để chọn văn bản từ máy.</p>
              </div>
            ) : (
              <div className="space-y-2.5">
                {files.map((item) => {
                  const audit = audits[item.path];
                  const res = results[item.path];
                  const displayAudit = res?.auditAfter || audit;
                  return (
                    <div
                      key={item.path}
                      className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-3.5 space-y-2 shadow-sm"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="min-w-0">
                          <div className="text-xs font-bold truncate">{item.name}</div>
                          <div className="text-[11px] text-slate-500">
                            {(item.sizeBytes / 1024).toFixed(1)} KB · .{item.ext}
                          </div>
                        </div>
                        <div className="flex items-center gap-1.5 shrink-0">
                          {displayAudit && (
                            <button
                              type="button"
                              onClick={() => onOpenReport(displayAudit)}
                              className="px-2 py-0.5 rounded-lg bg-emerald-100 text-emerald-700 text-xs font-bold"
                            >
                              {displayAudit.score} điểm
                            </button>
                          )}
                          <button
                            type="button"
                            onClick={() => onRemoveFile(item.path)}
                            className="p-1 text-slate-400 hover:text-rose-500"
                          >
                            <Trash2 className="w-4 h-4" />
                          </button>
                        </div>
                      </div>

                      {res && (
                        <div className="pt-2 border-t border-slate-200 dark:border-slate-800 space-y-2">
                          {res.success ? (
                            <>
                              <div className="text-[11px] font-medium text-emerald-600">{res.statsSummary}</div>
                              <div className="flex gap-2">
                                {res.blobUrl && (
                                  <button
                                    type="button"
                                    onClick={() => onDownloadOutput(res.blobUrl!, res.outputName || res.outputPath)}
                                    className="mw-button mw-button-primary min-h-11 flex-1"
                                  >
                                    <Download className="w-3.5 h-3.5" /> Tải về
                                  </button>
                                )}
                                {res.outputBlob && (
                                  <button
                                    type="button"
                                    onClick={() => onShareOutput(res)}
                                    className="flex-1 py-2 rounded-xl bg-emerald-600 text-white text-xs font-bold flex items-center justify-center gap-1"
                                  >
                                    <Share2 className="w-3.5 h-3.5" /> Chia sẻ Zalo/Mail
                                  </button>
                                )}
                              </div>
                            </>
                          ) : (
                            <div className="text-[11px] text-rose-600">{res.error}</div>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

            {files.length > 0 && (
              <div className="flex gap-2 pt-1">
                <button
                  type="button"
                  disabled={busy}
                  onClick={onAuditFiles}
                  className="flex-1 py-2.5 rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs font-bold flex items-center justify-center gap-1.5"
                >
                  <SearchCheck className="w-4 h-4 text-indigo-500" />
                  {busy && progress?.phase === 'audit' ? `${progress.current}/${progress.total}` : 'Kiểm tra lỗi'}
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={onProcessFiles}
                  className="mw-button mw-button-primary min-h-11 flex-[1.4]"
                >
                  <Play className="w-3.5 h-3.5 fill-current" />
                  Chuẩn hóa ngay
                </button>
              </div>
            )}
          </div>
        )}

        {/* TAB RULES */}
        {mobileTab === 'rules' && (
          <div className="space-y-3">
            <WorkspaceNavigation
              activeTab={activeConfigTab === 'home' ? 'oneClick' : activeConfigTab}
              onSelect={setActiveConfigTab}
              hideHome
            />
            <ConfigPanels {...configPanelsProps} activeTab={activeConfigTab === 'home' ? 'oneClick' : activeConfigTab} />
          </div>
        )}

        {/* TAB LIVE OTA */}
        {mobileTab === 'ota' && (
          <div className="space-y-3">
            <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-3 shadow-sm">
              <div className="flex items-center justify-between">
                <div>
                  <div className="text-sm font-bold">MonWord Live OTA</div>
                  <div className="text-xs text-slate-500">Phiên bản hiện tại: v{currentVersion}</div>
                </div>
                <RefreshCw className="w-5 h-5 text-blue-600" />
              </div>
              <p className="text-xs text-slate-600 dark:text-slate-400 leading-relaxed">
                Khi có quy chuẩn mới hoặc bản cập nhật giao diện từ <span className="font-mono text-blue-600">monword.vercel.app</span>,
                chỉ cần bấm đồng bộ ngay mà không cần cài lại ứng dụng.
              </p>
              <button
                type="button"
                onClick={onOpenUpdate}
                className="mw-button mw-button-primary min-h-11 w-full"
              >
                Mở Trung tâm Live OTA
              </button>
            </div>

            <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-2.5 shadow-sm">
              <div className="text-xs font-bold flex items-center gap-1.5">
                <Settings2 className="w-4 h-4 text-slate-500" /> Chuyển chế độ hiển thị
              </div>
              <PlatformSwitcher
                edition={edition}
                override={override}
                onChangeOverride={onChangeOverride}
              />
            </div>
          </div>
        )}
      </main>

      {/* Fixed Bottom Navigation Dock */}
      <nav aria-label="Điều hướng chính trên điện thoại" className="fixed bottom-0 inset-x-0 z-30 bg-white/95 dark:bg-slate-900/95 backdrop-blur border-t border-slate-200 dark:border-slate-800 px-2 py-1.5 grid grid-cols-4 gap-1">
        {(
          [
            ['home', 'Trang chủ', <Home key="h" className="w-4 h-4" />],
            ['files', `Tài liệu${files.length > 0 ? ` (${files.length})` : ''}`, <FolderOpen key="f" className="w-4 h-4" />],
            ['rules', 'Công cụ', <SlidersHorizontal key="r" className="w-4 h-4" />],
            ['ota', 'Live OTA', <RefreshCw key="o" className="w-4 h-4" />],
          ] as [MobileTab, string, React.ReactNode][]
        ).map(([id, label, icon]) => {
          const active = mobileTab === id;
          return (
            <button
              key={id}
              type="button"
              aria-current={active ? 'page' : undefined}
              onClick={() => setMobileTab(id)}
              className={`flex flex-col items-center justify-center py-1.5 rounded-xl text-[11px] font-semibold transition-colors ${
                active ? 'text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-950/40' : 'text-slate-500 dark:text-slate-400'
              }`}
            >
              {icon}
              <span className="mt-0.5 truncate">{label}</span>
            </button>
          );
        })}
      </nav>
    </div>
  );
};
