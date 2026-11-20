import {
  DEFAULT_ND30_CONFIG,
  type AuditSyncMarker,
  type DynamicTemplateFields,
  type FileQueueItem,
  type PreviewBlock,
  type PreviewBorderEdge,
  type PreviewBuildRequest,
  type PreviewDocumentRender,
  type PreviewElementDiff,
  type PreviewImageRef,
  type PreviewOverrideConfig,
  type PreviewPageModel,
  type PreviewParagraphBlock,
  type PreviewSectionSpec,
  type PreviewSessionPayload,
  type PreviewSpan,
  type PreviewTableBlock,
  type PreviewTableBorders,
  type PreviewTableCell,
  type PreviewTableRow,
  type PrintCheckReport,
  type PrintDiagnosticItem,
  type ProcessConfig,
  type StdDocScope,
} from '../types';
import { runWebComplianceAudit } from './webComplianceAudit';
import { processWebDocxFile, readZipEntries } from './webDocxEngine';
import { applyWebDocxHeader, listWebOrgProfiles } from './webHeaderBuilder';
import { executeWebStdDocument } from './webStandardizer';
import { applyWebTemplateToDocx } from './webTemplateEngine';

export const PREVIEW_FIDELITY_DISCLAIMER_VI =
  'Lưu ý độ trung thực: Bản xem trước OpenXML dựng trực tiếp khổ giấy, section, lề, phông chữ, bảng biểu, hình ảnh và ước lượng phân trang từ cấu trúc tệp; KHÔNG khẳng định khớp tuyệt đối từng dòng với Microsoft Word nếu chưa đối chiếu trực tiếp bằng Microsoft Word.';

const cancelledJobs = new Set<string>();
const mediaUrlCache = new Map<string, string>();
const MAX_MEDIA_CACHE = 40;

export function cancelWebPreviewRender(jobId: string): boolean {
  cancelledJobs.add(jobId);
  return true;
}

function isJobCancelled(jobId: string): boolean {
  return cancelledJobs.has(jobId) || cancelledJobs.has('*');
}

function twipsToMm(tw: number): number {
  return Math.round((tw / 56.692913) * 10) / 10;
}

function emuToMm(emu: number): number {
  return Math.round((emu / 36_000) * 10) / 10;
}

function twipsToPt(tw: number): number {
  return Math.round((tw / 20) * 10) / 10;
}

function round1(v: number): number {
  return Math.round(v * 10) / 10;
}

function round2(v: number): number {
  return Math.round(v * 100) / 100;
}

async function computeSha256Hex(buf: ArrayBuffer): Promise<string> {
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    const hash = await crypto.subtle.digest('SHA-256', buf);
    return Array.from(new Uint8Array(hash))
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  }
  return `len_${buf.byteLength}`;
}

export async function loadWebPreviewMediaUrl(
  item: FileQueueItem,
  mediaEntry: string,
): Promise<string | null> {
  if (!item.webFile) return null;
  const norm = mediaEntry.replace(/^\/+/, '').replace(/\\/g, '/');
  if (norm.includes('..') || !norm.toLowerCase().startsWith('word/media/')) {
    return null;
  }
  const cacheKey = `${item.path}::${norm}`;
  const hit = mediaUrlCache.get(cacheKey);
  if (hit) {
    mediaUrlCache.delete(cacheKey);
    mediaUrlCache.set(cacheKey, hit);
    return hit;
  }

  const entries = await readZipEntries(await item.webFile.arrayBuffer());
  let bytes: Uint8Array | undefined;
  for (const [k, v] of entries.entries()) {
    if (k.replace(/^\/+/, '').replace(/\\/g, '/').toLowerCase() === norm.toLowerCase()) {
      bytes = v;
      break;
    }
  }
  if (!bytes) return null;

  const mime = mimeFromMediaPath(norm);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  const url = URL.createObjectURL(new Blob([copy], { type: mime }));
  if (mediaUrlCache.size >= MAX_MEDIA_CACHE) {
    const oldestKey = mediaUrlCache.keys().next().value;
    if (oldestKey) {
      const oldUrl = mediaUrlCache.get(oldestKey);
      if (oldUrl?.startsWith('blob:')) URL.revokeObjectURL(oldUrl);
      mediaUrlCache.delete(oldestKey);
    }
  }
  mediaUrlCache.set(cacheKey, url);
  return url;
}

function mimeFromMediaPath(p: string): string {
  const low = p.toLowerCase();
  if (low.endsWith('.png')) return 'image/png';
  if (low.endsWith('.jpg') || low.endsWith('.jpeg')) return 'image/jpeg';
  if (low.endsWith('.gif')) return 'image/gif';
  if (low.endsWith('.svg')) return 'image/svg+xml';
  if (low.endsWith('.webp')) return 'image/webp';
  return 'application/octet-stream';
}

function classifyPaperLabel(wMm: number, hMm: number): string {
  const mn = Math.min(wMm, hMm);
  const mx = Math.max(wMm, hMm);
  if (Math.abs(mn - 210) <= 4 && Math.abs(mx - 297) <= 4) return 'A4';
  if (Math.abs(mn - 297) <= 5 && Math.abs(mx - 420) <= 5) return 'A3';
  if (Math.abs(mn - 215.9) <= 4 && Math.abs(mx - 279.4) <= 4) return 'Letter';
  return `Custom (${round1(wMm)}×${round1(hMm)}mm)`;
}

function wAttr(el: Element | null | undefined, localName: string): string | null {
  if (!el) return null;
  return el.getAttribute(`w:${localName}`) ?? el.getAttribute(localName);
}

function directChild(el: Element | null | undefined, localName: string): Element | null {
  if (!el) return null;
  for (let i = 0; i < el.children.length; i++) {
    const c = el.children[i];
    if (c.localName === localName || c.tagName === `w:${localName}`) return c;
  }
  return null;
}

function directChildren(el: Element | null | undefined, localName: string): Element[] {
  if (!el) return [];
  const res: Element[] = [];
  for (let i = 0; i < el.children.length; i++) {
    const c = el.children[i];
    if (c.localName === localName || c.tagName === `w:${localName}`) res.push(c);
  }
  return res;
}

interface WebStyleCatalog {
  docDefaultFont: string;
  docDefaultSzPt: number;
  byId: Map<
    string,
    {
      font?: string;
      szPt?: number;
      bold?: boolean;
      italic?: boolean;
      jc?: string;
      firstLine?: number;
      before?: number;
      after?: number;
      line?: number;
    }
  >;
}

function parseStyleCatalog(stylesXml?: string): WebStyleCatalog {
  const out: WebStyleCatalog = {
    docDefaultFont: 'Times New Roman',
    docDefaultSzPt: 14,
    byId: new Map(),
  };
  if (!stylesXml) return out;
  const doc = new DOMParser().parseFromString(stylesXml, 'application/xml');
  const root = doc.documentElement;
  if (!root) return out;

  const defRPr = directChild(directChild(directChild(root, 'docDefaults'), 'rPrDefault'), 'rPr');
  if (defRPr) {
    const rf = directChild(defRPr, 'rFonts');
    const fn = wAttr(rf, 'ascii') || wAttr(rf, 'hAnsi');
    if (fn) out.docDefaultFont = fn;
    const sz = Number(wAttr(directChild(defRPr, 'sz'), 'val'));
    if (sz > 0) out.docDefaultSzPt = sz / 2;
  }

  for (const st of directChildren(root, 'style')) {
    const id = wAttr(st, 'styleId');
    if (!id) continue;
    const rPr = directChild(st, 'rPr');
    const pPr = directChild(st, 'pPr');
    const rf = directChild(rPr, 'rFonts');
    const szVal = Number(wAttr(directChild(rPr, 'sz'), 'val'));
    const bEl = directChild(rPr, 'b');
    const iEl = directChild(rPr, 'i');
    const sp = directChild(pPr, 'spacing');
    const ind = directChild(pPr, 'ind');
    out.byId.set(id, {
      font: wAttr(rf, 'ascii') || wAttr(rf, 'hAnsi') || undefined,
      szPt: szVal > 0 ? szVal / 2 : undefined,
      bold: bEl ? wAttr(bEl, 'val') !== '0' && wAttr(bEl, 'val') !== 'false' : undefined,
      italic: iEl ? wAttr(iEl, 'val') !== '0' && wAttr(iEl, 'val') !== 'false' : undefined,
      jc: wAttr(directChild(pPr, 'jc'), 'val') || undefined,
      firstLine: ind && wAttr(ind, 'firstLine') ? Number(wAttr(ind, 'firstLine')) : undefined,
      before: sp && wAttr(sp, 'before') ? Number(wAttr(sp, 'before')) : undefined,
      after: sp && wAttr(sp, 'after') ? Number(wAttr(sp, 'after')) : undefined,
      line: sp && wAttr(sp, 'line') ? Number(wAttr(sp, 'line')) : undefined,
    });
  }
  return out;
}

function parseRelsMap(relsXml?: string): Map<string, string> {
  const map = new Map<string, string>();
  if (!relsXml) return map;
  const doc = new DOMParser().parseFromString(relsXml, 'application/xml');
  for (const el of Array.from(doc.getElementsByTagName('Relationship'))) {
    const id = el.getAttribute('Id');
    const tgt = el.getAttribute('Target');
    if (id && tgt) {
      const clean = tgt.replace(/^\/+/, '').replace(/\\/g, '/').replace(/^(\.\.\/)+/, '');
      map.set(id, clean.startsWith('word/') || clean.startsWith('http') ? clean : `word/${clean}`);
    }
  }
  return map;
}

function estimateParaHeightMm(p: PreviewParagraphBlock, colWidthMm: number): number {
  const ptToMm = 0.352778;
  const maxSz = p.spans.reduce((acc, s) => Math.max(acc, s.fontSizePt), 13.5);
  const lineH = p.exactLinePt
    ? p.exactLinePt * ptToMm
    : maxSz * ptToMm * 1.2 * Math.max(0.85, p.lineSpacingMult);
  const textW = Math.max(20, colWidthMm - p.indentLeftMm - p.indentRightMm);
  const avgCharMm = Math.max(1.2, maxSz * ptToMm * 0.48);
  const charsPerLine = Math.max(12, Math.floor(textW / avgCharMm));

  let visualLines = 0;
  const flDelta = Math.max(0, Math.floor(p.firstLineIndentMm / avgCharMm));
  for (const line of p.plainText.split('\n')) {
    if (!line.length) {
      visualLines += 1;
    } else {
      visualLines += Math.ceil((line.length + flDelta) / charsPerLine);
    }
  }
  if (visualLines === 0 && p.images.length === 0) visualLines = 1;
  const imgH = p.images.reduce((sum, im) => sum + im.heightMm + 2, 0);
  const spacing = (p.spaceBeforePt + p.spaceAfterPt) * ptToMm;
  return round2(Math.max(4, visualLines * lineH + imgH + spacing));
}

function extractWebPara(
  pEl: Element,
  elementPath: string,
  globalIndex: number,
  sectionIndex: number,
  colWidthMm: number,
  styles: WebStyleCatalog,
  rels: Map<string, string>,
  forcedRole?: string,
): PreviewParagraphBlock {
  const pPr = directChild(pEl, 'pPr');
  const styleId = wAttr(directChild(pPr, 'pStyle'), 'val') || '';
  const st = styles.byId.get(styleId);

  const alignment = wAttr(directChild(pPr, 'jc'), 'val') || st?.jc || 'left';
  const ind = directChild(pPr, 'ind');
  const sp = directChild(pPr, 'spacing');

  const firstLineTw = Number(wAttr(ind, 'firstLine') ?? st?.firstLine ?? 0) || 0;
  const hangTw = Number(wAttr(ind, 'hanging') ?? 0) || 0;
  const leftTw = Number(wAttr(ind, 'left') ?? 0) || 0;
  const rightTw = Number(wAttr(ind, 'right') ?? 0) || 0;

  const beforeTw = Number(wAttr(sp, 'before') ?? st?.before ?? 0) || 0;
  const afterTw = Number(wAttr(sp, 'after') ?? st?.after ?? 120) || 0;
  const lineRaw = Number(wAttr(sp, 'line') ?? st?.line ?? 276) || 276;
  const lineRule = wAttr(sp, 'lineRule') || 'auto';
  const isExact = lineRule === 'exact' || lineRule === 'atLeast';
  const lineSpacingMult = isExact
    ? round2(Math.max(1, lineRaw / 20 / 14))
    : round2(Math.min(3, Math.max(0.85, lineRaw / 240)));
  const exactLinePt = isExact ? round1(Math.max(8, lineRaw / 20)) : null;

  const spans: PreviewSpan[] = [];
  const images: PreviewImageRef[] = [];
  let hasLeadingBreak = false;
  let hasExplicitBreak = false;
  let hasLastRenderedBreak = false;

  const defFont = st?.font || styles.docDefaultFont || 'Times New Roman';
  const defSz = st?.szPt || styles.docDefaultSzPt || 14;
  const defBold = Boolean(st?.bold);
  const defItalic = Boolean(st?.italic);

  let fldInstr: string | null = null;
  let inResult = false;

  const walkRun = (r: Element) => {
    const rPr = directChild(r, 'rPr');
    const rf = directChild(rPr, 'rFonts');
    const fontFamily = wAttr(rf, 'ascii') || wAttr(rf, 'hAnsi') || defFont;
    const szHalf = Number(wAttr(directChild(rPr, 'sz'), 'val'));
    const fontSizePt = szHalf > 0 ? szHalf / 2 : defSz;
    const bEl = directChild(rPr, 'b');
    const iEl = directChild(rPr, 'i');
    const uEl = directChild(rPr, 'u');
    const strEl = directChild(rPr, 'strike');
    const bold = bEl ? wAttr(bEl, 'val') !== '0' && wAttr(bEl, 'val') !== 'false' : defBold;
    const italic = iEl ? wAttr(iEl, 'val') !== '0' && wAttr(iEl, 'val') !== 'false' : defItalic;
    const underline = uEl ? (wAttr(uEl, 'val') || 'single') !== 'none' : false;
    const strike = strEl ? wAttr(strEl, 'val') !== '0' && wAttr(strEl, 'val') !== 'false' : false;
    const colAttr = wAttr(directChild(rPr, 'color'), 'val');
    const colorHex = colAttr && colAttr !== 'auto' && colAttr.length === 6 ? colAttr : '000000';
    const vertAlign = wAttr(directChild(rPr, 'vertAlign'), 'val') || 'baseline';

    for (let k = 0; k < r.children.length; k++) {
      const ch = r.children[k];
      if (ch.localName === 'fldChar') {
        const ft = wAttr(ch, 'fldCharType');
        if (ft === 'begin') {
          fldInstr = '';
          inResult = false;
        } else if (ft === 'separate') {
          inResult = true;
        } else if (ft === 'end') {
          fldInstr = null;
          inResult = false;
        }
      } else if (ch.localName === 'instrText' && fldInstr !== null) {
        fldInstr += ch.textContent || '';
      } else if (ch.localName === 'br') {
        if (wAttr(ch, 'type') === 'page') {
          if (spans.length === 0 && images.length === 0) hasLeadingBreak = true;
          else hasExplicitBreak = true;
        } else {
          spans.push({
            text: '\n',
            fontFamily,
            fontSizePt,
            bold,
            italic,
            underline,
            strike,
            colorHex,
            highlightHex: null,
            vertAlign,
            isDynamicPageField: null,
          });
        }
      } else if (ch.localName === 'lastRenderedPageBreak') {
        hasLastRenderedBreak = true;
      } else if (ch.localName === 'tab') {
        spans.push({
          text: '    ',
          fontFamily,
          fontSizePt,
          bold,
          italic,
          underline,
          strike,
          colorHex,
          highlightHex: null,
          vertAlign,
          isDynamicPageField: null,
        });
      } else if (ch.localName === 't') {
        const dyn =
          inResult && fldInstr
            ? fldInstr.toUpperCase().includes('NUMPAGES')
              ? 'NUMPAGES'
              : fldInstr.toUpperCase().includes('PAGE')
                ? 'PAGE'
                : null
            : null;
        const txt = ch.textContent || '';
        if (txt || dyn) {
          spans.push({
            text: txt,
            fontFamily,
            fontSizePt,
            bold,
            italic,
            underline,
            strike,
            colorHex,
            highlightHex: null,
            vertAlign,
            isDynamicPageField: dyn,
          });
        }
      } else if (ch.localName === 'drawing') {
        const blips = Array.from(ch.getElementsByTagName('*')).filter((n) => n.localName === 'blip');
        for (const bl of blips) {
          if (ch.getElementsByTagName('*').length > 0 && Array.from(ch.getElementsByTagName('*')).some((n) => n.localName === 'txbxContent')) {
            continue;
          }
          const rid = bl.getAttribute('r:embed') || bl.getAttribute('embed') || bl.getAttribute('r:link');
          if (!rid) continue;
          const tgt = rels.get(rid);
          if (!tgt) continue;
          const extEl = Array.from(ch.getElementsByTagName('*')).find((n) => n.localName === 'extent');
          const cx = Number(extEl?.getAttribute('cx') || 1800000);
          const cy = Number(extEl?.getAttribute('cy') || 1200000);
          const wMm = Math.max(4, emuToMm(cx));
          const hMm = Math.max(4, emuToMm(cy));
          const docPr = Array.from(ch.getElementsByTagName('*')).find((n) => n.localName === 'docPr');
          images.push({
            relId: rid,
            mediaEntry: tgt,
            mimeType: mimeFromMediaPath(tgt),
            widthMm: wMm,
            heightMm: hMm,
            wrapMode: Array.from(ch.getElementsByTagName('*')).some((n) => n.localName === 'anchor')
              ? 'floating'
              : 'inline',
            altText: docPr?.getAttribute('descr') || docPr?.getAttribute('name') || 'Hình ảnh',
            overflowsPrintableArea: wMm > colWidthMm + 1.5,
          });
        }
      }
    }
  };

  for (let i = 0; i < pEl.children.length; i++) {
    const c = pEl.children[i];
    if (c.localName === 'r') walkRun(c);
    else if (c.localName === 'hyperlink') {
      for (const r of directChildren(c, 'r')) walkRun(r);
    } else if (c.localName === 'fldSimple') {
      const instr = (wAttr(c, 'instr') || '').toUpperCase();
      if (instr.includes('PAGE') || instr.includes('NUMPAGES')) {
        spans.push({
          text: '1',
          fontFamily: defFont,
          fontSizePt: defSz,
          bold: defBold,
          italic: defItalic,
          underline: false,
          strike: false,
          colorHex: '000000',
          highlightHex: null,
          vertAlign: 'baseline',
          isDynamicPageField: instr.includes('NUMPAGES') ? 'NUMPAGES' : 'PAGE',
        });
      }
    }
  }

  const plainText = spans.map((s) => s.text).join('');
  let role = forcedRole || 'body';
  const lowStyle = styleId.toLowerCase();
  if (!forcedRole) {
    if (lowStyle === 'heading1' || lowStyle === 'heading 1') role = 'heading1';
    else if (lowStyle === 'heading2' || lowStyle === 'heading 2') role = 'heading2';
    else if (lowStyle.startsWith('heading')) role = 'heading3';
  }

  const hasPageBreakBefore =
    Boolean(directChild(pPr, 'pageBreakBefore')) ||
    (hasLeadingBreak && (spans.length > 0 || images.length > 0));
  if (hasLeadingBreak && spans.length === 0 && images.length === 0) {
    hasExplicitBreak = true;
  }

  const block: PreviewParagraphBlock = {
    elementPath,
    globalIndex,
    sectionIndex,
    role,
    listPrefix: '',
    alignment,
    indentLeftMm: twipsToMm(leftTw),
    indentRightMm: twipsToMm(rightTw),
    firstLineIndentMm: twipsToMm(firstLineTw),
    hangingIndentMm: twipsToMm(hangTw),
    spaceBeforePt: twipsToPt(beforeTw),
    spaceAfterPt: twipsToPt(afterTw),
    lineSpacingMult,
    exactLinePt,
    keepNext: Boolean(directChild(pPr, 'keepNext')),
    pageBreakBefore: hasPageBreakBefore,
    hasExplicitPageBreak: hasExplicitBreak,
    hasLastRenderedBreak: hasLastRenderedBreak,
    plainText,
    spans,
    images,
    estimatedHeightMm: 4,
  };
  block.estimatedHeightMm = estimateParaHeightMm(block, colWidthMm);
  return block;
}

function parseEdge(el: Element | null, defVis: boolean): PreviewBorderEdge {
  if (!el) {
    return {
      visible: defVis,
      style: defVis ? 'single' : 'none',
      widthPt: defVis ? 0.5 : 0,
      colorHex: '334155',
    };
  }
  const style = wAttr(el, 'val') || 'single';
  const visible = style !== 'none' && style !== 'nil';
  const sz = Number(wAttr(el, 'sz') ?? (visible ? 4 : 0));
  const col = wAttr(el, 'color');
  return {
    visible,
    style,
    widthPt: round2(Math.min(4, Math.max(0.25, sz / 8))),
    colorHex: col && col !== 'auto' && col.length === 6 ? col : '334155',
  };
}

function extractBorders(
  el: Element | null,
  fallback: PreviewTableBorders | null,
  defVis: boolean,
): PreviewTableBorders {
  return {
    top: parseEdge(directChild(el, 'top'), fallback?.top.visible ?? defVis),
    bottom: parseEdge(directChild(el, 'bottom'), fallback?.bottom.visible ?? defVis),
    left: parseEdge(directChild(el, 'left'), fallback?.left.visible ?? defVis),
    right: parseEdge(directChild(el, 'right'), fallback?.right.visible ?? defVis),
  };
}

function paginateWebBlocks(
  sections: PreviewSectionSpec[],
  blocks: PreviewBlock[],
  preferLastRendered: boolean,
): PreviewPageModel[] {
  if (!sections.length) return [];

  interface DraftPage {
    sectionIndex: number;
    isFirstOfSect: boolean;
    blocks: PreviewBlock[];
    usedMm: number;
    breakReason: string;
    overflow: boolean;
  }

  const drafts: DraftPage[] = [];
  let curSectIdx = sections[0].sectionIndex;
  const getSect = (idx: number) => sections.find((s) => s.sectionIndex === idx) || sections[0];
  const pushPage = (sectIdx: number, first: boolean, reason: string) =>
    drafts.push({
      sectionIndex: sectIdx,
      isFirstOfSect: first,
      blocks: [],
      usedMm: 0,
      breakReason: reason,
      overflow: false,
    });

  pushPage(curSectIdx, true, 'sectionStart');

  for (let i = 0; i < blocks.length; i++) {
    const blk = blocks[i];
    if (blk.sectionIndex !== curSectIdx) {
      curSectIdx = blk.sectionIndex;
      if (drafts[drafts.length - 1].blocks.length > 0) {
        pushPage(curSectIdx, true, 'sectionBreak');
      } else {
        drafts[drafts.length - 1].sectionIndex = curSectIdx;
        drafts[drafts.length - 1].isFirstOfSect = true;
      }
    }

    const sect = getSect(curSectIdx);
    const capMm = Math.max(40, sect.heightMm - sect.marginTopMm - sect.marginBottomMm);

    if (blk.kind === 'paragraph') {
      const last = drafts[drafts.length - 1];
      if (
        last.blocks.length > 0 &&
        (blk.pageBreakBefore || (preferLastRendered && blk.hasLastRenderedBreak))
      ) {
        pushPage(curSectIdx, false, blk.pageBreakBefore ? 'pageBreakBefore' : 'wordCachedBreak');
      }
      const nextH = blk.keepNext && blocks[i + 1]
        ? blocks[i + 1].kind === 'paragraph'
          ? Math.min(28, (blocks[i + 1] as PreviewParagraphBlock).estimatedHeightMm)
          : 10
        : 0;
      const pg = drafts[drafts.length - 1];
      if (pg.usedMm > 1 && pg.usedMm + blk.estimatedHeightMm + nextH > capMm) {
        pushPage(curSectIdx, false, 'autoFlow');
      }
      const tgt = drafts[drafts.length - 1];
      tgt.blocks.push(blk);
      tgt.usedMm = round2(tgt.usedMm + blk.estimatedHeightMm);
      if (blk.images.some((im) => im.overflowsPrintableArea) || tgt.usedMm > capMm + 2) {
        tgt.overflow = true;
      }
      if (blk.hasExplicitPageBreak) pushPage(curSectIdx, false, 'explicitPageBreak');
    } else {
      const rowH = (r: PreviewTableRow) =>
        Math.max(
          r.minHeightMm || 0,
          r.cells.reduce(
            (mx, c) =>
              c.mergedContinuation
                ? mx
                : Math.max(mx, c.paragraphs.reduce((s, p) => s + p.estimatedHeightMm, 0) + 2.4),
            5,
          ),
        );
      const totalH = round2(blk.rows.reduce((s, r) => s + rowH(r), 0) + 2);
      const cur = drafts[drafts.length - 1];
      if (cur.usedMm + totalH <= capMm || (blk.rows.length <= 2 && cur.usedMm < 1)) {
        cur.blocks.push(blk);
        cur.usedMm = round2(cur.usedMm + totalH);
        if (blk.overflowsPrintableWidth) cur.overflow = true;
        continue;
      }

      const hdrRows = blk.rows.filter((r) => r.isHeaderRow);
      let rIdx = 0;
      let continued = false;
      while (rIdx < blk.rows.length) {
        const tgt = drafts[drafts.length - 1];
        const slice: PreviewTableRow[] = [];
        let sliceH = 0;
        if (continued && hdrRows.length) {
          for (const hr of hdrRows) {
            slice.push(hr);
            sliceH += rowH(hr);
          }
        }
        const startR = blk.rows[rIdx].rowIndex;
        let endR = startR;
        while (rIdx < blk.rows.length) {
          const r = blk.rows[rIdx];
          const h = rowH(r);
          const left = Math.max(8, capMm - tgt.usedMm);
          if (slice.length > 0 && sliceH + h > left) break;
          slice.push(r);
          sliceH += h;
          endR = r.rowIndex + 1;
          rIdx++;
          if (sliceH >= left) break;
        }
        tgt.blocks.push({ ...blk, rows: slice, rowSliceRange: [startR, endR] });
        tgt.usedMm = round2(tgt.usedMm + sliceH + 1.5);
        if (blk.overflowsPrintableWidth) tgt.overflow = true;
        if (rIdx < blk.rows.length) {
          continued = true;
          pushPage(curSectIdx, false, 'tableContinued');
        }
      }
    }
  }

  if (drafts.length > 1 && drafts[drafts.length - 1].blocks.length === 0) drafts.pop();

  const totalPages = Math.max(1, drafts.length);
  let disp = sections[0].pageNumStart || 1;
  let prevSect: number | null = null;

  const patchHf = (list: PreviewParagraphBlock[], pNum: number) =>
    list.map((b) => {
      const spans = b.spans.map((s) =>
        s.isDynamicPageField
          ? { ...s, text: s.isDynamicPageField === 'NUMPAGES' ? String(totalPages) : String(pNum) }
          : s,
      );
      return { ...b, spans, plainText: spans.map((s) => s.text).join('') };
    });

  return drafts.map((d, idx) => {
    const sect = getSect(d.sectionIndex);
    if (prevSect !== d.sectionIndex) {
      if (sect.pageNumStart) disp = sect.pageNumStart;
      prevSect = d.sectionIndex;
    }
    const pDisp = disp++;
    const rawHdr = d.isFirstOfSect && sect.differentFirstPage ? sect.firstHeaderBlocks : sect.defaultHeaderBlocks;
    const rawFtr = d.isFirstOfSect && sect.differentFirstPage ? sect.firstFooterBlocks : sect.defaultFooterBlocks;
    const capMm = Math.max(40, sect.heightMm - sect.marginTopMm - sect.marginBottomMm);

    const pathSet = new Set<string>();
    for (const b of d.blocks) {
      pathSet.add(b.elementPath);
      if (b.kind === 'table') {
        pathSet.add(`tbl[${b.tableIndex}]`);
        for (const r of b.rows) for (const c of r.cells) for (const cp of c.paragraphs) pathSet.add(cp.elementPath);
      }
    }

    return {
      pageNumber: idx + 1,
      displayPageNumber: pDisp,
      totalPages,
      sectionIndex: d.sectionIndex,
      isFirstPageOfSection: d.isFirstOfSect,
      paperLabel: sect.paperLabel,
      orientation: sect.orientation,
      widthMm: sect.widthMm,
      heightMm: sect.heightMm,
      marginTopMm: sect.marginTopMm,
      marginBottomMm: sect.marginBottomMm,
      marginLeftMm: sect.marginLeftMm,
      marginRightMm: sect.marginRightMm,
      headerBlocks: patchHf(rawHdr, pDisp),
      footerBlocks: patchHf(rawFtr, pDisp),
      blocks: d.blocks,
      usedHeightMm: d.usedMm,
      fillRatioPct: round1(Math.min(120, (d.usedMm / capMm) * 100)),
      breakReason: d.breakReason,
      hasOverflowWarning: d.overflow,
      elementPathsOnPage: Array.from(pathSet),
    };
  });
}

function evaluateWebPrintCheck(
  pages: PreviewPageModel[],
  sections: PreviewSectionSpec[],
  scope: StdDocScope,
): PrintCheckReport {
  const diagnostics: PrintDiagnosticItem[] = [];
  let clippedCount = 0;
  let overflowTableCount = 0;
  let seq = 0;
  const isFormal = ['adminNd30', 'partyDocument', 'specializedLegal', 'academicThesis'].includes(scope);

  for (const s of sections) {
    const firstPg = pages.find((p) => p.sectionIndex === s.sectionIndex)?.pageNumber || 1;
    if (isFormal && s.paperLabel !== 'A4') {
      diagnostics.push({
        itemId: `PRT-${++seq}`,
        kind: 'paperSize',
        severity: 'error',
        pageNumber: firstPg,
        elementPath: `section[${s.sectionIndex}]`,
        title: `Section ${s.sectionIndex}: khổ giấy ${s.paperLabel} (chuẩn là A4)`,
        detail: `${s.widthMm}×${s.heightMm}mm`,
      });
    }
    if (Math.min(s.marginTopMm, s.marginBottomMm, s.marginLeftMm, s.marginRightMm) < 5) {
      clippedCount++;
      diagnostics.push({
        itemId: `PRT-${++seq}`,
        kind: 'clippedContent',
        severity: 'critical',
        pageNumber: firstPg,
        elementPath: `section[${s.sectionIndex}]`,
        title: `Lề Section ${s.sectionIndex} nhỏ hơn 5mm — nguy cơ máy in cắt mép`,
        detail: `Trên ${s.marginTopMm} · Dưới ${s.marginBottomMm} · Trái ${s.marginLeftMm} · Phải ${s.marginRightMm}mm`,
      });
    }
  }

  for (const pg of pages) {
    for (const b of pg.blocks) {
      if (b.kind === 'paragraph') {
        for (const im of b.images) {
          if (im.overflowsPrintableArea) {
            clippedCount++;
            diagnostics.push({
              itemId: `PRT-${++seq}`,
              kind: 'clippedContent',
              severity: 'error',
              pageNumber: pg.pageNumber,
              elementPath: b.elementPath,
              title: `Trang ${pg.pageNumber}: ảnh vượt vùng in (${im.widthMm}mm)`,
              detail: im.altText,
            });
          }
        }
      } else if (b.overflowsPrintableWidth) {
        overflowTableCount++;
        diagnostics.push({
          itemId: `PRT-${++seq}`,
          kind: 'tableOverflow',
          severity: 'error',
          pageNumber: pg.pageNumber,
          elementPath: b.elementPath,
          title: `Trang ${pg.pageNumber}: bảng #${b.tableIndex + 1} tràn lề phải (${b.totalWidthMm}mm)`,
          detail: `Vượt bề rộng vùng in của Section ${b.sectionIndex}`,
        });
      }
    }
  }

  return {
    isPrintReady: clippedCount === 0 && overflowTableCount === 0 && !diagnostics.some((d) => d.severity === 'error' || d.severity === 'critical'),
    paperSummary: `${Array.from(new Set(sections.map((s) => s.paperLabel))).join(' + ')} (${sections.length} Section)`,
    orientationSummary: Array.from(new Set(sections.map((s) => (s.orientation === 'landscape' ? 'Ngang' : 'Dọc')))).join(' → '),
    marginSummary: sections[0]
      ? `S1: T ${sections[0].marginTopMm} · D ${sections[0].marginBottomMm} · Tr ${sections[0].marginLeftMm} · P ${sections[0].marginRightMm} mm`
      : '',
    headerFooterSummary: sections.some((s) => s.defaultHeaderBlocks.length || s.defaultFooterBlocks.length)
      ? 'Có Header/Footer'
      : 'Chưa gắn Header/Footer',
    pageNumberSummary: sections.some((s) => s.hasDynamicPageField)
      ? `Đánh số tự động (${pages.length} trang)`
      : `${pages.length} trang`,
    clippedCount,
    overflowTableCount,
    warningCount: diagnostics.length,
    diagnostics,
    fidelityDisclaimer: PREVIEW_FIDELITY_DISCLAIMER_VI,
  };
}

async function extractWebDocumentRender(buf: ArrayBuffer, label: string, scope: StdDocScope): Promise<PreviewDocumentRender> {
  const entries = await readZipEntries(buf);
  const dec = new TextDecoder('utf-8');
  const getXml = (name: string): string | undefined => {
    for (const [k, v] of entries.entries()) {
      if (k.replace(/^\/+/, '').replace(/\\/g, '/').toLowerCase() === name.toLowerCase()) {
        return dec.decode(v);
      }
    }
    return undefined;
  };

  const docXml = getXml('word/document.xml');
  if (!docXml) throw new Error('Không tìm thấy word/document.xml.');
  const doc = new DOMParser().parseFromString(docXml, 'application/xml');
  const body = doc.getElementsByTagNameNS('*', 'body')[0] || doc.getElementsByTagName('w:body')[0];
  if (!body) throw new Error('Thiếu w:body.');

  const styles = parseStyleCatalog(getXml('word/styles.xml'));
  const rels = parseRelsMap(getXml('word/_rels/document.xml.rels'));

  const sections: PreviewSectionSpec[] = [];
  const inlineSectMap = new Map<number, number>();
  const children = Array.from(body.children);

  const parseSect = (sect: Element, idx: number): PreviewSectionSpec => {
    const pgSz = directChild(sect, 'pgSz');
    const pgW = Number(wAttr(pgSz, 'w') || 11906);
    const pgH = Number(wAttr(pgSz, 'h') || 16838);
    const orient = (wAttr(pgSz, 'orient') || (pgW > pgH ? 'landscape' : 'portrait')).toLowerCase();
    const mar = directChild(sect, 'pgMar');
    const mt = twipsToMm(Number(wAttr(mar, 'top') || 1134));
    const mb = twipsToMm(Number(wAttr(mar, 'bottom') || 1134));
    const ml = twipsToMm(Number(wAttr(mar, 'left') || 1701));
    const mr = twipsToMm(Number(wAttr(mar, 'right') || 850));
    const wMm = twipsToMm(pgW);
    const hMm = twipsToMm(pgH);
    const pW = Math.max(40, wMm - ml - mr);

    const defH: PreviewParagraphBlock[] = [];
    const firstH: PreviewParagraphBlock[] = [];
    const defF: PreviewParagraphBlock[] = [];
    const firstF: PreviewParagraphBlock[] = [];
    let hasDyn = false;

    for (const ch of Array.from(sect.children)) {
      if (ch.localName === 'headerReference' || ch.localName === 'footerReference') {
        const t = wAttr(ch, 'type') || 'default';
        const rid = ch.getAttribute('r:id') || ch.getAttribute('id');
        const tgt = rid ? rels.get(rid) : undefined;
        const xml = tgt ? getXml(tgt) : undefined;
        if (!tgt || !xml) continue;
        const hdDoc = new DOMParser().parseFromString(xml, 'application/xml');
        const pEls = Array.from(hdDoc.documentElement.children).filter((n) => n.localName === 'p');
        const blocks = pEls
          .map((p, pi) => extractWebPara(p, `${tgt}/p[${pi}]`, pi, idx, pW, styles, rels, 'headerFooter'))
          .filter((b) => b.plainText.trim() || b.images.length || b.spans.some((s) => s.isDynamicPageField));
        if (blocks.some((b) => b.spans.some((s) => s.isDynamicPageField))) hasDyn = true;
        if (ch.localName === 'headerReference') {
          if (t === 'first') firstH.push(...blocks);
          else if (!defH.length) defH.push(...blocks);
        } else {
          if (t === 'first') firstF.push(...blocks);
          else if (!defF.length) defF.push(...blocks);
        }
      }
    }

    const pgNumStartAttr = wAttr(directChild(sect, 'pgNumType'), 'start');
    return {
      sectionIndex: idx,
      paperLabel: classifyPaperLabel(wMm, hMm),
      orientation: orient,
      widthMm: wMm,
      heightMm: hMm,
      marginTopMm: mt,
      marginBottomMm: mb,
      marginLeftMm: ml,
      marginRightMm: mr,
      headerDistanceMm: twipsToMm(Number(wAttr(mar, 'header') || 708)),
      footerDistanceMm: twipsToMm(Number(wAttr(mar, 'footer') || 708)),
      differentFirstPage: Boolean(directChild(sect, 'titlePg')),
      pageNumStart: pgNumStartAttr ? Number(pgNumStartAttr) : null,
      hasDynamicPageField: hasDyn,
      defaultHeaderBlocks: defH,
      firstHeaderBlocks: firstH,
      defaultFooterBlocks: defF,
      firstFooterBlocks: firstF,
    };
  };

  children.forEach((el, bIdx) => {
    if (el.localName === 'p') {
      const sp = directChild(directChild(el, 'pPr'), 'sectPr');
      if (sp) {
        const sIdx = sections.length + 1;
        inlineSectMap.set(bIdx, sIdx);
        sections.push(parseSect(sp, sIdx));
      }
    }
  });
  const bodySect = directChild(body, 'sectPr');
  if (bodySect) sections.push(parseSect(bodySect, sections.length + 1));
  if (!sections.length) {
    sections.push({
      sectionIndex: 1,
      paperLabel: 'A4',
      orientation: 'portrait',
      widthMm: 210,
      heightMm: 297,
      marginTopMm: 20,
      marginBottomMm: 20,
      marginLeftMm: 30,
      marginRightMm: 15,
      headerDistanceMm: 12.5,
      footerDistanceMm: 12.5,
      differentFirstPage: false,
      pageNumStart: null,
      hasDynamicPageField: false,
      defaultHeaderBlocks: [],
      firstHeaderBlocks: [],
      defaultFooterBlocks: [],
      firstFooterBlocks: [],
    });
  }

  const blocks: PreviewBlock[] = [];
  let curSect = 1;
  let gPara = 0;
  let tSeq = 0;
  let imgCnt = 0;

  children.forEach((el, bIdx) => {
    const sect = sections.find((s) => s.sectionIndex === curSect) || sections[0];
    const pW = Math.max(40, sect.widthMm - sect.marginLeftMm - sect.marginRightMm);

    if (el.localName === 'p') {
      const pBlk = extractWebPara(el, `body/p[${bIdx}]`, gPara++, curSect, pW, styles, rels);
      imgCnt += pBlk.images.length;
      blocks.push({ kind: 'paragraph', ...pBlk });
      if (inlineSectMap.has(bIdx)) curSect = Math.min(sections.length, curSect + 1);
    } else if (el.localName === 'tbl') {
      const tIdx = tSeq++;
      const tblPr = directChild(el, 'tblPr');
      const gridCols = directChildren(directChild(el, 'tblGrid'), 'gridCol').map((gc) =>
        twipsToMm(Number(wAttr(gc, 'w') || 2000)),
      );
      const tblBordersEl = directChild(tblPr, 'tblBorders');
      const bVal = wAttr(directChild(tblBordersEl, 'top'), 'val');
      const isAdminTbl = bVal === 'none' || bVal === 'nil';
      const tblRole = isAdminTbl ? 'adminHeader' : 'dataTable';
      const defBorders = extractBorders(tblBordersEl, null, !isAdminTbl);
      const indMm = twipsToMm(Number(wAttr(directChild(tblPr, 'tblInd'), 'w') || 0));

      const rows: PreviewTableRow[] = directChildren(el, 'tr').map((tr, rIdx) => {
        const trPr = directChild(tr, 'trPr');
        let colCursor = 0;
        const cells: PreviewTableCell[] = directChildren(tr, 'tc').map((tc, cIdx) => {
          const tcPr = directChild(tc, 'tcPr');
          const cSpan = Math.max(1, Number(wAttr(directChild(tcPr, 'gridSpan'), 'val') || 1));
          const vm = directChild(tcPr, 'vMerge');
          const mergedContinuation = Boolean(vm && wAttr(vm, 'val') !== 'restart');
          const cellW =
            gridCols.slice(colCursor, colCursor + cSpan).reduce((s, v) => s + v, 0) ||
            Math.max(20, pW / 2);
          const paras = directChildren(tc, 'p').map((p, cpIdx) => {
            const pb = extractWebPara(
              p,
              `body/tbl[${bIdx}]/tr[${rIdx}]/tc[${cIdx}]/p[${cpIdx}]`,
              gPara++,
              curSect,
              Math.max(12, cellW - 3.5),
              styles,
              rels,
              tblRole === 'dataTable' ? 'tableCell' : 'adminHeader',
            );
            imgCnt += pb.images.length;
            return pb;
          });
          const cellObj: PreviewTableCell = {
            colIndex: colCursor,
            colSpan: cSpan,
            rowSpan: 1,
            mergedContinuation,
            widthMm: round1(cellW),
            vAlign: wAttr(directChild(tcPr, 'vAlign'), 'val') || 'top',
            shadingHex: wAttr(directChild(tcPr, 'shd'), 'fill') || null,
            borders: extractBorders(directChild(tcPr, 'tcBorders'), defBorders, !isAdminTbl),
            paragraphs: paras,
          };
          colCursor += cSpan;
          return cellObj;
        });
        return {
          rowIndex: rIdx,
          isHeaderRow: Boolean(directChild(trPr, 'tblHeader')),
          cantSplit: Boolean(directChild(trPr, 'cantSplit')),
          minHeightMm: twipsToMm(Number(wAttr(directChild(trPr, 'trHeight'), 'val') || 0)),
          cells,
        };
      });

      const totalW = round1(gridCols.reduce((s, v) => s + v, 0) || pW);
      const tblBlk: PreviewTableBlock = {
        elementPath: `body/tbl[${bIdx}]`,
        tableIndex: tIdx,
        sectionIndex: curSect,
        tableRole: tblRole,
        alignment: wAttr(directChild(tblPr, 'jc'), 'val') || 'left',
        indentLeftMm: indMm,
        totalWidthMm: totalW,
        colWidthsMm: gridCols,
        borders: defBorders,
        overflowsPrintableWidth: totalW + indMm > pW + 1.5,
        rows,
        rowSliceRange: [0, rows.length],
      };
      blocks.push({ kind: 'table', ...tblBlk });
    }
  });

  const pages = paginateWebBlocks(
    sections,
    blocks,
    blocks.some((b) => b.kind === 'paragraph' && b.hasLastRenderedBreak),
  );
  return {
    label,
    docSha256: await computeSha256Hex(buf),
    sectionCount: sections.length,
    totalPages: pages.length,
    paragraphCount: gPara,
    tableCount: tSeq,
    imageCount: imgCnt,
    sections,
    pages,
    printCheck: evaluateWebPrintCheck(pages, sections, scope),
  };
}

export function applyClientIncrementalPatch(
  baseDoc: PreviewDocumentRender,
  ov: PreviewOverrideConfig,
  scope: StdDocScope,
): PreviewDocumentRender {
  const sections: PreviewSectionSpec[] = baseDoc.sections.map((s) => {
    let wMm = s.widthMm;
    let hMm = s.heightMm;
    let orient = s.orientation;
    let label = s.paperLabel;
    if (ov.paperSize) {
      const [pw, ph] =
        ov.paperSize === 'a3' ? [297, 420] : ov.paperSize === 'letter' ? [215.9, 279.4] : [210, 297];
      const isLand =
        ov.orientationOverride === 'allLandscape'
          ? true
          : ov.orientationOverride === 'allPortrait'
            ? false
            : s.orientation === 'landscape';
      wMm = isLand ? ph : pw;
      hMm = isLand ? pw : ph;
      orient = isLand ? 'landscape' : 'portrait';
      label = classifyPaperLabel(wMm, hMm);
    } else if (ov.orientationOverride === 'allLandscape' || ov.orientationOverride === 'allPortrait') {
      const isLand = ov.orientationOverride === 'allLandscape';
      wMm = isLand ? Math.max(s.widthMm, s.heightMm) : Math.min(s.widthMm, s.heightMm);
      hMm = isLand ? Math.min(s.widthMm, s.heightMm) : Math.max(s.widthMm, s.heightMm);
      orient = isLand ? 'landscape' : 'portrait';
    }

    const tBase = ov.marginTopCm != null ? ov.marginTopCm * 10 : s.marginTopMm;
    const bBase = ov.marginBottomCm != null ? ov.marginBottomCm * 10 : s.marginBottomMm;
    const lBase = ov.marginLeftCm != null ? ov.marginLeftCm * 10 : s.marginLeftMm;
    const rBase = ov.marginRightCm != null ? ov.marginRightCm * 10 : s.marginRightMm;
    const hasMarginOv =
      ov.marginTopCm != null || ov.marginBottomCm != null || ov.marginLeftCm != null || ov.marginRightCm != null;
    const swap = orient === 'landscape' && ov.autoSwapBindingMargin && hasMarginOv;

    return {
      ...s,
      paperLabel: label,
      orientation: orient,
      widthMm: wMm,
      heightMm: hMm,
      marginTopMm: swap ? lBase : tBase,
      marginBottomMm: swap ? rBase : bBase,
      marginLeftMm: swap ? bBase : lBase,
      marginRightMm: swap ? tBase : rBase,
    };
  });

  // Reconstruct full ordered block list by merging sliced tables back cleanly
  const seenTables = new Map<string, PreviewBlock>();
  const flatBlocks: PreviewBlock[] = [];
  for (const pg of baseDoc.pages) {
    for (const blk of pg.blocks) {
      if (blk.kind === 'paragraph') flatBlocks.push(blk);
      else if (!seenTables.has(blk.elementPath)) {
        seenTables.set(blk.elementPath, blk);
        flatBlocks.push(blk);
      } else {
        const prev = seenTables.get(blk.elementPath) as PreviewTableBlock & { kind: 'table' };
        prev.rows = [...prev.rows, ...blk.rows];
      }
    }
  }

  const patchedBlocks: PreviewBlock[] = flatBlocks.map((b) => {
    const s = sections.find((x) => x.sectionIndex === b.sectionIndex) || sections[0];
    const pW = Math.max(40, s.widthMm - s.marginLeftMm - s.marginRightMm);

    if (b.kind === 'paragraph') {
      const isProt = ['adminHeader', 'docTypeHeading', 'docSubject', 'signature', 'recipient'].includes(b.role);
      const isBody = !isProt && !b.role.startsWith('heading') && b.role !== 'empty';
      const spans = b.spans.map((sp) => ({
        ...sp,
        fontFamily:
          ov.fontName && sp.fontFamily.toLowerCase() !== 'symbol' && sp.fontFamily.toLowerCase() !== 'wingdings'
            ? ov.fontName
            : sp.fontFamily,
        fontSizePt: ov.fontSizePt != null && isBody ? ov.fontSizePt : sp.fontSizePt,
      }));
      const nb: PreviewParagraphBlock & { kind: 'paragraph' } = {
        ...b,
        alignment: isBody && ov.bodyAlignment ? ov.bodyAlignment : b.alignment,
        lineSpacingMult: isBody && ov.lineSpacingMult != null ? ov.lineSpacingMult : b.lineSpacingMult,
        spaceBeforePt: isBody && ov.spaceBeforePt != null ? ov.spaceBeforePt : b.spaceBeforePt,
        spaceAfterPt: isBody && ov.spaceAfterPt != null ? ov.spaceAfterPt : b.spaceAfterPt,
        firstLineIndentMm:
          isBody && ov.firstLineIndentCm != null ? round1(ov.firstLineIndentCm * 10) : b.firstLineIndentMm,
        spans,
        images: b.images.map((im) => ({ ...im, overflowsPrintableArea: im.widthMm > pW + 1.5 })),
      };
      nb.estimatedHeightMm = estimateParaHeightMm(nb, pW);
      return nb;
    }

    const rows = b.rows.map((r) => ({
      ...r,
      cells: r.cells.map((c) => ({
        ...c,
        paragraphs: c.paragraphs.map((p) => {
          const sp = p.spans.map((sItem) => ({
            ...sItem,
            fontFamily: ov.fontName || sItem.fontFamily,
            fontSizePt: b.tableRole === 'dataTable' && ov.tableFontSizePt != null ? ov.tableFontSizePt : sItem.fontSizePt,
          }));
          return { ...p, spans: sp };
        }),
      })),
    }));
    return {
      ...b,
      rows,
      overflowsPrintableWidth: b.totalWidthMm + b.indentLeftMm > pW + 1.5,
    };
  });

  const pages = paginateWebBlocks(sections, patchedBlocks, false);
  return {
    ...baseDoc,
    label: 'Xem trước trực tiếp (Cập nhật tức thì)',
    totalPages: pages.length,
    sections,
    pages,
    printCheck: evaluateWebPrintCheck(pages, sections, scope),
  };
}

export function computeWebBeforeAfterDiffs(
  before: PreviewDocumentRender,
  after: PreviewDocumentRender,
): PreviewElementDiff[] {
  const diffs: PreviewElementDiff[] = [];
  let seq = 0;

  after.sections.forEach((aSec, idx) => {
    const bSec = before.sections[idx];
    if (
      bSec &&
      (Math.abs(bSec.widthMm - aSec.widthMm) > 0.3 ||
        bSec.orientation !== aSec.orientation ||
        Math.abs(bSec.marginTopMm - aSec.marginTopMm) > 0.3 ||
        Math.abs(bSec.marginLeftMm - aSec.marginLeftMm) > 0.3)
    ) {
      diffs.push({
        diffId: `DIFF-${String(++seq).padStart(4, '0')}`,
        elementPath: `section[${aSec.sectionIndex}]`,
        beforePage: before.pages.find((p) => p.sectionIndex === bSec.sectionIndex)?.pageNumber || 1,
        afterPage: after.pages.find((p) => p.sectionIndex === aSec.sectionIndex)?.pageNumber || 1,
        changeKinds: ['pageSetup'],
        summaryVi: `Section ${aSec.sectionIndex}: thay đổi khổ/hướng hoặc lề (${aSec.paperLabel} ${aSec.orientation})`,
        beforeSummary: `${bSec.paperLabel} ${bSec.orientation} · T/D/Tr/P ${bSec.marginTopMm}/${bSec.marginBottomMm}/${bSec.marginLeftMm}/${bSec.marginRightMm}mm`,
        afterSummary: `${aSec.paperLabel} ${aSec.orientation} · T/D/Tr/P ${aSec.marginTopMm}/${aSec.marginBottomMm}/${aSec.marginLeftMm}/${aSec.marginRightMm}mm`,
      });
    }
  });

  const mapOf = (doc: PreviewDocumentRender) => {
    const m = new Map<string, { pg: number; font: string; align: string; sp: string; text: string }>();
    for (const pg of doc.pages) {
      for (const b of pg.blocks) {
        if (b.kind === 'paragraph') {
          const sp0 = b.spans[0];
          m.set(b.elementPath, {
            pg: pg.pageNumber,
            font: sp0 ? `${sp0.fontFamily} ${round1(sp0.fontSizePt)}pt` : 'Normal',
            align: b.alignment,
            sp: `${b.lineSpacingMult}/${b.spaceAfterPt}/${b.firstLineIndentMm}`,
            text: b.plainText.slice(0, 60),
          });
        }
      }
    }
    return m;
  };
  const bMap = mapOf(before);
  const aMap = mapOf(after);
  for (const [ep, a] of aMap.entries()) {
    const b = bMap.get(ep);
    if (!b) continue;
    const kinds: string[] = [];
    if (b.font !== a.font) kinds.push('typography');
    if (b.align !== a.align) kinds.push('alignment');
    if (b.sp !== a.sp) kinds.push('spacing');
    if (b.text !== a.text) kinds.push('content');
    if (b.pg !== a.pg && !kinds.length) kinds.push('pageMoved');
    if (kinds.length) {
      diffs.push({
        diffId: `DIFF-${String(++seq).padStart(4, '0')}`,
        elementPath: ep,
        beforePage: b.pg,
        afterPage: a.pg,
        changeKinds: kinds,
        summaryVi: `Thay đổi [${kinds.join(', ')}] tại ${ep}`,
        beforeSummary: `${b.font} · ${b.align} · ${b.text}`,
        afterSummary: `${a.font} · ${a.align} · ${a.text}`,
      });
    }
  }
  return diffs;
}

export async function buildWebLivePreview(
  item: FileQueueItem,
  req: PreviewBuildRequest,
): Promise<PreviewSessionPayload> {
  const t0 = performance.now();
  cancelledJobs.delete(req.jobId);
  if (!item.webFile) throw new Error('Không có tệp nguồn trên trình duyệt.');

  const srcBuf = await item.webFile.arrayBuffer();
  if (isJobCancelled(req.jobId)) {
    return {
      jobId: req.jobId,
      status: 'cancelled',
      updateMode: 'cancelled',
      elapsedMs: Math.round(performance.now() - t0),
      fidelityMode: 'openxmlMetricLayout',
      fidelityDisclaimer: PREVIEW_FIDELITY_DISCLAIMER_VI,
      wordInstalled: false,
      before: null,
      after: null,
      diffs: [],
      auditMarkers: [],
    };
  }

  const before = await extractWebDocumentRender(srcBuf, 'Bản gốc', req.scope);
  let after: PreviewDocumentRender | null = null;
  let updateMode = 'initial';

  if (req.includeAfter) {
    const ov = req.overrides;
    const needsFullWebTransform =
      Boolean(ov.templateId) ||
      Boolean(ov.applyHeaderMode) ||
      ov.applyStdGroupA ||
      ov.applyVnCleaner ||
      Boolean(req.processConfig);

    if (!needsFullWebTransform && ov.enabled) {
      updateMode = 'incrementalPatch';
      after = applyClientIncrementalPatch(before, ov, req.scope);
    } else if (needsFullWebTransform) {
      updateMode = 'fullRebuild';
      let workingFile: File = item.webFile;
      let workingItem: FileQueueItem = { ...item, webFile: workingFile };

      if (ov.templateId) {
        const org = listWebOrgProfiles().find((o) => o.id === ov.orgProfileId) || null;
        const emptyFields: DynamicTemplateFields = {
          tenCoQuan: org?.issuingOrg || '',
          coQuanChuQuan: org?.governingOrg || '',
          vietTatCoQuan: org?.agencyAbbreviation || '',
          soVanBan: '',
          diaDanh: org?.defaultLocation || 'Hà Nội',
          ngayThang: '',
          day: '',
          month: '',
          year: '',
          nguoiKy: '',
          chucVu: '',
          nguoiNhan: '',
          trichYeu: '',
          extraFields: {},
        };
        const tRes = await applyWebTemplateToDocx(
          {
            templateId: ov.templateId,
            orgProfileId: ov.orgProfileId,
            orgProfileOverride: null,
            targetMode: 'applyToExistingDocx',
            targetPath: item.path,
            destPath: null,
            outputMode: 'newCopy',
            enabledSectionIds: [],
            dynamicFields: emptyFields,
            insertFramingOnExisting: true,
          },
          org,
          workingItem,
        );
        if (tRes.outputBlob) {
          workingFile = new File([tRes.outputBlob], item.name, { type: workingFile.type });
          workingItem = { ...item, webFile: workingFile };
        }
      }

      if (ov.applyHeaderMode && ov.headerConfig) {
        const org = listWebOrgProfiles().find((o) => o.id === (ov.headerConfig?.orgProfileId || ov.orgProfileId)) || null;
        const hRes = await applyWebDocxHeader(workingItem, ov.headerConfig, ov.applyHeaderMode, org);
        if (hRes.outputBlob) {
          workingFile = new File([hRes.outputBlob], item.name, { type: workingFile.type });
          workingItem = { ...item, webFile: workingFile };
        }
      }

      if (ov.applyStdGroupA) {
        const sRes = await executeWebStdDocument(workingItem, {
          path: item.path,
          scope: req.scope,
          orgProfile: listWebOrgProfiles().find((o) => o.id === ov.orgProfileId) || null,
          outputMode: 'newCopy',
          applyAllGroupA: true,
          approvedFindingIds: [],
          skippedFindingIds: [],
          manualReviewFindingIds: [],
          disabledRuleIds: [],
        });
        if (sRes.outputBlob) {
          workingFile = new File([sRes.outputBlob], item.name, { type: workingFile.type });
          workingItem = { ...item, webFile: workingFile };
        }
      }

      const procCfg: ProcessConfig | null =
        req.processConfig ||
        (ov.applyVnCleaner || ov.enabled
          ? {
              ...DEFAULT_ND30_CONFIG,
              format: {
                ...DEFAULT_ND30_CONFIG.format,
                enabled: ov.enabled,
                fontName: ov.fontName || DEFAULT_ND30_CONFIG.format.fontName,
                fontSizePt: ov.fontSizePt ?? DEFAULT_ND30_CONFIG.format.fontSizePt,
                lineSpacing: ov.lineSpacingMult ?? DEFAULT_ND30_CONFIG.format.lineSpacing,
                spaceBeforePt: ov.spaceBeforePt ?? DEFAULT_ND30_CONFIG.format.spaceBeforePt,
                spaceAfterPt: ov.spaceAfterPt ?? DEFAULT_ND30_CONFIG.format.spaceAfterPt,
                firstLineIndentCm: ov.firstLineIndentCm ?? DEFAULT_ND30_CONFIG.format.firstLineIndentCm,
                paperSize: ov.paperSize ?? DEFAULT_ND30_CONFIG.format.paperSize,
                marginTopCm: ov.marginTopCm ?? DEFAULT_ND30_CONFIG.format.marginTopCm,
                marginBottomCm: ov.marginBottomCm ?? DEFAULT_ND30_CONFIG.format.marginBottomCm,
                marginLeftCm: ov.marginLeftCm ?? DEFAULT_ND30_CONFIG.format.marginLeftCm,
                marginRightCm: ov.marginRightCm ?? DEFAULT_ND30_CONFIG.format.marginRightCm,
              },
              cleaner: {
                ...DEFAULT_ND30_CONFIG.cleaner,
                enabled: ov.applyVnCleaner,
              },
            }
          : null);

      if (procCfg) {
        const pRes = await processWebDocxFile(workingItem, procCfg);
        if (pRes.outputBlob) {
          workingFile = new File([pRes.outputBlob], item.name, { type: workingFile.type });
        }
      }

      after = await extractWebDocumentRender(await workingFile.arrayBuffer(), 'Bản sau chuẩn hóa (Tạm)', req.scope);
      if (ov.enabled && ov.bodyAlignment) {
        after = applyClientIncrementalPatch(after, { ...ov, paperSize: null, orientationOverride: null, marginTopCm: null, marginBottomCm: null, marginLeftCm: null, marginRightCm: null }, req.scope);
      }
    }
  }

  let auditMarkers: AuditSyncMarker[] = [];
  if (req.includeAuditSync) {
    try {
      const org = listWebOrgProfiles().find((o) => o.id === req.overrides.orgProfileId) || null;
      const rep = await runWebComplianceAudit(item, req.scope, org, null, [], false);
      const findPg = (ep: string) =>
        before.pages.find((pg) => pg.elementPathsOnPage.some((p) => p === ep || ep.startsWith(p)))?.pageNumber || 1;
      let mSeq = 0;
      auditMarkers = [
        ...rep.traceableFindings.map((f) => ({
          markerId: `AUD-${String(++mSeq).padStart(4, '0')}`,
          source: 'ruleFinding',
          ruleOrIssueId: f.ruleId,
          title: f.ruleName,
          severity: f.severity,
          obligation: String(f.obligation),
          legalBasis: f.legalBasis,
          elementPath: f.elementPath,
          pageNumber: findPg(f.elementPath),
          currentValue: f.currentValue,
          expectedValue: f.expectedValue,
          remediation: f.remediationMethod,
        })),
        ...rep.advancedIssues.map((adv) => ({
          markerId: `AUD-${String(++mSeq).padStart(4, '0')}`,
          source: 'advancedIssue',
          ruleOrIssueId: adv.issueId,
          title: adv.title,
          severity: adv.severity,
          obligation: 'Cảnh báo nâng cao',
          legalBasis: adv.legalBasis,
          elementPath: adv.elementPath,
          pageNumber: findPg(adv.elementPath),
          currentValue: adv.location,
          expectedValue: '',
          remediation: adv.remediation,
        })),
      ];
    } catch {
      auditMarkers = [];
    }
  }

  return {
    jobId: req.jobId,
    status: isJobCancelled(req.jobId) ? 'cancelled' : 'completed',
    updateMode,
    elapsedMs: Math.round(performance.now() - t0),
    fidelityMode: 'openxmlMetricLayout',
    fidelityDisclaimer: PREVIEW_FIDELITY_DISCLAIMER_VI,
    wordInstalled: false,
    before,
    after,
    diffs: after ? computeWebBeforeAfterDiffs(before, after) : [],
    auditMarkers,
  };
}
