import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  Download,
  ExternalLink,
  Eye,
  FileSearch,
  Filter,
  Flag,
  FolderOpen,
  GitCompare,
  HelpCircle,
  Info,
  Layers,
  MapPin,
  Play,
  RotateCcw,
  Share2,
  ShieldCheck,
  SkipForward,
  Wrench,
} from 'lucide-react';
import { bridge } from '../bridge';
import type {
  DiffDimension,
  FileQueueItem,
  FixSafetyTier,
  OrganizationProfile,
  OutputMode,
  RuleCategory,
  StdAnalysisReport,
  StdDocScope,
  StdExecutionRequest,
  StdExecutionResult,
  StdFinding,
  StdSeverity,
} from '../types';
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
  onReplaceWebFile?: (path: string, file: File) => void;
  onInspectInPreview?: (filePath: string, elementPath: string) => void;
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

const SCOPE_OPTIONS: { id: StdDocScope; label: string }[] = [
  { id: 'autoDetect', label: 'Tự động nhận diện phạm vi' },
  { id: 'adminNd30', label: 'Văn bản hành chính (NĐ 30/2020/NĐ-CP)' },
  { id: 'enterpriseInternal', label: 'Văn bản nội bộ Doanh nghiệp' },
  { id: 'partyDocument', label: 'Văn bản Đảng (36-HD/VPTW)' },
  { id: 'specializedLegal', label: 'Văn bản quy phạm / chuyên ngành' },
  { id: 'academicThesis', label: 'Luận văn / Báo cáo học thuật' },
  { id: 'freeform', label: 'Tài liệu tự do' },
];

const CATEGORY_LABELS: Record<RuleCategory, string> = {
  pageAndSection: '4.1 Trang & Section',
  fontAndEncoding: '4.2 Phông & Mã chữ',
  paragraphAndAlignment: '4.3 Đoạn & Căn chỉnh',
  headingAndStructure: '4.4 Đề mục & Cấu trúc',
  tableLayout: '4.5 Bảng biểu',
  pageNumberAndHeaderFooter: '4.6 Số trang & Header/Footer',
  capitalizationAndAbbreviation: '4.7 Viết hoa & Viết tắt',
  preservationAndIntegrity: 'Bảo toàn OpenXML',
};

const SEVERITY_META: Record<StdSeverity, { label: string; cls: string; icon: React.ReactNode }> = {
  critical: {
    label: 'Nghiêm trọng',
    cls: 'bg-rose-100 text-rose-800 dark:bg-rose-950/70 dark:text-rose-300',
    icon: <AlertCircle className="w-3.5 h-3.5 text-rose-600" />,
  },
  error: {
    label: 'Lỗi cần sửa',
    cls: 'bg-orange-100 text-orange-800 dark:bg-orange-950/70 dark:text-orange-300',
    icon: <AlertTriangle className="w-3.5 h-3.5 text-orange-600" />,
  },
  warning: {
    label: 'Cảnh báo',
    cls: 'bg-amber-100 text-amber-800 dark:bg-amber-950/70 dark:text-amber-300',
    icon: <AlertTriangle className="w-3.5 h-3.5 text-amber-600" />,
  },
  information: {
    label: 'Thông tin đề xuất',
    cls: 'bg-sky-100 text-sky-800 dark:bg-sky-950/70 dark:text-sky-300',
    icon: <Info className="w-3.5 h-3.5 text-sky-600" />,
  },
};

const TIER_META: Record<FixSafetyTier, { label: string; cls: string }> = {
  groupAAuto: {
    label: 'Nhóm A · Tự động an toàn',
    cls: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300',
  },
  groupBConfirm: {
    label: 'Nhóm B · Cần xác nhận',
    cls: 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300',
  },
  groupCManualOnly: {
    label: 'Nhóm C · Không tự sửa',
    cls: 'bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300',
  },
};

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

export const StandardizationWorkspace: React.FC<StandardizationWorkspaceProps> = ({
  files,
  onPickFiles,
  onReplaceWebFile,
  onInspectInPreview,
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();

  const wordFiles = useMemo(
    () =>
      files.filter((f) => {
        const ext = f.ext.toLowerCase();
        return ext === 'docx' || ext === 'docm' || ext === 'doc';
      }),
    [files],
  );

  const [selectedPath, setSelectedPath] = useState<string>('');
  const [scope, setScope] = useState<StdDocScope>('autoDetect');
  const [outputMode, setOutputMode] = useState<OutputMode>('newCopy');
  const [orgProfiles, setOrgProfiles] = useState<OrganizationProfile[]>([]);
  const [orgProfileId, setOrgProfileId] = useState<string>('');

  const [applyAllGroupA, setApplyAllGroupA] = useState<boolean>(true);
  const [approvedIds, setApprovedIds] = useState<string[]>([]);
  const [skippedIds, setSkippedIds] = useState<string[]>([]);
  const [manualReviewIds, setManualReviewIds] = useState<string[]>([]);

  const [sevFilter, setSevFilter] = useState<'all' | StdSeverity>('all');
  const [catFilter, setCatFilter] = useState<'all' | RuleCategory>('all');
  const [diffFilter, setDiffFilter] = useState<'all' | DiffDimension>('all');

  const [analyzing, setAnalyzing] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [report, setReport] = useState<StdAnalysisReport | null>(null);
  const [execResult, setExecResult] = useState<StdExecutionResult | null>(null);
  const [highlightedPath, setHighlightedPath] = useState<string | null>(null);
  const [backupByPath, setBackupByPath] = useState<Record<string, string>>({});

  const inspectorRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let alive = true;
    const loadOrgs = async () => {
      if (isNative) {
        try {
          const list = await bridge.invoke<OrganizationProfile[]>('list_org_profiles');
          if (alive) setOrgProfiles(Array.isArray(list) ? list : []);
        } catch {
          // ignore
        }
      } else {
        setOrgProfiles(listWebOrgProfiles());
      }
    };
    void loadOrgs();
    return () => {
      alive = false;
    };
  }, [isNative]);

  useEffect(() => {
    if (wordFiles.length === 0) {
      setSelectedPath('');
      setReport(null);
      return;
    }
    if (!wordFiles.some((f) => f.path === selectedPath)) {
      setSelectedPath(wordFiles[0].path);
      setApprovedIds([]);
      setSkippedIds([]);
      setManualReviewIds([]);
      setExecResult(null);
    }
  }, [wordFiles, selectedPath]);

  const activeFile = useMemo(
    () => wordFiles.find((f) => f.path === selectedPath) ?? null,
    [wordFiles, selectedPath],
  );

  const activeOrg = useMemo(
    () => orgProfiles.find((p) => p.id === orgProfileId) ?? null,
    [orgProfiles, orgProfileId],
  );

  const runAnalysis = useCallback(async () => {
    if (!activeFile) return;
    setAnalyzing(true);
    setHighlightedPath(null);
    try {
      const rep = isNative
        ? await bridge.invoke<StdAnalysisReport>('analyze_std_document', {
            path: activeFile.path,
            scope,
            orgProfile: activeOrg,
          })
        : await analyzeWebStdDocument(activeFile, scope, activeOrg);
      setReport(rep);
      setApprovedIds([]);
      setSkippedIds([]);
      setManualReviewIds([]);
      if (rep.requiresScopeSelection) {
        onStatusToast('info', rep.scopeReason);
      }
    } catch (err) {
      onStatusToast('error', `Lỗi phân tích tài liệu: ${(err as Error).message}`);
    } finally {
      setAnalyzing(false);
    }
  }, [activeFile, scope, activeOrg, isNative, onStatusToast]);

  useEffect(() => {
    if (activeFile) {
      void runAnalysis();
    }
  }, [activeFile?.path, scope, orgProfileId]);

  const filteredFindings = useMemo(() => {
    if (!report) return [];
    return report.findings.filter((f) => {
      if (sevFilter !== 'all' && f.severity !== sevFilter) return false;
      if (catFilter !== 'all' && f.category !== catFilter) return false;
      return true;
    });
  }, [report, sevFilter, catFilter]);

  const previewFixCount = useMemo(() => {
    if (!report) return 0;
    const sk = new Set(skippedIds);
    const man = new Set(manualReviewIds);
    const app = new Set(approvedIds);
    return report.findings.filter((f) => {
      if (!f.autoFixSupported || sk.has(f.findingId) || man.has(f.findingId)) return false;
      if (f.safetyTier === 'groupAAuto') return applyAllGroupA || app.has(f.findingId);
      if (f.safetyTier === 'groupBConfirm') return app.has(f.findingId);
      return false;
    }).length;
  }, [report, applyAllGroupA, approvedIds, skippedIds, manualReviewIds]);

  const toggleApproved = (id: string) => {
    setSkippedIds((prev) => prev.filter((x) => x !== id));
    setManualReviewIds((prev) => prev.filter((x) => x !== id));
    setApprovedIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  };

  const toggleSkipped = (id: string) => {
    setApprovedIds((prev) => prev.filter((x) => x !== id));
    setSkippedIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  };

  const toggleManualReview = (id: string) => {
    setApprovedIds((prev) => prev.filter((x) => x !== id));
    setManualReviewIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  };

  const jumpToLocation = (elementPath: string) => {
    setHighlightedPath(elementPath);
    inspectorRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  };

  const runExecute = async (singleFinding?: StdFinding) => {
    if (!activeFile) {
      onStatusToast('error', 'Vui lòng chọn tệp tài liệu trước.');
      return;
    }
    const req: StdExecutionRequest = singleFinding
      ? {
          path: activeFile.path,
          scope,
          orgProfile: activeOrg,
          outputMode,
          applyAllGroupA: false,
          approvedFindingIds: [singleFinding.findingId],
          skippedFindingIds: [],
          manualReviewFindingIds: [],
          disabledRuleIds: [],
        }
      : {
          path: activeFile.path,
          scope,
          orgProfile: activeOrg,
          outputMode,
          applyAllGroupA,
          approvedFindingIds: approvedIds,
          skippedFindingIds: skippedIds,
          manualReviewFindingIds: manualReviewIds,
          disabledRuleIds: [],
        };

    setExecuting(true);
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

      if (res.success) {
        if (!isNative && res.outputBlob && onReplaceWebFile) {
          const updated = new File([res.outputBlob], activeFile.name, {
            type: res.outputBlob.type,
          });
          onReplaceWebFile(activeFile.path, updated);
        }
        onStatusToast(
          'success',
          `Đã chuẩn hóa & xác minh lại tệp (${res.actionsApplied.length} thay đổi, bảo toàn 100%).`,
        );
      } else {
        onStatusToast('error', res.error || 'Chuẩn hóa không hoàn tất.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi chuẩn hóa: ${(err as Error).message}`);
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
        await runAnalysis();
      } else {
        const snap = getWebStdSnapshot(activeFile.path);
        if (!snap || !onReplaceWebFile) {
          onStatusToast('error', 'Không tìm thấy bản gốc trong bộ nhớ trình duyệt.');
          return;
        }
        onReplaceWebFile(activeFile.path, snap);
        const restoredRep = await analyzeWebStdDocument(
          { ...activeFile, webFile: snap },
          scope,
          activeOrg,
        );
        setReport(restoredRep);
        onStatusToast('info', `Đã hoàn tác về bản gốc "${activeFile.name}".`);
      }
      setExecResult(null);
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

  return (
    <div className="space-y-4">
      {/* BƯỚC 1-3: CHỌN TỆP, PHẠM VI & PHÂN TÍCH */}
      <section className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 className="text-sm sm:text-base font-bold flex items-center gap-2">
              <ShieldCheck className="w-4 h-4 text-blue-600" />
              Bộ máy Phân tích &amp; Chuẩn hóa Thể thức (Phase 3)
            </h2>
            <p className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
              Phân tích 8 nhóm quy tắc NĐ 30/2020/NĐ-CP · Sửa theo 3 cấp an toàn A/B/C · So sánh Diff &amp; Kiểm tra lại sau lưu.
            </p>
          </div>
          {wordFiles.length === 0 && (
            <button
              type="button"
              onClick={onPickFiles}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-blue-600 text-white hover:bg-blue-700 cursor-pointer"
            >
              <FolderOpen className="w-3.5 h-3.5" />
              Chọn tệp Word
            </button>
          )}
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-2.5 text-xs">
          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Bước 1: Tài liệu Word
            </span>
            <select
              value={selectedPath}
              onChange={(e) => setSelectedPath(e.target.value)}
              className="mt-1 w-full px-2.5 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 font-medium"
            >
              {wordFiles.length === 0 && <option value="">-- Chưa có tệp trong hàng đợi --</option>}
              {wordFiles.map((f) => (
                <option key={f.path} value={f.path}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>

          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Bước 2: Phạm vi áp dụng quy tắc
            </span>
            <select
              value={scope}
              onChange={(e) => setScope(e.target.value as StdDocScope)}
              className="mt-1 w-full px-2.5 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 font-medium"
            >
              {SCOPE_OPTIONS.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>

          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Hồ sơ Tổ chức (Phase 1)
            </span>
            <select
              value={orgProfileId}
              onChange={(e) => setOrgProfileId(e.target.value)}
              className="mt-1 w-full px-2.5 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950"
            >
              <option value="">-- Không áp hồ sơ riêng --</option>
              {orgProfiles.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.profileName} ({o.sector})
                </option>
              ))}
            </select>
          </label>

          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Chế độ ghi kết quả
            </span>
            <select
              value={outputMode}
              onChange={(e) => setOutputMode(e.target.value as OutputMode)}
              className="mt-1 w-full px-2.5 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950"
            >
              <option value="newCopy">Bản sao mới (_ChuanTheThuc)</option>
              <option value="overwriteWithBackup">Ghi đè (tự sao lưu .bak để hoàn tác)</option>
            </select>
          </label>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
          <button
            type="button"
            disabled={!activeFile || analyzing}
            onClick={() => void runAnalysis()}
            className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl text-xs font-bold bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 hover:opacity-90 disabled:opacity-45 cursor-pointer"
          >
            <FileSearch className="w-4 h-4" />
            {analyzing ? 'Đang phân tích cấu trúc OpenXML...' : 'Bước 3: Phân tích tài liệu'}
          </button>

          {report && (
            <div className="text-[11px] text-slate-600 dark:text-slate-300">
              Phạm vi hiệu lực:{' '}
              <strong className="text-blue-600 dark:text-blue-400">
                {SCOPE_OPTIONS.find((o) => o.id === report.effectiveScope)?.label ||
                  'Chưa xác định (chọn thủ công)'}
              </strong>{' '}
              — {report.scopeReason}
            </div>
          )}
        </div>

        {report?.requiresScopeSelection && (
          <div className="p-3 rounded-xl border border-violet-300 bg-violet-50/70 dark:border-violet-800 dark:bg-violet-950/30 text-xs flex items-start gap-2">
            <HelpCircle className="w-4 h-4 text-violet-600 shrink-0 mt-0.5" />
            <div>
              <strong>Không thể tự suy đoán phạm vi áp dụng:</strong> {report.scopeReason} Hãy chọn loại tài liệu phù hợp ở mục <em>Bước 2</em> rồi bấm <em>Phân tích tài liệu</em>.
            </div>
          </div>
        )}
      </section>

      {/* BƯỚC 4: BÁO CÁO PHÂN TÍCH ĐỊNH LƯỢNG THỰC TẾ (KHÔNG DÙNG ĐIỂM PHẦN TRĂM CẢM TÍNH) */}
      {report && (
        <section className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="text-sm font-bold flex items-center gap-2">
              <Layers className="w-4 h-4 text-blue-600" />
              Bước 4: Kết quả kiểm tra quy tắc &amp; Kiểm kê tài sản bảo toàn
            </h3>
            <span className="text-[11px] text-slate-500">
              {report.paragraphCount} đoạn · {report.tables.length} bảng · {report.sections.length} section
            </span>
          </div>

          {/* Trạng thái quy tắc */}
          <div className="grid grid-cols-2 sm:grid-cols-5 gap-2 text-center text-xs">
            <div className="p-2 rounded-xl border border-emerald-200 dark:border-emerald-900/60 bg-emerald-50/60 dark:bg-emerald-950/20">
              <div className="text-base font-extrabold text-emerald-700 dark:text-emerald-300">
                {report.statusCounts.passed}
              </div>
              <div className="text-[10px] font-semibold text-emerald-700/80 dark:text-emerald-300/80">
                Quy tắc Đạt
              </div>
            </div>
            <div className="p-2 rounded-xl border border-rose-200 dark:border-rose-900/60 bg-rose-50/60 dark:bg-rose-950/20">
              <div className="text-base font-extrabold text-rose-700 dark:text-rose-300">
                {report.statusCounts.failed}
              </div>
              <div className="text-[10px] font-semibold text-rose-700/80 dark:text-rose-300/80">
                Không đạt
              </div>
            </div>
            <div className="p-2 rounded-xl border border-amber-200 dark:border-amber-900/60 bg-amber-50/60 dark:bg-amber-950/20">
              <div className="text-base font-extrabold text-amber-700 dark:text-amber-300">
                {report.statusCounts.needsReview}
              </div>
              <div className="text-[10px] font-semibold text-amber-700/80 dark:text-amber-300/80">
                Cần kiểm tra
              </div>
            </div>
            <div className="p-2 rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/40">
              <div className="text-base font-extrabold text-slate-600 dark:text-slate-300">
                {report.statusCounts.notApplicable}
              </div>
              <div className="text-[10px] font-semibold text-slate-500">Không áp dụng</div>
            </div>
            <div className="p-2 rounded-xl border border-violet-200 dark:border-violet-900/60 bg-violet-50/50 dark:bg-violet-950/20">
              <div className="text-base font-extrabold text-violet-700 dark:text-violet-300">
                {report.statusCounts.indeterminate}
              </div>
              <div className="text-[10px] font-semibold text-violet-700/80 dark:text-violet-300/80">
                Không xác định
              </div>
            </div>
          </div>

          {/* Kiểm kê bảo toàn đối tượng */}
          <div className="flex flex-wrap gap-1.5 text-[11px]">
            <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
              Ảnh/Logo: <strong>{report.assets.drawingCount}</strong> ({report.assets.mediaCount} media)
            </span>
            <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
              Hyperlink: <strong>{report.assets.hyperlinkCount}</strong>
            </span>
            <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
              Bookmark: <strong>{report.assets.bookmarkCount}</strong>
            </span>
            <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
              Footnote/Endnote: <strong>{report.assets.footnoteCount}/{report.assets.endnoteCount}</strong>
            </span>
            <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
              Comment: <strong>{report.assets.commentCount}</strong>
            </span>
            <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800">
              Field tự động: <strong>{report.assets.fieldCount}</strong>
            </span>
            {report.assets.hasVbaMacros && (
              <span className="px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-300 font-semibold">
                Có Macro VBA (.docm) — Được bảo toàn
              </span>
            )}
          </div>

          {/* Bộ lọc lỗi & thanh chọn thao tác */}
          <div className="flex flex-wrap items-center justify-between gap-2 pt-1 border-t border-slate-200 dark:border-slate-800 text-xs">
            <div className="flex flex-wrap items-center gap-2">
              <span className="inline-flex items-center gap-1 font-semibold text-slate-500">
                <Filter className="w-3.5 h-3.5" /> Lọc:
              </span>
              <select
                value={sevFilter}
                onChange={(e) => setSevFilter(e.target.value as 'all' | StdSeverity)}
                className="px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-[11px]"
              >
                <option value="all">Tất cả mức độ ({report.findings.length})</option>
                <option value="critical">Nghiêm trọng ({report.severityCounts.critical})</option>
                <option value="error">Lỗi cần sửa ({report.severityCounts.error})</option>
                <option value="warning">Cảnh báo ({report.severityCounts.warning})</option>
                <option value="information">Đề xuất ({report.severityCounts.information})</option>
              </select>

              <select
                value={catFilter}
                onChange={(e) => setCatFilter(e.target.value as 'all' | RuleCategory)}
                className="px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-[11px]"
              >
                <option value="all">Tất cả nhóm quy tắc</option>
                {Object.entries(CATEGORY_LABELS).map(([k, lbl]) => (
                  <option key={k} value={k}>
                    {lbl}
                  </option>
                ))}
              </select>
            </div>

            <label className="inline-flex items-center gap-1.5 text-[11px] font-semibold cursor-pointer">
              <input
                type="checkbox"
                checked={applyAllGroupA}
                onChange={(e) => setApplyAllGroupA(e.target.checked)}
              />
              Tự động sửa toàn bộ Nhóm A an toàn
            </label>
          </div>

          {/* Danh sách lỗi chi tiết */}
          <div className="space-y-2 max-h-[420px] overflow-y-auto pr-1">
            {filteredFindings.length === 0 ? (
              <div className="p-4 rounded-xl bg-emerald-50/70 dark:bg-emerald-950/20 border border-emerald-200 dark:border-emerald-900 text-xs text-emerald-700 dark:text-emerald-300 flex items-center gap-2">
                <CheckCircle2 className="w-4 h-4 shrink-0" />
                Không có lỗi nào thuộc bộ lọc hiện tại.
              </div>
            ) : (
              filteredFindings.map((f) => {
                const isSkipped = skippedIds.includes(f.findingId);
                const isManual = manualReviewIds.includes(f.findingId);
                const isChecked =
                  f.autoFixSupported &&
                  !isSkipped &&
                  !isManual &&
                  ((f.safetyTier === 'groupAAuto' && applyAllGroupA) ||
                    approvedIds.includes(f.findingId));

                return (
                  <div
                    key={f.findingId}
                    className={`p-3 rounded-xl border text-xs space-y-1.5 transition-colors ${
                      isSkipped
                        ? 'opacity-55 border-slate-200 dark:border-slate-800 bg-slate-50/40 dark:bg-slate-950/20'
                        : 'border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-950/40'
                    }`}
                  >
                    <div className="flex flex-wrap items-center justify-between gap-1.5">
                      <div className="flex flex-wrap items-center gap-1.5">
                        {f.autoFixSupported && (
                          <input
                            type="checkbox"
                            checked={isChecked}
                            onChange={() => toggleApproved(f.findingId)}
                            title={
                              f.safetyTier === 'groupBConfirm'
                                ? 'Bắt buộc tích xác nhận để sửa Nhóm B'
                                : 'Chọn sửa lỗi này'
                            }
                          />
                        )}
                        <span className="font-mono font-bold text-[10px] px-1.5 py-0.5 rounded bg-slate-200 dark:bg-slate-800">
                          {f.ruleId}
                        </span>
                        <span
                          className={`inline-flex items-center gap-1 text-[10px] font-bold px-2 py-0.5 rounded-full ${SEVERITY_META[f.severity].cls}`}
                        >
                          {SEVERITY_META[f.severity].icon}
                          {SEVERITY_META[f.severity].label}
                        </span>
                        <span
                          className={`text-[10px] font-semibold px-2 py-0.5 rounded-full ${TIER_META[f.safetyTier].cls}`}
                        >
                          {TIER_META[f.safetyTier].label}
                        </span>
                        <span className="font-bold text-slate-900 dark:text-slate-100">
                          {f.ruleName}
                        </span>
                      </div>
                      <span className="text-[10px] font-mono text-slate-500">{f.location}</span>
                    </div>

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-[11px] bg-white/80 dark:bg-slate-900/70 p-2 rounded-lg border border-slate-200/70 dark:border-slate-800">
                      <div>
                        <span className="text-slate-500 font-semibold">Hiện tại: </span>
                        <span className="text-rose-700 dark:text-rose-300">{f.currentValue}</span>
                      </div>
                      <div>
                        <span className="text-slate-500 font-semibold">Yêu cầu: </span>
                        <span className="text-emerald-700 dark:text-emerald-300">
                          {f.expectedValue}
                        </span>
                      </div>
                    </div>

                    <div className="text-[11px] text-slate-700 dark:text-slate-300">
                      <strong>Đề xuất:</strong> {f.proposal}{' '}
                      <span className="text-slate-500">({f.legalBasis})</span>
                    </div>

                    <div className="flex flex-wrap items-center gap-1.5 pt-0.5 text-[11px]">
                      <button
                        type="button"
                        onClick={() => jumpToLocation(f.elementPath)}
                        className="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer"
                      >
                        <MapPin className="w-3 h-3 text-blue-600" />
                        Chuyển đến vị trí
                      </button>

                      {onInspectInPreview && activeFile && (
                        <button
                          type="button"
                          onClick={() => onInspectInPreview(activeFile.path, f.elementPath)}
                          className="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-blue-200 dark:border-blue-800 bg-blue-50/70 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300 hover:bg-blue-100 cursor-pointer font-semibold"
                        >
                          <Eye className="w-3 h-3" />
                          Xem trên A4
                        </button>
                      )}

                      {f.autoFixSupported && f.safetyTier !== 'groupCManualOnly' && (
                        <button
                          type="button"
                          disabled={executing}
                          onClick={() => void runExecute(f)}
                          className="inline-flex items-center gap-1 px-2 py-1 rounded-lg bg-blue-600 text-white hover:bg-blue-700 font-semibold cursor-pointer disabled:opacity-45"
                        >
                          <Wrench className="w-3 h-3" />
                          Sửa lỗi này
                        </button>
                      )}

                      <button
                        type="button"
                        onClick={() => toggleSkipped(f.findingId)}
                        className={`inline-flex items-center gap-1 px-2 py-1 rounded-lg border cursor-pointer ${
                          isSkipped
                            ? 'border-slate-400 bg-slate-200 dark:bg-slate-700 font-semibold'
                            : 'border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800'
                        }`}
                      >
                        <SkipForward className="w-3 h-3" />
                        {isSkipped ? 'Đã bỏ qua' : 'Bỏ qua'}
                      </button>

                      <button
                        type="button"
                        onClick={() => toggleManualReview(f.findingId)}
                        className={`inline-flex items-center gap-1 px-2 py-1 rounded-lg border cursor-pointer ${
                          isManual
                            ? 'border-violet-400 bg-violet-100 text-violet-800 dark:bg-violet-900/50 dark:text-violet-200 font-semibold'
                            : 'border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800'
                        }`}
                      >
                        <Flag className="w-3 h-3" />
                        {isManual ? 'Cần kiểm tra thủ công' : 'Đánh dấu kiểm tra'}
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </div>

          {/* Trình soi vị trí cấu trúc tài liệu */}
          <div
            ref={inspectorRef}
            className="rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-950/50 p-3 space-y-2"
          >
            <div className="flex items-center justify-between text-xs font-bold">
              <span className="flex items-center gap-1.5">
                <Eye className="w-3.5 h-3.5 text-blue-600" />
                Vị trí trong tài liệu gốc ({report.paragraphPreviews.length}/{report.paragraphCount} đoạn trích)
              </span>
              {highlightedPath && (
                <span className="font-mono text-[10px] px-2 py-0.5 rounded bg-blue-100 text-blue-700 dark:bg-blue-900/60 dark:text-blue-300">
                  Đang trỏ: {highlightedPath}
                </span>
              )}
            </div>
            <div className="max-h-44 overflow-y-auto space-y-1 text-[11px]">
              {report.paragraphPreviews.map((p) => {
                const isHit = highlightedPath === p.elementPath;
                return (
                  <div
                    key={p.elementPath}
                    className={`p-1.5 rounded-lg border flex items-start justify-between gap-2 ${
                      isHit
                        ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/60 ring-1 ring-blue-500'
                        : 'border-slate-200/70 dark:border-slate-800 bg-white/70 dark:bg-slate-900/40'
                    }`}
                  >
                    <div className="min-w-0">
                      <span className="font-mono text-[10px] text-slate-500 mr-1.5">
                        #{p.index + 1} [{p.role}]
                      </span>
                      <span className="text-slate-800 dark:text-slate-200 break-words">
                        {p.excerpt || '(Đoạn trống)'}
                      </span>
                    </div>
                    <div className="shrink-0 text-[10px] font-mono text-slate-500 text-right">
                      {p.fontName || '?'} · {p.fontSizePt ?? '?'}pt · {p.alignment}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* BƯỚC 6-7: XEM TRƯỚC THAY ĐỔI & THỰC THI */}
          <div className="p-3 rounded-xl border border-blue-200 dark:border-blue-900/60 bg-blue-50/60 dark:bg-blue-950/20 flex flex-wrap items-center justify-between gap-2 text-xs">
            <div>
              <strong>Bước 6 · Xem trước kế hoạch sửa:</strong> Sẽ áp dụng{' '}
              <strong className="text-blue-700 dark:text-blue-300">{previewFixCount}</strong> sửa đổi
              (Nhóm A + Nhóm B đã xác nhận); giữ nguyên toàn bộ Nhóm C (Số VB, Ngày, Chữ ký, Nội dung pháp lý).
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                disabled={executing || (report.requiresScopeSelection && scope === 'autoDetect')}
                onClick={() => void runExecute()}
                className="inline-flex items-center gap-1.5 px-4 py-2 rounded-xl text-xs font-bold bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-45 cursor-pointer"
              >
                <Play className="w-4 h-4" />
                {executing ? 'Đang chuẩn hóa & xác minh...' : 'Bước 7: Thực hiện chuẩn hóa'}
              </button>
              {canRestore && (
                <button
                  type="button"
                  disabled={executing}
                  onClick={() => void handleRestore()}
                  className="inline-flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-semibold border border-amber-300 dark:border-amber-800 text-amber-700 dark:text-amber-300 hover:bg-amber-50 dark:hover:bg-amber-950/30 cursor-pointer"
                >
                  <RotateCcw className="w-4 h-4" />
                  Hoàn tác
                </button>
              )}
            </div>
          </div>
        </section>
      )}

      {/* BƯỚC 8-9: KIỂM TRA SAU LƯU & DOCUMENT DIFF ENGINE (10 CHIỀU) */}
      {execResult && (
        <section className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="text-sm font-bold flex items-center gap-2">
              <GitCompare className="w-4 h-4 text-emerald-600" />
              Bước 8–9: Xác minh tệp sau lưu &amp; So sánh Trước / Sau (Document Diff Engine)
            </h3>

            {execResult.success && (
              <div className="flex items-center gap-2">
                {isNative ? (
                  <button
                    type="button"
                    onClick={() =>
                      void bridge.invoke('open_path', {
                        path: execResult.outputPath,
                        reveal: false,
                      })
                    }
                    className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg bg-emerald-600 text-white hover:bg-emerald-700 text-xs font-semibold cursor-pointer"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    Mở trong Word
                  </button>
                ) : (
                  execResult.blobUrl &&
                  execResult.outputName && (
                    <>
                      <button
                        type="button"
                        onClick={() =>
                          triggerBrowserDownload(execResult.blobUrl!, execResult.outputName!)
                        }
                        className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg bg-emerald-600 text-white hover:bg-emerald-700 text-xs font-semibold cursor-pointer"
                      >
                        <Download className="w-3.5 h-3.5" />
                        Xuất / Tải (.docx)
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
                          className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg border border-emerald-400 text-emerald-700 dark:text-emerald-300 text-xs font-semibold cursor-pointer"
                        >
                          <Share2 className="w-3.5 h-3.5" />
                          Chia sẻ
                        </button>
                      )}
                    </>
                  )
                )}
              </div>
            )}
          </div>

          {execResult.verification && (
            <div
              className={`p-3 rounded-xl border text-xs space-y-1 ${
                execResult.verification.preservationPassed && execResult.verification.xmlValid
                  ? 'border-emerald-200 bg-emerald-50/70 dark:border-emerald-900/60 dark:bg-emerald-950/20 text-emerald-900 dark:text-emerald-200'
                  : 'border-rose-200 bg-rose-50/70 dark:border-rose-900/60 dark:bg-rose-950/20 text-rose-900 dark:text-rose-200'
              }`}
            >
              <div className="font-bold">{execResult.verification.summaryMessage}</div>
              <div className="text-[11px] opacity-85">
                Đầu ra: <span className="font-mono">{execResult.outputName || execResult.outputPath}</span>
              </div>
            </div>
          )}

          {execResult.diff && (
            <>
              {/* Bảng đối chiếu bảo toàn */}
              <div className="grid grid-cols-2 sm:grid-cols-5 gap-2 text-[11px]">
                <div className="p-2 rounded-lg border border-slate-200 dark:border-slate-800">
                  Đoạn chữ: <strong>{execResult.diff.preservation.textParagraphsBefore}</strong> →{' '}
                  <strong>{execResult.diff.preservation.textParagraphsAfter}</strong>
                </div>
                <div className="p-2 rounded-lg border border-slate-200 dark:border-slate-800">
                  Bảng: <strong>{execResult.diff.preservation.tablesBefore}</strong> →{' '}
                  <strong>{execResult.diff.preservation.tablesAfter}</strong>
                </div>
                <div className="p-2 rounded-lg border border-slate-200 dark:border-slate-800">
                  Ảnh/Logo: <strong>{execResult.diff.preservation.drawingsBefore}</strong> →{' '}
                  <strong>{execResult.diff.preservation.drawingsAfter}</strong>
                </div>
                <div className="p-2 rounded-lg border border-slate-200 dark:border-slate-800">
                  Link/BM: <strong>{execResult.diff.preservation.hyperlinksBefore}/{execResult.diff.preservation.bookmarksBefore}</strong> →{' '}
                  <strong>{execResult.diff.preservation.hyperlinksAfter}/{execResult.diff.preservation.bookmarksAfter}</strong>
                </div>
                <div className="p-2 rounded-lg border border-slate-200 dark:border-slate-800">
                  FN/EN/CMT: <strong>{execResult.diff.preservation.footnotesBefore}/{execResult.diff.preservation.endnotesBefore}/{execResult.diff.preservation.commentsBefore}</strong> →{' '}
                  <strong>{execResult.diff.preservation.footnotesAfter}/{execResult.diff.preservation.endnotesAfter}/{execResult.diff.preservation.commentsAfter}</strong>
                </div>
              </div>

              {/* Bộ lọc Diff 10 chiều */}
              <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                <span className="font-semibold">
                  Chi tiết thay đổi trước/sau ({filteredDiffs.length}/{execResult.diff.entries.length})
                </span>
                <select
                  value={diffFilter}
                  onChange={(e) => setDiffFilter(e.target.value as 'all' | DiffDimension)}
                  className="px-2.5 py-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-[11px]"
                >
                  <option value="all">Tất cả 10 chiều so sánh</option>
                  {Object.entries(DIFF_DIM_LABELS).map(([k, lbl]) => (
                    <option key={k} value={k}>
                      {lbl} ({execResult.diff?.dimensionCounts[k] ?? 0})
                    </option>
                  ))}
                </select>
              </div>

              <div className="max-h-60 overflow-y-auto space-y-1.5 text-[11px]">
                {filteredDiffs.length === 0 ? (
                  <div className="text-slate-500 p-2">Không có thay đổi thuộc chiều đang lọc.</div>
                ) : (
                  filteredDiffs.map((d, i) => (
                    <div
                      key={i}
                      className="p-2 rounded-lg border border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-950/40"
                    >
                      <div className="flex flex-wrap items-center justify-between gap-1 font-semibold">
                        <span>
                          [{DIFF_DIM_LABELS[d.dimension]}] {d.location}
                        </span>
                        <span className="font-mono text-[10px] text-blue-600 dark:text-blue-400">
                          {d.ruleId}
                        </span>
                      </div>
                      <div className="mt-0.5 text-slate-600 dark:text-slate-400">
                        <span className="line-through text-rose-600 dark:text-rose-400 mr-2">
                          {d.beforeVal}
                        </span>
                        → <span className="text-emerald-700 dark:text-emerald-300 font-medium">{d.afterVal}</span>
                      </div>
                      <div className="text-[10px] text-slate-500 mt-0.5">Lý do: {d.reason}</div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}
        </section>
      )}
    </div>
  );
};
