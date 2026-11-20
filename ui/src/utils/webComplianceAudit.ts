import defaultRulePackJson from '../data/complianceRulePack.json';
import type {
  AdvancedAuditIssue,
  AppliedRuleRecord,
  AuditHistoryEntry,
  AuditRecheckSummary,
  AuditRunComparison,
  ComplianceAuditReport,
  ComplianceAutoFixAndRecheckResult,
  ComplianceBasisKind,
  ComplianceMatrixRow,
  ComplianceObligation,
  ComplianceRulePack,
  DocumentConsistencyMetrics,
  FileQueueItem,
  FindingDisposition,
  OrganizationProfile,
  RedactedFindingSummary,
  RepeatedErrorCluster,
  RuleVersionEntry,
  StdDocScope,
  StdExecutionRequest,
  TraceableFinding,
} from '../types';
import { writeZipEntries } from './webDocxEngine';
import { analyzeWebLayoutIntelligence } from './webLayoutIntelligence';
import { analyzeWebStdDocument, executeWebStdDocument, getWebStdRules } from './webStandardizer';

export const NON_LEGAL_DISCLAIMER_VI =
  'Lưu ý giới hạn phạm vi: Báo cáo chỉ phản ánh kết quả kiểm tra kỹ thuật trình bày, bố cục và thể thức OpenXML theo bộ quy tắc cấu hình tại thời điểm kiểm tra; KHÔNG phải là kết luận thẩm định tính hợp pháp, thẩm quyền ban hành hay giá trị pháp lý của văn bản.';

const RULE_PACK_STORAGE_KEY = 'monword_compliance_rule_pack_v1';
const HISTORY_STORAGE_KEY = 'monword_audit_history_v1';

export function getWebRulePack(): ComplianceRulePack {
  try {
    const raw = localStorage.getItem(RULE_PACK_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as ComplianceRulePack;
      if (parsed && Array.isArray(parsed.rules) && parsed.rules.length > 0) {
        return parsed;
      }
    }
  } catch {
    // ignore storage parse failure
  }
  return defaultRulePackJson as unknown as ComplianceRulePack;
}

export function saveWebRulePack(pack: ComplianceRulePack): ComplianceRulePack {
  localStorage.setItem(RULE_PACK_STORAGE_KEY, JSON.stringify(pack));
  return pack;
}

export function resetWebRulePack(): ComplianceRulePack {
  localStorage.removeItem(RULE_PACK_STORAGE_KEY);
  return defaultRulePackJson as unknown as ComplianceRulePack;
}

export function listWebAuditHistory(): AuditHistoryEntry[] {
  try {
    const raw = localStorage.getItem(HISTORY_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as AuditHistoryEntry[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function clearWebAuditHistory(): void {
  localStorage.removeItem(HISTORY_STORAGE_KEY);
}

export function deleteWebAuditHistoryEntry(auditId: string): AuditHistoryEntry[] {
  const next = listWebAuditHistory().filter((e) => e.auditId !== auditId);
  localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(next));
  return next;
}

function scopeKey(scope: StdDocScope): string {
  switch (scope) {
    case 'adminNd30':
    case 'autoDetect':
      return 'adminNd30';
    case 'partyDocument':
      return 'partyDocument';
    case 'enterpriseInternal':
      return 'enterpriseInternal';
    case 'specializedLegal':
      return 'specializedLegal';
    case 'academicThesis':
      return 'academicThesis';
    default:
      return 'freeform';
  }
}

function docScopeLabelVi(scope: StdDocScope): string {
  switch (scope) {
    case 'autoDetect':
      return 'Tự động nhận diện';
    case 'adminNd30':
      return 'Văn bản hành chính (NĐ 30/2020/NĐ-CP)';
    case 'enterpriseInternal':
      return 'Văn bản nội bộ Doanh nghiệp';
    case 'specializedLegal':
      return 'Văn bản quy phạm / chuyên ngành';
    case 'academicThesis':
      return 'Luận văn / Báo cáo học thuật';
    case 'partyDocument':
      return 'Văn bản Đảng (36-HD/VPTW)';
    case 'freeform':
      return 'Tài liệu tự do';
    case 'unknownRequiresSelection':
      return 'Chưa xác định phạm vi';
  }
}

async function computeSha256Hex(file: File): Promise<string> {
  const buf = await file.arrayBuffer();
  const hashBuf = await crypto.subtle.digest('SHA-256', buf);
  return Array.from(new Uint8Array(hashBuf))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

export function buildWebComplianceMatrix(
  pack: ComplianceRulePack,
  scope: StdDocScope,
  docType: string,
  orgProfile: OrganizationProfile | null,
  disabledRuleIds: string[] = [],
  stdSummaries?: { ruleId: string; status: string; findingCount: number }[],
  estPages = 2,
  dataTableCount = 1,
): ComplianceMatrixRow[] {
  const catalog = getWebStdRules();
  const packMap = new Map<string, RuleVersionEntry>(pack.rules.map((r) => [r.ruleId, r]));
  const evalMap = new Map(stdSummaries?.map((s) => [s.ruleId, s]) ?? []);
  const disabledSet = new Set(disabledRuleIds);
  const skey = scopeKey(scope);
  const entNd30 = orgProfile?.sector === 'enterprise' && orgProfile.useNd30LayoutForEnterprise;

  return catalog.map((desc) => {
    const meta = packMap.get(desc.ruleId);
    const enabled = !disabledSet.has(desc.ruleId) && (meta?.enabled ?? true);
    let obligation: ComplianceObligation = !enabled
      ? 'notApplicable'
      : scope === 'unknownRequiresSelection'
      ? ['FONT-01', 'FONT-05', 'PAGE-03', 'PGNUM-02', 'PRESERVE-01'].includes(desc.ruleId)
        ? meta?.defaultObligations?.[skey] ?? 'recommended'
        : 'conditional'
      : meta?.defaultObligations?.[skey] ?? 'notApplicable';

    if (
      enabled &&
      scope === 'enterpriseInternal' &&
      !entNd30 &&
      ['PAGE-02', 'PARA-01', 'PARA-02', 'PGNUM-01', 'CAP-01', 'ABBR-01', 'STRUCT-01'].includes(desc.ruleId)
    ) {
      obligation = 'notApplicable';
    }

    const isDocExcepted = meta?.docTypeExceptions?.some((ex) => ex.toLowerCase() === docType.toLowerCase()) ?? false;
    let isConditionMet: boolean | null = null;
    if (obligation === 'notApplicable') isConditionMet = false;
    else if (obligation === 'conditional') {
      if (desc.ruleId === 'PGNUM-01') isConditionMet = estPages >= 2;
      else if (desc.ruleId === 'TBL-01' || desc.ruleId === 'TBL-03') isConditionMet = dataTableCount > 0;
      else if (desc.ruleId === 'STRUCT-01' || desc.ruleId === 'ABBR-01') isConditionMet = !isDocExcepted;
      else isConditionMet = scope === 'enterpriseInternal' ? Boolean(entNd30) : true;
    }

    const ev = evalMap.get(desc.ruleId);
    return {
      ruleId: desc.ruleId,
      ruleName: desc.ruleName,
      ruleVersion: meta?.ruleVersion ?? '1.0.0',
      formatGroup: desc.category,
      formComponent: meta?.formComponent ?? 'general',
      basisKind: (meta?.basisKind ?? 'technicalRecommendation') as ComplianceBasisKind,
      legalBasis: meta?.legalCitation || desc.legalBasis,
      applicableDocTypes: desc.applicableDocumentTypes.map(docScopeLabelVi),
      scope,
      orgConfigNote: orgProfile ? `Hồ sơ "${orgProfile.profileName}"` : 'Mặc định hệ thống',
      obligation,
      conditionDetail: meta?.conditionDescription ?? desc.description,
      isConditionMetOnDoc: isConditionMet,
      evaluationStatus: (ev?.status as ComplianceMatrixRow['evaluationStatus']) ?? null,
      findingCount: ev?.findingCount ?? 0,
      autoFixSupported: desc.autoFixSupported,
    };
  });
}

export async function runWebComplianceAudit(
  item: FileQueueItem,
  scope: StdDocScope,
  orgProfile: OrganizationProfile | null,
  docTypeHint: string | null = null,
  disabledRuleIds: string[] = [],
  recordHistory = true,
  recheckMeta?: {
    beforeFailedRules: number;
    beforeFindings: number;
    fixedCount: number;
    skippedCount: number;
    preservationPassed: boolean;
    beforeAnomalies: number;
  },
): Promise<ComplianceAuditReport> {
  if (!item.webFile) throw new Error('Không tìm thấy tệp trong trình duyệt.');
  const sha256 = await computeSha256Hex(item.webFile);
  const stdReport = await analyzeWebStdDocument(item, scope, orgProfile);
  const layoutReport = await analyzeWebLayoutIntelligence(item.webFile, stdReport.effectiveScope);

  const pack = getWebRulePack();
  const effectiveDisabled = Array.from(
    new Set([...disabledRuleIds, ...pack.rules.filter((r) => !r.enabled).map((r) => r.ruleId)]),
  );
  const detectedDocType = docTypeHint?.trim()
    ? docTypeHint.trim()
    : layoutReport.maps.documentMap.some((d) => d.kind === 'docTypeTitle')
    ? 'quyetDinh'
    : 'vanBanChung';

  const dataTables = layoutReport.maps.tableMap.filter((t) => t.tableRole === 'dataTable').length;
  const estPages = Math.max(1, layoutReport.maps.pageLayoutMap.length);

  const matrix = buildWebComplianceMatrix(
    pack,
    stdReport.effectiveScope,
    detectedDocType,
    orgProfile,
    effectiveDisabled,
    stdReport.ruleSummaries.map((s) => ({
      ruleId: s.ruleId,
      status: s.status,
      findingCount: s.findingCount,
    })),
    estPages,
    dataTables,
  );

  const matrixById = new Map(matrix.map((m) => [m.ruleId, m]));
  const packById = new Map(pack.rules.map((r) => [r.ruleId, r]));

  const traceableFindings: TraceableFinding[] = stdReport.findings
    .filter((f) => !effectiveDisabled.includes(f.ruleId))
    .map((f) => {
      const m = matrixById.get(f.ruleId);
      const p = packById.get(f.ruleId);
      return {
        findingId: f.findingId,
        ruleId: f.ruleId,
        ruleName: f.ruleName,
        ruleVersion: m?.ruleVersion ?? '1.0.0',
        packVersion: pack.packVersion,
        legalBasis: m?.legalBasis ?? f.legalBasis,
        basisKind: m?.basisKind ?? 'technicalRecommendation',
        obligation: m?.obligation ?? 'recommended',
        applicableScope: docScopeLabelVi(stdReport.effectiveScope),
        formatGroup: f.category,
        formComponent: m?.formComponent ?? 'general',
        severity: f.severity,
        location: f.location,
        elementPath: f.elementPath,
        checkResult: f.status,
        currentValue: f.currentValue,
        expectedValue: f.expectedValue,
        remediationMethod: p?.remediationGuidance
          ? `${f.proposal} (${p.remediationGuidance})`
          : f.proposal,
        safetyTier: f.safetyTier,
        autoFixSupported: f.autoFixSupported,
        disposition: f.disposition,
      };
    });

  // Cross-checks & Advanced issues from IR + Layout Map
  const advancedIssues: AdvancedAuditIssue[] = [];
  if (
    ['adminNd30', 'partyDocument'].includes(stdReport.effectiveScope) &&
    estPages >= 2 &&
    !layoutReport.maps.headerFooterMap.some((hf) => hf.hasPageField)
  ) {
    advancedIssues.push({
      issueId: 'XCHK-04',
      kind: 'crossComponentMismatch',
      severity: 'warning',
      basisKind: 'legalRegulation',
      legalBasis: 'Khoản 3 Điều 9 & Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP',
      title: 'Tài liệu nhiều trang ước tính nhưng chưa có trường số trang PAGE',
      explanation: `Ước tính tài liệu dài ${estPages} trang nhưng Header/Footer chưa có trường PAGE.`,
      location: 'Header/Footer ↔ PageLayoutMap',
      elementPath: 'word/header*.xml',
      remediation: 'Bổ sung đánh số trang giữa trên (13–14pt, bỏ qua trang đầu).',
    });
  }

  if (orgProfile?.sector === 'enterprise' && !orgProfile.useNd30LayoutForEnterprise && scope === 'adminNd30') {
    advancedIssues.push({
      issueId: 'CCFG-01',
      kind: 'configurationConflict',
      severity: 'warning',
      basisKind: 'organizationPolicy',
      legalBasis: 'Hồ sơ Tổ chức ↔ Phạm vi Kiểm chuẩn',
      title: 'Mâu thuẫn Hồ sơ Doanh nghiệp (tắt NĐ30) với phạm vi AdminNd30',
      explanation: `Hồ sơ "${orgProfile.profileName}" tắt tương thích NĐ30 nhưng lại kiểm tra theo AdminNd30.`,
      location: 'Cấu hình Tổ chức',
      elementPath: 'config/orgProfile',
      remediation: 'Đổi phạm vi sang EnterpriseInternal hoặc bật tương thích NĐ30.',
    });
  }

  const overflowCount =
    layoutReport.maps.tableMap.filter((t) => t.tableRole === 'dataTable' && t.overflowsWidth).length +
    layoutReport.maps.visualObjects.filter((v) => v.overflowsPrintableArea).length;
  if (overflowCount > 0) {
    advancedIssues.push({
      issueId: 'ANOM-OVERFLOW',
      kind: 'structuralAnomaly',
      severity: 'error',
      basisKind: 'technicalRecommendation',
      legalBasis: 'Bố cục vùng in Layout Intelligence (Phase 5)',
      title: `${overflowCount} đối tượng bảng/ảnh tràn vùng in`,
      explanation: 'Bảng dữ liệu hoặc ảnh vượt quá lề in khả dụng của Section.',
      location: 'Layout Map',
      elementPath: 'body',
      remediation: 'Sử dụng Phân tích Bố cục để co bảng/ảnh vừa khổ in.',
    });
  }

  // Repeated error clusters
  const groups = new Map<string, { ruleId: string; ruleName: string; expected: string; locs: string[] }>();
  for (const f of traceableFindings) {
    const k = `${f.ruleId}||${f.expectedValue}`;
    const cur = groups.get(k) ?? { ruleId: f.ruleId, ruleName: f.ruleName, expected: f.expectedValue, locs: [] };
    cur.locs.push(f.location);
    groups.set(k, cur);
  }
  const repeatedClusters: RepeatedErrorCluster[] = [];
  let cIdx = 0;
  for (const g of groups.values()) {
    if (g.locs.length >= 3) {
      cIdx += 1;
      repeatedClusters.push({
        clusterId: `CLU-${String(cIdx).padStart(2, '0')}`,
        ruleId: g.ruleId,
        ruleName: g.ruleName,
        ruleVersion: matrixById.get(g.ruleId)?.ruleVersion ?? '1.0.0',
        occurrenceCount: g.locs.length,
        sampleLocations: g.locs.slice(0, 5),
        systemicCause: `Lỗi lặp trên nhiều đoạn cùng hướng chuẩn "${g.expected}".`,
        recommendedBatchAction: `Áp dụng Auto-Fix hàng loạt cho quy tắc ${g.ruleId}.`,
      });
    }
  }

  const mandFails = traceableFindings.filter((f) => f.obligation === 'mandatory').length;
  const recFails = traceableFindings.length - mandFails;
  const techIdx = Math.max(5, 100 - Math.min(95, mandFails * 6 + recFails * 2 + advancedIssues.length * 4));
  const consistency: DocumentConsistencyMetrics = {
    fontUniformityPct: Math.max(0, Math.round((100 - traceableFindings.filter((f) => f.ruleId === 'FONT-02').length * 8) * 10) / 10),
    sizeUniformityPct: Math.max(0, Math.round((100 - traceableFindings.filter((f) => f.ruleId === 'FONT-03').length * 8) * 10) / 10),
    spacingUniformityPct: Math.max(0, Math.round((100 - traceableFindings.filter((f) => f.ruleId === 'PARA-03').length * 8) * 10) / 10),
    headingHierarchyValid: !advancedIssues.some((a) => a.issueId.startsWith('ANOM-H')),
    crossComponentConsistent: !advancedIssues.some((a) => a.kind === 'crossComponentMismatch'),
    technicalQualityIndex: techIdx,
  };

  const nowStr = new Date().toLocaleString('vi-VN');
  const recheck: AuditRecheckSummary | null = recheckMeta
    ? {
        recheckedAt: nowStr,
        outputPath: item.name,
        outputSha256: sha256,
        beforeFailedRules: recheckMeta.beforeFailedRules,
        afterFailedRules: matrix.filter((m) => m.evaluationStatus === 'failed').length,
        beforeFindings: recheckMeta.beforeFindings,
        afterFindings: traceableFindings.length,
        fixedCount: recheckMeta.fixedCount,
        skippedCount: recheckMeta.skippedCount,
        remainingMandatoryFailures: mandFails,
        remainingRecommendedWarnings: recFails,
        newAnomaliesIntroduced: Math.max(0, advancedIssues.length - recheckMeta.beforeAnomalies),
        preservationPassed: recheckMeta.preservationPassed,
      }
    : null;

  const report: ComplianceAuditReport = {
    auditId: `AUD-WEB-${Date.now()}-${sha256.slice(0, 6)}`,
    checkedAt: nowStr,
    legalDisclaimer: NON_LEGAL_DISCLAIMER_VI,
    fileInfo: {
      fileName: item.name,
      fileExt: item.ext,
      fileSizeBytes: item.sizeBytes,
      docSha256: sha256,
      detectedDocType,
      requestedScope: scope,
      effectiveScope: stdReport.effectiveScope,
      sectionCount: stdReport.sections.length,
      estimatedPageCount: estPages,
      paragraphCount: stdReport.paragraphCount,
      tableCount: stdReport.tables.length,
    },
    ruleProfile: {
      packId: pack.packId,
      packVersion: pack.packVersion,
      effectiveDate: pack.effectiveDate,
      orgProfileId: orgProfile?.id ?? null,
      orgProfileName: orgProfile?.profileName ?? null,
      mandatoryCount: matrix.filter((m) => m.obligation === 'mandatory').length,
      conditionalCount: matrix.filter((m) => m.obligation === 'conditional').length,
      recommendedCount: matrix.filter((m) => m.obligation === 'recommended').length,
      notApplicableCount: matrix.filter((m) => m.obligation === 'notApplicable').length,
      disabledRuleIds: effectiveDisabled,
    },
    matrix,
    traceableFindings,
    advancedIssues,
    repeatedClusters,
    consistency,
    recheck,
  };

  if (recordHistory) {
    recordWebHistory(report, [], []);
  }
  return report;
}

export async function executeWebComplianceAutoFixAndRecheck(
  item: FileQueueItem,
  execRequest: StdExecutionRequest,
  docTypeHint: string | null = null,
  recordHistory = true,
): Promise<ComplianceAutoFixAndRecheckResult> {
  const before = await runWebComplianceAudit(
    item,
    execRequest.scope,
    execRequest.orgProfile,
    docTypeHint,
    execRequest.disabledRuleIds,
    false,
  );
  const stdExec = await executeWebStdDocument(item, execRequest);
  let after: ComplianceAuditReport | null = null;
  let historyEntry: AuditHistoryEntry | null = null;

  if (stdExec.success && stdExec.outputBlob) {
    const outFile = new File([stdExec.outputBlob], stdExec.outputName || item.name, {
      type: stdExec.outputBlob.type,
    });
    const afterItem: FileQueueItem = {
      ...item,
      name: outFile.name,
      sizeBytes: outFile.size,
      webFile: outFile,
    };
    after = await runWebComplianceAudit(
      afterItem,
      execRequest.scope,
      execRequest.orgProfile,
      docTypeHint,
      execRequest.disabledRuleIds,
      false,
      {
        beforeFailedRules: before.matrix.filter((m) => m.evaluationStatus === 'failed').length,
        beforeFindings: before.traceableFindings.length,
        fixedCount: stdExec.actionsApplied.length,
        skippedCount: execRequest.skippedFindingIds.length,
        preservationPassed: stdExec.verification?.preservationPassed ?? false,
        beforeAnomalies: before.advancedIssues.length,
      },
    );
    if (recordHistory) {
      historyEntry = recordWebHistory(
        after,
        stdExec.actionsApplied.map((a) => `${a.ruleId}@${a.elementPath}`),
        execRequest.skippedFindingIds,
      );
    }
  } else if (recordHistory) {
    historyEntry = recordWebHistory(before, [], execRequest.skippedFindingIds);
  }

  return { stdExecution: stdExec, auditBefore: before, auditAfter: after, historyEntry };
}

function recordWebHistory(
  report: ComplianceAuditReport,
  fixedIds: string[],
  skippedIds: string[],
): AuditHistoryEntry {
  const entries = listWebAuditHistory();
  const hashes = Array.from(
    new Set(entries.filter((e) => e.fileName === report.fileInfo.fileName).map((e) => e.docSha256)),
  );
  const pos = hashes.indexOf(report.fileInfo.docSha256);
  const verNo = pos >= 0 ? pos + 1 : hashes.length + 1;

  const appliedRules: AppliedRuleRecord[] = report.matrix
    .filter((m) => m.evaluationStatus !== null)
    .map((m) => ({
      ruleId: m.ruleId,
      ruleVersion: m.ruleVersion,
      obligation: m.obligation,
      status: m.evaluationStatus!,
      findingCount: m.findingCount,
    }));

  const detectedFindings: RedactedFindingSummary[] = report.traceableFindings.map((f) => ({
    findingId: f.findingId,
    ruleId: f.ruleId,
    ruleVersion: f.ruleVersion,
    obligation: f.obligation,
    severity: f.severity,
    structuralLocator: f.elementPath || 'document-level',
    disposition: f.disposition as FindingDisposition,
  }));

  const entry: AuditHistoryEntry = {
    auditId: report.auditId,
    checkedAt: report.checkedAt,
    fileName: report.fileInfo.fileName,
    fileSizeBytes: report.fileInfo.fileSizeBytes,
    docSha256: report.fileInfo.docSha256,
    docVersionLabel: `Bản #${verNo} (sha:${report.fileInfo.docSha256.slice(0, 8)})`,
    packId: report.ruleProfile.packId,
    packVersion: report.ruleProfile.packVersion,
    effectiveScope: report.fileInfo.effectiveScope,
    detectedDocType: report.fileInfo.detectedDocType,
    technicalQualityIndex: report.consistency.technicalQualityIndex,
    appliedRules,
    detectedFindings,
    advancedIssueCodes: report.advancedIssues.map((a) => a.issueId),
    fixedFindingIds: fixedIds,
    skippedFindingIds: skippedIds,
    recheck: report.recheck,
  };
  const next = [entry, ...entries].slice(0, 200);
  localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(next));
  return entry;
}

export function compareWebAuditHistory(older: AuditHistoryEntry, newer: AuditHistoryEntry): AuditRunComparison {
  const key = (f: RedactedFindingSummary) => `${f.ruleId}@${f.structuralLocator}`;
  const oldSet = new Set(older.detectedFindings.map(key));
  const newSet = new Set(newer.detectedFindings.map(key));
  const oldVers = new Map(older.appliedRules.map((r) => [r.ruleId, r.ruleVersion]));
  const ruleVersionDiffs: string[] = [];
  for (const r of newer.appliedRules) {
    const ov = oldVers.get(r.ruleId);
    if (ov && ov !== r.ruleVersion) ruleVersionDiffs.push(`${r.ruleId}: v${ov} -> v${r.ruleVersion}`);
  }
  return {
    olderAuditId: older.auditId,
    newerAuditId: newer.auditId,
    olderCheckedAt: older.checkedAt,
    newerCheckedAt: newer.checkedAt,
    sameDocumentHash: older.docSha256 === newer.docSha256,
    olderDocSha256: older.docSha256,
    newerDocSha256: newer.docSha256,
    packVersionChanged: older.packVersion !== newer.packVersion,
    olderPackVersion: older.packVersion,
    newerPackVersion: newer.packVersion,
    qualityIndexDelta: newer.technicalQualityIndex - older.technicalQualityIndex,
    resolvedRuleLocations: Array.from(oldSet).filter((k) => !newSet.has(k)),
    persistentRuleLocations: Array.from(oldSet).filter((k) => newSet.has(k)),
    newlyDetectedRuleLocations: Array.from(newSet).filter((k) => !oldSet.has(k)),
    skippedCountDelta: newer.skippedFindingIds.length - older.skippedFindingIds.length,
    ruleVersionDiffs,
  };
}

export function buildWebReportCsv(report: ComplianceAuditReport): Blob {
  const esc = (v: string) => `"${String(v ?? '').replace(/[\r\n]+/g, ' ').replace(/"/g, '""')}"`;
  const lines = [
    `\uFEFF# ${report.legalDisclaimer}`,
    `# Tep: ${report.fileInfo.fileName} | Thoi diem: ${report.checkedAt} | Bo quy tac: ${report.ruleProfile.packId} v${report.ruleProfile.packVersion} | SHA256: ${report.fileInfo.docSha256}`,
    'ID,MaQuyTac,PhienBanQT,TenQuyTac,NghiaVu,NguonCanCu,CanCu,MucDo,ViTri,DuongDanXML,HienTai,ChuanDeXuat,PhuongAnXuLy,TrangThai',
    ...report.traceableFindings.map((f) =>
      [
        f.findingId,
        f.ruleId,
        f.ruleVersion,
        f.ruleName,
        f.obligation,
        f.basisKind,
        f.legalBasis,
        f.severity,
        f.location,
        f.elementPath,
        f.currentValue,
        f.expectedValue,
        f.remediationMethod,
        f.disposition,
      ]
        .map(esc)
        .join(','),
    ),
  ];
  return new Blob([lines.join('\r\n')], { type: 'text/csv;charset=utf-8' });
}

function xmlEsc(s: string): string {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export async function buildWebReportDocxBlob(report: ComplianceAuditReport): Promise<Blob> {
  const p = (txt: string, bold = false, center = false) =>
    `<w:p><w:pPr><w:jc w:val="${center ? 'center' : 'left'}"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/>${bold ? '<w:b/>' : ''}<w:sz w:val="24"/></w:rPr><w:t xml:space="preserve">${xmlEsc(txt)}</w:t></w:r></w:p>`;
  const tr = (cells: string[], bold = false) =>
    `<w:tr>${cells.map((c) => `<w:tc><w:tcPr><w:tcW w:w="2300" w:type="dxa"/></w:tcPr>${p(c, bold)}</w:tc>`).join('')}</w:tr>`;

  const docXml = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
${p('BÁO CÁO KIỂM TOÁN TUÂN THỦ THỂ THỨC & CHẤT LƯỢNG KỸ THUẬT VĂN BẢN', true, true)}
${p(`Tệp: ${report.fileInfo.fileName} · Thời điểm: ${report.checkedAt} · Rule Pack v${report.ruleProfile.packVersion}`, false, true)}
${p(report.legalDisclaimer, false, false)}
${p(`SHA-256: ${report.fileInfo.docSha256} · Điểm kỹ thuật: ${report.consistency.technicalQualityIndex}/100`, false, false)}
<w:tbl><w:tblPr><w:tblW w:w="9200" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="94A3B8"/><w:left w:val="single" w:sz="4" w:color="94A3B8"/><w:bottom w:val="single" w:sz="4" w:color="94A3B8"/><w:right w:val="single" w:sz="4" w:color="94A3B8"/><w:insideH w:val="single" w:sz="4" w:color="94A3B8"/><w:insideV w:val="single" w:sz="4" w:color="94A3B8"/></w:tblBorders></w:tblPr>
<w:tblGrid><w:gridCol w:w="1800"/><w:gridCol w:w="1800"/><w:gridCol w:w="2800"/><w:gridCol w:w="2800"/></w:tblGrid>
${tr(['Mã (Phiên bản)', 'Nghĩa vụ & Mức', 'Vị trí & Căn cứ', 'Phương án xử lý'], true)}
${report.traceableFindings
  .slice(0, 120)
  .map((f) => tr([`${f.ruleId} (v${f.ruleVersion})`, `${f.obligation} · ${f.severity}`, `${f.location} | ${f.legalBasis}`, f.remediationMethod]))
  .join('')}
</w:tbl>
${p(report.legalDisclaimer, false, false)}
<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701"/></w:sectPr>
</w:body></w:document>`;

  const ctXml = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>`;
  const relsXml = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>`;
  const enc = new TextEncoder();
  const map = new Map<string, Uint8Array>([
    ['[Content_Types].xml', enc.encode(ctXml)],
    ['_rels/.rels', enc.encode(relsXml)],
    ['word/document.xml', enc.encode(docXml)],
  ]);
  const bytes = await writeZipEntries(map);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy], {
    type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  });
}
