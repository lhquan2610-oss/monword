import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  BookOpen,
  CheckCircle2,
  FileOutput,
  FileText,
  Info,
  Layout,
  Lock,
  Play,
  RefreshCw,
  SearchCheck,
  Shield,
  Sparkles,
  Sun,
  Moon,
  Type,
  Wrench,
  Zap,
} from 'lucide-react';
import { bridge } from './bridge';
import { A4LivePreview } from './components/A4LivePreview';
import { AboutDialog } from './components/AboutDialog';
import { AuditModal } from './components/AuditModal';
import { ConfigPanels, type ConfigPanelsProps } from './components/ConfigPanels';
import { FileQueuePanel } from './components/FileQueuePanel';
import { MobileWorkspace } from './components/MobileWorkspace';
import { PlatformSwitcher, VersionOtaPill } from './components/PlatformSwitcher';
import { StatusBar } from './components/StatusBar';
import { UpdateDialog } from './components/UpdateDialog';
import { WebWorkspace } from './components/WebWorkspace';
import {
  type AddinItem,
  type AppInfoPayload,
  type AuditReport,
  DEFAULT_ND30_CONFIG,
  DEFAULT_PRESETS_FALLBACK,
  type FileProcessResult,
  type FileQueueItem,
  type NavTab,
  type PresetId,
  type PresetInfo,
  type ProcessConfig,
  type ProgressPayload,
  type SystemStatus,
  type SystemTweaksRequest,
  type UpdateCheckResult,
  type UpdateProgressPayload,
} from './types';
import { usePlatformEdition } from './utils/platform';
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

export function App() {
  const { edition, override, setOverride, isNativeWin, label: editionLabel } = usePlatformEdition();

  const [darkMode, setDarkMode] = useState(false);
  const [activeTab, setActiveTab] = useState<NavTab>('oneClick');
  const [presets, setPresets] = useState<PresetInfo[]>(DEFAULT_PRESETS_FALLBACK);
  const [config, setConfig] = useState<ProcessConfig>(DEFAULT_ND30_CONFIG);

  const [files, setFiles] = useState<FileQueueItem[]>([]);
  const [audits, setAudits] = useState<Record<string, AuditReport>>({});
  const [results, setResults] = useState<Record<string, FileProcessResult>>({});
  const [selectedAudit, setSelectedAudit] = useState<AuditReport | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ProgressPayload | null>(null);
  const [activityLogs, setActivityLogs] = useState<string[]>([
    'Sẵn sàng xử lý văn bản (MonWord v1.0.0 · OpenXML + Live OTA).',
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

  const webFileInputRef = useRef<HTMLInputElement | null>(null);
  const busyRef = useRef(false);
  const filesCountRef = useRef(0);
  busyRef.current = busy;
  filesCountRef.current = files.length;

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode);
  }, [darkMode]);

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
          reports.push(await auditWebDocxFile(files[i]));
        }
      }

      const next: Record<string, AuditReport> = {};
      for (const r of reports) next[r.filePath] = r;
      setAudits(next);
      appendLog(`Kiểm tra lỗi hoàn tất ${reports.length} tệp.`);
      if (reports.length === 1) {
        setSelectedAudit(reports[0]);
      } else {
        setToast({ type: 'ok', text: `Đã chấm điểm ${reports.length} tệp. Bấm vào điểm số để xem chi tiết.` });
      }
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const handleProcessFiles = async () => {
    if (files.length === 0) {
      setToast({ type: 'err', text: 'Hàng đợi đang trống. Hãy chọn file trước.' });
      return;
    }
    setBusy(true);
    setProgress(null);
    try {
      let out: FileProcessResult[] = [];
      if (isNativeWin && files.every((f) => !f.webFile)) {
        out = await bridge.invoke<FileProcessResult[]>('process_files', {
          paths: files.map((f) => f.path),
          config,
        });
      } else {
        for (let i = 0; i < files.length; i++) {
          setProgress({ phase: 'process', current: i + 1, total: files.length, file: files[i].name });
          out.push(await processWebDocxFile(files[i], config));
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

  const handleOpenOutput = async (path: string, reveal = false) => {
    try {
      await bridge.invoke('open_path', { path, reveal });
    } catch (e) {
      setToast({ type: 'err', text: (e as Error).message });
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

  const navItems: { id: NavTab; label: string; group: 'doc' | 'sys'; icon: React.ReactNode }[] = [
    { id: 'oneClick', label: 'Chuẩn hóa 1-Click', group: 'doc', icon: <Zap className="w-4 h-4" /> },
    { id: 'format', label: 'Thể thức & Bảng', group: 'doc', icon: <Type className="w-4 h-4" /> },
    { id: 'pageToc', label: 'Trang & Mục lục', group: 'doc', icon: <BookOpen className="w-4 h-4" /> },
    { id: 'cleaner', label: 'Làm sạch Tiếng Việt', group: 'doc', icon: <Sparkles className="w-4 h-4" /> },
    { id: 'security', label: 'Bảo mật & Macro', group: 'doc', icon: <Shield className="w-4 h-4" /> },
    { id: 'system', label: 'Hệ thống & Add-in', group: 'sys', icon: <Wrench className="w-4 h-4" /> },
  ];

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
          setActiveConfigTab={setActiveTab}
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
          setActiveTab={setActiveTab}
          navItems={navItems}
          configPanelsProps={configPanelsProps}
          presets={presets}
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

      {edition === 'win' && (
        <div
          onDragOver={!isNativeWin ? handleWebDragOver : undefined}
          onDragLeave={!isNativeWin ? handleWebDragLeave : undefined}
          onDrop={!isNativeWin ? handleWebDrop : undefined}
          className="h-screen flex flex-col bg-slate-100 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
        >
          {/* Windows Topbar */}
          <header className="h-14 shrink-0 px-4 border-b border-slate-200 dark:border-slate-800 bg-white/95 dark:bg-slate-900/95 backdrop-blur flex items-center justify-between gap-3">
            <div className="flex items-center gap-3 min-w-0">
              <img
                src="/monword-icon.png"
                alt="Logo MonWord"
                className="w-10 h-10 object-contain drop-shadow-sm shrink-0"
              />
              <div className="min-w-0">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <h1 className="text-base font-extrabold tracking-tight">
                    <span className="text-[#35283C] dark:text-slate-100">Mon</span>
                    <span className="text-[#2563EB] dark:text-blue-400">Word</span>
                  </h1>
                  <VersionOtaPill
                    currentVersion={currentVersion}
                    updateInfo={updateInfo}
                    onOpenUpdate={() => setUpdateDialogOpen(true)}
                    editionLabel={editionLabel}
                  />
                  <span className="hidden sm:inline-block text-[10px] font-bold px-1.5 py-0.5 rounded bg-orange-100 text-orange-700 dark:bg-orange-950 dark:text-orange-300 border border-orange-200 dark:border-orange-800">
                    RUST CORE
                  </span>
                </div>
                <p className="text-[11px] text-slate-500 dark:text-slate-400 truncate">
                  Chuẩn hóa văn bản NĐ 30/2020 &amp; Văn bản Đảng · Hệ sinh thái Mon
                </p>
              </div>
            </div>

            <div className="flex items-center gap-2 shrink-0">
              <PlatformSwitcher
                edition={edition}
                override={override}
                onChangeOverride={setOverride}
                compact
              />

              <div className="hidden xl:flex items-center gap-1 bg-slate-100 dark:bg-slate-800/80 p-1 rounded-xl border border-slate-200 dark:border-slate-700">
                {(
                  [
                    { id: 'nd30', label: 'NĐ 30/2020' },
                    { id: 'vbDang', label: 'VB Đảng' },
                    { id: 'doanhNghiep', label: 'Doanh nghiệp' },
                    { id: 'luanVan', label: 'Luận văn' },
                  ] as { id: PresetId; label: string }[]
                ).map((p) => (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => applyPreset(p.id)}
                    className={`px-2 py-1 rounded-lg text-xs font-semibold transition-all ${
                      config.preset === p.id
                        ? 'bg-white dark:bg-slate-900 text-blue-600 dark:text-blue-400 shadow-sm'
                        : 'text-slate-600 dark:text-slate-300 hover:text-slate-900 dark:hover:text-white'
                    }`}
                  >
                    {p.label}
                  </button>
                ))}
              </div>

              <button
                type="button"
                onClick={() => setAboutDialogOpen(true)}
                className="p-2 rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 hover:bg-slate-100 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-300"
                title="Giới thiệu & Phiên bản"
              >
                <Info className="w-4 h-4" />
              </button>

              <button
                type="button"
                onClick={() => setDarkMode((d) => !d)}
                className="p-2 rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 hover:bg-slate-100 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-300"
                title={darkMode ? 'Chuyển sang Giao diện Sáng' : 'Chuyển sang Giao diện Tối'}
              >
                {darkMode ? <Sun className="w-4 h-4 text-amber-400" /> : <Moon className="w-4 h-4" />}
              </button>
            </div>
          </header>

          {/* Main 3-Pane Workspace */}
          <div className="flex-1 min-h-0 flex overflow-hidden">
            {/* Pane 1: Sidebar */}
            <aside className="w-52 shrink-0 border-r border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-3 flex flex-col justify-between">
              <div className="space-y-4">
                <div>
                  <div className="px-2.5 mb-1.5 text-[10px] font-bold uppercase tracking-wider text-slate-400">
                    Xử lý tài liệu
                  </div>
                  <nav className="space-y-1">
                    {navItems
                      .filter((i) => i.group === 'doc')
                      .map((item) => (
                        <button
                          key={item.id}
                          type="button"
                          onClick={() => setActiveTab(item.id)}
                          className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-xl text-xs font-semibold transition-all ${
                            activeTab === item.id
                              ? 'bg-blue-600 text-white shadow-sm'
                              : 'text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
                          }`}
                        >
                          {item.icon}
                          <span>{item.label}</span>
                        </button>
                      ))}
                  </nav>
                </div>

                <div className="pt-2 border-t border-slate-200 dark:border-slate-800">
                  <div className="px-2.5 mb-1.5 text-[10px] font-bold uppercase tracking-wider text-slate-400">
                    Cấu hình máy tính
                  </div>
                  <nav className="space-y-1">
                    {navItems
                      .filter((i) => i.group === 'sys')
                      .map((item) => (
                        <button
                          key={item.id}
                          type="button"
                          onClick={() => {
                            setActiveTab(item.id);
                            void refreshSystemAndAddins();
                          }}
                          className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-xl text-xs font-semibold transition-all ${
                            activeTab === item.id
                              ? 'bg-indigo-600 text-white shadow-sm'
                              : 'text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800'
                          }`}
                        >
                          {item.icon}
                          <span>{item.label}</span>
                        </button>
                      ))}
                  </nav>
                </div>
              </div>

              <div className="p-2.5 rounded-xl bg-slate-50 dark:bg-slate-950/60 border border-slate-200 dark:border-slate-800 text-[11px] space-y-1">
                <div className="font-semibold text-slate-700 dark:text-slate-200 flex items-center gap-1.5">
                  <Layout className="w-3.5 h-3.5 text-blue-500" />
                  Đang chọn:
                </div>
                <div className="text-slate-500 dark:text-slate-400 truncate">{activePresetLabel}</div>
              </div>
            </aside>

            {/* Pane 2: Center Configuration */}
            <main className="flex-1 min-w-0 overflow-y-auto p-5 space-y-4">
              {toast && (
                <div
                  className={`px-4 py-2.5 rounded-xl border text-xs font-semibold flex items-center gap-2 shadow-sm ${
                    toast.type === 'ok'
                      ? 'bg-emerald-50 border-emerald-200 text-emerald-800 dark:bg-emerald-950/70 dark:border-emerald-800 dark:text-emerald-200'
                      : 'bg-rose-50 border-rose-200 text-rose-800 dark:bg-rose-950/70 dark:border-rose-800 dark:text-rose-200'
                  }`}
                >
                  {toast.type === 'ok' ? (
                    <CheckCircle2 className="w-4 h-4 shrink-0" />
                  ) : (
                    <AlertTriangle className="w-4 h-4 shrink-0" />
                  )}
                  <span>{toast.text}</span>
                </div>
              )}

              <ConfigPanels {...configPanelsProps} />
            </main>

            {/* Pane 3: File Queue & Live A4 Preview */}
            <aside className="w-[330px] shrink-0 border-l border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-900/50 p-3 flex flex-col gap-3 overflow-y-auto">
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
              <A4LivePreview config={config} />
            </aside>
          </div>

          {/* Bottom Action Bar */}
          <footer className="h-14 shrink-0 px-4 border-t border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 flex items-center justify-between gap-3">
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
                  <FileOutput className="w-3.5 h-3.5" /> Tạo bản sao (_chuanhoa)
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
                  <Lock className="w-3.5 h-3.5" /> Ghi đè + Tự Backup (.bak)
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
                  Xuất kèm PDF (Word COM)
                </label>
              )}
            </div>

            <div className="flex items-center gap-2.5">
              {busy && progress && (
                <div className="flex items-center gap-2 text-xs text-blue-600 dark:text-blue-400 font-semibold">
                  <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                  <span>
                    {progress.phase === 'audit' ? 'Đang quét' : 'Đang xử lý'} {progress.current}/{progress.total}
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
                onClick={handleProcessFiles}
                className="inline-flex items-center gap-1.5 px-5 py-2 rounded-xl bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-700 hover:to-indigo-700 text-white text-xs font-bold shadow-sm disabled:opacity-50"
              >
                <Play className="w-3.5 h-3.5 fill-current" />
                ÁP DỤNG CHUẨN HÓA
              </button>
            </div>
          </footer>

          <StatusBar
            fileCount={files.length}
            config={config}
            activePresetLabel={activePresetLabel}
            busy={busy}
            progress={progress}
            currentVersion={currentVersion}
            updateInfo={updateInfo}
            onOpenUpdate={() => setUpdateDialogOpen(true)}
            onOpenAbout={() => setAboutDialogOpen(true)}
          />
        </div>
      )}

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
