import { Toast } from './components/Toast';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  FileOutput,
  FileText,
  Info,
  Lock,
  Play,
  RefreshCw,
  SearchCheck,
  Sun,
  Moon,
} from 'lucide-react';
import { bridge } from './bridge';
import { AboutDialog } from './components/AboutDialog';
import { AuditModal } from './components/AuditModal';
import { ComplianceAuditWorkspace } from './components/ComplianceAuditWorkspace';
import { ConfigPanels, type ConfigPanelsProps } from './components/ConfigPanels';
import { FileQueuePanel } from './components/FileQueuePanel';
import { HeaderBuilderWorkspace } from './components/HeaderBuilderWorkspace';
import { LayoutIntelligenceWorkspace } from './components/LayoutIntelligenceWorkspace';
import { LivePreviewWorkspace, type LivePreviewFocusTarget } from './components/LivePreviewWorkspace';
import { MobileWorkspace } from './components/MobileWorkspace';
import { VersionOtaPill } from './components/PlatformSwitcher';
import { StandardizationWorkspace } from './components/StandardizationWorkspace';
import { StatusBar } from './components/StatusBar';
import { TemplateWorkspace } from './components/TemplateWorkspace';
import { UpdateDialog } from './components/UpdateDialog';
import { WebWorkspace } from './components/WebWorkspace';
import { WorkspaceHome } from './components/WorkspaceHome';
import { WorkspaceNavigation } from './components/WorkspaceNavigation';
import {
  type AddinItem,
  type AppInfoPayload,
  type AuditReport,
  type ComplianceAuditReport,
  DEFAULT_ND30_CONFIG,
  DEFAULT_PRESETS_FALLBACK,
  type DocumentTemplate,
  type FileProcessResult,
  type FileQueueItem,
  type NavTab,
  type OperationLogEntry,
  type PresetId,
  type PresetInfo,
  type PreviewOverrideConfig,
  type ProcessConfig,
  type ProgressPayload,
  type RecentDocEntry,
  type StdDocScope,
  type StdExecutionResult,
  type SystemStatus,
  type SystemTweaksRequest,
  type TemplateApplyResult,
  type UpdateCheckResult,
  type UpdateProgressPayload,
} from './types';
import { isAdminToolsEnabled, usePlatformEdition } from './utils/platform';
import {
  APP_VERSION,
  GITHUB_RELEASES_URL,
  WEB_AUTO_UPDATED_SESSION_KEY,
  applyWebUpdate,
  checkSoftwareUpdate,
} from './utils/updater';
import {
  auditWebDocxFile,
  processWebDocxFile,
  shareOutputBlob,
  triggerBrowserDownload,
} from './utils/webDocxEngine';

const RECENT_DOCS_STORAGE_KEY = 'monword_recent_docs_v1';
const PROCESS_CONFIG_STORAGE_KEY = 'monword_process_config_v1';

function loadInitialProcessConfig(): ProcessConfig {
  if (typeof window === 'undefined') return DEFAULT_ND30_CONFIG;
  try {
    const raw = localStorage.getItem(PROCESS_CONFIG_STORAGE_KEY);
    if (!raw) return DEFAULT_ND30_CONFIG;
    const parsed = JSON.parse(raw) as Partial<ProcessConfig>;
    return {
      ...DEFAULT_ND30_CONFIG,
      ...parsed,
      format: { ...DEFAULT_ND30_CONFIG.format, ...(parsed.format || {}) },
      cleaner: { ...DEFAULT_ND30_CONFIG.cleaner, ...(parsed.cleaner || {}) },
      pageToc: { ...DEFAULT_ND30_CONFIG.pageToc, ...(parsed.pageToc || {}) },
      security: { ...DEFAULT_ND30_CONFIG.security, ...(parsed.security || {}) },
    };
  } catch {
    return DEFAULT_ND30_CONFIG;
  }
}

const VALID_TABS: ReadonlySet<string> = new Set<NavTab>([
  'home',
  'oneClick',
  'standardizer',
  'complianceAudit',
  'livePreview',
  'layoutIntelligence',
  'templateEngine',
  'headerBuilder',
  'format',
  'pageToc',
  'cleaner',
  'security',
  'system',
  'history',
  'advancedSettings',
]);

function getInitialTab(): NavTab {
  if (typeof window === 'undefined') return 'home';
  try {
    const param = new URLSearchParams(window.location.search).get('tab');
    if (param && VALID_TABS.has(param)) {
      return param as NavTab;
    }
  } catch {
    // ignore
  }
  return 'home';
}

function loadInitialRecentDocs(): RecentDocEntry[] {
  if (typeof window === 'undefined') return [];
  try {
    const raw = localStorage.getItem(RECENT_DOCS_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as RecentDocEntry[]).slice(0, 20) : [];
  } catch {
    return [];
  }
}

function getRecentEntryKey(item: RecentDocEntry): string {
  const op = item.operation || (item.status === 'selected' ? 'selected' : item.status === 'checked' ? 'audit' : 'standardize');
  return `${item.path.toLowerCase()}::${op}`;
}

function upsertRecentDocs(prev: RecentDocEntry[], updates: RecentDocEntry[]): RecentDocEntry[] {
  const map = new Map<string, RecentDocEntry>();
  for (const item of prev) {
    map.set(getRecentEntryKey(item), item);
  }
  for (const next of updates) {
    const key = getRecentEntryKey(next);
    if (next.operation) {
      map.delete(`${next.path.toLowerCase()}::selected`);
    }
    const existing = map.get(key);
    map.set(key, {
      ...existing,
      ...next,
      outputPath: next.outputPath ?? existing?.outputPath,
      backupPath: next.backupPath ?? existing?.backupPath ?? null,
    });
  }
  return Array.from(map.values())
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .slice(0, 20);
}

export function App() {
  const { edition, override, setOverride, isNativeWin, label: editionLabel } = usePlatformEdition();

  const [darkMode, setDarkMode] = useState(false);
  const [activeTab, setActiveTab] = useState<NavTab>(() => getInitialTab());
  const [presets, setPresets] = useState<PresetInfo[]>(DEFAULT_PRESETS_FALLBACK);
  const [config, setConfig] = useState<ProcessConfig>(() => loadInitialProcessConfig());

  const [files, setFiles] = useState<FileQueueItem[]>([]);
  const [audits, setAudits] = useState<Record<string, AuditReport>>({});
  const [results, setResults] = useState<Record<string, FileProcessResult>>({});
  const [recentDocs, setRecentDocs] = useState<RecentDocEntry[]>(() => loadInitialRecentDocs());
  const [selectedAudit, setSelectedAudit] = useState<AuditReport | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ProgressPayload | null>(null);
  const [activityLogs, setActivityLogs] = useState<string[]>([
    `Sẵn sàng xử lý văn bản (MonWord v${APP_VERSION}).`,
  ]);
  const [toast, setToast] = useState<{ type: 'ok' | 'err'; text: string } | null>(null);

  // Version & Live OTA state
  const [currentVersion, setCurrentVersion] = useState<string>(APP_VERSION);
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [aboutDialogOpen, setAboutDialogOpen] = useState(false);
  const [otaProgress, setOtaProgress] = useState<UpdateProgressPayload | null>(null);

  // System & Add-in state (Windows native)
  const [systemStatus, setSystemStatus] = useState<SystemStatus | null>(null);
  const [addinApp, setAddinApp] = useState<'word' | 'excel'>('word');
  const [addins, setAddins] = useState<AddinItem[]>([]);
  const [loadingSystem, setLoadingSystem] = useState(false);
  const [operationLogs, setOperationLogs] = useState<OperationLogEntry[]>([]);
  const [previewFocusTarget, setPreviewFocusTarget] = useState<LivePreviewFocusTarget | null>(null);
  const [preferredStdTarget, setPreferredStdTarget] = useState<{
    filePath: string;
    scope: StdDocScope;
  } | null>(null);

  const webFileInputRef = useRef<HTMLInputElement | null>(null);
  const busyRef = useRef(false);
  const filesCountRef = useRef(0);
  busyRef.current = busy;
  filesCountRef.current = files.length;

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode);
  }, [darkMode]);

  useEffect(() => {
    try {
      localStorage.setItem(RECENT_DOCS_STORAGE_KEY, JSON.stringify(recentDocs));
    } catch {
      // ignore storage quota
    }
  }, [recentDocs]);

  useEffect(() => {
    try {
      localStorage.setItem(PROCESS_CONFIG_STORAGE_KEY, JSON.stringify(config));
    } catch {
      // ignore storage quota
    }
  }, [config]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 4500);
    return () => clearTimeout(t);
  }, [toast]);

  const appendLog = useCallback((line: string) => {
    const time = new Date().toLocaleTimeString('vi-VN', { hour12: false });
    setActivityLogs((prev) => [`[${time}] ${line}`, ...prev].slice(0, 120));
  }, []);

  const mergeFilesToQueue = useCallback((incoming: FileQueueItem[]) => {
    if (incoming.length > 0) {
      setActiveTab((tab) => (tab === 'home' ? 'standardizer' : tab));
      const now = new Date().toISOString();
      setRecentDocs((prev) =>
        upsertRecentDocs(
          prev,
          incoming.map((f) => ({
            path: f.path,
            name: f.name,
            ext: f.ext,
            updatedAt: now,
            status: 'selected',
          })),
        ),
      );
    }
    setFiles((prev) => {
      const map = new Map<string, FileQueueItem>();
      for (const f of prev) map.set(f.path.toLowerCase(), f);
      for (const f of incoming) map.set(f.path.toLowerCase(), f);
      return Array.from(map.values());
    });
  }, []);

  const addWebFiles = useCallback(
    (fileList: FileList | File[]) => {
      const incoming: FileQueueItem[] = [];
      const arr = Array.from(fileList);
      for (const f of arr) {
        const extMatch = f.name.match(/\.([^.]+)$/);
        const ext = extMatch ? extMatch[1].toLowerCase() : '';
        if (!['docx', 'docm', 'doc', 'xlsx', 'xlsm'].includes(ext)) continue;
        incoming.push({
          path: `web://${f.name}::${f.size}::${f.lastModified}`,
          name: f.name,
          ext,
          sizeBytes: f.size,
          webFile: f,
        });
      }
      if (incoming.length === 0) {
        setToast({
          type: 'err',
          text: 'Vui lòng chọn tệp Word (.docx, .docm) hợp lệ.',
        });
        return;
      }
      mergeFilesToQueue(incoming);
      appendLog(`Đã nạp ${incoming.length} tệp từ trình duyệt (xử lý cục bộ tại máy).`);
    },
    [appendLog, mergeFilesToQueue],
  );

  // Native WebView2 IPC + Background Live OTA check
  useEffect(() => {
    if (bridge.isNativeHost()) {
      bridge
        .invoke<PresetInfo[]>('get_presets')
        .then((list) => {
          if (Array.isArray(list) && list.length > 0) setPresets(list);
        })
        .catch(() => {});
      bridge
        .invoke<AppInfoPayload>('get_app_info')
        .then((info) => {
          if (info?.effectiveVersion) setCurrentVersion(info.effectiveVersion);
        })
        .catch(() => {});
      bridge
        .invoke<SystemStatus>('get_system_status')
        .then((s) => setSystemStatus(s))
        .catch(() => {});
      bridge
        .invoke<AddinItem[]>('list_addins', { app: 'word' })
        .then((list) => setAddins(list))
        .catch(() => {});
    }

    const unsubDrag = bridge.on('drag_state', (data) => {
      const payload = data as { over?: boolean } | null;
      setIsDragging(Boolean(payload?.over));
    });
    const unsubDrop = bridge.on('files_dropped', (data) => {
      const dropped = (data as FileQueueItem[]) || [];
      if (dropped.length > 0) {
        mergeFilesToQueue(dropped);
        appendLog(`Đã thêm ${dropped.length} tệp từ thao tác kéo-thả.`);
      }
    });
    const unsubProgress = bridge.on('progress', (data) => {
      setProgress(data as ProgressPayload);
    });
    const unsubOta = bridge.on('update_progress', (data) => {
      setOtaProgress(data as UpdateProgressPayload);
    });

    const runAutoOtaCheck = async () => {
      try {
        const res = await checkSoftwareUpdate();
        setUpdateInfo(res);
        if (res.currentVersion) setCurrentVersion(res.currentVersion);
        if (!bridge.isNativeHost() && res.hasUpdate && filesCountRef.current === 0 && !busyRef.current) {
          let already = '';
          try {
            already = sessionStorage.getItem(WEB_AUTO_UPDATED_SESSION_KEY) || '';
          } catch {
            // ignore
          }
          if (already !== res.latestVersion) {
            await applyWebUpdate(res.latestVersion);
          }
        }
      } catch {
        // silent background check
      }
    };

    const bootTimer = setTimeout(() => {
      void runAutoOtaCheck();
    }, 1800);
    const interval = setInterval(() => {
      void runAutoOtaCheck();
    }, 20 * 60 * 1000);

    return () => {
      unsubDrag();
      unsubDrop();
      unsubProgress();
      unsubOta();
      clearTimeout(bootTimer);
      clearInterval(interval);
    };
  }, [appendLog, mergeFilesToQueue]);

  const applyPreset = (presetId: PresetId) => {
    const found = presets.find((p) => p.id === presetId);
    if (found) {
      setConfig((prev) => ({
        ...found.config,
        outputMode: prev.outputMode,
        alsoExportPdf: prev.alsoExportPdf,
      }));
      setToast({ type: 'ok', text: `Đã áp dụng bộ chuẩn: ${found.name}` });
      appendLog(`Chuyển bộ quy chuẩn sang "${found.name}".`);
    }
  };

  const updateFormat = <K extends keyof ProcessConfig['format']>(
    key: K,
    value: ProcessConfig['format'][K],
  ) => {
    setConfig((prev) => ({
      ...prev,
      preset: 'custom',
      format: { ...prev.format, [key]: value },
    }));
  };

  const updateCleaner = <K extends keyof ProcessConfig['cleaner']>(
    key: K,
    value: ProcessConfig['cleaner'][K],
  ) => {
    setConfig((prev) => ({
      ...prev,
      preset: 'custom',
      cleaner: { ...prev.cleaner, [key]: value },
    }));
  };

  const updatePageToc = <K extends keyof ProcessConfig['pageToc']>(
    key: K,
    value: ProcessConfig['pageToc'][K],
  ) => {
    setConfig((prev) => ({
      ...prev,
      preset: 'custom',
      pageToc: { ...prev.pageToc, [key]: value },
    }));
  };

  const updateSecurity = <K extends keyof ProcessConfig['security']>(
    key: K,
    value: ProcessConfig['security'][K],
  ) => {
    setConfig((prev) => ({
      ...prev,
      security: { ...prev.security, [key]: value },
    }));
  };

  const handlePickFiles = async () => {
    if (!isNativeWin) {
      webFileInputRef.current?.click();
      return;
    }
    try {
      const res = await bridge.invoke<{ items: FileQueueItem[] }>('pick_files');
      if (res?.items?.length) {
        mergeFilesToQueue(res.items);
        appendLog(`Đã chọn thêm ${res.items.length} tệp vào hàng đợi.`);
      }
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    }
  };

  const handlePickFolder = async () => {
    if (!isNativeWin) {
      webFileInputRef.current?.click();
      return;
    }
    try {
      const res = await bridge.invoke<{ items: FileQueueItem[] }>('pick_folder');
      if (res?.items?.length) {
        mergeFilesToQueue(res.items);
        appendLog(`Quét thư mục tìm thấy ${res.items.length} tệp Office hợp lệ.`);
      } else {
        setToast({ type: 'err', text: 'Không tìm thấy file Word/Excel hợp lệ trong thư mục đã chọn.' });
      }
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    }
  };

  const handleAuditFiles = async () => {
    if (files.length === 0) {
      setToast({ type: 'err', text: 'Vui lòng chọn hoặc kéo thả ít nhất 1 file để kiểm tra.' });
      return;
    }
    setBusy(true);
    setProgress(null);
    try {
      let reports: AuditReport[] = [];
      if (isNativeWin && files.every((f) => !f.webFile)) {
        reports = await bridge.invoke<AuditReport[]>('audit_files', {
          paths: files.map((f) => f.path),
        });
      } else {
        for (let i = 0; i < files.length; i++) {
          setProgress({ phase: 'audit', current: i + 1, total: files.length, file: files[i].name });
          try {
            reports.push(await auditWebDocxFile(files[i]));
          } catch (fileErr) {
            if (files.length <= 1) throw fileErr;
            reports.push({
              filePath: files[i].path,
              fileName: files[i].name,
              fileType: files[i].ext.toUpperCase(),
              score: 0,
              sectionCount: 0,
              tableCount: 0,
              paragraphCount: 0,
              hasMacros: false,
              isProtected: false,
              hasLegacyEncoding: false,
              issues: [
                {
                  category: 'Không thể đọc tệp',
                  severity: 'error',
                  message: (fileErr as Error).message || 'Lỗi đọc tệp trên trình duyệt.',
                  count: 1,
                },
              ],
            });
          }
        }
      }

      const next: Record<string, AuditReport> = {};
      for (const r of reports) next[r.filePath] = r;
      setAudits(next);
      const checkedAt = new Date().toISOString();
      setRecentDocs((prev) =>
        upsertRecentDocs(
          prev,
          reports.map((r) => ({
            path: r.filePath,
            name: r.fileName,
            ext: r.fileType.toLowerCase(),
            updatedAt: checkedAt,
            status: 'checked',
            operation: 'audit',
            resultStatus: r.issues.length === 0 ? 'passed' : 'needsFix',
            summary:
              r.issues.length === 0
                ? 'Kết quả kiểm tra: Đạt'
                : `Cần chỉnh sửa (${r.issues.length} mục)`,
          })),
        ),
      );
      appendLog(`Kiểm tra lỗi hoàn tất ${reports.length} tệp.`);
      if (reports.length === 1) {
        setSelectedAudit(reports[0]);
      } else {
        setToast({ type: 'ok', text: `Đã kiểm tra ${reports.length} tệp.` });
      }
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const handleProcessFiles = async (
    targetPath?: string,
    previewOverrides?: PreviewOverrideConfig,
  ) => {
    const targetFiles = targetPath ? files.filter((f) => f.path === targetPath) : files;
    if (targetFiles.length === 0) {
      setToast({ type: 'err', text: 'Hàng đợi đang trống. Hãy chọn file trước.' });
      return;
    }
    const effectiveConfig: ProcessConfig =
      previewOverrides && previewOverrides.enabled
        ? {
            ...config,
            format: {
              ...config.format,
              ...(previewOverrides.fontName ? { fontName: previewOverrides.fontName } : {}),
              ...(previewOverrides.fontSizePt != null
                ? { fontSizePt: previewOverrides.fontSizePt }
                : {}),
              ...(previewOverrides.tableFontSizePt != null
                ? { tableFontSizePt: previewOverrides.tableFontSizePt }
                : {}),
              ...(previewOverrides.lineSpacingMult != null
                ? { lineSpacing: previewOverrides.lineSpacingMult }
                : {}),
              ...(previewOverrides.spaceBeforePt != null
                ? { spaceBeforePt: previewOverrides.spaceBeforePt }
                : {}),
              ...(previewOverrides.spaceAfterPt != null
                ? { spaceAfterPt: previewOverrides.spaceAfterPt }
                : {}),
              ...(previewOverrides.firstLineIndentCm != null
                ? { firstLineIndentCm: previewOverrides.firstLineIndentCm }
                : {}),
              ...(previewOverrides.paperSize ? { paperSize: previewOverrides.paperSize } : {}),
              ...(previewOverrides.marginTopCm != null
                ? { marginTopCm: previewOverrides.marginTopCm }
                : {}),
              ...(previewOverrides.marginBottomCm != null
                ? { marginBottomCm: previewOverrides.marginBottomCm }
                : {}),
              ...(previewOverrides.marginLeftCm != null
                ? { marginLeftCm: previewOverrides.marginLeftCm }
                : {}),
              ...(previewOverrides.marginRightCm != null
                ? { marginRightCm: previewOverrides.marginRightCm }
                : {}),
            },
            pageToc: {
              ...config.pageToc,
              ...(previewOverrides.orientationOverride
                ? {
                    orientationMode:
                      previewOverrides.orientationOverride === 'keepSections'
                        ? 'keep'
                        : previewOverrides.orientationOverride,
                  }
                : {}),
              autoSwapBindingMargin: previewOverrides.autoSwapBindingMargin,
            },
          }
        : config;
    setBusy(true);
    setProgress(null);
    try {
      let out: FileProcessResult[] = [];
      if (isNativeWin && targetFiles.every((f) => !f.webFile)) {
        out = await bridge.invoke<FileProcessResult[]>('process_files', {
          paths: targetFiles.map((f) => f.path),
          config: effectiveConfig,
        });
      } else {
        for (let i = 0; i < targetFiles.length; i++) {
          setProgress({
            phase: 'process',
            current: i + 1,
            total: targetFiles.length,
            file: targetFiles[i].name,
          });
          out.push(await processWebDocxFile(targetFiles[i], effectiveConfig));
        }
      }

      const nextRes: Record<string, FileProcessResult> = { ...results };
      let okCount = 0;
      let errCount = 0;
      for (const r of out) {
        nextRes[r.inputPath] = r;
        if (r.success) {
          okCount += 1;
          appendLog(`✓ ${r.outputPath} (${r.statsSummary})`);
        } else {
          errCount += 1;
          appendLog(`✕ Lỗi ${r.inputPath}: ${r.error}`);
        }
        for (const l of r.logs) appendLog(`   · ${l}`);
      }
      setResults(nextRes);
      const doneAt = new Date().toISOString();
      const succeeded = out.filter((r) => r.success);
      if (succeeded.length > 0) {
        setRecentDocs((prev) =>
          upsertRecentDocs(
            prev,
            succeeded.map((r) => {
              const matched = files.find((f) => f.path === r.inputPath);
              const name = matched?.name || r.inputPath.split(/[/\\]/).pop() || r.inputPath;
              const ext = matched?.ext || name.split('.').pop()?.toLowerCase() || 'docx';
              return {
                path: r.inputPath,
                name,
                ext,
                updatedAt: doneAt,
                status: 'standardized',
                operation: 'standardize',
                resultStatus: 'completed',
                outputPath: r.outputPath,
                backupPath: r.backupPath,
                summary: r.statsSummary,
              };
            }),
          ),
        );
      }
      setToast({
        type: errCount === 0 ? 'ok' : 'err',
        text: `Hoàn tất: ${okCount} thành công${errCount > 0 ? `, ${errCount} lỗi` : ''}.`,
      });
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const handleDownloadWebOutput = (blobUrl: string, fileName: string) => {
    triggerBrowserDownload(blobUrl, fileName);
  };

  const handleShareWebOutput = async (res: FileProcessResult) => {
    if (!res.outputBlob) return;
    const mode = await shareOutputBlob(res.outputBlob, res.outputName || res.outputPath, res.blobUrl);
    if (mode === 'downloaded') {
      setToast({ type: 'ok', text: `Đã tải xuống ${res.outputName || res.outputPath}` });
    }
  };

  const handleRestoreBackup = async (originalPath: string, backupPath: string) => {
    try {
      const res = await bridge.invoke<{ message: string }>('restore_backup', {
        originalPath,
        backupPath,
      });
      setToast({ type: 'ok', text: res.message });
      appendLog(res.message);
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    }
  };

  const handleOpenOutput = async (path: string, reveal = false): Promise<boolean> => {
    try {
      await bridge.invoke('open_path', { path, reveal });
      return true;
    } catch (e) {
      setToast({
        type: 'err',
        text: `Không thể mở đường dẫn "${path}": ${(e as Error).message}`,
      });
      return false;
    }
  };

  const refreshSystemAndAddins = async (targetApp = addinApp) => {
    if (!isNativeWin) return;
    setLoadingSystem(true);
    try {
      const [st, list] = await Promise.all([
        bridge.invoke<SystemStatus>('get_system_status'),
        bridge.invoke<AddinItem[]>('list_addins', { app: targetApp }),
      ]);
      setSystemStatus(st);
      setAddins(list);
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    } finally {
      setLoadingSystem(false);
    }
  };

  const applySystemTweak = async (req: SystemTweaksRequest) => {
    setLoadingSystem(true);
    try {
      const res = await bridge.invoke<{ logs: string[]; status: SystemStatus }>('apply_system_tweaks', req);
      setSystemStatus(res.status);
      for (const l of res.logs) appendLog(l);
      setToast({ type: 'ok', text: res.logs[0] || 'Đã cập nhật cấu hình hệ thống Office.' });
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    } finally {
      setLoadingSystem(false);
    }
  };

  const handleAddAddin = async () => {
    try {
      const picked = await bridge.invoke<{ paths: string[] }>('pick_addin_file', { app: addinApp });
      if (!picked?.paths?.length) return;
      const res = await bridge.invoke<{ message: string; items: AddinItem[] }>('manage_addin', {
        action: 'add',
        app: addinApp,
        path: picked.paths[0],
      });
      setAddins(res.items);
      setToast({ type: 'ok', text: res.message });
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    }
  };

  const handleManageAddin = async (action: 'toggle' | 'delete', path: string) => {
    try {
      const res = await bridge.invoke<{ message: string; items: AddinItem[] }>('manage_addin', {
        action,
        app: addinApp,
        path,
      });
      setAddins(res.items);
      setToast({ type: 'ok', text: res.message });
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    }
  };

  const handleInstallFonts = async () => {
    try {
      const picked = await bridge.invoke<{ paths: string[] }>('pick_font_files');
      if (!picked?.paths?.length) return;
      const res = await bridge.invoke<{ installed: string[] }>('install_fonts', {
        paths: picked.paths,
      });
      setToast({
        type: 'ok',
        text: `Đã cài đặt ${res.installed.length} font chữ vào hệ thống người dùng.`,
      });
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    }
  };

  const handleOpenDesktopDownload = () => {
    const url = updateInfo?.downloadUrl || GITHUB_RELEASES_URL;
    if (isNativeWin) {
      void bridge.invoke('open_external_url', { url });
    } else {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  };

  const handleHeaderToast = useCallback(
    (kind: 'success' | 'error' | 'info', msg: string) => {
      setToast({ type: kind === 'error' ? 'err' : 'ok', text: msg });
      appendLog(msg);
    },
    [appendLog],
  );

  const handleReplaceWebFile = useCallback((path: string, newFile: File) => {
    setFiles((prev) =>
      prev.map((f) =>
        f.path === path ? { ...f, webFile: newFile, sizeBytes: newFile.size } : f,
      ),
    );
  }, []);

  const handleInspectInPreview = useCallback((filePath: string, elementPath: string) => {
    setPreviewFocusTarget({ filePath, elementPath });
    setActiveTab('livePreview');
  }, []);

  const handleRecordSingleStandardization = useCallback(
    (file: FileQueueItem, res: StdExecutionResult) => {
      if (!res.success) return;
      const summary =
        res.verification?.summaryMessage ||
        `Đã chuẩn hóa (${res.actionsApplied.length} thay đổi)`;
      setResults((prev) => ({
        ...prev,
        [file.path]: {
          inputPath: file.path,
          outputPath: res.outputPath,
          backupPath: res.backupPath,
          success: true,
          statsSummary: summary,
          logs: res.logs,
          error: null,
          auditBefore: null,
          auditAfter: null,
          blobUrl: res.blobUrl,
          outputName: res.outputName,
          outputBlob: res.outputBlob,
        },
      }));
      setRecentDocs((prev) =>
        upsertRecentDocs(prev, [
          {
            path: file.path,
            name: file.name,
            ext: file.ext,
            updatedAt: new Date().toISOString(),
            status: 'standardized',
            operation: 'standardize',
            resultStatus: 'completed',
            outputPath: res.outputPath,
            backupPath: res.backupPath,
            summary,
          },
        ]),
      );
    },
    [],
  );

  const handleRecordAuditCompletion = useCallback(
    (file: FileQueueItem, rep: ComplianceAuditReport) => {
      const fixableIssues =
        rep.traceableFindings.filter((f) => f.safetyTier !== 'groupCManualOnly').length +
        rep.advancedIssues.length;
      const summary =
        fixableIssues === 0
          ? 'Kết quả kiểm tra: Đạt'
          : `Cần chỉnh sửa (${fixableIssues} mục)`;
      setRecentDocs((prev) =>
        upsertRecentDocs(prev, [
          {
            path: file.path,
            name: file.name,
            ext: file.ext,
            updatedAt: new Date().toISOString(),
            status: 'checked',
            operation: 'audit',
            resultStatus: fixableIssues === 0 ? 'passed' : 'needsFix',
            summary,
          },
        ]),
      );
    },
    [],
  );

  const handleRecordTemplateCreation = useCallback(
    (res: TemplateApplyResult, tpl: DocumentTemplate) => {
      const outName =
        res.outputName ||
        res.outputPath?.split(/[/\\]/).pop() ||
        `${tpl.docType}_MonWord.docx`;
      const recordPath =
        isNativeWin && res.outputPath
          ? res.outputPath
          : `web://tpl_${tpl.templateId}_${Date.now()}`;
      setRecentDocs((prev) =>
        upsertRecentDocs(prev, [
          {
            path: recordPath,
            name: outName,
            ext: 'docx',
            updatedAt: new Date().toISOString(),
            status: 'standardized',
            operation: 'template',
            resultStatus: res.success ? 'completed' : 'failed',
            outputPath: isNativeWin ? res.outputPath : outName,
            backupPath: isNativeWin ? (res.backupPath ?? null) : null,
            summary: res.success ? `Tạo từ mẫu: ${tpl.name}` : res.error || 'Tạo mẫu thất bại',
          },
        ]),
      );
    },
    [isNativeWin],
  );

  const refreshOperationLogs = useCallback(async () => {
    if (!isNativeWin) return;
    try {
      const list = await bridge.invoke<OperationLogEntry[]>('get_operation_logs', { limit: 120 });
      if (Array.isArray(list)) setOperationLogs(list);
    } catch {
      // ignore
    }
  }, [isNativeWin]);

  const handleClearOperationLogs = useCallback(async () => {
    if (!isNativeWin) return;
    try {
      await bridge.invoke('clear_operation_logs');
      setOperationLogs([]);
      setToast({ type: 'ok', text: 'Đã xóa nhật ký hệ thống.' });
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    }
  }, [isNativeWin]);

  const handleCancelActiveTask = useCallback(async () => {
    if (isNativeWin) {
      try {
        await bridge.invoke('cancel_active_task');
      } catch {
        // ignore
      }
    }
    setToast({ type: 'ok', text: 'Đã gửi yêu cầu dừng tác vụ an toàn.' });
    appendLog('Đã yêu cầu dừng tác vụ đang chạy.');
  }, [appendLog, isNativeWin]);

  const navigateTo = (tab: NavTab) => {
    setActiveTab(tab);
    if ((tab === 'system' || tab === 'advancedSettings') && isNativeWin) void refreshSystemAndAddins();
    if (tab === 'security' && isNativeWin) void refreshOperationLogs();
  };

  const activePresetLabel = useMemo(() => {
    if (config.preset === 'custom') return 'Tùy chỉnh';
    return presets.find((p) => p.id === config.preset)?.name || 'Nghị định 30/2020/NĐ-CP';
  }, [config.preset, presets]);

  const configPanelsProps: ConfigPanelsProps = {
    activeTab,
    presets,
    config,
    applyPreset,
    updateFormat,
    updateCleaner,
    updatePageToc,
    updateSecurity,
    activityLogs,
    onClearLogs: () => setActivityLogs([]),
    operationLogs,
    onRefreshOpLogs: () => void refreshOperationLogs(),
    onClearOpLogs: () => void handleClearOperationLogs(),
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
    onOpenDesktopDownload: handleOpenDesktopDownload,
    files,
    audits,
    results,
    recentDocs,
    onNavigate: navigateTo,
    onPickFiles: () => void handlePickFiles(),
    onOpenOutput: (path: string, reveal?: boolean) => handleOpenOutput(path, reveal),
    onDownloadOutput: handleDownloadWebOutput,
    onRestoreBackup: (originalPath: string, backupPath: string) =>
      void handleRestoreBackup(originalPath, backupPath),
    onClearRecent: () => setRecentDocs([]),
    currentVersion,
    onOpenUpdate: () => setUpdateDialogOpen(true),
    renderHeaderBuilder: () => (
      <HeaderBuilderWorkspace
        files={files}
        onPickFiles={handlePickFiles}
        onReplaceWebFile={handleReplaceWebFile}
        onStatusToast={handleHeaderToast}
      />
    ),
    renderStandardizer: () => (
      <StandardizationWorkspace
        files={files}
        onPickFiles={handlePickFiles}
        onDropWebFiles={addWebFiles}
        onReplaceWebFile={handleReplaceWebFile}
        onInspectInPreview={handleInspectInPreview}
        renderEmbeddedPreview={(filePath) => {
          const target = files.find((f) => f.path === filePath);
          return (
            <LivePreviewWorkspace
              files={target ? [target] : files}
              results={results}
              config={config}
              onPickFiles={handlePickFiles}
              onProcessCurrentFile={(fp, ov) => void handleProcessFiles(fp, ov)}
              onStatusToast={handleHeaderToast}
            />
          );
        }}
        onNavigateHome={() => navigateTo('home')}
        onStatusToast={handleHeaderToast}
        onPickFolder={isNativeWin ? () => void handlePickFolder() : undefined}
        onRemoveFile={(path) => setFiles((prev) => prev.filter((f) => f.path !== path))}
        onClearFiles={() => {
          setFiles([]);
          setAudits({});
          setResults({});
        }}
        onBatchProcess={() => void handleProcessFiles()}
        batchResults={results}
        batchBusy={busy}
        batchProgress={progress}
        presets={presets}
        activePreset={config.preset}
        onSelectPreset={applyPreset}
        outputModeConfig={config.outputMode}
        onChangeOutputModeConfig={(mode) => setConfig((p) => ({ ...p, outputMode: mode }))}
        onOpenOutput={(path, reveal) => void handleOpenOutput(path, reveal)}
        onDownloadOutput={handleDownloadWebOutput}
        onRestoreBackup={(orig, bak) => void handleRestoreBackup(orig, bak)}
        onSingleExecComplete={handleRecordSingleStandardization}
        preferredFilePath={preferredStdTarget?.filePath ?? null}
        preferredScope={preferredStdTarget?.scope ?? null}
        onClearPreferredTarget={() => setPreferredStdTarget(null)}
      />
    ),
    renderTemplateEngine: () => (
      <TemplateWorkspace
        files={files}
        onPickFiles={handlePickFiles}
        onReplaceWebFile={handleReplaceWebFile}
        onTemplateCreated={handleRecordTemplateCreation}
        onStatusToast={handleHeaderToast}
      />
    ),
    renderLayoutIntelligence: () => (
      <LayoutIntelligenceWorkspace
        files={files}
        onPickFiles={handlePickFiles}
        onReplaceWebFile={handleReplaceWebFile}
        onStatusToast={handleHeaderToast}
      />
    ),
    renderComplianceAudit: () => (
      <ComplianceAuditWorkspace
        files={files}
        onPickFiles={handlePickFiles}
        onReplaceWebFile={handleReplaceWebFile}
        onInspectInPreview={handleInspectInPreview}
        onStartStandardize={(filePath, scope) => {
          setPreferredStdTarget({ filePath, scope });
          navigateTo('standardizer');
        }}
        onAuditCompleted={handleRecordAuditCompletion}
        onStatusToast={handleHeaderToast}
      />
    ),
    renderLivePreview: () => (
      <LivePreviewWorkspace
        files={files}
        results={results}
        config={config}
        focusTarget={previewFocusTarget}
        onClearFocusTarget={() => setPreviewFocusTarget(null)}
        onPickFiles={handlePickFiles}
        onProcessCurrentFile={(fp, ov) => void handleProcessFiles(fp, ov)}
        onStatusToast={handleHeaderToast}
      />
    ),
  };

  const handleWebDragOver = (e: React.DragEvent) => {
    if (e.dataTransfer?.types?.includes('Files')) {
      e.preventDefault();
      setIsDragging(true);
    }
  };

  const handleWebDragLeave = () => {
    setIsDragging(false);
  };

  const handleWebDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer?.files?.length) {
      addWebFiles(e.dataTransfer.files);
    }
  };

  return (
    <>
      <input
        ref={webFileInputRef}
        type="file"
        accept=".docx,.docm"
        multiple
        className="hidden"
        onChange={(e) => {
          if (e.target.files?.length) {
            addWebFiles(e.target.files);
            e.target.value = '';
          }
        }}
      />

      {edition === 'mobile' && (
        <MobileWorkspace
          edition={edition}
          override={override}
          onChangeOverride={setOverride}
          currentVersion={currentVersion}
          updateInfo={updateInfo}
          onOpenUpdate={() => setUpdateDialogOpen(true)}
          onOpenAbout={() => setAboutDialogOpen(true)}
          presets={presets}
          config={config}
          applyPreset={applyPreset}
          configPanelsProps={configPanelsProps}
          activeConfigTab={activeTab}
          setActiveConfigTab={navigateTo}
          files={files}
          audits={audits}
          results={results}
          onPickFiles={handlePickFiles}
          onRemoveFile={(path) => setFiles((prev) => prev.filter((f) => f.path !== path))}
          onOpenReport={(report) => setSelectedAudit(report)}
          onDownloadOutput={handleDownloadWebOutput}
          onShareOutput={(res) => void handleShareWebOutput(res)}
          onAuditFiles={() => void handleAuditFiles()}
          onProcessFiles={() => void handleProcessFiles()}
          busy={busy}
          progress={progress}
          toast={toast}
        />
      )}

      {edition === 'web' && (
        <WebWorkspace
          darkMode={darkMode}
          onToggleDark={() => setDarkMode((d) => !d)}
          edition={edition}
          override={override}
          onChangeOverride={setOverride}
          currentVersion={currentVersion}
          updateInfo={updateInfo}
          onOpenUpdate={() => setUpdateDialogOpen(true)}
          onOpenAbout={() => setAboutDialogOpen(true)}
          activeTab={activeTab}
          setActiveTab={navigateTo}
          configPanelsProps={configPanelsProps}
          config={config}
          files={files}
          audits={audits}
          results={results}
          isDragging={isDragging}
          onWebDragOver={handleWebDragOver}
          onWebDragLeave={handleWebDragLeave}
          onWebDrop={handleWebDrop}
          onPickFiles={handlePickFiles}
          onRemoveFile={(path) => setFiles((prev) => prev.filter((f) => f.path !== path))}
          onClearAll={() => {
            setFiles([]);
            setAudits({});
            setResults({});
          }}
          onOpenReport={(report) => setSelectedAudit(report)}
          onDownloadOutput={handleDownloadWebOutput}
          onShareOutput={(res) => void handleShareWebOutput(res)}
          onAuditFiles={() => void handleAuditFiles()}
          onProcessFiles={() => void handleProcessFiles()}
          busy={busy}
          progress={progress}
          toast={toast}
          onOpenDesktopDownload={handleOpenDesktopDownload}
        />
      )}

      {edition === 'win' && (() => {
        const showBatchPanels =
          isAdminToolsEnabled() &&
          ['oneClick', 'format', 'pageToc', 'cleaner', 'security'].includes(activeTab);
        return (
        <div
          onDragOver={!isNativeWin ? handleWebDragOver : undefined}
          onDragLeave={!isNativeWin ? handleWebDragLeave : undefined}
          onDrop={!isNativeWin ? handleWebDrop : undefined}
          className="h-screen flex flex-col mw-canvas"
        >
          {/* Windows Topbar */}
          <header className="h-14 shrink-0 px-4 border-b border-slate-200 dark:border-slate-800 bg-white/95 dark:bg-slate-900/95 backdrop-blur flex items-center justify-between gap-3">
            <div className="flex items-center gap-2.5 min-w-0">
              <img
                src="/monword-icon.png"
                alt="MonWord"
                className="w-8 h-8 object-contain shrink-0"
              />
              <div className="flex items-center gap-2 flex-wrap min-w-0">
                <h1 className="text-base font-bold tracking-tight">
                  <span className="text-[var(--mw-ink)]">MonWord</span>
                </h1>
                <VersionOtaPill
                  currentVersion={currentVersion}
                  updateInfo={updateInfo}
                  onOpenUpdate={() => setUpdateDialogOpen(true)}
                  editionLabel={editionLabel}
                />
              </div>
            </div>

            <div className="flex items-center gap-1.5 shrink-0">
              <button
                type="button"
                onClick={() => setAboutDialogOpen(true)}
                className="mw-button mw-button-secondary mw-icon-button"
                title="Giới thiệu & Phiên bản"
              >
                <Info className="w-4 h-4" />
              </button>

              <button
                type="button"
                onClick={() => setDarkMode((d) => !d)}
                className="mw-button mw-button-secondary mw-icon-button"
                title={darkMode ? 'Chuyển sang Giao diện Sáng' : 'Chuyển sang Giao diện Tối'}
              >
                {darkMode ? <Sun className="w-4 h-4 text-amber-400" /> : <Moon className="w-4 h-4" />}
              </button>
            </div>
          </header>

          {/* Main Workspace */}
          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto lg:flex-row lg:overflow-hidden">
            {/* Pane 1: Sidebar */}
            <aside className="shrink-0 border-b border-slate-200 bg-white p-3 dark:border-slate-800 dark:bg-slate-900 lg:flex lg:w-56 lg:flex-col lg:overflow-y-auto lg:border-b-0 lg:border-r">
              <WorkspaceNavigation activeTab={activeTab} onSelect={navigateTo} />
            </aside>

            {/* Pane 2: Center Workspace */}
            <main className="flex-1 min-w-0 p-4 space-y-4 lg:overflow-y-auto lg:p-6">
              {toast && <Toast {...toast} />}

              {activeTab === 'home' ? (
                <WorkspaceHome
                  files={files}
                  recentDocs={recentDocs}
                  onPickFiles={handlePickFiles}
                  onPickFolder={isNativeWin ? handlePickFolder : undefined}
                  onOpenOutput={(path, reveal) => void handleOpenOutput(path, reveal)}
                  onTask={navigateTo}
                  onShowFiles={() => navigateTo('history')}
                  progress={busy ? progress : null}
                  isDragging={isDragging}
                />
              ) : <ConfigPanels {...configPanelsProps} />}
            </main>

            {/* Pane 3: File Queue (only on batch config tabs) */}
            {showBatchPanels && (
              <aside className="shrink-0 border-t border-slate-200 bg-slate-50/70 p-3 flex flex-col gap-3 dark:border-slate-800 dark:bg-slate-900/50 lg:w-[290px] lg:overflow-y-auto lg:border-l lg:border-t-0 xl:w-[330px]">
                <FileQueuePanel
                  files={files}
                  audits={audits}
                  results={results}
                  isDragging={isDragging}
                  isBrowserMode={!isNativeWin}
                  onPickFiles={handlePickFiles}
                  onPickFolder={handlePickFolder}
                  onRemoveFile={(path) => setFiles((prev) => prev.filter((f) => f.path !== path))}
                  onClearAll={() => {
                    setFiles([]);
                    setAudits({});
                    setResults({});
                  }}
                  onOpenReport={(report) => setSelectedAudit(report)}
                  onOpenOutput={handleOpenOutput}
                  onDownloadOutput={handleDownloadWebOutput}
                  onShareOutput={(res) => void handleShareWebOutput(res)}
                  onRestoreBackup={handleRestoreBackup}
                />
              </aside>
            )}
          </div>

          {/* Bottom Batch Action Bar (only on batch config tabs) */}
          {showBatchPanels && (
            <footer className="min-h-14 shrink-0 flex-wrap px-4 py-2 border-t border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 flex items-center justify-between gap-3">
              <div className="flex items-center gap-4 text-xs">
                <div className="inline-flex items-center rounded-xl border border-slate-200 dark:border-slate-700 p-0.5 bg-slate-100 dark:bg-slate-800">
                  <button
                    type="button"
                    onClick={() => setConfig((p) => ({ ...p, outputMode: 'newCopy' }))}
                    className={`inline-flex items-center gap-1 px-2.5 py-1 rounded-lg font-semibold ${
                      config.outputMode === 'newCopy'
                        ? 'bg-white dark:bg-slate-900 text-blue-600 shadow-sm'
                        : 'text-slate-600 dark:text-slate-300'
                    }`}
                  >
                    <FileOutput className="w-3.5 h-3.5" /> Lưu thành tệp mới
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfig((p) => ({ ...p, outputMode: 'overwriteWithBackup' }))}
                    className={`inline-flex items-center gap-1 px-2.5 py-1 rounded-lg font-semibold ${
                      config.outputMode === 'overwriteWithBackup'
                        ? 'bg-white dark:bg-slate-900 text-amber-600 shadow-sm'
                        : 'text-slate-600 dark:text-slate-300'
                    }`}
                  >
                    <Lock className="w-3.5 h-3.5" /> Ghi đè tệp gốc (có sao lưu)
                  </button>
                </div>

                {isNativeWin && (
                  <label className="hidden lg:inline-flex items-center gap-1.5 font-medium cursor-pointer text-slate-600 dark:text-slate-300">
                    <input
                      type="checkbox"
                      checked={config.alsoExportPdf}
                      onChange={(e) => setConfig((p) => ({ ...p, alsoExportPdf: e.target.checked }))}
                      className="rounded border-slate-300 text-blue-600"
                    />
                    <FileText className="w-3.5 h-3.5 text-rose-500" />
                    Lưu thêm bản PDF
                  </label>
                )}
              </div>

              <div className="flex items-center gap-2.5">
                {busy && progress && (
                  <div className="flex items-center gap-2 text-xs text-blue-600 dark:text-blue-400 font-semibold">
                    <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                    <span>
                      {progress.phase === 'audit' ? 'Đang kiểm tra' : 'Đang chuẩn hóa'} {progress.current}/{progress.total}
                    </span>
                  </div>
                )}

                <button
                  type="button"
                  disabled={busy}
                  onClick={handleAuditFiles}
                  className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700 text-xs font-bold disabled:opacity-50"
                >
                  <SearchCheck className="w-4 h-4 text-indigo-500" />
                  Kiểm tra lỗi
                </button>

                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handleProcessFiles()}
                  className="mw-button mw-button-primary"
                >
                  <Play className="w-3.5 h-3.5 fill-current" />
                  Bắt đầu chuẩn hóa
                </button>
              </div>
            </footer>
          )}

          <StatusBar
            fileCount={files.length}
            config={config}
            activePresetLabel={activePresetLabel}
            busy={busy}
            progress={progress}
            currentVersion={currentVersion}
            updateInfo={updateInfo}
            onCancelTask={() => void handleCancelActiveTask()}
            onOpenUpdate={() => setUpdateDialogOpen(true)}
            onOpenAbout={() => setAboutDialogOpen(true)}
          />
        </div>
        );
      })()}

      <AuditModal report={selectedAudit} onClose={() => setSelectedAudit(null)} />

      <UpdateDialog
        open={updateDialogOpen}
        onClose={() => setUpdateDialogOpen(false)}
        edition={edition}
        isNativeWin={isNativeWin}
        currentVersion={currentVersion}
        initialResult={updateInfo}
        onUpdateChecked={(res) => {
          setUpdateInfo(res);
          if (res.currentVersion) setCurrentVersion(res.currentVersion);
        }}
        onToast={(type, text) => setToast({ type, text })}
        progress={otaProgress}
      />

      <AboutDialog
        open={aboutDialogOpen}
        onClose={() => setAboutDialogOpen(false)}
        onOpenUpdate={() => setUpdateDialogOpen(true)}
        edition={edition}
        isNativeWin={isNativeWin}
        currentVersion={currentVersion}
      />
    </>
  );
}

export default App;
