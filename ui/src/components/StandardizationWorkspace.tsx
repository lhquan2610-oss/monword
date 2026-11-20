import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertCircle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  Download,
  ExternalLink,
  Eye,
  FileSearch,
  FileText,
  Files,
  FolderOpen,
  FolderPlus,
  GitCompare,
  HelpCircle,
  Home,
  MapPin,
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  Share2,
  ShieldCheck,
  Trash2,
  Upload,
} from 'lucide-react';
import { bridge } from '../bridge';
import type {
  DiffDimension,
  FileProcessResult,
  FileQueueItem,
  FixSafetyTier,
  OrganizationProfile,
  OutputMode,
  PresetId,
  PresetInfo,
  ProgressPayload,
  StdAnalysisReport,
  StdDocScope,
  StdExecutionRequest,
  StdExecutionResult,
  StdFinding,
} from '../types';
import { isAdminToolsEnabled, loadDefaultDocInfo } from '../utils/platform';
import { shareOutputBlob, triggerBrowserDownload } from '../utils/webDocxEngine';
import { listWebOrgProfiles } from '../utils/webHeaderBuilder';
import {
  analyzeWebStdDocument,
  executeWebStdDocument,
  getWebStdSnapshot,
} from '../utils/webStandardizer';

export interface StandardizationWorkspaceProps {
  files: FileQueueItem[];
  onPickFiles: () => void;
  onDropWebFiles?: (fileList: FileList | File[]) => void;
  onReplaceWebFile?: (path: string, file: File) => void;
  onInspectInPreview?: (filePath: string, elementPath: string) => void;
  renderEmbeddedPreview?: (filePath: string) => React.ReactNode;
  onNavigateHome?: () => void;
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
  onPickFolder?: () => void;
  onRemoveFile?: (path: string) => void;
  onClearFiles?: () => void;
  onBatchProcess?: () => void;
  batchResults?: Record<string, FileProcessResult>;
  batchBusy?: boolean;
  batchProgress?: ProgressPayload | null;
  presets?: PresetInfo[];
  activePreset?: PresetId;
  onSelectPreset?: (presetId: PresetId) => void;
  outputModeConfig?: OutputMode;
  onChangeOutputModeConfig?: (mode: OutputMode) => void;
  onOpenOutput?: (path: string, reveal?: boolean) => void;
  onDownloadOutput?: (blobUrl: string, fileName: string) => void;
  onRestoreBackup?: (originalPath: string, backupPath: string) => void;
  onSingleExecComplete?: (file: FileQueueItem, res: StdExecutionResult) => void;
  preferredFilePath?: string | null;
  preferredScope?: StdDocScope | null;
  onClearPreferredTarget?: () => void;
}

type WizardStep = 1 | 2 | 3 | 4 | 5;
type StdGroupKey = 'formal' | 'format' | 'layout' | 'manual';

const WIZARD_STEPS: { id: WizardStep; label: string }[] = [
  { id: 1, label: 'Chọn tài liệu' },
  { id: 2, label: 'Chọn loại văn bản' },
  { id: 3, label: 'Kiểm tra tài liệu' },
  { id: 4, label: 'Xác nhận xử lý' },
  { id: 5, label: 'Xem kết quả' },
];

const DOC_TYPE_CARDS: { id: StdDocScope; title: string; description: string }[] = [
  {
    id: 'adminNd30',
    title: 'Văn bản hành chính',
    description:
      'Công văn, quyết định, tờ trình, báo cáo, thông báo, biên bản theo thể thức hành chính.',
  },
  {
    id: 'partyDocument',
    title: 'Văn bản Đảng',
    description: 'Văn bản của cấp ủy, tổ chức Đảng theo quy định trình bày văn bản Đảng.',
  },
  {
    id: 'enterpriseInternal',
    title: 'Văn bản doanh nghiệp',
    description: 'Quy chế, thông báo, tờ trình, biên bản họp và văn bản điều hành nội bộ.',
  },
  {
    id: 'academicThesis',
    title: 'Luận văn và tài liệu khác',
    description: 'Luận văn, báo cáo học thuật, tài liệu nghiên cứu hoặc văn bản tự do.',
  },
];

const ADMIN_SCOPE_OPTIONS: { id: StdDocScope; label: string }[] = [
  { id: 'autoDetect', label: 'Tự động nhận diện phạm vi' },
  { id: 'adminNd30', label: 'Văn bản hành chính (NĐ 30/2020/NĐ-CP)' },
  { id: 'enterpriseInternal', label: 'Văn bản nội bộ Doanh nghiệp' },
  { id: 'partyDocument', label: 'Văn bản Đảng (36-HD/VPTW)' },
  { id: 'specializedLegal', label: 'Văn bản quy phạm / chuyên ngành' },
  { id: 'academicThesis', label: 'Luận văn / Báo cáo học thuật' },
  { id: 'freeform', label: 'Tài liệu tự do' },
];

const STD_GROUP_META: { key: StdGroupKey; title: string; subtitle: string }[] = [
  {
    key: 'formal',
    title: 'A. Thể thức văn bản',
    subtitle: 'Quốc hiệu, tiêu ngữ, tên cơ quan, số/ký hiệu, ngày tháng, tên loại, trích yếu, chữ ký và nơi nhận.',
  },
  {
    key: 'format',
    title: 'B. Định dạng văn bản',
    subtitle: 'Phông chữ, cỡ chữ, căn lề đoạn, giãn dòng, khoảng cách đoạn và định dạng tiêu đề.',
  },
  {
    key: 'layout',
    title: 'C. Bố cục và trình bày',
    subtitle: 'Lề trang, khổ giấy, hướng giấy, bảng biểu, hình ảnh, ngắt trang và đánh số trang.',
  },
  {
    key: 'manual',
    title: 'D. Nội dung cần kiểm tra thủ công',
    subtitle: 'Các nội dung nghiệp vụ, số liệu, ngày tháng hoặc trường hợp hệ thống chưa thể xác định tự động.',
  },
];

const DIFF_DIM_LABELS: Record<DiffDimension, string> = {
  textContent: 'Nội dung chữ',
  fontFamily: 'Phông chữ',
  fontSize: 'Cỡ chữ',
  fontStyle: 'Kiểu/Màu chữ',
  alignment: 'Căn chỉnh',
  spacingAndIndent: 'Thụt lề & Giãn dòng',
  pageMarginsAndSize: 'Khổ giấy & Lề',
  sectionStructure: 'Section',
  tableStructure: 'Bảng biểu',
  headerFooterAndPageNum: 'Header/Footer & Số trang',
};

const TIER_PLAIN_META: Record<FixSafetyTier, { label: string; cls: string }> = {
  groupAAuto: {
    label: 'Có thể tự động sửa an toàn',
    cls: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-300',
  },
  groupBConfirm: {
    label: 'Cần bạn xác nhận trước khi sửa',
    cls: 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300',
  },
  groupCManualOnly: {
    label: 'Không tự động sửa (kiểm tra thủ công)',
    cls: 'bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300',
  },
};

function getInitialStdMode(): 'single' | 'batch' {
  if (typeof window === 'undefined') return 'single';
  try {
    const p = new URLSearchParams(window.location.search).get('stdMode');
    if (p === 'batch') return 'batch';
  } catch {
    // ignore
  }
  return 'single';
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return 'Chưa rõ dung lượng';
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function classifyFindingGroup(f: StdFinding): StdGroupKey {
  if (f.safetyTier === 'groupCManualOnly' || f.status === 'indeterminate') {
    return 'manual';
  }
  const text = `${f.ruleName} ${f.location} ${f.proposal}`.toLowerCase();
  if (
    f.category === 'capitalizationAndAbbreviation' ||
    text.includes('quốc hiệu') ||
    text.includes('tiêu ngữ') ||
    text.includes('thể thức') ||
    text.includes('số và ký hiệu') ||
    text.includes('địa danh') ||
    text.includes('chữ ký') ||
    text.includes('nơi nhận') ||
    text.includes('tên loại') ||
    text.includes('trích yếu')
  ) {
    return 'formal';
  }
  if (
    f.category === 'fontAndEncoding' ||
    f.category === 'paragraphAndAlignment' ||
    f.category === 'headingAndStructure'
  ) {
    return 'format';
  }
  if (f.status === 'needsReview' && (text.includes('ngày') || text.includes('người ký') || text.includes('nội dung'))) {
    return 'manual';
  }
  return 'layout';
}

function mapDocKindLabel(scope: StdDocScope | null): string {
  if (!scope) return 'Chưa chọn';
  return (
    DOC_TYPE_CARDS.find((d) => d.id === scope)?.title ||
    ADMIN_SCOPE_OPTIONS.find((d) => d.id === scope)?.label ||
    'Tài liệu Word'
  );
}

function sanitizeOutputBaseName(name: string): string {
  const clean = name.replace(/\.[^.]+$/, '');
  return `${clean}_chuanhoa_ban_khac.docx`;
}

export const StandardizationWorkspace: React.FC<StandardizationWorkspaceProps> = ({
  files,
  onPickFiles,
  onDropWebFiles,
  onReplaceWebFile,
  onInspectInPreview,
  renderEmbeddedPreview,
  onNavigateHome,
  onStatusToast,
  onPickFolder,
  onRemoveFile,
  onClearFiles,
  onBatchProcess,
  batchResults = {},
  batchBusy = false,
  batchProgress = null,
  presets = [],
  activePreset = 'nd30',
  onSelectPreset,
  outputModeConfig = 'newCopy',
  onChangeOutputModeConfig,
  onOpenOutput,
  onDownloadOutput,
  onRestoreBackup,
  onSingleExecComplete,
  preferredFilePath,
  preferredScope,
  onClearPreferredTarget,
}) => {
  const isNative = bridge.isNativeHost();
  const adminMode = isAdminToolsEnabled();
  const [mode, setMode] = useState<'single' | 'batch'>(() => getInitialStdMode());

  const wordFiles = useMemo(
    () =>
      files.filter((f) => {
        const ext = f.ext.toLowerCase();
        return ext === 'docx' || ext === 'docm' || ext === 'doc';
      }),
    [files],
  );

  const [step, setStep] = useState<WizardStep>(() =>
    preferredFilePath && wordFiles.some((f) => f.path === preferredFilePath) ? 2 : 1,
  );
  const [selectedPath, setSelectedPath] = useState<string>(() =>
    preferredFilePath && wordFiles.some((f) => f.path === preferredFilePath)
      ? preferredFilePath
      : wordFiles[0]?.path || '',
  );
  const [selectedScope, setSelectedScope] = useState<StdDocScope | null>(() =>
    preferredScope && preferredScope !== 'autoDetect' ? preferredScope : null,
  );
  const [outputMode, setOutputMode] = useState<OutputMode>(outputModeConfig);
  const [orgProfiles, setOrgProfiles] = useState<OrganizationProfile[]>([]);
  const [orgProfileId, setOrgProfileId] = useState<string>('');
  const [dropWarning, setDropWarning] = useState<string | null>(null);
  const [isDropOver, setIsDropOver] = useState<boolean>(false);

  useEffect(() => {
    setOutputMode(outputModeConfig);
  }, [outputModeConfig]);

  const [applyAllGroupA, setApplyAllGroupA] = useState<boolean>(true);
  const [approvedIds, setApprovedIds] = useState<string[]>([]);
  const [skippedIds, setSkippedIds] = useState<string[]>([]);
  const [manualReviewIds, setManualReviewIds] = useState<string[]>([]);
  const [diffFilter, setDiffFilter] = useState<'all' | DiffDimension>('all');

  const [analyzing, setAnalyzing] = useState(false);
  const [analysisError, setAnalysisError] = useState<string | null>(null);
  const [executing, setExecuting] = useState(false);
  const [execError, setExecError] = useState<string | null>(null);
  const [report, setReport] = useState<StdAnalysisReport | null>(null);
  const [execResult, setExecResult] = useState<StdExecutionResult | null>(null);
  const [highlightedPath, setHighlightedPath] = useState<string | null>(null);
  const [backupByPath, setBackupByPath] = useState<Record<string, string>>({});
  const [saveAsOpen, setSaveAsOpen] = useState(false);
  const [saveAsName, setSaveAsName] = useState('');

  const inspectorRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let alive = true;
    const loadOrgs = async () => {
      const defId = loadDefaultDocInfo().defaultOrgProfileId;
      let loaded: OrganizationProfile[] = [];
      if (isNative) {
        try {
          const list = await bridge.invoke<OrganizationProfile[]>('list_org_profiles');
          loaded = Array.isArray(list) ? list : [];
        } catch {
          // ignore
        }
      } else {
        loaded = listWebOrgProfiles();
      }
      if (!alive) return;
      setOrgProfiles(loaded);
      if (defId && loaded.some((o) => o.id === defId)) {
        setOrgProfileId((prev) => prev || defId);
      }
    };
    void loadOrgs();
    return () => {
      alive = false;
    };
  }, [isNative]);

  // Apply handoff target when coming from ComplianceAuditWorkspace ("Chuẩn hóa tài liệu này")
  useEffect(() => {
    if (!preferredFilePath) return;
    if (wordFiles.some((f) => f.path === preferredFilePath)) {
      setMode('single');
      setSelectedPath(preferredFilePath);
      setReport(null);
      setExecResult(null);
      if (preferredScope && preferredScope !== 'autoDetect') {
        setSelectedScope(preferredScope);
      }
      setStep(2);
    }
    onClearPreferredTarget?.();
  }, [preferredFilePath, preferredScope, wordFiles, onClearPreferredTarget]);

  useEffect(() => {
    if (wordFiles.length === 0) {
      setSelectedPath('');
      setReport(null);
      setExecResult(null);
      setStep(1);
      return;
    }
    if (preferredFilePath && wordFiles.some((f) => f.path === preferredFilePath)) {
      return;
    }
    if (!wordFiles.some((f) => f.path === selectedPath)) {
      setSelectedPath(wordFiles[0].path);
      setApprovedIds([]);
      setSkippedIds([]);
      setManualReviewIds([]);
      setReport(null);
      setExecResult(null);
      setStep(1);
    }
  }, [wordFiles, selectedPath, preferredFilePath]);

  const activeFile = useMemo(
    () => wordFiles.find((f) => f.path === selectedPath) ?? null,
    [wordFiles, selectedPath],
  );

  const activeOrg = useMemo(
    () => orgProfiles.find((p) => p.id === orgProfileId) ?? null,
    [orgProfiles, orgProfileId],
  );

  const runAnalysis = useCallback(
    async (targetScope: StdDocScope, moveToStep3 = true): Promise<boolean> => {
      if (!activeFile) {
        onStatusToast('error', 'Vui lòng chọn tệp Word trước khi phân tích.');
        return false;
      }
      setAnalyzing(true);
      setAnalysisError(null);
      setHighlightedPath(null);
      try {
        const rep = isNative
          ? await bridge.invoke<StdAnalysisReport>('analyze_std_document', {
              path: activeFile.path,
              scope: targetScope,
              orgProfile: activeOrg,
            })
          : await analyzeWebStdDocument(activeFile, targetScope, activeOrg);
        setReport(rep);
        setApprovedIds([]);
        setSkippedIds([]);
        setManualReviewIds([]);
        setExecResult(null);
        if (rep.requiresScopeSelection) {
          onStatusToast('info', rep.scopeReason);
        }
        if (moveToStep3) {
          setStep(3);
        }
        return true;
      } catch (err) {
        const msg = `Không thể phân tích tài liệu: ${(err as Error).message}`;
        setAnalysisError(msg);
        onStatusToast('error', msg);
        return false;
      } finally {
        setAnalyzing(false);
      }
    },
    [activeFile, activeOrg, isNative, onStatusToast],
  );

  const handleStep1Drop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDropOver(false);
    setDropWarning(null);
    const dropped = e.dataTransfer?.files;
    if (!dropped || dropped.length === 0) return;

    const arr = Array.from(dropped);
    const invalid = arr.filter((f) => !/\.(docx|docm|doc)$/i.test(f.name));
    if (invalid.length > 0 && invalid.length === arr.length) {
      setDropWarning(
        `Định dạng tệp "${invalid[0].name}" không được hỗ trợ. Vui lòng chọn tệp Word (.docx, .docm hoặc .doc).`,
      );
      return;
    }
    if (!isNative && arr.every((f) => /\.doc$/i.test(f.name))) {
      setDropWarning(
        'Định dạng .doc cũ cần Microsoft Word trên bản Windows Desktop. Trên trình duyệt hãy dùng tệp .docx hoặc .docm.',
      );
      return;
    }
    if (onDropWebFiles) {
      onDropWebFiles(arr);
    }
  };

  const handleContinueFromStep1 = () => {
    if (!activeFile) {
      setDropWarning('Vui lòng chọn hoặc kéo thả một tài liệu Word hợp lệ.');
      return;
    }
    if (!isNative && activeFile.ext.toLowerCase() === 'doc') {
      setDropWarning(
        'Tệp .doc cũ cần mở trên ứng dụng Windows Desktop có Microsoft Word để chuyển đổi an toàn.',
      );
      return;
    }
    setDropWarning(null);
    setStep(2);
  };

  const handleAnalyzeFromStep2 = async () => {
    if (!selectedScope) {
      onStatusToast('info', 'Vui lòng chọn một trong 4 loại văn bản trước khi tiếp tục.');
      return;
    }
    await runAnalysis(selectedScope, true);
  };

  const groupedFindings = useMemo(() => {
    const groups: Record<StdGroupKey, StdFinding[]> = {
      formal: [],
      format: [],
      layout: [],
      manual: [],
    };
    if (!report) return groups;
    for (const f of report.findings) {
      groups[classifyFindingGroup(f)].push(f);
    }
    return groups;
  }, [report]);

  const actionGroups = useMemo(() => {
    const groupA: StdFinding[] = [];
    const groupB: StdFinding[] = [];
    const groupC: StdFinding[] = [];
    if (!report) return { groupA, groupB, groupC };
    for (const f of report.findings) {
      if (!f.autoFixSupported || f.safetyTier === 'groupCManualOnly') groupC.push(f);
      else if (f.safetyTier === 'groupBConfirm') groupB.push(f);
      else groupA.push(f);
    }
    return { groupA, groupB, groupC };
  }, [report]);

  const isFindingSelectedForFix = useCallback(
    (f: StdFinding): boolean => {
      if (!f.autoFixSupported || f.safetyTier === 'groupCManualOnly') return false;
      if (skippedIds.includes(f.findingId) || manualReviewIds.includes(f.findingId)) return false;
      if (f.safetyTier === 'groupAAuto') {
        return applyAllGroupA || approvedIds.includes(f.findingId);
      }
      return approvedIds.includes(f.findingId);
    },
    [applyAllGroupA, approvedIds, skippedIds, manualReviewIds],
  );

  const previewFixCount = useMemo(() => {
    if (!report) return 0;
    return report.findings.filter((f) => isFindingSelectedForFix(f)).length;
  }, [report, isFindingSelectedForFix]);

  const toggleFindingFixSelection = (f: StdFinding) => {
    if (!f.autoFixSupported || f.safetyTier === 'groupCManualOnly') return;
    if (f.safetyTier === 'groupAAuto' && applyAllGroupA) {
      setSkippedIds((prev) =>
        prev.includes(f.findingId) ? prev.filter((x) => x !== f.findingId) : [...prev, f.findingId],
      );
      return;
    }
    setSkippedIds((prev) => prev.filter((x) => x !== f.findingId));
    setApprovedIds((prev) =>
      prev.includes(f.findingId) ? prev.filter((x) => x !== f.findingId) : [...prev, f.findingId],
    );
  };

  const runExecute = async () => {
    if (executing) return;
    if (!activeFile || !selectedScope) {
      onStatusToast('error', 'Vui lòng chọn tệp tài liệu và loại văn bản trước.');
      return;
    }
    setExecuting(true);
    setExecError(null);
    const req: StdExecutionRequest = {
      path: activeFile.path,
      scope: selectedScope,
      orgProfile: activeOrg,
      outputMode,
      applyAllGroupA,
      approvedFindingIds: approvedIds,
      skippedFindingIds: skippedIds,
      manualReviewFindingIds: manualReviewIds,
      disabledRuleIds: [],
    };

    try {
      const res = isNative
        ? await bridge.invoke<StdExecutionResult>('execute_std_document', req)
        : await executeWebStdDocument(activeFile, req);

      setExecResult(res);
      if (res.backupPath) {
        setBackupByPath((prev) => ({ ...prev, [activeFile.path]: res.backupPath! }));
      }
      if (res.reportAfter) {
        setReport(res.reportAfter);
      }

      const verifiedOk =
        res.success && (!res.verification || res.verification.xmlValid);

      if (verifiedOk) {
        if (!isNative && res.outputBlob && outputMode === 'overwriteWithBackup' && onReplaceWebFile) {
          const updated = new File([res.outputBlob], activeFile.name, {
            type: res.outputBlob.type,
          });
          onReplaceWebFile(activeFile.path, updated);
        }
        onSingleExecComplete?.(activeFile, res);
        setSaveAsName(sanitizeOutputBaseName(activeFile.name));
        setSaveAsOpen(false);
        setStep(5);
        onStatusToast(
          'success',
          `Đã chuẩn hóa và xác minh tệp đầu ra thành công (${res.actionsApplied.length} thay đổi).`,
        );
      } else {
        const errMsg =
          res.error ||
          (!res.verification?.xmlValid
            ? 'Tệp sau chuẩn hóa chưa đạt kiểm tra cấu trúc. Đã giữ nguyên tệp gốc.'
            : 'Chuẩn hóa không hoàn tất.');
        setExecError(errMsg);
        onStatusToast('error', errMsg);
      }
    } catch (err) {
      const errMsg = `Lỗi chuẩn hóa: ${(err as Error).message}`;
      setExecError(errMsg);
      onStatusToast('error', errMsg);
    } finally {
      setExecuting(false);
    }
  };

  const handleRestore = async () => {
    if (!activeFile) return;
    try {
      if (isNative) {
        const bak = backupByPath[activeFile.path];
        if (!bak) {
          onStatusToast('error', 'Chưa có bản sao lưu cho tệp này.');
          return;
        }
        const out = await bridge.invoke<{ message: string }>('restore_std_backup', {
          originalPath: activeFile.path,
          backupPath: bak,
        });
        onStatusToast('info', out.message);
        if (selectedScope) {
          await runAnalysis(selectedScope, false);
        }
      } else {
        const snap = getWebStdSnapshot(activeFile.path);
        if (!snap || !onReplaceWebFile) {
          onStatusToast('error', 'Không tìm thấy bản gốc trong bộ nhớ trình duyệt.');
          return;
        }
        onReplaceWebFile(activeFile.path, snap);
        if (selectedScope) {
          const restoredRep = await analyzeWebStdDocument(
            { ...activeFile, webFile: snap },
            selectedScope,
            activeOrg,
          );
          setReport(restoredRep);
        }
        onStatusToast('info', `Đã hoàn tác về bản gốc "${activeFile.name}".`);
      }
      setExecResult(null);
      setStep(4);
    } catch (err) {
      onStatusToast('error', `Lỗi hoàn tác: ${(err as Error).message}`);
    }
  };

  const filteredDiffs = useMemo(() => {
    if (!execResult?.diff) return [];
    if (diffFilter === 'all') return execResult.diff.entries;
    return execResult.diff.entries.filter((e) => e.dimension === diffFilter);
  }, [execResult, diffFilter]);

  const canRestore = Boolean(
    activeFile &&
      (backupByPath[activeFile.path] || (!isNative && getWebStdSnapshot(activeFile.path))),
  );

  const canJumpToStep = (targetStep: WizardStep): boolean => {
    if (targetStep === step) return true;
    if (targetStep < step) return true;
    if (targetStep === 2) return Boolean(activeFile);
    if (targetStep === 3) return Boolean(activeFile && selectedScope && report);
    if (targetStep === 4) return Boolean(activeFile && selectedScope && report);
    if (targetStep === 5) return Boolean(execResult?.success);
    return false;
  };

  const verifiedOutputName =
    execResult?.outputName ||
    execResult?.outputPath.split(/[/\\]/).pop() ||
    (activeFile ? sanitizeOutputBaseName(activeFile.name).replace('_ban_khac', '') : '');

  return (
    <div className="mx-auto max-w-5xl space-y-3.5">
      {/* Unified Header + Mode Switch + 5-Step Strip */}
      <div className="mw-panel overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
          <h2 className="text-base font-bold flex items-center gap-2 text-[var(--mw-ink)]">
            <ShieldCheck className="w-4 h-4 text-[var(--mw-accent)] shrink-0" />
            Chuẩn hóa văn bản
          </h2>

          <div
            role="group"
            aria-label="Chế độ chuẩn hóa"
            className="inline-flex items-center border border-[var(--mw-border)] bg-[var(--mw-canvas)] p-0.5 text-xs"
          >
            <button
              type="button"
              onClick={() => setMode('single')}
              className={`inline-flex items-center gap-1.5 px-3 py-1 font-semibold transition-colors ${
                mode === 'single'
                  ? 'bg-[var(--mw-surface)] text-[var(--mw-ink)]'
                  : 'text-[var(--mw-muted)] hover:text-[var(--mw-ink)]'
              }`}
            >
              <FileText className="size-3.5" aria-hidden="true" />
              Một tài liệu
            </button>
            <button
              type="button"
              onClick={() => setMode('batch')}
              className={`inline-flex items-center gap-1.5 px-3 py-1 font-semibold transition-colors ${
                mode === 'batch'
                  ? 'bg-[var(--mw-surface)] text-[var(--mw-ink)]'
                  : 'text-[var(--mw-muted)] hover:text-[var(--mw-ink)]'
              }`}
            >
              <Files className="size-3.5" aria-hidden="true" />
              Nhiều tài liệu
            </button>
          </div>
        </div>

        {mode === 'single' && (
          <nav
            aria-label="Tiến trình 5 bước chuẩn hóa"
            className="border-t border-[var(--mw-border)] bg-[var(--mw-surface)]"
          >
            <ol className="grid grid-cols-2 sm:grid-cols-5 divide-y sm:divide-y-0 sm:divide-x divide-[var(--mw-border)]">
              {WIZARD_STEPS.map((s) => {
                const isCurrent = step === s.id;
                const isDone = step > s.id;
                const reachable = canJumpToStep(s.id);
                return (
                  <li key={s.id}>
                    <button
                      type="button"
                      disabled={!reachable}
                      onClick={() => {
                        if (reachable) setStep(s.id);
                      }}
                      aria-current={isCurrent ? 'step' : undefined}
                      className={`w-full flex items-center gap-2 px-3 py-2 text-left text-xs transition-colors border-b-2 ${
                        isCurrent
                          ? 'border-b-[var(--mw-accent)] bg-[var(--mw-tint)] text-[var(--mw-ink)] font-bold'
                          : isDone
                            ? 'border-b-transparent text-[var(--mw-ink)] font-medium hover:bg-[var(--mw-canvas)]'
                            : 'border-b-transparent text-[var(--mw-muted)] opacity-65'
                      } disabled:cursor-not-allowed`}
                    >
                      <span className="w-4 text-center font-bold shrink-0">
                        {isDone ? '✓' : `${s.id}.`}
                      </span>
                      <span>{s.label}</span>
                    </button>
                  </li>
                );
              })}
            </ol>
          </nav>
        )}
      </div>

      {mode === 'batch' ? (
        <section className="mw-panel p-4 sm:p-5 space-y-4">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 text-xs">
            <label className="block space-y-1">
              <span className="font-semibold text-[var(--mw-muted)]">Loại quy chuẩn áp dụng</span>
              <select
                value={activePreset}
                onChange={(e) => onSelectPreset?.(e.target.value as PresetId)}
                className="w-full rounded-lg border border-[var(--mw-border)] bg-[var(--mw-surface)] px-3 py-2 text-xs font-medium"
              >
                {presets.length === 0 ? (
                  <option value="nd30">Văn bản hành chính (NĐ 30/2020/NĐ-CP)</option>
                ) : (
                  presets.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))
                )}
              </select>
            </label>

            <label className="block space-y-1">
              <span className="font-semibold text-[var(--mw-muted)]">Cách lưu kết quả</span>
              <select
                value={outputModeConfig}
                onChange={(e) => onChangeOutputModeConfig?.(e.target.value as OutputMode)}
                className="w-full rounded-lg border border-[var(--mw-border)] bg-[var(--mw-surface)] px-3 py-2 text-xs font-medium"
              >
                <option value="newCopy">Lưu thành tệp mới (_chuanhoa)</option>
                <option value="overwriteWithBackup">Ghi đè tệp gốc (có sao lưu an toàn)</option>
              </select>
            </label>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--mw-border)] pt-3">
            <div className="text-xs font-semibold text-[var(--mw-ink)]">
              Danh sách tài liệu ({files.length})
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={onPickFiles}
                className="mw-button mw-button-secondary text-xs"
              >
                <FolderOpen className="size-3.5" aria-hidden="true" />
                Chọn thêm tài liệu
              </button>
              {onPickFolder && (
                <button
                  type="button"
                  onClick={onPickFolder}
                  className="mw-button mw-button-secondary text-xs"
                >
                  <FolderPlus className="size-3.5" aria-hidden="true" />
                  Chọn thư mục
                </button>
              )}
              {files.length > 0 && onClearFiles && (
                <button
                  type="button"
                  onClick={onClearFiles}
                  className="mw-button mw-button-secondary text-xs text-rose-600"
                >
                  <Trash2 className="size-3.5" aria-hidden="true" />
                  Xóa danh sách
                </button>
              )}
            </div>
          </div>

          {files.length === 0 ? (
            <div className="mw-empty p-6 text-center text-xs text-[var(--mw-muted)] space-y-2">
              <p>
                Chưa có tài liệu nào được chọn. Bấm &quot;Chọn thêm tài liệu&quot; hoặc &quot;Chọn
                thư mục&quot; để thêm nhiều tệp cùng lúc.
              </p>
            </div>
          ) : (
            <div className="divide-y divide-[var(--mw-border)] rounded-lg border border-[var(--mw-border)] max-h-72 overflow-y-auto">
              {files.map((item) => {
                const res = batchResults[item.path];
                return (
                  <div
                    key={item.path}
                    className="flex flex-wrap items-center justify-between gap-2 p-3 text-xs"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="font-semibold text-[var(--mw-ink)] truncate">{item.name}</div>
                      {res ? (
                        <div
                          className={`text-[11px] ${
                            res.success
                              ? 'text-emerald-700 dark:text-emerald-300'
                              : 'text-rose-600'
                          }`}
                        >
                          {res.success ? res.statsSummary : res.error || 'Có lỗi khi xử lý'}
                        </div>
                      ) : (
                        <div className="text-[11px] text-[var(--mw-muted)]">Chờ xử lý</div>
                      )}
                    </div>

                    <div className="flex flex-wrap items-center gap-1.5">
                      {res?.success && res.blobUrl && onDownloadOutput && (
                        <button
                          type="button"
                          onClick={() =>
                            onDownloadOutput(res.blobUrl!, res.outputName || res.outputPath)
                          }
                          className="mw-button mw-button-secondary text-xs"
                        >
                          <Download className="size-3" aria-hidden="true" />
                          Tải về
                        </button>
                      )}
                      {res?.success && !item.path.startsWith('web://') && onOpenOutput && (
                        <button
                          type="button"
                          onClick={() => onOpenOutput(res.outputPath, false)}
                          className="mw-button mw-button-secondary text-xs"
                        >
                          Mở kết quả
                        </button>
                      )}
                      {res?.backupPath && !item.path.startsWith('web://') && onRestoreBackup && (
                        <button
                          type="button"
                          onClick={() => onRestoreBackup(item.path, res.backupPath!)}
                          className="mw-button mw-button-secondary text-xs"
                        >
                          <RotateCcw className="size-3" aria-hidden="true" />
                          Khôi phục
                        </button>
                      )}
                      {onRemoveFile && (
                        <button
                          type="button"
                          onClick={() => onRemoveFile(item.path)}
                          className="p-1 text-slate-400 hover:text-rose-600"
                          title="Xóa khỏi danh sách"
                        >
                          <Trash2 className="size-3.5" aria-hidden="true" />
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
            {batchBusy && batchProgress ? (
              <div className="inline-flex items-center gap-1.5 text-xs font-semibold text-blue-600">
                <RefreshCw className="size-3.5 animate-spin" aria-hidden="true" />
                Đang xử lý {batchProgress.current}/{batchProgress.total}...
              </div>
            ) : (
              <span className="text-xs text-[var(--mw-muted)]">
                Chỉ giữ nguyên nội dung gốc, tự động đồng bộ phông chữ, cỡ chữ và căn lề.
              </span>
            )}
            <button
              type="button"
              disabled={batchBusy || files.length === 0 || !onBatchProcess}
              onClick={onBatchProcess}
              className="mw-button mw-button-primary"
            >
              <Play className="size-3.5 fill-current" aria-hidden="true" />
              Chuẩn hóa tất cả ({files.length})
            </button>
          </div>
        </section>
      ) : (
        <>
          {/* BƯỚC 1 – CHỌN TÀI LIỆU */}
          {step === 1 && (
            <section className="mw-panel p-4 sm:p-5 space-y-4">
              <h3 className="text-sm font-bold text-[var(--mw-ink)]">
                Bước 1: Chọn tài liệu Word cần chuẩn hóa
              </h3>

              <div
                onDragOver={(e) => {
                  e.preventDefault();
                  setIsDropOver(true);
                }}
                onDragLeave={() => setIsDropOver(false)}
                onDrop={handleStep1Drop}
                className={`mw-empty flex flex-col items-center justify-center gap-3 px-6 py-8 text-center transition-colors ${
                  isDropOver ? 'border-[var(--mw-focus)] bg-[var(--mw-tint)]' : ''
                }`}
              >
                <Upload className="size-8 text-[var(--mw-accent)]" aria-hidden="true" />
                <div className="space-y-1">
                  <div className="text-sm font-bold text-[var(--mw-ink)]">
                    Kéo thả tài liệu Word vào đây
                  </div>
                  <p className="text-xs text-[var(--mw-muted)]">
                    Hoặc chọn trực tiếp tệp từ máy tính ({isNative ? 'DOCX, DOCM, DOC' : 'DOCX, DOCM'})
                  </p>
                </div>
                <div className="flex flex-wrap items-center justify-center gap-2">
                  <button type="button" onClick={onPickFiles} className="mw-button mw-button-secondary">
                    <FolderOpen className="size-4" aria-hidden="true" />
                    Chọn tài liệu
                  </button>
                  {onPickFolder && (
                    <button
                      type="button"
                      onClick={onPickFolder}
                      className="mw-button mw-button-secondary"
                    >
                      <FolderPlus className="size-4" aria-hidden="true" />
                      Chọn thư mục
                    </button>
                  )}
                </div>
              </div>

              {dropWarning && (
                <div role="alert" className="mw-toast mw-toast-err text-xs">
                  <AlertCircle className="size-4 shrink-0" aria-hidden="true" />
                  <span>{dropWarning}</span>
                </div>
              )}

              {activeFile && (
                <div className="rounded-xl border border-[var(--mw-border)] bg-[var(--mw-canvas)] p-4 space-y-3">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="flex items-start gap-3 min-w-0">
                      <FileText className="size-5 shrink-0 text-[var(--mw-accent)] mt-0.5" aria-hidden="true" />
                      <div className="min-w-0 space-y-1">
                        <div className="text-sm font-bold text-[var(--mw-ink)] break-all">
                          {activeFile.name}
                        </div>
                        <div className="flex flex-wrap items-center gap-3 text-xs text-[var(--mw-muted)]">
                          <span>Dung lượng: <strong>{formatBytes(activeFile.sizeBytes)}</strong></span>
                          <span>Định dạng: <strong>{activeFile.ext.toUpperCase()}</strong></span>
                          <span className="inline-flex items-center gap-1 text-[var(--mw-ink)] font-semibold">
                            <CheckCircle2 className="size-3.5" aria-hidden="true" />
                            Đã tải sẵn sàng
                          </span>
                        </div>
                      </div>
                    </div>

                    <div className="flex flex-wrap items-center gap-2">
                      <button
                        type="button"
                        onClick={onPickFiles}
                        className="mw-button mw-button-secondary text-xs"
                      >
                        Thay thế tệp khác
                      </button>
                      {onRemoveFile && (
                        <button
                          type="button"
                          onClick={() => {
                            onRemoveFile(activeFile.path);
                            setReport(null);
                            setExecResult(null);
                          }}
                          className="mw-button mw-button-danger text-xs"
                        >
                          Loại bỏ tệp
                        </button>
                      )}
                    </div>
                  </div>

                  {wordFiles.length > 1 && (
                    <label className="block text-xs space-y-1 pt-2 border-t border-[var(--mw-border)]">
                      <span className="font-semibold text-[var(--mw-muted)]">
                        Đang có {wordFiles.length} tệp Word — chọn tệp cần xử lý:
                      </span>
                      <select
                        value={selectedPath}
                        onChange={(e) => {
                          setSelectedPath(e.target.value);
                          setReport(null);
                          setExecResult(null);
                        }}
                        className="w-full rounded-lg border border-[var(--mw-border)] bg-[var(--mw-surface)] px-3 py-1.5 text-xs"
                      >
                        {wordFiles.map((f) => (
                          <option key={f.path} value={f.path}>
                            {f.name} ({formatBytes(f.sizeBytes)})
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                </div>
              )}

              <div className="flex items-center justify-end gap-2 pt-2 border-t border-[var(--mw-border)]">
                <button
                  type="button"
                  disabled={!activeFile}
                  onClick={handleContinueFromStep1}
                  className="mw-button mw-button-primary px-5 py-2 text-sm"
                >
                  Tiếp tục
                  <ArrowRight className="size-4" aria-hidden="true" />
                </button>
              </div>
            </section>
          )}

          {/* BƯỚC 2 – CHỌN LOẠI VĂN BẢN */}
          {step === 2 && (
            <section className="mw-panel p-4 sm:p-5 space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-bold text-[var(--mw-ink)]">
                  Bước 2: Chọn loại văn bản cần chuẩn hóa
                </h3>
                <span className="text-xs text-[var(--mw-muted)]">
                  Tài liệu: <strong className="text-[var(--mw-ink)]">{activeFile?.name}</strong>
                </span>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                {DOC_TYPE_CARDS.map((card) => {
                  const chosen = selectedScope === card.id;
                  return (
                    <button
                      key={card.id}
                      type="button"
                      aria-pressed={chosen}
                      onClick={() => {
                        setSelectedScope(card.id);
                        setAnalysisError(null);
                      }}
                      className={`text-left rounded-xl border p-4 space-y-1.5 transition-colors ${
                        chosen
                          ? 'border-[var(--mw-accent)] bg-[var(--mw-tint)] ring-1 ring-[var(--mw-focus)]'
                          : 'border-[var(--mw-border)] bg-[var(--mw-surface)] hover:border-[var(--mw-focus)]'
                      }`}
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-sm font-bold text-[var(--mw-ink)]">{card.title}</span>
                        {chosen && (
                          <CheckCircle2 className="size-4 text-[var(--mw-accent)] shrink-0" aria-hidden="true" />
                        )}
                      </div>
                      <p className="text-xs leading-5 text-[var(--mw-muted)]">{card.description}</p>
                    </button>
                  );
                })}
              </div>

              {adminMode && (
                <div className="rounded-xl border border-[var(--mw-border)] bg-[var(--mw-canvas)] p-3 grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
                  <label className="block space-y-1">
                    <span className="font-semibold text-[var(--mw-muted)]">
                      Phạm vi mở rộng (?admin=1)
                    </span>
                    <select
                      value={selectedScope || ''}
                      onChange={(e) => setSelectedScope((e.target.value || null) as StdDocScope | null)}
                      className="w-full rounded-lg border border-[var(--mw-border)] px-2.5 py-1.5 text-xs"
                    >
                      <option value="">-- Chưa chọn --</option>
                      {ADMIN_SCOPE_OPTIONS.map((o) => (
                        <option key={o.id} value={o.id}>
                          {o.label}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="block space-y-1">
                    <span className="font-semibold text-[var(--mw-muted)]">Hồ sơ cơ quan (?admin=1)</span>
                    <select
                      value={orgProfileId}
                      onChange={(e) => setOrgProfileId(e.target.value)}
                      className="w-full rounded-lg border border-[var(--mw-border)] px-2.5 py-1.5 text-xs"
                    >
                      <option value="">-- Không áp hồ sơ riêng --</option>
                      {orgProfiles.map((o) => (
                        <option key={o.id} value={o.id}>
                          {o.profileName}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              )}

              {analysisError && (
                <div role="alert" className="mw-toast mw-toast-err text-xs">
                  <AlertCircle className="size-4 shrink-0" aria-hidden="true" />
                  <span>{analysisError}</span>
                </div>
              )}

              <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-[var(--mw-border)]">
                <button
                  type="button"
                  onClick={() => setStep(1)}
                  className="mw-button mw-button-secondary"
                >
                  <ArrowLeft className="size-4" aria-hidden="true" />
                  Quay lại bước trước
                </button>
                <button
                  type="button"
                  disabled={!selectedScope || analyzing}
                  onClick={() => void handleAnalyzeFromStep2()}
                  className="mw-button mw-button-primary px-5 py-2 text-sm"
                >
                  <FileSearch className="size-4" aria-hidden="true" />
                  {analyzing ? 'Đang phân tích tài liệu...' : 'Phân tích tài liệu'}
                </button>
              </div>
            </section>
          )}

          {/* BƯỚC 3 – PHÂN TÍCH TÀI LIỆU (4 NHÓM NGÔN NGỮ TỰ NHIÊN) */}
          {step === 3 && report && (
            <section className="mw-panel p-5 sm:p-6 space-y-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                  <h3 className="text-base font-bold text-[var(--mw-ink)]">
                    Bước 3: Kết quả kiểm tra tài liệu
                  </h3>
                  <p className="text-xs text-[var(--mw-muted)]">
                    Tài liệu: <strong>{activeFile?.name}</strong> · Loại văn bản:{' '}
                    <strong>{mapDocKindLabel(selectedScope)}</strong>
                    {adminMode ? ` · ${report.paragraphCount} đoạn · ${report.tables.length} bảng` : ''}
                  </p>
                </div>

                {activeFile && onInspectInPreview && (
                  <button
                    type="button"
                    onClick={() => onInspectInPreview(activeFile.path, '')}
                    className="mw-button mw-button-secondary text-xs"
                  >
                    <Eye className="size-4 text-[var(--mw-accent)]" aria-hidden="true" />
                    Xem trước trang A4
                  </button>
                )}
              </div>

              {report.requiresScopeSelection && (
                <div className="mw-warning rounded-xl border border-amber-300 dark:border-amber-800 p-3 text-xs flex items-start gap-2">
                  <HelpCircle className="size-4 shrink-0 mt-0.5" aria-hidden="true" />
                  <div>
                    <strong>Cần xác nhận lại loại văn bản:</strong> Hệ thống nhận thấy tài liệu có đặc điểm chưa rõ ràng theo phạm vi đang chọn. Bạn có thể quay lại Bước 2 để đổi loại văn bản nếu cần.
                  </div>
                </div>
              )}

              {report.findings.length === 0 ? (
                <div className="border border-[var(--mw-border)] bg-[var(--mw-surface)] p-4 text-xs text-[var(--mw-ink)] flex items-start gap-2.5">
                  <CheckCircle2 className="size-4 shrink-0 mt-0.5" aria-hidden="true" />
                  <div className="font-semibold">
                    Chưa phát hiện vấn đề cần chỉnh sửa thuộc phạm vi kiểm tra.
                  </div>
                </div>
              ) : (
                <div className="space-y-4">
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-center text-xs">
                    {STD_GROUP_META.map((grp) => (
                      <div
                        key={grp.key}
                        className="border border-[var(--mw-border)] bg-[var(--mw-surface)] p-2.5"
                      >
                        <div className="text-base font-bold text-[var(--mw-ink)]">
                          {groupedFindings[grp.key].length}
                        </div>
                        <div className="text-[11px] font-semibold text-[var(--mw-muted)]">
                          {grp.title}
                        </div>
                      </div>
                    ))}
                  </div>

                  {STD_GROUP_META.map((grp) => {
                    const list = groupedFindings[grp.key];
                    if (list.length === 0) return null;
                    const isManualGroup = grp.key === 'manual';
                    return (
                      <div
                        key={grp.key}
                        className="border border-[var(--mw-border)] overflow-hidden"
                      >
                        <div className="border-b border-[var(--mw-border)] bg-[var(--mw-canvas)] px-3.5 py-2">
                          <div className="text-xs font-bold text-[var(--mw-ink)]">
                            {grp.title} ({list.length})
                          </div>
                        </div>

                        <div className="divide-y divide-[var(--mw-border)]">
                          {list.map((f) => {
                            const tier = TIER_PLAIN_META[f.safetyTier];
                            return (
                              <div key={f.findingId} className="p-3 text-xs space-y-1.5">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                  <div className="flex flex-wrap items-center gap-2">
                                    {adminMode && (
                                      <span className="font-mono font-bold text-[10px] px-1.5 py-0.5 bg-[var(--mw-canvas)] border border-[var(--mw-border)]">
                                        {f.ruleId}
                                      </span>
                                    )}
                                    <span className="font-bold text-[var(--mw-ink)]">
                                      {f.ruleName}
                                    </span>
                                  </div>
                                  <span className="border border-[var(--mw-border)] bg-[var(--mw-canvas)] px-2 py-0.5 text-[11px] font-semibold text-[var(--mw-ink)]">
                                    {isManualGroup ? 'Cần kiểm tra thủ công' : 'Cần chỉnh sửa'}
                                  </span>
                                </div>

                                <p className="text-[11px] text-[var(--mw-muted)] leading-5">
                                  {f.proposal}
                                  {f.location ? ` (Vị trí: ${f.location})` : ''}
                                </p>

                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 border border-[var(--mw-border)] bg-[var(--mw-canvas)] px-2.5 py-1.5 text-[11px]">
                                  <div>
                                    <span className="font-semibold text-[var(--mw-muted)]">Hiện tại: </span>
                                    <span className="text-[var(--mw-ink)]">
                                      {f.currentValue || 'Chưa xác định'}
                                    </span>
                                  </div>
                                  <div>
                                    <span className="font-semibold text-[var(--mw-muted)]">Đề xuất: </span>
                                    <span className="font-semibold text-[var(--mw-ink)]">
                                      {f.expectedValue || 'Kiểm tra lại theo nội dung văn bản'}
                                    </span>
                                  </div>
                                </div>

                                {adminMode && (
                                  <div className="flex flex-wrap items-center justify-between gap-2 text-[11px]">
                                    <span className="border border-[var(--mw-border)] px-2 py-0.5 text-[10px] font-semibold text-[var(--mw-muted)]">
                                      {tier.label}
                                    </span>
                                    <button
                                      type="button"
                                      onClick={() => {
                                        setHighlightedPath(f.elementPath);
                                        inspectorRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
                                      }}
                                      className="inline-flex items-center gap-1 text-[11px] font-semibold text-[var(--mw-accent)] hover:underline"
                                    >
                                      <MapPin className="size-3" /> Soi XML
                                    </button>
                                  </div>
                                )}
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}

              {adminMode && (
                <div className="space-y-2 pt-2">
                  <div className="flex flex-wrap gap-1.5 text-[11px]">
                    <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
                      Ảnh/Logo: <strong>{report.assets.drawingCount}</strong> ({report.assets.mediaCount} media)
                    </span>
                    <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
                      Hyperlink: <strong>{report.assets.hyperlinkCount}</strong> · Bookmark:{' '}
                      <strong>{report.assets.bookmarkCount}</strong> · Field: <strong>{report.assets.fieldCount}</strong>
                    </span>
                  </div>
                  <div
                    ref={inspectorRef}
                    className="rounded-xl border border-[var(--mw-border)] bg-[var(--mw-canvas)] p-3 max-h-40 overflow-y-auto space-y-1 text-[11px]"
                  >
                    {report.paragraphPreviews.map((p) => (
                      <div
                        key={p.elementPath}
                        className={`p-1.5 rounded border ${
                          highlightedPath === p.elementPath ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50' : 'border-transparent'
                        }`}
                      >
                        <span className="font-mono text-[10px] text-[var(--mw-muted)] mr-1.5">
                          #{p.index + 1} [{p.role}]
                        </span>
                        <span>{p.excerpt || '(Đoạn trống)'}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-[var(--mw-border)]">
                <button
                  type="button"
                  onClick={() => setStep(2)}
                  className="mw-button mw-button-secondary"
                >
                  <ArrowLeft className="size-4" aria-hidden="true" />
                  Quay lại bước trước
                </button>
                <button
                  type="button"
                  onClick={() => setStep(4)}
                  className="mw-button mw-button-primary px-5 py-2 text-sm"
                >
                  Tiếp tục xác nhận
                  <ArrowRight className="size-4" aria-hidden="true" />
                </button>
              </div>
            </section>
          )}

          {/* BƯỚC 4 – XÁC NHẬN CHUẨN HÓA */}
          {step === 4 && report && (
            <section className="mw-panel p-5 sm:p-6 space-y-4">
              <h3 className="text-sm font-bold text-[var(--mw-ink)]">
                Bước 4: Xác nhận nội dung chuẩn hóa
              </h3>

              {/* 1. Tự động sửa an toàn */}
              <div className="border border-[var(--mw-border)] overflow-hidden">
                <div className="flex flex-wrap items-center justify-between gap-2 bg-[var(--mw-canvas)] px-3.5 py-2 border-b border-[var(--mw-border)]">
                  <div className="text-xs font-bold text-[var(--mw-ink)]">
                    1. Có thể tự động sửa an toàn ({actionGroups.groupA.length})
                  </div>
                  {actionGroups.groupA.length > 0 && (
                    <label className="inline-flex items-center gap-1.5 text-xs font-semibold cursor-pointer text-[var(--mw-ink)]">
                      <input
                        type="checkbox"
                        checked={applyAllGroupA}
                        onChange={(e) => {
                          setApplyAllGroupA(e.target.checked);
                          if (e.target.checked) setSkippedIds([]);
                        }}
                      />
                      Chọn tất cả mục an toàn
                    </label>
                  )}
                </div>
                {actionGroups.groupA.length === 0 ? (
                  <p className="p-3 text-xs text-[var(--mw-muted)]">
                    Không có mục sửa tự động nào thuộc nhóm này.
                  </p>
                ) : (
                  <div className="divide-y divide-[var(--mw-border)] max-h-60 overflow-y-auto">
                    {actionGroups.groupA.map((f) => {
                      const checked = isFindingSelectedForFix(f);
                      return (
                        <label
                          key={f.findingId}
                          className="flex items-start gap-2.5 px-3.5 py-2.5 text-xs cursor-pointer hover:bg-[var(--mw-canvas)]"
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggleFindingFixSelection(f)}
                            className="mt-0.5"
                          />
                          <div className="min-w-0 flex-1 space-y-0.5">
                            <div className="font-bold text-[var(--mw-ink)]">
                              {f.ruleName}
                              {f.location ? (
                                <span className="ml-1.5 font-normal text-[11px] text-[var(--mw-muted)]">
                                  ({f.location})
                                </span>
                              ) : null}
                            </div>
                            <div className="text-[11px] text-[var(--mw-muted)]">
                              {f.currentValue} → <strong className="text-[var(--mw-ink)]">{f.expectedValue}</strong>
                            </div>
                          </div>
                        </label>
                      );
                    })}
                  </div>
                )}
              </div>

              {/* 2. Cần người dùng xác nhận */}
              {actionGroups.groupB.length > 0 && (
                <div className="border border-[var(--mw-border)] overflow-hidden">
                  <div className="bg-[var(--mw-canvas)] px-3.5 py-2 border-b border-[var(--mw-border)]">
                    <div className="text-xs font-bold text-[var(--mw-ink)]">
                      2. Cần người dùng xác nhận ({actionGroups.groupB.length})
                    </div>
                  </div>
                  <div className="divide-y divide-[var(--mw-border)] max-h-48 overflow-y-auto">
                    {actionGroups.groupB.map((f) => {
                      const checked = isFindingSelectedForFix(f);
                      return (
                        <label
                          key={f.findingId}
                          className="flex items-start gap-2.5 px-3.5 py-2.5 text-xs cursor-pointer hover:bg-[var(--mw-canvas)]"
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggleFindingFixSelection(f)}
                            className="mt-0.5"
                          />
                          <div className="min-w-0 flex-1 space-y-0.5">
                            <div className="font-bold text-[var(--mw-ink)]">{f.ruleName}</div>
                            <div className="text-[11px] text-[var(--mw-muted)]">{f.proposal}</div>
                          </div>
                        </label>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* 3. Không tự động sửa */}
              <div className="rounded-xl border border-[var(--mw-border)] bg-[var(--mw-canvas)] p-3.5 text-xs space-y-1.5">
                <div className="font-bold text-[var(--mw-ink)]">
                  3. Không tự động sửa, cần kiểm tra thủ công ({actionGroups.groupC.length})
                </div>
                <p className="text-[11px] text-[var(--mw-muted)]">
                  MonWord cam kết <strong>không tự động thay đổi</strong> số liệu, ngày tháng, nội dung nghiệp vụ, tên người ký hay chức danh trong văn bản.
                </p>
                {actionGroups.groupC.length > 0 && (
                  <ul className="list-disc pl-5 text-[11px] text-[var(--mw-muted)] space-y-0.5">
                    {actionGroups.groupC.map((f) => (
                      <li key={f.findingId}>
                        <strong className="text-[var(--mw-ink)]">{f.ruleName}:</strong> {f.proposal}
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {/* Cách lưu đầu ra */}
              <div className="rounded-xl border border-[var(--mw-border)] p-3.5 text-xs space-y-2">
                <div className="font-bold text-[var(--mw-ink)]">Cách lưu kết quả</div>
                <div className="flex flex-wrap gap-4">
                  <label className="inline-flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="stdOutputMode"
                      checked={outputMode === 'newCopy'}
                      onChange={() => {
                        setOutputMode('newCopy');
                        onChangeOutputModeConfig?.('newCopy');
                      }}
                    />
                    <span>Lưu thành tệp mới (an toàn, giữ nguyên tệp gốc)</span>
                  </label>
                  <label className="inline-flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="stdOutputMode"
                      checked={outputMode === 'overwriteWithBackup'}
                      onChange={() => {
                        setOutputMode('overwriteWithBackup');
                        onChangeOutputModeConfig?.('overwriteWithBackup');
                      }}
                    />
                    <span>Ghi đè tệp gốc (có tạo bản sao lưu)</span>
                  </label>
                </div>
              </div>

              {execError && (
                <div role="alert" className="mw-toast mw-toast-err text-xs">
                  <AlertCircle className="size-4 shrink-0" aria-hidden="true" />
                  <span>{execError}</span>
                </div>
              )}

              <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-[var(--mw-border)]">
                <button
                  type="button"
                  disabled={executing}
                  onClick={() => setStep(3)}
                  className="mw-button mw-button-secondary"
                >
                  <ArrowLeft className="size-4" aria-hidden="true" />
                  Quay lại bước trước
                </button>
                <div className="flex items-center gap-3">
                  <span className="text-xs text-[var(--mw-muted)]">
                    Sẽ áp dụng <strong>{previewFixCount}</strong> thay đổi đã chọn
                  </span>
                  <button
                    type="button"
                    disabled={executing}
                    onClick={() => void runExecute()}
                    className="mw-button mw-button-primary px-5 py-2 text-sm"
                  >
                    {executing ? (
                      <>
                        <RefreshCw className="size-4 animate-spin" aria-hidden="true" />
                        Đang thực hiện chuẩn hóa...
                      </>
                    ) : (
                      <>
                        <Play className="size-4 fill-current" aria-hidden="true" />
                        Thực hiện chuẩn hóa
                      </>
                    )}
                  </button>
                </div>
              </div>
            </section>
          )}

          {/* BƯỚC 5 – XEM TRƯỚC VÀ LƯU TÀI LIỆU */}
          {step === 5 && execResult && execResult.success && (
            <section className="space-y-3.5">
              <div className="mw-panel p-4 space-y-3">
                <div className="border border-[var(--mw-border)] bg-[var(--mw-canvas)] px-3.5 py-2.5 flex flex-wrap items-center justify-between gap-3">
                  <div className="flex items-center gap-2.5 min-w-0">
                    <CheckCircle2 className="size-4 text-[var(--mw-success)] shrink-0" aria-hidden="true" />
                    <div className="min-w-0 text-xs">
                      <span className="font-bold text-[var(--mw-ink)]">
                        Đã chuẩn hóa và xác minh tệp đầu ra thành công
                      </span>
                      <span className="mx-1.5 text-[var(--mw-muted)]">·</span>
                      <span className="text-[var(--mw-muted)]">
                        Tệp kết quả: <strong className="text-[var(--mw-ink)]">{verifiedOutputName}</strong>
                      </span>
                    </div>
                  </div>
                  {canRestore && (
                    <button
                      type="button"
                      onClick={() => void handleRestore()}
                      className="mw-button mw-button-secondary text-xs"
                    >
                      <RotateCcw className="size-3.5" aria-hidden="true" />
                      Hoàn tác về bản gốc
                    </button>
                  )}
                </div>

                {/* Các nút cuối cùng */}
                <div className="flex flex-wrap items-center gap-2">
                  {!isNative && execResult.blobUrl && execResult.outputName ? (
                    <>
                      <button
                        type="button"
                        onClick={() =>
                          triggerBrowserDownload(execResult.blobUrl!, execResult.outputName!)
                        }
                        className="mw-button mw-button-primary"
                      >
                        <Save className="size-4" aria-hidden="true" />
                        Lưu tài liệu
                      </button>
                      <button
                        type="button"
                        onClick={() => setSaveAsOpen((v) => !v)}
                        className="mw-button mw-button-secondary"
                      >
                        <Download className="size-4" aria-hidden="true" />
                        Lưu thành file khác
                      </button>
                      {execResult.outputBlob && (
                        <button
                          type="button"
                          onClick={() =>
                            void shareOutputBlob(
                              execResult.outputBlob!,
                              execResult.outputName!,
                              execResult.blobUrl,
                            )
                          }
                          className="mw-button mw-button-secondary"
                        >
                          <Share2 className="size-4" aria-hidden="true" />
                          Chia sẻ
                        </button>
                      )}
                    </>
                  ) : (
                    isNative &&
                    execResult.outputPath && (
                      <>
                        <button
                          type="button"
                          onClick={() =>
                            void bridge
                              .invoke('open_path', { path: execResult.outputPath, reveal: false })
                              .catch((err) => onStatusToast('error', String(err)))
                          }
                          className="mw-button mw-button-primary"
                        >
                          <ExternalLink className="size-4" aria-hidden="true" />
                          Lưu tài liệu
                        </button>
                        <button
                          type="button"
                          onClick={() => {
                            void (async () => {
                              try {
                                const defaultName =
                                  execResult.outputPath!.split(/[/\\]/).pop() ||
                                  'VanBan_ChuanHoa.docx';
                                const picked = await bridge.invoke<{ paths: string[] }>(
                                  'pick_save_docx',
                                  { defaultName },
                                );
                                const destPath = picked.paths?.[0];
                                if (!destPath) return;
                                const saved = await bridge.invoke<{ outputPath: string }>(
                                  'save_output_as',
                                  {
                                    sourcePath: execResult.outputPath,
                                    destPath,
                                  },
                                );
                                onStatusToast(
                                  'success',
                                  `Đã lưu tài liệu thành tệp mới: ${saved.outputPath}`,
                                );
                              } catch (err) {
                                onStatusToast('error', String(err));
                              }
                            })();
                          }}
                          className="mw-button mw-button-secondary"
                        >
                          <Download className="size-4" aria-hidden="true" />
                          Lưu thành file khác
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void bridge
                              .invoke('open_path', { path: execResult.outputPath, reveal: true })
                              .catch((err) => onStatusToast('error', String(err)))
                          }
                          className="mw-button mw-button-secondary"
                        >
                          <FolderOpen className="size-4" aria-hidden="true" />
                          Mở thư mục chứa file
                        </button>
                      </>
                    )
                  )}

                  {onNavigateHome && (
                    <button
                      type="button"
                      onClick={onNavigateHome}
                      className="mw-button mw-button-secondary"
                    >
                      <Home className="size-4" aria-hidden="true" />
                      Quay lại trang chủ
                    </button>
                  )}

                  <button
                    type="button"
                    onClick={() => setStep(4)}
                    className="mw-button mw-button-secondary"
                  >
                    <ArrowLeft className="size-4" aria-hidden="true" />
                    Quay lại xác nhận
                  </button>
                </div>

                {saveAsOpen && !isNative && execResult.blobUrl && (
                  <div className="rounded-xl border border-[var(--mw-border)] bg-[var(--mw-canvas)] p-3 flex flex-wrap items-center gap-2 text-xs">
                    <label className="font-semibold text-[var(--mw-ink)]" htmlFor="std-save-as-input">
                      Tên file mới:
                    </label>
                    <input
                      id="std-save-as-input"
                      type="text"
                      value={saveAsName}
                      onChange={(e) => setSaveAsName(e.target.value)}
                      className="flex-1 min-w-[220px] rounded-lg border border-[var(--mw-border)] px-2.5 py-1.5 text-xs"
                    />
                    <button
                      type="button"
                      onClick={() => {
                        const targetName = saveAsName.trim().endsWith('.docx') || saveAsName.trim().endsWith('.docm')
                          ? saveAsName.trim()
                          : `${saveAsName.trim() || 'VanBan_ChuanHoa'}.docx`;
                        triggerBrowserDownload(execResult.blobUrl!, targetName);
                        onStatusToast('success', `Đã lưu tệp "${targetName}".`);
                        setSaveAsOpen(false);
                      }}
                      className="mw-button mw-button-primary text-xs"
                    >
                      Tải xuống file này
                    </button>
                  </div>
                )}
              </div>

              {/* Bản xem trước thực tế Trước / Sau bằng LivePreview Engine */}
              {activeFile && renderEmbeddedPreview ? (
                <div className="mw-panel p-4 space-y-2">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <h4 className="text-sm font-bold text-[var(--mw-ink)] flex items-center gap-2">
                      <Eye className="size-4 text-[var(--mw-accent)]" aria-hidden="true" />
                      Xem trước trang A4 thực tế (Trước &amp; Sau chuẩn hóa)
                    </h4>
                    {onInspectInPreview && (
                      <button
                        type="button"
                        onClick={() => onInspectInPreview(activeFile.path, '')}
                        className="mw-button mw-button-secondary text-xs"
                      >
                        Mở toàn màn hình
                      </button>
                    )}
                  </div>
                  {renderEmbeddedPreview(activeFile.path)}
                </div>
              ) : (
                activeFile &&
                onInspectInPreview && (
                  <div className="mw-panel p-4 flex items-center justify-between gap-3 text-xs">
                    <span>Mở bản xem trước trang A4 thực tế để kiểm tra trước và sau khi chuẩn hóa.</span>
                    <button
                      type="button"
                      onClick={() => onInspectInPreview(activeFile.path, '')}
                      className="mw-button mw-button-secondary"
                    >
                      <Eye className="size-4" aria-hidden="true" />
                      Xem trước Trước / Sau
                    </button>
                  </div>
                )
              )}

              {/* Chi tiết kỹ thuật 10 chiều chỉ hiện ở chế độ ?admin=1 */}
              {execResult.diff && adminMode && (
                <div className="mw-panel p-4 space-y-3">
                  <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                    <span className="font-bold flex items-center gap-1.5">
                      <GitCompare className="size-4 text-emerald-600" />
                      Chi tiết đối chiếu 10 chiều (?admin=1: {filteredDiffs.length}/{execResult.diff.entries.length})
                    </span>
                    <select
                      value={diffFilter}
                      onChange={(e) => setDiffFilter(e.target.value as 'all' | DiffDimension)}
                      className="px-2.5 py-1 rounded-lg border border-[var(--mw-border)] text-[11px]"
                    >
                      <option value="all">Tất cả 10 chiều so sánh</option>
                      {Object.entries(DIFF_DIM_LABELS).map(([k, lbl]) => (
                        <option key={k} value={k}>
                          {lbl} ({execResult.diff?.dimensionCounts[k] ?? 0})
                        </option>
                      ))}
                    </select>
                  </div>
                  <div className="max-h-56 overflow-y-auto space-y-1.5 text-[11px]">
                    {filteredDiffs.map((d, i) => (
                      <div key={i} className="p-2 rounded-lg border border-[var(--mw-border)]">
                        <div className="font-semibold">
                          [{DIFF_DIM_LABELS[d.dimension]}] {d.location} ({d.ruleId})
                        </div>
                        <div>
                          <span className="line-through text-rose-600 mr-2">{d.beforeVal}</span>→{' '}
                          <span className="text-emerald-700 font-medium">{d.afterVal}</span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </section>
          )}
        </>
      )}
    </div>
  );
};
