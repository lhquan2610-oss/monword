export type PresetId = 'nd30' | 'vbDang' | 'doanhNghiep' | 'luanVan' | 'custom';
export type OutputMode = 'newCopy' | 'overwriteWithBackup';
export type PaperSize = 'a4' | 'a3' | 'letter';
export type QuoteStyle = 'curlyNd30' | 'straightDang' | 'keep';
export type PageNumPosition = 'topCenter' | 'topRight' | 'bottomCenter' | 'bottomRight';
export type OrientationMode = 'keep' | 'allPortrait' | 'allLandscape' | 'rotateRanges';
export type LockMode = 'none' | 'readOnly' | 'commentsOnly' | 'trackedChanges';
export type AuditSeverity = 'error' | 'warning' | 'info';

export type NavTab =
  | 'oneClick'
  | 'format'
  | 'pageToc'
  | 'cleaner'
  | 'security'
  | 'system';

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
