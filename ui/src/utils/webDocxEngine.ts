import type { AuditIssue, AuditReport, FileProcessResult, FileQueueItem, ProcessConfig } from '../types';

const W_NS = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main';
const WEB_MAX_FILE_BYTES = 128 * 1024 * 1024;
const WEB_MAX_UNCOMPRESSED_ZIP_BYTES = 384 * 1024 * 1024;
const WEB_MAX_ZIP_ENTRIES = 5000;
const WEB_MAX_COMPRESSION_RATIO = 120;

interface ZipEntry {
  name: string;
  data: Uint8Array;
}

const CRC32_TABLE = (() => {
  const tbl = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let k = 0; k < 8; k++) {
      c = (c & 1) !== 0 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    tbl[i] = c >>> 0;
  }
  return tbl;
})();

function crc32(data: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < data.length; i++) {
    c = CRC32_TABLE[(c ^ data[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

async function transformDeflateRaw(
  data: Uint8Array,
  mode: 'inflate' | 'deflate',
): Promise<Uint8Array> {
  const copy = new Uint8Array(data.byteLength);
  copy.set(data);
  const streamCtor =
    mode === 'inflate'
      ? (window as unknown as { DecompressionStream: new (fmt: string) => GenericTransformStream }).DecompressionStream
      : (window as unknown as { CompressionStream: new (fmt: string) => GenericTransformStream }).CompressionStream;

  const transformer = new streamCtor('deflate-raw');
  const writer = transformer.writable.getWriter();
  void writer.write(copy);
  void writer.close();

  const reader = transformer.readable.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    if (value) {
      chunks.push(value);
      total += value.byteLength;
    }
  }
  const out = new Uint8Array(total);
  let offset = 0;
  for (const ch of chunks) {
    out.set(ch, offset);
    offset += ch.byteLength;
  }
  return out;
}

export async function readZipEntries(buffer: ArrayBuffer): Promise<Map<string, Uint8Array>> {
  if (buffer.byteLength > WEB_MAX_FILE_BYTES) {
    throw new Error('[ERR_RESOURCE_LIMIT] Tệp vượt quá giới hạn dung lượng 128 MB trên trình duyệt.');
  }
  const bytes = new Uint8Array(buffer);
  if (
    bytes.length >= 8 &&
    bytes[0] === 0xd0 &&
    bytes[1] === 0xcf &&
    bytes[2] === 0x11 &&
    bytes[3] === 0xe0 &&
    bytes[4] === 0xa1 &&
    bytes[5] === 0xb1 &&
    bytes[6] === 0x1a &&
    bytes[7] === 0xe1
  ) {
    const sample = bytes.subarray(0, Math.min(bytes.length, 262144));
    const utf16Text = new TextDecoder('utf-16le').decode(sample);
    if (utf16Text.includes('EncryptedPackage') || utf16Text.includes('EncryptionInfo')) {
      throw new Error(
        '[ERR_FILE_LOCKED_OR_CONFLICT] Tệp được đặt mật khẩu mã hóa (Encrypt with Password). Hãy mở bằng Word và gỡ mật khẩu trước khi chuẩn hóa.',
      );
    }
    throw new Error(
      '[ERR_DOC_CONVERT] Tệp có định dạng nhị phân Office 97-2003 (.doc/.xls). Trình duyệt chỉ hỗ trợ .docx/.docm; hãy lưu sang .docx hoặc dùng bản Windows MonWord.exe.',
    );
  }
  const view = new DataView(buffer);
  const minPos = Math.max(0, bytes.length - 65558);
  let eocdOffset = -1;
  for (let i = bytes.length - 22; i >= minPos; i--) {
    if (view.getUint32(i, true) === 0x06054b50) {
      eocdOffset = i;
      break;
    }
  }
  if (eocdOffset < 0) {
    throw new Error('[ERR_OPENXML_CORRUPT] Không tìm thấy cấu trúc ZIP OpenXML hợp lệ trong tệp.');
  }

  const totalEntries = view.getUint16(eocdOffset + 10, true);
  if (totalEntries > WEB_MAX_ZIP_ENTRIES) {
    throw new Error(`[ERR_RESOURCE_LIMIT] Số tệp trong gói ZIP (${totalEntries}) vượt giới hạn ${WEB_MAX_ZIP_ENTRIES}.`);
  }
  let cdOffset = view.getUint32(eocdOffset + 16, true);
  const decoder = new TextDecoder('utf-8');
  const map = new Map<string, Uint8Array>();
  const seenLower = new Set<string>();
  let totalInflatedBytes = 0;

  for (let i = 0; i < totalEntries; i++) {
    if (cdOffset + 46 > bytes.length || view.getUint32(cdOffset, true) !== 0x02014b50) {
      break;
    }
    const method = view.getUint16(cdOffset + 10, true);
    const compSize = view.getUint32(cdOffset + 20, true);
    const declUncompSize = view.getUint32(cdOffset + 24, true);
    const nameLen = view.getUint16(cdOffset + 28, true);
    const extraLen = view.getUint16(cdOffset + 30, true);
    const commentLen = view.getUint16(cdOffset + 32, true);
    const localOffset = view.getUint32(cdOffset + 42, true);

    const rawName = decoder.decode(bytes.subarray(cdOffset + 46, cdOffset + 46 + nameLen));
    cdOffset += 46 + nameLen + extraLen + commentLen;

    if (rawName.endsWith('/')) continue;
    if (
      !rawName ||
      rawName.includes(':') ||
      rawName.startsWith('//') ||
      rawName.startsWith('\\\\') ||
      rawName.split(/[\\/]/).some((s) => s === '..' || s === '.') ||
      /[\x00-\x1f]/.test(rawName)
    ) {
      throw new Error(`[ERR_OPENXML_CORRUPT] Đường dẫn entry ZIP không an toàn: '${rawName}'.`);
    }
    const name = rawName.replace(/^\/+/, '').replace(/\\/g, '/');
    const nameLow = name.toLowerCase();
    if (seenLower.has(nameLow)) {
      throw new Error(`[ERR_OPENXML_CORRUPT] Gói ZIP chứa entry trùng lặp bất thường: '${name}'.`);
    }
    seenLower.add(nameLow);
    if (view.getUint32(localOffset, true) !== 0x04034b50) continue;

    if (declUncompSize >= 16777216 && declUncompSize / Math.max(1, compSize) > WEB_MAX_COMPRESSION_RATIO) {
      throw new Error(`[ERR_ZIP_BOMB] Phát hiện tỷ lệ nén bất thường tại '${name}'.`);
    }

    const localNameLen = view.getUint16(localOffset + 26, true);
    const localExtraLen = view.getUint16(localOffset + 28, true);
    const dataStart = localOffset + 30 + localNameLen + localExtraLen;
    const rawSlice = bytes.subarray(dataStart, dataStart + compSize);

    let entryBytes: Uint8Array;
    if (method === 0) {
      entryBytes = new Uint8Array(rawSlice);
    } else if (method === 8) {
      entryBytes = await transformDeflateRaw(rawSlice, 'inflate');
    } else {
      throw new Error(`[ERR_OPENXML_CORRUPT] Không hỗ trợ phương thức nén ZIP #${method} trên trình duyệt.`);
    }

    if (entryBytes.byteLength >= 16777216 && entryBytes.byteLength / Math.max(1, compSize) > WEB_MAX_COMPRESSION_RATIO) {
      throw new Error(`[ERR_ZIP_BOMB] Tỷ lệ giải nén thực tế bất thường tại '${name}'.`);
    }
    totalInflatedBytes += entryBytes.byteLength;
    if (totalInflatedBytes > WEB_MAX_UNCOMPRESSED_ZIP_BYTES) {
      throw new Error('[ERR_RESOURCE_LIMIT] Dung lượng giải nén gói DOCX vượt ngưỡng 384 MB.');
    }
    if ((nameLow.endsWith('.xml') || nameLow.endsWith('.rels')) && !nameLow.startsWith('word/embeddings/')) {
      const xmlSample = decoder.decode(entryBytes.subarray(0, Math.min(entryBytes.byteLength, 16384))).toUpperCase();
      if (xmlSample.includes('<!DOCTYPE') || xmlSample.includes('<!ENTITY')) {
        throw new Error(`[ERR_XML_MALFORMED] Từ chối XML chứa khai báo DOCTYPE/ENTITY không hợp lệ tại '${name}'.`);
      }
    }
    map.set(name, entryBytes);
  }
  return map;
}

export async function writeZipEntries(entriesMap: Map<string, Uint8Array>): Promise<Uint8Array> {
  const encoder = new TextEncoder();
  const entries: ZipEntry[] = Array.from(entriesMap.entries()).map(([name, data]) => ({ name, data }));

  const localParts: Uint8Array[] = [];
  const cdParts: Uint8Array[] = [];
  let offset = 0;

  for (const entry of entries) {
    const nameBytes = encoder.encode(entry.name);
    const crc = crc32(entry.data);
    let method = 0;
    let payload = entry.data;

    if (entry.data.byteLength >= 32 && 'CompressionStream' in window) {
      try {
        const deflated = await transformDeflateRaw(entry.data, 'deflate');
        if (deflated.byteLength < entry.data.byteLength) {
          method = 8;
          payload = deflated;
        }
      } catch {
        // store uncompressed fallback
      }
    }

    const localHdr = new ArrayBuffer(30 + nameBytes.length);
    const lv = new DataView(localHdr);
    lv.setUint32(0, 0x04034b50, true);
    lv.setUint16(4, 20, true);
    lv.setUint16(6, 0x0800, true); // UTF-8 filename flag
    lv.setUint16(8, method, true);
    lv.setUint16(10, 0, true);
    lv.setUint16(12, 0x5421, true);
    lv.setUint32(14, crc, true);
    lv.setUint32(18, payload.byteLength, true);
    lv.setUint32(22, entry.data.byteLength, true);
    lv.setUint16(26, nameBytes.length, true);
    lv.setUint16(28, 0, true);
    new Uint8Array(localHdr).set(nameBytes, 30);

    const localBytes = new Uint8Array(localHdr);
    localParts.push(localBytes, payload);

    const cdHdr = new ArrayBuffer(46 + nameBytes.length);
    const cv = new DataView(cdHdr);
    cv.setUint32(0, 0x02014b50, true);
    cv.setUint16(4, 20, true);
    cv.setUint16(6, 20, true);
    cv.setUint16(8, 0x0800, true);
    cv.setUint16(10, method, true);
    cv.setUint16(12, 0, true);
    cv.setUint16(14, 0x5421, true);
    cv.setUint32(16, crc, true);
    cv.setUint32(20, payload.byteLength, true);
    cv.setUint32(24, entry.data.byteLength, true);
    cv.setUint16(28, nameBytes.length, true);
    cv.setUint16(30, 0, true);
    cv.setUint16(32, 0, true);
    cv.setUint16(34, 0, true);
    cv.setUint16(36, 0, true);
    cv.setUint32(38, 0, true);
    cv.setUint32(42, offset, true);
    new Uint8Array(cdHdr).set(nameBytes, 46);
    cdParts.push(new Uint8Array(cdHdr));

    offset += localBytes.byteLength + payload.byteLength;
  }

  let cdSize = 0;
  for (const part of cdParts) cdSize += part.byteLength;

  const eocd = new ArrayBuffer(22);
  const ev = new DataView(eocd);
  ev.setUint32(0, 0x06054b50, true);
  ev.setUint16(4, 0, true);
  ev.setUint16(6, 0, true);
  ev.setUint16(8, entries.length, true);
  ev.setUint16(10, entries.length, true);
  ev.setUint32(12, cdSize, true);
  ev.setUint32(16, offset, true);
  ev.setUint16(20, 0, true);

  const totalSize = offset + cdSize + 22;
  const out = new Uint8Array(totalSize);
  let pos = 0;
  for (const p of [...localParts, ...cdParts, new Uint8Array(eocd)]) {
    out.set(p, pos);
    pos += p.byteLength;
  }
  return out;
}

function getOrCreateWChild(doc: Document, parent: Element, localName: string): Element {
  const existing = parent.getElementsByTagNameNS(W_NS, localName);
  if (existing.length > 0 && existing[0].parentNode === parent) {
    return existing[0];
  }
  const byPrefix = parent.getElementsByTagName(`w:${localName}`);
  for (let i = 0; i < byPrefix.length; i++) {
    if (byPrefix[i].parentNode === parent) return byPrefix[i];
  }
  const el = doc.createElementNS(W_NS, `w:${localName}`);
  parent.appendChild(el);
  return el;
}

function setWAttr(el: Element, name: string, value: string): void {
  el.setAttributeNS(W_NS, `w:${name}`, value);
}

function cmToTwips(cm: number): number {
  return Math.max(100, Math.round(cm * 567));
}

function cleanVietnameseText(raw: string, cleaner: ProcessConfig['cleaner']): string {
  if (!raw) return raw;
  // Preserve pure fill-in-the-blank runs (____ or .... or multiple spaces inside blank lines)
  if (/^[_\.\-\s]{3,}$/.test(raw) && /[_\.\-]/.test(raw)) {
    return raw;
  }
  let s = raw;
  if (cleaner.fixMultipleSpaces) {
    s = s.replace(/ {2,}/g, ' ');
  }
  if (cleaner.fixPunctuationSpacing) {
    s = s.replace(/\s+([,.;:!?])/g, '$1');
    s = s.replace(/([,;])([^\s\d"'”’)\]]+)/g, '$1 $2');
  }
  if (cleaner.normalizeNd30Dates) {
    s = s.replace(/ngày\s+([1-9])\s+tháng/gi, 'ngày 0$1 tháng');
    s = s.replace(/tháng\s+([12])\s+năm/gi, 'tháng 0$1 năm');
    s = s.replace(/tháng\s+0([3-9])\s+năm/gi, 'tháng $1 năm');
  }
  if (cleaner.quoteStyle === 'curlyNd30') {
    s = s.replace(/"([^"]+)"/g, '“$1”');
  } else if (cleaner.quoteStyle === 'straightDang') {
    s = s.replace(/[“”]/g, '"');
  }
  return s;
}

function isBlankDotsLine(text: string): boolean {
  return text.includes('....') || text.includes('____') || text.includes('…');
}

function auditDocumentXml(
  fileName: string,
  filePath: string,
  ext: string,
  entries: Map<string, Uint8Array>,
): AuditReport {
  const docBytes = entries.get('word/document.xml');
  if (!docBytes) {
    throw new Error('Không tìm thấy word/document.xml (bản Web hiện hỗ trợ trực tiếp .docx/.docm).');
  }
  const decoder = new TextDecoder('utf-8');
  const docXml = decoder.decode(docBytes);
  const parser = new DOMParser();
  const doc = parser.parseFromString(docXml, 'application/xml');

  const paragraphs = Array.from(doc.getElementsByTagNameNS(W_NS, 'p'));
  const tables = Array.from(doc.getElementsByTagNameNS(W_NS, 'tbl'));
  const sections = Math.max(1, doc.getElementsByTagNameNS(W_NS, 'sectPr').length);

  let multiSpaceCount = 0;
  let emptyParaCount = 0;
  let hasLegacyEncoding = false;
  let nonTimesRuns = 0;

  for (const p of paragraphs) {
    const texts = Array.from(p.getElementsByTagNameNS(W_NS, 't'));
    const fullText = texts.map((t) => t.textContent || '').join('');
    if (!fullText.trim()) {
      emptyParaCount++;
    } else if (!isBlankDotsLine(fullText) && /  +/.test(fullText)) {
      multiSpaceCount++;
    }
    const rFonts = Array.from(p.getElementsByTagNameNS(W_NS, 'rFonts'));
    for (const rf of rFonts) {
      const f =
        rf.getAttributeNS(W_NS, 'ascii') ||
        rf.getAttribute('w:ascii') ||
        rf.getAttributeNS(W_NS, 'hAnsi') ||
        rf.getAttribute('w:hAnsi') ||
        '';
      if (/^\.Vn|^VNI-/i.test(f)) {
        hasLegacyEncoding = true;
      } else if (f && !/times new roman/i.test(f)) {
        nonTimesRuns++;
      }
    }
  }

  const hasMacros =
    ext.toLowerCase() === 'docm' ||
    Array.from(entries.keys()).some((k) => k.toLowerCase().includes('vbaproject.bin'));
  const settingsXml = entries.has('word/settings.xml')
    ? decoder.decode(entries.get('word/settings.xml')!)
    : '';
  const isProtected = /w:documentProtection/i.test(settingsXml);

  const issues: AuditIssue[] = [];
  let penalty = 0;
  if (nonTimesRuns > 0) {
    issues.push({
      category: 'Phông chữ',
      severity: 'warning',
      message: 'Có đoạn văn bản chưa dùng font chuẩn Times New Roman.',
      count: nonTimesRuns,
    });
    penalty += Math.min(20, nonTimesRuns * 2);
  }
  if (multiSpaceCount > 0) {
    issues.push({
      category: 'Chính tả & Khoảng trắng',
      severity: 'warning',
      message: 'Phát hiện nhiều khoảng trắng kép dư thừa trong nội dung.',
      count: multiSpaceCount,
    });
    penalty += Math.min(18, multiSpaceCount * 2);
  }
  if (emptyParaCount > 3) {
    issues.push({
      category: 'Dòng trống',
      severity: 'info',
      message: 'Có nhiều dòng Enter trống thừa gây thưa trang văn bản.',
      count: emptyParaCount,
    });
    penalty += Math.min(12, emptyParaCount);
  }
  if (hasLegacyEncoding) {
    issues.push({
      category: 'Bảng mã cũ',
      severity: 'error',
      message: 'Phát hiện font TCVN3 / VNI. Nên dùng bản Desktop MonWord.exe để chuyển mã chính xác 100%.',
      count: 1,
    });
    penalty += 20;
  }
  if (hasMacros) {
    issues.push({
      category: 'Macro VBA',
      severity: 'warning',
      message: 'Tài liệu chứa mã Macro VBA (.docm / vbaProject.bin).',
      count: 1,
    });
    penalty += 10;
  }
  if (isProtected) {
    issues.push({
      category: 'Bảo vệ tệp',
      severity: 'info',
      message: 'Tài liệu đang bật Restrict Editing.',
      count: 1,
    });
    penalty += 5;
  }

  return {
    filePath,
    fileName,
    fileType: ext.toUpperCase(),
    score: Math.max(20, 100 - penalty),
    sectionCount: sections,
    tableCount: tables.length,
    paragraphCount: paragraphs.length,
    hasMacros,
    isProtected,
    hasLegacyEncoding,
    issues,
  };
}

export async function auditWebDocxFile(item: FileQueueItem): Promise<AuditReport> {
  if (!item.webFile) {
    throw new Error('Không tìm thấy dữ liệu tệp trên trình duyệt.');
  }
  if (!['docx', 'docm'].includes(item.ext.toLowerCase())) {
    throw new Error('Chế độ Web hỗ trợ kiểm tra trực tiếp tệp .docx/.docm. Với file .doc/.xlsx hãy dùng MonWord Desktop.');
  }
  const buf = await item.webFile.arrayBuffer();
  const entries = await readZipEntries(buf);
  return auditDocumentXml(item.name, item.path, item.ext, entries);
}

export async function processWebDocxFile(
  item: FileQueueItem,
  config: ProcessConfig,
): Promise<FileProcessResult> {
  if (!item.webFile) {
    return {
      inputPath: item.path,
      outputPath: '',
      backupPath: null,
      success: false,
      statsSummary: '',
      logs: [],
      error: 'Không tìm thấy đối tượng tệp trên trình duyệt.',
      auditBefore: null,
      auditAfter: null,
    };
  }

  if (!['docx', 'docm'].includes(item.ext.toLowerCase())) {
    return {
      inputPath: item.path,
      outputPath: '',
      backupPath: null,
      success: false,
      statsSummary: '',
      logs: [],
      error: 'Trình duyệt hỗ trợ chuẩn hóa trực tiếp .docx/.docm. Định dạng .doc cũ hoặc .xlsx cần bản Desktop MonWord.exe.',
      auditBefore: null,
      auditAfter: null,
    };
  }

  try {
    const logs: string[] = [];
    const buf = await item.webFile.arrayBuffer();
    const entries = await readZipEntries(buf);
    const auditBefore = auditDocumentXml(item.name, item.path, item.ext, entries);

    const decoder = new TextDecoder('utf-8');
    const encoder = new TextEncoder();
    const parser = new DOMParser();
    const serializer = new XMLSerializer();

    const docBytes = entries.get('word/document.xml')!;
    const doc = parser.parseFromString(decoder.decode(docBytes), 'application/xml');
    const body = doc.getElementsByTagNameNS(W_NS, 'body')[0];

    let formattedParas = 0;
    let cleanedTexts = 0;
    let normalizedTables = 0;

    if (body) {
      const topTables = Array.from(doc.getElementsByTagNameNS(W_NS, 'tbl'));
      const firstTbl = topTables[0];

      // 1. Format paragraphs & clean Vietnamese text
      const allParas = Array.from(doc.getElementsByTagNameNS(W_NS, 'p'));
      for (let idx = 0; idx < allParas.length; idx++) {
        const p = allParas[idx];
        const inTable = Boolean(p.closest && p.closest('tbl')) || p.parentNode?.nodeName.endsWith('tc');
        const fullText = Array.from(p.getElementsByTagNameNS(W_NS, 't'))
          .map((t) => t.textContent || '')
          .join('')
          .trim();

        const isAdminTop =
          config.format.protectAdminBlocks &&
          (idx < 14 ||
            /CỘNG HÒA XÃ HỘI|ĐỘC LẬP\s*[-–]\s*TỰ DO|QUYẾT ĐỊNH|CHỦ TỊCH|GIÁM ĐỐC|Nơi nhận:/i.test(
              fullText,
            ));

        if (config.cleaner.enabled) {
          const textNodes = Array.from(p.getElementsByTagNameNS(W_NS, 't'));
          for (const tNode of textNodes) {
            const orig = tNode.textContent || '';
            const next = cleanVietnameseText(orig, config.cleaner);
            if (next !== orig) {
              tNode.textContent = next;
              if (next.startsWith(' ') || next.endsWith(' ')) {
                tNode.setAttribute('xml:space', 'preserve');
              }
              cleanedTexts++;
            }
          }
        }

        if (config.format.enabled) {
          const runs = Array.from(p.getElementsByTagNameNS(W_NS, 'r'));
          const targetPt = inTable ? config.format.tableFontSizePt : config.format.fontSizePt;
          for (const r of runs) {
            const rPr = getOrCreateWChild(doc, r, 'rPr');
            const rFonts = getOrCreateWChild(doc, rPr, 'rFonts');
            setWAttr(rFonts, 'ascii', config.format.fontName);
            setWAttr(rFonts, 'hAnsi', config.format.fontName);
            setWAttr(rFonts, 'cs', config.format.fontName);
            setWAttr(rFonts, 'eastAsia', config.format.fontName);

            if (!isAdminTop) {
              const halfPt = String(Math.round(targetPt * 2));
              const sz = getOrCreateWChild(doc, rPr, 'sz');
              setWAttr(sz, 'val', halfPt);
              const szCs = getOrCreateWChild(doc, rPr, 'szCs');
              setWAttr(szCs, 'val', halfPt);
            }
          }

          if (!inTable && !isAdminTop) {
            const pPr = getOrCreateWChild(doc, p, 'pPr');
            const jc = getOrCreateWChild(doc, pPr, 'jc');
            const curAlign = jc.getAttributeNS(W_NS, 'val') || jc.getAttribute('w:val');
            if (!curAlign) {
              setWAttr(jc, 'val', 'both');
            }
            const spacing = getOrCreateWChild(doc, pPr, 'spacing');
            setWAttr(spacing, 'before', String(Math.round(config.format.spaceBeforePt * 20)));
            setWAttr(spacing, 'after', String(Math.round(config.format.spaceAfterPt * 20)));
            setWAttr(spacing, 'line', String(Math.round(config.format.lineSpacing * 240)));
            setWAttr(spacing, 'lineRule', 'auto');

            if (fullText.length > 0 && curAlign !== 'center' && curAlign !== 'right') {
              const ind = getOrCreateWChild(doc, pPr, 'ind');
              setWAttr(ind, 'firstLine', String(cmToTwips(config.format.firstLineIndentCm)));
            }
          }
          formattedParas++;
        }
      }

      // 2. Smart Tables (keep National Header 2-col table untouched)
      if (config.format.enabled && config.format.smartTables) {
        for (const tbl of topTables) {
          const tblText = Array.from(tbl.getElementsByTagNameNS(W_NS, 't'))
            .map((t) => t.textContent || '')
            .join(' ');
          if (
            tbl === firstTbl &&
            /CỘNG HÒA|ĐỘC LẬP|ỦY BAN|UBND|CÔNG TY/i.test(tblText)
          ) {
            continue;
          }
          const rows = Array.from(tbl.getElementsByTagNameNS(W_NS, 'tr'));
          rows.forEach((tr, rIdx) => {
            const trPrList = tr.getElementsByTagNameNS(W_NS, 'trPr');
            if (trPrList.length > 0) {
              const trPr = trPrList[0];
              for (const cs of Array.from(trPr.getElementsByTagNameNS(W_NS, 'cantSplit'))) {
                trPr.removeChild(cs);
              }
              for (const th of Array.from(trPr.getElementsByTagNameNS(W_NS, 'trHeight'))) {
                trPr.removeChild(th);
              }
            }
            if (rIdx === 0 && config.format.repeatTableHeaders) {
              const trPr = getOrCreateWChild(doc, tr, 'trPr');
              getOrCreateWChild(doc, trPr, 'tblHeader');
            }
          });
          normalizedTables++;
        }
      }

      // 3. Section margins & paper size
      if (config.format.enabled) {
        const sectPrs = Array.from(doc.getElementsByTagNameNS(W_NS, 'sectPr'));
        const dims: Record<string, [number, number]> = {
          a4: [11906, 16838],
          a3: [16838, 23811],
          letter: [12240, 15840],
        };
        const [pw, ph] = dims[config.format.paperSize] || dims.a4;
        for (const sectPr of sectPrs) {
          const pgSz = getOrCreateWChild(doc, sectPr, 'pgSz');
          const orient = pgSz.getAttributeNS(W_NS, 'orient') || pgSz.getAttribute('w:orient');
          const landscape = orient === 'landscape';
          setWAttr(pgSz, 'w', String(landscape ? ph : pw));
          setWAttr(pgSz, 'h', String(landscape ? pw : ph));

          const pgMar = getOrCreateWChild(doc, sectPr, 'pgMar');
          setWAttr(pgMar, 'top', String(cmToTwips(config.format.marginTopCm)));
          setWAttr(pgMar, 'bottom', String(cmToTwips(config.format.marginBottomCm)));
          setWAttr(pgMar, 'left', String(cmToTwips(config.format.marginLeftCm)));
          setWAttr(pgMar, 'right', String(cmToTwips(config.format.marginRightCm)));
        }
      }
    }

    entries.set('word/document.xml', encoder.encode(serializer.serializeToString(doc)));

    // 4. Security unlock / Macro cleaning
    if (config.security.unlockProtection && entries.has('word/settings.xml')) {
      const sDoc = parser.parseFromString(decoder.decode(entries.get('word/settings.xml')!), 'application/xml');
      const locks = Array.from(sDoc.getElementsByTagNameNS(W_NS, 'documentProtection'));
      for (const l of locks) l.parentNode?.removeChild(l);
      entries.set('word/settings.xml', encoder.encode(serializer.serializeToString(sDoc)));
      if (locks.length > 0) logs.push('Đã gỡ Restrict Editing khỏi word/settings.xml.');
    }

    let finalExt = item.ext.toLowerCase();
    if (config.security.cleanMacros) {
      for (const key of Array.from(entries.keys())) {
        if (/vbaProject\.bin|vbaData\.xml/i.test(key)) {
          entries.delete(key);
          logs.push(`Đã gỡ mã Macro: ${key}`);
        }
      }
      if (finalExt === 'docm') finalExt = 'docx';
    }

    const auditAfter = auditDocumentXml(item.name, item.path, finalExt, entries);
    const outZip = await writeZipEntries(entries);
    const outBuffer = new Uint8Array(outZip).buffer;
    const verifiedEntries = await readZipEntries(outBuffer);
    if (!verifiedEntries.has('word/document.xml')) {
      throw new Error('[ERR_POST_SAVE_VERIFY] Tệp DOCX sau đóng gói thiếu word/document.xml.');
    }
    const outBlob = new Blob([outBuffer], {
      type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    });
    const blobUrl = URL.createObjectURL(outBlob);
    const stem = item.name.replace(/\.[^.]+$/, '');
    const outputName = `${stem}_chuanhoa.${finalExt}`;

    logs.unshift(`Xử lý cục bộ 100% trên trình duyệt: ${formattedParas} đoạn, ${normalizedTables} bảng, sửa ${cleanedTexts} cụm chữ.`);

    return {
      inputPath: item.path,
      outputPath: outputName,
      backupPath: null,
      success: true,
      statsSummary: `${formattedParas} đoạn · ${normalizedTables} bảng · Điểm ${auditBefore.score}→${auditAfter.score}`,
      logs,
      error: null,
      errorCode: null,
      auditBefore,
      auditAfter,
      blobUrl,
      outputName,
      outputBlob: outBlob,
    };
  } catch (err) {
    const rawMsg = (err as Error).message || 'Không thể xử lý tệp .docx trên trình duyệt.';
    const codeMatch = rawMsg.match(/^\[(ERR_[A-Z0-9_]+)\]/);
    return {
      inputPath: item.path,
      outputPath: '',
      backupPath: null,
      success: false,
      statsSummary: '',
      logs: [],
      error: rawMsg,
      errorCode: codeMatch ? codeMatch[1] : 'ERR_OPENXML_CORRUPT',
      auditBefore: null,
      auditAfter: null,
    };
  }
}

export function triggerBrowserDownload(blobUrl: string, fileName: string): void {
  const a = document.createElement('a');
  a.href = blobUrl;
  a.download = fileName;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

export async function shareOutputBlob(blob: Blob, fileName: string, blobUrl?: string): Promise<'shared' | 'downloaded'> {
  try {
    const file = new File([blob], fileName, {
      type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    });
    if (
      typeof navigator !== 'undefined' &&
      typeof navigator.share === 'function' &&
      (!navigator.canShare || navigator.canShare({ files: [file] }))
    ) {
      await navigator.share({
        title: fileName,
        text: 'Tài liệu đã được chuẩn hóa bằng MonWord',
        files: [file],
      });
      return 'shared';
    }
  } catch {
    // User cancelled or Web Share not permitted -> download instead
  }
  if (blobUrl) {
    triggerBrowserDownload(blobUrl, fileName);
  }
  return 'downloaded';
}
