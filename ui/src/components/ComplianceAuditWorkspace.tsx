import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  ClipboardCheck,
  Download,
  Eye,
  FileSpreadsheet,
  FileText,
  FolderOpen,
  GitCompare,
  History,
  Info,
  Layers,
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  ShieldAlert,
  ShieldCheck,
  TableProperties,
  Trash2,
} from 'lucide-react';
import { bridge } from '../bridge';
import { isAdminToolsEnabled, loadDefaultDocInfo } from '../utils/platform';
import type {
  AdvancedAuditIssue,
  AuditExportFormat,
  AuditExportResult,
  AuditHistoryEntry,
  AuditRunComparison,
  ComplianceAuditReport,
  ComplianceAutoFixAndRecheckResult,
  ComplianceBasisKind,
  ComplianceMatrixRow,
  ComplianceObligation,
  ComplianceRulePack,
  FileQueueItem,
  OrganizationProfile,
  OutputMode,
  StdDocScope,
  StdExecutionRequest,
  TraceableFinding,
} from '../types';
import { triggerBrowserDownload } from '../utils/webDocxEngine';
import { listWebOrgProfiles } from '../utils/webHeaderBuilder';
import {
  buildWebComplianceMatrix,
  buildWebReportCsv,
  buildWebReportDocxBlob,
  clearWebAuditHistory,
  compareWebAuditHistory,
  deleteWebAuditHistoryEntry,
  executeWebComplianceAutoFixAndRecheck,
  getWebRulePack,
  listWebAuditHistory,
  resetWebRulePack,
  runWebComplianceAudit,
  saveWebRulePack,
} from '../utils/webComplianceAudit';

export interface ComplianceAuditWorkspaceProps {
  files: FileQueueItem[];
  onPickFiles: () => void;
  onReplaceWebFile?: (path: string, file: File) => void;
  onInspectInPreview?: (filePath: string, elementPath: string) => void;
  onStartStandardize?: (filePath: string, scope: StdDocScope) => void;
  onAuditCompleted?: (file: FileQueueItem, report: ComplianceAuditReport) => void;
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

type SubTab = 'report' | 'matrix' | 'rulepack' | 'history';
type AuditGroupKey = 'formal' | 'format' | 'layout' | 'manual';

interface AuditDisplayItem {
  id: string;
  title: string;
  description: string;
  location: string;
  elementPath: string;
  suggestion: string;
  statusLabel: 'Cần chỉnh sửa' | 'Cần kiểm tra thủ công';
  adminRuleTag?: string;
}

const DOC_KIND_OPTIONS: { id: StdDocScope; title: string; hint: string }[] = [
  { id: 'adminNd30', title: 'Văn bản hành chính', hint: 'Công văn, quyết định, tờ trình, báo cáo, biên bản' },
  { id: 'partyDocument', title: 'Văn bản Đảng', hint: 'Văn bản cấp ủy, tổ chức Đảng' },
  { id: 'enterpriseInternal', title: 'Văn bản doanh nghiệp', hint: 'Quy chế, thông báo, văn bản nội bộ' },
  { id: 'academicThesis', title: 'Luận văn và tài liệu khác', hint: 'Luận văn, báo cáo chuyên môn, tài liệu tự do' },
];

const ADMIN_SCOPE_OPTIONS: { id: StdDocScope; label: string }[] = [
  { id: 'autoDetect', label: 'Tự động nhận diện phạm vi' },
  { id: 'adminNd30', label: 'Hành chính (NĐ 30/2020/NĐ-CP)' },
  { id: 'partyDocument', label: 'Văn bản Đảng (36-HD/VPTW)' },
  { id: 'enterpriseInternal', label: 'Doanh nghiệp / Nội bộ' },
  { id: 'specializedLegal', label: 'Quy phạm / Chuyên ngành' },
  { id: 'academicThesis', label: 'Luận văn / Đề án' },
  { id: 'freeform', label: 'Tài liệu tự do' },
];

const DOC_TYPE_HINTS: { id: string; label: string }[] = [
  { id: '', label: 'Tự động nhận diện từ văn bản' },
  { id: 'congVan', label: 'Công văn (không có dòng tên loại)' },
  { id: 'quyetDinh', label: 'Quyết định' },
  { id: 'toTrinh', label: 'Tờ trình' },
  { id: 'baoCao', label: 'Báo cáo' },
  { id: 'thongBao', label: 'Thông báo' },
  { id: 'keHoach', label: 'Kế hoạch' },
  { id: 'bienBan', label: 'Biên bản' },
];

const OBLIGATION_BADGE: Record<ComplianceObligation, { label: string; cls: string }> = {
  mandatory: {
    label: 'Bắt buộc',
    cls: 'bg-rose-100 text-rose-800 dark:bg-rose-950/70 dark:text-rose-300',
  },
  conditional: {
    label: 'Có điều kiện',
    cls: 'bg-amber-100 text-amber-800 dark:bg-amber-950/70 dark:text-amber-300',
  },
  recommended: {
    label: 'Khuyến nghị',
    cls: 'bg-sky-100 text-sky-800 dark:bg-sky-950/70 dark:text-sky-300',
  },
  notApplicable: {
    label: 'Không áp dụng',
    cls: 'bg-slate-200 text-slate-600 dark:bg-slate-800 dark:text-slate-400',
  },
};

const BASIS_LABEL: Record<ComplianceBasisKind, string> = {
  legalRegulation: 'Quy định thể thức',
  technicalRecommendation: 'Khuyến nghị kỹ thuật',
  organizationPolicy: 'Cấu hình tổ chức',
};

function classifyFindingForAudit(f: TraceableFinding): AuditGroupKey {
  if (f.safetyTier === 'groupCManualOnly' || f.checkResult === 'indeterminate') {
    return 'manual';
  }
  const text = `${f.ruleName} ${f.formComponent} ${f.location}`.toLowerCase();
  if (
    f.formatGroup === 'capitalizationAndAbbreviation' ||
    text.includes('quốc hiệu') ||
    text.includes('tiêu ngữ') ||
    text.includes('số') ||
    text.includes('chữ ký') ||
    text.includes('thể thức') ||
    text.includes('nơi nhận') ||
    text.includes('trích yếu')
  ) {
    return 'formal';
  }
  if (
    f.formatGroup === 'fontAndEncoding' ||
    f.formatGroup === 'paragraphAndAlignment' ||
    f.formatGroup === 'headingAndStructure'
  ) {
    return 'format';
  }
  return 'layout';
}

function classifyAdvancedIssueForAudit(adv: AdvancedAuditIssue): AuditGroupKey {
  if (adv.kind === 'structuralAnomaly') return 'layout';
  if (adv.kind === 'crossComponentMismatch') return 'formal';
  return 'manual';
}

export const ComplianceAuditWorkspace: React.FC<ComplianceAuditWorkspaceProps> = ({
  files,
  onPickFiles,
  onReplaceWebFile,
  onInspectInPreview,
  onStartStandardize,
  onAuditCompleted,
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();
  const adminMode = isAdminToolsEnabled();
  const [subTab, setSubTab] = useState<SubTab>('report');
  const [selectedPath, setSelectedPath] = useState<string>('');
  const [scope, setScope] = useState<StdDocScope | null>(null);
  const [docTypeHint, setDocTypeHint] = useState<string>('');
  const [outputMode, setOutputMode] = useState<OutputMode>('newCopy');
  const [orgProfiles, setOrgProfiles] = useState<OrganizationProfile[]>([]);
  const [selectedOrgId, setSelectedOrgId] = useState<string>('');
  const [busy, setBusy] = useState<boolean>(false);

  const [report, setReport] = useState<ComplianceAuditReport | null>(null);
  const [recheckRes, setRecheckRes] = useState<ComplianceAutoFixAndRecheckResult | null>(null);
  const [skippedFindingIds, setSkippedFindingIds] = useState<Record<string, boolean>>({});
  const [approvedFindingIds, setApprovedFindingIds] = useState<Record<string, boolean>>({});

  // Admin Matrix & Rule Pack & History states
  const [rulePack, setRulePack] = useState<ComplianceRulePack | null>(null);
  const [previewMatrix, setPreviewMatrix] = useState<ComplianceMatrixRow[]>([]);
  const [matrixFilterObl, setMatrixFilterObl] = useState<string>('all');
  const [matrixFilterComp, setMatrixFilterComp] = useState<string>('all');
  const [history, setHistory] = useState<AuditHistoryEntry[]>([]);
  const [compareOlderId, setCompareOlderId] = useState<string>('');
  const [compareNewerId, setCompareNewerId] = useState<string>('');
  const [comparison, setComparison] = useState<AuditRunComparison | null>(null);

  const wordFiles = useMemo(
    () => files.filter((f) => ['docx', 'docm', 'doc'].includes(f.ext.toLowerCase())),
    [files],
  );

  useEffect(() => {
    if (!selectedPath && wordFiles.length > 0) setSelectedPath(wordFiles[0].path);
    else if (selectedPath && !wordFiles.some((f) => f.path === selectedPath)) {
      setSelectedPath(wordFiles[0]?.path ?? '');
    }
  }, [wordFiles, selectedPath]);

  const loadMetadata = useCallback(async () => {
    try {
      const defOrgId = loadDefaultDocInfo().defaultOrgProfileId;
      if (isNative) {
        const [profiles, pack, hist] = await Promise.all([
          bridge.invoke<OrganizationProfile[]>('list_org_profiles', {}),
          bridge.invoke<ComplianceRulePack>('get_compliance_rule_pack', {}),
          bridge.invoke<AuditHistoryEntry[]>('list_audit_history', {}),
        ]);
        setOrgProfiles(profiles);
        if (!selectedOrgId && profiles.length > 0) {
          const preferred = profiles.find((p) => p.id === defOrgId);
          setSelectedOrgId((preferred ?? profiles[0]).id);
        }
        setRulePack(pack);
        setHistory(hist);
      } else {
        const profiles = listWebOrgProfiles();
        setOrgProfiles(profiles);
        if (!selectedOrgId && profiles.length > 0) {
          const preferred = profiles.find((p) => p.id === defOrgId);
          setSelectedOrgId((preferred ?? profiles[0]).id);
        }
        setRulePack(getWebRulePack());
        setHistory(listWebAuditHistory());
      }
    } catch {
      // ignore initial load hiccup
    }
  }, [isNative, selectedOrgId]);

  useEffect(() => {
    void loadMetadata();
  }, [loadMetadata]);

  const currentFile = useMemo(
    () => wordFiles.find((f) => f.path === selectedPath) ?? null,
    [wordFiles, selectedPath],
  );
  const selectedOrg = useMemo(
    () => orgProfiles.find((o) => o.id === selectedOrgId) ?? null,
    [orgProfiles, selectedOrgId],
  );

  const refreshHistory = async () => {
    if (isNative) {
      const h = await bridge.invoke<AuditHistoryEntry[]>('list_audit_history', {});
      setHistory(h);
    } else {
      setHistory(listWebAuditHistory());
    }
  };

  const handleRunAudit = async () => {
    if (!currentFile) {
      onStatusToast('info', 'Hãy chọn một tài liệu Word (.docx, .docm) để kiểm tra.');
      return;
    }
    if (!scope) {
      onStatusToast('info', 'Vui lòng chọn loại văn bản trước khi bắt đầu kiểm tra.');
      return;
    }
    setBusy(true);
    setRecheckRes(null);
    try {
      const rep = isNative
        ? await bridge.invoke<ComplianceAuditReport>('run_compliance_audit', {
            path: currentFile.path,
            scope,
            docTypeHint: docTypeHint || null,
            orgProfile: selectedOrg,
            disabledRuleIds: [],
            recordHistory: true,
          })
        : await runWebComplianceAudit(
            currentFile,
            scope,
            selectedOrg,
            docTypeHint || null,
            [],
            true,
          );
      setReport(rep);
      setSkippedFindingIds({});
      setApprovedFindingIds({});
      await refreshHistory();
      setSubTab('report');
      onAuditCompleted?.(currentFile, rep);
      onStatusToast(
        'success',
        adminMode
          ? `Kiểm toán xong: ${rep.traceableFindings.length} mục truy xuất · Chỉ số kỹ thuật ${rep.consistency.technicalQualityIndex}/100.`
          : `Đã hoàn tất kiểm tra "${currentFile.name}".`,
      );
    } catch (err) {
      onStatusToast('error', `Lỗi kiểm tra văn bản: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleAutoFixAndRecheck = async () => {
    if (!currentFile || !scope) return;
    setBusy(true);
    try {
      const execRequest: StdExecutionRequest = {
        path: currentFile.path,
        scope: report?.fileInfo.effectiveScope ?? scope,
        orgProfile: selectedOrg,
        outputMode,
        applyAllGroupA: true,
        approvedFindingIds: Object.keys(approvedFindingIds).filter((k) => approvedFindingIds[k]),
        skippedFindingIds: Object.keys(skippedFindingIds).filter((k) => skippedFindingIds[k]),
        manualReviewFindingIds: [],
        disabledRuleIds: [],
      };
      let res: ComplianceAutoFixAndRecheckResult;
      if (isNative) {
        res = await bridge.invoke<ComplianceAutoFixAndRecheckResult>(
          'execute_compliance_autofix_recheck',
          {
            execRequest,
            docTypeHint: docTypeHint || null,
            recordHistory: true,
          },
        );
      } else {
        res = await executeWebComplianceAutoFixAndRecheck(
          currentFile,
          execRequest,
          docTypeHint || null,
          true,
        );
        if (
          res.stdExecution.success &&
          res.stdExecution.outputBlob &&
          outputMode === 'overwriteWithBackup' &&
          onReplaceWebFile
        ) {
          const updated = new File([res.stdExecution.outputBlob], currentFile.name, {
            type: res.stdExecution.outputBlob.type,
          });
          onReplaceWebFile(currentFile.path, updated);
        }
      }
      setRecheckRes(res);
      if (res.auditAfter) setReport(res.auditAfter);
      await refreshHistory();
      if (res.stdExecution.success) {
        onStatusToast(
          'success',
          `Đã sửa ${res.stdExecution.actionsApplied.length} lỗi & kiểm tra lại xong.`,
        );
      } else {
        onStatusToast('error', res.stdExecution.error ?? 'Tự sửa thất bại.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi kiểm tra lại: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleExport = async (fmt: AuditExportFormat) => {
    if (!report) return;
    const baseName = report.fileInfo.fileName.replace(/\.[^.]+$/, '');
    const defaultName = `${baseName}_BaoCaoKiemTra.${fmt}`;

    if (!isNative) {
      if (fmt === 'pdf') {
        onStatusToast('info', 'Xuất PDF dùng Microsoft Word trên bản Windows Desktop. Trên Web hãy dùng Xuất báo cáo DOCX.');
        return;
      }
      const blob =
        fmt === 'docx'
          ? await buildWebReportDocxBlob(report)
          : fmt === 'csv'
          ? buildWebReportCsv(report)
          : new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      triggerBrowserDownload(url, defaultName);
      onStatusToast('success', `Đã tải xuống báo cáo ${defaultName}.`);
      return;
    }

    try {
      const destPath = await bridge.invoke<string | null>('pick_save_audit_export', {
        defaultName,
        ext: fmt,
      });
      if (!destPath) return;
      const res = await bridge.invoke<AuditExportResult>('export_compliance_report', {
        report,
        format: fmt,
        destPath,
      });
      if (res.success) {
        onStatusToast('success', `Đã xuất báo cáo kiểm tra: ${res.outputPath}`);
      } else {
        onStatusToast('error', res.error ?? 'Xuất báo cáo thất bại.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi xuất báo cáo: ${String(err)}`);
    }
  };

  const handleLoadPreviewMatrix = async () => {
    const activeScope = scope ?? 'adminNd30';
    try {
      if (isNative) {
        const rows = await bridge.invoke<ComplianceMatrixRow[]>('get_compliance_matrix', {
          scope: activeScope === 'autoDetect' ? 'adminNd30' : activeScope,
          docType: docTypeHint || 'quyetDinh',
          orgProfile: selectedOrg,
        });
        setPreviewMatrix(rows);
      } else if (rulePack) {
        setPreviewMatrix(
          buildWebComplianceMatrix(
            rulePack,
            activeScope === 'autoDetect' ? 'adminNd30' : activeScope,
            docTypeHint || 'quyetDinh',
            selectedOrg,
          ),
        );
      }
    } catch (err) {
      onStatusToast('error', String(err));
    }
  };

  const handleSaveRulePack = async () => {
    if (!rulePack) return;
    try {
      const saved = isNative
        ? await bridge.invoke<ComplianceRulePack>('save_compliance_rule_pack', rulePack)
        : saveWebRulePack(rulePack);
      setRulePack(saved);
      onStatusToast('success', `Đã lưu Rule Pack phiên bản v${saved.packVersion}.`);
    } catch (err) {
      onStatusToast('error', String(err));
    }
  };

  const handleResetRulePack = async () => {
    try {
      const defPack = isNative
        ? await bridge.invoke<ComplianceRulePack>('reset_compliance_rule_pack', {})
        : resetWebRulePack();
      setRulePack(defPack);
      onStatusToast('info', `Đã khôi phục Rule Pack gốc v${defPack.packVersion}.`);
    } catch (err) {
      onStatusToast('error', String(err));
    }
  };

  const handleCompareHistory = async () => {
    if (!compareOlderId || !compareNewerId) {
      onStatusToast('info', 'Hãy chọn đủ 2 lần kiểm tra để đối chiếu.');
      return;
    }
    try {
      if (isNative) {
        const cmp = await bridge.invoke<AuditRunComparison>('compare_audit_history', {
          olderAuditId: compareOlderId,
          newerAuditId: compareNewerId,
        });
        setComparison(cmp);
      } else {
        const older = history.find((h) => h.auditId === compareOlderId);
        const newer = history.find((h) => h.auditId === compareNewerId);
        if (older && newer) setComparison(compareWebAuditHistory(older, newer));
      }
    } catch (err) {
      onStatusToast('error', String(err));
    }
  };

  // Group audit results into 4 issue buckets + 1 recommendations bucket (5 groups total)
  const categorizedAudit = useMemo(() => {
    const groups: Record<AuditGroupKey, AuditDisplayItem[]> = {
      formal: [],
      format: [],
      layout: [],
      manual: [],
    };
    const suggestions: string[] = [];
    let passedCount = 0;
    let notApplicableCount = 0;
    let needsFixCount = 0;
    let manualCount = 0;
    let autoFixableCount = 0;

    if (!report) {
      return {
        groups,
        suggestions,
        counts: { passedCount, needsFixCount, manualCount, notApplicableCount, autoFixableCount },
      };
    }

    for (const m of report.matrix) {
      if (m.evaluationStatus === 'passed') passedCount += 1;
      else if (m.evaluationStatus === 'notApplicable' || m.obligation === 'notApplicable') {
        notApplicableCount += 1;
      }
    }

    for (const f of report.traceableFindings) {
      const grp = classifyFindingForAudit(f);
      const isManual = grp === 'manual';
      if (isManual) manualCount += 1;
      else needsFixCount += 1;
      if (f.autoFixSupported && f.safetyTier !== 'groupCManualOnly') {
        autoFixableCount += 1;
      }
      groups[grp].push({
        id: f.findingId,
        title: f.ruleName,
        description: `Hiện tại: ${f.currentValue || 'Chưa rõ'} → Cần đạt: ${f.expectedValue || 'Đúng quy định'}`,
        location: f.location,
        elementPath: f.elementPath,
        suggestion: f.remediationMethod,
        statusLabel: isManual ? 'Cần kiểm tra thủ công' : 'Cần chỉnh sửa',
        adminRuleTag: `${f.ruleId} (v${f.ruleVersion})`,
      });
    }

    for (const adv of report.advancedIssues) {
      const grp = classifyAdvancedIssueForAudit(adv);
      const isManual = grp === 'manual';
      if (isManual) manualCount += 1;
      else needsFixCount += 1;
      groups[grp].push({
        id: adv.issueId,
        title: adv.title,
        description: adv.explanation,
        location: adv.location,
        elementPath: adv.elementPath,
        suggestion: adv.remediation,
        statusLabel: isManual ? 'Cần kiểm tra thủ công' : 'Cần chỉnh sửa',
        adminRuleTag: adv.issueId,
      });
    }

    if (groups.format.length > 0) {
      suggestions.push(
        `Có ${groups.format.length} vấn đề định dạng (phông chữ, cỡ chữ, giãn dòng, căn lề) có thể xử lý tự động bằng nút "Chuẩn hóa tài liệu này".`,
      );
    }
    if (groups.layout.length > 0) {
      suggestions.push(
        `Có ${groups.layout.length} vấn đề về bố cục trang, bảng biểu hoặc đánh số trang nên chuẩn hóa và xem trước trang A4 trước khi lưu.`,
      );
    }
    if (groups.formal.length > 0) {
      suggestions.push(
        `Có ${groups.formal.length} mục thể thức văn bản cần đối chiếu tên cơ quan, số hiệu, ngày tháng và chữ ký trước khi phát hành.`,
      );
    }
    if (groups.manual.length > 0) {
      suggestions.push(
        `Có ${groups.manual.length} nội dung cần bạn kiểm tra trực tiếp bằng mắt để bảo đảm chính xác thông tin nghiệp vụ.`,
      );
    }
    for (const c of report.repeatedClusters) {
      suggestions.push(
        `${c.ruleName} lặp lại tại ${c.occurrenceCount} vị trí — nên chuẩn hóa đồng loạt để thống nhất toàn văn bản.`,
      );
    }
    if (suggestions.length === 0) {
      suggestions.push(
        'Văn bản chưa phát hiện lỗi trình bày hay thể thức cần sửa trong phạm vi đã kiểm tra.',
      );
    }

    return {
      groups,
      suggestions,
      counts: { passedCount, needsFixCount, manualCount, notApplicableCount, autoFixableCount },
    };
  }, [report]);

  const activeMatrix = report?.matrix ?? previewMatrix;
  const filteredMatrix = useMemo(
    () =>
      activeMatrix.filter(
        (m) =>
          (matrixFilterObl === 'all' || m.obligation === matrixFilterObl) &&
          (matrixFilterComp === 'all' || m.formComponent === matrixFilterComp),
      ),
    [activeMatrix, matrixFilterObl, matrixFilterComp],
  );
  const componentOptions = useMemo(
    () => Array.from(new Set(activeMatrix.map((m) => m.formComponent))),
    [activeMatrix],
  );

  return (
    <div className="mx-auto max-w-5xl space-y-4">
      {/* Control Header */}
      <section className="mw-panel p-5 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-base font-bold flex items-center gap-2 text-[var(--mw-ink)]">
              <ClipboardCheck className="w-5 h-5 text-[var(--mw-accent)]" />
              Kiểm tra văn bản
            </h2>
            <p className="text-xs text-[var(--mw-muted)] mt-0.5">
              Rà soát lỗi thể thức, định dạng và bố cục độc lập mà không làm thay đổi tệp gốc.
            </p>
          </div>
          <button type="button" onClick={onPickFiles} className="mw-button mw-button-secondary">
            <FolderOpen className="w-4 h-4" /> Chọn tài liệu
          </button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
          <label className="block space-y-1">
            <span className="font-semibold text-[var(--mw-ink)]">1. Tài liệu cần kiểm tra</span>
            <select
              value={selectedPath}
              onChange={(e) => {
                setSelectedPath(e.target.value);
                setReport(null);
              }}
              className="w-full rounded-xl border border-[var(--mw-border)] px-3 py-2 text-xs font-medium"
            >
              {wordFiles.length === 0 ? (
                <option value="">-- Chưa chọn tệp Word --</option>
              ) : (
                wordFiles.map((f) => (
                  <option key={f.path} value={f.path}>
                    {f.name}
                  </option>
                ))
              )}
            </select>
          </label>

          <label className="block space-y-1">
            <span className="font-semibold text-[var(--mw-ink)]">2. Loại văn bản</span>
            <select
              value={scope ?? ''}
              onChange={(e) => setScope((e.target.value || null) as StdDocScope | null)}
              className="w-full rounded-xl border border-[var(--mw-border)] px-3 py-2 text-xs font-medium"
            >
              <option value="">-- Chọn loại văn bản --</option>
              {DOC_KIND_OPTIONS.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.title} — {d.hint}
                </option>
              ))}
              {adminMode &&
                ADMIN_SCOPE_OPTIONS.filter(
                  (a) => !DOC_KIND_OPTIONS.some((d) => d.id === a.id),
                ).map((a) => (
                  <option key={a.id} value={a.id}>
                    [Admin] {a.label}
                  </option>
                ))}
            </select>
          </label>
        </div>

        {adminMode && (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1 text-xs">
            <label className="block space-y-1">
              <span className="font-semibold text-[var(--mw-muted)]">Gợi ý loại văn bản (?admin=1)</span>
              <select
                value={docTypeHint}
                onChange={(e) => setDocTypeHint(e.target.value)}
                className="w-full rounded-xl border border-[var(--mw-border)] px-2.5 py-2 text-xs"
              >
                {DOC_TYPE_HINTS.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="block space-y-1">
              <span className="font-semibold text-[var(--mw-muted)]">Hồ sơ Tổ chức (?admin=1)</span>
              <select
                value={selectedOrgId}
                onChange={(e) => setSelectedOrgId(e.target.value)}
                className="w-full rounded-xl border border-[var(--mw-border)] px-2.5 py-2 text-xs"
              >
                <option value="">Không chọn hồ sơ</option>
                {orgProfiles.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.profileName}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}

        <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
          <p className="text-[11px] text-[var(--mw-muted)] flex items-center gap-1.5">
            <ShieldAlert className="w-3.5 h-3.5 text-amber-600 shrink-0" />
            <span>
              {adminMode && report?.legalDisclaimer
                ? report.legalDisclaimer
                : 'Chỉ kiểm tra lỗi trình bày, định dạng và thể thức; không làm thay đổi tệp gốc và không thay thế thẩm định nội dung pháp lý.'}
            </span>
          </p>

          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              disabled={busy || !currentFile || !scope}
              onClick={() => void handleRunAudit()}
              className="mw-button mw-button-primary px-5 py-2 text-sm"
            >
              <ShieldCheck className="w-4 h-4" />
              {busy ? 'Đang kiểm tra...' : 'Bắt đầu kiểm tra'}
            </button>

            {adminMode && (
              <button
                type="button"
                disabled={busy || !currentFile || !scope}
                onClick={() => void handleAutoFixAndRecheck()}
                className="mw-button mw-button-secondary"
              >
                <Play className="w-3.5 h-3.5 text-emerald-600" />
                Sửa &amp; Hậu kiểm
              </button>
            )}
          </div>
        </div>

        {/* Subtabs (?admin=1 only) */}
        {adminMode && (
          <div className="flex flex-wrap items-center justify-between gap-2 pt-3 border-t border-[var(--mw-border)]">
            <div className="flex flex-wrap gap-1.5">
              {(
                [
                  ['report', 'Báo cáo & Truy xuất lỗi', FileText],
                  ['matrix', 'Ma trận Tuân thủ', TableProperties],
                  ['rulepack', `Phiên bản Quy tắc (v${rulePack?.packVersion ?? '2026.1.0'})`, Layers],
                  ['history', `Lịch sử & So sánh (${history.length})`, History],
                ] as const
              ).map(([id, label, Icon]) => (
                <button
                  key={id}
                  type="button"
                  onClick={() => {
                    setSubTab(id);
                    if (id === 'matrix' && activeMatrix.length === 0) void handleLoadPreviewMatrix();
                  }}
                  className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold ${
                    subTab === id
                      ? 'bg-blue-600 text-white'
                      : 'bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5" />
                  {label}
                </button>
              ))}
            </div>

            <div className="inline-flex items-center rounded-lg border border-slate-200 dark:border-slate-700 p-0.5 text-[11px]">
              <button
                type="button"
                onClick={() => setOutputMode('newCopy')}
                className={`px-2 py-1 rounded font-semibold ${outputMode === 'newCopy' ? 'bg-blue-600 text-white' : ''}`}
              >
                Bản sao
              </button>
              <button
                type="button"
                onClick={() => setOutputMode('overwriteWithBackup')}
                className={`px-2 py-1 rounded font-semibold ${outputMode === 'overwriteWithBackup' ? 'bg-amber-600 text-white' : ''}`}
              >
                Ghi đè + Backup
              </button>
            </div>
          </div>
        )}
      </section>

      {/* TAB REPORT */}
      {subTab === 'report' &&
        (report ? (
          <div className="space-y-4">
            {/* 4 Trạng thái thống nhất (không điểm /100) */}
            <section className="mw-panel p-4 space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="space-y-0.5 text-xs">
                  <div className="text-sm font-bold text-[var(--mw-ink)]">
                    Kết quả kiểm tra: {report.fileInfo.fileName}
                    {adminMode && (
                      <span className="ml-2 font-mono text-[11px] font-normal text-[var(--mw-muted)]">
                        · {report.fileInfo.detectedDocType} · SHA-256: {report.fileInfo.docSha256.slice(0, 12)}…
                      </span>
                    )}
                  </div>
                  <div className="text-[11px] text-[var(--mw-muted)]">
                    Tệp gốc hoàn toàn không bị thay đổi.
                  </div>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  {onStartStandardize && currentFile && scope && categorizedAudit.counts.autoFixableCount > 0 && (
                    <button
                      type="button"
                      onClick={() => onStartStandardize(currentFile.path, scope)}
                      className="mw-button mw-button-primary"
                    >
                      Chuẩn hóa tài liệu này
                      <ArrowRight className="w-4 h-4" />
                    </button>
                  )}

                  <button
                    type="button"
                    onClick={() => void handleExport('docx')}
                    className="mw-button mw-button-secondary"
                  >
                    <Download className="w-3.5 h-3.5" /> Xuất báo cáo kiểm tra
                  </button>

                  {onInspectInPreview && selectedPath && (
                    <button
                      type="button"
                      onClick={() => onInspectInPreview(selectedPath, '')}
                      className="mw-button mw-button-secondary"
                    >
                      <Eye className="w-3.5 h-3.5 text-[var(--mw-accent)]" /> Xem trước trang A4
                    </button>
                  )}

                  {adminMode && (
                    <>
                      <button
                        type="button"
                        onClick={() => void handleExport('csv')}
                        className="mw-button mw-button-secondary"
                      >
                        <FileSpreadsheet className="w-3.5 h-3.5 text-emerald-600" /> CSV
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleExport('json')}
                        className="mw-button mw-button-secondary"
                      >
                        <Download className="w-3.5 h-3.5 text-indigo-500" /> JSON
                      </button>
                    </>
                  )}
                </div>
              </div>

              <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5 text-center text-xs">
                <div className="rounded-xl border border-emerald-200 dark:border-emerald-900/60 bg-emerald-50/70 dark:bg-emerald-950/20 p-2.5">
                  <div className="text-lg font-extrabold text-emerald-700 dark:text-emerald-300">
                    {categorizedAudit.counts.passedCount}
                  </div>
                  <div className="text-[11px] font-semibold text-emerald-800/80 dark:text-emerald-300/80">
                    Đạt
                  </div>
                </div>
                <div className="rounded-xl border border-amber-200 dark:border-amber-900/60 bg-amber-50/70 dark:bg-amber-950/20 p-2.5">
                  <div className="text-lg font-extrabold text-amber-700 dark:text-amber-300">
                    {categorizedAudit.counts.needsFixCount}
                  </div>
                  <div className="text-[11px] font-semibold text-amber-800/80 dark:text-amber-300/80">
                    Cần chỉnh sửa
                  </div>
                </div>
                <div className="rounded-xl border border-slate-200 dark:border-slate-800 bg-[var(--mw-canvas)] p-2.5">
                  <div className="text-lg font-extrabold text-[var(--mw-ink)]">
                    {categorizedAudit.counts.manualCount}
                  </div>
                  <div className="text-[11px] font-semibold text-[var(--mw-muted)]">
                    Cần kiểm tra thủ công
                  </div>
                </div>
                <div className="rounded-xl border border-slate-200 dark:border-slate-800 bg-[var(--mw-canvas)] p-2.5">
                  <div className="text-lg font-extrabold text-[var(--mw-muted)]">
                    {categorizedAudit.counts.notApplicableCount}
                  </div>
                  <div className="text-[11px] font-semibold text-[var(--mw-muted)]">
                    Không áp dụng
                  </div>
                </div>
              </div>

              {adminMode && (
                <div className="text-[11px] text-[var(--mw-muted)] pt-1 border-t border-[var(--mw-border)]">
                  Chỉ số kỹ thuật (?admin=1): {report.consistency.technicalQualityIndex}/100 · Đồng nhất phông:{' '}
                  {report.consistency.fontUniformityPct}% · Rule Pack v{report.ruleProfile.packVersion}
                </div>
              )}
            </section>

            {recheckRes?.stdExecution.outputBlob && (
              <div className="mw-panel p-3 flex items-center justify-between gap-2 text-xs">
                <span className="font-semibold text-emerald-700">
                  Tệp đã sửa: {recheckRes.stdExecution.outputName || recheckRes.stdExecution.outputPath}
                </span>
                {recheckRes.stdExecution.blobUrl && (
                  <button
                    type="button"
                    onClick={() =>
                      triggerBrowserDownload(
                        recheckRes.stdExecution.blobUrl!,
                        recheckRes.stdExecution.outputName || recheckRes.stdExecution.outputPath,
                      )
                    }
                    className="mw-button mw-button-primary"
                  >
                    <Download className="w-3.5 h-3.5" /> Tải tệp đã sửa
                  </button>
                )}
              </div>
            )}

            {/* 5 Nhóm kết quả kiểm tra */}
            {(
              [
                { key: 'formal', title: '1. Lỗi thể thức', items: categorizedAudit.groups.formal },
                { key: 'format', title: '2. Lỗi định dạng', items: categorizedAudit.groups.format },
                { key: 'layout', title: '3. Vấn đề về bố cục', items: categorizedAudit.groups.layout },
                { key: 'manual', title: '4. Nội dung cần kiểm tra thủ công', items: categorizedAudit.groups.manual },
              ] as const
            ).map((section) => (
              <section key={section.key} className="mw-panel overflow-hidden">
                <div className="flex items-center justify-between gap-2 border-b border-[var(--mw-border)] bg-[var(--mw-canvas)] px-4 py-2.5">
                  <h3 className="text-xs sm:text-sm font-bold text-[var(--mw-ink)]">
                    {section.title} ({section.items.length})
                  </h3>
                  {section.items.length === 0 && (
                    <span className="inline-flex items-center gap-1 rounded-full bg-emerald-100 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-300 px-2 py-0.5 text-[10px] font-bold">
                      <CheckCircle2 className="size-3" /> Đạt
                    </span>
                  )}
                </div>

                {section.items.length === 0 ? (
                  <p className="px-4 py-3 text-xs text-[var(--mw-muted)]">
                    Không phát hiện vấn đề trong nhóm này.
                  </p>
                ) : (
                  <div className="divide-y divide-[var(--mw-border)]">
                    {section.items.map((item) => (
                      <div key={item.id} className="p-3.5 text-xs space-y-1.5">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <div className="font-bold text-[var(--mw-ink)]">
                            {adminMode && item.adminRuleTag ? `[${item.adminRuleTag}] ` : ''}
                            {item.title}
                          </div>
                          <span
                            className={`rounded-full px-2 py-0.5 text-[10px] font-bold ${
                              item.statusLabel === 'Cần kiểm tra thủ công'
                                ? 'bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300'
                                : 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300'
                            }`}
                          >
                            {item.statusLabel}
                          </span>
                        </div>
                        <p className="text-[11px] text-[var(--mw-muted)]">{item.description}</p>
                        <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-[var(--mw-muted)]">
                          <span>
                            {item.location ? (
                              <>
                                <strong>Vị trí:</strong> {item.location} ·{' '}
                              </>
                            ) : null}
                            <strong>Đề xuất:</strong> {item.suggestion}
                          </span>
                          {onInspectInPreview && selectedPath && item.elementPath && (
                            <button
                              type="button"
                              onClick={() => onInspectInPreview(selectedPath, item.elementPath)}
                              className="inline-flex items-center gap-1 text-[11px] font-semibold text-[var(--mw-accent)] hover:underline"
                            >
                              <Eye className="w-3 h-3" /> Xem trên trang A4
                            </button>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            ))}

            {/* 5. Đề xuất cách xử lý */}
            <section className="mw-panel p-4 space-y-2">
              <h3 className="text-xs sm:text-sm font-bold text-[var(--mw-ink)] flex items-center gap-1.5">
                <AlertTriangle className="size-4 text-amber-600" />
                5. Đề xuất cách xử lý
              </h3>
              <ul className="list-disc pl-5 space-y-1 text-xs text-[var(--mw-muted)]">
                {categorizedAudit.suggestions.map((s, idx) => (
                  <li key={idx}>{s}</li>
                ))}
              </ul>
            </section>
          </div>
        ) : (
          <div className="mw-panel p-8 text-center text-xs text-[var(--mw-muted)] space-y-2">
            <Info className="w-6 h-6 mx-auto text-[var(--mw-accent)]" />
            <p>
              Chọn tài liệu Word và loại văn bản phía trên rồi bấm <strong>Bắt đầu kiểm tra</strong> để xem báo cáo 5 nhóm chi tiết.
            </p>
          </div>
        ))}

      {/* ADMIN SUBTABS (?admin=1 only) */}
      {adminMode && subTab === 'matrix' && (
        <div className="mw-panel p-4 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="text-xs font-bold">
              Ma trận Quy tắc Tuân thủ ({filteredMatrix.length} quy tắc)
            </div>
            <div className="flex flex-wrap gap-2">
              <select
                value={matrixFilterObl}
                onChange={(e) => setMatrixFilterObl(e.target.value)}
                className="rounded-lg border border-slate-300 dark:border-slate-700 px-2 py-1 text-xs"
              >
                <option value="all">Mọi nghĩa vụ</option>
                <option value="mandatory">Bắt buộc</option>
                <option value="conditional">Có điều kiện</option>
                <option value="recommended">Khuyến nghị kỹ thuật</option>
                <option value="notApplicable">Không áp dụng</option>
              </select>
              <select
                value={matrixFilterComp}
                onChange={(e) => setMatrixFilterComp(e.target.value)}
                className="rounded-lg border border-slate-300 dark:border-slate-700 px-2 py-1 text-xs"
              >
                <option value="all">Mọi thành phần thể thức</option>
                {componentOptions.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
              <button
                type="button"
                onClick={() => void handleLoadPreviewMatrix()}
                className="mw-button mw-button-secondary"
              >
                <RefreshCw className="w-3.5 h-3.5" /> Cập nhật Ma trận
              </button>
            </div>
          </div>

          <div className="max-h-[420px] overflow-y-auto space-y-2 pr-1">
            {filteredMatrix.map((row) => {
              const ob = OBLIGATION_BADGE[row.obligation];
              return (
                <div
                  key={row.ruleId}
                  className="rounded-xl border border-slate-200 dark:border-slate-800 p-2.5 text-xs space-y-1"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span className="font-mono font-bold text-blue-600">
                        {row.ruleId} v{row.ruleVersion}
                      </span>
                      <span className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${ob.cls}`}>
                        {ob.label}
                      </span>
                      <span className="font-semibold">{row.ruleName}</span>
                    </div>
                    <span className="text-[11px] font-semibold">
                      {row.evaluationStatus ?? 'Chưa quét tệp'} ({row.findingCount} lỗi)
                    </span>
                  </div>
                  <div className="text-[11px] text-slate-600 dark:text-slate-400">
                    {BASIS_LABEL[row.basisKind]}: {row.legalBasis}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {adminMode && subTab === 'rulepack' && rulePack && (
        <div className="mw-panel p-4 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <div className="text-xs font-bold">{rulePack.packName}</div>
              <div className="text-[11px] text-slate-500">{rulePack.legalReferenceSummary}</div>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => void handleResetRulePack()}
                className="mw-button mw-button-secondary"
              >
                <RotateCcw className="w-3.5 h-3.5" /> Khôi phục gốc
              </button>
              <button
                type="button"
                onClick={() => void handleSaveRulePack()}
                className="mw-button mw-button-primary"
              >
                <Save className="w-3.5 h-3.5" /> Lưu Phiên bản Quy tắc
              </button>
            </div>
          </div>

          <div className="max-h-80 overflow-y-auto space-y-1.5 pr-1">
            {rulePack.rules.map((r, idx) => (
              <div
                key={r.ruleId}
                className="rounded-lg border border-slate-200 dark:border-slate-800 p-2 text-xs grid grid-cols-1 sm:grid-cols-12 gap-2 items-center"
              >
                <div className="sm:col-span-3 font-mono font-bold flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={r.enabled}
                    onChange={(e) => {
                      const next = [...rulePack.rules];
                      next[idx] = { ...r, enabled: e.target.checked };
                      setRulePack({ ...rulePack, rules: next });
                    }}
                  />
                  <span>{r.ruleId}</span>
                </div>
                <div className="sm:col-span-2">
                  <input
                    type="text"
                    value={r.ruleVersion}
                    onChange={(e) => {
                      const next = [...rulePack.rules];
                      next[idx] = { ...r, ruleVersion: e.target.value };
                      setRulePack({ ...rulePack, rules: next });
                    }}
                    className="w-full rounded border px-1.5 py-0.5 text-[11px] font-mono"
                  />
                </div>
                <div className="sm:col-span-7">
                  <input
                    type="text"
                    value={r.legalCitation}
                    onChange={(e) => {
                      const next = [...rulePack.rules];
                      next[idx] = { ...r, legalCitation: e.target.value };
                      setRulePack({ ...rulePack, rules: next });
                    }}
                    className="w-full rounded border px-1.5 py-0.5 text-[11px]"
                  />
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {adminMode && subTab === 'history' && (
        <div className="mw-panel p-4 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="text-xs font-bold">Lịch sử Kiểm toán Kỹ thuật ({history.length})</div>
            {history.length > 0 && (
              <button
                type="button"
                onClick={async () => {
                  if (isNative) await bridge.invoke('clear_audit_history', {});
                  else clearWebAuditHistory();
                  setHistory([]);
                  setComparison(null);
                }}
                className="mw-button mw-button-secondary text-rose-600"
              >
                <Trash2 className="w-3.5 h-3.5" /> Xóa lịch sử
              </button>
            )}
          </div>

          {history.length >= 2 && (
            <div className="grid grid-cols-1 md:grid-cols-3 gap-2 items-end">
              <select
                value={compareOlderId}
                onChange={(e) => setCompareOlderId(e.target.value)}
                className="rounded-lg border px-2 py-1.5 text-xs"
              >
                <option value="">-- Lần trước --</option>
                {history.map((h) => (
                  <option key={h.auditId} value={h.auditId}>
                    {h.fileName} ({h.checkedAt})
                  </option>
                ))}
              </select>
              <select
                value={compareNewerId}
                onChange={(e) => setCompareNewerId(e.target.value)}
                className="rounded-lg border px-2 py-1.5 text-xs"
              >
                <option value="">-- Lần sau --</option>
                {history.map((h) => (
                  <option key={h.auditId} value={h.auditId}>
                    {h.fileName} ({h.checkedAt})
                  </option>
                ))}
              </select>
              <button
                type="button"
                onClick={() => void handleCompareHistory()}
                className="mw-button mw-button-primary justify-center"
              >
                <GitCompare className="w-3.5 h-3.5" /> So sánh 2 lần
              </button>
            </div>
          )}

          {comparison && (
            <div className="rounded-xl border border-blue-200 p-3 text-xs">
              Đã khắc phục: <strong>{comparison.resolvedRuleLocations.length}</strong> · Còn tồn tại:{' '}
              <strong>{comparison.persistentRuleLocations.length}</strong>
            </div>
          )}

          <div className="max-h-72 overflow-y-auto space-y-2">
            {history.map((h) => (
              <div
                key={h.auditId}
                className="rounded-xl border border-slate-200 dark:border-slate-800 p-2.5 text-xs flex items-center justify-between gap-2"
              >
                <div>
                  <div className="font-bold">{h.fileName}</div>
                  <div className="text-[11px] text-slate-500">
                    {h.checkedAt} · Phát hiện: {h.detectedFindings.length}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={async () => {
                    const updated = isNative
                      ? await bridge.invoke<AuditHistoryEntry[]>('delete_audit_history_entry', {
                          auditId: h.auditId,
                        })
                      : deleteWebAuditHistoryEntry(h.auditId);
                    setHistory(updated);
                  }}
                  className="p-1.5 text-slate-400 hover:text-rose-500"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
