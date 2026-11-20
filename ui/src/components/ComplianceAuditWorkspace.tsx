import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  AlertTriangle,
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
import type {
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
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

type SubTab = 'report' | 'matrix' | 'rulepack' | 'history';

const SCOPE_OPTIONS: { id: StdDocScope; label: string }[] = [
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

export const ComplianceAuditWorkspace: React.FC<ComplianceAuditWorkspaceProps> = ({
  files,
  onPickFiles,
  onReplaceWebFile,
  onInspectInPreview,
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();
  const [subTab, setSubTab] = useState<SubTab>('report');
  const [selectedPath, setSelectedPath] = useState<string>('');
  const [scope, setScope] = useState<StdDocScope>('autoDetect');
  const [docTypeHint, setDocTypeHint] = useState<string>('');
  const [outputMode, setOutputMode] = useState<OutputMode>('newCopy');
  const [orgProfiles, setOrgProfiles] = useState<OrganizationProfile[]>([]);
  const [selectedOrgId, setSelectedOrgId] = useState<string>('');
  const [busy, setBusy] = useState<boolean>(false);

  const [report, setReport] = useState<ComplianceAuditReport | null>(null);
  const [recheckRes, setRecheckRes] = useState<ComplianceAutoFixAndRecheckResult | null>(null);
  const [skippedFindingIds, setSkippedFindingIds] = useState<Record<string, boolean>>({});
  const [approvedFindingIds, setApprovedFindingIds] = useState<Record<string, boolean>>({});

  // Matrix & Rule Pack states
  const [rulePack, setRulePack] = useState<ComplianceRulePack | null>(null);
  const [previewMatrix, setPreviewMatrix] = useState<ComplianceMatrixRow[]>([]);
  const [matrixFilterObl, setMatrixFilterObl] = useState<string>('all');
  const [matrixFilterComp, setMatrixFilterComp] = useState<string>('all');

  // Audit History & Comparison states
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
      if (isNative) {
        const [profiles, pack, hist] = await Promise.all([
          bridge.invoke<OrganizationProfile[]>('list_org_profiles', {}),
          bridge.invoke<ComplianceRulePack>('get_compliance_rule_pack', {}),
          bridge.invoke<AuditHistoryEntry[]>('list_audit_history', {}),
        ]);
        setOrgProfiles(profiles);
        if (profiles.length > 0 && !selectedOrgId) setSelectedOrgId(profiles[0].id);
        setRulePack(pack);
        setHistory(hist);
      } else {
        const profiles = listWebOrgProfiles();
        setOrgProfiles(profiles);
        if (profiles.length > 0 && !selectedOrgId) setSelectedOrgId(profiles[0].id);
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
      onStatusToast('info', 'Hãy mở hoặc chọn 1 tệp Word (.docx, .docm).');
      return;
    }
    setBusy(true);
    setRecheckRes(null);
    try {
      let rep: ComplianceAuditReport;
      if (isNative) {
        rep = await bridge.invoke<ComplianceAuditReport>('run_compliance_audit', {
          path: currentFile.path,
          scope,
          docTypeHint: docTypeHint || null,
          orgProfile: selectedOrg,
          disabledRuleIds: [],
          recordHistory: true,
        });
      } else {
        rep = await runWebComplianceAudit(
          currentFile,
          scope,
          selectedOrg,
          docTypeHint || null,
          [],
          true,
        );
      }
      setReport(rep);
      setSkippedFindingIds({});
      setApprovedFindingIds({});
      await refreshHistory();
      setSubTab('report');
      onStatusToast(
        'success',
        `Kiểm toán xong: ${rep.traceableFindings.length} mục truy xuất · Chỉ số kỹ thuật ${rep.consistency.technicalQualityIndex}/100.`,
      );
    } catch (err) {
      onStatusToast('error', `Lỗi kiểm toán tuân thủ: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleAutoFixAndRecheck = async () => {
    if (!currentFile) return;
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
        onStatusToast('error', res.stdExecution.error ?? 'Auto-Fix thất bại.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi Auto-Fix & Kiểm tra lại: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleExport = async (fmt: AuditExportFormat) => {
    if (!report) return;
    const baseName = report.fileInfo.fileName.replace(/\.[^.]+$/, '');
    const defaultName = `${baseName}_AuditReport.${fmt}`;

    if (!isNative) {
      if (fmt === 'pdf') {
        onStatusToast('info', 'Xuất PDF tự động dùng Word COM trên bản Windows Desktop. Trên Web hãy xuất DOCX.');
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
        onStatusToast('success', `Đã xuất báo cáo (${fmt.toUpperCase()}): ${res.outputPath}`);
      } else {
        onStatusToast('error', res.error ?? 'Xuất báo cáo thất bại.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi xuất báo cáo: ${String(err)}`);
    }
  };

  const handleLoadPreviewMatrix = async () => {
    try {
      if (isNative) {
        const rows = await bridge.invoke<ComplianceMatrixRow[]>('get_compliance_matrix', {
          scope: scope === 'autoDetect' ? 'adminNd30' : scope,
          docType: docTypeHint || 'quyetDinh',
          orgProfile: selectedOrg,
        });
        setPreviewMatrix(rows);
      } else if (rulePack) {
        setPreviewMatrix(
          buildWebComplianceMatrix(
            rulePack,
            scope === 'autoDetect' ? 'adminNd30' : scope,
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
    <div className="space-y-4">
      {/* Control Header */}
      <div className="mw-panel p-4 sm:p-5 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-base font-bold flex items-center gap-2">
              <ClipboardCheck className="w-5 h-5 text-blue-600" />
              Advanced Compliance &amp; Audit · Kiểm toán Tuân thủ Chuyên sâu
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400 mt-1">
              Ma trận quy tắc theo loại văn bản &amp; phạm vi, truy xuất căn cứ &amp; phiên bản quy tắc, kiểm tra chéo, xuất báo cáo DOCX/PDF/CSV và quản lý lịch sử ẩn danh.
            </p>
          </div>
          <button type="button" onClick={onPickFiles} className="mw-button mw-button-secondary">
            <FolderOpen className="w-4 h-4" /> Mở tệp Word
          </button>
        </div>

        {/* Non-Legal Scope Disclaimer */}
        <div className="rounded-xl border border-amber-200 dark:border-amber-900/60 bg-amber-50/70 dark:bg-amber-950/30 p-3 flex items-start gap-2.5 text-xs text-amber-900 dark:text-amber-200">
          <ShieldAlert className="w-4 h-4 shrink-0 mt-0.5 text-amber-600" />
          <div>
            <span className="font-bold">Giới hạn phạm vi pháp lý: </span>
            {report?.legalDisclaimer ??
              'Kết quả kiểm toán chỉ phản ánh tính tuân thủ kỹ thuật trình bày, bố cục và thể thức OpenXML; KHÔNG phải kết luận văn bản hợp pháp hay có giá trị pháp lý.'}
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-3 items-end">
          <div>
            <label className="block text-xs font-semibold mb-1">Tài liệu kiểm tra</label>
            <select
              value={selectedPath}
              onChange={(e) => setSelectedPath(e.target.value)}
              className="w-full rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2.5 py-2 text-xs"
            >
              {wordFiles.length === 0 ? (
                <option value="">-- Chưa có tệp Word --</option>
              ) : (
                wordFiles.map((f) => (
                  <option key={f.path} value={f.path}>
                    {f.name}
                  </option>
                ))
              )}
            </select>
          </div>

          <div>
            <label className="block text-xs font-semibold mb-1">Phạm vi áp dụng</label>
            <select
              value={scope}
              onChange={(e) => setScope(e.target.value as StdDocScope)}
              className="w-full rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2.5 py-2 text-xs"
            >
              {SCOPE_OPTIONS.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-xs font-semibold mb-1">Loại văn bản / Hồ sơ Tổ chức</label>
            <div className="grid grid-cols-2 gap-1.5">
              <select
                value={docTypeHint}
                onChange={(e) => setDocTypeHint(e.target.value)}
                className="rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2 py-2 text-xs"
              >
                {DOC_TYPE_HINTS.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.label}
                  </option>
                ))}
              </select>
              <select
                value={selectedOrgId}
                onChange={(e) => setSelectedOrgId(e.target.value)}
                className="rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2 py-2 text-xs"
              >
                <option value="">Không chọn hồ sơ</option>
                {orgProfiles.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.profileName}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy || !currentFile}
              onClick={() => void handleRunAudit()}
              className="mw-button mw-button-primary flex-1 justify-center"
            >
              <ShieldCheck className="w-4 h-4" />
              {busy ? 'Đang chạy...' : 'Kiểm toán'}
            </button>
            <button
              type="button"
              disabled={busy || !currentFile}
              onClick={() => void handleAutoFixAndRecheck()}
              className="mw-button mw-button-secondary justify-center"
              title="Tự sửa nhóm an toàn + mục đã chọn rồi kiểm tra lại"
            >
              <Play className="w-3.5 h-3.5 text-emerald-600" />
              Sửa &amp; Hậu kiểm
            </button>
          </div>
        </div>

        {/* Subtabs */}
        <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-slate-200 dark:border-slate-800">
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
                className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold transition-colors ${
                  subTab === id
                    ? 'bg-blue-600 text-white'
                    : 'bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200'
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
      </div>

      {/* TAB 1: AUDIT REPORT & TRACEABILITY */}
      {subTab === 'report' && (
        report ? (
          <div className="space-y-4">
            {/* Summary Metrics */}
            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2.5">
              {[
                { label: 'Chỉ số kỹ thuật', val: `${report.consistency.technicalQualityIndex}/100` },
                { label: 'Bắt buộc (Ma trận)', val: report.ruleProfile.mandatoryCount },
                { label: 'Có điều kiện', val: report.ruleProfile.conditionalCount },
                { label: 'Khuyến nghị', val: report.ruleProfile.recommendedCount },
                { label: 'Lỗi truy xuất', val: report.traceableFindings.length },
                { label: 'Kiểm tra chéo / Thiếu', val: report.advancedIssues.length },
              ].map((m) => (
                <div key={m.label} className="mw-panel p-3 text-center">
                  <div className="text-lg font-extrabold">{m.val}</div>
                  <div className="text-[11px] text-slate-500">{m.label}</div>
                </div>
              ))}
            </div>

            {/* Post-Fix Recheck Banner + Export Bar */}
            <div className="mw-panel p-3.5 flex flex-wrap items-center justify-between gap-3">
              <div className="text-xs space-y-0.5">
                <div className="font-bold">
                  {report.fileInfo.fileName} · Loại: {report.fileInfo.detectedDocType} · SHA-256:{' '}
                  <span className="font-mono text-[11px]">{report.fileInfo.docSha256.slice(0, 12)}…</span>
                </div>
                <div className="text-[11px] text-slate-500">
                  Đồng nhất phông: {report.consistency.fontUniformityPct}% · Cỡ chữ: {report.consistency.sizeUniformityPct}% · Khoảng cách: {report.consistency.spacingUniformityPct}% · Rule Pack v{report.ruleProfile.packVersion}
                </div>
                {report.recheck && (
                  <div className="text-[11px] font-semibold text-emerald-700 dark:text-emerald-400">
                    Hậu kiểm Auto-Fix ({report.recheck.recheckedAt}): {report.recheck.beforeFindings} -&gt; {report.recheck.afterFindings} lỗi · Đã sửa: {report.recheck.fixedCount} · Bỏ qua: {report.recheck.skippedCount} · Bảo toàn: {report.recheck.preservationPassed ? 'Đạt 100%' : 'Cảnh báo'}
                  </div>
                )}
              </div>

              <div className="flex flex-wrap gap-2">
                <button type="button" onClick={() => void handleExport('docx')} className="mw-button mw-button-secondary">
                  <Download className="w-3.5 h-3.5" /> Xuất DOCX
                </button>
                <button type="button" onClick={() => void handleExport('pdf')} className="mw-button mw-button-secondary">
                  <FileText className="w-3.5 h-3.5 text-rose-500" /> Xuất PDF
                </button>
                <button type="button" onClick={() => void handleExport('csv')} className="mw-button mw-button-secondary">
                  <FileSpreadsheet className="w-3.5 h-3.5 text-emerald-600" /> Xuất CSV lỗi
                </button>
                <button type="button" onClick={() => void handleExport('json')} className="mw-button mw-button-secondary">
                  <Download className="w-3.5 h-3.5 text-indigo-500" /> JSON
                </button>
              </div>
            </div>

            {recheckRes?.stdExecution.outputBlob && (
              <div className="mw-panel p-3 flex items-center justify-between gap-2 text-xs">
                <span className="font-semibold text-emerald-700">
                  Tệp đã chuẩn hóa: {recheckRes.stdExecution.outputName || recheckRes.stdExecution.outputPath}
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

            {/* Advanced Cross-Checks, Config Conflicts, Missing Content, Clusters */}
            {(report.advancedIssues.length > 0 || report.repeatedClusters.length > 0) && (
              <div className="mw-panel p-4 space-y-3">
                <div className="text-xs font-bold flex items-center gap-1.5">
                  <AlertTriangle className="w-4 h-4 text-amber-500" />
                  Kiểm tra Chéo, Mâu thuẫn Cấu hình, Nội dung Thiếu &amp; Cụm lỗi lặp
                </div>
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-2.5">
                  {report.advancedIssues.map((adv) => (
                    <div key={adv.issueId} className="rounded-xl border border-slate-200 dark:border-slate-800 p-2.5 text-xs space-y-1">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-bold">[{adv.issueId}] {adv.title}</span>
                        <span className="px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px]">
                          {BASIS_LABEL[adv.basisKind]}
                        </span>
                      </div>
                      <p className="text-[11px] text-slate-600 dark:text-slate-400">{adv.explanation}</p>
                      <div className="text-[10px] text-slate-500">
                        Căn cứ: {adv.legalBasis} · Vị trí: {adv.location} · Đề xuất: {adv.remediation}
                      </div>
                    </div>
                  ))}
                  {report.repeatedClusters.map((clu) => (
                    <div key={clu.clusterId} className="rounded-xl border border-indigo-200 dark:border-indigo-900/60 bg-indigo-50/40 dark:bg-indigo-950/20 p-2.5 text-xs space-y-1">
                      <div className="font-bold">
                        Cụm lặp {clu.ruleId} (v{clu.ruleVersion}) · {clu.occurrenceCount} vị trí
                      </div>
                      <p className="text-[11px] text-slate-600 dark:text-slate-400">{clu.systemicCause}</p>
                      <div className="text-[10px] text-indigo-700 dark:text-indigo-300 font-semibold">
                        Xử lý hàng loạt: {clu.recommendedBatchAction}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Traceable Findings Table */}
            <div className="mw-panel p-4 space-y-2.5">
              <div className="text-xs font-bold flex items-center gap-1.5">
                <AlertCircle className="w-4 h-4 text-blue-600" />
                Bảng Truy xuất Lỗi &amp; Căn cứ ({report.traceableFindings.length})
              </div>
              {report.traceableFindings.length === 0 ? (
                <div className="text-xs text-emerald-600 flex items-center gap-1.5 py-4">
                  <CheckCircle2 className="w-4 h-4" /> Không có lỗi nào phát sinh trên các quy tắc áp dụng.
                </div>
              ) : (
                <div className="max-h-96 overflow-y-auto space-y-2 pr-1">
                  {report.traceableFindings.map((f) => {
                    const ob = OBLIGATION_BADGE[f.obligation];
                    return (
                      <div key={f.findingId} className="rounded-xl border border-slate-200 dark:border-slate-800 p-2.5 text-xs space-y-1">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <div className="flex items-center gap-1.5 flex-wrap">
                            <span className="font-mono font-bold text-blue-600">{f.ruleId} (v{f.ruleVersion})</span>
                            <span className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${ob.cls}`}>{ob.label}</span>
                            <span className="px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px]">
                              {BASIS_LABEL[f.basisKind]}
                            </span>
                            <span className="font-semibold">{f.ruleName}</span>
                          </div>
                          <div className="flex items-center gap-2 text-[11px]">
                            {f.autoFixSupported && f.safetyTier === 'groupBConfirm' && (
                              <label className="inline-flex items-center gap-1 cursor-pointer">
                                <input
                                  type="checkbox"
                                  checked={Boolean(approvedFindingIds[f.findingId])}
                                  onChange={(e) =>
                                    setApprovedFindingIds((p) => ({ ...p, [f.findingId]: e.target.checked }))
                                  }
                                />
                                Duyệt sửa
                              </label>
                            )}
                            <label className="inline-flex items-center gap-1 cursor-pointer text-slate-500">
                              <input
                                type="checkbox"
                                checked={Boolean(skippedFindingIds[f.findingId])}
                                onChange={(e) =>
                                  setSkippedFindingIds((p) => ({ ...p, [f.findingId]: e.target.checked }))
                                }
                              />
                              Bỏ qua
                            </label>
                          </div>
                        </div>
                        <div className="text-[11px] text-slate-600 dark:text-slate-400">
                          Vị trí: <span className="font-medium">{f.location}</span> ({f.elementPath}) · Hiện tại: <span className="font-mono">{f.currentValue}</span> -&gt; Chuẩn: <span className="font-mono">{f.expectedValue}</span>
                        </div>
                        <div className="flex flex-wrap items-center justify-between gap-2 text-[10px] text-slate-500">
                          <span>Căn cứ: {f.legalBasis} · Phương án xử lý: {f.remediationMethod}</span>
                          {onInspectInPreview && selectedPath && (
                            <button
                              type="button"
                              onClick={() => onInspectInPreview(selectedPath, f.elementPath)}
                              className="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-blue-200 dark:border-blue-800 bg-blue-50/70 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300 font-semibold hover:bg-blue-100"
                            >
                              <Eye className="w-3 h-3" />
                              Xem trên A4
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="mw-panel p-8 text-center text-xs text-slate-500 space-y-2">
            <Info className="w-6 h-6 mx-auto text-blue-500" />
            <p>Chọn tài liệu Word phía trên và bấm <b>Kiểm toán</b> để xem ma trận tuân thủ, truy xuất lỗi và báo cáo chi tiết.</p>
          </div>
        )
      )}

      {/* TAB 2: COMPLIANCE MATRIX */}
      {subTab === 'matrix' && (
        <div className="mw-panel p-4 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="text-xs font-bold">
              Ma trận Quy tắc Tuân thủ ({filteredMatrix.length} quy tắc)
            </div>
            <div className="flex flex-wrap gap-2">
              <select
                value={matrixFilterObl}
                onChange={(e) => setMatrixFilterObl(e.target.value)}
                className="rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2 py-1 text-xs"
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
                className="rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2 py-1 text-xs"
              >
                <option value="all">Mọi thành phần thể thức</option>
                {componentOptions.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
              <button type="button" onClick={() => void handleLoadPreviewMatrix()} className="mw-button mw-button-secondary">
                <RefreshCw className="w-3.5 h-3.5" /> Cập nhật Ma trận
              </button>
            </div>
          </div>

          <div className="max-h-[420px] overflow-y-auto space-y-2 pr-1">
            {filteredMatrix.map((row) => {
              const ob = OBLIGATION_BADGE[row.obligation];
              return (
                <div key={row.ruleId} className="rounded-xl border border-slate-200 dark:border-slate-800 p-2.5 text-xs space-y-1">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span className="font-mono font-bold text-blue-600">{row.ruleId} v{row.ruleVersion}</span>
                      <span className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${ob.cls}`}>{ob.label}</span>
                      <span className="px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px]">{row.formComponent}</span>
                      <span className="font-semibold">{row.ruleName}</span>
                    </div>
                    <span className="text-[11px] font-semibold">
                      {row.evaluationStatus ?? 'Chưa quét tệp'} ({row.findingCount} lỗi)
                    </span>
                  </div>
                  <div className="text-[11px] text-slate-600 dark:text-slate-400">
                    {BASIS_LABEL[row.basisKind]}: {row.legalBasis}
                  </div>
                  <div className="text-[10px] text-slate-500">
                    Điều kiện: {row.conditionDetail} · Tổ chức: {row.orgConfigNote}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* TAB 3: RULE PACK VERSIONING */}
      {subTab === 'rulepack' && rulePack && (
        <div className="mw-panel p-4 space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <div className="text-xs font-bold">{rulePack.packName}</div>
              <div className="text-[11px] text-slate-500">{rulePack.legalReferenceSummary}</div>
            </div>
            <div className="flex items-center gap-2">
              <button type="button" onClick={() => void handleResetRulePack()} className="mw-button mw-button-secondary">
                <RotateCcw className="w-3.5 h-3.5" /> Khôi phục gốc
              </button>
              <button type="button" onClick={() => void handleSaveRulePack()} className="mw-button mw-button-primary">
                <Save className="w-3.5 h-3.5" /> Lưu Phiên bản Quy tắc
              </button>
            </div>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 text-xs">
            <div>
              <label className="block font-semibold mb-1">Phiên bản Bộ Quy tắc (packVersion)</label>
              <input
                type="text"
                value={rulePack.packVersion}
                onChange={(e) => setRulePack({ ...rulePack, packVersion: e.target.value })}
                className="w-full rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2.5 py-1.5 text-xs"
              />
            </div>
            <div>
              <label className="block font-semibold mb-1">Ngày hiệu lực văn bản căn cứ</label>
              <input
                type="text"
                value={rulePack.effectiveDate}
                onChange={(e) => setRulePack({ ...rulePack, effectiveDate: e.target.value })}
                className="w-full rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-2.5 py-1.5 text-xs"
              />
            </div>
          </div>

          <div className="max-h-80 overflow-y-auto space-y-1.5 pr-1">
            {rulePack.rules.map((r, idx) => (
              <div key={r.ruleId} className="rounded-lg border border-slate-200 dark:border-slate-800 p-2 text-xs grid grid-cols-1 sm:grid-cols-12 gap-2 items-center">
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
                    className="w-full rounded border border-slate-300 dark:border-slate-700 px-1.5 py-0.5 text-[11px] font-mono"
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
                    className="w-full rounded border border-slate-300 dark:border-slate-700 px-1.5 py-0.5 text-[11px]"
                  />
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* TAB 4: PRIVACY-SAFE AUDIT HISTORY & COMPARISON */}
      {subTab === 'history' && (
        <div className="space-y-4">
          <div className="mw-panel p-4 space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <div className="text-xs font-bold">Lịch sử Kiểm toán (Ẩn danh nội dung tài liệu)</div>
                <div className="text-[11px] text-slate-500">Chỉ lưu mã SHA-256 phiên bản tệp, mã quy tắc và đường dẫn thẻ XML; không lưu trích đoạn nội dung nhạy cảm.</div>
              </div>
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
              <div className="rounded-xl border border-slate-200 dark:border-slate-800 p-3 grid grid-cols-1 md:grid-cols-3 gap-2 items-end">
                <select
                  value={compareOlderId}
                  onChange={(e) => setCompareOlderId(e.target.value)}
                  className="rounded-lg border border-slate-300 dark:border-slate-700 px-2 py-1.5 text-xs"
                >
                  <option value="">-- Chọn lần kiểm tra trước --</option>
                  {history.map((h) => (
                    <option key={h.auditId} value={h.auditId}>
                      {h.fileName} ({h.checkedAt})
                    </option>
                  ))}
                </select>
                <select
                  value={compareNewerId}
                  onChange={(e) => setCompareNewerId(e.target.value)}
                  className="rounded-lg border border-slate-300 dark:border-slate-700 px-2 py-1.5 text-xs"
                >
                  <option value="">-- Chọn lần kiểm tra sau --</option>
                  {history.map((h) => (
                    <option key={h.auditId} value={h.auditId}>
                      {h.fileName} ({h.checkedAt})
                    </option>
                  ))}
                </select>
                <button type="button" onClick={() => void handleCompareHistory()} className="mw-button mw-button-primary justify-center">
                  <GitCompare className="w-3.5 h-3.5" /> So sánh 2 lần kiểm tra
                </button>
              </div>
            )}

            {comparison && (
              <div className="rounded-xl border border-blue-200 dark:border-blue-800 bg-blue-50/40 dark:bg-blue-950/20 p-3 text-xs space-y-1.5">
                <div className="font-bold">
                  Đối chiếu: {comparison.olderCheckedAt} -&gt; {comparison.newerCheckedAt} · Điểm kỹ thuật {comparison.qualityIndexDelta >= 0 ? `+${comparison.qualityIndexDelta}` : comparison.qualityIndexDelta}
                </div>
                <div className="text-[11px]">
                  Đã khắc phục: <b>{comparison.resolvedRuleLocations.length}</b> · Còn tồn tại: <b>{comparison.persistentRuleLocations.length}</b> · Mới phát sinh: <b>{comparison.newlyDetectedRuleLocations.length}</b> · {comparison.sameDocumentHash ? 'Cùng phiên bản SHA-256' : 'Tài liệu đã thay đổi SHA-256'}
                </div>
                {comparison.ruleVersionDiffs.length > 0 && (
                  <div className="text-[11px] text-indigo-700">Thay đổi phiên bản quy tắc: {comparison.ruleVersionDiffs.join(', ')}</div>
                )}
              </div>
            )}

            <div className="max-h-80 overflow-y-auto space-y-2 pr-1">
              {history.map((h) => (
                <div key={h.auditId} className="rounded-xl border border-slate-200 dark:border-slate-800 p-2.5 text-xs flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <div className="font-bold">
                      {h.fileName} · <span className="font-mono text-[11px] text-blue-600">{h.docVersionLabel}</span>
                    </div>
                    <div className="text-[11px] text-slate-500">
                      {h.checkedAt} · Rule Pack v{h.packVersion} · Phát hiện: {h.detectedFindings.length} | Đã sửa: {h.fixedFindingIds.length} | Bỏ qua: {h.skippedFindingIds.length} · Điểm KT: {h.technicalQualityIndex}/100
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={async () => {
                      if (isNative) {
                        const updated = await bridge.invoke<AuditHistoryEntry[]>('delete_audit_history_entry', { auditId: h.auditId });
                        setHistory(updated);
                      } else {
                        setHistory(deleteWebAuditHistoryEntry(h.auditId));
                      }
                    }}
                    className="p-1.5 text-slate-400 hover:text-rose-500"
                    title="Xóa bản ghi này"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
