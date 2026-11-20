import {
  ADMIN_DOC_TYPE_META,
  type AdminDocType,
  type DetectedHeaderFields,
  type FileQueueItem,
  type HeaderActionMode,
  type HeaderApplyResult,
  type HeaderBuilderConfig,
  type HeaderCandidateSummary,
  type HeaderClassification,
  type HeaderFinding,
  type HeaderInspectionResult,
  type HeaderLayoutKind,
  type HeaderRuleMeta,
  type OrganizationProfile,
} from '../types';
import { readZipEntries, writeZipEntries } from './webDocxEngine';

const W_NS = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main';
const ORG_STORAGE_KEY = 'monword_org_profiles_v1';

// Stores the first unmodified File per web queue path for "Khôi phục nội dung ban đầu"
const initialWebSnapshots = new Map<string, File>();

export function registerWebInitialSnapshot(item: FileQueueItem): void {
  if (item.webFile && !initialWebSnapshots.has(item.path)) {
    initialWebSnapshots.set(item.path, item.webFile);
  }
}

export function getWebInitialSnapshot(path: string): File | undefined {
  return initialWebSnapshots.get(path);
}

// ============================================================================
// PHASE 1: WEB/MOBILE ORGANIZATION PROFILE STORAGE (localStorage)
// ============================================================================

export function listWebOrgProfiles(): OrganizationProfile[] {
  try {
    const raw = localStorage.getItem(ORG_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as OrganizationProfile[]) : [];
  } catch {
    return [];
  }
}

function saveWebOrgList(list: OrganizationProfile[]): void {
  try {
    localStorage.setItem(ORG_STORAGE_KEY, JSON.stringify(list));
  } catch {
    // ignore quota error
  }
}

export function saveWebOrgProfile(profile: OrganizationProfile): OrganizationProfile[] {
  const existing = listWebOrgProfiles();
  const normalized: OrganizationProfile = {
    ...profile,
    id: profile.id?.trim() || `org-${Date.now()}`,
    updatedAt: new Date().toISOString(),
  };
  const idx = existing.findIndex((p) => p.id === normalized.id);
  const next =
    idx >= 0
      ? existing.map((p, i) => (i === idx ? normalized : p))
      : [...existing, normalized];
  saveWebOrgList(next);
  return next;
}

export function deleteWebOrgProfile(id: string): OrganizationProfile[] {
  const next = listWebOrgProfiles().filter((p) => p.id !== id);
  saveWebOrgList(next);
  return next;
}

export function exportWebOrgProfiles(): string {
  return JSON.stringify(listWebOrgProfiles(), null, 2);
}

export function importWebOrgProfiles(json: string): OrganizationProfile[] {
  const parsed = JSON.parse(json);
  const incoming: OrganizationProfile[] = Array.isArray(parsed)
    ? parsed
    : parsed && typeof parsed === 'object' && Array.isArray(parsed.profiles)
      ? parsed.profiles
      : parsed && typeof parsed === 'object' && typeof parsed.issuingOrg === 'string'
        ? [parsed as OrganizationProfile]
        : [];
  if (incoming.length === 0) {
    throw new Error('JSON không chứa danh sách hồ sơ tổ chức hợp lệ.');
  }
  const existing = listWebOrgProfiles();
  const byId = new Map<string, OrganizationProfile>();
  for (const p of existing) byId.set(p.id, p);
  for (const raw of incoming) {
    const id = raw.id?.trim() || `org-imp-${Math.random().toString(36).slice(2, 9)}`;
    byId.set(id, {
      ...raw,
      id,
      updatedAt: new Date().toISOString(),
    });
  }
  const merged = Array.from(byId.values());
  saveWebOrgList(merged);
  return merged;
}

// ============================================================================
// LEGAL RULES CATALOG (NĐ 30/2020/NĐ-CP & 36-HD/VPTW)
// ============================================================================

export function getWebHeaderRules(): HeaderRuleMeta[] {
  return [
    {
      ruleId: 'ND30-PLI-01',
      title: 'Quốc hiệu & Tiêu ngữ',
      description:
        'CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM (cỡ 12-13pt, đứng, đậm, IN HOA) và Độc lập - Tự do - Hạnh phúc (cỡ 13-14pt, đứng, đậm, gạch chân nét liền bên dưới).',
      obligation: 'mandatory',
      scope: 'Cơ quan nhà nước, đơn vị sự nghiệp công lập; Doanh nghiệp tự chọn',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.1',
    },
    {
      ruleId: 'ND30-PLI-02',
      title: 'Tên cơ quan ban hành & cơ quan chủ quản',
      description:
        'Tên cơ quan chủ quản (nếu có) cỡ 12-13pt, đứng, thường/IN HOA; tên cơ quan ban hành cỡ 12-13pt, đứng, đậm, IN HOA, đường kẻ nét liền 1/3 đến 1/2 dòng.',
      obligation: 'mandatory',
      scope: 'Mọi văn bản hành chính có cơ quan ban hành',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.2',
    },
    {
      ruleId: 'ND30-PLI-03',
      title: 'Số và ký hiệu văn bản',
      description:
        'Ghi đúng cấu trúc Số: <số>/<tên viết tắt loại VB>-<tên viết tắt CQ ban hành>; chữ "Số:" đứng thường cỡ 13pt, không tự sinh số trái quy trình.',
      obligation: 'mandatory',
      scope: 'Văn bản hành chính chính thức; bản dự thảo để trống chờ cấp số',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.3',
    },
    {
      ruleId: 'ND30-PLI-04',
      title: 'Địa danh và ngày, tháng, năm ban hành',
      description:
        'Ghi đúng "<Địa danh>, ngày ... tháng ... năm ..." căn giữa dưới Tiêu ngữ, kiểu chữ nghiêng, cỡ 13-14pt; ngày dưới 10 và tháng 1-2 không bắt buộc nhưng NĐ 30 dùng 2 chữ số cho ngày <10, tháng 1-2.',
      obligation: 'mandatory',
      scope: 'Mọi văn bản ban hành chính thức',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.4',
    },
    {
      ruleId: 'ND30-PLI-05',
      title: 'Tên loại và trích yếu (Văn bản có tên loại)',
      description:
        'Tên loại (QUYẾT ĐỊNH, TỜ TRÌNH, BÁO CÁO, ...) đặt giữa, cỡ 13-14pt đứng đậm IN HOA; trích yếu đặt ngay dưới, đứng đậm cỡ 13-14pt, có đường kẻ ngang nét liền dài 1/3-1/2 dòng.',
      obligation: 'docTypeConditional',
      scope: 'Áp dụng cho văn bản có tên loại (trừ Công văn)',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.5.a',
    },
    {
      ruleId: 'ND30-PLI-06',
      title: 'Trích yếu Công văn (V/v ...)',
      description:
        'Công văn không ghi chữ CÔNG VĂN ở giữa; dòng "V/v ..." đặt ở ô trái dưới Số, ký hiệu, kiểu đứng thường cỡ 12-13pt.',
      obligation: 'docTypeConditional',
      scope: 'Áp dụng riêng cho Công văn',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.5.b',
    },
    {
      ruleId: 'ND30-PLI-07',
      title: 'Bảng không viền 2 cột chia khối đầu trang',
      description:
        'Khối tên cơ quan (trái, ~40-45%) và khối Quốc hiệu/Tiêu ngữ (phải, ~55-60%) đặt trong bảng 2 cột không hiển thị viền, bảo toàn logo/hình ảnh nếu có.',
      obligation: 'recommended',
      scope: 'Văn bản hành chính NĐ 30',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Bảng bố cục mẫu',
    },
    {
      ruleId: 'ND30-PLI-08',
      title: 'Dấu độ mật / độ khẩn',
      description:
        'Khi văn bản có yếu tố mật/khẩn (MẬT, TỐI MẬT, KHẨN, HỎA TỐC), trình bày nổi bật đúng vị trí theo quy định.',
      obligation: 'caseSpecific',
      scope: 'Chỉ áp dụng khi người dùng chỉ định hoặc tài liệu gốc có dấu mật/khẩn',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.10',
    },
    {
      ruleId: 'ND30-PLI-09',
      title: 'Hồ sơ Doanh nghiệp / Nội bộ linh hoạt',
      description:
        'Cho phép giữ mẫu tiêu đề riêng hoặc dùng bố cục tương thích NĐ 30 nếu doanh nghiệp bật tùy chọn tương thích.',
      obligation: 'orgConfigurable',
      scope: 'Hồ sơ sector = Doanh nghiệp / Khác',
      legalBasis: 'Quy chế văn thư doanh nghiệp / NĐ 30 tham khảo',
    },
    {
      ruleId: 'DANG-36HD-01',
      title: 'Thể thức đầu văn bản Đảng',
      description:
        'Cơ quan của Đảng dùng tiêu đề "ĐẢNG CỘNG SẢN VIỆT NAM" ở cột phải; cơ quan ban hành/chủ quản ở cột trái theo Hướng dẫn 36-HD/VPTW.',
      obligation: 'docTypeConditional',
      scope: 'Áp dụng khi Organization Profile có sector = Tổ chức Đảng',
      legalBasis: 'Hướng dẫn 36-HD/VPTW năm 2018 của Văn phòng Trung ương Đảng',
    },
  ];
}

function applicableRulesFor(
  org: OrganizationProfile | null,
  docType: AdminDocType,
): HeaderRuleMeta[] {
  const all = getWebHeaderRules();
  const isParty = org?.sector === 'partyOrg';
  const isEnt = org?.sector === 'enterprise' || org?.sector === 'other';
  const entNd30 = org?.useNd30LayoutForEnterprise ?? false;

  return all.filter((r) => {
    if (r.ruleId === 'DANG-36HD-01') return isParty;
    if (r.ruleId === 'ND30-PLI-01') return !isParty && (!isEnt || entNd30);
    if (r.ruleId === 'ND30-PLI-05') return docType !== 'congVan';
    if (r.ruleId === 'ND30-PLI-06') return docType === 'congVan';
    if (r.ruleId === 'ND30-PLI-09') return isEnt;
    return true;
  });
}

// ============================================================================
// XML & TEXT UTILITIES
// ============================================================================

function collapseSpaces(s: string): string {
  return s.replace(/\s+/g, ' ').trim();
}

function normalizeUpper(s: string): string {
  return collapseSpaces(s).toLocaleUpperCase('vi-VN');
}

function stripDiacriticsUpper(s: string): string {
  return s
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/đ/g, 'd')
    .replace(/Đ/g, 'D')
    .toUpperCase()
    .replace(/\s+/g, ' ')
    .trim();
}

function isDividerText(s: string): boolean {
  const t = s.trim();
  if (!t || t.length < 3) return false;
  return /^[\-_–—=~*_.\s]+$/.test(t) && /[\-_–—=~*]/.test(t);
}

function getDirectChildrenByLocalName(parent: Element, localName: string): Element[] {
  const out: Element[] = [];
  for (let i = 0; i < parent.childNodes.length; i++) {
    const n = parent.childNodes[i];
    if (n.nodeType === 1) {
      const el = n as Element;
      if ((el.localName || el.nodeName.split(':').pop()) === localName) {
        out.push(el);
      }
    }
  }
  return out;
}

function hasAnyDrawing(el: Element): boolean {
  const tags = ['drawing', 'pict', 'imagedata', 'AlternateContent'];
  for (const tag of tags) {
    if (el.getElementsByTagName(`w:${tag}`).length > 0) return true;
    if (el.getElementsByTagName(tag).length > 0) return true;
    if (el.getElementsByTagName(`v:${tag}`).length > 0) return true;
    if (el.getElementsByTagName(`mc:${tag}`).length > 0) return true;
  }
  return false;
}

function extractElementText(el: Element): string {
  const ts = el.getElementsByTagNameNS(W_NS, 't');
  if (ts.length > 0) {
    const parts: string[] = [];
    for (let i = 0; i < ts.length; i++) {
      parts.push(ts[i].textContent || '');
    }
    return parts.join('');
  }
  return el.textContent || '';
}

function emptyDetectedFields(): DetectedHeaderFields {
  return {
    governingOrg: '',
    issuingOrg: '',
    affiliatedUnit: '',
    department: '',
    nationalHeader: '',
    motto: '',
    hasOrgLine: false,
    hasMottoLine: false,
    numberRaw: '',
    numberValue: '',
    symbolValue: '',
    location: '',
    dateRaw: '',
    day: '',
    month: '',
    year: '',
    docTypeLabel: '',
    subject: '',
    summaryLine: '',
    classificationMark: '',
  };
}

function parseNumberSymbol(raw: string): { num: string; sym: string } {
  const after = raw.replace(/^\s*S[ỐốOo]\s*:?\s*/i, '').trim();
  const slashIdx = after.indexOf('/');
  if (slashIdx >= 0) {
    return {
      num: after.slice(0, slashIdx).trim(),
      sym: after.slice(slashIdx + 1).trim(),
    };
  }
  return { num: after, sym: '' };
}

function parseLocationDate(raw: string): {
  loc: string;
  day: string;
  month: string;
  year: string;
} {
  const clean = collapseSpaces(raw);
  const idx = clean.toLowerCase().indexOf('ngày');
  const loc =
    idx > 0
      ? clean
          .slice(0, idx)
          .replace(/[,\s]+$/g, '')
          .trim()
      : '';
  const tail = idx >= 0 ? clean.slice(idx) : clean;
  const m = tail.match(
    /ngày\s+([0-9.\s…_-]*)\s*tháng\s+([0-9.\s…_-]*)\s*năm\s+([0-9.\s…_-]*)/i,
  );
  if (m) {
    const normPart = (v: string) =>
      v.includes('.') || v.includes('…') || v.includes('_') ? '' : v.trim();
    return {
      loc,
      day: normPart(m[1]),
      month: normPart(m[2]),
      year: normPart(m[3]),
    };
  }
  return { loc, day: '', month: '', year: '' };
}

function looksLikeDocTypeHeading(upper: string): boolean {
  const ascii = stripDiacriticsUpper(upper);
  const headings = [
    'QUYET DINH',
    'TO TRINH',
    'BAO CAO',
    'THONG BAO',
    'KE HOACH',
    'BIEN BAN',
    'CHI THI',
    'NGHI QUYET',
    'DE AN',
    'QUY CHE',
    'HOP DONG',
    'GIAY MOI',
    'CONG VAN',
    'CONG DIEN',
    'PHUONG AN',
    'CHUONG TRINH',
  ];
  return headings.includes(ascii);
}

function looksLikeNationalOrPartyHeader(upper: string): boolean {
  const ascii = stripDiacriticsUpper(upper);
  return (
    ascii.includes('CONG HOA XA HOI') ||
    ascii.includes('DANG CONG SAN VIET NAM')
  );
}

function looksLikeMotto(text: string): boolean {
  const ascii = stripDiacriticsUpper(text);
  return ascii.includes('DOC LAP') && ascii.includes('HANH PHUC');
}

function looksLikeNumberLine(text: string): boolean {
  return /^\s*S[ỐốOo]\s*[:\/]/i.test(text);
}

function looksLikeDateLine(text: string): boolean {
  const lower = text.toLowerCase();
  return lower.includes('ngày') && lower.includes('tháng') && lower.includes('năm');
}

function isBodyBoundary(upper: string): boolean {
  const ascii = stripDiacriticsUpper(upper);
  return (
    ascii.startsWith('KINH GUI') ||
    ascii.startsWith('CAN CU') ||
    ascii.startsWith('THUC HIEN') ||
    ascii.startsWith('DIEU 1') ||
    ascii.startsWith('I.') ||
    ascii.startsWith('1.')
  );
}

// ============================================================================
// CANDIDATE PARSER & INSPECTION
// ============================================================================

interface WebCandidateInternal {
  summary: HeaderCandidateSummary;
  preservedLogoNodes: Element[];
  hasHeaderTableIssues: string[];
}

function parseTableCandidate(
  tbl: Element,
  children: Element[],
  tblIdx: number,
  candidateIndex: number,
): WebCandidateInternal | null {
  const rows = getDirectChildrenByLocalName(tbl, 'tr');
  if (rows.length === 0) return null;
  const firstCells = getDirectChildrenByLocalName(rows[0], 'tc');
  if (firstCells.length < 2) return null;

  const fullAscii = stripDiacriticsUpper(extractElementText(tbl));
  const looksAdmin =
    fullAscii.includes('CONG HOA XA HOI') ||
    fullAscii.includes('DOC LAP') ||
    fullAscii.includes('DANG CONG SAN') ||
    looksLikeNumberLine(extractElementText(tbl)) ||
    looksLikeDateLine(extractElementText(tbl));

  if (!looksAdmin) return null;

  const fields = emptyDetectedFields();
  const hasLogo = hasAnyDrawing(tbl);
  const logoNodes: Element[] = [];
  const issues: string[] = [];
  const leftOrgLines: string[] = [];

  for (const row of rows) {
    const cells = getDirectChildrenByLocalName(row, 'tc');
    if (cells.length === 0) continue;
    const leftCell = cells[0];
    const rightCell = cells.length > 1 ? cells[cells.length - 1] : null;

    for (const p of getDirectChildrenByLocalName(leftCell, 'p')) {
      if (hasAnyDrawing(p)) {
        logoNodes.push(p.cloneNode(true) as Element);
      }
      const txt = collapseSpaces(extractElementText(p));
      if (!txt) {
        if (p.getElementsByTagNameNS(W_NS, 'pBdr').length > 0) {
          fields.hasOrgLine = true;
        }
        continue;
      }
      if (isDividerText(txt)) {
        fields.hasOrgLine = true;
        issues.push(`Đang dùng ký tự "${txt}" làm đường kẻ cơ quan.`);
        continue;
      }
      if (looksLikeNumberLine(txt)) {
        fields.numberRaw = txt;
        const { num, sym } = parseNumberSymbol(txt);
        fields.numberValue = num;
        fields.symbolValue = sym;
        continue;
      }
      if (/^v\/v/i.test(txt)) {
        fields.subject = txt;
        if (!fields.docTypeLabel) fields.docTypeLabel = 'CÔNG VĂN';
        continue;
      }
      if (!looksLikeNationalOrPartyHeader(txt) && !looksLikeMotto(txt)) {
        leftOrgLines.push(txt);
      }
    }

    if (rightCell) {
      for (const p of getDirectChildrenByLocalName(rightCell, 'p')) {
        if (hasAnyDrawing(p)) {
          logoNodes.push(p.cloneNode(true) as Element);
        }
        const txt = collapseSpaces(extractElementText(p));
        if (!txt) {
          if (p.getElementsByTagNameNS(W_NS, 'pBdr').length > 0) {
            fields.hasMottoLine = true;
          }
          continue;
        }
        if (isDividerText(txt)) {
          fields.hasMottoLine = true;
          issues.push(`Đang dùng ký tự "${txt}" làm đường kẻ tiêu ngữ.`);
          continue;
        }
        if (looksLikeNationalOrPartyHeader(txt)) {
          fields.nationalHeader = txt;
          continue;
        }
        if (looksLikeMotto(txt)) {
          fields.motto = txt;
          continue;
        }
        if (looksLikeDateLine(txt)) {
          fields.dateRaw = txt;
          const d = parseLocationDate(txt);
          fields.location = d.loc;
          fields.day = d.day;
          fields.month = d.month;
          fields.year = d.year;
          continue;
        }
      }
    }
  }

  if (leftOrgLines.length === 1) {
    fields.issuingOrg = leftOrgLines[0];
  } else if (leftOrgLines.length === 2) {
    fields.governingOrg = leftOrgLines[0];
    fields.issuingOrg = leftOrgLines[1];
  } else if (leftOrgLines.length >= 3) {
    fields.governingOrg = leftOrgLines[0];
    fields.issuingOrg = leftOrgLines[1];
    fields.affiliatedUnit = leftOrgLines[2];
    if (leftOrgLines.length >= 4) fields.department = leftOrgLines[3];
  }

  // Scan trailing paragraphs immediately below header table for doc type + subject
  let bodyEnd = tblIdx;
  const maxLook = Math.min(children.length, tblIdx + 8);
  for (let j = tblIdx + 1; j < maxLook; j++) {
    const el = children[j];
    if ((el.localName || el.nodeName.split(':').pop()) !== 'p') break;
    const txt = collapseSpaces(extractElementText(el));
    const upper = normalizeUpper(txt);
    if (txt && isBodyBoundary(upper)) break;
    if (!txt) {
      if (el.getElementsByTagNameNS(W_NS, 'pBdr').length > 0) {
        bodyEnd = j;
        break;
      }
      continue;
    }
    if (isDividerText(txt)) {
      bodyEnd = j;
      break;
    }
    if (!fields.docTypeLabel && looksLikeDocTypeHeading(upper)) {
      fields.docTypeLabel = upper;
      bodyEnd = j;
      continue;
    }
    if (/^v\/v/i.test(txt) && !fields.subject) {
      fields.subject = txt;
      bodyEnd = j;
      continue;
    }
    if (fields.docTypeLabel && !fields.subject && txt.length <= 260) {
      fields.subject = txt;
      bodyEnd = j;
      continue;
    }
    break;
  }

  const layoutKind: HeaderLayoutKind =
    hasLogo && !fields.nationalHeader
      ? 'customTemplateWithLogo'
      : bodyEnd > tblIdx && fields.docTypeLabel
        ? 'mixedTableAndParagraphs'
        : 'tableLayout';

  const snippet = [
    fields.governingOrg,
    fields.issuingOrg,
    fields.nationalHeader,
    fields.numberRaw,
    fields.docTypeLabel,
  ]
    .filter(Boolean)
    .join(' | ');

  return {
    summary: {
      candidateIndex,
      layoutKind,
      bodyStartIndex: tblIdx,
      bodyEndIndex: bodyEnd,
      hasLogoOrImage: hasLogo,
      previewSnippet: snippet || '(Bảng phần đầu văn bản)',
      fields,
    },
    preservedLogoNodes: logoNodes,
    hasHeaderTableIssues: issues,
  };
}

function parseParagraphCandidate(
  children: Element[],
  startIdx: number,
  candidateIndex: number,
): WebCandidateInternal | null {
  const fields = emptyDetectedFields();
  const logoNodes: Element[] = [];
  let hasLogo = false;
  let endIdx = startIdx;
  const maxEnd = Math.min(children.length, startIdx + 10);
  const unassigned: string[] = [];

  for (let i = startIdx; i < maxEnd; i++) {
    const el = children[i];
    const lname = el.localName || el.nodeName.split(':').pop();
    if (lname !== 'p') break;
    if (hasAnyDrawing(el)) {
      hasLogo = true;
      logoNodes.push(el.cloneNode(true) as Element);
      endIdx = i;
      continue;
    }
    const txt = collapseSpaces(extractElementText(el));
    const upper = normalizeUpper(txt);
    if (txt && isBodyBoundary(upper)) break;
    if (!txt) {
      if (i > startIdx) endIdx = i;
      continue;
    }
    if (looksLikeNationalOrPartyHeader(txt)) {
      fields.nationalHeader = txt;
      endIdx = i;
      continue;
    }
    if (looksLikeMotto(txt)) {
      fields.motto = txt;
      endIdx = i;
      continue;
    }
    if (looksLikeNumberLine(txt)) {
      fields.numberRaw = txt;
      const ns = parseNumberSymbol(txt);
      fields.numberValue = ns.num;
      fields.symbolValue = ns.sym;
      endIdx = i;
      continue;
    }
    if (looksLikeDateLine(txt)) {
      fields.dateRaw = txt;
      const d = parseLocationDate(txt);
      fields.location = d.loc;
      fields.day = d.day;
      fields.month = d.month;
      fields.year = d.year;
      endIdx = i;
      continue;
    }
    if (looksLikeDocTypeHeading(upper) && !fields.docTypeLabel) {
      fields.docTypeLabel = upper;
      endIdx = i;
      continue;
    }
    if ((fields.nationalHeader || fields.motto || fields.dateRaw) && !fields.docTypeLabel) {
      break;
    }
    if (unassigned.length < 3 && txt.length <= 120 && !txt.endsWith('.')) {
      unassigned.push(txt);
      endIdx = i;
      continue;
    }
    break;
  }

  const recognized =
    Boolean(fields.nationalHeader) ||
    Boolean(fields.motto) ||
    Boolean(fields.numberRaw) ||
    Boolean(fields.dateRaw) ||
    Boolean(fields.docTypeLabel);
  if (!recognized) return null;

  if (unassigned.length === 1) {
    fields.issuingOrg = unassigned[0];
  } else if (unassigned.length >= 2) {
    fields.governingOrg = unassigned[0];
    fields.issuingOrg = unassigned[1];
  }

  const snippet = [
    fields.issuingOrg,
    fields.nationalHeader,
    fields.numberRaw,
    fields.docTypeLabel,
  ]
    .filter(Boolean)
    .join(' | ');

  return {
    summary: {
      candidateIndex,
      layoutKind: hasLogo ? 'customTemplateWithLogo' : 'paragraphLayout',
      bodyStartIndex: startIdx,
      bodyEndIndex: endIdx,
      hasLogoOrImage: hasLogo,
      previewSnippet: snippet || '(Đoạn đầu văn bản)',
      fields,
    },
    preservedLogoNodes: logoNodes,
    hasHeaderTableIssues: [],
  };
}

function detectWebCandidates(body: Element): WebCandidateInternal[] {
  const children = Array.from(body.childNodes).filter(
    (n): n is Element => n.nodeType === 1,
  );
  const candidates: WebCandidateInternal[] = [];
  let i = 0;
  const limit = Math.min(children.length, 30);

  while (i < limit) {
    const el = children[i];
    const lname = el.localName || el.nodeName.split(':').pop();
    if (lname === 'tbl') {
      const c = parseTableCandidate(el, children, i, candidates.length);
      if (c) {
        i = c.summary.bodyEndIndex + 1;
        candidates.push(c);
        continue;
      }
    } else if (lname === 'p') {
      const txt = collapseSpaces(extractElementText(el));
      if (
        txt &&
        (looksLikeNationalOrPartyHeader(txt) ||
          looksLikeMotto(txt) ||
          looksLikeNumberLine(txt) ||
          looksLikeDocTypeHeading(normalizeUpper(txt)))
      ) {
        const c = parseParagraphCandidate(children, i, candidates.length);
        if (c) {
          i = c.summary.bodyEndIndex + 1;
          candidates.push(c);
          continue;
        }
      }
    }
    i++;
  }
  return candidates;
}

function inspectParsedDocument(
  doc: Document,
  entries: Map<string, Uint8Array>,
  filePath: string,
  fileName: string,
  cfg: HeaderBuilderConfig,
  org: OrganizationProfile | null,
): { result: HeaderInspectionResult; candidates: WebCandidateInternal[] } {
  const bodyList = doc.getElementsByTagNameNS(W_NS, 'body');
  const body = bodyList.length > 0 ? bodyList[0] : doc.documentElement;
  const candidates = detectWebCandidates(body);

  let hasHeaderFooter = false;
  let hasMacros = false;
  for (const name of entries.keys()) {
    if (/^word\/(header|footer)\d+\.xml$/i.test(name)) hasHeaderFooter = true;
    if (/^word\/vbaProject\.bin$/i.test(name)) hasMacros = true;
  }

  const sectCount = Math.max(1, doc.getElementsByTagNameNS(W_NS, 'sectPr').length);
  const rules = applicableRulesFor(org, cfg.docType);
  const findings: HeaderFinding[] = [];

  let classification: HeaderClassification = 'valid';
  let requiresUserChoice = false;
  let activeCandidateIndex: number | null = null;

  if (candidates.length > 1 && cfg.selectedCandidateIndex === null) {
    classification = 'uncertain';
    requiresUserChoice = true;
    findings.push({
      ruleId: 'CANDIDATE-MULTI',
      severity: 'warning',
      obligation: 'caseSpecific',
      message: `Phát hiện ${candidates.length} cụm có khả năng là phần đầu văn bản. Hãy chọn đúng cụm trước khi chuẩn hóa.`,
      legalBasis: 'Nguyên tắc an toàn bảo toàn cấu trúc tài liệu',
    });
  } else if (candidates.length === 0) {
    classification = 'missingComponents';
    findings.push({
      ruleId: 'ND30-PLI-01',
      severity: 'warning',
      obligation: 'mandatory',
      message: 'Tài liệu chưa có khối phần đầu văn bản hành chính.',
      legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I',
    });
  } else {
    const idx = Math.min(
      Math.max(cfg.selectedCandidateIndex ?? 0, 0),
      candidates.length - 1,
    );
    activeCandidateIndex = idx;
    const cand = candidates[idx];
    const f = cand.summary.fields;
    let missingCount = 0;
    let formatCount = 0;
    let conflictCount = 0;

    for (const issue of cand.hasHeaderTableIssues) {
      formatCount++;
      findings.push({
        ruleId: 'ND30-PLI-02',
        severity: 'warning',
        obligation: 'mandatory',
        message: issue,
        legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I',
      });
    }

    if (cand.summary.layoutKind === 'paragraphLayout') {
      formatCount++;
      findings.push({
        ruleId: 'ND30-PLI-07',
        severity: 'warning',
        obligation: 'recommended',
        message: 'Phần đầu đang xếp dạng đoạn văn thay vì bảng 2 cột không viền NĐ 30.',
        legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Bảng bố cục mẫu',
      });
    }

    const isEntNoNd30 =
      (org?.sector === 'enterprise' || org?.sector === 'other') &&
      !(org?.useNd30LayoutForEnterprise ?? false);

    if (org?.sector !== 'partyOrg' && !isEntNoNd30) {
      if (!f.nationalHeader) {
        missingCount++;
        findings.push({
          ruleId: 'ND30-PLI-01',
          severity: 'error',
          obligation: 'mandatory',
          message: 'Thiếu dòng Quốc hiệu "CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM".',
          legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.1',
        });
      } else if (
        normalizeUpper(f.nationalHeader) !== 'CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM'
      ) {
        formatCount++;
        findings.push({
          ruleId: 'ND30-PLI-01',
          severity: 'warning',
          obligation: 'mandatory',
          message: `Quốc hiệu "${f.nationalHeader}" chưa khớp chính tả/chữ hoa chuẩn.`,
          legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.1',
        });
      }
      if (!f.motto) {
        missingCount++;
        findings.push({
          ruleId: 'ND30-PLI-01',
          severity: 'error',
          obligation: 'mandatory',
          message: 'Thiếu Tiêu ngữ "Độc lập - Tự do - Hạnh phúc".',
          legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.1',
        });
      }
    }

    if (!f.issuingOrg && org?.issuingOrg) {
      missingCount++;
      findings.push({
        ruleId: 'ND30-PLI-02',
        severity: 'warning',
        obligation: 'mandatory',
        message: 'Thiếu tên cơ quan ban hành trong tài liệu.',
        legalBasis: 'Nghị định 30/2020/NĐ-CP, Phụ lục I, Mục II.2',
      });
    } else if (
      f.issuingOrg &&
      org?.issuingOrg &&
      stripDiacriticsUpper(f.issuingOrg) !== stripDiacriticsUpper(org.issuingOrg) &&
      !cfg.allowOverrideExistingOrg
    ) {
      conflictCount++;
      findings.push({
        ruleId: 'CONFLICT-ORG',
        severity: 'warning',
        obligation: 'caseSpecific',
        message: `Cơ quan ban hành trên tài liệu ("${f.issuingOrg}") khác Hồ sơ đang chọn ("${org.issuingOrg}").`,
        legalBasis: 'Xác nhận trước khi ghi đè thông tin cơ quan',
      });
    }

    if (
      cfg.numberMode === 'manualInput' &&
      (cfg.docNumber.trim() || cfg.docSymbol.trim()) &&
      (f.numberValue || f.symbolValue) &&
      !cfg.allowOverrideExistingNumber
    ) {
      conflictCount++;
      findings.push({
        ruleId: 'CONFLICT-NUMBER',
        severity: 'warning',
        obligation: 'caseSpecific',
        message: `Văn bản đã có "${f.numberRaw}"; giữ nguyên trừ khi bạn bật cho phép ghi đè.`,
        legalBasis: 'Nghị định 30/2020/NĐ-CP & nguyên tắc giữ số gốc',
      });
    }

    if (
      (cfg.dateMode === 'useToday' || cfg.dateMode === 'manualDate') &&
      f.dateRaw &&
      !cfg.allowOverrideExistingDate
    ) {
      conflictCount++;
      findings.push({
        ruleId: 'CONFLICT-DATE',
        severity: 'warning',
        obligation: 'caseSpecific',
        message: `Văn bản đã có ngày "${f.dateRaw}"; không tự thay nếu chưa cho phép ghi đè.`,
        legalBasis: 'Nguyên tắc giữ nguyên ngày ban hành gốc',
      });
    }

    if (conflictCount > 0) {
      classification = 'dataConflict';
    } else if (missingCount > 0) {
      classification = 'missingComponents';
    } else if (formatCount > 0) {
      classification = 'needsStandardization';
    } else {
      classification = 'valid';
    }
  }

  const labelMap: Record<HeaderClassification, string> = {
    valid: 'Đã hợp lệ thể thức',
    needsStandardization: 'Cần chuẩn hóa bố cục / chính tả',
    missingComponents: 'Thiếu thành phần phần đầu',
    dataConflict: 'Xung đột dữ liệu với văn bản gốc',
    uncertain: 'Không chắc chắn — cần chọn vùng phần đầu',
  };

  const result: HeaderInspectionResult = {
    filePath,
    fileName,
    classification,
    classificationLabel: labelMap[classification],
    sectionCount: sectCount,
    hasHeaderFooter,
    hasMacros,
    candidates: candidates.map((c) => c.summary),
    activeCandidateIndex,
    findings,
    applicableRules: rules,
    canCreateNew: candidates.length === 0,
    canStandardize:
      candidates.length === 1 ||
      (candidates.length > 1 && cfg.selectedCandidateIndex !== null),
    requiresUserChoice,
  };

  return { result, candidates };
}

export async function inspectWebDocxHeader(
  item: FileQueueItem,
  cfg: HeaderBuilderConfig,
  org: OrganizationProfile | null,
): Promise<HeaderInspectionResult> {
  if (item.ext.toLowerCase() === 'doc') {
    return {
      filePath: item.path,
      fileName: item.name,
      classification: 'uncertain',
      classificationLabel: 'Cần bản Windows + Word COM để đọc .doc nhị phân',
      sectionCount: 1,
      hasHeaderFooter: false,
      hasMacros: false,
      candidates: [],
      activeCandidateIndex: null,
      findings: [
        {
          ruleId: 'WEB-DOC-BINARY',
          severity: 'warning',
          obligation: 'caseSpecific',
          message:
            'Trình duyệt chỉ đọc trực tiếp .docx / .docm. Vui lòng dùng MonWord Windows để xử lý tệp .doc nhị phân.',
          legalBasis: 'Giới hạn định dạng nhị phân OLE2 trên trình duyệt',
        },
      ],
      applicableRules: applicableRulesFor(org, cfg.docType),
      canCreateNew: false,
      canStandardize: false,
      requiresUserChoice: false,
    };
  }
  if (!item.webFile) {
    throw new Error('Không tìm thấy dữ liệu tệp trong trình duyệt.');
  }
  registerWebInitialSnapshot(item);
  const buffer = await item.webFile.arrayBuffer();
  const entries = await readZipEntries(buffer);
  const docBytes = entries.get('word/document.xml');
  if (!docBytes) {
    throw new Error('Không tìm thấy word/document.xml trong tệp OpenXML.');
  }
  const xmlStr = new TextDecoder('utf-8').decode(docBytes);
  const doc = new DOMParser().parseFromString(xmlStr, 'application/xml');
  return inspectParsedDocument(doc, entries, item.path, item.name, cfg, org).result;
}

// ============================================================================
// OPENXML HEADER NODE BUILDER (CLIENT-SIDE PARITY)
// ============================================================================

function wEl(doc: Document, local: string): Element {
  return doc.createElementNS(W_NS, `w:${local}`);
}

function wAttr(el: Element, local: string, val: string): void {
  el.setAttributeNS(W_NS, `w:${local}`, val);
}

function makeParagraph(
  doc: Document,
  text: string,
  opts: {
    align?: 'left' | 'center' | 'right';
    bold?: boolean;
    italic?: boolean;
    sizeHalfPt: number;
    spaceBefore?: number;
    spaceAfter?: number;
  },
): Element {
  const p = wEl(doc, 'p');
  const pPr = wEl(doc, 'pPr');
  if (opts.align) {
    const jc = wEl(doc, 'jc');
    wAttr(jc, 'val', opts.align);
    pPr.appendChild(jc);
  }
  const sp = wEl(doc, 'spacing');
  wAttr(sp, 'before', String(opts.spaceBefore ?? 0));
  wAttr(sp, 'after', String(opts.spaceAfter ?? 20));
  wAttr(sp, 'line', '240');
  wAttr(sp, 'lineRule', 'auto');
  pPr.appendChild(sp);
  p.appendChild(pPr);

  const r = wEl(doc, 'r');
  const rPr = wEl(doc, 'rPr');
  const rf = wEl(doc, 'rFonts');
  wAttr(rf, 'ascii', 'Times New Roman');
  wAttr(rf, 'hAnsi', 'Times New Roman');
  wAttr(rf, 'cs', 'Times New Roman');
  rPr.appendChild(rf);
  if (opts.bold) rPr.appendChild(wEl(doc, 'b'));
  if (opts.italic) rPr.appendChild(wEl(doc, 'i'));
  const col = wEl(doc, 'color');
  wAttr(col, 'val', '000000');
  rPr.appendChild(col);
  const sz = wEl(doc, 'sz');
  wAttr(sz, 'val', String(opts.sizeHalfPt));
  const szCs = wEl(doc, 'szCs');
  wAttr(szCs, 'val', String(opts.sizeHalfPt));
  rPr.appendChild(sz);
  rPr.appendChild(szCs);
  r.appendChild(rPr);

  const t = wEl(doc, 't');
  t.setAttribute('xml:space', 'preserve');
  t.textContent = text;
  r.appendChild(t);
  p.appendChild(r);
  return p;
}

function makeSolidUnderlineParagraph(
  doc: Document,
  indentLeftTwips: number,
  indentRightTwips: number,
  spaceAfter = 60,
): Element {
  const p = wEl(doc, 'p');
  const pPr = wEl(doc, 'pPr');
  const pBdr = wEl(doc, 'pBdr');
  const bottom = wEl(doc, 'bottom');
  wAttr(bottom, 'val', 'single');
  wAttr(bottom, 'sz', '6');
  wAttr(bottom, 'space', '1');
  wAttr(bottom, 'color', '000000');
  pBdr.appendChild(bottom);
  pPr.appendChild(pBdr);

  const ind = wEl(doc, 'ind');
  wAttr(ind, 'left', String(Math.max(0, Math.round(indentLeftTwips))));
  wAttr(ind, 'right', String(Math.max(0, Math.round(indentRightTwips))));
  pPr.appendChild(ind);

  const sp = wEl(doc, 'spacing');
  wAttr(sp, 'before', '0');
  wAttr(sp, 'after', String(spaceAfter));
  wAttr(sp, 'line', '240');
  wAttr(sp, 'lineRule', 'auto');
  pPr.appendChild(sp);
  p.appendChild(pPr);
  return p;
}

function resolveEffectiveHeaderValues(
  cfg: HeaderBuilderConfig,
  org: OrganizationProfile | null,
  existing: DetectedHeaderFields | null,
) {
  const useExistingOrg = Boolean(existing && !cfg.allowOverrideExistingOrg);

  const governingOrg = cfg.showGoverningOrg
    ? useExistingOrg && existing?.governingOrg
      ? existing.governingOrg
      : (org?.governingOrg || '')
    : '';
  const issuingOrg = cfg.showIssuingOrg
    ? useExistingOrg && existing?.issuingOrg
      ? existing.issuingOrg
      : (org?.issuingOrg || '')
    : '';
  const affiliatedUnit = cfg.showAffiliatedUnit
    ? useExistingOrg && existing?.affiliatedUnit
      ? existing.affiliatedUnit
      : (org?.affiliatedUnit || '')
    : '';
  const department = cfg.showDepartment
    ? useExistingOrg && existing?.department
      ? existing.department
      : (org?.departmentOrBranch || '')
    : '';

  const isParty = org?.sector === 'partyOrg';
  const nationalHeader = cfg.showNationalHeader
    ? cfg.customNationalHeader.trim() ||
      (isParty ? 'ĐẢNG CỘNG SẢN VIỆT NAM' : 'CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM')
    : '';
  const motto = cfg.showNationalHeader && !isParty
    ? cfg.customMotto.trim() || 'Độc lập - Tự do - Hạnh phúc'
    : '';

  let numberLine = '';
  if (cfg.showNumberSymbol) {
    if (
      (cfg.numberMode === 'keepExisting' ||
        (existing && (existing.numberValue || existing.symbolValue) && !cfg.allowOverrideExistingNumber)) &&
      existing?.numberRaw
    ) {
      numberLine = existing.numberRaw;
    } else {
      const n = cfg.docNumber.trim();
      const s = cfg.docSymbol.trim();
      if (!n && !s) {
        numberLine = 'Số:        /';
      } else if (!n && s) {
        numberLine = `Số:      /${s}`;
      } else if (n && !s) {
        numberLine = `Số: ${n}`;
      } else {
        numberLine = `Số: ${n}/${s}`;
      }
    }
  }

  let dateLine = '';
  if (cfg.showLocationDate) {
    if (
      (cfg.dateMode === 'keepExisting' || (existing?.dateRaw && !cfg.allowOverrideExistingDate)) &&
      existing?.dateRaw
    ) {
      dateLine = existing.dateRaw;
    } else {
      const loc = cfg.locationOverride.trim() || org?.defaultLocation.trim() || '';
      const prefix = loc ? `${loc}, ` : '';
      if (cfg.dateMode === 'useToday') {
        const now = new Date();
        const dd = String(now.getDate()).padStart(2, '0');
        const mm = String(now.getMonth() + 1).padStart(2, '0');
        const yyyy = String(now.getFullYear());
        dateLine = `${prefix}ngày ${dd} tháng ${mm} năm ${yyyy}`;
      } else if (cfg.dateMode === 'manualDate') {
        const dd = cfg.manualDay.trim() || '...';
        const mm = cfg.manualMonth.trim() || '...';
        const yyyy = cfg.manualYear.trim() || '20...';
        dateLine = `${prefix}ngày ${dd} tháng ${mm} năm ${yyyy}`;
      } else {
        dateLine = `${prefix}ngày ... tháng ... năm ...`;
      }
    }
  }

  const docMeta = ADMIN_DOC_TYPE_META.find((m) => m.id === cfg.docType);
  const docTypeLabel = cfg.showDocTypeAndSubject
    ? cfg.customDocTypeLabel.trim() ||
      existing?.docTypeLabel ||
      docMeta?.docTypeHeading ||
      ''
    : '';
  const subjectText = cfg.showDocTypeAndSubject
    ? cfg.subjectText.trim() || existing?.subject || ''
    : '';

  return {
    governingOrg,
    issuingOrg,
    affiliatedUnit,
    department,
    nationalHeader,
    motto,
    numberLine,
    dateLine,
    docTypeLabel,
    subjectText,
    summaryLine: cfg.summaryLine.trim() || existing?.summaryLine || '',
    classificationMark: cfg.classificationMark.trim() || existing?.classificationMark || '',
    subjectInLeftCell: cfg.docType === 'congVan',
  };
}

function buildHeaderNodes(
  doc: Document,
  cfg: HeaderBuilderConfig,
  org: OrganizationProfile | null,
  existing: DetectedHeaderFields | null,
  preservedLogos: Element[],
): Element[] {
  const vals = resolveEffectiveHeaderValues(cfg, org, existing);
  const out: Element[] = [];

  if (vals.classificationMark) {
    out.push(
      makeParagraph(doc, vals.classificationMark.toLocaleUpperCase('vi-VN'), {
        align: 'right',
        bold: true,
        sizeHalfPt: 24,
        spaceBefore: 0,
        spaceAfter: 40,
      }),
    );
  }

  // Borderless 2-col table (9355 twips = 11906 - 1701 - 850)
  const LEFT_W = 4150;
  const RIGHT_W = 5205;
  const tbl = wEl(doc, 'tbl');
  const tblPr = wEl(doc, 'tblPr');
  const tblW = wEl(doc, 'tblW');
  wAttr(tblW, 'w', String(LEFT_W + RIGHT_W));
  wAttr(tblW, 'type', 'dxa');
  tblPr.appendChild(tblW);

  const jc = wEl(doc, 'jc');
  wAttr(jc, 'val', 'center');
  tblPr.appendChild(jc);

  const tblBorders = wEl(doc, 'tblBorders');
  for (const side of ['top', 'left', 'bottom', 'right', 'insideH', 'insideV']) {
    const b = wEl(doc, side);
    wAttr(b, 'val', 'none');
    wAttr(b, 'sz', '0');
    wAttr(b, 'space', '0');
    wAttr(b, 'color', 'auto');
    tblBorders.appendChild(b);
  }
  tblPr.appendChild(tblBorders);

  const tblLayout = wEl(doc, 'tblLayout');
  wAttr(tblLayout, 'type', 'fixed');
  tblPr.appendChild(tblLayout);
  tbl.appendChild(tblPr);

  const tblGrid = wEl(doc, 'tblGrid');
  for (const w of [LEFT_W, RIGHT_W]) {
    const gc = wEl(doc, 'gridCol');
    wAttr(gc, 'w', String(w));
    tblGrid.appendChild(gc);
  }
  tbl.appendChild(tblGrid);

  // Row 1: Org block (left) & National/Motto (right)
  const tr1 = wEl(doc, 'tr');
  const tcLeft = wEl(doc, 'tc');
  const tcPrL = wEl(doc, 'tcPr');
  const tcWL = wEl(doc, 'tcW');
  wAttr(tcWL, 'w', String(LEFT_W));
  wAttr(tcWL, 'type', 'dxa');
  tcPrL.appendChild(tcWL);
  tcLeft.appendChild(tcPrL);

  for (const logoP of preservedLogos) {
    tcLeft.appendChild(doc.importNode(logoP, true));
  }
  if (vals.governingOrg) {
    tcLeft.appendChild(
      makeParagraph(doc, vals.governingOrg.toLocaleUpperCase('vi-VN'), {
        align: 'center',
        bold: false,
        sizeHalfPt: 24,
        spaceAfter: 20,
      }),
    );
  }
  if (vals.issuingOrg) {
    tcLeft.appendChild(
      makeParagraph(doc, vals.issuingOrg.toLocaleUpperCase('vi-VN'), {
        align: 'center',
        bold: true,
        sizeHalfPt: 26,
        spaceAfter: 20,
      }),
    );
  }
  if (vals.affiliatedUnit) {
    tcLeft.appendChild(
      makeParagraph(doc, vals.affiliatedUnit, {
        align: 'center',
        bold: false,
        sizeHalfPt: 24,
        spaceAfter: 20,
      }),
    );
  }
  if (vals.department) {
    tcLeft.appendChild(
      makeParagraph(doc, vals.department, {
        align: 'center',
        bold: false,
        sizeHalfPt: 24,
        spaceAfter: 20,
      }),
    );
  }
  if (cfg.showOrgUnderline && (vals.issuingOrg || vals.department || vals.affiliatedUnit)) {
    const pad = Math.round(LEFT_W * 0.25);
    tcLeft.appendChild(makeSolidUnderlineParagraph(doc, pad, pad, 60));
  }
  if (getDirectChildrenByLocalName(tcLeft, 'p').length === 0) {
    tcLeft.appendChild(makeParagraph(doc, '', { align: 'center', sizeHalfPt: 24 }));
  }

  const tcRight = wEl(doc, 'tc');
  const tcPrR = wEl(doc, 'tcPr');
  const tcWR = wEl(doc, 'tcW');
  wAttr(tcWR, 'w', String(RIGHT_W));
  wAttr(tcWR, 'type', 'dxa');
  tcPrR.appendChild(tcWR);
  tcRight.appendChild(tcPrR);

  if (vals.nationalHeader) {
    tcRight.appendChild(
      makeParagraph(doc, vals.nationalHeader.toLocaleUpperCase('vi-VN'), {
        align: 'center',
        bold: true,
        sizeHalfPt: 24,
        spaceAfter: 20,
      }),
    );
  }
  if (vals.motto) {
    tcRight.appendChild(
      makeParagraph(doc, vals.motto, {
        align: 'center',
        bold: true,
        sizeHalfPt: 26,
        spaceAfter: 20,
      }),
    );
  }
  if (cfg.showMottoUnderline && (vals.motto || vals.nationalHeader)) {
    const pad = Math.round(RIGHT_W * 0.2);
    tcRight.appendChild(makeSolidUnderlineParagraph(doc, pad, pad, 60));
  }
  if (getDirectChildrenByLocalName(tcRight, 'p').length === 0) {
    tcRight.appendChild(makeParagraph(doc, '', { align: 'center', sizeHalfPt: 24 }));
  }

  tr1.appendChild(tcLeft);
  tr1.appendChild(tcRight);
  tbl.appendChild(tr1);

  // Row 2: Number/Symbol (left) & Location/Date (right)
  if (vals.numberLine || vals.dateLine || (vals.subjectInLeftCell && vals.subjectText)) {
    const tr2 = wEl(doc, 'tr');
    const tc2L = wEl(doc, 'tc');
    const tcPr2L = wEl(doc, 'tcPr');
    const tcW2L = wEl(doc, 'tcW');
    wAttr(tcW2L, 'w', String(LEFT_W));
    wAttr(tcW2L, 'type', 'dxa');
    tcPr2L.appendChild(tcW2L);
    tc2L.appendChild(tcPr2L);

    if (vals.numberLine) {
      tc2L.appendChild(
        makeParagraph(doc, vals.numberLine, {
          align: 'center',
          bold: false,
          sizeHalfPt: 26,
          spaceBefore: 40,
          spaceAfter: 40,
        }),
      );
    }
    if (vals.subjectInLeftCell && vals.subjectText) {
      tc2L.appendChild(
        makeParagraph(doc, vals.subjectText, {
          align: 'left',
          bold: false,
          sizeHalfPt: 24,
          spaceBefore: 40,
          spaceAfter: 40,
        }),
      );
    }
    if (getDirectChildrenByLocalName(tc2L, 'p').length === 0) {
      tc2L.appendChild(makeParagraph(doc, '', { align: 'left', sizeHalfPt: 24 }));
    }

    const tc2R = wEl(doc, 'tc');
    const tcPr2R = wEl(doc, 'tcPr');
    const tcW2R = wEl(doc, 'tcW');
    wAttr(tcW2R, 'w', String(RIGHT_W));
    wAttr(tcW2R, 'type', 'dxa');
    tcPr2R.appendChild(tcW2R);
    tc2R.appendChild(tcPr2R);

    if (vals.dateLine) {
      tc2R.appendChild(
        makeParagraph(doc, vals.dateLine, {
          align: 'center',
          italic: true,
          sizeHalfPt: 26,
          spaceBefore: 40,
          spaceAfter: 40,
        }),
      );
    } else {
      tc2R.appendChild(makeParagraph(doc, '', { align: 'center', sizeHalfPt: 24 }));
    }

    tr2.appendChild(tc2L);
    tr2.appendChild(tc2R);
    tbl.appendChild(tr2);
  }

  out.push(tbl);

  // Post-table docType + subject + summary for non-CongVan
  if (cfg.showDocTypeAndSubject && !vals.subjectInLeftCell) {
    if (vals.docTypeLabel) {
      out.push(
        makeParagraph(doc, vals.docTypeLabel.toLocaleUpperCase('vi-VN'), {
          align: 'center',
          bold: true,
          sizeHalfPt: 28,
          spaceBefore: 160,
          spaceAfter: 40,
        }),
      );
    }
    if (vals.subjectText) {
      out.push(
        makeParagraph(doc, vals.subjectText, {
          align: 'center',
          bold: true,
          sizeHalfPt: 28,
          spaceBefore: 0,
          spaceAfter: 40,
        }),
      );
    }
    if (vals.summaryLine) {
      out.push(
        makeParagraph(doc, vals.summaryLine, {
          align: 'center',
          bold: false,
          italic: true,
          sizeHalfPt: 26,
          spaceBefore: 0,
          spaceAfter: 40,
        }),
      );
    }
    if (vals.docTypeLabel || vals.subjectText) {
      out.push(makeSolidUnderlineParagraph(doc, 3200, 3200, 120));
    }
  } else if (vals.summaryLine) {
    out.push(
      makeParagraph(doc, vals.summaryLine, {
        align: 'left',
        sizeHalfPt: 26,
        spaceBefore: 60,
        spaceAfter: 60,
      }),
    );
  }

  return out;
}

export async function applyWebDocxHeader(
  item: FileQueueItem,
  cfg: HeaderBuilderConfig,
  mode: HeaderActionMode,
  org: OrganizationProfile | null,
): Promise<HeaderApplyResult> {
  if (item.ext.toLowerCase() === 'doc') {
    return {
      inputPath: item.path,
      outputPath: '',
      initialBackupPath: null,
      success: false,
      classificationBefore: 'uncertain',
      classificationAfter: 'uncertain',
      logs: [],
      warnings: [],
      error: 'Trình duyệt không hỗ trợ ghi .doc nhị phân. Hãy dùng MonWord Windows.',
      inspectionAfter: null,
    };
  }
  if (!item.webFile) {
    throw new Error('Không có tệp nguồn để áp dụng.');
  }
  registerWebInitialSnapshot(item);

  const buffer = await item.webFile.arrayBuffer();
  const entries = await readZipEntries(buffer);
  const docBytes = entries.get('word/document.xml');
  if (!docBytes) {
    throw new Error('Không tìm thấy word/document.xml.');
  }

  const xmlStr = new TextDecoder('utf-8').decode(docBytes);
  const doc = new DOMParser().parseFromString(xmlStr, 'application/xml');
  const { result: beforeInsp, candidates } = inspectParsedDocument(
    doc,
    entries,
    item.path,
    item.name,
    cfg,
    org,
  );

  if (mode === 'createNew' && candidates.length > 0) {
    return {
      inputPath: item.path,
      outputPath: '',
      initialBackupPath: null,
      success: false,
      classificationBefore: beforeInsp.classification,
      classificationAfter: beforeInsp.classification,
      logs: [],
      warnings: [],
      error: 'Tài liệu đã có phần đầu. Hãy dùng "Chuẩn hóa phần đầu hiện có" để tránh tạo trùng lặp.',
      inspectionAfter: beforeInsp,
    };
  }

  if (candidates.length > 1 && cfg.selectedCandidateIndex === null) {
    return {
      inputPath: item.path,
      outputPath: '',
      initialBackupPath: null,
      success: false,
      classificationBefore: 'uncertain',
      classificationAfter: 'uncertain',
      logs: [],
      warnings: [],
      error: 'Vui lòng chọn đúng vùng phần đầu trong danh sách ứng viên trước khi chuẩn hóa.',
      inspectionAfter: beforeInsp,
    };
  }

  const bodyList = doc.getElementsByTagNameNS(W_NS, 'body');
  const body = bodyList.length > 0 ? bodyList[0] : doc.documentElement;
  const children = Array.from(body.childNodes).filter(
    (n): n is Element => n.nodeType === 1,
  );

  let insertBeforeNode: Node | null = children[0] || null;
  let existingFields: DetectedHeaderFields | null = null;
  let preservedLogos: Element[] = [];
  const logs: string[] = [];

  if (mode === 'standardizeExisting' && candidates.length > 0) {
    const idx = Math.min(
      Math.max(cfg.selectedCandidateIndex ?? 0, 0),
      candidates.length - 1,
    );
    const chosen = candidates[idx];
    existingFields = chosen.summary.fields;
    preservedLogos = chosen.preservedLogoNodes;
    insertBeforeNode = children[chosen.summary.bodyEndIndex + 1] || null;

    for (let k = chosen.summary.bodyStartIndex; k <= chosen.summary.bodyEndIndex; k++) {
      const node = children[k];
      if (node && node.parentNode === body) {
        body.removeChild(node);
      }
    }
    logs.push(
      `Đã thay thế vùng phần đầu ứng viên #${idx + 1} (giữ nguyên toàn bộ thân văn bản và footer).`,
    );
  } else {
    logs.push('Đã chèn mới khối phần đầu văn bản vào đầu tài liệu.');
  }

  const builtNodes = buildHeaderNodes(doc, cfg, org, existingFields, preservedLogos);
  for (const node of builtNodes) {
    body.insertBefore(node, insertBeforeNode);
  }

  const serialized = new XMLSerializer().serializeToString(doc);
  entries.set('word/document.xml', new TextEncoder().encode(serialized));
  const outBytes = await writeZipEntries(entries);
  const copyBytes = new Uint8Array(outBytes.byteLength);
  copyBytes.set(outBytes);

  const mime =
    item.ext.toLowerCase() === 'docm'
      ? 'application/vnd.ms-word.document.macroEnabled.12'
      : 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
  const outBlob = new Blob([copyBytes], { type: mime });
  const blobUrl = URL.createObjectURL(outBlob);

  const dotIdx = item.name.lastIndexOf('.');
  const baseName = dotIdx > 0 ? item.name.slice(0, dotIdx) : item.name;
  const outputName = `${baseName}_ChuanDau.${item.ext || 'docx'}`;

  const docAfter = new DOMParser().parseFromString(serialized, 'application/xml');
  const { result: afterInsp } = inspectParsedDocument(
    docAfter,
    entries,
    item.path,
    outputName,
    cfg,
    org,
  );

  return {
    inputPath: item.path,
    outputPath: outputName,
    initialBackupPath: initialWebSnapshots.has(item.path) ? 'web-memory-snapshot' : null,
    success: true,
    classificationBefore: beforeInsp.classification,
    classificationAfter: afterInsp.classification,
    logs,
    warnings: [],
    error: null,
    inspectionAfter: afterInsp,
    blobUrl,
    outputName,
    outputBlob: outBlob,
  };
}

export async function createStandaloneWebHeaderDocx(
  cfg: HeaderBuilderConfig,
  org: OrganizationProfile | null,
  fileName = 'VanBan_KhoiTaoDau.docx',
): Promise<HeaderApplyResult> {
  const entries = new Map<string, Uint8Array>();
  const enc = new TextEncoder();

  entries.set(
    '[Content_Types].xml',
    enc.encode(
      '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>' +
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>' +
        '</Types>',
    ),
  );
  entries.set(
    '_rels/.rels',
    enc.encode(
      '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>' +
        '</Relationships>',
    ),
  );
  entries.set(
    'word/_rels/document.xml.rels',
    enc.encode(
      '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>',
    ),
  );

  const blankDocXml =
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
    `<w:document xmlns:w="${W_NS}">` +
    '<w:body>' +
    '<w:p><w:r><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr><w:t xml:space="preserve"> </w:t></w:r></w:p>' +
    '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>' +
    '</w:body></w:document>';

  const doc = new DOMParser().parseFromString(blankDocXml, 'application/xml');
  const body = doc.getElementsByTagNameNS(W_NS, 'body')[0];
  const sectPr = body.getElementsByTagNameNS(W_NS, 'sectPr')[0];
  const nodes = buildHeaderNodes(doc, cfg, org, null, []);
  for (const node of nodes) {
    body.insertBefore(node, body.firstChild || sectPr);
  }

  const serialized = new XMLSerializer().serializeToString(doc);
  entries.set('word/document.xml', enc.encode(serialized));
  const outBytes = await writeZipEntries(entries);
  const copyBytes = new Uint8Array(outBytes.byteLength);
  copyBytes.set(outBytes);

  const outBlob = new Blob([copyBytes], {
    type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  });
  const blobUrl = URL.createObjectURL(outBlob);
  const finalName = fileName.toLowerCase().endsWith('.docx') ? fileName : `${fileName}.docx`;

  const { result: afterInsp } = inspectParsedDocument(
    doc,
    entries,
    finalName,
    finalName,
    cfg,
    org,
  );

  return {
    inputPath: finalName,
    outputPath: finalName,
    initialBackupPath: null,
    success: true,
    classificationBefore: 'missingComponents',
    classificationAfter: afterInsp.classification,
    logs: [`Đã tạo tệp DOCX mới chứa phần đầu chuẩn NĐ 30: ${finalName}`],
    warnings: [],
    error: null,
    inspectionAfter: afterInsp,
    blobUrl,
    outputName: finalName,
    outputBlob: outBlob,
  };
}
