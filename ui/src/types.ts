export type PresetId = 'nd30' | 'vbDang' | 'doanhNghiep' | 'luanVan' | 'custom';
export type OutputMode = 'newCopy' | 'overwriteWithBackup';
export type PaperSize = 'a4' | 'a3' | 'letter';
export type QuoteStyle = 'curlyNd30' | 'straightDang' | 'keep';
export type PageNumPosition = 'topCenter' | 'topRight' | 'bottomCenter' | 'bottomRight';
export type OrientationMode = 'keep' | 'allPortrait' | 'allLandscape' | 'rotateRanges';
export type LockMode = 'none' | 'readOnly' | 'commentsOnly' | 'trackedChanges';
export type AuditSeverity = 'error' | 'warning' | 'info';

export type NavTab =
  | 'home'
  | 'oneClick'
  | 'standardizer'
  | 'complianceAudit'
  | 'livePreview'
  | 'layoutIntelligence'
  | 'templateEngine'
  | 'headerBuilder'
  | 'format'
  | 'pageToc'
  | 'cleaner'
  | 'security'
  | 'system'
  | 'history'
  | 'advancedSettings';

export interface RecentDocEntry {
  path: string;
  name: string;
  ext: string;
  updatedAt: string;
  status: 'selected' | 'checked' | 'standardized';
  operation?: 'standardize' | 'audit' | 'template';
  resultStatus?: 'completed' | 'passed' | 'needsFix' | 'failed';
  outputPath?: string;
  backupPath?: string | null;
  summary?: string;
}

export type PlatformEdition = 'win' | 'web' | 'mobile';
export type UILayoutOverride = 'auto' | PlatformEdition;

export interface VersionHistoryEntry {
  version: string;
  date: string;
  notes: string[];
}

export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string;
  codename?: string;
  hasUpdate: boolean;
  releaseDate: string;
  releaseNotes: string[];
  downloadUrl: string;
  isDirectExe?: boolean;
  otaActive?: boolean;
  history: VersionHistoryEntry[];
  source?: string;
  offline?: boolean;
}

export interface UpdateProgressPayload {
  percent: number;
  status: string;
}

export interface AppInfoPayload {
  name: string;
  codename: string;
  embeddedVersion: string;
  effectiveVersion: string;
  otaActive: boolean;
  otaVersion?: string | null;
  otaDir?: string;
  webUrl: string;
}

export interface FormatConfig {
  enabled: boolean;
  fontName: string;
  fontSizePt: number;
  lineSpacing: number;
  spaceBeforePt: number;
  spaceAfterPt: number;
  firstLineIndentCm: number;
  paperSize: PaperSize;
  marginTopCm: number;
  marginBottomCm: number;
  marginLeftCm: number;
  marginRightCm: number;
  protectAdminBlocks: boolean;
  smartTables: boolean;
  tableFontSizePt: number;
  repeatTableHeaders: boolean;
  syncStylesDefault: boolean;
}

export interface CleanerConfig {
  enabled: boolean;
  fixMultipleSpaces: boolean;
  fixPunctuationSpacing: boolean;
  fixSoftBreaks: boolean;
  removeEmptyParagraphs: boolean;
  quoteStyle: QuoteStyle;
  normalizeNd30Dates: boolean;
  convertLegacyEncoding: boolean;
}

export interface PageTocConfig {
  pageNumbersEnabled: boolean;
  pageNumPosition: PageNumPosition;
  pageNumFontSizePt: number;
  pageNumDistanceCm: number;
  skipFirstPage: boolean;
  mirrorOddEven: boolean;
  startAt: number;
  removeExistingPageNumbers: boolean;
  orientationMode: OrientationMode;
  landscapeRanges: string;
  autoSwapBindingMargin: boolean;
  resetPortraitAndRemoveSections: boolean;
  headerText: string;
  footerText: string;
  applyNd30Headings: boolean;
  autoDetectHeadings: boolean;
  insertToc: boolean;
  tocLevels: number;
  tocPageBreakAfter: boolean;
}

export interface SecurityConfig {
  unlockProtection: boolean;
  lockMode: LockMode;
  lockPassword: string;
  scrubPersonalInfo: boolean;
  author: string;
  title: string;
  cleanMacros: boolean;
}

export interface ProcessConfig {
  preset: PresetId;
  outputMode: OutputMode;
  alsoExportPdf: boolean;
  convertLegacyDoc: boolean;
  format: FormatConfig;
  cleaner: CleanerConfig;
  pageToc: PageTocConfig;
  security: SecurityConfig;
}

export interface PresetInfo {
  id: PresetId;
  name: string;
  badge: string;
  description: string;
  config: ProcessConfig;
}

export interface AuditIssue {
  category: string;
  severity: AuditSeverity;
  message: string;
  count: number;
}

export interface AuditReport {
  filePath: string;
  fileName: string;
  fileType: string;
  score: number;
  sectionCount: number;
  tableCount: number;
  paragraphCount: number;
  hasMacros: boolean;
  isProtected: boolean;
  hasLegacyEncoding: boolean;
  issues: AuditIssue[];
}

export interface FileQueueItem {
  path: string;
  name: string;
  ext: string;
  sizeBytes: number;
  webFile?: File;
}

export interface FileProcessResult {
  inputPath: string;
  outputPath: string;
  backupPath: string | null;
  success: boolean;
  statsSummary: string;
  logs: string[];
  error: string | null;
  errorCode?: string | null;
  auditBefore: AuditReport | null;
  auditAfter: AuditReport | null;
  blobUrl?: string;
  outputName?: string;
  outputBlob?: Blob;
}

export interface SystemStatus {
  officeVersion: string | null;
  wordProtectedView: boolean;
  wordStartScreenDisabled: boolean;
  wordSpellCheckDisabled: boolean;
  vbaWarningsLevel: number | null;
  normalDotmPath: string | null;
  wordStartupDir: string;
  excelAddinsDir: string;
  userFontsDir: string;
}

export interface SystemTweaksRequest {
  disableProtectedView?: boolean;
  disableStartScreen?: boolean;
  disableSpellCheck?: boolean;
  enableVbaMacros?: boolean;
  applyNormalDotmNd30?: boolean;
}

export interface AddinItem {
  app: string;
  name: string;
  fileName: string;
  path: string;
  enabled: boolean;
  sizeBytes: number;
}

export interface ProgressPayload {
  phase: 'audit' | 'process';
  current: number;
  total: number;
  file: string;
}

export const DEFAULT_ND30_CONFIG: ProcessConfig = {
  preset: 'nd30',
  outputMode: 'newCopy',
  alsoExportPdf: false,
  convertLegacyDoc: true,
  format: {
    enabled: true,
    fontName: 'Times New Roman',
    fontSizePt: 14,
    lineSpacing: 1.15,
    spaceBeforePt: 0,
    spaceAfterPt: 6,
    firstLineIndentCm: 1.27,
    paperSize: 'a4',
    marginTopCm: 2,
    marginBottomCm: 2,
    marginLeftCm: 3,
    marginRightCm: 2,
    protectAdminBlocks: true,
    smartTables: true,
    tableFontSizePt: 13,
    repeatTableHeaders: true,
    syncStylesDefault: true,
  },
  cleaner: {
    enabled: true,
    fixMultipleSpaces: true,
    fixPunctuationSpacing: true,
    fixSoftBreaks: true,
    removeEmptyParagraphs: true,
    quoteStyle: 'curlyNd30',
    normalizeNd30Dates: true,
    convertLegacyEncoding: true,
  },
  pageToc: {
    pageNumbersEnabled: true,
    pageNumPosition: 'topCenter',
    pageNumFontSizePt: 13,
    pageNumDistanceCm: 1.25,
    skipFirstPage: true,
    mirrorOddEven: false,
    startAt: 1,
    removeExistingPageNumbers: false,
    orientationMode: 'keep',
    landscapeRanges: '',
    autoSwapBindingMargin: true,
    resetPortraitAndRemoveSections: false,
    headerText: '',
    footerText: '',
    applyNd30Headings: false,
    autoDetectHeadings: false,
    insertToc: false,
    tocLevels: 3,
    tocPageBreakAfter: true,
  },
  security: {
    unlockProtection: false,
    lockMode: 'none',
    lockPassword: '',
    scrubPersonalInfo: false,
    author: '',
    title: '',
    cleanMacros: false,
  },
};

export const DEFAULT_PRESETS_FALLBACK: PresetInfo[] = [
  {
    id: 'nd30',
    name: 'Nghị định 30/2020/NĐ-CP',
    badge: 'Chuẩn Nhà nước',
    description:
      'Times New Roman 14pt, lề 2-2-3-2 cm, số trang đầu trang giữa 13pt (bỏ trang 1), ngoặc kép “ ”, chuẩn ngày tháng NĐ30.',
    config: DEFAULT_ND30_CONFIG,
  },
  {
    id: 'vbDang',
    name: 'Văn bản Đảng (36-HD/VPTW)',
    badge: 'Đảng & Đoàn thể',
    description:
      'Times New Roman 14pt, lề 2-2-3-2 cm, số trang giữa trên 14pt (bỏ trang 1), giữ ngoặc kép thẳng " ".',
    config: {
      ...DEFAULT_ND30_CONFIG,
      preset: 'vbDang',
      cleaner: {
        ...DEFAULT_ND30_CONFIG.cleaner,
        quoteStyle: 'straightDang',
        normalizeNd30Dates: false,
      },
      pageToc: {
        ...DEFAULT_ND30_CONFIG.pageToc,
        pageNumFontSizePt: 14,
      },
    },
  },
  {
    id: 'doanhNghiep',
    name: 'Báo cáo & Hợp đồng DN',
    badge: 'Doanh nghiệp',
    description:
      'Times New Roman 13pt, lề 2-2-2.5-2 cm, thụt đầu dòng 1.0 cm, đánh số trang góc dưới phải.',
    config: {
      ...DEFAULT_ND30_CONFIG,
      preset: 'doanhNghiep',
      format: {
        ...DEFAULT_ND30_CONFIG.format,
        fontSizePt: 13,
        firstLineIndentCm: 1.0,
        marginLeftCm: 2.5,
        tableFontSizePt: 12,
      },
      cleaner: {
        ...DEFAULT_ND30_CONFIG.cleaner,
        normalizeNd30Dates: false,
      },
      pageToc: {
        ...DEFAULT_ND30_CONFIG.pageToc,
        pageNumPosition: 'bottomRight',
        pageNumFontSizePt: 12,
        skipFirstPage: false,
      },
    },
  },
  {
    id: 'luanVan',
    name: 'Đề án / Luận văn / Nghiên cứu',
    badge: 'Học thuật',
    description:
      'Times New Roman 13pt, giãn dòng 1.5, lề gáy 3.5 cm, tự nhận diện Heading & chèn Mục lục tự động.',
    config: {
      ...DEFAULT_ND30_CONFIG,
      preset: 'luanVan',
      format: {
        ...DEFAULT_ND30_CONFIG.format,
        fontSizePt: 13,
        lineSpacing: 1.5,
        marginTopCm: 2.5,
        marginBottomCm: 2.5,
        marginLeftCm: 3.5,
        protectAdminBlocks: false,
        tableFontSizePt: 12,
      },
      cleaner: {
        ...DEFAULT_ND30_CONFIG.cleaner,
        normalizeNd30Dates: false,
      },
      pageToc: {
        ...DEFAULT_ND30_CONFIG.pageToc,
        pageNumPosition: 'bottomCenter',
        applyNd30Headings: true,
        autoDetectHeadings: true,
        insertToc: true,
        tocLevels: 3,
      },
    },
  },
];

// ============================================================================
// PHASE 1 & PHASE 2: ORGANIZATION PROFILE + DOCUMENT HEADER BUILDER
// ============================================================================

export type OrgSector = 'stateAgency' | 'partyOrg' | 'enterprise' | 'other';

export interface OrganizationProfile {
  id: string;
  profileName: string;
  sector: OrgSector;
  governingOrg: string;
  issuingOrg: string;
  affiliatedUnit: string;
  departmentOrBranch: string;
  agencyAbbreviation: string;
  defaultLocation: string;
  useNd30LayoutForEnterprise: boolean;
  showGoverningOrgDefault: boolean;
  showAffiliatedUnitDefault: boolean;
  showDepartmentDefault: boolean;
  updatedAt: string;
}

export type AdminDocType =
  | 'congVan'
  | 'quyetDinh'
  | 'toTrinh'
  | 'baoCao'
  | 'thongBao'
  | 'keHoach'
  | 'bienBan'
  | 'chiThi'
  | 'nghiQuyet'
  | 'deAn'
  | 'quyChe'
  | 'hopDong'
  | 'vanBanNoiBo';

export type RuleObligation =
  | 'mandatory'
  | 'docTypeConditional'
  | 'caseSpecific'
  | 'orgConfigurable'
  | 'recommended';

export interface HeaderRuleMeta {
  ruleId: string;
  title: string;
  description: string;
  obligation: RuleObligation;
  scope: string;
  legalBasis: string;
}

export type HeaderNumberMode = 'keepExisting' | 'manualInput';
export type HeaderDateMode = 'keepExisting' | 'blankDraft' | 'useToday' | 'manualDate';

export interface HeaderBuilderConfig {
  orgProfileId: string | null;
  docType: AdminDocType;
  outputMode: OutputMode;
  showGoverningOrg: boolean;
  showIssuingOrg: boolean;
  showAffiliatedUnit: boolean;
  showDepartment: boolean;
  showOrgUnderline: boolean;
  showNationalHeader: boolean;
  customNationalHeader: string;
  customMotto: string;
  showMottoUnderline: boolean;
  showNumberSymbol: boolean;
  numberMode: HeaderNumberMode;
  docNumber: string;
  docSymbol: string;
  allowOverrideExistingNumber: boolean;
  allowOverrideExistingOrg: boolean;
  showLocationDate: boolean;
  locationOverride: string;
  dateMode: HeaderDateMode;
  manualDay: string;
  manualMonth: string;
  manualYear: string;
  allowOverrideExistingDate: boolean;
  showDocTypeAndSubject: boolean;
  customDocTypeLabel: string;
  subjectText: string;
  summaryLine: string;
  classificationMark: string;
  selectedCandidateIndex: number | null;
}

export type HeaderClassification =
  | 'valid'
  | 'needsStandardization'
  | 'missingComponents'
  | 'dataConflict'
  | 'uncertain';

export type HeaderLayoutKind =
  | 'none'
  | 'tableLayout'
  | 'paragraphLayout'
  | 'mixedTableAndParagraphs'
  | 'customTemplateWithLogo';

export interface DetectedHeaderFields {
  governingOrg: string;
  issuingOrg: string;
  affiliatedUnit: string;
  department: string;
  nationalHeader: string;
  motto: string;
  hasOrgLine: boolean;
  hasMottoLine: boolean;
  numberRaw: string;
  numberValue: string;
  symbolValue: string;
  location: string;
  dateRaw: string;
  day: string;
  month: string;
  year: string;
  docTypeLabel: string;
  subject: string;
  summaryLine: string;
  classificationMark: string;
}

export interface HeaderCandidateSummary {
  candidateIndex: number;
  layoutKind: HeaderLayoutKind;
  bodyStartIndex: number;
  bodyEndIndex: number;
  hasLogoOrImage: boolean;
  previewSnippet: string;
  fields: DetectedHeaderFields;
}

export interface HeaderFinding {
  ruleId: string;
  severity: AuditSeverity;
  obligation: RuleObligation;
  message: string;
  legalBasis: string;
}

export interface HeaderInspectionResult {
  filePath: string;
  fileName: string;
  classification: HeaderClassification;
  classificationLabel: string;
  sectionCount: number;
  hasHeaderFooter: boolean;
  hasMacros: boolean;
  candidates: HeaderCandidateSummary[];
  activeCandidateIndex: number | null;
  findings: HeaderFinding[];
  applicableRules: HeaderRuleMeta[];
  canCreateNew: boolean;
  canStandardize: boolean;
  requiresUserChoice: boolean;
}

export type HeaderActionMode = 'standardizeExisting' | 'createNew';

export interface HeaderApplyResult {
  inputPath: string;
  outputPath: string;
  initialBackupPath: string | null;
  success: boolean;
  classificationBefore: HeaderClassification;
  classificationAfter: HeaderClassification;
  logs: string[];
  warnings: string[];
  error: string | null;
  errorCode?: string | null;
  inspectionAfter: HeaderInspectionResult | null;
  blobUrl?: string;
  outputName?: string;
  outputBlob?: Blob;
}

export const ADMIN_DOC_TYPE_META: {
  id: AdminDocType;
  label: string;
  docTypeHeading: string;
  symbolPrefix: string;
  subjectInLeftCell: boolean;
}[] = [
  { id: 'congVan', label: 'Công văn', docTypeHeading: 'CÔNG VĂN', symbolPrefix: 'CV', subjectInLeftCell: true },
  { id: 'quyetDinh', label: 'Quyết định', docTypeHeading: 'QUYẾT ĐỊNH', symbolPrefix: 'QĐ', subjectInLeftCell: false },
  { id: 'toTrinh', label: 'Tờ trình', docTypeHeading: 'TỜ TRÌNH', symbolPrefix: 'TTr', subjectInLeftCell: false },
  { id: 'baoCao', label: 'Báo cáo', docTypeHeading: 'BÁO CÁO', symbolPrefix: 'BC', subjectInLeftCell: false },
  { id: 'thongBao', label: 'Thông báo', docTypeHeading: 'THÔNG BÁO', symbolPrefix: 'TB', subjectInLeftCell: false },
  { id: 'keHoach', label: 'Kế hoạch', docTypeHeading: 'KẾ HOẠCH', symbolPrefix: 'KH', subjectInLeftCell: false },
  { id: 'bienBan', label: 'Biên bản', docTypeHeading: 'BIÊN BẢN', symbolPrefix: 'BB', subjectInLeftCell: false },
  { id: 'chiThi', label: 'Chỉ thị', docTypeHeading: 'CHỈ THỊ', symbolPrefix: 'CT', subjectInLeftCell: false },
  { id: 'nghiQuyet', label: 'Nghị quyết', docTypeHeading: 'NGHỊ QUYẾT', symbolPrefix: 'NQ', subjectInLeftCell: false },
  { id: 'deAn', label: 'Đề án', docTypeHeading: 'ĐỀ ÁN', symbolPrefix: 'ĐA', subjectInLeftCell: false },
  { id: 'quyChe', label: 'Quy chế', docTypeHeading: 'QUY CHẾ', symbolPrefix: 'QC', subjectInLeftCell: false },
  { id: 'hopDong', label: 'Hợp đồng', docTypeHeading: 'HỢP ĐỒNG', symbolPrefix: 'HĐ', subjectInLeftCell: false },
  { id: 'vanBanNoiBo', label: 'Văn bản nội bộ', docTypeHeading: 'VĂN BẢN NỘI BỘ', symbolPrefix: 'VB', subjectInLeftCell: false },
];

export const DEFAULT_HEADER_BUILDER_CONFIG: HeaderBuilderConfig = {
  orgProfileId: null,
  docType: 'congVan',
  outputMode: 'newCopy',
  showGoverningOrg: false,
  showIssuingOrg: true,
  showAffiliatedUnit: false,
  showDepartment: false,
  showOrgUnderline: true,
  showNationalHeader: true,
  customNationalHeader: '',
  customMotto: '',
  showMottoUnderline: true,
  showNumberSymbol: true,
  numberMode: 'keepExisting',
  docNumber: '',
  docSymbol: '',
  allowOverrideExistingNumber: false,
  allowOverrideExistingOrg: false,
  showLocationDate: true,
  locationOverride: '',
  dateMode: 'keepExisting',
  manualDay: '',
  manualMonth: '',
  manualYear: '',
  allowOverrideExistingDate: false,
  showDocTypeAndSubject: true,
  customDocTypeLabel: '',
  subjectText: '',
  summaryLine: '',
  classificationMark: '',
  selectedCandidateIndex: null,
};

// ============================================================================
// PHASE 3: DOCUMENT STANDARDIZATION ENGINE TYPES
// ============================================================================

export type StdDocScope =
  | 'autoDetect'
  | 'adminNd30'
  | 'enterpriseInternal'
  | 'specializedLegal'
  | 'academicThesis'
  | 'partyDocument'
  | 'freeform'
  | 'unknownRequiresSelection';

export type RuleCategory =
  | 'pageAndSection'
  | 'fontAndEncoding'
  | 'paragraphAndAlignment'
  | 'headingAndStructure'
  | 'tableLayout'
  | 'pageNumberAndHeaderFooter'
  | 'capitalizationAndAbbreviation'
  | 'preservationAndIntegrity';

export type StdSeverity = 'critical' | 'error' | 'warning' | 'information';

export type RuleCheckStatus =
  | 'passed'
  | 'failed'
  | 'needsReview'
  | 'notApplicable'
  | 'indeterminate';

export type FixSafetyTier = 'groupAAuto' | 'groupBConfirm' | 'groupCManualOnly';

export type FindingDisposition =
  | 'pending'
  | 'autoFixed'
  | 'approvedFix'
  | 'skipped'
  | 'needsManualReview';

export interface StdRuleDescriptor {
  ruleId: string;
  ruleName: string;
  category: RuleCategory;
  description: string;
  legalBasis: string;
  applicableDocumentTypes: StdDocScope[];
  severity: StdSeverity;
  safetyTier: FixSafetyTier;
  checkFunction: string;
  fixFunction: string | null;
  autoFixSupported: boolean;
  enabled: boolean;
}

export interface SectionSummary {
  sectionIndex: number;
  paperWidthMm: number;
  paperHeightMm: number;
  orientation: string;
  marginTopMm: number;
  marginBottomMm: number;
  marginLeftMm: number;
  marginRightMm: number;
  differentFirstPage: boolean;
  differentOddEven: boolean;
  breakType: string;
  hasHeaderRef: boolean;
  hasFooterRef: boolean;
}

export interface ParagraphLocationPreview {
  elementPath: string;
  index: number;
  role: string;
  sectionIndex: number;
  excerpt: string;
  alignment: string;
  fontName: string;
  fontSizePt: number | null;
}

export interface TableSummary {
  tableIndex: number;
  elementPath: string;
  kind: string;
  rowCount: number;
  colCount: number;
  totalWidthTwips: number;
  printableWidthTwips: number;
  exceedsPrintableWidth: boolean;
  hasRepeatHeader: boolean;
  hasLockedLongRows: boolean;
}

export interface HeaderFooterSummary {
  partName: string;
  kind: string;
  hasPageField: boolean;
  hasNumpagesField: boolean;
  staticPageDigitSuspect: boolean;
  fontNames: string[];
  fontSizesPt: number[];
  textPreview: string;
}

export interface AssetInventory {
  mediaCount: number;
  mediaBytes: number;
  drawingCount: number;
  hyperlinkCount: number;
  bookmarkCount: number;
  footnoteCount: number;
  endnoteCount: number;
  commentCount: number;
  fieldCount: number;
  hasVbaMacros: boolean;
  vbaBytes: number;
}

export interface RuleEvalSummary {
  ruleId: string;
  ruleName: string;
  category: RuleCategory;
  severity: StdSeverity;
  status: RuleCheckStatus;
  findingCount: number;
  legalBasis: string;
  autoFixSupported: boolean;
  safetyTier: FixSafetyTier;
}

export interface StdFinding {
  findingId: string;
  ruleId: string;
  ruleName: string;
  category: RuleCategory;
  severity: StdSeverity;
  status: RuleCheckStatus;
  safetyTier: FixSafetyTier;
  location: string;
  elementPath: string;
  currentValue: string;
  expectedValue: string;
  proposal: string;
  legalBasis: string;
  autoFixSupported: boolean;
  disposition: FindingDisposition;
}

export interface StdStatusCounts {
  passed: number;
  failed: number;
  needsReview: number;
  notApplicable: number;
  indeterminate: number;
}

export interface StdSeverityCounts {
  critical: number;
  error: number;
  warning: number;
  information: number;
}

export interface StdAnalysisReport {
  filePath: string;
  fileName: string;
  fileExt: string;
  requestedScope: StdDocScope;
  effectiveScope: StdDocScope;
  requiresScopeSelection: boolean;
  scopeReason: string;
  sections: SectionSummary[];
  tables: TableSummary[];
  headersFooters: HeaderFooterSummary[];
  paragraphCount: number;
  paragraphPreviews: ParagraphLocationPreview[];
  assets: AssetInventory;
  ruleSummaries: RuleEvalSummary[];
  findings: StdFinding[];
  statusCounts: StdStatusCounts;
  severityCounts: StdSeverityCounts;
}

export interface StdExecutionRequest {
  path: string;
  scope: StdDocScope;
  orgProfile: OrganizationProfile | null;
  outputMode: OutputMode;
  applyAllGroupA: boolean;
  approvedFindingIds: string[];
  skippedFindingIds: string[];
  manualReviewFindingIds: string[];
  disabledRuleIds: string[];
}

export interface FixActionRecord {
  actionId: string;
  ruleId: string;
  safetyTier: FixSafetyTier;
  location: string;
  elementPath: string;
  beforeValue: string;
  afterValue: string;
  timestamp: string;
  reversible: boolean;
}

export type DiffDimension =
  | 'textContent'
  | 'fontFamily'
  | 'fontSize'
  | 'fontStyle'
  | 'alignment'
  | 'spacingAndIndent'
  | 'pageMarginsAndSize'
  | 'sectionStructure'
  | 'tableStructure'
  | 'headerFooterAndPageNum';

export interface DiffEntry {
  dimension: DiffDimension;
  location: string;
  beforeVal: string;
  afterVal: string;
  reason: string;
  ruleId: string;
}

export interface PreservationCheckSummary {
  passed: boolean;
  textParagraphsBefore: number;
  textParagraphsAfter: number;
  tablesBefore: number;
  tablesAfter: number;
  mediaBefore: number;
  mediaAfter: number;
  drawingsBefore: number;
  drawingsAfter: number;
  hyperlinksBefore: number;
  hyperlinksAfter: number;
  bookmarksBefore: number;
  bookmarksAfter: number;
  footnotesBefore: number;
  footnotesAfter: number;
  endnotesBefore: number;
  endnotesAfter: number;
  commentsBefore: number;
  commentsAfter: number;
  fieldsBefore: number;
  fieldsAfter: number;
  macrosBefore: boolean;
  macrosAfter: boolean;
  warnings: string[];
}

export interface DocumentDiffReport {
  entries: DiffEntry[];
  dimensionCounts: Record<string, number>;
  preservation: PreservationCheckSummary;
}

export interface PostSaveVerification {
  outputExists: boolean;
  outputSizeBytes: number;
  reopenedFromDisk: boolean;
  xmlValid: boolean;
  preservationPassed: boolean;
  remainingCriticalOrError: number;
  summaryMessage: string;
}

export interface StdExecutionResult {
  success: boolean;
  inputPath: string;
  outputPath: string;
  backupPath: string | null;
  error: string | null;
  errorCode?: string | null;
  logs: string[];
  actionsApplied: FixActionRecord[];
  reportBefore: StdAnalysisReport | null;
  reportAfter: StdAnalysisReport | null;
  diff: DocumentDiffReport | null;
  verification: PostSaveVerification | null;
  blobUrl?: string;
  outputName?: string;
  outputBlob?: Blob;
}

// ============================================================================
// PHASE 4: TEMPLATE ENGINE TYPES
// ============================================================================

export type TemplateGroup = 'administrative' | 'enterprise' | 'party' | 'academic' | 'custom';

export type TemplateUsageStatus = 'active' | 'draft' | 'archived';

export type TemplateBlockKind =
  | 'recipientSalutation'
  | 'authorityHeading'
  | 'preambleBasis'
  | 'decisionKeyword'
  | 'metadataBlock'
  | 'headingSection'
  | 'articleClause'
  | 'paragraphBlock'
  | 'tableBlock';

export type SignatureLayoutKind =
  | 'singleRight'
  | 'dualSecretaryChair'
  | 'tripleApproval'
  | 'none';

export interface TemplateTableColumn {
  header: string;
  widthPct: number;
  align: string;
}

export interface TemplateTableBlueprint {
  caption: string;
  columns: TemplateTableColumn[];
  sampleRows: string[][];
}

export interface TemplateSectionBlock {
  sectionId: string;
  title: string;
  kind: TemplateBlockKind;
  required: boolean;
  enabledByDefault: boolean;
  headingAlignment: string;
  headingBold: boolean;
  headingItalic: boolean;
  headingUppercase: boolean;
  bodyParagraphs: string[];
  inlineTable: TemplateTableBlueprint | null;
}

export interface TemplateHeaderConfig {
  enabled: boolean;
  baseAdminDocType: AdminDocType;
  customDocTypeLabel: string;
  symbolPrefix: string;
  useLeftCellSubject: boolean;
  showGoverningOrg: boolean;
  showIssuingOrg: boolean;
  showNationalHeader: boolean;
  respectOrgEnterpriseLayout: boolean;
  showNumberSymbol: boolean;
  showLocationDate: boolean;
  showDocTypeAndSubject: boolean;
  includeControlBox: boolean;
}

export interface TemplateFooterConfig {
  showRecipientList: boolean;
  recipientHeaderLabel: string;
  defaultRecipients: string[];
  includePageNumbers: boolean;
  pageNumberPosition: PageNumPosition;
  footerNoteTemplate: string;
}

export interface TemplateSignatureConfig {
  layout: SignatureLayoutKind;
  authorityMode: string;
  authorityPrefixLabel: string;
  signerTitlePlaceholder: string;
  signerNamePlaceholder: string;
  leftColumnTitle: string;
  middleColumnTitle: string;
  blankLinesForSeal: number;
}

export interface TemplateTableConfig {
  defaultFontName: string;
  defaultFontSizePt: number;
  headerBold: boolean;
  repeatHeaderRow: boolean;
  preventRowSplit: boolean;
  borderStyle: string;
  borderSizeEighthsPt: number;
  cellPaddingTopBottomTwips: number;
  cellPaddingLeftRightTwips: number;
  fitToPrintableWidth: boolean;
}

export interface TemplateFormatRules {
  paperSize: PaperSize;
  marginTopMm: number;
  marginBottomMm: number;
  marginLeftMm: number;
  marginRightMm: number;
  fontFamily: string;
  bodyFontSizePt: number;
  lineSpacingMultiple: number;
  spaceBeforePt: number;
  spaceAfterPt: number;
  firstLineIndentCm: number;
  bodyAlignment: string;
  enforceCompatibilityCheck: boolean;
}

export interface DocumentTemplate {
  templateId: string;
  name: string;
  group: TemplateGroup;
  docType: string;
  scope: StdDocScope;
  contentStructure: TemplateSectionBlock[];
  headerConfig: TemplateHeaderConfig;
  footerConfig: TemplateFooterConfig;
  signatureConfig: TemplateSignatureConfig;
  tableConfig: TemplateTableConfig;
  formatRules: TemplateFormatRules;
  version: string;
  status: TemplateUsageStatus;
  description: string;
  isSystem: boolean;
  isDefaultForDocType: boolean;
  sourceDocxAssetId: string | null;
  detectedPlaceholders: string[];
  updatedAt: string;
}

export interface DynamicTemplateFields {
  tenCoQuan: string;
  coQuanChuQuan: string;
  vietTatCoQuan: string;
  soVanBan: string;
  diaDanh: string;
  ngayThang: string;
  day: string;
  month: string;
  year: string;
  nguoiKy: string;
  chucVu: string;
  nguoiNhan: string;
  trichYeu: string;
  extraFields: Record<string, string>;
}

export interface MissingFieldMarker {
  fieldKey: string;
  labelVi: string;
  markerText: string;
  requiredBy: string;
}

export interface TemplateVersionRecord {
  version: string;
  savedAt: string;
  changeNote: string;
  hasDocxAsset: boolean;
  docxAssetId: string | null;
  snapshot: DocumentTemplate;
}

export interface TemplateCatalogResponse {
  templates: DocumentTemplate[];
  customDropinDir: string;
  defaults: Record<string, string>;
}

export interface SaveTemplateRequest {
  template: DocumentTemplate;
  confirmSystemOverride: boolean;
  changeNote: string;
}

export interface ImportDocxTemplateRequest {
  sourcePath: string;
  name: string;
  group: TemplateGroup;
  docType: string;
  scope: StdDocScope;
}

export type TemplateTargetMode = 'createNewDocx' | 'applyToExistingDocx';

export interface TemplateApplyRequest {
  templateId: string;
  orgProfileId: string | null;
  orgProfileOverride: OrganizationProfile | null;
  targetMode: TemplateTargetMode;
  targetPath: string | null;
  destPath: string | null;
  outputMode: OutputMode;
  enabledSectionIds: string[];
  dynamicFields: DynamicTemplateFields;
  insertFramingOnExisting: boolean;
}

export interface TemplatePipelineStep {
  stepNumber: number;
  stepName: string;
  detail: string;
  completed: boolean;
}

export interface TemplateCompatibilityReport {
  compatible: boolean;
  scope: StdDocScope;
  passedRules: number;
  failedRules: number;
  needsReviewRules: number;
  missingFields: MissingFieldMarker[];
  preservedAssets: AssetInventory;
  warnings: string[];
  stdFindings: StdFinding[];
}

export interface TemplatePreviewBlock {
  role: string;
  title: string;
  lines: VecPreviewLines;
  tablePreview: TemplateTableBlueprint | null;
}

type VecPreviewLines = string[];

export interface TemplatePreviewResult {
  templateId: string;
  templateName: string;
  docType: string;
  resolvedOrgName: string;
  headerLeftLines: string[];
  headerRightLines: string[];
  titleLines: string[];
  bodyBlocks: TemplatePreviewBlock[];
  recipientLines: string[];
  signatureColumns: string[][];
  pipelineSteps: TemplatePipelineStep[];
  compatibility: TemplateCompatibilityReport;
}

export interface TemplateApplyResult {
  success: boolean;
  outputPath: string;
  backupPath: string | null;
  error: string | null;
  errorCode?: string | null;
  postSaveVerified: boolean;
  outputSizeBytes: number;
  preview: TemplatePreviewResult | null;
  logs: string[];
  blobUrl?: string;
  outputName?: string;
  outputBlob?: Blob;
}

// ============================================================================
// PHASE 5: LAYOUT INTELLIGENCE TYPES
// ============================================================================

export type RecognizedStructureKind =
  | 'documentHeader'
  | 'docTypeTitle'
  | 'docSubject'
  | 'mainTitle'
  | 'mainBody'
  | 'headingLevel1'
  | 'headingLevel2'
  | 'headingLevel3'
  | 'headingLevel4'
  | 'dataTable'
  | 'imageAsset'
  | 'signatureBlock'
  | 'recipientBlock'
  | 'appendixBlock'
  | 'headerFooter'
  | 'textBox';

export interface ElementLocator {
  partName: string;
  elementPath: string;
  bodyChildIndex: number | null;
  sectionIndex: number;
  estimatedPageStart: number;
  estimatedPageEnd: number;
  columnIndex: number;
  xLeftMm: number;
  yTopMm: number;
  widthMm: number;
  heightMm: number;
}

export interface DocumentMapNode {
  nodeId: string;
  parentId: string | null;
  childrenIds: string[];
  kind: RecognizedStructureKind;
  label: string;
  excerpt: string;
  level: number;
  confidence: number;
  signals: string[];
  locator: ElementLocator;
}

export interface ParagraphMapEntry {
  paraId: string;
  globalIndex: number;
  kind: RecognizedStructureKind;
  roleLabel: string;
  excerpt: string;
  styleId: string | null;
  fontName: string;
  fontSizePt: number | null;
  alignment: string;
  indentLeftMm: number;
  indentRightMm: number;
  indentFirstLineMm: number;
  spaceBeforePt: number;
  spaceAfterPt: number;
  lineSpacingMult: number;
  widowControl: boolean;
  keepNext: boolean;
  keepLines: boolean;
  pageBreakBefore: boolean;
  inTableId: string | null;
  inTextBoxId: string | null;
  estimatedLines: number;
  gapFromPrevPt: number;
  locator: ElementLocator;
}

export interface TableMapEntry {
  tableId: string;
  tableIndex: number;
  tableRole: string;
  rowCount: number;
  colCount: number;
  colWidthsMm: number[];
  totalWidthMm: number;
  printableWidthMm: number;
  indentLeftMm: number;
  alignment: string;
  hasRepeatHeader: boolean;
  hasMergedCells: boolean;
  lockedLongRowCount: number;
  overflowsWidth: boolean;
  overflowMm: number;
  recommendedOrientation: string;
  locator: ElementLocator;
}

export interface SectionMapEntry {
  sectionId: string;
  sectionIndex: number;
  breakType: string;
  orientation: string;
  paperWidthMm: number;
  paperHeightMm: number;
  marginTopMm: number;
  marginBottomMm: number;
  marginLeftMm: number;
  marginRightMm: number;
  printableWidthMm: number;
  printableHeightMm: number;
  columnCount: number;
  columnSpaceMm: number;
  equalColumnWidth: boolean;
  colWidthsMm: number[];
  headerIds: string[];
  footerIds: string[];
  pageStart: number;
  pageEnd: number;
}

export interface StyleMapEntry {
  styleId: string;
  styleName: string;
  styleType: string;
  basedOn: string | null;
  isDefault: boolean;
  fontName: string | null;
  fontSizePt: number | null;
  alignment: string | null;
  outlineLevel: number | null;
  usageCount: number;
  directConflictCount: number;
}

export interface HeaderFooterMapEntry {
  hfId: string;
  partName: string;
  hfKind: string;
  bindTypes: string[];
  linkedSections: number[];
  paragraphCount: number;
  tableCount: number;
  hasPageField: boolean;
  hasNumpagesField: boolean;
  hasImages: boolean;
  textPreview: string;
  locator: ElementLocator;
}

export interface PageLayoutEntry {
  pageId: string;
  pageNumber: number;
  sectionIndex: number;
  orientation: string;
  widthMm: number;
  heightMm: number;
  printableWidthMm: number;
  printableHeightMm: number;
  usedHeightMm: number;
  fillRatioPct: number;
  columnCount: number;
  elementIds: string[];
  hasOverflow: boolean;
  hasOrphanOrStrandedHeading: boolean;
  hasAbnormalWhitespace: boolean;
}

export interface VisualObjectMapEntry {
  objectId: string;
  objectKind: string;
  anchorMode: string;
  relationshipId: string | null;
  mediaTarget: string | null;
  widthMm: number;
  heightMm: number;
  overflowsPrintableArea: boolean;
  innerParagraphCount: number;
  textExcerpt: string;
  locator: ElementLocator;
}

export interface IntelligentDocumentMap {
  documentMap: DocumentMapNode[];
  paragraphMap: ParagraphMapEntry[];
  tableMap: TableMapEntry[];
  sectionMap: SectionMapEntry[];
  styleMap: StyleMapEntry[];
  headerFooterMap: HeaderFooterMapEntry[];
  pageLayoutMap: PageLayoutEntry[];
  visualObjects: VisualObjectMapEntry[];
}

export type LayoutWarningKind =
  | 'tableOverflowWidth'
  | 'imageOverflowPage'
  | 'paragraphIndentOverflow'
  | 'widowOrphanRisk'
  | 'strandedHeadingBottom'
  | 'signaturePageSplit'
  | 'stackedEmptyParagraphs'
  | 'prematurePageBreak'
  | 'excessiveSpacing'
  | 'inconsistentSectionSpacing'
  | 'misalignedComponent'
  | 'unbalancedLayout';

export interface LayoutWarning {
  warningId: string;
  kind: LayoutWarningKind;
  severity: StdSeverity;
  title: string;
  detail: string;
  targetElementId: string;
  locator: ElementLocator;
}

export type SmartProposalKind =
  | 'normalizeSpacing'
  | 'alignHeadings'
  | 'unifyParagraphFormat'
  | 'adjustTableLayout'
  | 'normalizeSectionSpacing'
  | 'resolveOverflow'
  | 'recommendOrientation'
  | 'rebalanceLayout';

export type LayoutProposalTier = 'highConfidenceAuto' | 'requiresConfirmation';

export interface LayoutProposal {
  proposalId: string;
  kind: SmartProposalKind;
  tier: LayoutProposalTier;
  confidence: number;
  title: string;
  reason: string;
  suggestedChange: string;
  targetElementIds: string[];
  locator: ElementLocator;
  affectsPagination: boolean;
  affectsSection: boolean;
  affectsTable: boolean;
  affectsSignature: boolean;
  paramValue: string | null;
}

export interface LayoutIntelligenceSummary {
  sectionCount: number;
  estimatedPageCount: number;
  paragraphCount: number;
  tableCount: number;
  imageCount: number;
  textBoxCount: number;
  headingCount: number;
  styleCount: number;
  headerFooterCount: number;
  warningCount: number;
  autoProposalCount: number;
  confirmProposalCount: number;
}

export interface LayoutIntelligenceReport {
  filePath: string;
  fileName: string;
  scope: StdDocScope;
  summary: LayoutIntelligenceSummary;
  maps: IntelligentDocumentMap;
  warnings: LayoutWarning[];
  proposals: LayoutProposal[];
  assets: AssetInventory;
}

export interface LayoutExecutionRequest {
  path: string;
  scope?: StdDocScope | null;
  outputMode: OutputMode;
  applyHighConfidenceAuto: boolean;
  acceptedProposalIds: string[];
  rejectedProposalIds: string[];
}

export interface LayoutExecutionResult {
  success: boolean;
  outputPath: string;
  backupPath: string | null;
  error: string | null;
  errorCode?: string | null;
  appliedProposalIds: string[];
  skippedProposalIds: string[];
  contentPreservedVerified: boolean;
  relationshipsPreservedVerified: boolean;
  postSaveReport: LayoutIntelligenceReport | null;
  logs: string[];
  blobUrl?: string;
  outputName?: string;
  outputBlob?: Blob;
}

// ============================================================================
// PHASE 6: ADVANCED COMPLIANCE & AUDIT TYPES
// ============================================================================

export type ComplianceObligation = 'mandatory' | 'conditional' | 'recommended' | 'notApplicable';

export type ComplianceBasisKind =
  | 'legalRegulation'
  | 'technicalRecommendation'
  | 'organizationPolicy';

export interface RuleVersionEntry {
  ruleId: string;
  ruleVersion: string;
  formComponent: string;
  basisKind: ComplianceBasisKind;
  legalCitation: string;
  defaultObligations: Record<string, ComplianceObligation>;
  conditionDescription: string;
  docTypeExceptions: string[];
  remediationGuidance: string;
  enabled: boolean;
}

export interface ComplianceRulePack {
  packId: string;
  packName: string;
  packVersion: string;
  effectiveDate: string;
  legalReferenceSummary: string;
  rules: RuleVersionEntry[];
}

export interface ComplianceMatrixRow {
  ruleId: string;
  ruleName: string;
  ruleVersion: string;
  formatGroup: RuleCategory;
  formComponent: string;
  basisKind: ComplianceBasisKind;
  legalBasis: string;
  applicableDocTypes: string[];
  scope: StdDocScope;
  orgConfigNote: string;
  obligation: ComplianceObligation;
  conditionDetail: string;
  isConditionMetOnDoc: boolean | null;
  evaluationStatus: RuleCheckStatus | null;
  findingCount: number;
  autoFixSupported: boolean;
}

export interface TraceableFinding {
  findingId: string;
  ruleId: string;
  ruleName: string;
  ruleVersion: string;
  packVersion: string;
  legalBasis: string;
  basisKind: ComplianceBasisKind;
  obligation: ComplianceObligation;
  applicableScope: string;
  formatGroup: RuleCategory;
  formComponent: string;
  severity: StdSeverity;
  location: string;
  elementPath: string;
  checkResult: RuleCheckStatus;
  currentValue: string;
  expectedValue: string;
  remediationMethod: string;
  safetyTier: FixSafetyTier;
  autoFixSupported: boolean;
  disposition: FindingDisposition;
}

export type AdvancedIssueKind =
  | 'crossComponentMismatch'
  | 'configurationConflict'
  | 'missingRequiredContent'
  | 'structuralAnomaly';

export interface AdvancedAuditIssue {
  issueId: string;
  kind: AdvancedIssueKind;
  severity: StdSeverity;
  basisKind: ComplianceBasisKind;
  legalBasis: string;
  title: string;
  explanation: string;
  location: string;
  elementPath: string;
  remediation: string;
}

export interface RepeatedErrorCluster {
  clusterId: string;
  ruleId: string;
  ruleName: string;
  ruleVersion: string;
  occurrenceCount: number;
  sampleLocations: string[];
  systemicCause: string;
  recommendedBatchAction: string;
}

export interface DocumentConsistencyMetrics {
  fontUniformityPct: number;
  sizeUniformityPct: number;
  spacingUniformityPct: number;
  headingHierarchyValid: boolean;
  crossComponentConsistent: boolean;
  technicalQualityIndex: number;
}

export interface AuditFileDigest {
  fileName: string;
  fileExt: string;
  fileSizeBytes: number;
  docSha256: string;
  detectedDocType: string;
  requestedScope: StdDocScope;
  effectiveScope: StdDocScope;
  sectionCount: number;
  estimatedPageCount: number;
  paragraphCount: number;
  tableCount: number;
}

export interface AuditRuleProfileUsed {
  packId: string;
  packVersion: string;
  effectiveDate: string;
  orgProfileId: string | null;
  orgProfileName: string | null;
  mandatoryCount: number;
  conditionalCount: number;
  recommendedCount: number;
  notApplicableCount: number;
  disabledRuleIds: string[];
}

export interface AuditRecheckSummary {
  recheckedAt: string;
  outputPath: string;
  outputSha256: string;
  beforeFailedRules: number;
  afterFailedRules: number;
  beforeFindings: number;
  afterFindings: number;
  fixedCount: number;
  skippedCount: number;
  remainingMandatoryFailures: number;
  remainingRecommendedWarnings: number;
  newAnomaliesIntroduced: number;
  preservationPassed: boolean;
}

export interface ComplianceAuditReport {
  auditId: string;
  checkedAt: string;
  legalDisclaimer: string;
  fileInfo: AuditFileDigest;
  ruleProfile: AuditRuleProfileUsed;
  matrix: ComplianceMatrixRow[];
  traceableFindings: TraceableFinding[];
  advancedIssues: AdvancedAuditIssue[];
  repeatedClusters: RepeatedErrorCluster[];
  consistency: DocumentConsistencyMetrics;
  recheck: AuditRecheckSummary | null;
}

export interface AppliedRuleRecord {
  ruleId: string;
  ruleVersion: string;
  obligation: ComplianceObligation;
  status: RuleCheckStatus;
  findingCount: number;
}

export interface RedactedFindingSummary {
  findingId: string;
  ruleId: string;
  ruleVersion: string;
  obligation: ComplianceObligation;
  severity: StdSeverity;
  structuralLocator: string;
  disposition: FindingDisposition;
}

export interface AuditHistoryEntry {
  auditId: string;
  checkedAt: string;
  fileName: string;
  fileSizeBytes: number;
  docSha256: string;
  docVersionLabel: string;
  packId: string;
  packVersion: string;
  effectiveScope: StdDocScope;
  detectedDocType: string;
  technicalQualityIndex: number;
  appliedRules: AppliedRuleRecord[];
  detectedFindings: RedactedFindingSummary[];
  advancedIssueCodes: string[];
  fixedFindingIds: string[];
  skippedFindingIds: string[];
  recheck: AuditRecheckSummary | null;
}

export interface AuditRunComparison {
  olderAuditId: string;
  newerAuditId: string;
  olderCheckedAt: string;
  newerCheckedAt: string;
  sameDocumentHash: boolean;
  olderDocSha256: string;
  newerDocSha256: string;
  packVersionChanged: boolean;
  olderPackVersion: string;
  newerPackVersion: string;
  qualityIndexDelta: number;
  resolvedRuleLocations: string[];
  persistentRuleLocations: string[];
  newlyDetectedRuleLocations: string[];
  skippedCountDelta: number;
  ruleVersionDiffs: string[];
}

export interface ComplianceAuditRequest {
  path: string;
  scope: StdDocScope;
  docTypeHint?: string | null;
  orgProfile?: OrganizationProfile | null;
  disabledRuleIds: string[];
  recordHistory: boolean;
}

export interface ComplianceAutoFixAndRecheckRequest {
  execRequest: StdExecutionRequest;
  docTypeHint?: string | null;
  recordHistory: boolean;
}

export interface ComplianceAutoFixAndRecheckResult {
  stdExecution: StdExecutionResult;
  auditBefore: ComplianceAuditReport | null;
  auditAfter: ComplianceAuditReport | null;
  historyEntry: AuditHistoryEntry | null;
}

export type AuditExportFormat = 'docx' | 'pdf' | 'csv' | 'json';

export interface AuditExportResult {
  success: boolean;
  outputPath: string;
  format: AuditExportFormat;
  sizeBytes: number;
  error: string | null;
  errorCode?: string | null;
}

// ============================================================================
// PHASE 7: LIVE PREVIEW ENGINE TYPES
// ============================================================================

export type PreviewOrientationOverride = 'keepSections' | 'allPortrait' | 'allLandscape';
export type PreviewBodyAlignment = 'justify' | 'left' | 'center' | 'right';

export interface PreviewOverrideConfig {
  enabled: boolean;
  fontName: string | null;
  fontSizePt: number | null;
  tableFontSizePt: number | null;
  bodyAlignment: PreviewBodyAlignment | null;
  lineSpacingMult: number | null;
  spaceBeforePt: number | null;
  spaceAfterPt: number | null;
  firstLineIndentCm: number | null;
  paperSize: PaperSize | null;
  orientationOverride: PreviewOrientationOverride | null;
  marginTopCm: number | null;
  marginBottomCm: number | null;
  marginLeftCm: number | null;
  marginRightCm: number | null;
  autoSwapBindingMargin: boolean;
  templateId: string | null;
  orgProfileId: string | null;
  headerConfig: HeaderBuilderConfig | null;
  applyHeaderMode: HeaderActionMode | null;
  applyVnCleaner: boolean;
  applyStdGroupA: boolean;
}

export interface PreviewBuildRequest {
  jobId: string;
  path: string;
  afterFilePath?: string | null;
  includeAfter: boolean;
  includeAuditSync: boolean;
  scope: StdDocScope;
  processConfig?: ProcessConfig | null;
  overrides: PreviewOverrideConfig;
  preferIncremental: boolean;
}

export interface PreviewSpan {
  text: string;
  fontFamily: string;
  fontSizePt: number;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  colorHex: string;
  highlightHex: string | null;
  vertAlign: string;
  isDynamicPageField: string | null;
}

export interface PreviewImageRef {
  relId: string;
  mediaEntry: string;
  mimeType: string;
  widthMm: number;
  heightMm: number;
  wrapMode: string;
  altText: string;
  overflowsPrintableArea: boolean;
}

export interface PreviewParagraphBlock {
  elementPath: string;
  globalIndex: number;
  sectionIndex: number;
  role: string;
  listPrefix: string;
  alignment: string;
  indentLeftMm: number;
  indentRightMm: number;
  firstLineIndentMm: number;
  hangingIndentMm: number;
  spaceBeforePt: number;
  spaceAfterPt: number;
  lineSpacingMult: number;
  exactLinePt: number | null;
  keepNext: boolean;
  pageBreakBefore: boolean;
  hasExplicitPageBreak: boolean;
  hasLastRenderedBreak: boolean;
  plainText: string;
  spans: PreviewSpan[];
  images: PreviewImageRef[];
  estimatedHeightMm: number;
}

export interface PreviewBorderEdge {
  visible: boolean;
  style: string;
  widthPt: number;
  colorHex: string;
}

export interface PreviewTableBorders {
  top: PreviewBorderEdge;
  bottom: PreviewBorderEdge;
  left: PreviewBorderEdge;
  right: PreviewBorderEdge;
}

export interface PreviewTableCell {
  colIndex: number;
  colSpan: number;
  rowSpan: number;
  mergedContinuation: boolean;
  widthMm: number;
  vAlign: string;
  shadingHex: string | null;
  borders: PreviewTableBorders;
  paragraphs: PreviewParagraphBlock[];
}

export interface PreviewTableRow {
  rowIndex: number;
  isHeaderRow: boolean;
  cantSplit: boolean;
  minHeightMm: number;
  cells: PreviewTableCell[];
}

export interface PreviewTableBlock {
  elementPath: string;
  tableIndex: number;
  sectionIndex: number;
  tableRole: string;
  alignment: string;
  indentLeftMm: number;
  totalWidthMm: number;
  colWidthsMm: number[];
  borders: PreviewTableBorders;
  overflowsPrintableWidth: boolean;
  rows: PreviewTableRow[];
  rowSliceRange: [number, number];
}

export type PreviewBlock =
  | ({ kind: 'paragraph' } & PreviewParagraphBlock)
  | ({ kind: 'table' } & PreviewTableBlock);

export interface PreviewSectionSpec {
  sectionIndex: number;
  paperLabel: string;
  orientation: 'portrait' | 'landscape' | string;
  widthMm: number;
  heightMm: number;
  marginTopMm: number;
  marginBottomMm: number;
  marginLeftMm: number;
  marginRightMm: number;
  headerDistanceMm: number;
  footerDistanceMm: number;
  differentFirstPage: boolean;
  pageNumStart: number | null;
  hasDynamicPageField: boolean;
  defaultHeaderBlocks: PreviewParagraphBlock[];
  firstHeaderBlocks: PreviewParagraphBlock[];
  defaultFooterBlocks: PreviewParagraphBlock[];
  firstFooterBlocks: PreviewParagraphBlock[];
}

export interface PreviewPageModel {
  pageNumber: number;
  displayPageNumber: number;
  totalPages: number;
  sectionIndex: number;
  isFirstPageOfSection: boolean;
  paperLabel: string;
  orientation: 'portrait' | 'landscape' | string;
  widthMm: number;
  heightMm: number;
  marginTopMm: number;
  marginBottomMm: number;
  marginLeftMm: number;
  marginRightMm: number;
  headerBlocks: PreviewParagraphBlock[];
  footerBlocks: PreviewParagraphBlock[];
  blocks: PreviewBlock[];
  usedHeightMm: number;
  fillRatioPct: number;
  breakReason: string;
  hasOverflowWarning: boolean;
  elementPathsOnPage: string[];
}

export interface PreviewElementDiff {
  diffId: string;
  elementPath: string;
  beforePage: number | null;
  afterPage: number | null;
  changeKinds: string[];
  summaryVi: string;
  beforeSummary: string;
  afterSummary: string;
}

export interface PrintDiagnosticItem {
  itemId: string;
  kind: string;
  severity: StdSeverity;
  pageNumber: number;
  elementPath: string;
  title: string;
  detail: string;
}

export interface PrintCheckReport {
  isPrintReady: boolean;
  paperSummary: string;
  orientationSummary: string;
  marginSummary: string;
  headerFooterSummary: string;
  pageNumberSummary: string;
  clippedCount: number;
  overflowTableCount: number;
  warningCount: number;
  diagnostics: PrintDiagnosticItem[];
  fidelityDisclaimer: string;
}

export interface AuditSyncMarker {
  markerId: string;
  source: string;
  ruleOrIssueId: string;
  title: string;
  severity: StdSeverity;
  obligation: string;
  legalBasis: string;
  elementPath: string;
  pageNumber: number;
  currentValue: string;
  expectedValue: string;
  remediation: string;
}

export interface PreviewDocumentRender {
  label: string;
  docSha256: string;
  sectionCount: number;
  totalPages: number;
  paragraphCount: number;
  tableCount: number;
  imageCount: number;
  sections: PreviewSectionSpec[];
  pages: PreviewPageModel[];
  printCheck: PrintCheckReport;
}

export interface PreviewSessionPayload {
  jobId: string;
  status: 'completed' | 'cancelled' | 'error' | string;
  updateMode: 'initial' | 'incrementalPatch' | 'fullRebuild' | string;
  elapsedMs: number;
  fidelityMode: string;
  fidelityDisclaimer: string;
  wordInstalled: boolean;
  before: PreviewDocumentRender | null;
  after: PreviewDocumentRender | null;
  diffs: PreviewElementDiff[];
  auditMarkers: AuditSyncMarker[];
}

export interface WordComPreviewVerification {
  jobId: string;
  cancelled: boolean;
  targetVariant: string;
  wordPageCount: number;
  openxmlEstimatedPages: number;
  pageCountDelta: number;
  pdfUrlPath: string;
  pdfDiskPath: string;
  tempDocxPath: string;
  elapsedMs: number;
  note: string;
}

// ============================================================================
// PHASE 8: RELIABILITY, BACKUP VERSIONING & STRUCTURED LOGGING TYPES
// ============================================================================

export type BackupKind = 'initialBaseline' | 'preEditVersion';

export interface BackupVersionRecord {
  backupId: string;
  originalFileName: string;
  backupFileName: string;
  backupPath: string;
  kind: BackupKind;
  versionNumber: number;
  sourceModule: string;
  createdAt: string;
  sizeBytes: number;
  sha256: string;
  integrityOk: boolean;
}

export interface OperationLogEntry {
  timestamp: string;
  task: string;
  fileName: string;
  module: string;
  status: string;
  elapsedMs: number;
  errorCode: string | null;
  detail: string;
}
