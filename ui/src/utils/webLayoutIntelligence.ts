import type {
  AssetInventory,
  DocumentMapNode,
  ElementLocator,
  HeaderFooterMapEntry,
  IntelligentDocumentMap,
  LayoutExecutionRequest,
  LayoutExecutionResult,
  LayoutIntelligenceReport,
  LayoutProposal,
  LayoutWarning,
  PageLayoutEntry,
  ParagraphMapEntry,
  RecognizedStructureKind,
  SectionMapEntry,
  StdDocScope,
  StyleMapEntry,
  TableMapEntry,
  VisualObjectMapEntry,
} from '../types';
import { readZipEntries, writeZipEntries } from './webDocxEngine';

const W_NS = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main';
const R_NS = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';

const P_PR_ORDER = [
  'w:pStyle', 'w:keepNext', 'w:keepLines', 'w:pageBreakBefore', 'w:framePr', 'w:widowControl',
  'w:numPr', 'w:suppressLineNumbers', 'w:pBdr', 'w:shd', 'w:tabs', 'w:suppressAutoHyphens',
  'w:kinsoku', 'w:wordWrap', 'w:overflowPunct', 'w:topLinePunct', 'w:autoSpaceDE', 'w:autoSpaceDN',
  'w:bidi', 'w:adjustRightInd', 'w:snapToGrid', 'w:spacing', 'w:ind', 'w:contextualSpacing',
  'w:mirrorIndents', 'w:suppressOverlap', 'w:jc', 'w:textDirection', 'w:textAlignment',
  'w:textboxTightWrap', 'w:outlineLvl', 'w:divId', 'w:cnfStyle', 'w:rPr', 'w:sectPr',
];

const TBL_PR_ORDER = [
  'w:tblStyle', 'w:tblpPr', 'w:tblOverlap', 'w:bidiVisual', 'w:tblStyleRowBandSize',
  'w:tblStyleColBandSize', 'w:tblW', 'w:jc', 'w:tblCellSpacing', 'w:tblInd', 'w:tblBorders',
  'w:shd', 'w:tblLayout', 'w:tblCellMar', 'w:tblLook',
];

const TR_PR_ORDER = ['w:cnfStyle', 'w:divId', 'w:cantSplit', 'w:trHeight', 'w:tblHeader', 'w:jc'];
const TC_PR_ORDER = ['w:cnfStyle', 'w:tcW', 'w:gridSpan', 'w:hMerge', 'w:vMerge', 'w:tcBorders', 'w:shd', 'w:vAlign'];
const SECT_PR_ORDER = [
  'w:headerReference', 'w:footerReference', 'w:footnotePr', 'w:endnotePr', 'w:type',
  'w:pgSz', 'w:pgMar', 'w:paperSrc', 'w:pgBorders', 'w:lnNumType', 'w:pgNumType',
  'w:cols', 'w:formProt', 'w:vAlign', 'w:noEndnote', 'w:titlePg', 'w:docGrid',
];

const round1 = (v: number): number => Math.round(v * 10) / 10;
const round2 = (v: number): number => Math.round(v * 100) / 100;
const twipsToMm = (t: number): number => round1(t / 56.692913);
const emuToMm = (e: number): number => round1(e / 36000);
const twipsToPt = (t: number): number => round1(t / 20);

const webLayoutSnapshots = new Map<string, File>();

export function registerWebLayoutSnapshot(path: string, file: File): void {
  if (!webLayoutSnapshots.has(path)) {
    webLayoutSnapshots.set(path, file);
  }
}

export function getWebLayoutSnapshot(path: string): File | undefined {
  return webLayoutSnapshots.get(path);
}

function directChildren(parent: Element, localName?: string): Element[] {
  const out: Element[] = [];
  for (let i = 0; i < parent.childNodes.length; i++) {
    const n = parent.childNodes[i];
    if (n.nodeType === 1) {
      const el = n as Element;
      if (!localName || el.localName === localName) {
        out.push(el);
      }
    }
  }
  return out;
}

function firstChild(parent: Element, localName: string): Element | null {
  for (let i = 0; i < parent.childNodes.length; i++) {
    const n = parent.childNodes[i];
    if (n.nodeType === 1 && (n as Element).localName === localName) {
      return n as Element;
    }
  }
  return null;
}

function getAttr(el: Element | null, local: string): string | null {
  if (!el) return null;
  return el.getAttribute(`w:${local}`) ?? el.getAttribute(`r:${local}`) ?? el.getAttribute(local);
}

function setWAttr(el: Element, local: string, val: string): void {
  el.setAttributeNS(W_NS, `w:${local}`, val);
}

function removeWAttr(el: Element, local: string): void {
  el.removeAttributeNS(W_NS, local);
  el.removeAttribute(`w:${local}`);
  el.removeAttribute(local);
}

function ensureWChildOrdered(parent: Element, doc: XMLDocument, tagName: string, order: string[]): Element {
  const local = tagName.includes(':') ? tagName.split(':')[1] : tagName;
  const existing = firstChild(parent, local);
  if (existing) return existing;

  const newEl = doc.createElementNS(W_NS, tagName);
  const targetRank = order.indexOf(tagName);
  const rank = targetRank === -1 ? order.length : targetRank;

  for (let i = 0; i < parent.childNodes.length; i++) {
    const n = parent.childNodes[i];
    if (n.nodeType === 1) {
      const childTag = `w:${(n as Element).localName}`;
      const r = order.indexOf(childTag);
      if ((r === -1 ? order.length : r) > rank) {
        parent.insertBefore(newEl, n);
        return newEl;
      }
    }
  }
  parent.appendChild(newEl);
  return newEl;
}

function collectParagraphText(el: Element): string {
  let out = '';
  const walk = (cur: Element) => {
    const ln = cur.localName;
    if (
      ln === 'drawing' || ln === 'pict' || ln === 'AlternateContent' ||
      ln === 'instrText' || ln === 'delText' || ln === 'footnoteReference' || ln === 'endnoteReference'
    ) {
      return;
    }
    if (ln === 'tab') {
      out += '\t';
      return;
    }
    if (ln === 'br' || ln === 'cr') {
      out += ' ';
      return;
    }
    for (let i = 0; i < cur.childNodes.length; i++) {
      const n = cur.childNodes[i];
      if (n.nodeType === 3) {
        out += n.textContent ?? '';
      } else if (n.nodeType === 1) {
        walk(n as Element);
      }
    }
  };
  walk(el);
  return out;
}

function collectAllNonEmptyTexts(root: Element): string[] {
  const out: string[] = [];
  const walk = (el: Element) => {
    if (el.localName === 'AlternateContent') {
      const choice = firstChild(el, 'Choice');
      if (choice) walk(choice);
      else {
        const fb = firstChild(el, 'Fallback');
        if (fb) walk(fb);
      }
      return;
    }
    if (el.localName === 'p') {
      const t = collectParagraphText(el).trim();
      if (t) out.push(t);
      for (const ch of directChildren(el)) {
        if (['AlternateContent', 'drawing', 'pict', 'r', 'sdt', 'sdtContent'].includes(ch.localName)) {
          walk(ch);
        }
      }
      return;
    }
    for (const ch of directChildren(el)) {
      walk(ch);
    }
  };
  walk(root);
  return out;
}

function excerptStr(s: string, maxLen: number): string {
  const tr = s.trim();
  return tr.length <= maxLen ? tr : `${tr.slice(0, maxLen - 1)}…`;
}

function readRprFont(rPr: Element | null): string | null {
  if (!rPr) return null;
  const rf = firstChild(rPr, 'rFonts');
  return getAttr(rf, 'ascii') ?? getAttr(rf, 'hAnsi') ?? getAttr(rf, 'cs');
}

function readRprSzPt(rPr: Element | null): number | null {
  if (!rPr) return null;
  const sz = firstChild(rPr, 'sz') ?? firstChild(rPr, 'szCs');
  const v = getAttr(sz, 'val');
  if (!v) return null;
  const n = Number(v);
  return Number.isFinite(n) ? n / 2 : null;
}

interface RawVisual {
  isTextBox: boolean;
  anchorMode: string;
  relId: string | null;
  widthMm: number;
  heightMm: number;
  innerParagraphs: Element[];
}

function extractVisualsDedup(el: Element, out: RawVisual[]): void {
  const ln = el.localName;
  if (ln === 'AlternateContent') {
    const choice = firstChild(el, 'Choice');
    if (choice) extractVisualsDedup(choice, out);
    else {
      const fb = firstChild(el, 'Fallback');
      if (fb) extractVisualsDedup(fb, out);
    }
    return;
  }
  if (ln === 'drawing') {
    const inline = firstChild(el, 'inline');
    const anchor = firstChild(el, 'anchor');
    const mode = inline ? 'inline' : anchor ? 'floating' : 'inline';
    const ext = firstChild(inline ?? anchor ?? el, 'extent');
    const cx = Number(ext?.getAttribute('cx') ?? '0') || 0;
    const cy = Number(ext?.getAttribute('cy') ?? '0') || 0;
    const wMm = emuToMm(cx);
    const hMm = emuToMm(cy);

    const txbxParas: Element[] = [];
    const blips: string[] = [];
    const walkSub = (cur: Element) => {
      if (cur.localName === 'txbxContent') {
        for (const c of directChildren(cur)) {
          if (c.localName === 'p') txbxParas.push(c);
          else if (c.localName === 'tbl') {
            for (const tr of directChildren(c, 'tr')) {
              for (const tc of directChildren(tr, 'tc')) {
                for (const tp of directChildren(tc, 'p')) txbxParas.push(tp);
              }
            }
          }
        }
        return;
      }
      if (cur.localName === 'blip') {
        const rid = cur.getAttributeNS(R_NS, 'embed') || cur.getAttribute('r:embed') || cur.getAttributeNS(R_NS, 'link') || cur.getAttribute('r:link');
        if (rid) blips.push(rid);
      }
      for (const ch of directChildren(cur)) walkSub(ch);
    };
    walkSub(el);

    if (txbxParas.length > 0) {
      out.push({
        isTextBox: true,
        anchorMode: mode,
        relId: null,
        widthMm: wMm > 0 ? wMm : 80,
        heightMm: hMm > 0 ? hMm : 25,
        innerParagraphs: txbxParas,
      });
    }
    for (const rid of blips) {
      out.push({ isTextBox: false, anchorMode: mode, relId: rid, widthMm: wMm, heightMm: hMm, innerParagraphs: [] });
    }
    return;
  }
  if (ln === 'pict') {
    const txbxParas: Element[] = [];
    const vmlRids: string[] = [];
    let wMm = 0;
    let hMm = 0;
    const walkPict = (cur: Element) => {
      const st = cur.getAttribute('style');
      if (st && wMm === 0) {
        const wm = st.match(/width\s*:\s*([\d.]+)(pt|in|cm|mm|px)/i);
        const hm = st.match(/height\s*:\s*([\d.]+)(pt|in|cm|mm|px)/i);
        const toMm = (n: number, u: string) =>
          u === 'pt' ? round1((n * 25.4) / 72) : u === 'in' ? round1(n * 25.4) : u === 'cm' ? round1(n * 10) : u === 'px' ? round1((n * 25.4) / 96) : round1(n);
        if (wm) wMm = toMm(Number(wm[1]), wm[2].toLowerCase());
        if (hm) hMm = toMm(Number(hm[1]), hm[2].toLowerCase());
      }
      if (cur.localName === 'txbxContent') {
        for (const p of directChildren(cur, 'p')) txbxParas.push(p);
        return;
      }
      if (cur.localName === 'imagedata') {
        const rid = cur.getAttributeNS(R_NS, 'id') || cur.getAttribute('r:id') || cur.getAttribute('o:relid');
        if (rid) vmlRids.push(rid);
      }
      for (const ch of directChildren(cur)) walkPict(ch);
    };
    walkPict(el);
    if (txbxParas.length > 0) {
      out.push({ isTextBox: true, anchorMode: 'vml', relId: null, widthMm: wMm || 80, heightMm: hMm || 25, innerParagraphs: txbxParas });
    } else {
      for (const rid of vmlRids) {
        out.push({ isTextBox: false, anchorMode: 'vml', relId: rid, widthMm: wMm, heightMm: hMm, innerParagraphs: [] });
      }
    }
    return;
  }
  for (const ch of directChildren(el)) extractVisualsDedup(ch, out);
}

const DOC_TYPE_TITLES = new Set([
  'QUYẾT ĐỊNH', 'NGHỊ QUYẾT', 'CHỈ THỊ', 'QUY CHẾ', 'QUY ĐỊNH', 'THÔNG CÁO', 'THÔNG BÁO',
  'HƯỚNG DẪN', 'CHƯƠNG TRÌNH', 'KẾ HOẠCH', 'PHƯƠNG ÁN', 'ĐỀ ÁN', 'DỰ ÁN', 'BÁO CÁO',
  'BIÊN BẢN', 'TỜ TRÌNH', 'HỢP ĐỒNG', 'CÔNG ĐIỆN', 'BẢN GHI NHỚ', 'BẢN THỎA THUẬN',
  'GIẤY ỦY QUYỀN', 'GIẤY MỜI', 'GIẤY GIỚI THIỆU', 'PHIẾU GỬI', 'PHIẾU CHUYỂN', 'PHIẾU BÁO',
]);

function isRemovableEmptyWParagraph(p: Element): boolean {
  if (collectParagraphText(p).trim().length > 0) return false;
  let keep = false;
  const scan = (el: Element) => {
    if (['drawing', 'pict', 'AlternateContent', 'sectPr', 'fldSimple', 'instrText', 'bookmarkStart', 'object'].includes(el.localName)) {
      keep = true;
      return;
    }
    if (el.localName === 'br' && el.getAttribute('w:type') === 'page') {
      keep = true;
      return;
    }
    for (const c of directChildren(el)) scan(c);
  };
  scan(p);
  return !keep;
}

export async function analyzeWebLayoutIntelligence(
  file: File,
  scope: StdDocScope = 'autoDetect',
): Promise<LayoutIntelligenceReport> {
  const entriesMap = await readZipEntries(await file.arrayBuffer());
  const entriesList = Array.from(entriesMap.entries()).map(([name, data]) => ({ name, data }));
  const decoder = new TextDecoder('utf-8');
  const parser = new DOMParser();

  const findEntry = (target: string) =>
    entriesList.find((e) => e.name.replace(/^\/+/, '').replace(/\\/g, '/').toLowerCase() === target.toLowerCase());
  const docEntry = findEntry('word/document.xml');
  if (!docEntry) {
    throw new Error('Tệp DOCX thiếu phần bắt buộc word/document.xml.');
  }

  const docXmlDoc = parser.parseFromString(decoder.decode(docEntry.data).replace(/^\uFEFF/, ''), 'application/xml');
  const docRoot = docXmlDoc.documentElement;
  const body = firstChild(docRoot, 'body');
  if (!body) {
    throw new Error('word/document.xml không có thẻ w:body.');
  }

  // Rel map
  const relsMap = new Map<string, { type: string; target: string }>();
  const relEntry = findEntry('word/_rels/document.xml.rels');
  if (relEntry) {
    const rDoc = parser.parseFromString(decoder.decode(relEntry.data).replace(/^\uFEFF/, ''), 'application/xml');
    for (const rel of directChildren(rDoc.documentElement, 'Relationship')) {
      const id = rel.getAttribute('Id');
      if (id) {
        relsMap.set(id, { type: rel.getAttribute('Type') ?? '', target: rel.getAttribute('Target') ?? '' });
      }
    }
  }

  // Numbering lookup
  const numToAbs = new Map<string, string>();
  const numLvls = new Map<string, string>();
  const numEntry = findEntry('word/numbering.xml');
  if (numEntry) {
    const nDoc = parser.parseFromString(decoder.decode(numEntry.data).replace(/^\uFEFF/, ''), 'application/xml');
    for (const el of directChildren(nDoc.documentElement)) {
      if (el.localName === 'abstractNum') {
        const absId = getAttr(el, 'abstractNumId') ?? '';
        for (const lvl of directChildren(el, 'lvl')) {
          const ilvl = getAttr(lvl, 'ilvl') ?? '0';
          const fmt = getAttr(firstChild(lvl, 'numFmt'), 'val') ?? 'decimal';
          numLvls.set(`${absId}:${ilvl}`, fmt);
        }
      } else if (el.localName === 'num') {
        const numId = getAttr(el, 'numId') ?? '';
        const absId = getAttr(firstChild(el, 'abstractNumId'), 'val') ?? '';
        if (numId && absId) numToAbs.set(numId, absId);
      }
    }
  }

  // Styles metadata
  const styleMap: StyleMapEntry[] = [];
  const styleById = new Map<string, { font?: string; sizePt?: number; jc?: string; outline?: number; widow?: boolean }>();
  const styleEntry = findEntry('word/styles.xml');
  if (styleEntry) {
    const sDoc = parser.parseFromString(decoder.decode(styleEntry.data).replace(/^\uFEFF/, ''), 'application/xml');
    for (const st of directChildren(sDoc.documentElement, 'style')) {
      const id = getAttr(st, 'styleId');
      if (!id) continue;
      const sType = getAttr(st, 'type') ?? 'paragraph';
      const isDefault = getAttr(st, 'default') === '1';
      const sName = getAttr(firstChild(st, 'name'), 'val') ?? id;
      const basedOn = getAttr(firstChild(st, 'basedOn'), 'val');
      const rPr = firstChild(st, 'rPr');
      const pPr = firstChild(st, 'pPr');
      const font = readRprFont(rPr) ?? undefined;
      const sizePt = readRprSzPt(rPr) ?? undefined;
      const jc = getAttr(firstChild(pPr ?? st, 'jc'), 'val') ?? undefined;
      const olRaw = pPr ? getAttr(firstChild(pPr, 'outlineLvl'), 'val') : null;
      const outline = olRaw !== null && olRaw !== undefined ? Number(olRaw) : undefined;
      const wcEl = pPr ? firstChild(pPr, 'widowControl') : null;
      const wc = wcEl ? getAttr(wcEl, 'val') !== '0' && getAttr(wcEl, 'val') !== 'false' : undefined;
      styleById.set(id, { font, sizePt, jc: jc ?? undefined, outline, widow: wc });
      styleMap.push({
        styleId: id,
        styleName: sName,
        styleType: sType,
        basedOn,
        isDefault,
        fontName: font ?? null,
        fontSizePt: sizePt ?? null,
        alignment: jc ?? null,
        outlineLevel: outline ?? null,
        usageCount: 0,
        directConflictCount: 0,
      });
    }
  }

  // Sections
  const parseSect = (sect: Element, idx: number) => {
    const pgSz = firstChild(sect, 'pgSz');
    const pgW = Number(getAttr(pgSz, 'w') ?? '11906') || 11906;
    const pgH = Number(getAttr(pgSz, 'h') ?? '16838') || 16838;
    const orientation = getAttr(pgSz, 'orient') ?? (pgW > pgH ? 'landscape' : 'portrait');
    const mar = firstChild(sect, 'pgMar');
    const top = Number(getAttr(mar, 'top') ?? '1134') || 1134;
    const bottom = Number(getAttr(mar, 'bottom') ?? '1134') || 1134;
    const left = Number(getAttr(mar, 'left') ?? '1701') || 1701;
    const right = Number(getAttr(mar, 'right') ?? '850') || 850;
    const printableW = Math.max(2000, pgW - left - right);
    const printableH = Math.max(3000, pgH - top - bottom);
    const breakType = getAttr(firstChild(sect, 'type'), 'val') ?? 'nextPage';

    const cols = firstChild(sect, 'cols');
    const colCount = Math.max(1, Number(getAttr(cols, 'num') ?? '1') || 1);
    const colSpace = Number(getAttr(cols, 'space') ?? '720') || 720;
    const eqWidth = cols ? getAttr(cols, 'equalWidth') !== '0' : true;
    let colWidths = cols
      ? directChildren(cols, 'col').map((c) => Number(getAttr(c, 'w') ?? '0')).filter((w) => w > 0)
      : [];
    if (colWidths.length === 0) {
      if (colCount > 1) {
        const each = Math.max(720, Math.floor((printableW - (colCount - 1) * colSpace) / colCount));
        colWidths = Array.from({ length: colCount }, () => each);
      } else {
        colWidths = [printableW];
      }
    }

    const hRefs: { rid: string; btype: string }[] = [];
    const fRefs: { rid: string; btype: string }[] = [];
    for (const ch of directChildren(sect)) {
      if (ch.localName === 'headerReference') {
        const rid = getAttr(ch, 'id');
        if (rid) hRefs.push({ rid, btype: getAttr(ch, 'type') ?? 'default' });
      } else if (ch.localName === 'footerReference') {
        const rid = getAttr(ch, 'id');
        if (rid) fRefs.push({ rid, btype: getAttr(ch, 'type') ?? 'default' });
      }
    }

    return {
      sectionId: `sec-${idx}`,
      sectionIndex: idx,
      breakType,
      orientation,
      paperWidthMm: round1(pgW / 56.6929),
      paperHeightMm: round1(pgH / 56.6929),
      marginTopMm: round1(top / 56.6929),
      marginBottomMm: round1(bottom / 56.6929),
      marginLeftMm: round1(left / 56.6929),
      marginRightMm: round1(right / 56.6929),
      printableWidthMm: twipsToMm(printableW),
      printableHeightMm: twipsToMm(printableH),
      printableWidthTwips: printableW,
      columnCount: colCount,
      columnSpaceMm: twipsToMm(colSpace),
      equalColumnWidth: eqWidth,
      colWidthsMm: colWidths.map(twipsToMm),
      hRefs,
      fRefs,
    };
  };

  const bodyChildren = directChildren(body);
  const inlineSectParaIdx = new Map<number, number>();
  const rawSections: ReturnType<typeof parseSect>[] = [];
  bodyChildren.forEach((el, bIdx) => {
    if (el.localName === 'p') {
      const sect = firstChild(firstChild(el, 'pPr') ?? el, 'sectPr');
      if (sect && firstChild(el, 'pPr')?.contains(sect)) {
        const sIdx = rawSections.length + 1;
        inlineSectParaIdx.set(bIdx, sIdx);
        rawSections.push(parseSect(sect, sIdx));
      }
    }
  });
  const trailingSect = firstChild(body, 'sectPr');
  if (trailingSect) {
    rawSections.push(parseSect(trailingSect, rawSections.length + 1));
  }
  if (rawSections.length === 0) {
    rawSections.push(parseSect(docXmlDoc.createElementNS(W_NS, 'w:sectPr'), 1));
  }

  // Header/Footer map
  const partBindSections = new Map<string, { secs: number[]; types: string[] }>();
  rawSections.forEach((s) => {
    for (const hr of [...s.hRefs, ...s.fRefs]) {
      const rel = relsMap.get(hr.rid);
      if (rel) {
        const pn = rel.target.replace(/^\/+/, '').replace(/\\/g, '/');
        const full = pn.toLowerCase().startsWith('word/') ? pn.toLowerCase() : `word/${pn.toLowerCase()}`;
        const cur = partBindSections.get(full) ?? { secs: [], types: [] };
        if (!cur.secs.includes(s.sectionIndex)) cur.secs.push(s.sectionIndex);
        if (!cur.types.includes(hr.btype)) cur.types.push(hr.btype);
        partBindSections.set(full, cur);
      }
    }
  });

  const hfEntries = entriesList
    .filter((e) => {
      const ln = e.name.replace(/^\/+/, '').replace(/\\/g, '/').toLowerCase();
      return (ln.startsWith('word/header') || ln.startsWith('word/footer')) && ln.endsWith('.xml');
    })
    .sort((a, b) => a.name.localeCompare(b.name));

  const headerFooterMap: HeaderFooterMapEntry[] = [];
  const docNodes: DocumentMapNode[] = [];
  const secHeaderIds = new Map<number, string[]>();
  const secFooterIds = new Map<number, string[]>();
  let nodeSeq = 0;

  hfEntries.forEach((e, idx) => {
    const partName = e.name.replace(/^\/+/, '').replace(/\\/g, '/');
    const hfKind = partName.toLowerCase().includes('header') ? 'header' : 'footer';
    const xmlStr = decoder.decode(e.data).replace(/^\uFEFF/, '');
    const hfDoc = parser.parseFromString(xmlStr, 'application/xml');
    const paras = directChildren(hfDoc.documentElement, 'p');
    const tbls = directChildren(hfDoc.documentElement, 'tbl');
    const vis: RawVisual[] = [];
    extractVisualsDedup(hfDoc.documentElement, vis);
    const preview = excerptStr(paras.map(collectParagraphText).join(' '), 90);
    const upXml = xmlStr.toUpperCase();
    const bind = partBindSections.get(partName.toLowerCase()) ?? { secs: [], types: ['unlinked'] };
    const hfId = `hf-${idx + 1}`;
    for (const secN of bind.secs) {
      const m = hfKind === 'header' ? secHeaderIds : secFooterIds;
      const arr = m.get(secN) ?? [];
      arr.push(hfId);
      m.set(secN, arr);
    }
    const loc: ElementLocator = {
      partName,
      elementPath: `${hfKind}[${idx}]`,
      bodyChildIndex: null,
      sectionIndex: bind.secs[0] ?? 1,
      estimatedPageStart: 1,
      estimatedPageEnd: 1,
      columnIndex: 0,
      xLeftMm: 0,
      yTopMm: hfKind === 'header' ? 10 : 282,
      widthMm: 165,
      heightMm: 12,
    };
    headerFooterMap.push({
      hfId,
      partName,
      hfKind,
      bindTypes: bind.types,
      linkedSections: bind.secs,
      paragraphCount: paras.length,
      tableCount: tbls.length,
      hasPageField: upXml.includes('PAGE'),
      hasNumpagesField: upXml.includes('NUMPAGES'),
      hasImages: vis.length > 0,
      textPreview: preview,
      locator: loc,
    });
    nodeSeq += 1;
    docNodes.push({
      nodeId: hfId,
      parentId: null,
      childrenIds: [],
      kind: 'headerFooter',
      label: `${hfKind === 'header' ? 'Header' : 'Footer'} (${partName.replace(/^word\//i, '')})`,
      excerpt: preview || `Liên kết Section ${bind.secs.join(', ')}`,
      level: 0,
      confidence: 0.99,
      signals: [`part:${partName}`],
      locator: loc,
    });
  });

  // Walk body elements
  const paragraphMap: ParagraphMapEntry[] = [];
  const tableMap: TableMapEntry[] = [];
  const visualObjects: VisualObjectMapEntry[] = [];
  const pageBuckets = new Map<number, PageLayoutEntry>();
  const ensureBucket = (pageNo: number, secIdx: number) => {
    if (!pageBuckets.has(pageNo)) {
      const s = rawSections[secIdx - 1] ?? rawSections[0];
      pageBuckets.set(pageNo, {
        pageId: `page-${pageNo}`,
        pageNumber: pageNo,
        sectionIndex: secIdx,
        orientation: s.orientation,
        widthMm: s.paperWidthMm,
        heightMm: s.paperHeightMm,
        printableWidthMm: s.printableWidthMm,
        printableHeightMm: s.printableHeightMm,
        usedHeightMm: 0,
        fillRatioPct: 0,
        columnCount: s.columnCount,
        elementIds: [],
        hasOverflow: false,
        hasOrphanOrStrandedHeading: false,
        hasAbnormalWhitespace: false,
      });
    }
  };
  ensureBucket(1, 1);

  const secRanges = new Map<number, [number, number]>();
  secRanges.set(1, [1, 1]);

  let currentSec = 1;
  let currentPage = 1;
  let curCol = 0;
  let curYMm = 0;
  let seenHeaderOrTitle = false;
  let seenDocType = false;
  let seenFirstHeading = false;
  let seenSigZone = false;
  let bodyNonEmpty = 0;
  let prevAfterPt = 0;
  let bodyNodeInserted = false;
  let visualSeq = 0;
  let txbxSeq = 0;
  let tblIdx = 0;
  const hierarchyStack: { lvl: number; id: string }[] = [];
  let curAppendixId: string | null = null;

  bodyChildren.forEach((el, bIdx) => {
    const sec = rawSections[currentSec - 1] ?? rawSections[0];
    if (el.localName === 'p') {
      const rawText = collectParagraphText(el);
      const trimmed = rawText.trim();
      const upper = trimmed.replace(/\s+/g, ' ').toUpperCase();
      if (trimmed) bodyNonEmpty += 1;

      const pPr = firstChild(el, 'pPr');
      const styleId = getAttr(firstChild(pPr ?? el, 'pStyle'), 'val');
      if (styleId) {
        const sm = styleMap.find((s) => s.styleId === styleId);
        if (sm) sm.usageCount += 1;
      }
      const stMeta = styleId ? styleById.get(styleId) : undefined;
      const directJc = pPr ? getAttr(firstChild(pPr, 'jc'), 'val') : null;
      const alignment = directJc ?? stMeta?.jc ?? 'left';

      const ind = pPr ? firstChild(pPr, 'ind') : null;
      const indL = Number(getAttr(ind, 'left') ?? '0') || 0;
      const indR = Number(getAttr(ind, 'right') ?? '0') || 0;
      const indFl = Number(getAttr(ind, 'firstLine') ?? '0') || 0;

      const sp = pPr ? firstChild(pPr, 'spacing') : null;
      const spB = Number(getAttr(sp, 'before') ?? '0') || 0;
      const spA = Number(getAttr(sp, 'after') ?? '0') || 0;
      const spL = Number(getAttr(sp, 'line') ?? '240') || 240;

      const wcEl = pPr ? firstChild(pPr, 'widowControl') : null;
      const widowControl = wcEl ? getAttr(wcEl, 'val') !== '0' : stMeta?.widow ?? true;
      const keepNext = Boolean(pPr && firstChild(pPr, 'keepNext') && getAttr(firstChild(pPr, 'keepNext'), 'val') !== '0');
      const keepLines = Boolean(pPr && firstChild(pPr, 'keepLines') && getAttr(firstChild(pPr, 'keepLines'), 'val') !== '0');
      const pageBreakBefore = Boolean(pPr && firstChild(pPr, 'pageBreakBefore'));
      if (pageBreakBefore && curYMm > 2) {
        currentPage += 1;
        curCol = 0;
        curYMm = 0;
        ensureBucket(currentPage, currentSec);
      }

      const numPr = pPr ? firstChild(pPr, 'numPr') : null;
      const numId = numPr ? getAttr(firstChild(numPr, 'numId'), 'val') : null;
      const numIlvl = numPr ? getAttr(firstChild(numPr, 'ilvl'), 'val') ?? '0' : '0';

      const runs = directChildren(el, 'r');
      let primaryFont = stMeta?.font ?? 'Times New Roman';
      let primarySz: number | null = stMeta?.sizePt ?? null;
      let isBold = false;
      for (const r of runs) {
        const rrPr = firstChild(r, 'rPr');
        const rf = readRprFont(rrPr);
        if (rf) primaryFont = rf;
        const rsz = readRprSzPt(rrPr);
        if (rsz) primarySz = rsz;
        const b = firstChild(rrPr ?? r, 'b');
        if (b && getAttr(b, 'val') !== '0') isBold = true;
        if (rf || rsz) break;
      }

      const olAttr = pPr ? getAttr(firstChild(pPr, 'outlineLvl'), 'val') : null;
      const outlineLvl = olAttr !== null ? Number(olAttr) : stMeta?.outline;

      // Classify structure
      let kind: RecognizedStructureKind = 'mainBody';
      const signals: string[] = [];
      let confidence = 0.9;
      if (trimmed) {
        const isCaption = /^(Bảng|Hình|Biểu\s*đồ|Sơ\s*đồ)\s+\d+/i.test(trimmed);
        if (!seenDocType && (upper.includes('CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM') || upper.includes('ĐỘC LẬP - TỰ DO')) && bodyNonEmpty <= 10) {
          kind = 'documentHeader';
          seenHeaderOrTitle = true;
          signals.push('zone:top_of_document');
        } else if (!seenDocType && DOC_TYPE_TITLES.has(upper) && (alignment === 'center' || isBold) && bodyNonEmpty <= 14) {
          kind = 'docTypeTitle';
          seenHeaderOrTitle = true;
          seenDocType = true;
          signals.push('zone:pre_body', `align:${alignment}`);
        } else if ((seenDocType && !seenFirstHeading && bodyNonEmpty <= 12) && (upper.startsWith('V/V') || upper.startsWith('VỀ VIỆC'))) {
          kind = 'docSubject';
          signals.push('context:follows_doc_type');
        } else if (!seenFirstHeading && !seenHeaderOrTitle && bodyNonEmpty <= 4 && trimmed.length <= 180 && !trimmed.endsWith('.') &&
          ((styleId ?? '').toLowerCase() === 'title' || (alignment === 'center' && (isBold || (primarySz ?? 13) >= 15)))) {
          kind = 'mainTitle';
          seenHeaderOrTitle = true;
          signals.push('zone:opening_block');
        } else if (upper.startsWith('NƠI NHẬN') || (seenSigZone && trimmed.startsWith('-'))) {
          kind = 'recipientBlock';
          seenSigZone = true;
          signals.push('context:closing_zone');
        } else if (/^(TM\.|KT\.|TL\.|TUQ\.|CHỦ TỊCH|GIÁM ĐỐC|TỔNG GIÁM ĐỐC|THỦ TRƯỞNG)/.test(upper) || (seenSigZone && trimmed.length <= 60)) {
          kind = 'signatureBlock';
          seenSigZone = true;
          signals.push('context:closing_zone');
        } else if (/^PHỤ\s+LỤC(?:\s|$|:|\.)/i.test(trimmed) && trimmed.length <= 120 && !trimmed.endsWith('.') && (seenSigZone || pageBreakBefore || alignment === 'center' || isBold)) {
          kind = 'appendixBlock';
          seenFirstHeading = true;
          signals.push(seenSigZone ? 'context:after_signature_zone' : 'format:appendix_head');
        } else if (!isCaption) {
          let hLvl: number | undefined;
          if (outlineLvl !== undefined && outlineLvl >= 0 && outlineLvl <= 5) {
            hLvl = Math.min(4, outlineLvl + 1);
            signals.push(`openxml:outlineLvl=${outlineLvl}`);
          } else if (styleId && /^heading\s*[1-4]/i.test(styleId)) {
            hLvl = Number(styleId.replace(/\D+/g, '')) || 1;
            signals.push(`openxml:pStyle=${styleId}`);
          } else if (numId) {
            const absId = numToAbs.get(numId);
            const fmt = absId ? numLvls.get(`${absId}:${numIlvl}`) : undefined;
            if (fmt && fmt !== 'bullet' && (isBold || (primarySz ?? 13) > 13.5) && trimmed.length <= 130 && !trimmed.endsWith('.')) {
              hLvl = Math.min(4, Number(numIlvl) + 1);
              signals.push(`openxml:numbering(${fmt})`);
            }
          }
          if (!hLvl && trimmed.length <= 130 && !trimmed.endsWith('.') && !trimmed.endsWith(';')) {
            if (/^(PHẦN|CHƯƠNG|[IVXLCDM]+\.)\s+/i.test(trimmed) && (isBold || alignment === 'center')) {
              hLvl = 1;
              signals.push('structure:chapter_or_roman');
            } else if (/^Điều\s+\d+[:.]/i.test(trimmed)) {
              hLvl = 2;
              signals.push('structure:dieu');
            } else {
              const dec = trimmed.match(/^(\d+(?:\.\d+){0,3})\.?\s+\S+/);
              if (dec && (isBold || dec[1].includes('.'))) {
                hLvl = Math.min(4, dec[1].split('.').length);
                signals.push(`structure:decimal_${hLvl}`);
              }
            }
          }
          if (hLvl) {
            kind = (`headingLevel${hLvl}`) as RecognizedStructureKind;
            seenFirstHeading = true;
            confidence = 0.94;
          } else {
            signals.push('context:prose_flow');
          }
        } else {
          signals.push('context:caption');
        }
      }

      const visuals: RawVisual[] = [];
      extractVisualsDedup(el, visuals);

      const fontPt = primarySz ?? 13;
      const colWMm = sec.colWidthsMm[curCol] ?? sec.printableWidthMm;
      const usableW = Math.max(20, colWMm - twipsToMm(indL) - twipsToMm(indR));
      const estLines = trimmed ? Math.max(1, Math.ceil(trimmed.length / Math.max(12, usableW / (fontPt * 0.175)))) : 1;
      const spBeforePt = twipsToPt(spB);
      const spAfterPt = twipsToPt(spA);
      const lineMult = round2(Math.max(0.8, spL / 240));
      const inlineImgH = visuals.filter((v) => !v.isTextBox && v.anchorMode === 'inline').reduce((m, v) => Math.max(m, v.heightMm), 0);
      const paraH = Math.max(4.5, round1((spBeforePt + spAfterPt) * 0.3528 + estLines * (fontPt * 0.3528 * Math.max(1.05, lineMult)) + inlineImgH));

      if (curYMm > 0 && curYMm + paraH > sec.printableHeightMm + 1) {
        if (curCol + 1 < sec.columnCount) {
          curCol += 1;
          curYMm = 0;
        } else {
          currentPage += 1;
          curCol = 0;
          curYMm = 0;
          ensureBucket(currentPage, currentSec);
        }
      }

      const pId = `p-${paragraphMap.length}`;
      const loc: ElementLocator = {
        partName: 'word/document.xml',
        elementPath: `body/p[${bIdx}]`,
        bodyChildIndex: bIdx,
        sectionIndex: currentSec,
        estimatedPageStart: currentPage,
        estimatedPageEnd: currentPage,
        columnIndex: curCol,
        xLeftMm: round1(sec.marginLeftMm + curCol * (colWMm + sec.columnSpaceMm) + twipsToMm(indL)),
        yTopMm: round1(sec.marginTopMm + curYMm),
        widthMm: round1(usableW),
        heightMm: paraH,
      };

      paragraphMap.push({
        paraId: pId,
        globalIndex: paragraphMap.length,
        kind,
        roleLabel: kind,
        excerpt: excerptStr(trimmed, 120),
        styleId,
        fontName: primaryFont,
        fontSizePt: primarySz,
        alignment,
        indentLeftMm: twipsToMm(indL),
        indentRightMm: twipsToMm(indR),
        indentFirstLineMm: twipsToMm(indFl),
        spaceBeforePt: spBeforePt,
        spaceAfterPt: spAfterPt,
        lineSpacingMult: lineMult,
        widowControl,
        keepNext,
        keepLines,
        pageBreakBefore,
        inTableId: null,
        inTextBoxId: null,
        estimatedLines: estLines,
        gapFromPrevPt: round1(prevAfterPt + spBeforePt),
        locator: loc,
      });
      prevAfterPt = spAfterPt;

      const bk = pageBuckets.get(currentPage);
      if (bk) {
        bk.elementIds.push(pId);
        bk.usedHeightMm = round1(Math.min(sec.printableHeightMm * 1.2, curYMm + paraH));
      }
      const sr = secRanges.get(currentSec);
      if (sr) sr[1] = Math.max(sr[1], currentPage);

      const shouldAddNode = kind === 'mainBody' ? (!bodyNodeInserted && Boolean(trimmed) && (bodyNodeInserted = true)) : Boolean(trimmed);
      if (shouldAddNode) {
        nodeSeq += 1;
        const nId = `node-${nodeSeq}`;
        const hLevel = kind.startsWith('headingLevel') ? Number(kind.slice(-1)) : 0;
        let parentId: string | null = null;
        if (kind === 'appendixBlock') {
          hierarchyStack.length = 0;
          curAppendixId = nId;
        } else if (hLevel > 0) {
          while (hierarchyStack.length > 0 && hierarchyStack[hierarchyStack.length - 1].lvl >= hLevel) {
            hierarchyStack.pop();
          }
          parentId = hierarchyStack.length > 0 ? hierarchyStack[hierarchyStack.length - 1].id : curAppendixId;
          hierarchyStack.push({ lvl: hLevel, id: nId });
        } else if (!['documentHeader', 'docTypeTitle', 'docSubject', 'mainTitle', 'signatureBlock', 'recipientBlock'].includes(kind)) {
          parentId = hierarchyStack.length > 0 ? hierarchyStack[hierarchyStack.length - 1].id : curAppendixId;
        }
        docNodes.push({
          nodeId: nId,
          parentId,
          childrenIds: [],
          kind,
          label: labelForKindVi(kind),
          excerpt: excerptStr(trimmed, 96),
          level: hLevel,
          confidence,
          signals,
          locator: loc,
        });
      }

      // Register visuals
      const visParent = hierarchyStack.length > 0 ? hierarchyStack[hierarchyStack.length - 1].id : curAppendixId;
      for (const vis of visuals) {
        if (vis.isTextBox) {
          txbxSeq += 1;
          const tId = `txbx-${txbxSeq}`;
          const innerTexts: string[] = [];
          vis.innerParagraphs.forEach((ip, ipIdx) => {
            const it = collectParagraphText(ip).trim();
            if (it) innerTexts.push(it);
            paragraphMap.push({
              paraId: `txbx-${txbxSeq}-p-${ipIdx + 1}`,
              globalIndex: paragraphMap.length,
              kind: 'textBox',
              roleLabel: 'Đoạn trong Text Box',
              excerpt: excerptStr(it, 120),
              styleId: null,
              fontName: 'Times New Roman',
              fontSizePt: 13,
              alignment: getAttr(firstChild(firstChild(ip, 'pPr') ?? ip, 'jc'), 'val') ?? 'left',
              indentLeftMm: 0,
              indentRightMm: 0,
              indentFirstLineMm: 0,
              spaceBeforePt: 0,
              spaceAfterPt: 0,
              lineSpacingMult: 1.15,
              widowControl: true,
              keepNext: false,
              keepLines: false,
              pageBreakBefore: false,
              inTableId: null,
              inTextBoxId: tId,
              estimatedLines: 1,
              gapFromPrevPt: 0,
              locator: { ...loc, elementPath: `body/p[${bIdx}]/txbx[${txbxSeq - 1}]/p[${ipIdx}]` },
            });
          });
          const overflows = vis.widthMm > sec.printableWidthMm + 1.5 || vis.heightMm > sec.printableHeightMm + 1.5;
          const vLoc: ElementLocator = { ...loc, elementPath: `body/p[${bIdx}]/txbx[${txbxSeq - 1}]`, widthMm: vis.widthMm, heightMm: vis.heightMm };
          const combined = excerptStr(innerTexts.join(' | '), 120);
          visualObjects.push({
            objectId: tId,
            objectKind: 'textBox',
            anchorMode: vis.anchorMode,
            relationshipId: null,
            mediaTarget: null,
            widthMm: vis.widthMm,
            heightMm: vis.heightMm,
            overflowsPrintableArea: overflows,
            innerParagraphCount: vis.innerParagraphs.length,
            textExcerpt: combined,
            locator: vLoc,
          });
          nodeSeq += 1;
          docNodes.push({
            nodeId: `node-${nodeSeq}`,
            parentId: visParent,
            childrenIds: [],
            kind: 'textBox',
            label: `Text Box #${txbxSeq} (${vis.innerParagraphs.length} đoạn)`,
            excerpt: combined || '(Text Box trống)',
            level: 0,
            confidence: 0.98,
            signals: ['openxml:txbxContent', `mode:${vis.anchorMode}`],
            locator: vLoc,
          });
        } else {
          visualSeq += 1;
          const imgId = `img-${visualSeq}`;
          const target = vis.relId ? relsMap.get(vis.relId)?.target ?? null : null;
          const overflows = vis.widthMm > sec.printableWidthMm + 1.5 || vis.heightMm > sec.printableHeightMm + 1.5;
          const vLoc: ElementLocator = { ...loc, elementPath: `body/p[${bIdx}]/drawing[${visualSeq - 1}]`, widthMm: vis.widthMm, heightMm: vis.heightMm };
          visualObjects.push({
            objectId: imgId,
            objectKind: 'image',
            anchorMode: vis.anchorMode,
            relationshipId: vis.relId,
            mediaTarget: target,
            widthMm: vis.widthMm,
            heightMm: vis.heightMm,
            overflowsPrintableArea: overflows,
            innerParagraphCount: 0,
            textExcerpt: target ?? 'Ảnh nhúng',
            locator: vLoc,
          });
          nodeSeq += 1;
          docNodes.push({
            nodeId: `node-${nodeSeq}`,
            parentId: visParent,
            childrenIds: [],
            kind: 'imageAsset',
            label: `Hình ảnh #${visualSeq} (${vis.widthMm}×${vis.heightMm} mm)`,
            excerpt: target ?? 'Ảnh nhúng OpenXML',
            level: 0,
            confidence: 0.99,
            signals: [`openxml:${vis.anchorMode}`],
            locator: vLoc,
          });
        }
      }

      curYMm += paraH;
      const nextSec = inlineSectParaIdx.get(bIdx);
      if (nextSec && nextSec < rawSections.length) {
        currentSec = nextSec + 1;
        const ns = rawSections[currentSec - 1];
        if (ns.breakType !== 'continuous') {
          currentPage += 1;
          curCol = 0;
          curYMm = 0;
          ensureBucket(currentPage, currentSec);
        }
        secRanges.set(currentSec, [currentPage, currentPage]);
      }
    } else if (el.localName === 'tbl') {
      const tId = `tbl-${tblIdx}`;
      const rows = directChildren(el, 'tr');
      const gridCols = firstChild(el, 'tblGrid')
        ? directChildren(firstChild(el, 'tblGrid')!, 'gridCol')
            .map((c) => Number(getAttr(c, 'w') ?? '0'))
            .filter((n) => n > 0)
        : [];
      const tblPr = firstChild(el, 'tblPr');
      const tblW = Number(getAttr(firstChild(tblPr ?? el, 'tblW'), 'w') ?? '0') || 0;
      const totalTw = gridCols.length > 0 ? gridCols.reduce((a, b) => a + b, 0) : tblW > 0 ? tblW : sec.printableWidthTwips;
      const totalMm = twipsToMm(totalTw);
      const tblInd = Number(getAttr(firstChild(tblPr ?? el, 'tblInd'), 'w') ?? '0') || 0;
      const tblJc = getAttr(firstChild(tblPr ?? el, 'jc'), 'val') ?? 'left';
      const maxCols = rows.reduce((m, r) => Math.max(m, directChildren(r, 'tc').length), 0) || gridCols.length || 1;
      const colWidthsMm = gridCols.length > 0 ? gridCols.map(twipsToMm) : Array.from({ length: maxCols }, () => round1(totalMm / maxCols));

      const tblUpper = collectParagraphText(el).toUpperCase();
      const firstRowTexts = rows[0] ? collectParagraphText(rows[0]).toUpperCase() : '';
      let tableRole = 'dataTable';
      if (bIdx <= 3 && maxCols === 2 && (tblUpper.includes('CỘNG HÒA') || tblUpper.includes('ĐỘC LẬP'))) {
        tableRole = 'adminHeader';
        seenHeaderOrTitle = true;
      } else if (tblUpper.includes('NƠI NHẬN') || firstRowTexts.includes('TM.') || tblUpper.includes('KT.')) {
        tableRole = 'signatureLayout';
        seenSigZone = true;
      }

      const tblH = round1(Math.max(12, rows.length * 10));
      if (curYMm > 0 && curYMm + Math.min(45, tblH) > sec.printableHeightMm) {
        currentPage += 1;
        curCol = 0;
        curYMm = 0;
        ensureBucket(currentPage, currentSec);
      }
      curYMm += Math.min(tblH, sec.printableHeightMm * 0.9);
      const overflowMm = round1(Math.max(0, totalMm - sec.printableWidthMm));
      const overflowsWidth = overflowMm > 2.1;
      const loc: ElementLocator = {
        partName: 'word/document.xml',
        elementPath: `body/tbl[${bIdx}]`,
        bodyChildIndex: bIdx,
        sectionIndex: currentSec,
        estimatedPageStart: currentPage,
        estimatedPageEnd: currentPage,
        columnIndex: curCol,
        xLeftMm: round1(sec.marginLeftMm + twipsToMm(tblInd)),
        yTopMm: round1(sec.marginTopMm + curYMm),
        widthMm: totalMm,
        heightMm: tblH,
      };

      const hasRepeatHeader = Boolean(rows[0] && firstChild(firstChild(rows[0], 'trPr') ?? rows[0], 'tblHeader'));
      tableMap.push({
        tableId: tId,
        tableIndex: tblIdx,
        tableRole,
        rowCount: rows.length,
        colCount: maxCols,
        colWidthsMm,
        totalWidthMm: totalMm,
        printableWidthMm: sec.printableWidthMm,
        indentLeftMm: twipsToMm(tblInd),
        alignment: tblJc,
        hasRepeatHeader,
        hasMergedCells: false,
        lockedLongRowCount: 0,
        overflowsWidth,
        overflowMm,
        recommendedOrientation: tableRole === 'dataTable' && overflowsWidth && (maxCols >= 6 || totalMm > sec.printableWidthMm * 1.15) ? 'landscape' : sec.orientation,
        locator: loc,
      });

      const bk = pageBuckets.get(currentPage);
      if (bk) {
        bk.elementIds.push(tId);
        bk.usedHeightMm = round1(Math.min(sec.printableHeightMm * 1.1, curYMm));
        if (overflowsWidth) bk.hasOverflow = true;
      }

      nodeSeq += 1;
      docNodes.push({
        nodeId: `node-${nodeSeq}`,
        parentId: tableRole === 'dataTable' ? (hierarchyStack.length > 0 ? hierarchyStack[hierarchyStack.length - 1].id : curAppendixId) : null,
        childrenIds: [],
        kind: tableRole === 'adminHeader' ? 'documentHeader' : tableRole === 'signatureLayout' ? 'signatureBlock' : 'dataTable',
        label: tableRole === 'adminHeader' ? 'Phần đầu văn bản (Bảng 2 cột)' : tableRole === 'signatureLayout' ? 'Khung Nơi nhận & Chữ ký' : `Bảng ${tblIdx + 1} (${rows.length}×${maxCols})`,
        excerpt: excerptStr(collectParagraphText(el), 90),
        level: 0,
        confidence: 0.98,
        signals: [`openxml:tbl[${tblIdx}]`],
        locator: loc,
      });

      // Index table cell paragraphs
      rows.forEach((tr, rIdx) => {
        directChildren(tr, 'tc').forEach((tc, cIdx) => {
          directChildren(tc, 'p').forEach((cp, cpIdx) => {
            const cTxt = collectParagraphText(cp).trim();
            paragraphMap.push({
              paraId: `p-${paragraphMap.length}`,
              globalIndex: paragraphMap.length,
              kind: tableRole === 'adminHeader' ? 'documentHeader' : tableRole === 'signatureLayout' ? (cIdx === 0 ? 'recipientBlock' : 'signatureBlock') : 'mainBody',
              roleLabel: 'Ô bảng',
              excerpt: excerptStr(cTxt, 100),
              styleId: null,
              fontName: 'Times New Roman',
              fontSizePt: 13,
              alignment: getAttr(firstChild(firstChild(cp, 'pPr') ?? cp, 'jc'), 'val') ?? 'left',
              indentLeftMm: 0,
              indentRightMm: 0,
              indentFirstLineMm: 0,
              spaceBeforePt: 0,
              spaceAfterPt: 0,
              lineSpacingMult: 1.15,
              widowControl: true,
              keepNext: false,
              keepLines: false,
              pageBreakBefore: false,
              inTableId: tId,
              inTextBoxId: null,
              estimatedLines: 1,
              gapFromPrevPt: 0,
              locator: { ...loc, elementPath: `body/tbl[${bIdx}]/tr[${rIdx}]/tc[${cIdx}]/p[${cpIdx}]` },
            });
          });
        });
      });
      tblIdx += 1;
    }
  });

  // Wire DocumentMap childrenIds
  const kidMap = new Map<string, string[]>();
  for (const n of docNodes) {
    if (n.parentId) {
      const list = kidMap.get(n.parentId) ?? [];
      list.push(n.nodeId);
      kidMap.set(n.parentId, list);
    }
  }
  for (const n of docNodes) {
    n.childrenIds = kidMap.get(n.nodeId) ?? [];
  }

  const sectionMap: SectionMapEntry[] = rawSections.map((s) => {
    const [ps, pe] = secRanges.get(s.sectionIndex) ?? [1, currentPage];
    return {
      sectionId: s.sectionId,
      sectionIndex: s.sectionIndex,
      breakType: s.breakType,
      orientation: s.orientation,
      paperWidthMm: s.paperWidthMm,
      paperHeightMm: s.paperHeightMm,
      marginTopMm: s.marginTopMm,
      marginBottomMm: s.marginBottomMm,
      marginLeftMm: s.marginLeftMm,
      marginRightMm: s.marginRightMm,
      printableWidthMm: s.printableWidthMm,
      printableHeightMm: s.printableHeightMm,
      columnCount: s.columnCount,
      columnSpaceMm: s.columnSpaceMm,
      equalColumnWidth: s.equalColumnWidth,
      colWidthsMm: s.colWidthsMm,
      headerIds: secHeaderIds.get(s.sectionIndex) ?? [],
      footerIds: secFooterIds.get(s.sectionIndex) ?? [],
      pageStart: ps,
      pageEnd: Math.max(ps, pe),
    };
  });

  const pageLayoutMap: PageLayoutEntry[] = Array.from(pageBuckets.values()).sort((a, b) => a.pageNumber - b.pageNumber);
  for (const pg of pageLayoutMap) {
    pg.fillRatioPct = round1(Math.min(120, (pg.usedHeightMm / Math.max(50, pg.printableHeightMm)) * 100));
  }

  const maps: IntelligentDocumentMap = {
    documentMap: docNodes,
    paragraphMap,
    tableMap,
    sectionMap,
    styleMap,
    headerFooterMap,
    pageLayoutMap,
    visualObjects,
  };

  // Analyze Warnings & Proposals
  const { warnings, proposals } = generateWarningsAndProposals(maps);

  const mediaEntries = entriesList.filter((e) => e.name.toLowerCase().startsWith('word/media/'));
  const assets: AssetInventory = {
    mediaCount: mediaEntries.length,
    mediaBytes: mediaEntries.reduce((s, e) => s + e.data.byteLength, 0),
    drawingCount: visualObjects.length,
    hyperlinkCount: docRoot.getElementsByTagNameNS(W_NS, 'hyperlink').length,
    bookmarkCount: docRoot.getElementsByTagNameNS(W_NS, 'bookmarkStart').length,
    fieldCount: 0,
    footnoteCount: 0,
    endnoteCount: 0,
    commentCount: 0,
    hasVbaMacros: entriesList.some((e) => e.name.toLowerCase() === 'word/vbaproject.bin'),
    vbaBytes: 0,
  };

  const autoProposalCount = proposals.filter((p) => p.tier === 'highConfidenceAuto').length;
  return {
    filePath: file.name,
    fileName: file.name,
    scope: scope === 'autoDetect' ? (seenDocType ? 'adminNd30' : 'enterpriseInternal') : scope,
    summary: {
      sectionCount: sectionMap.length,
      estimatedPageCount: pageLayoutMap.length,
      paragraphCount: paragraphMap.length,
      tableCount: tableMap.length,
      imageCount: visualObjects.filter((v) => v.objectKind === 'image').length,
      textBoxCount: visualObjects.filter((v) => v.objectKind === 'textBox').length,
      headingCount: docNodes.filter((d) => d.kind.startsWith('headingLevel')).length,
      styleCount: styleMap.length,
      headerFooterCount: headerFooterMap.length,
      warningCount: warnings.length,
      autoProposalCount,
      confirmProposalCount: proposals.length - autoProposalCount,
    },
    maps,
    warnings,
    proposals,
    assets,
  };
}

function generateWarningsAndProposals(maps: IntelligentDocumentMap): { warnings: LayoutWarning[]; proposals: LayoutProposal[] } {
  const warnings: LayoutWarning[] = [];
  const proposals: LayoutProposal[] = [];
  let wSeq = 0;
  let pSeq = 0;

  for (const tbl of maps.tableMap) {
    if (tbl.tableRole !== 'dataTable') continue;
    if (tbl.overflowsWidth) {
      wSeq += 1;
      warnings.push({
        warningId: `warn-${wSeq}`,
        kind: 'tableOverflowWidth',
        severity: 'error',
        title: `Bảng ${tbl.tableIndex + 1} tràn lề ngang (+${tbl.overflowMm} mm)`,
        detail: `Tổng độ rộng ${tbl.totalWidthMm} mm vượt khổ in ${tbl.printableWidthMm} mm.`,
        targetElementId: tbl.tableId,
        locator: tbl.locator,
      });
      pSeq += 1;
      proposals.push({
        proposalId: `prop-${pSeq}`,
        kind: 'resolveOverflow',
        tier: 'requiresConfirmation',
        confidence: 0.94,
        title: `Co bảng ${tbl.tableIndex + 1} vừa khít vùng in (${tbl.printableWidthMm} mm)`,
        reason: 'Bảng vượt lề phải có thể bị cắt nội dung khi in.',
        suggestedChange: 'Co tỷ lệ tblGrid và tcW về đúng khổ in Section.',
        targetElementIds: [tbl.tableId],
        locator: tbl.locator,
        affectsPagination: true,
        affectsSection: false,
        affectsTable: true,
        affectsSignature: false,
        paramValue: 'fitTablePrintableWidth',
      });
      if (tbl.recommendedOrientation === 'landscape') {
        pSeq += 1;
        proposals.push({
          proposalId: `prop-${pSeq}`,
          kind: 'recommendOrientation',
          tier: 'requiresConfirmation',
          confidence: 0.87,
          title: `Đề xuất chuyển Section ${tbl.locator.sectionIndex} sang khổ giấy Ngang`,
          reason: `Bảng có ${tbl.colCount} cột thích hợp trang A4 Ngang.`,
          suggestedChange: 'Đặt hướng giấy Landscape và xoay lề chuẩn.',
          targetElementIds: [`sec-${tbl.locator.sectionIndex}`, tbl.tableId],
          locator: tbl.locator,
          affectsPagination: true,
          affectsSection: true,
          affectsTable: true,
          affectsSignature: false,
          paramValue: `landscapeSection:${tbl.locator.sectionIndex}`,
        });
      }
    }
    if (tbl.indentLeftMm > 2.5 || (tbl.rowCount >= 5 && !tbl.hasRepeatHeader) || tbl.alignment !== 'center') {
      pSeq += 1;
      proposals.push({
        proposalId: `prop-${pSeq}`,
        kind: 'adjustTableLayout',
        tier: 'requiresConfirmation',
        confidence: 0.9,
        title: `Căn chỉnh & lặp dòng tiêu đề cho Bảng ${tbl.tableIndex + 1}`,
        reason: 'Đưa w:tblInd về 0, căn giữa và bật tblHeader nếu bảng dài.',
        suggestedChange: 'Chuẩn hóa căn giữa bảng và thuộc tính hàng tiêu đề.',
        targetElementIds: [tbl.tableId],
        locator: tbl.locator,
        affectsPagination: true,
        affectsSection: false,
        affectsTable: true,
        affectsSignature: false,
        paramValue: 'normalizeTable',
      });
    }
  }

  for (const vis of maps.visualObjects) {
    if (vis.overflowsPrintableArea) {
      wSeq += 1;
      warnings.push({
        warningId: `warn-${wSeq}`,
        kind: 'imageOverflowPage',
        severity: 'error',
        title: `${vis.objectKind === 'textBox' ? 'Text Box' : 'Hình ảnh'} (${vis.widthMm}×${vis.heightMm} mm) tràn vùng in`,
        detail: `Vượt khổ in tại Section ${vis.locator.sectionIndex}.`,
        targetElementId: vis.objectId,
        locator: vis.locator,
      });
      if (vis.objectKind === 'image' && vis.anchorMode === 'inline') {
        pSeq += 1;
        proposals.push({
          proposalId: `prop-${pSeq}`,
          kind: 'resolveOverflow',
          tier: 'requiresConfirmation',
          confidence: 0.93,
          title: `Thu tỷ lệ ảnh ${vis.objectId} vừa khổ trang in`,
          reason: 'Tránh ảnh vượt lề phải trang giấy.',
          suggestedChange: 'Thu đồng dạng wp:extent cx/cy giữ nguyên tỷ lệ khung hình.',
          targetElementIds: [vis.objectId],
          locator: vis.locator,
          affectsPagination: true,
          affectsSection: false,
          affectsTable: false,
          affectsSignature: false,
          paramValue: `fitInlineImage:${vis.objectId}`,
        });
      }
    }
  }

  const bodyParas = maps.paragraphMap.filter((p) => !p.inTableId && !p.inTextBoxId);
  const spacingIds: string[] = [];
  const alignIds: string[] = [];
  const unifyIds: string[] = [];
  const strandedIds: string[] = [];
  const emptyRunIds: string[] = [];

  for (let i = 0; i < bodyParas.length; i++) {
    const p = bodyParas[i];
    if (!p.excerpt && !p.pageBreakBefore) {
      if (i > 0 && !bodyParas[i - 1].excerpt && !bodyParas[i - 1].pageBreakBefore) {
        if (!emptyRunIds.includes(bodyParas[i - 1].paraId)) emptyRunIds.push(bodyParas[i - 1].paraId);
        emptyRunIds.push(p.paraId);
      }
      continue;
    }
    if (p.spaceBeforePt > 24 || p.spaceAfterPt > 24) {
      wSeq += 1;
      warnings.push({
        warningId: `warn-${wSeq}`,
        kind: 'excessiveSpacing',
        severity: 'warning',
        title: `Khoảng cách đoạn lớn (${p.spaceBeforePt}pt / ${p.spaceAfterPt}pt)`,
        detail: `Quanh đoạn "${p.excerpt}"`,
        targetElementId: p.paraId,
        locator: p.locator,
      });
      spacingIds.push(p.paraId);
    }
    if (!p.widowControl && p.estimatedLines >= 3) {
      wSeq += 1;
      warnings.push({
        warningId: `warn-${wSeq}`,
        kind: 'widowOrphanRisk',
        severity: 'warning',
        title: 'Nguy cơ dòng đơn độc (tắt Widow/Orphan)',
        detail: `"${p.excerpt}"`,
        targetElementId: p.paraId,
        locator: p.locator,
      });
      unifyIds.push(p.paraId);
    }
    if (p.kind === 'mainBody' && p.excerpt.length >= 35 && (p.alignment === 'left' || !p.widowControl)) {
      if (!unifyIds.includes(p.paraId)) unifyIds.push(p.paraId);
    }
    if (p.kind.startsWith('headingLevel')) {
      if (!p.keepNext) {
        wSeq += 1;
        warnings.push({
          warningId: `warn-${wSeq}`,
          kind: 'strandedHeadingBottom',
          severity: 'warning',
          title: 'Tiêu đề thiếu keepNext (nguy cơ trơ cuối trang)',
          detail: `"${p.excerpt}"`,
          targetElementId: p.paraId,
          locator: p.locator,
        });
        strandedIds.push(p.paraId);
      }
      if (p.indentFirstLineMm > 1.5 || p.alignment === 'right') {
        wSeq += 1;
        warnings.push({
          warningId: `warn-${wSeq}`,
          kind: 'misalignedComponent',
          severity: 'warning',
          title: 'Tiêu đề bị lệch căn chỉnh / thụt dòng',
          detail: `"${p.excerpt}"`,
          targetElementId: p.paraId,
          locator: p.locator,
        });
        alignIds.push(p.paraId);
      }
    }
    if (['docTypeTitle', 'mainTitle', 'docSubject'].includes(p.kind) && (p.alignment !== 'center' || p.indentFirstLineMm > 0.5)) {
      wSeq += 1;
      warnings.push({
        warningId: `warn-${wSeq}`,
        kind: 'misalignedComponent',
        severity: 'warning',
        title: 'Tiêu đề chính / Trích yếu bị lệch tâm',
        detail: `"${p.excerpt}"`,
        targetElementId: p.paraId,
        locator: p.locator,
      });
      alignIds.push(p.paraId);
    }
  }

  if (emptyRunIds.length >= 2) {
    wSeq += 1;
    const firstP = bodyParas.find((p) => p.paraId === emptyRunIds[0])!;
    warnings.push({
      warningId: `warn-${wSeq}`,
      kind: 'stackedEmptyParagraphs',
      severity: 'warning',
      title: `${emptyRunIds.length} đoạn Enter trống tạo khoảng trắng giả`,
      detail: 'Nên thu gọn đoạn rỗng lặp để tránh nhảy trang bất thường.',
      targetElementId: firstP.paraId,
      locator: firstP.locator,
    });
    pSeq += 1;
    proposals.push({
      proposalId: `prop-${pSeq}`,
      kind: 'normalizeSectionSpacing',
      tier: 'requiresConfirmation',
      confidence: 0.89,
      title: `Thu gọn ${emptyRunIds.length} đoạn Enter trống lặp`,
      reason: 'Giữ nguyên toàn bộ nội dung chữ, xóa đoạn Enter trống liên tiếp.',
      suggestedChange: 'Xóa các thẻ w:p hoàn toàn rỗng liên tiếp.',
      targetElementIds: emptyRunIds,
      locator: firstP.locator,
      affectsPagination: true,
      affectsSection: false,
      affectsTable: false,
      affectsSignature: false,
      paramValue: 'collapseStackedEmptyParagraphs',
    });
  }

  if (spacingIds.length > 0) {
    const first = bodyParas.find((p) => p.paraId === spacingIds[0])!;
    pSeq += 1;
    proposals.push({
      proposalId: `prop-${pSeq}`,
      kind: 'normalizeSpacing',
      tier: 'highConfidenceAuto',
      confidence: 0.96,
      title: `Chuẩn hóa khoảng cách cho ${spacingIds.length} đoạn`,
      reason: 'Đưa khoảng cách Before/After quá lớn về mức cân đối (6pt).',
      suggestedChange: 'Đặt w:spacing (before=120, after=120, line=276).',
      targetElementIds: spacingIds,
      locator: first.locator,
      affectsPagination: false,
      affectsSection: false,
      affectsTable: false,
      affectsSignature: false,
      paramValue: 'normalizeSpacingAuto',
    });
  }

  if (alignIds.length > 0) {
    const first = bodyParas.find((p) => p.paraId === alignIds[0])!;
    pSeq += 1;
    proposals.push({
      proposalId: `prop-${pSeq}`,
      kind: 'alignHeadings',
      tier: 'highConfidenceAuto',
      confidence: 0.97,
      title: `Căn chỉnh lại ${alignIds.length} tiêu đề`,
      reason: 'Xóa thụt đầu dòng thừa gây lệch tâm/lề tiêu đề.',
      suggestedChange: 'Đặt firstLine=0 và căn giữa/trái chuẩn theo cấp.',
      targetElementIds: alignIds,
      locator: first.locator,
      affectsPagination: false,
      affectsSection: false,
      affectsTable: false,
      affectsSignature: false,
      paramValue: 'alignHeadingsAuto',
    });
  }

  if (unifyIds.length > 0) {
    const first = bodyParas.find((p) => p.paraId === unifyIds[0])!;
    pSeq += 1;
    proposals.push({
      proposalId: `prop-${pSeq}`,
      kind: 'unifyParagraphFormat',
      tier: 'highConfidenceAuto',
      confidence: 0.95,
      title: `Đồng nhất căn đều & Widow/Orphan cho ${unifyIds.length} đoạn`,
      reason: 'Đảm bảo đoạn văn căn đều 2 bên và bật chống dòng đơn độc.',
      suggestedChange: 'Đặt w:jc=both, firstLine=720 và bật w:widowControl.',
      targetElementIds: unifyIds,
      locator: first.locator,
      affectsPagination: false,
      affectsSection: false,
      affectsTable: false,
      affectsSignature: false,
      paramValue: 'unifyBodyParagraphsAuto',
    });
  }

  if (strandedIds.length > 0) {
    const first = bodyParas.find((p) => p.paraId === strandedIds[0])!;
    pSeq += 1;
    proposals.push({
      proposalId: `prop-${pSeq}`,
      kind: 'normalizeSectionSpacing',
      tier: 'requiresConfirmation',
      confidence: 0.92,
      title: `Gắn keepNext giữ liền cho ${strandedIds.length} tiêu đề`,
      reason: 'Ngăn tiêu đề đứng một mình cuối trang.',
      suggestedChange: 'Bổ sung w:keepNext và w:keepLines.',
      targetElementIds: strandedIds,
      locator: first.locator,
      affectsPagination: true,
      affectsSection: false,
      affectsTable: false,
      affectsSignature: false,
      paramValue: 'keepHeadingsWithNext',
    });
  }

  return { warnings, proposals };
}

function labelForKindVi(kind: RecognizedStructureKind): string {
  switch (kind) {
    case 'documentHeader': return 'Phần đầu văn bản';
    case 'docTypeTitle': return 'Tên loại văn bản';
    case 'docSubject': return 'Trích yếu';
    case 'mainTitle': return 'Tiêu đề chính';
    case 'mainBody': return 'Nội dung chính';
    case 'headingLevel1': return 'Tiêu đề cấp 1';
    case 'headingLevel2': return 'Tiêu đề cấp 2';
    case 'headingLevel3': return 'Tiêu đề cấp 3';
    case 'headingLevel4': return 'Tiêu đề cấp 4';
    case 'dataTable': return 'Bảng biểu';
    case 'imageAsset': return 'Hình ảnh';
    case 'signatureBlock': return 'Chữ ký';
    case 'recipientBlock': return 'Nơi nhận';
    case 'appendixBlock': return 'Phụ lục';
    case 'headerFooter': return 'Header / Footer';
    case 'textBox': return 'Text Box';
  }
}

export async function executeWebLayoutProposals(
  file: File,
  req: LayoutExecutionRequest,
): Promise<LayoutExecutionResult> {
  const report = await analyzeWebLayoutIntelligence(file, req.scope ?? 'autoDetect');
  const entriesMap = await readZipEntries(await file.arrayBuffer());
  const decoder = new TextDecoder('utf-8');
  const encoder = new TextEncoder();
  const parser = new DOMParser();
  const serializer = new XMLSerializer();

  let docKey: string | null = null;
  for (const k of entriesMap.keys()) {
    if (k.replace(/^\/+/, '').replace(/\\/g, '/').toLowerCase() === 'word/document.xml') {
      docKey = k;
      break;
    }
  }
  if (!docKey) {
    return {
      success: false,
      outputPath: '',
      backupPath: null,
      error: 'Thiếu word/document.xml',
      appliedProposalIds: [],
      skippedProposalIds: [],
      contentPreservedVerified: true,
      relationshipsPreservedVerified: true,
      postSaveReport: null,
      logs: [],
    };
  }

  const doc = parser.parseFromString(decoder.decode(entriesMap.get(docKey)!).replace(/^\uFEFF/, ''), 'application/xml');
  const baselineTexts = collectAllNonEmptyTexts(doc.documentElement);
  const body = firstChild(doc.documentElement, 'body')!;
  const bodyChildren = directChildren(body);

  const accepted = new Set(req.acceptedProposalIds);
  const rejected = new Set(req.rejectedProposalIds);
  const applied: string[] = [];
  const skipped: string[] = [];
  const logs: string[] = [];
  const removeIndices = new Set<number>();

  const paraBodyIdx = new Map<string, { bIdx: number; kind: RecognizedStructureKind }>();
  for (const p of report.maps.paragraphMap) {
    if (!p.inTableId && !p.inTextBoxId && p.locator.bodyChildIndex !== null) {
      paraBodyIdx.set(p.paraId, { bIdx: p.locator.bodyChildIndex, kind: p.kind });
    }
  }
  const tblBodyIdx = new Map<string, { bIdx: number; printableMm: number }>();
  for (const t of report.maps.tableMap) {
    if (t.locator.bodyChildIndex !== null) {
      tblBodyIdx.set(t.tableId, { bIdx: t.locator.bodyChildIndex, printableMm: t.printableWidthMm });
    }
  }

  for (const prop of report.proposals) {
    const canRun = !rejected.has(prop.proposalId) && (
      prop.affectsPagination || prop.affectsSection || prop.affectsTable || prop.affectsSignature || prop.tier === 'requiresConfirmation'
        ? accepted.has(prop.proposalId)
        : req.applyHighConfidenceAuto || accepted.has(prop.proposalId)
    );
    if (!canRun) {
      skipped.push(prop.proposalId);
      continue;
    }

    if (prop.kind === 'normalizeSpacing') {
      for (const id of prop.targetElementIds) {
        const info = paraBodyIdx.get(id);
        const p = info ? bodyChildren[info.bIdx] : undefined;
        if (p && p.localName === 'p') {
          const pPr = ensureWChildOrdered(p, doc, 'w:pPr', P_PR_ORDER);
          const sp = ensureWChildOrdered(pPr, doc, 'w:spacing', P_PR_ORDER);
          setWAttr(sp, 'before', info!.kind.startsWith('headingLevel') ? '160' : '120');
          setWAttr(sp, 'after', '120');
          setWAttr(sp, 'line', '276');
          setWAttr(sp, 'lineRule', 'auto');
        }
      }
    } else if (prop.kind === 'alignHeadings') {
      for (const id of prop.targetElementIds) {
        const info = paraBodyIdx.get(id);
        const p = info ? bodyChildren[info.bIdx] : undefined;
        if (p && p.localName === 'p') {
          const pPr = ensureWChildOrdered(p, doc, 'w:pPr', P_PR_ORDER);
          const align = ['docTypeTitle', 'docSubject', 'mainTitle', 'headingLevel1'].includes(info!.kind) ? 'center' : 'left';
          setWAttr(ensureWChildOrdered(pPr, doc, 'w:jc', P_PR_ORDER), 'val', align);
          const ind = ensureWChildOrdered(pPr, doc, 'w:ind', P_PR_ORDER);
          setWAttr(ind, 'firstLine', '0');
          removeWAttr(ind, 'firstLineChars');
        }
      }
    } else if (prop.kind === 'unifyParagraphFormat') {
      for (const id of prop.targetElementIds) {
        const info = paraBodyIdx.get(id);
        const p = info ? bodyChildren[info.bIdx] : undefined;
        if (p && p.localName === 'p' && info!.kind === 'mainBody') {
          const pPr = ensureWChildOrdered(p, doc, 'w:pPr', P_PR_ORDER);
          removeWAttr(ensureWChildOrdered(pPr, doc, 'w:widowControl', P_PR_ORDER), 'val');
          setWAttr(ensureWChildOrdered(pPr, doc, 'w:jc', P_PR_ORDER), 'val', 'both');
        }
      }
    } else if (prop.kind === 'adjustTableLayout') {
      for (const tid of prop.targetElementIds) {
        const info = tblBodyIdx.get(tid);
        const tbl = info ? bodyChildren[info.bIdx] : undefined;
        if (tbl && tbl.localName === 'tbl') {
          const tblPr = ensureWChildOrdered(tbl, doc, 'w:tblPr', TBL_PR_ORDER);
          setWAttr(ensureWChildOrdered(tblPr, doc, 'w:jc', TBL_PR_ORDER), 'val', 'center');
          const ind = ensureWChildOrdered(tblPr, doc, 'w:tblInd', TBL_PR_ORDER);
          setWAttr(ind, 'w', '0');
          setWAttr(ind, 'type', 'dxa');
          const rows = directChildren(tbl, 'tr');
          if (rows.length >= 5) {
            const trPr = ensureWChildOrdered(rows[0], doc, 'w:trPr', TR_PR_ORDER);
            ensureWChildOrdered(trPr, doc, 'w:tblHeader', TR_PR_ORDER);
          }
        }
      }
    } else if (prop.kind === 'resolveOverflow') {
      if (prop.paramValue === 'fitTablePrintableWidth') {
        for (const tid of prop.targetElementIds) {
          const info = tblBodyIdx.get(tid);
          const tbl = info ? bodyChildren[info.bIdx] : undefined;
          if (tbl && tbl.localName === 'tbl') {
            const targetTw = Math.max(4000, Math.round(info!.printableMm * 56.6929) - 40);
            const tblPr = ensureWChildOrdered(tbl, doc, 'w:tblPr', TBL_PR_ORDER);
            const tblW = ensureWChildOrdered(tblPr, doc, 'w:tblW', TBL_PR_ORDER);
            setWAttr(tblW, 'w', String(targetTw));
            setWAttr(tblW, 'type', 'dxa');
            const grid = firstChild(tbl, 'tblGrid');
            if (grid) {
              const gcs = directChildren(grid, 'gridCol');
              const oldSum = gcs.reduce((s, c) => s + (Number(getAttr(c, 'w') ?? '0') || 0), 0);
              if (oldSum > 0) {
                const newCols = gcs.map((c) => Math.max(360, Math.floor(((Number(getAttr(c, 'w') ?? '0') || 0) * targetTw) / oldSum)));
                gcs.forEach((c, ci) => setWAttr(c, 'w', String(newCols[ci])));
                for (const tr of directChildren(tbl, 'tr')) {
                  directChildren(tr, 'tc').forEach((tc, ci) => {
                    if (ci < newCols.length) {
                      const tcPr = ensureWChildOrdered(tc, doc, 'w:tcPr', TC_PR_ORDER);
                      const tcW = ensureWChildOrdered(tcPr, doc, 'w:tcW', TC_PR_ORDER);
                      setWAttr(tcW, 'w', String(newCols[ci]));
                      setWAttr(tcW, 'type', 'dxa');
                    }
                  });
                }
              }
            }
          }
        }
      }
    } else if (prop.kind === 'recommendOrientation' && prop.paramValue?.startsWith('landscapeSection:')) {
      const targetSec = Number(prop.paramValue.split(':')[1] ?? '1');
      let secCursor = 0;
      for (const ch of bodyChildren) {
        if (ch.localName === 'p') {
          const s = firstChild(firstChild(ch, 'pPr') ?? ch, 'sectPr');
          if (s && firstChild(ch, 'pPr')?.contains(s)) {
            secCursor += 1;
            if (secCursor === targetSec) {
              const pgSz = ensureWChildOrdered(s, doc, 'w:pgSz', SECT_PR_ORDER);
              const w = Number(getAttr(pgSz, 'w') ?? '11906');
              const h = Number(getAttr(pgSz, 'h') ?? '16838');
              setWAttr(pgSz, 'w', String(Math.max(w, h)));
              setWAttr(pgSz, 'h', String(Math.min(w, h)));
              setWAttr(pgSz, 'orient', 'landscape');
            }
          }
        }
      }
      const trail = firstChild(body, 'sectPr');
      if (trail && secCursor + 1 === targetSec) {
        const pgSz = ensureWChildOrdered(trail, doc, 'w:pgSz', SECT_PR_ORDER);
        const w = Number(getAttr(pgSz, 'w') ?? '11906');
        const h = Number(getAttr(pgSz, 'h') ?? '16838');
        setWAttr(pgSz, 'w', String(Math.max(w, h)));
        setWAttr(pgSz, 'h', String(Math.min(w, h)));
        setWAttr(pgSz, 'orient', 'landscape');
      }
    } else if (prop.kind === 'normalizeSectionSpacing') {
      if (prop.paramValue === 'collapseStackedEmptyParagraphs') {
        for (const id of prop.targetElementIds) {
          const info = paraBodyIdx.get(id);
          const p = info ? bodyChildren[info.bIdx] : undefined;
          if (p && p.localName === 'p' && isRemovableEmptyWParagraph(p)) {
            removeIndices.add(info!.bIdx);
          }
        }
      } else {
        for (const id of prop.targetElementIds) {
          const info = paraBodyIdx.get(id);
          const p = info ? bodyChildren[info.bIdx] : undefined;
          if (p && p.localName === 'p') {
            const pPr = ensureWChildOrdered(p, doc, 'w:pPr', P_PR_ORDER);
            ensureWChildOrdered(pPr, doc, 'w:keepLines', P_PR_ORDER);
            ensureWChildOrdered(pPr, doc, 'w:keepNext', P_PR_ORDER);
          }
        }
      }
    } else if (prop.kind === 'rebalanceLayout') {
      for (const id of prop.targetElementIds) {
        const info = paraBodyIdx.get(id);
        const p = info ? bodyChildren[info.bIdx] : undefined;
        if (p && p.localName === 'p') {
          const pPr = ensureWChildOrdered(p, doc, 'w:pPr', P_PR_ORDER);
          ensureWChildOrdered(pPr, doc, 'w:keepLines', P_PR_ORDER);
          ensureWChildOrdered(pPr, doc, 'w:keepNext', P_PR_ORDER);
        }
      }
    }

    applied.push(prop.proposalId);
    logs.push(`Đã áp dụng: ${prop.title}`);
  }

  for (const rIdx of Array.from(removeIndices).sort((a, b) => b - a)) {
    const targetNode = bodyChildren[rIdx];
    if (targetNode && body.contains(targetNode)) {
      body.removeChild(targetNode);
    }
  }

  const afterTexts = collectAllNonEmptyTexts(doc.documentElement);
  const contentOk = afterTexts.length === baselineTexts.length && afterTexts.every((v, i) => v === baselineTexts[i]);
  if (!contentOk) {
    return {
      success: false,
      outputPath: '',
      backupPath: null,
      error: 'Phát hiện thay đổi nội dung văn bản — thao tác bị hủy để bảo toàn 100% nội dung.',
      appliedProposalIds: [],
      skippedProposalIds: skipped,
      contentPreservedVerified: false,
      relationshipsPreservedVerified: true,
      postSaveReport: null,
      logs,
    };
  }

  entriesMap.set(docKey, encoder.encode(serializer.serializeToString(doc)));
  const dot = file.name.lastIndexOf('.');
  const stem = dot > 0 ? file.name.slice(0, dot) : file.name;
  const ext = dot > 0 ? file.name.slice(dot + 1) : 'docx';
  const outputName = `${stem}_layout.${ext}`;
  const rawOutBytes = await writeZipEntries(entriesMap);
  const exactBuf = new Uint8Array(rawOutBytes.byteLength);
  exactBuf.set(rawOutBytes);
  const outBlob = new Blob([exactBuf], { type: file.type });
  const outFile = new File([exactBuf], outputName, { type: file.type });
  const blobUrl = URL.createObjectURL(outBlob);
  const postSaveReport = await analyzeWebLayoutIntelligence(outFile, req.scope ?? 'autoDetect');

  return {
    success: true,
    outputPath: outputName,
    backupPath: req.outputMode === 'overwriteWithBackup' ? `${file.name}.bak` : null,
    error: null,
    appliedProposalIds: applied,
    skippedProposalIds: skipped,
    contentPreservedVerified: true,
    relationshipsPreservedVerified: true,
    postSaveReport,
    logs,
    blobUrl,
    outputName,
    outputBlob: outBlob,
  };
}
