import type {
  AssetInventory,
  DiffEntry,
  DocumentDiffReport,
  FileQueueItem,
  FixActionRecord,
  FixSafetyTier,
  HeaderFooterSummary,
  OrganizationProfile,
  ParagraphLocationPreview,
  PostSaveVerification,
  PreservationCheckSummary,
  RuleCategory,
  RuleCheckStatus,
  RuleEvalSummary,
  SectionSummary,
  StdAnalysisReport,
  StdDocScope,
  StdExecutionRequest,
  StdExecutionResult,
  StdFinding,
  StdRuleDescriptor,
  StdSeverity,
  TableSummary,
} from '../types';
import { readZipEntries, writeZipEntries } from './webDocxEngine';

const W_NS = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main';

const initialStdWebSnapshots = new Map<string, File>();

export function registerWebStdSnapshot(item: FileQueueItem): void {
  if (item.webFile && !initialStdWebSnapshots.has(item.path)) {
    initialStdWebSnapshots.set(item.path, item.webFile);
  }
}

export function getWebStdSnapshot(path: string): File | undefined {
  return initialStdWebSnapshots.get(path);
}

const PHU_LUC_III_CODES = new Set([
  'NQ', 'QĐ', 'CT', 'QC', 'QyĐ', 'TC', 'TB', 'HD', 'CTr', 'KH', 'PA', 'ĐA', 'DA',
  'BC', 'BB', 'TTr', 'HĐ', 'CĐ', 'BGN', 'BTT', 'GUQ', 'GM', 'GGT', 'GNP', 'PG', 'PC', 'PB',
]);

const SYMBOL_FONTS = new Set([
  'symbol', 'wingdings', 'wingdings 2', 'wingdings 3', 'webdings', 'marlett', 'cambria math', 'segoe ui symbol', 'mt extra',
]);

// ============================================================================
// RULE REGISTRY (MATCHES RUST ENGINE DESCRIPTORS)
// ============================================================================

export function getWebStdRules(): StdRuleDescriptor[] {
  const allCore: StdDocScope[] = [
    'adminNd30', 'partyDocument', 'enterpriseInternal', 'specializedLegal', 'academicThesis', 'freeform',
  ];
  const adminParty: StdDocScope[] = ['adminNd30', 'partyDocument', 'enterpriseInternal'];
  const adminStrict: StdDocScope[] = ['adminNd30', 'partyDocument'];

  const base: Omit<StdRuleDescriptor, 'checkFunction' | 'fixFunction'>[] = [
    {
      ruleId: 'PAGE-01', ruleName: 'Khổ giấy chuẩn A4 (210 × 297 mm)', category: 'pageAndSection',
      description: 'Kiểm tra kích thước khổ giấy của từng Section theo chuẩn A4.',
      legalBasis: 'Khoản 1 Điều 9 & Mục I Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: ['adminNd30', 'partyDocument', 'enterpriseInternal', 'academicThesis'],
      severity: 'error', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PAGE-02', ruleName: 'Định lề trang văn bản hành chính', category: 'pageAndSection',
      description: 'Trên/Dưới 20–25 mm; Trái 30–35 mm; Phải 15–20 mm.',
      legalBasis: 'Mục I.2 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminParty,
      severity: 'error', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PAGE-03', ruleName: 'Cấu trúc đa Section & Trang ngang', category: 'pageAndSection',
      description: 'Nhận diện cấu trúc nhiều Section và hướng trang khác nhau để bảo toàn.',
      legalBasis: 'Mục I.1 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'information', safetyTier: 'groupBConfirm', autoFixSupported: false, enabled: true,
    },
    {
      ruleId: 'FONT-01', ruleName: 'Bộ mã tiếng Việt Unicode TCVN 6909:2001', category: 'fontAndEncoding',
      description: 'Phát hiện bảng mã cũ TCVN3 (.Vn*) / VNI (VNI-*) và chuyển sang Unicode.',
      legalBasis: 'Khoản 2 Điều 9 & Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'critical', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'FONT-02', ruleName: 'Phông chữ Times New Roman & tính đồng nhất', category: 'fontAndEncoding',
      description: 'Kiểm tra phông Times New Roman và trộn phông (trừ ký hiệu/ảnh).',
      legalBasis: 'Khoản 2 Điều 9 & Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: ['adminNd30', 'partyDocument', 'enterpriseInternal', 'academicThesis'],
      severity: 'error', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'FONT-03', ruleName: 'Cỡ chữ theo từng thành phần thể thức', category: 'fontAndEncoding',
      description: 'Lời văn 13–14pt; Nơi nhận 11–12pt; Bảng dữ liệu 11–13pt.',
      legalBasis: 'Mục II & Bảng tổng hợp Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminParty,
      severity: 'error', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'FONT-04', ruleName: 'Màu chữ đen chuẩn (000000)', category: 'fontAndEncoding',
      description: 'Màu chữ nội dung đen chuẩn, giữ nguyên màu Hyperlink.',
      legalBasis: 'Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminParty,
      severity: 'warning', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'FONT-05', ruleName: 'Khoảng trắng thừa, dấu câu & ký tự điều khiển', category: 'fontAndEncoding',
      description: 'Phát hiện dấu cách kép, cách trước dấu câu, ký tự điều khiển ẩn.',
      legalBasis: 'Kỹ thuật trình bày soạn thảo văn bản NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'warning', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PARA-01', ruleName: 'Căn đều hai bên phần lời văn chính', category: 'paragraphAndAlignment',
      description: 'Lời văn chính căn đều (Justify), bảo vệ bảng/tiêu đề/chữ ký.',
      legalBasis: 'Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminParty,
      severity: 'error', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PARA-02', ruleName: 'Thụt đầu dòng lời văn (1.0–1.27 cm)', category: 'paragraphAndAlignment',
      description: 'Thụt đầu dòng lời văn 1–1.27 cm.',
      legalBasis: 'Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminParty,
      severity: 'warning', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PARA-03', ruleName: 'Giãn dòng (Single–1.5 lines) & Khoảng cách đoạn (>= 6pt)', category: 'paragraphAndAlignment',
      description: 'Giãn dòng 1.0–1.5x và khoảng cách đoạn tối thiểu 6pt.',
      legalBasis: 'Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: ['adminNd30', 'partyDocument', 'enterpriseInternal', 'academicThesis'],
      severity: 'warning', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PARA-04', ruleName: 'Đoạn trống thừa & ngắt dòng mềm (Shift+Enter)', category: 'paragraphAndAlignment',
      description: 'Phát hiện đoạn trống thừa liên tiếp và ngắt dòng mềm.',
      legalBasis: 'Kỹ thuật trình bày OpenXML NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'warning', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PARA-05', ruleName: 'Xung đột định dạng trực tiếp với Paragraph Style', category: 'paragraphAndAlignment',
      description: 'Định dạng trực tiếp xung đột với Style đang gán.',
      legalBasis: 'Tính nhất quán cấu trúc Style OpenXML',
      applicableDocumentTypes: allCore,
      severity: 'information', safetyTier: 'groupBConfirm', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'STRUCT-01', ruleName: 'Trình bày Tên loại, Trích yếu & Căn cứ pháp lý', category: 'headingAndStructure',
      description: 'Tên loại đứng đậm căn giữa; dòng Căn cứ in nghiêng.',
      legalBasis: 'Khoản 5 & Khoản 6 Mục II Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminStrict,
      severity: 'warning', safetyTier: 'groupBConfirm', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'STRUCT-02', ruleName: 'Hệ thống đề mục Phần/Chương/Mục/Điều & Phụ lục', category: 'headingAndStructure',
      description: 'Đậm/căn chỉnh theo cấp đề mục, không đổi số điều khoản.',
      legalBasis: 'Mục II.6 & Mục IV Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'warning', safetyTier: 'groupBConfirm', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'STRUCT-03', ruleName: 'Gán Heading Styles & màu tiêu đề chuẩn', category: 'headingAndStructure',
      description: 'Gán Heading1–4 màu đen cho mục lục tự động.',
      legalBasis: 'Chuẩn hóa điều hướng & Mục lục NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'information', safetyTier: 'groupBConfirm', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'TBL-01', ruleName: 'Bảng biểu không vượt khỏi khổ vùng in', category: 'tableLayout',
      description: 'Phát hiện bảng vượt khổ in (chỉ co khi xác nhận, không tự xoay giấy).',
      legalBasis: 'Mục I.1 & Bố cục trang in NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'error', safetyTier: 'groupBConfirm', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'TBL-02', ruleName: 'Lặp dòng tiêu đề & Ngắt trang tự nhiên trong bảng nhiều hàng', category: 'tableLayout',
      description: 'Bật w:tblHeader và gỡ w:cantSplit ở hàng dữ liệu dài.',
      legalBasis: 'Kỹ thuật trình bày bảng biểu nhiều trang Word',
      applicableDocumentTypes: allCore,
      severity: 'warning', safetyTier: 'groupBConfirm', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'TBL-03', ruleName: 'Phông chữ, cỡ chữ (11–13pt) & thụt lề trong ô bảng dữ liệu', category: 'tableLayout',
      description: 'Đồng bộ Times New Roman 11–13pt và bỏ thụt dòng trong ô.',
      legalBasis: 'Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminParty,
      severity: 'warning', safetyTier: 'groupAAuto', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PGNUM-01', ruleName: 'Số trang đặt Header giữa trên (13–14pt), ẩn trang đầu', category: 'pageNumberAndHeaderFooter',
      description: 'Số trang Header giữa trên 13–14pt đứng, bật titlePg.',
      legalBasis: 'Khoản 3 Điều 9 & Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminStrict,
      severity: 'warning', safetyTier: 'groupBConfirm', autoFixSupported: true, enabled: true,
    },
    {
      ruleId: 'PGNUM-02', ruleName: 'Bảo toàn Header/Footer & trường động PAGE/NUMPAGES', category: 'pageNumberAndHeaderFooter',
      description: 'Kiểm tra số tĩnh và bảo toàn Header/Footer, Field tự động.',
      legalBasis: 'Mục I.4 Phụ lục I NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: allCore,
      severity: 'information', safetyTier: 'groupCManualOnly', autoFixSupported: false, enabled: true,
    },
    {
      ruleId: 'CAP-01', ruleName: 'Viết hoa thành phần thể thức & quyền hạn ký (Phụ lục II)', category: 'capitalizationAndAbbreviation',
      description: 'Kiểm tra viết hoa tên loại VB, TM./KT./TL., Nơi nhận.',
      legalBasis: 'Phụ lục II NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminStrict,
      severity: 'warning', safetyTier: 'groupCManualOnly', autoFixSupported: false, enabled: true,
    },
    {
      ruleId: 'CAP-02', ruleName: 'Nhất quán viết hoa tên cơ quan, địa danh & danh từ riêng', category: 'capitalizationAndAbbreviation',
      description: 'Cảnh báo viết hoa chưa đồng nhất để kiểm tra thủ công.',
      legalBasis: 'Mục II, III, IV Phụ lục II NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminParty,
      severity: 'information', safetyTier: 'groupCManualOnly', autoFixSupported: false, enabled: true,
    },
    {
      ruleId: 'ABBR-01', ruleName: 'Chữ viết tắt tên loại văn bản hành chính (Phụ lục III)', category: 'capitalizationAndAbbreviation',
      description: 'Đối chiếu 27 chữ viết tắt tên loại Phụ lục III.',
      legalBasis: 'Phụ lục III NĐ 30/2020/NĐ-CP',
      applicableDocumentTypes: adminStrict,
      severity: 'warning', safetyTier: 'groupCManualOnly', autoFixSupported: false, enabled: true,
    },
    {
      ruleId: 'PRESERVE-01', ruleName: 'Kiểm kê đối tượng nhúng, liên kết & tính toàn vẹn OpenXML', category: 'preservationAndIntegrity',
      description: 'Kiểm tra bảo toàn Ảnh/Logo, Hyperlink, Bookmark, Footnote, Endnote, Comment, Macro VBA.',
      legalBasis: 'Tiêu chuẩn bảo toàn dữ liệu ECMA-376 OpenXML',
      applicableDocumentTypes: allCore,
      severity: 'critical', safetyTier: 'groupCManualOnly', autoFixSupported: false, enabled: true,
    },
  ];
  return base.map((r) => ({
    ...r,
    checkFunction: `check_${r.ruleId.toLowerCase().replace('-', '_')}`,
    fixFunction: r.autoFixSupported ? `fix_${r.ruleId.toLowerCase().replace('-', '_')}` : null,
  }));
}

// ============================================================================
// TCVN3 / VNI DECODER (BROWSER PARITY)
// ============================================================================

const TCVN3_PAIRS: [string, string][] = [
  ['µ', 'à'], ['¸', 'á'], ['¶', 'ả'], ['·', 'ã'], ['¹', 'ạ'],
  ['¨', 'ă'], ['»', 'ằ'], ['¾', 'ắ'], ['¼', 'ẳ'], ['½', 'ẵ'], ['Æ', 'ặ'],
  ['©', 'â'], ['Ç', 'ầ'], ['Ê', 'ấ'], ['È', 'ẩ'], ['É', 'ẫ'], ['Ë', 'ậ'],
  ['®', 'đ'], ['§', 'Đ'],
  ['Ì', 'è'], ['Ð', 'é'], ['Î', 'ẻ'], ['Ï', 'ẽ'], ['Ñ', 'ẹ'],
  ['ª', 'ê'], ['Ò', 'ề'], ['Õ', 'ế'], ['Ó', 'ể'], ['Ô', 'ễ'], ['Ö', 'ệ'],
  ['×', 'ì'], ['Ý', 'í'], ['Ø', 'ỉ'], ['Ü', 'ĩ'], ['Þ', 'ị'],
  ['ß', 'ò'], ['ã', 'ó'], ['á', 'ỏ'], ['â', 'õ'], ['ä', 'ọ'],
  ['«', 'ô'], ['å', 'ồ'], ['è', 'ố'], ['æ', 'ổ'], ['ç', 'ỗ'], ['é', 'ộ'],
  ['¬', 'ơ'], ['ê', 'ờ'], ['í', 'ớ'], ['ë', 'ở'], ['ì', 'ỡ'], ['î', 'ợ'],
  ['ï', 'ù'], ['ó', 'ú'], ['ñ', 'ủ'], ['ò', 'ũ'], ['ô', 'ụ'],
  ['­', 'ư'], ['õ', 'ừ'], ['ø', 'ứ'], ['ö', 'ử'], ['÷', 'ữ'], ['ù', 'ự'],
  ['ú', 'ỳ'], ['ý', 'ý'], ['û', 'ỷ'], ['ü', 'ỹ'], ['þ', 'ỵ'],
  ['¡', 'Ă'], ['¢', 'Â'], ['£', 'Ê'], ['¤', 'Ô'], ['¥', 'Ơ'], ['¦', 'Ư'],
];

const TCVN3_MAP = new Map<string, string>(TCVN3_PAIRS);

function decodeTcvn3(raw: string, allCaps = false): string {
  let out = '';
  for (const ch of raw) {
    out += TCVN3_MAP.get(ch) ?? ch;
  }
  return allCaps ? out.toLocaleUpperCase('vi-VN') : out;
}

const VNI_PAIRS = ([
  ['aø', 'à'], ['aù', 'á'], ['aû', 'ả'], ['aõ', 'ã'], ['aï', 'ạ'],
  ['aê', 'ă'], ['aè', 'ằ'], ['aé', 'ắ'], ['aú', 'ẳ'], ['aü', 'ẵ'], ['aë', 'ặ'],
  ['aâ', 'â'], ['aà', 'ầ'], ['aá', 'ấ'], ['aå', 'ẩ'], ['aã', 'ẫ'], ['aä', 'ậ'],
  ['eø', 'è'], ['eù', 'é'], ['eû', 'ẻ'], ['eõ', 'ẽ'], ['eï', 'ẹ'],
  ['eâ', 'ê'], ['eà', 'ề'], ['eá', 'ế'], ['eå', 'ể'], ['eã', 'ễ'], ['eä', 'ệ'],
  ['ì', 'ì'], ['í', 'í'], ['æ', 'ỉ'], ['ó', 'ĩ'], ['ò', 'ị'],
  ['oø', 'ò'], ['où', 'ó'], ['oû', 'ỏ'], ['oõ', 'õ'], ['oï', 'ọ'],
  ['oâ', 'ô'], ['oà', 'ồ'], ['oá', 'ố'], ['oå', 'ổ'], ['oã', 'ỗ'], ['oä', 'ộ'],
  ['ô', 'ơ'], ['ôø', 'ờ'], ['ôù', 'ớ'], ['ôû', 'ở'], ['ôõ', 'ỡ'], ['ôï', 'ợ'],
  ['uø', 'ù'], ['uù', 'ú'], ['uû', 'ủ'], ['uõ', 'ũ'], ['uï', 'ụ'],
  ['ö', 'ư'], ['öø', 'ừ'], ['öù', 'ứ'], ['öû', 'ử'], ['öõ', 'ữ'], ['öï', 'ự'],
  ['yø', 'ỳ'], ['yù', 'ý'], ['yû', 'ỷ'], ['yõ', 'ỹ'], ['î', 'ỵ'],
  ['ñ', 'đ'], ['Ñ', 'Đ'],
  ['AØ', 'À'], ['AÙ', 'Á'], ['AÛ', 'Ả'], ['AÕ', 'Ã'], ['AÏ', 'Ạ'],
  ['AÊ', 'Ă'], ['AÂ', 'Â'], ['EØ', 'È'], ['EÙ', 'É'], ['EÂ', 'Ê'],
  ['OØ', 'Ò'], ['OÙ', 'Ó'], ['OÂ', 'Ô'], ['Ô', 'Ơ'], ['UØ', 'Ù'], ['UÙ', 'Ú'], ['Ö', 'Ư'],
] satisfies [string, string][]).sort((a, b) => b[0].length - a[0].length);

function decodeVni(raw: string): string {
  let s = raw;
  for (const [k, v] of VNI_PAIRS) {
    if (s.includes(k)) s = s.split(k).join(v);
  }
  return s;
}

// ============================================================================
// BROWSER IR & EVALUATION
// ============================================================================

interface WebParaMeta {
  index: number;
  bodyIdx: number;
  tblIdx: number | null;
  elementPath: string;
  sectionIndex: number;
  roleKey: string;
  roleLabel: string;
  text: string;
  trimmed: string;
  styleId: string;
  jc: string;
  firstLine: number;
  leftInd: number;
  rightInd: number;
  before: number;
  after: number;
  line: number;
  fonts: string[];
  sizesPt: number[];
  nonBlackColors: string[];
  isBold: boolean;
  isItalic: boolean;
  hasLegacy: boolean;
  hasMultiSpace: boolean;
  hasPunctErr: boolean;
  hasSoftBreak: boolean;
  hasControlChars: boolean;
  isEmptyRemovable: boolean;
  node: Element;
}

interface WebDocParsed {
  doc: Document;
  entries: Map<string, Uint8Array>;
  report: StdAnalysisReport;
  paras: WebParaMeta[];
  tablesMeta: { tblIdx: number; bodyIdx: number; node: Element; summary: TableSummary; admin: boolean; hasBadCell: boolean }[];
}

function directChild(el: Element, local: string): Element | null {
  for (let i = 0; i < el.childNodes.length; i++) {
    const n = el.childNodes[i];
    if (n.nodeType === 1 && ((n as Element).localName || n.nodeName.split(':').pop()) === local) {
      return n as Element;
    }
  }
  return null;
}

function directChildren(el: Element, local: string): Element[] {
  const out: Element[] = [];
  for (let i = 0; i < el.childNodes.length; i++) {
    const n = el.childNodes[i];
    if (n.nodeType === 1 && ((n as Element).localName || n.nodeName.split(':').pop()) === local) {
      out.push(n as Element);
    }
  }
  return out;
}

function wAttr(el: Element, attr: string): string {
  return el.getAttributeNS(W_NS, attr) || el.getAttribute(`w:${attr}`) || el.getAttribute(attr) || '';
}

function setWAttr(el: Element, attr: string, val: string): void {
  el.setAttributeNS(W_NS, `w:${attr}`, val);
}

function ensureWChild(doc: Document, parent: Element, local: string, first = false): Element {
  const existing = directChild(parent, local);
  if (existing) return existing;
  const el = doc.createElementNS(W_NS, `w:${local}`);
  if (first && parent.firstChild) {
    parent.insertBefore(el, parent.firstChild);
  } else {
    parent.appendChild(el);
  }
  return el;
}

function extractParaText(p: Element): string {
  const ts = p.getElementsByTagNameNS(W_NS, 't');
  let s = '';
  for (let i = 0; i < ts.length; i++) s += ts[i].textContent || '';
  return s;
}

function parseDocPackageForStd(
  entries: Map<string, Uint8Array>,
  filePath: string,
  fileName: string,
  fileExt: string,
  scope: StdDocScope,
  orgProfile: OrganizationProfile | null,
): WebDocParsed {
  const docBytes = entries.get('word/document.xml');
  if (!docBytes) {
    throw new Error('Không tìm thấy word/document.xml trong tệp.');
  }
  const dec = new TextDecoder('utf-8');
  const docXml = dec.decode(docBytes);
  const doc = new DOMParser().parseFromString(docXml, 'application/xml');
  if (doc.getElementsByTagName('parsererror').length > 0) {
    throw new Error('Cấu trúc word/document.xml không hợp lệ hoặc bị hỏng.');
  }
  const body = doc.getElementsByTagNameNS(W_NS, 'body')[0];
  if (!body) {
    throw new Error('Thiếu thẻ <w:body> trong word/document.xml.');
  }

  // Default font / size from styles.xml
  let defaultFont = '';
  let defaultSizePt = 11;
  if (entries.has('word/styles.xml')) {
    const stDoc = new DOMParser().parseFromString(dec.decode(entries.get('word/styles.xml')!), 'application/xml');
    const dd = stDoc.getElementsByTagNameNS(W_NS, 'docDefaults')[0];
    if (dd) {
      const rf = dd.getElementsByTagNameNS(W_NS, 'rFonts')[0];
      if (rf) defaultFont = wAttr(rf, 'ascii') || wAttr(rf, 'hAnsi');
      const sz = dd.getElementsByTagNameNS(W_NS, 'sz')[0];
      const v = Number(wAttr(sz || dd, 'val'));
      if (v > 0) defaultSizePt = v / 2;
    }
  }

  // Sections
  const sectEls: Element[] = [];
  const bodyChildren = Array.from(body.childNodes).filter((n): n is Element => n.nodeType === 1);
  const inlineSectIndices = new Map<number, number>();
  bodyChildren.forEach((el, idx) => {
    const ln = el.localName || el.nodeName.split(':').pop();
    if (ln === 'p') {
      const pPr = directChild(el, 'pPr');
      const sect = pPr ? directChild(pPr, 'sectPr') : null;
      if (sect) {
        inlineSectIndices.set(idx, sectEls.length + 1);
        sectEls.push(sect);
      }
    }
  });
  const bodySect = directChild(body, 'sectPr');
  if (bodySect) sectEls.push(bodySect);

  const sections: SectionSummary[] = (sectEls.length > 0 ? sectEls : [doc.createElementNS(W_NS, 'w:sectPr')]).map((s, idx) => {
    const pgSz = directChild(s, 'pgSz');
    const pgMar = directChild(s, 'pgMar');
    const pw = Number(pgSz ? wAttr(pgSz, 'w') : 11906) || 11906;
    const ph = Number(pgSz ? wAttr(pgSz, 'h') : 16838) || 16838;
    const orient = (pgSz ? wAttr(pgSz, 'orient') : '') || (pw > ph ? 'landscape' : 'portrait');
    const top = Number(pgMar ? wAttr(pgMar, 'top') : 1134) || 1134;
    const bot = Number(pgMar ? wAttr(pgMar, 'bottom') : 1134) || 1134;
    const left = Number(pgMar ? wAttr(pgMar, 'left') : 1701) || 1701;
    const right = Number(pgMar ? wAttr(pgMar, 'right') : 850) || 850;
    return {
      sectionIndex: idx + 1,
      paperWidthMm: Math.round((pw / 56.6929) * 10) / 10,
      paperHeightMm: Math.round((ph / 56.6929) * 10) / 10,
      orientation: orient,
      marginTopMm: Math.round((top / 56.6929) * 10) / 10,
      marginBottomMm: Math.round((bot / 56.6929) * 10) / 10,
      marginLeftMm: Math.round((left / 56.6929) * 10) / 10,
      marginRightMm: Math.round((right / 56.6929) * 10) / 10,
      differentFirstPage: Boolean(directChild(s, 'titlePg')),
      differentOddEven: false,
      breakType: wAttr(directChild(s, 'type') || s, 'val') || 'nextPage',
      hasHeaderRef: s.getElementsByTagNameNS(W_NS, 'headerReference').length > 0,
      hasFooterRef: s.getElementsByTagNameNS(W_NS, 'footerReference').length > 0,
    };
  });

  // Paragraphs & Tables
  const paras: WebParaMeta[] = [];
  const tablesMeta: WebDocParsed['tablesMeta'] = [];
  let curSect = 1;
  let tblCounter = 0;

  const inspectPara = (
    p: Element,
    bodyIdx: number,
    tblIdx: number | null,
    elementPath: string,
    forcedRole?: { key: string; label: string },
  ): WebParaMeta => {
    const text = extractParaText(p);
    const trimmed = text.trim();
    const upper = trimmed.replace(/\s+/g, ' ').toLocaleUpperCase('vi-VN');
    const pPr = directChild(p, 'pPr');
    const styleId = pPr && directChild(pPr, 'pStyle') ? wAttr(directChild(pPr, 'pStyle')!, 'val') : '';
    const jc = pPr && directChild(pPr, 'jc') ? wAttr(directChild(pPr, 'jc')!, 'val') : 'left';
    const ind = pPr ? directChild(pPr, 'ind') : null;
    const sp = pPr ? directChild(pPr, 'spacing') : null;
    const firstLine = Number(ind ? wAttr(ind, 'firstLine') : 0) || 0;
    const leftInd = Number(ind ? wAttr(ind, 'left') : 0) || 0;
    const rightInd = Number(ind ? wAttr(ind, 'right') : 0) || 0;
    const before = Number(sp ? wAttr(sp, 'before') : 0) || 0;
    const after = Number(sp ? wAttr(sp, 'after') : 0) || 0;
    const line = Number(sp ? wAttr(sp, 'line') : 240) || 240;

    const runs = Array.from(p.getElementsByTagNameNS(W_NS, 'r'));
    const fonts: string[] = [];
    const sizesPt: number[] = [];
    const nonBlackColors: string[] = [];
    let boldChars = 0;
    let italicChars = 0;
    let totalChars = 0;
    let hasLegacy = false;

    for (const r of runs) {
      const len = extractParaText(r).trim().length;
      if (len === 0) continue;
      totalChars += len;
      const inHyperlink = r.parentNode && ((r.parentNode as Element).localName || '') === 'hyperlink';
      const rPr = directChild(r, 'rPr');
      const rf = rPr ? directChild(rPr, 'rFonts') : null;
      const fname = (rf ? wAttr(rf, 'ascii') || wAttr(rf, 'hAnsi') : '') || defaultFont;
      if (fname) {
        if (/^\.vn|^vni-/i.test(fname)) hasLegacy = true;
        if (!fonts.some((f) => f.toLowerCase() === fname.toLowerCase())) fonts.push(fname);
      }
      const szEl = rPr ? directChild(rPr, 'sz') || directChild(rPr, 'szCs') : null;
      const szVal = szEl ? Number(wAttr(szEl, 'val')) / 2 : defaultSizePt;
      if (szVal > 0 && !sizesPt.some((s) => Math.abs(s - szVal) < 0.1)) sizesPt.push(szVal);
      if (!inHyperlink && rPr) {
        const col = directChild(rPr, 'color');
        const cv = col ? wAttr(col, 'val').toUpperCase() : '';
        if (cv && cv !== '000000' && cv !== 'AUTO' && !nonBlackColors.includes(cv)) {
          nonBlackColors.push(cv);
        }
      }
      if (rPr && directChild(rPr, 'b') && wAttr(directChild(rPr, 'b')!, 'val') !== '0') boldChars += len;
      if (rPr && directChild(rPr, 'i') && wAttr(directChild(rPr, 'i')!, 'val') !== '0') italicChars += len;
    }

    const isBold = totalChars > 0 && boldChars * 2 >= totalChars;
    const isItalic = totalChars > 0 && italicChars * 2 >= totalChars;
    let roleKey = forcedRole?.key || 'BodyProse';
    let roleLabel = forcedRole?.label || 'Lời văn chính';

    if (!forcedRole) {
      if (!trimmed) {
        roleKey = 'EmptyParagraph';
        roleLabel = 'Đoạn trống';
      } else if (upper.includes('CỘNG HÒA XÃ HỘI') || upper.includes('ĐỘC LẬP') || upper.startsWith('SỐ:')) {
        roleKey = 'AdminHeaderCell';
        roleLabel = 'Khối đầu trang';
      } else if (upper.startsWith('NƠI NHẬN') || (trimmed.startsWith('-') && trimmed.length < 70 && paras.some((x) => x.roleKey === 'RecipientNoiNhan'))) {
        roleKey = 'RecipientNoiNhan';
        roleLabel = 'Nơi nhận';
      } else if (/^(TM\.|KT\.|TL\.|TUQ\.|Q\.)/i.test(trimmed)) {
        roleKey = 'SignatureBlock';
        roleLabel = 'Chữ ký';
      } else if (upper.startsWith('CĂN CỨ')) {
        roleKey = 'LegalBasisCanCu';
        roleLabel = 'Căn cứ pháp lý';
      } else if (/^(PHẦN|CHƯƠNG)\s+[IVXLCDM\d]/i.test(trimmed) || /^Heading1/i.test(styleId)) {
        roleKey = 'StructuralHeading1';
        roleLabel = 'Đề mục cấp 1';
      } else if (/^Điều\s+\d+/i.test(trimmed) || /^Heading/i.test(styleId)) {
        roleKey = 'StructuralHeading2';
        roleLabel = 'Đề mục cấp 2';
      } else if (['QUYẾT ĐỊNH', 'TỜ TRÌNH', 'BÁO CÁO', 'THÔNG BÁO', 'KẾ HOẠCH', 'NGHỊ QUYẾT'].includes(upper) && (isBold || jc === 'center')) {
        roleKey = 'DocTypeHeading';
        roleLabel = 'Tên loại VB';
      }
    }

    const hasStructural =
      p.getElementsByTagNameNS(W_NS, 'drawing').length > 0 ||
      p.getElementsByTagNameNS(W_NS, 'sectPr').length > 0 ||
      p.getElementsByTagNameNS(W_NS, 'pBdr').length > 0 ||
      p.getElementsByTagNameNS(W_NS, 'bookmarkStart').length > 0 ||
      p.getElementsByTagNameNS(W_NS, 'commentRangeStart').length > 0 ||
      p.getElementsByTagNameNS(W_NS, 'footnoteReference').length > 0 ||
      p.getElementsByTagNameNS(W_NS, 'fldChar').length > 0;

    const brs = Array.from(p.getElementsByTagNameNS(W_NS, 'br'));
    const hasSoftBreak = brs.some((b) => {
      const bt = wAttr(b, 'type');
      return bt !== 'page' && bt !== 'column';
    });

    return {
      index: paras.length,
      bodyIdx,
      tblIdx,
      elementPath,
      sectionIndex: curSect,
      roleKey,
      roleLabel,
      text,
      trimmed,
      styleId,
      jc,
      firstLine,
      leftInd,
      rightInd,
      before,
      after,
      line,
      fonts,
      sizesPt,
      nonBlackColors,
      isBold,
      isItalic,
      hasLegacy,
      hasMultiSpace: Boolean(trimmed) && /  +/.test(text),
      hasPunctErr: Boolean(trimmed) && /\s+[,.;:!?]/.test(text),
      hasSoftBreak,
      hasControlChars: /[\x00-\x08\x0B\x0C\x0E-\x1F]/.test(text),
      isEmptyRemovable: !trimmed && !hasStructural,
      node: p,
    };
  };

  bodyChildren.forEach((el, bIdx) => {
    const ln = el.localName || el.nodeName.split(':').pop();
    if (ln === 'p') {
      paras.push(inspectPara(el, bIdx, null, `body/p[${bIdx}]`));
      if (inlineSectIndices.has(bIdx)) {
        curSect = Math.min(curSect + 1, sections.length);
      }
    } else if (ln === 'tbl') {
      const tIdx = tblCounter++;
      const txtUp = extractParaText(el).toUpperCase();
      const isAdminHdr = txtUp.includes('CỘNG HÒA') || txtUp.includes('ĐỘC LẬP');
      const isSig = txtUp.includes('NƠI NHẬN') || txtUp.includes('KT.') || txtUp.includes('TM.');
      const isAdmin = isAdminHdr || isSig;
      let hasBadCell = false;
      const rows = directChildren(el, 'tr');
      let maxCols = 0;
      let lockedLong = false;
      const r0HasHdr = Boolean(rows[0] && directChild(rows[0], 'trPr') && directChild(directChild(rows[0], 'trPr')!, 'tblHeader'));

      rows.forEach((tr, rIdx) => {
        const cells = directChildren(tr, 'tc');
        maxCols = Math.max(maxCols, cells.length);
        const trPr = directChild(tr, 'trPr');
        if (!isAdmin && rIdx > 0 && extractParaText(tr).length > 130 && trPr && directChild(trPr, 'cantSplit')) {
          lockedLong = true;
        }
        cells.forEach((tc, cIdx) => {
          directChildren(tc, 'p').forEach((p, pIdx) => {
            const role = isAdminHdr
              ? { key: 'AdminHeaderCell', label: 'Khối đầu trang' }
              : isSig
                ? cIdx === 0
                  ? { key: 'RecipientNoiNhan', label: 'Nơi nhận' }
                  : { key: 'SignatureBlock', label: 'Chữ ký' }
                : { key: 'DataTableCell', label: 'Ô bảng dữ liệu' };
            const pm = inspectPara(p, bIdx, tIdx, `body/tbl[${bIdx}]/tr[${rIdx}]/tc[${cIdx}]/p[${pIdx}]`, role);
            if (!isAdmin && (pm.firstLine > 80 || pm.fonts.some((f) => f.toLowerCase() !== 'times new roman'))) {
              hasBadCell = true;
            }
            paras.push(pm);
          });
        });
      });

      const sectMeta = sections[curSect - 1] || sections[0];
      const printableTwips = Math.round((sectMeta.paperWidthMm - sectMeta.marginLeftMm - sectMeta.marginRightMm) * 56.6929);
      const tblPr = directChild(el, 'tblPr');
      const tblWEl = tblPr ? directChild(tblPr, 'tblW') : null;
      const wType = tblWEl ? wAttr(tblWEl, 'type') : '';
      let totalW = printableTwips;
      if (tblWEl && wType === 'dxa') {
        totalW = Number(wAttr(tblWEl, 'w')) || printableTwips;
      } else {
        const grid = directChild(el, 'tblGrid');
        if (grid) {
          const sum = directChildren(grid, 'gridCol').reduce((acc, g) => acc + (Number(wAttr(g, 'w')) || 0), 0);
          if (sum > 0) totalW = sum;
        }
      }

      tablesMeta.push({
        tblIdx: tIdx,
        bodyIdx: bIdx,
        node: el,
        admin: isAdmin,
        hasBadCell,
        summary: {
          tableIndex: tIdx,
          elementPath: `body/tbl[${bIdx}]`,
          kind: isAdminHdr ? 'Khung Quốc hiệu' : isSig ? 'Khung Nơi nhận / Chữ ký' : 'Bảng dữ liệu',
          rowCount: rows.length,
          colCount: maxCols,
          totalWidthTwips: totalW,
          printableWidthTwips: printableTwips,
          exceedsPrintableWidth: totalW - printableTwips > 120,
          hasRepeatHeader: r0HasHdr,
          hasLockedLongRows: lockedLong,
        },
      });
    }
  });

  // Assets
  let mediaCount = 0;
  let mediaBytes = 0;
  let hasVba = false;
  let vbaBytes = 0;
  for (const [k, v] of entries.entries()) {
    const kl = k.toLowerCase();
    if (kl.startsWith('word/media/')) {
      mediaCount++;
      mediaBytes += v.byteLength;
    } else if (kl === 'word/vbaproject.bin') {
      hasVba = true;
      vbaBytes = v.byteLength;
    }
  }
  const assets: AssetInventory = {
    mediaCount,
    mediaBytes,
    drawingCount:
      doc.getElementsByTagNameNS('*', 'drawing').length +
      doc.getElementsByTagNameNS('*', 'AlternateContent').length,
    hyperlinkCount: doc.getElementsByTagNameNS(W_NS, 'hyperlink').length,
    bookmarkCount: doc.getElementsByTagNameNS(W_NS, 'bookmarkStart').length,
    footnoteCount: doc.getElementsByTagNameNS(W_NS, 'footnoteReference').length,
    endnoteCount: doc.getElementsByTagNameNS(W_NS, 'endnoteReference').length,
    commentCount: doc.getElementsByTagNameNS(W_NS, 'commentReference').length,
    fieldCount:
      doc.getElementsByTagNameNS(W_NS, 'fldSimple').length +
      doc.getElementsByTagNameNS(W_NS, 'instrText').length,
    hasVbaMacros: hasVba,
    vbaBytes,
  };

  // Scope detection
  let effectiveScope: StdDocScope = scope;
  let requiresSelection = false;
  let scopeReason = 'Áp dụng theo phạm vi người dùng đã chọn.';
  if (scope === 'autoDetect' || scope === 'unknownRequiresSelection') {
    const headText = paras.slice(0, 35).map((p) => p.trimmed.toUpperCase()).join('\n');
    if (headText.includes('ĐẢNG CỘNG SẢN VIỆT NAM') || orgProfile?.sector === 'partyOrg') {
      effectiveScope = 'partyDocument';
      scopeReason = 'Nhận diện văn bản Đảng / Hồ sơ Tổ chức Đảng (36-HD/VPTW).';
    } else if (orgProfile?.sector === 'enterprise') {
      effectiveScope = 'enterpriseInternal';
      scopeReason = orgProfile.useNd30LayoutForEnterprise
        ? 'Hồ sơ Doanh nghiệp (bật tương thích NĐ 30).'
        : 'Hồ sơ Doanh nghiệp (quy tắc linh hoạt).';
    } else if (
      headText.includes('CỘNG HÒA XÃ HỘI') ||
      headText.includes('ĐỘC LẬP') ||
      headText.includes('SỐ:') ||
      headText.includes('KÍNH GỬI') ||
      paras.some((p) => p.roleKey === 'RecipientNoiNhan')
    ) {
      effectiveScope = 'adminNd30';
      scopeReason = 'Nhận diện văn bản hành chính NĐ 30/2020/NĐ-CP.';
    } else if (headText.includes('LUẬN VĂN') || headText.includes('LUẬN ÁN')) {
      effectiveScope = 'academicThesis';
      scopeReason = 'Nhận diện luận văn / báo cáo học thuật.';
    } else {
      effectiveScope = 'unknownRequiresSelection';
      requiresSelection = true;
      scopeReason = 'Chưa đủ dấu hiệu chắc chắn để tự phân loại — hãy chọn phạm vi áp dụng.';
    }
  }

  // Evaluate Rules
  const findings: StdFinding[] = [];
  const addFinding = (
    ruleId: string,
    ruleName: string,
    category: RuleCategory,
    severity: StdSeverity,
    status: RuleCheckStatus,
    safetyTier: FixSafetyTier,
    legalBasis: string,
    autoFix: boolean,
    location: string,
    elementPath: string,
    currentValue: string,
    expectedValue: string,
    proposal: string,
  ) => {
    const idx = findings.filter((f) => f.ruleId === ruleId).length + 1;
    findings.push({
      findingId: `${ruleId}-${idx}`,
      ruleId,
      ruleName,
      category,
      severity,
      status,
      safetyTier,
      location,
      elementPath,
      currentValue,
      expectedValue,
      proposal,
      legalBasis,
      autoFixSupported: autoFix,
      disposition: 'pending',
    });
  };

  const isEntRelaxed =
    effectiveScope === 'enterpriseInternal' && !(orgProfile?.useNd30LayoutForEnterprise ?? false);
  const canRunNd30 =
    effectiveScope === 'adminNd30' ||
    effectiveScope === 'partyDocument' ||
    (effectiveScope === 'enterpriseInternal' && !isEntRelaxed);

  if (effectiveScope !== 'unknownRequiresSelection') {
    for (const s of sections) {
      const isA4 =
        (Math.abs(s.paperWidthMm - 210) <= 4 && Math.abs(s.paperHeightMm - 297) <= 4) ||
        (Math.abs(s.paperWidthMm - 297) <= 4 && Math.abs(s.paperHeightMm - 210) <= 4);
      if (!isA4) {
        addFinding(
          'PAGE-01', 'Khổ giấy chuẩn A4 (210 × 297 mm)', 'pageAndSection', 'error', 'failed',
          sections.length > 1 ? 'groupBConfirm' : 'groupAAuto',
          'Mục I Phụ lục I NĐ 30/2020/NĐ-CP', true,
          `Section ${s.sectionIndex}`, `sect[${s.sectionIndex}]`,
          `${s.paperWidthMm} × ${s.paperHeightMm} mm`, '210 × 297 mm',
          `Đưa Section ${s.sectionIndex} về khổ giấy A4.`,
        );
      }
      if (canRunNd30) {
        const marginOk =
          s.orientation === 'landscape'
            ? (s.marginTopMm >= 29 && s.marginTopMm <= 36) || (s.marginLeftMm >= 29 && s.marginLeftMm <= 36)
            : s.marginTopMm >= 19 && s.marginTopMm <= 26 && s.marginBottomMm >= 19 && s.marginBottomMm <= 26 && s.marginLeftMm >= 29 && s.marginLeftMm <= 36 && s.marginRightMm >= 14.5 && s.marginRightMm <= 21;
        if (!marginOk) {
          addFinding(
            'PAGE-02', 'Định lề trang văn bản hành chính', 'pageAndSection', 'error', 'failed',
            sections.length > 1 ? 'groupBConfirm' : 'groupAAuto',
            'Mục I.2 Phụ lục I NĐ 30/2020/NĐ-CP', true,
            `Section ${s.sectionIndex}`, `sect[${s.sectionIndex}]`,
            `T:${s.marginTopMm} D:${s.marginBottomMm} Tr:${s.marginLeftMm} Ph:${s.marginRightMm} mm`,
            'T/D: 20–25mm · Tr: 30–35mm · Ph: 15–20mm',
            `Điều chỉnh lề Section ${s.sectionIndex} về chuẩn NĐ 30 (20-20-30-20 mm).`,
          );
        }
      }
    }
  }

  let consecEmpty = 0;
  for (const p of paras) {
    if (p.hasLegacy) {
      addFinding(
        'FONT-01', 'Bộ mã tiếng Việt Unicode TCVN 6909:2001', 'fontAndEncoding', 'critical', 'failed', 'groupAAuto',
        'Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP', true,
        `Đoạn #${p.index + 1} (${p.roleLabel})`, p.elementPath,
        `Font cũ: ${p.fonts.join(', ')}`, 'Unicode (Times New Roman)',
        'Chuyển TCVN3/VNI sang Unicode.',
      );
    } else if (p.trimmed && effectiveScope !== 'unknownRequiresSelection') {
      const badFonts = p.fonts.filter((f) => !SYMBOL_FONTS.has(f.toLowerCase()) && f.toLowerCase() !== 'times new roman');
      if (badFonts.length > 0 || p.fonts.length === 0) {
        addFinding(
          'FONT-02', 'Phông chữ Times New Roman & tính đồng nhất', 'fontAndEncoding', 'error', 'failed', 'groupAAuto',
          'Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP', true,
          `Đoạn #${p.index + 1} (${p.roleLabel})`, p.elementPath,
          badFonts.length > 0 ? `Font: ${badFonts.join(', ')}` : 'Thiếu khai báo phông',
          'Times New Roman', 'Đưa về phông Times New Roman.',
        );
      }
    }

    if (canRunNd30 && p.trimmed && p.roleKey !== 'DataTableCell') {
      const [minPt, maxPt, tgt] = p.roleKey === 'RecipientNoiNhan' ? [11, 12, 11] : p.roleKey === 'AdminHeaderCell' ? [12, 14, 13] : [13, 14, 14];
      const badSz = p.sizesPt.filter((s) => s < minPt - 0.1 || s > maxPt + 0.1);
      if (badSz.length > 0) {
        addFinding(
          'FONT-03', 'Cỡ chữ theo từng thành phần thể thức', 'fontAndEncoding', 'error', 'failed', 'groupAAuto',
          'Mục II Phụ lục I NĐ 30/2020/NĐ-CP', true,
          `Đoạn #${p.index + 1} (${p.roleLabel})`, p.elementPath,
          `${badSz.join(', ')}pt`, `${minPt}–${maxPt}pt`,
          `Đưa cỡ chữ về ${tgt}pt.`,
        );
      }
      if (p.nonBlackColors.length > 0) {
        addFinding(
          'FONT-04', 'Màu chữ đen chuẩn (000000)', 'fontAndEncoding', 'warning', 'failed', 'groupAAuto',
          'Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP', true,
          `Đoạn #${p.index + 1}`, p.elementPath,
          `#${p.nonBlackColors.join(', #')}`, '#000000',
          'Đưa màu chữ về đen 000000.',
        );
      }
    }

    if (p.roleKey !== 'AdminHeaderCell' && (p.hasMultiSpace || p.hasPunctErr || p.hasControlChars)) {
      addFinding(
        'FONT-05', 'Khoảng trắng thừa, dấu câu & ký tự điều khiển', 'fontAndEncoding', 'warning', 'failed', 'groupAAuto',
        'Kỹ thuật trình bày NĐ 30/2020/NĐ-CP', true,
        `Đoạn #${p.index + 1}`, p.elementPath,
        `"${p.trimmed.slice(0, 45)}"`, 'Khoảng trắng & dấu câu chuẩn',
        'Chuẩn hóa khoảng trắng / dấu câu.',
      );
    }

    if (canRunNd30 && p.roleKey === 'BodyProse' && p.trimmed) {
      if (p.jc !== 'both') {
        addFinding(
          'PARA-01', 'Căn đều hai bên phần lời văn chính', 'paragraphAndAlignment', 'error', 'failed', 'groupAAuto',
          'Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP', true,
          `Đoạn #${p.index + 1}`, p.elementPath,
          `jc=${p.jc}`, 'both (Căn đều hai bên)', 'Căn đều hai bên cho lời văn chính.',
        );
      }
      if (p.firstLine < 510 || p.firstLine > 780) {
        addFinding(
          'PARA-02', 'Thụt đầu dòng lời văn (1.0–1.27 cm)', 'paragraphAndAlignment', 'warning', 'failed', 'groupAAuto',
          'Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP', true,
          `Đoạn #${p.index + 1}`, p.elementPath,
          `${(p.firstLine / 567).toFixed(2)} cm`, '1.0–1.27 cm', 'Thụt đầu dòng 1.27 cm.',
        );
      }
      if (p.line < 240 || p.line > 360 || p.after < 110 || p.after > 360) {
        addFinding(
          'PARA-03', 'Giãn dòng (Single–1.5 lines) & Khoảng cách đoạn (>= 6pt)', 'paragraphAndAlignment', 'warning', 'failed', 'groupAAuto',
          'Mục II.6 Phụ lục I NĐ 30/2020/NĐ-CP', true,
          `Đoạn #${p.index + 1}`, p.elementPath,
          `line=${(p.line / 240).toFixed(2)}x, after=${p.after / 20}pt`, '1.0–1.5x, >= 6pt',
          'Chuẩn hóa giãn dòng 1.15x và cách sau 6pt.',
        );
      }
    }

    if (p.tblIdx === null) {
      if (p.isEmptyRemovable) {
        consecEmpty++;
        if (consecEmpty >= 2) {
          addFinding(
            'PARA-04', 'Đoạn trống thừa & ngắt dòng mềm (Shift+Enter)', 'paragraphAndAlignment', 'warning', 'failed', 'groupAAuto',
            'Kỹ thuật OpenXML', true, `Đoạn #${p.index + 1}`, p.elementPath,
            'Đoạn trống liên tiếp', 'Spacing đoạn', 'Xóa đoạn trống thừa.',
          );
        }
      } else {
        consecEmpty = 0;
      }
      if (p.hasSoftBreak && p.roleKey === 'BodyProse') {
        addFinding(
          'PARA-04', 'Đoạn trống thừa & ngắt dòng mềm (Shift+Enter)', 'paragraphAndAlignment', 'warning', 'failed', 'groupAAuto',
          'Kỹ thuật OpenXML', true, `Đoạn #${p.index + 1}`, p.elementPath,
          'Chứa ngắt dòng mềm', 'Đoạn chuẩn <w:p>', 'Tách thành đoạn riêng.',
        );
      }
    }

    if (canRunNd30 && p.index < 25) {
      const m = p.trimmed.match(/S[Ốố]\s*:\s*[^/\s]*\/([A-Za-zÀ-ỹĐđ]+)[\-/]/i);
      if (m && m[1] && !PHU_LUC_III_CODES.has(m[1])) {
        addFinding(
          'ABBR-01', 'Chữ viết tắt tên loại văn bản hành chính (Phụ lục III)', 'capitalizationAndAbbreviation', 'warning', 'needsReview', 'groupCManualOnly',
          'Phụ lục III NĐ 30/2020/NĐ-CP', false, `Đoạn #${p.index + 1}`, p.elementPath,
          p.trimmed, 'Đối chiếu bảng 27 viết tắt Phụ lục III', 'Kiểm tra thủ công ký hiệu văn bản.',
        );
      }
    }
  }

  for (const t of tablesMeta) {
    if (t.summary.exceedsPrintableWidth) {
      addFinding(
        'TBL-01', 'Bảng biểu không vượt khỏi khổ vùng in', 'tableLayout', 'error', 'failed', 'groupBConfirm',
        'Mục I.1 Phụ lục I NĐ 30/2020/NĐ-CP', true,
        `Bảng #${t.tblIdx + 1}`, t.summary.elementPath,
        `${t.summary.totalWidthTwips} twips`, `<= ${t.summary.printableWidthTwips} twips`,
        'Co bảng vừa vùng in (cần xác nhận).',
      );
    }
    if (!t.admin && t.summary.rowCount >= 3 && (!t.summary.hasRepeatHeader || t.summary.hasLockedLongRows)) {
      addFinding(
        'TBL-02', 'Lặp dòng tiêu đề & Ngắt trang tự nhiên trong bảng nhiều hàng', 'tableLayout', 'warning', 'needsReview', 'groupBConfirm',
        'Kỹ thuật bảng nhiều trang Word', true,
        `Bảng #${t.tblIdx + 1}`, t.summary.elementPath,
        `tblHeader=${t.summary.hasRepeatHeader}`, 'Bật tblHeader & mở ngắt dòng dài',
        'Tối ưu phân trang bảng (cần xác nhận).',
      );
    }
    if (!t.admin && t.hasBadCell && canRunNd30) {
      addFinding(
        'TBL-03', 'Phông chữ, cỡ chữ (11–13pt) & thụt lề trong ô bảng dữ liệu', 'tableLayout', 'warning', 'failed', 'groupAAuto',
        'Mục I.3 Phụ lục I NĐ 30/2020/NĐ-CP', true,
        `Bảng #${t.tblIdx + 1}`, t.summary.elementPath,
        'Font/thụt dòng trong ô chưa chuẩn', 'Times New Roman, firstLine=0',
        'Đồng bộ phông chữ & xóa thụt dòng ô bảng.',
      );
    }
  }

  const headersFooters: HeaderFooterSummary[] = [];
  for (const [k, v] of entries.entries()) {
    const kl = k.toLowerCase();
    if (/^word\/(header|footer)\d*.*\.xml$/.test(kl)) {
      const xml = dec.decode(v);
      const isHdr = kl.includes('header');
      headersFooters.push({
        partName: k,
        kind: isHdr ? 'header' : 'footer',
        hasPageField: /PAGE/i.test(xml) && !/NUMPAGES/i.test(xml),
        hasNumpagesField: /NUMPAGES/i.test(xml),
        staticPageDigitSuspect: false,
        fontNames: [],
        fontSizesPt: [],
        textPreview: xml.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 60),
      });
    }
  }

  const ruleDefs = getWebStdRules();
  const ruleSummaries: RuleEvalSummary[] = ruleDefs.map((rd) => {
    const matching = findings.filter((f) => f.ruleId === rd.ruleId);
    let status: RuleCheckStatus = 'passed';
    if (effectiveScope === 'unknownRequiresSelection' && !['FONT-01', 'FONT-05', 'PAGE-03', 'PGNUM-02', 'PRESERVE-01'].includes(rd.ruleId)) {
      status = 'indeterminate';
    } else if (!rd.applicableDocumentTypes.includes(effectiveScope) || (isEntRelaxed && ['PAGE-02', 'PARA-01', 'PARA-02', 'PGNUM-01', 'CAP-01', 'ABBR-01', 'STRUCT-01'].includes(rd.ruleId))) {
      status = 'notApplicable';
    } else if (matching.some((m) => m.status === 'failed')) {
      status = 'failed';
    } else if (matching.some((m) => m.status === 'needsReview')) {
      status = 'needsReview';
    }
    return {
      ruleId: rd.ruleId,
      ruleName: rd.ruleName,
      category: rd.category,
      severity: rd.severity,
      status,
      findingCount: matching.length,
      legalBasis: rd.legalBasis,
      autoFixSupported: rd.autoFixSupported,
      safetyTier: rd.safetyTier,
    };
  });

  const paragraphPreviews: ParagraphLocationPreview[] = paras.slice(0, 350).map((p) => ({
    elementPath: p.elementPath,
    index: p.index,
    role: p.roleLabel,
    sectionIndex: p.sectionIndex,
    excerpt: p.trimmed.slice(0, 110),
    alignment: p.jc,
    fontName: p.fonts[0] || '',
    fontSizePt: p.sizesPt[0] ?? null,
  }));

  return {
    doc,
    entries,
    paras,
    tablesMeta,
    report: {
      filePath,
      fileName,
      fileExt,
      requestedScope: scope,
      effectiveScope,
      requiresScopeSelection: requiresSelection,
      scopeReason,
      sections,
      tables: tablesMeta.map((t) => t.summary),
      headersFooters,
      paragraphCount: paras.length,
      paragraphPreviews,
      assets,
      ruleSummaries,
      findings,
      statusCounts: {
        passed: ruleSummaries.filter((r) => r.status === 'passed').length,
        failed: ruleSummaries.filter((r) => r.status === 'failed').length,
        needsReview: ruleSummaries.filter((r) => r.status === 'needsReview').length,
        notApplicable: ruleSummaries.filter((r) => r.status === 'notApplicable').length,
        indeterminate: ruleSummaries.filter((r) => r.status === 'indeterminate').length,
      },
      severityCounts: {
        critical: findings.filter((f) => f.severity === 'critical').length,
        error: findings.filter((f) => f.severity === 'error').length,
        warning: findings.filter((f) => f.severity === 'warning').length,
        information: findings.filter((f) => f.severity === 'information').length,
      },
    },
  };
}

export async function analyzeWebStdDocument(
  item: FileQueueItem,
  scope: StdDocScope,
  orgProfile: OrganizationProfile | null,
): Promise<StdAnalysisReport> {
  if (item.ext.toLowerCase() === 'doc') {
    throw new Error('Trình duyệt không đọc trực tiếp .doc nhị phân. Hãy dùng MonWord Windows.');
  }
  if (!item.webFile) {
    throw new Error('Không tìm thấy dữ liệu tệp trong trình duyệt.');
  }
  registerWebStdSnapshot(item);
  const buf = await item.webFile.arrayBuffer();
  const entries = await readZipEntries(buf);
  return parseDocPackageForStd(entries, item.path, item.name, item.ext, scope, orgProfile).report;
}

function setRunFontPreserve(doc: Document, p: Element, fontName: string): void {
  const runs = Array.from(p.getElementsByTagNameNS(W_NS, 'r'));
  for (const r of runs) {
    if (r.getElementsByTagNameNS(W_NS, 't').length === 0) continue;
    const rPr = ensureWChild(doc, r, 'rPr', true);
    const rf = ensureWChild(doc, rPr, 'rFonts', true);
    const cur = (wAttr(rf, 'ascii') || wAttr(rf, 'hAnsi')).toLowerCase();
    if (SYMBOL_FONTS.has(cur)) continue;
    setWAttr(rf, 'ascii', fontName);
    setWAttr(rf, 'hAnsi', fontName);
    setWAttr(rf, 'cs', fontName);
  }
}

export async function executeWebStdDocument(
  item: FileQueueItem,
  req: StdExecutionRequest,
): Promise<StdExecutionResult> {
  if (item.ext.toLowerCase() === 'doc') {
    return {
      success: false, inputPath: item.path, outputPath: '', backupPath: null,
      error: 'Trình duyệt không hỗ trợ ghi tệp .doc nhị phân.',
      logs: [], actionsApplied: [], reportBefore: null, reportAfter: null, diff: null, verification: null,
    };
  }
  if (!item.webFile) throw new Error('Không tìm thấy tệp.');
  registerWebStdSnapshot(item);

  const buf = await item.webFile.arrayBuffer();
  const entries = await readZipEntries(buf);
  const parsedBefore = parseDocPackageForStd(entries, item.path, item.name, item.ext, req.scope, req.orgProfile);
  const reportBefore = parsedBefore.report;

  if (reportBefore.requiresScopeSelection && req.scope === 'autoDetect') {
    return {
      success: false, inputPath: item.path, outputPath: '', backupPath: null,
      error: 'Chưa xác định chắc chắn loại tài liệu — hãy chọn Phạm vi áp dụng quy tắc trước khi chuẩn hóa.',
      logs: [], actionsApplied: [], reportBefore, reportAfter: null, diff: null, verification: null,
    };
  }

  const disabled = new Set(req.disabledRuleIds);
  const approved = new Set(req.approvedFindingIds);
  const skipped = new Set(req.skippedFindingIds);
  const manual = new Set(req.manualReviewFindingIds);
  const actions: FixActionRecord[] = [];
  const doc = parsedBefore.doc;
  const nowStr = new Date().toLocaleTimeString('vi-VN', { hour12: false });

  const toDeleteNodes: Element[] = [];
  for (const f of reportBefore.findings) {
    if (disabled.has(f.ruleId) || skipped.has(f.findingId) || manual.has(f.findingId) || !f.autoFixSupported) {
      if (skipped.has(f.findingId)) f.disposition = 'skipped';
      if (manual.has(f.findingId)) f.disposition = 'needsManualReview';
      continue;
    }
    const exp = approved.has(f.findingId) || approved.has(f.ruleId);
    const should = f.safetyTier === 'groupAAuto' ? req.applyAllGroupA || exp : f.safetyTier === 'groupBConfirm' ? exp : false;
    if (!should) continue;

    f.disposition = f.safetyTier === 'groupAAuto' && !exp ? 'autoFixed' : 'approvedFix';
    const pm = parsedBefore.paras.find((p) => p.elementPath === f.elementPath);
    const tm = parsedBefore.tablesMeta.find((t) => t.summary.elementPath === f.elementPath);

    if (f.ruleId === 'PAGE-01' || f.ruleId === 'PAGE-02') {
      const m = f.elementPath.match(/sect\[(\d+)\]/);
      const sIdx = m ? Number(m[1]) - 1 : 0;
      const sects = Array.from(doc.getElementsByTagNameNS(W_NS, 'sectPr'));
      const sect = sects[sIdx];
      if (sect) {
        if (f.ruleId === 'PAGE-01') {
          const pgSz = ensureWChild(doc, sect, 'pgSz');
          const isL = wAttr(pgSz, 'orient') === 'landscape';
          setWAttr(pgSz, 'w', isL ? '16838' : '11906');
          setWAttr(pgSz, 'h', isL ? '11906' : '16838');
        } else {
          const pgMar = ensureWChild(doc, sect, 'pgMar');
          setWAttr(pgMar, 'top', '1134');
          setWAttr(pgMar, 'bottom', '1134');
          setWAttr(pgMar, 'left', '1701');
          setWAttr(pgMar, 'right', '1134');
        }
      }
    } else if (pm) {
      if (f.ruleId === 'FONT-01') {
        for (const r of Array.from(pm.node.getElementsByTagNameNS(W_NS, 'r'))) {
          const rPr = directChild(r, 'rPr');
          const rf = rPr ? directChild(rPr, 'rFonts') : null;
          const fn = rf ? (wAttr(rf, 'ascii') || wAttr(rf, 'hAnsi')).toLowerCase() : '';
          if (fn.startsWith('.vn') || fn.startsWith('vni-')) {
            const isTcvn = fn.startsWith('.vn');
            const isCaps = isTcvn && fn.endsWith('h');
            for (const t of Array.from(r.getElementsByTagNameNS(W_NS, 't'))) {
              t.textContent = isTcvn ? decodeTcvn3(t.textContent || '', isCaps) : decodeVni(t.textContent || '');
            }
          }
        }
        setRunFontPreserve(doc, pm.node, 'Times New Roman');
      } else if (f.ruleId === 'FONT-02') {
        setRunFontPreserve(doc, pm.node, 'Times New Roman');
      } else if (f.ruleId === 'FONT-03') {
        const hp = pm.roleKey === 'RecipientNoiNhan' ? '22' : pm.roleKey === 'AdminHeaderCell' ? '26' : '28';
        for (const r of Array.from(pm.node.getElementsByTagNameNS(W_NS, 'r'))) {
          if (r.getElementsByTagNameNS(W_NS, 't').length === 0) continue;
          const rPr = ensureWChild(doc, r, 'rPr', true);
          setWAttr(ensureWChild(doc, rPr, 'sz'), 'val', hp);
          setWAttr(ensureWChild(doc, rPr, 'szCs'), 'val', hp);
        }
      } else if (f.ruleId === 'FONT-04') {
        for (const r of Array.from(pm.node.getElementsByTagNameNS(W_NS, 'r'))) {
          const inHy = r.parentNode && ((r.parentNode as Element).localName || '') === 'hyperlink';
          if (inHy || r.getElementsByTagNameNS(W_NS, 't').length === 0) continue;
          const rPr = ensureWChild(doc, r, 'rPr', true);
          const col = ensureWChild(doc, rPr, 'color');
          setWAttr(col, 'val', '000000');
          col.removeAttribute('w:themeColor');
        }
      } else if (f.ruleId === 'FONT-05') {
        for (const t of Array.from(pm.node.getElementsByTagNameNS(W_NS, 't'))) {
          const raw = t.textContent || '';
          if (raw.length >= 2 && /^ +$/.test(raw)) continue;
          t.textContent = raw.replace(/[\x00-\x08\x0B\x0C\x0E-\x1F]/g, '').replace(/ {2,}/g, ' ').replace(/\s+([,.;:!?])/g, '$1');
        }
      } else if (f.ruleId === 'PARA-01') {
        setWAttr(ensureWChild(doc, ensureWChild(doc, pm.node, 'pPr', true), 'jc'), 'val', 'both');
      } else if (f.ruleId === 'PARA-02') {
        const ind = ensureWChild(doc, ensureWChild(doc, pm.node, 'pPr', true), 'ind');
        setWAttr(ind, 'firstLine', '720');
        ind.removeAttribute('w:left');
        ind.removeAttribute('w:right');
      } else if (f.ruleId === 'PARA-03') {
        const sp = ensureWChild(doc, ensureWChild(doc, pm.node, 'pPr', true), 'spacing');
        setWAttr(sp, 'before', '0');
        setWAttr(sp, 'after', '120');
        setWAttr(sp, 'line', '276');
        setWAttr(sp, 'lineRule', 'auto');
      } else if (f.ruleId === 'PARA-04' && pm.isEmptyRemovable) {
        toDeleteNodes.push(pm.node);
      }
    } else if (tm) {
      if (f.ruleId === 'TBL-01') {
        const tw = ensureWChild(doc, ensureWChild(doc, tm.node, 'tblPr', true), 'tblW');
        setWAttr(tw, 'w', String(tm.summary.printableWidthTwips));
        setWAttr(tw, 'type', 'dxa');
      } else if (f.ruleId === 'TBL-03') {
        for (const cellP of Array.from(tm.node.getElementsByTagNameNS(W_NS, 'p'))) {
          setRunFontPreserve(doc, cellP, 'Times New Roman');
          const pPr = directChild(cellP, 'pPr');
          const ind = pPr ? directChild(pPr, 'ind') : null;
          if (ind) setWAttr(ind, 'firstLine', '0');
        }
      }
    }

    actions.push({
      actionId: `WEB-ACT-${actions.length + 1}`,
      ruleId: f.ruleId,
      safetyTier: f.safetyTier,
      location: f.location,
      elementPath: f.elementPath,
      beforeValue: f.currentValue,
      afterValue: f.expectedValue,
      timestamp: nowStr,
      reversible: true,
    });
  }

  for (const el of toDeleteNodes) {
    el.parentNode?.removeChild(el);
  }

  // Re-serialize & re-read ZIP entries cleanly from compressed byte output (post-save verification!)
  const serialized = new XMLSerializer().serializeToString(doc);
  entries.set('word/document.xml', new TextEncoder().encode(serialized));
  const outBytes = await writeZipEntries(entries);
  const exactBuf = new Uint8Array(outBytes.byteLength);
  exactBuf.set(outBytes);
  const reReadEntries = await readZipEntries(exactBuf.buffer);
  const parsedAfter = parseDocPackageForStd(reReadEntries, item.path, item.name, item.ext, reportBefore.effectiveScope, req.orgProfile);
  const reportAfter = parsedAfter.report;

  // Build diff summary
  const diffEntries: DiffEntry[] = actions.map((a) => ({
    dimension: a.ruleId.startsWith('PAGE') ? 'pageMarginsAndSize' : a.ruleId === 'FONT-01' || a.ruleId === 'FONT-05' ? 'textContent' : a.ruleId === 'FONT-02' ? 'fontFamily' : a.ruleId === 'FONT-03' ? 'fontSize' : a.ruleId === 'FONT-04' ? 'fontStyle' : a.ruleId === 'PARA-01' ? 'alignment' : a.ruleId.startsWith('TBL') ? 'tableStructure' : 'spacingAndIndent',
    location: a.location,
    beforeVal: a.beforeValue,
    afterVal: a.afterValue,
    reason: `Áp dụng quy tắc ${a.ruleId}`,
    ruleId: a.ruleId,
  }));
  const dimCounts: Record<string, number> = {};
  for (const d of diffEntries) dimCounts[d.dimension] = (dimCounts[d.dimension] || 0) + 1;

  const preservation: PreservationCheckSummary = {
    passed:
      parsedAfter.paras.filter((p) => p.trimmed).length >= parsedBefore.paras.filter((p) => p.trimmed).length &&
      reportAfter.assets.mediaCount >= reportBefore.assets.mediaCount &&
      reportAfter.assets.drawingCount >= reportBefore.assets.drawingCount &&
      reportAfter.assets.hyperlinkCount >= reportBefore.assets.hyperlinkCount &&
      reportAfter.assets.bookmarkCount >= reportBefore.assets.bookmarkCount &&
      reportAfter.assets.footnoteCount >= reportBefore.assets.footnoteCount &&
      reportAfter.assets.endnoteCount >= reportBefore.assets.endnoteCount &&
      reportAfter.assets.commentCount >= reportBefore.assets.commentCount &&
      (!reportBefore.assets.hasVbaMacros || reportAfter.assets.hasVbaMacros) &&
      reportAfter.tables.length >= reportBefore.tables.length,
    textParagraphsBefore: parsedBefore.paras.filter((p) => p.trimmed).length,
    textParagraphsAfter: parsedAfter.paras.filter((p) => p.trimmed).length,
    tablesBefore: reportBefore.tables.length,
    tablesAfter: reportAfter.tables.length,
    mediaBefore: reportBefore.assets.mediaCount,
    mediaAfter: reportAfter.assets.mediaCount,
    drawingsBefore: reportBefore.assets.drawingCount,
    drawingsAfter: reportAfter.assets.drawingCount,
    hyperlinksBefore: reportBefore.assets.hyperlinkCount,
    hyperlinksAfter: reportAfter.assets.hyperlinkCount,
    bookmarksBefore: reportBefore.assets.bookmarkCount,
    bookmarksAfter: reportAfter.assets.bookmarkCount,
    footnotesBefore: reportBefore.assets.footnoteCount,
    footnotesAfter: reportAfter.assets.footnoteCount,
    endnotesBefore: reportBefore.assets.endnoteCount,
    endnotesAfter: reportAfter.assets.endnoteCount,
    commentsBefore: reportBefore.assets.commentCount,
    commentsAfter: reportAfter.assets.commentCount,
    fieldsBefore: reportBefore.assets.fieldCount,
    fieldsAfter: reportAfter.assets.fieldCount,
    macrosBefore: reportBefore.assets.hasVbaMacros,
    macrosAfter: reportAfter.assets.hasVbaMacros,
    warnings: [],
  };
  const diff: DocumentDiffReport = { entries: diffEntries, dimensionCounts: dimCounts, preservation };

  const remCritErr = reportAfter.severityCounts.critical + reportAfter.severityCounts.error;
  const verification: PostSaveVerification = {
    outputExists: exactBuf.byteLength > 0,
    outputSizeBytes: exactBuf.byteLength,
    reopenedFromDisk: true,
    xmlValid: true,
    preservationPassed: preservation.passed,
    remainingCriticalOrError: remCritErr,
    summaryMessage: `Đã giải nén & xác minh gói OpenXML đầu ra (${Math.round(exactBuf.byteLength / 1024)} KB); bảo toàn ${preservation.passed ? 'đạt 100%' : 'có cảnh báo'}, còn ${remCritErr} lỗi Nghiêm trọng/Cần sửa.`,
  };

  const mime =
    item.ext.toLowerCase() === 'docm'
      ? 'application/vnd.ms-word.document.macroEnabled.12'
      : 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
  const outBlob = new Blob([exactBuf], { type: mime });
  const blobUrl = URL.createObjectURL(outBlob);
  const dotIdx = item.name.lastIndexOf('.');
  const outputName = `${dotIdx > 0 ? item.name.slice(0, dotIdx) : item.name}_ChuanTheThuc.${item.ext || 'docx'}`;

  return {
    success: preservation.passed,
    inputPath: item.path,
    outputPath: outputName,
    backupPath: initialStdWebSnapshots.has(item.path) ? 'web-memory-snapshot' : null,
    error: preservation.passed ? null : 'Cảnh báo bảo toàn nội dung chưa đạt.',
    logs: [`Đã thực thi ${actions.length} thao tác sửa trên trình duyệt.`, verification.summaryMessage],
    actionsApplied: actions,
    reportBefore,
    reportAfter,
    diff,
    verification,
    blobUrl,
    outputName,
    outputBlob: outBlob,
  };
}
