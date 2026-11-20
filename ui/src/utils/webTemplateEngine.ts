import builtinTemplatesJson from '../data/builtinTemplates.json';
import type {
  DocumentTemplate,
  DynamicTemplateFields,
  FileQueueItem,
  HeaderBuilderConfig,
  HeaderDateMode,
  MissingFieldMarker,
  OrgSector,
  OrganizationProfile,
  StdDocScope,
  TemplateApplyRequest,
  TemplateApplyResult,
  TemplateCatalogResponse,
  TemplateCompatibilityReport,
  TemplateGroup,
  TemplatePipelineStep,
  TemplatePreviewBlock,
  TemplatePreviewResult,
  TemplateSectionBlock,
  TemplateTableBlueprint,
  TemplateVersionRecord,
} from '../types';
import { DEFAULT_HEADER_BUILDER_CONFIG } from '../types';
import { readZipEntries, writeZipEntries } from './webDocxEngine';
import { applyWebDocxHeader, createStandaloneWebHeaderDocx, inspectWebDocxHeader } from './webHeaderBuilder';
import { analyzeWebStdDocument } from './webStandardizer';

const W_NS = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main';

const USER_TEMPLATES_KEY = 'monword_user_templates_v1';
const SYS_OVERRIDES_KEY = 'monword_sys_tpl_overrides_v1';
const TPL_DEFAULTS_KEY = 'monword_tpl_defaults_v1';
const TPL_VERSIONS_KEY = 'monword_tpl_versions_v1';
const TPL_DOCX_ASSETS_KEY = 'monword_tpl_docx_assets_v1';

const initialWebTemplateTargetSnapshots = new Map<string, File>();

export function registerWebTemplateTargetSnapshot(item: FileQueueItem): void {
  if (item.webFile && !initialWebTemplateTargetSnapshots.has(item.path)) {
    initialWebTemplateTargetSnapshots.set(item.path, item.webFile);
  }
}

export function getWebTemplateTargetSnapshot(path: string): File | undefined {
  return initialWebTemplateTargetSnapshots.get(path);
}

export function getBuiltinTemplates(): DocumentTemplate[] {
  return JSON.parse(JSON.stringify(builtinTemplatesJson)) as DocumentTemplate[];
}

function readJsonStorage<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function writeJsonStorage(key: string, val: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(val));
  } catch {
    // ignore storage quota errors
  }
}

function defaultKey(group: TemplateGroup, docType: string): string {
  return `${group}:${docType.trim()}`;
}

function bumpSemverPatch(ver: string): string {
  const parts = (ver || '1.0.0')
    .trim()
    .replace(/^v/i, '')
    .split('.')
    .map((p) => parseInt(p, 10) || 0);
  if (parts.length >= 3) return `${parts[0]}.${parts[1]}.${parts[2] + 1}`;
  if (parts.length === 2) return `${parts[0]}.${parts[1]}.1`;
  return '1.0.1';
}

function extractPlaceholdersFromString(text: string, set: Set<string>): void {
  const re = /\{\{\s*([a-zA-Z0-9_]+)\s*\}\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (m[1]) set.add(m[1].trim());
  }
}

function extractPlaceholdersFromTemplate(tpl: DocumentTemplate): string[] {
  const set = new Set<string>(tpl.detectedPlaceholders || []);
  extractPlaceholdersFromString(tpl.signatureConfig.signerTitlePlaceholder, set);
  extractPlaceholdersFromString(tpl.signatureConfig.signerNamePlaceholder, set);
  extractPlaceholdersFromString(tpl.footerConfig.footerNoteTemplate, set);
  for (const r of tpl.footerConfig.defaultRecipients) extractPlaceholdersFromString(r, set);
  for (const s of tpl.contentStructure) {
    extractPlaceholdersFromString(s.title, set);
    for (const p of s.bodyParagraphs) extractPlaceholdersFromString(p, set);
    if (s.inlineTable) {
      extractPlaceholdersFromString(s.inlineTable.caption, set);
      for (const row of s.inlineTable.sampleRows) {
        for (const c of row) extractPlaceholdersFromString(c, set);
      }
    }
  }
  return Array.from(set);
}

function uint8ToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

function base64ToUint8(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function getAllDocxAssetsMap(): Record<string, string> {
  return readJsonStorage<Record<string, string>>(TPL_DOCX_ASSETS_KEY, {});
}

function saveDocxAsset(assetId: string, bytes: Uint8Array): void {
  const map = getAllDocxAssetsMap();
  map[assetId] = uint8ToBase64(bytes);
  writeJsonStorage(TPL_DOCX_ASSETS_KEY, map);
}

function removeDocxAsset(assetId: string): void {
  const map = getAllDocxAssetsMap();
  delete map[assetId];
  writeJsonStorage(TPL_DOCX_ASSETS_KEY, map);
}

function getVersionsMap(): Record<string, TemplateVersionRecord[]> {
  return readJsonStorage<Record<string, TemplateVersionRecord[]>>(TPL_VERSIONS_KEY, {});
}

function appendWebVersionRecord(templateId: string, rec: TemplateVersionRecord): void {
  const map = getVersionsMap();
  const list = map[templateId] ? [...map[templateId]] : [];
  const idx = list.findIndex((r) => r.version === rec.version);
  if (idx >= 0) {
    list[idx] = rec;
  } else {
    list.push(rec);
  }
  map[templateId] = list;
  writeJsonStorage(TPL_VERSIONS_KEY, map);
}

export function listWebTemplates(): TemplateCatalogResponse {
  const builtins = getBuiltinTemplates();
  const overrides = readJsonStorage<DocumentTemplate[]>(SYS_OVERRIDES_KEY, []);
  const users = readJsonStorage<DocumentTemplate[]>(USER_TEMPLATES_KEY, []);
  const defaults = { ...readJsonStorage<Record<string, string>>(TPL_DEFAULTS_KEY, {}) };

  const merged: DocumentTemplate[] = [];
  const seen = new Set<string>();

  for (const b of builtins) {
    const ov = overrides.find((o) => o.templateId === b.templateId);
    const active = ov ? { ...ov, isSystem: true } : b;
    seen.add(active.templateId);
    merged.push(active);
  }
  for (const u of users) {
    if (!seen.has(u.templateId)) {
      seen.add(u.templateId);
      merged.push({ ...u, isSystem: false });
    }
  }
  for (const tpl of merged) {
    const k = defaultKey(tpl.group, tpl.docType);
    if (tpl.isDefaultForDocType && !defaults[k]) {
      defaults[k] = tpl.templateId;
    }
  }
  for (const tpl of merged) {
    const k = defaultKey(tpl.group, tpl.docType);
    if (defaults[k]) {
      tpl.isDefaultForDocType = defaults[k] === tpl.templateId;
    }
    tpl.detectedPlaceholders = extractPlaceholdersFromTemplate(tpl);
  }

  return {
    templates: merged,
    customDropinDir: 'localStorage (Web/Mobile Offline)',
    defaults,
  };
}

export function getWebTemplate(templateId: string): DocumentTemplate {
  const found = listWebTemplates().templates.find((t) => t.templateId === templateId);
  if (!found) {
    throw new Error(`Không tìm thấy mẫu văn bản "${templateId}".`);
  }
  return found;
}

export function listWebTemplateVersions(templateId: string): TemplateVersionRecord[] {
  const map = getVersionsMap();
  const list = map[templateId];
  if (list && list.length > 0) return list;
  const builtin = getBuiltinTemplates().find((b) => b.templateId === templateId);
  if (builtin) {
    return [
      {
        version: builtin.version,
        savedAt: builtin.updatedAt,
        changeNote: 'Phiên bản gốc mặc định của hệ thống',
        hasDocxAsset: Boolean(builtin.sourceDocxAssetId),
        docxAssetId: builtin.sourceDocxAssetId,
        snapshot: builtin,
      },
    ];
  }
  return [];
}

export function saveWebTemplate(req: {
  template: DocumentTemplate;
  confirmSystemOverride: boolean;
  changeNote: string;
}): TemplateCatalogResponse {
  const tpl: DocumentTemplate = JSON.parse(JSON.stringify(req.template));
  if (!tpl.name.trim()) throw new Error('Tên mẫu văn bản không được để trống.');
  if (!tpl.docType.trim()) throw new Error('Loại văn bản (docType) không được để trống.');

  const nowStr = new Date().toLocaleString('vi-VN');
  const builtin = getBuiltinTemplates().find((b) => b.templateId === tpl.templateId);
  const isSystem = Boolean(builtin) || tpl.isSystem;

  if (isSystem) {
    if (!builtin) {
      throw new Error('Không thể đánh dấu mẫu người dùng thành mẫu hệ thống built-in.');
    }
    if (!req.confirmSystemOverride) {
      throw new Error(
        'Không cho phép ghi đè mẫu hệ thống khi chưa có xác nhận. Hãy chọn "Sao chép mẫu" hoặc bật xác nhận ghi đè.',
      );
    }
    const current = getWebTemplate(tpl.templateId);
    if (listWebTemplateVersions(builtin.templateId).length === 1 && current.version === '1.0.0') {
      appendWebVersionRecord(builtin.templateId, {
        version: builtin.version,
        savedAt: builtin.updatedAt,
        changeNote: 'Phiên bản gốc mặc định của hệ thống',
        hasDocxAsset: false,
        docxAssetId: null,
        snapshot: builtin,
      });
    }
    appendWebVersionRecord(current.templateId, {
      version: current.version,
      savedAt: current.updatedAt || nowStr,
      changeNote: 'Bản trước khi ghi đè mẫu hệ thống',
      hasDocxAsset: Boolean(current.sourceDocxAssetId),
      docxAssetId: current.sourceDocxAssetId,
      snapshot: current,
    });

    const nextVer = bumpSemverPatch(current.version);
    tpl.isSystem = true;
    tpl.version = nextVer;
    tpl.updatedAt = nowStr;
    tpl.detectedPlaceholders = extractPlaceholdersFromTemplate(tpl);

    const overrides = readJsonStorage<DocumentTemplate[]>(SYS_OVERRIDES_KEY, []);
    const idx = overrides.findIndex((o) => o.templateId === tpl.templateId);
    if (idx >= 0) overrides[idx] = tpl;
    else overrides.push(tpl);
    writeJsonStorage(SYS_OVERRIDES_KEY, overrides);

    appendWebVersionRecord(tpl.templateId, {
      version: nextVer,
      savedAt: nowStr,
      changeNote: req.changeNote.trim() || 'Cập nhật mẫu hệ thống (đã xác nhận)',
      hasDocxAsset: Boolean(tpl.sourceDocxAssetId),
      docxAssetId: tpl.sourceDocxAssetId,
      snapshot: tpl,
    });
    return listWebTemplates();
  }

  tpl.isSystem = false;
  if (!tpl.templateId.trim()) {
    const slug = tpl.docType.replace(/[^a-zA-Z0-9]/g, '').toLowerCase() || 'custom';
    tpl.templateId = `usr-${slug}-${Date.now()}`;
  }
  const users = readJsonStorage<DocumentTemplate[]>(USER_TEMPLATES_KEY, []);
  const idx = users.findIndex((u) => u.templateId === tpl.templateId);
  if (idx >= 0) {
    const prev = users[idx];
    appendWebVersionRecord(prev.templateId, {
      version: prev.version,
      savedAt: prev.updatedAt || nowStr,
      changeNote: 'Phiên bản trước đó',
      hasDocxAsset: Boolean(prev.sourceDocxAssetId),
      docxAssetId: prev.sourceDocxAssetId,
      snapshot: prev,
    });
    const nextVer = bumpSemverPatch(prev.version);
    tpl.version = nextVer;
    tpl.updatedAt = nowStr;
    tpl.detectedPlaceholders = extractPlaceholdersFromTemplate(tpl);
    users[idx] = tpl;
    writeJsonStorage(USER_TEMPLATES_KEY, users);
    appendWebVersionRecord(tpl.templateId, {
      version: nextVer,
      savedAt: nowStr,
      changeNote: req.changeNote.trim() || `Cập nhật mẫu lên ${nextVer}`,
      hasDocxAsset: Boolean(tpl.sourceDocxAssetId),
      docxAssetId: tpl.sourceDocxAssetId,
      snapshot: tpl,
    });
  } else {
    tpl.version = tpl.version?.trim() || '1.0.0';
    tpl.updatedAt = nowStr;
    tpl.detectedPlaceholders = extractPlaceholdersFromTemplate(tpl);
    users.push(tpl);
    writeJsonStorage(USER_TEMPLATES_KEY, users);
    appendWebVersionRecord(tpl.templateId, {
      version: tpl.version,
      savedAt: nowStr,
      changeNote: req.changeNote.trim() || 'Tạo mẫu mới',
      hasDocxAsset: Boolean(tpl.sourceDocxAssetId),
      docxAssetId: tpl.sourceDocxAssetId,
      snapshot: tpl,
    });
  }
  return listWebTemplates();
}

export function cloneWebTemplate(
  sourceId: string,
  newName?: string,
): { template: DocumentTemplate; catalog: TemplateCatalogResponse } {
  const source = JSON.parse(JSON.stringify(getWebTemplate(sourceId))) as DocumentTemplate;
  const slug = source.docType.replace(/[^a-zA-Z0-9]/g, '').toLowerCase() || 'custom';
  const newId = `usr-${slug}-${Date.now()}`;
  if (source.sourceDocxAssetId) {
    const assetMap = getAllDocxAssetsMap();
    const b64 = assetMap[source.sourceDocxAssetId];
    if (b64) {
      const newAssetId = `asset_${newId}`;
      assetMap[newAssetId] = b64;
      writeJsonStorage(TPL_DOCX_ASSETS_KEY, assetMap);
      source.sourceDocxAssetId = newAssetId;
    } else {
      source.sourceDocxAssetId = null;
    }
  }
  source.templateId = newId;
  source.name = newName?.trim() ? newName.trim() : `${source.name} (Bản sao)`;
  source.isSystem = false;
  source.isDefaultForDocType = false;
  source.version = '1.0.0';
  const catalog = saveWebTemplate({
    template: source,
    confirmSystemOverride: false,
    changeNote: `Sao chép từ mẫu ${sourceId}`,
  });
  const saved = catalog.templates.find((t) => t.templateId === newId) || source;
  return { template: saved, catalog };
}

export function deleteWebTemplate(templateId: string): TemplateCatalogResponse {
  if (getBuiltinTemplates().some((b) => b.templateId === templateId)) {
    throw new Error('Không được phép xóa mẫu mặc định của hệ thống.');
  }
  const users = readJsonStorage<DocumentTemplate[]>(USER_TEMPLATES_KEY, []);
  const target = users.find((u) => u.templateId === templateId);
  if (!target) throw new Error('Không tìm thấy mẫu người dùng cần xóa.');
  const nextUsers = users.filter((u) => u.templateId !== templateId);
  writeJsonStorage(USER_TEMPLATES_KEY, nextUsers);
  if (target.sourceDocxAssetId) removeDocxAsset(target.sourceDocxAssetId);
  const vMap = getVersionsMap();
  delete vMap[templateId];
  writeJsonStorage(TPL_VERSIONS_KEY, vMap);
  const dMap = readJsonStorage<Record<string, string>>(TPL_DEFAULTS_KEY, {});
  for (const [k, v] of Object.entries(dMap)) {
    if (v === templateId) delete dMap[k];
  }
  writeJsonStorage(TPL_DEFAULTS_KEY, dMap);
  return listWebTemplates();
}

export function setWebDefaultTemplate(templateId: string): TemplateCatalogResponse {
  const tpl = getWebTemplate(templateId);
  const dMap = readJsonStorage<Record<string, string>>(TPL_DEFAULTS_KEY, {});
  dMap[defaultKey(tpl.group, tpl.docType)] = tpl.templateId;
  writeJsonStorage(TPL_DEFAULTS_KEY, dMap);
  return listWebTemplates();
}

export function restoreWebTemplateVersion(
  templateId: string,
  targetVersion: string,
): { template: DocumentTemplate; catalog: TemplateCatalogResponse } {
  const versions = listWebTemplateVersions(templateId);
  const rec = versions.find((v) => v.version === targetVersion);
  if (!rec) throw new Error(`Không tìm thấy phiên bản ${targetVersion}.`);
  const isBuiltin = getBuiltinTemplates().some((b) => b.templateId === templateId);
  if (isBuiltin) {
    let overrides = readJsonStorage<DocumentTemplate[]>(SYS_OVERRIDES_KEY, []);
    if (targetVersion === '1.0.0') {
      overrides = overrides.filter((o) => o.templateId !== templateId);
    } else {
      const snap = { ...rec.snapshot, isSystem: true, updatedAt: new Date().toLocaleString('vi-VN') };
      const idx = overrides.findIndex((o) => o.templateId === templateId);
      if (idx >= 0) overrides[idx] = snap;
      else overrides.push(snap);
    }
    writeJsonStorage(SYS_OVERRIDES_KEY, overrides);
  } else {
    const users = readJsonStorage<DocumentTemplate[]>(USER_TEMPLATES_KEY, []);
    const snap = { ...rec.snapshot, isSystem: false, updatedAt: new Date().toLocaleString('vi-VN') };
    const idx = users.findIndex((u) => u.templateId === templateId);
    if (idx >= 0) users[idx] = snap;
    else users.push(snap);
    writeJsonStorage(USER_TEMPLATES_KEY, users);
  }
  const catalog = listWebTemplates();
  const active = catalog.templates.find((t) => t.templateId === templateId) || rec.snapshot;
  return { template: active, catalog };
}

export async function importWebDocxTemplate(
  file: File,
  meta: { name: string; group: TemplateGroup; docType: string; scope: StdDocScope },
): Promise<{ template: DocumentTemplate; catalog: TemplateCatalogResponse }> {
  const ext = file.name.split('.').pop()?.toLowerCase() || '';
  if (ext !== 'docx' && ext !== 'docm') {
    throw new Error('Chỉ hỗ trợ nhập mẫu từ tệp .docx hoặc .docm.');
  }
  const buf = await file.arrayBuffer();
  const entries = await readZipEntries(buf);
  if (!entries.has('word/document.xml')) {
    throw new Error('Tệp không có word/document.xml hợp lệ.');
  }

  const placeholders = new Set<string>();
  const dec = new TextDecoder('utf-8');
  for (const [partName, bytes] of entries.entries()) {
    const low = partName.toLowerCase();
    if (
      low === 'word/document.xml' ||
      ((low.startsWith('word/header') || low.startsWith('word/footer')) && low.endsWith('.xml'))
    ) {
      const xml = dec.decode(bytes);
      const parsed = new DOMParser().parseFromString(xml, 'application/xml');
      const paras = Array.from(parsed.getElementsByTagNameNS(W_NS, 'p'));
      for (const p of paras) {
        extractPlaceholdersFromString(p.textContent || '', placeholders);
      }
    }
  }

  const dummyItem: FileQueueItem = {
    path: file.name,
    name: file.name,
    ext,
    sizeBytes: file.size,
    webFile: file,
  };
  const report = await analyzeWebStdDocument(dummyItem, meta.scope, null);

  const slug = (meta.docType || 'customDocx').replace(/[^a-zA-Z0-9]/g, '').toLowerCase();
  const templateId = `usr-docx-${slug}-${Date.now()}`;
  const assetId = `docx_${templateId}`;
  saveDocxAsset(assetId, new Uint8Array(buf));

  const baseTpl = JSON.parse(JSON.stringify(getBuiltinTemplates()[0])) as DocumentTemplate;
  const tpl: DocumentTemplate = {
    ...baseTpl,
    templateId,
    name: meta.name.trim() || file.name.replace(/\.[^.]+$/, ''),
    group: meta.group,
    docType: meta.docType.trim() || 'customDocx',
    scope: meta.scope === 'autoDetect' ? 'adminNd30' : meta.scope,
    isSystem: false,
    isDefaultForDocType: false,
    version: '1.0.0',
    status: 'active',
    sourceDocxAssetId: assetId,
    detectedPlaceholders: Array.from(placeholders),
    description: `Mẫu nhập trực tiếp từ DOCX (${placeholders.size} trường động, giữ nguyên ${report.tables.length} bảng & ${report.assets.mediaCount} hình ảnh).`,
    contentStructure: [
      {
        sectionId: 'docx-imported-body',
        title: 'Cấu trúc DOCX gốc được bảo toàn nguyên vẹn',
        kind: 'metadataBlock',
        required: true,
        enabledByDefault: true,
        headingAlignment: 'left',
        headingBold: true,
        headingItalic: false,
        headingUppercase: false,
        bodyParagraphs: [
          `Giữ nguyên ${report.paragraphCount} đoạn, ${report.tables.length} bảng biểu, ${Math.max(report.assets.mediaCount, report.assets.drawingCount)} hình ảnh/logo và ${report.sections.length} section từ tệp ${file.name}.`,
        ],
        inlineTable: null,
      },
    ],
  };

  const catalog = saveWebTemplate({
    template: tpl,
    confirmSystemOverride: false,
    changeNote: `Nhập mẫu DOCX từ ${file.name}`,
  });
  const saved = catalog.templates.find((t) => t.templateId === templateId) || tpl;
  return { template: saved, catalog };
}

function formatNd30DateParts(dayRaw: string, monthRaw: string, yearRaw: string): string {
  const dN = parseInt(dayRaw.trim(), 10);
  const mN = parseInt(monthRaw.trim(), 10);
  const dOut = !Number.isNaN(dN) && dN >= 1 && dN <= 9 ? `0${dN}` : dayRaw.trim();
  const mOut = !Number.isNaN(mN) && (mN === 1 || mN === 2) ? `0${mN}` : monthRaw.trim();
  return `ngày ${dOut} tháng ${mOut} năm ${yearRaw.trim()}`;
}

interface ResolvedWebData {
  values: Record<string, string>;
  missingFields: MissingFieldMarker[];
  effectiveOrg: OrganizationProfile;
  headerCfg: HeaderBuilderConfig;
  soVaKyHieu: string;
  ngayThangLine: string;
  diaDanhLine: string;
}

function resolveWebTemplateData(
  tpl: DocumentTemplate,
  orgOpt: OrganizationProfile | null,
  fields: DynamicTemplateFields,
  docPlaceholders: string[],
): ResolvedWebData {
  const values: Record<string, string> = {};
  const missingFields: MissingFieldMarker[] = [];
  const seenMissing = new Set<string>();

  const pushMissing = (key: string, labelVi: string, markerText: string, requiredBy: string) => {
    if (seenMissing.has(key)) return;
    seenMissing.add(key);
    missingFields.push({ fieldKey: key, labelVi, markerText, requiredBy });
  };

  const issuingRaw = fields.tenCoQuan.trim() || orgOpt?.issuingOrg.trim() || '';
  const tenCoQuan = issuingRaw || '[CHƯA NHẬP: TÊN CƠ QUAN BAN HÀNH]';
  if (!issuingRaw) {
    pushMissing('ten_co_quan', 'Tên cơ quan / tổ chức', tenCoQuan, 'Phần đầu & nội dung');
  }
  const govOrg = fields.coQuanChuQuan.trim() || orgOpt?.governingOrg.trim() || '';
  const vietTat = fields.vietTatCoQuan.trim() || orgOpt?.agencyAbbreviation.trim() || '';

  const prefix = tpl.headerConfig.symbolPrefix.trim();
  const defSymbol = prefix ? `${prefix}-${vietTat || '[CƠ QUAN]'}` : vietTat || '[CƠ QUAN]';
  let docNumber = '[CHƯA NHẬP SỐ]';
  let docSymbol = defSymbol;
  const rawSo = fields.soVanBan.trim();
  if (!rawSo) {
    pushMissing('so_van_ban', 'Số và ký hiệu văn bản', `Số: ${docNumber}/${docSymbol}`, 'Khối Số & Ký hiệu');
  } else if (rawSo.includes('/')) {
    const idx = rawSo.indexOf('/');
    docNumber = rawSo.slice(0, idx).trim() || '[CHƯA NHẬP SỐ]';
    docSymbol = rawSo.slice(idx + 1).trim() || defSymbol;
  } else {
    const n = parseInt(rawSo, 10);
    docNumber = !Number.isNaN(n) && n >= 1 && n <= 9 ? `0${n}` : rawSo;
  }
  const soVaKyHieu = rawSo.includes('/') ? rawSo : `${docNumber}/${docSymbol}`;

  const diaRaw = fields.diaDanh.trim() || orgOpt?.defaultLocation.trim() || '';
  const diaDanh = diaRaw || '[CHƯA NHẬP: Địa danh]';
  if (!diaRaw && tpl.headerConfig.showLocationDate) {
    pushMissing('dia_danh', 'Địa danh ban hành', diaDanh, 'Địa danh');
  }

  let mDay = '[..]';
  let mMonth = '[..]';
  let mYear = '[....]';
  let ngayThangLine = 'ngày [..] tháng [..] năm [....]';
  if (fields.day.trim() && fields.month.trim() && fields.year.trim()) {
    mDay = fields.day.trim();
    mMonth = fields.month.trim();
    mYear = fields.year.trim();
    ngayThangLine = formatNd30DateParts(mDay, mMonth, mYear);
  } else if (fields.ngayThang.trim()) {
    const rawDt = fields.ngayThang.trim();
    const viM = rawDt.match(/ngày\s*(\d{1,2})\s*tháng\s*(\d{1,2})\s*năm\s*(\d{4})/i);
    const slM = rawDt.match(/^\s*(\d{1,2})\s*[/\-.]\s*(\d{1,2})\s*[/\-.]\s*(\d{4})\s*$/);
    const m = viM || slM;
    if (m) {
      mDay = m[1];
      mMonth = m[2];
      mYear = m[3];
      ngayThangLine = formatNd30DateParts(mDay, mMonth, mYear);
    } else {
      ngayThangLine = rawDt;
    }
  } else {
    pushMissing('ngay_thang', 'Ngày tháng ban hành', ngayThangLine, 'Ngày ban hành');
  }

  const chucVu = fields.chucVu.trim() || '[CHƯA NHẬP: CHỨC VỤ NGƯỜI KÝ]';
  if (!fields.chucVu.trim() && tpl.signatureConfig.layout !== 'none') {
    pushMissing('chuc_vu', 'Chức vụ người ký', chucVu, 'Khối chữ ký');
  }
  const nguoiKy = fields.nguoiKy.trim() || '[CHƯA NHẬP: Họ và tên người ký]';
  if (!fields.nguoiKy.trim() && tpl.signatureConfig.layout !== 'none') {
    pushMissing('nguoi_ky', 'Họ và tên người ký', nguoiKy, 'Khối chữ ký');
  }
  const nguoiNhan = fields.nguoiNhan.trim() || '[CHƯA NHẬP: Kính gửi / Nơi nhận]';
  if (!fields.nguoiNhan.trim()) {
    pushMissing('nguoi_nhan', 'Người nhận / Kính gửi', nguoiNhan, 'Kính gửi / Nơi nhận');
  }
  const trichYeu = fields.trichYeu.trim() || '[CHƯA NHẬP: Trích yếu nội dung]';
  if (!fields.trichYeu.trim()) {
    pushMissing('trich_yeu', 'Trích yếu nội dung', trichYeu, 'Tiêu đề / Trích yếu');
  }

  Object.assign(values, {
    ten_co_quan: tenCoQuan,
    co_quan_chu_quan: govOrg,
    viet_tat_co_quan: vietTat,
    so_van_ban: soVaKyHieu,
    dia_danh: diaDanh,
    ngay_thang: ngayThangLine,
    nguoi_ky: nguoiKy,
    chuc_vu: chucVu,
    nguoi_nhan: nguoiNhan,
    trich_yeu: trichYeu,
    phien_ban: tpl.version,
    ten_mau: tpl.name,
  });

  for (const [k, v] of Object.entries(fields.extraFields || {})) {
    const key = k.trim();
    if (!key) continue;
    if (!v.trim()) {
      const m = `[CHƯA NHẬP: ${key}]`;
      pushMissing(key, key, m, 'Trường động tùy chỉnh');
      values[key] = m;
    } else {
      values[key] = v.trim();
    }
  }

  for (const p of [...(tpl.detectedPlaceholders || []), ...docPlaceholders]) {
    if (!(p in values)) {
      const m = `[CHƯA NHẬP: ${p}]`;
      pushMissing(p, p, m, 'Trường động trong mẫu DOCX');
      values[p] = m;
    }
  }

  const sector: OrgSector =
    orgOpt?.sector || (tpl.group === 'enterprise' ? 'enterprise' : tpl.group === 'party' ? 'partyOrg' : 'stateAgency');
  const effectiveOrg: OrganizationProfile = {
    id: orgOpt?.id || 'tpl-effective',
    profileName: orgOpt?.profileName || tenCoQuan,
    sector,
    governingOrg: govOrg,
    issuingOrg: tenCoQuan,
    affiliatedUnit: orgOpt?.affiliatedUnit || '',
    departmentOrBranch: orgOpt?.departmentOrBranch || '',
    agencyAbbreviation: vietTat,
    defaultLocation: diaDanh,
    useNd30LayoutForEnterprise: orgOpt?.useNd30LayoutForEnterprise ?? true,
    showGoverningOrgDefault: orgOpt?.showGoverningOrgDefault ?? Boolean(govOrg),
    showAffiliatedUnitDefault: orgOpt?.showAffiliatedUnitDefault ?? false,
    showDepartmentDefault: orgOpt?.showDepartmentDefault ?? false,
    updatedAt: orgOpt?.updatedAt || '',
  };

  const showNational =
    tpl.headerConfig.showNationalHeader &&
    !(tpl.headerConfig.respectOrgEnterpriseLayout && effectiveOrg.sector === 'enterprise' && !effectiveOrg.useNd30LayoutForEnterprise);

  const dateMode: HeaderDateMode = 'manualDate';
  const headerCfg: HeaderBuilderConfig = {
    ...DEFAULT_HEADER_BUILDER_CONFIG,
    orgProfileId: orgOpt?.id || null,
    docType: tpl.headerConfig.baseAdminDocType,
    showGoverningOrg: tpl.headerConfig.showGoverningOrg && Boolean(govOrg) && effectiveOrg.showGoverningOrgDefault,
    showIssuingOrg: tpl.headerConfig.showIssuingOrg,
    showAffiliatedUnit: effectiveOrg.showAffiliatedUnitDefault && Boolean(effectiveOrg.affiliatedUnit),
    showDepartment: effectiveOrg.showDepartmentDefault && Boolean(effectiveOrg.departmentOrBranch),
    showNationalHeader: showNational,
    showMottoUnderline: showNational,
    showNumberSymbol: tpl.headerConfig.showNumberSymbol,
    numberMode: 'manualInput',
    docNumber,
    docSymbol,
    allowOverrideExistingNumber: true,
    allowOverrideExistingOrg: true,
    showLocationDate: tpl.headerConfig.showLocationDate,
    locationOverride: diaDanh,
    dateMode,
    manualDay: mDay,
    manualMonth: mMonth,
    manualYear: mYear,
    allowOverrideExistingDate: true,
    showDocTypeAndSubject: tpl.headerConfig.showDocTypeAndSubject,
    customDocTypeLabel: tpl.headerConfig.customDocTypeLabel,
    subjectText: trichYeu,
  };

  return {
    values,
    missingFields,
    effectiveOrg,
    headerCfg,
    soVaKyHieu,
    ngayThangLine,
    diaDanhLine: diaDanh,
  };
}

function renderTemplateStr(input: string, values: Record<string, string>): string {
  if (!input.includes('{{')) return input;
  return input.replace(/\{\{\s*([a-zA-Z0-9_]+)\s*\}\}/g, (_, key: string) => {
    const k = key.trim();
    return k in values ? values[k] : `[CHƯA NHẬP: ${k}]`;
  });
}

function resolveEnabledSections(tpl: DocumentTemplate, req: TemplateApplyRequest): TemplateSectionBlock[] {
  const hasExplicit = req.enabledSectionIds && req.enabledSectionIds.length > 0;
  const set = new Set(req.enabledSectionIds || []);
  return tpl.contentStructure.filter((s) => (s.required ? true : hasExplicit ? set.has(s.sectionId) : s.enabledByDefault));
}

function elW(doc: Document, local: string): Element {
  return doc.createElementNS(W_NS, `w:${local}`);
}

function buildParaEl(
  doc: Document,
  text: string,
  font: string,
  pt: number,
  bold: boolean,
  italic: boolean,
  jc: string,
  firstLineTwips: number,
  beforeTwips: number,
  afterTwips: number,
  lineTwips = 276,
): Element {
  const p = elW(doc, 'p');
  const pPr = elW(doc, 'pPr');
  const sp = elW(doc, 'spacing');
  sp.setAttribute('w:before', String(beforeTwips));
  sp.setAttribute('w:after', String(afterTwips));
  sp.setAttribute('w:line', String(lineTwips));
  sp.setAttribute('w:lineRule', 'auto');
  pPr.appendChild(sp);

  const ind = elW(doc, 'ind');
  ind.setAttribute('w:left', '0');
  ind.setAttribute('w:right', '0');
  ind.setAttribute('w:firstLine', String(Math.max(0, firstLineTwips)));
  pPr.appendChild(ind);

  const j = elW(doc, 'jc');
  j.setAttribute('w:val', jc);
  pPr.appendChild(j);
  p.appendChild(pPr);

  const r = elW(doc, 'r');
  const rPr = elW(doc, 'rPr');
  const rf = elW(doc, 'rFonts');
  rf.setAttribute('w:ascii', font);
  rf.setAttribute('w:hAnsi', font);
  rf.setAttribute('w:cs', font);
  rPr.appendChild(rf);
  if (bold) {
    rPr.appendChild(elW(doc, 'b'));
    rPr.appendChild(elW(doc, 'bCs'));
  }
  if (italic) {
    rPr.appendChild(elW(doc, 'i'));
    rPr.appendChild(elW(doc, 'iCs'));
  }
  const col = elW(doc, 'color');
  col.setAttribute('w:val', '000000');
  rPr.appendChild(col);
  const half = String(Math.round(pt * 2));
  const sz = elW(doc, 'sz');
  sz.setAttribute('w:val', half);
  const szCs = elW(doc, 'szCs');
  szCs.setAttribute('w:val', half);
  rPr.appendChild(sz);
  rPr.appendChild(szCs);
  r.appendChild(rPr);

  const t = elW(doc, 't');
  if (text.startsWith(' ') || text.endsWith(' ') || text.includes('  ')) {
    t.setAttribute('xml:space', 'preserve');
  }
  t.textContent = text;
  r.appendChild(t);
  p.appendChild(r);
  return p;
}

function buildDataTableEl(
  doc: Document,
  tb: TemplateTableBlueprint,
  tpl: DocumentTemplate,
  printableTwips: number,
  renderFn: (s: string) => string,
): Element {
  const n = Math.max(1, tb.columns.length);
  const totalPct = tb.columns.reduce((acc, c) => acc + Math.max(1, c.widthPct), 0) || n * 25;
  const colWidths: number[] = [];
  let acc = 0;
  for (let i = 0; i < n; i++) {
    if (i === n - 1) {
      colWidths.push(Math.max(400, printableTwips - acc));
    } else {
      const w = Math.max(400, Math.round((printableTwips * Math.max(1, tb.columns[i].widthPct)) / totalPct));
      acc += w;
      colWidths.push(w);
    }
  }

  const tbl = elW(doc, 'tbl');
  const tblPr = elW(doc, 'tblPr');
  const tblW = elW(doc, 'tblW');
  tblW.setAttribute('w:w', String(printableTwips));
  tblW.setAttribute('w:type', 'dxa');
  tblPr.appendChild(tblW);
  const jc = elW(doc, 'jc');
  jc.setAttribute('w:val', 'center');
  tblPr.appendChild(jc);
  const borders = elW(doc, 'tblBorders');
  for (const side of ['top', 'left', 'bottom', 'right', 'insideH', 'insideV']) {
    const b = elW(doc, side);
    b.setAttribute('w:val', tpl.tableConfig.borderStyle || 'single');
    b.setAttribute('w:sz', String(Math.max(4, tpl.tableConfig.borderSizeEighthsPt)));
    b.setAttribute('w:space', '0');
    b.setAttribute('w:color', '000000');
    borders.appendChild(b);
  }
  tblPr.appendChild(borders);
  const layout = elW(doc, 'tblLayout');
  layout.setAttribute('w:type', 'fixed');
  tblPr.appendChild(layout);
  tbl.appendChild(tblPr);

  const grid = elW(doc, 'tblGrid');
  for (const w of colWidths) {
    const gc = elW(doc, 'gridCol');
    gc.setAttribute('w:w', String(w));
    grid.appendChild(gc);
  }
  tbl.appendChild(grid);

  const hdrTr = elW(doc, 'tr');
  const hdrTrPr = elW(doc, 'trPr');
  if (tpl.tableConfig.preventRowSplit) hdrTrPr.appendChild(elW(doc, 'cantSplit'));
  if (tpl.tableConfig.repeatHeaderRow) hdrTrPr.appendChild(elW(doc, 'tblHeader'));
  hdrTr.appendChild(hdrTrPr);
  for (let i = 0; i < n; i++) {
    const tc = elW(doc, 'tc');
    const tcPr = elW(doc, 'tcPr');
    const tcW = elW(doc, 'tcW');
    tcW.setAttribute('w:w', String(colWidths[i]));
    tcW.setAttribute('w:type', 'dxa');
    tcPr.appendChild(tcW);
    const shd = elW(doc, 'shd');
    shd.setAttribute('w:val', 'clear');
    shd.setAttribute('w:color', 'auto');
    shd.setAttribute('w:fill', 'F1F5F9');
    tcPr.appendChild(shd);
    tc.appendChild(tcPr);
    tc.appendChild(
      buildParaEl(
        doc,
        renderFn(tb.columns[i]?.header || ''),
        tpl.tableConfig.defaultFontName,
        tpl.tableConfig.defaultFontSizePt,
        tpl.tableConfig.headerBold,
        false,
        'center',
        0,
        40,
        40,
        240,
      ),
    );
    hdrTr.appendChild(tc);
  }
  tbl.appendChild(hdrTr);

  const rows = tb.sampleRows.length > 0 ? tb.sampleRows : [new Array(n).fill('')];
  for (const row of rows) {
    const tr = elW(doc, 'tr');
    const trPr = elW(doc, 'trPr');
    if (tpl.tableConfig.preventRowSplit) trPr.appendChild(elW(doc, 'cantSplit'));
    tr.appendChild(trPr);
    for (let i = 0; i < n; i++) {
      const tc = elW(doc, 'tc');
      const tcPr = elW(doc, 'tcPr');
      const tcW = elW(doc, 'tcW');
      tcW.setAttribute('w:w', String(colWidths[i]));
      tcW.setAttribute('w:type', 'dxa');
      tcPr.appendChild(tcW);
      tc.appendChild(tcPr);
      tc.appendChild(
        buildParaEl(
          doc,
          renderFn(row[i] || ''),
          tpl.tableConfig.defaultFontName,
          tpl.tableConfig.defaultFontSizePt,
          false,
          false,
          tb.columns[i]?.align || 'left',
          0,
          40,
          40,
          240,
        ),
      );
      tr.appendChild(tc);
    }
    tbl.appendChild(tr);
  }

  return tbl;
}

function buildSignatureTableEl(
  doc: Document,
  tpl: DocumentTemplate,
  resolved: ResolvedWebData,
  printableTwips: number,
  rawMasterMode: boolean,
): Element {
  const font = tpl.formatRules.fontFamily;
  const rFn = (s: string) => (rawMasterMode ? s : renderTemplateStr(s, resolved.values));
  const signerTitle = rFn(tpl.signatureConfig.signerTitlePlaceholder).toUpperCase();
  const signerName = rFn(tpl.signatureConfig.signerNamePlaceholder);
  const blanks = Math.max(2, Math.min(5, tpl.signatureConfig.blankLinesForSeal));

  type CellSpec = { w: number; lines: [string, number, boolean, boolean, string][] };
  let specs: CellSpec[] = [];

  if (tpl.signatureConfig.layout === 'dualSecretaryChair') {
    const lw = Math.floor(printableTwips / 2);
    const leftLines: [string, number, boolean, boolean, string][] = [
      [(tpl.signatureConfig.leftColumnTitle || 'THƯ KÝ').toUpperCase(), 13.5, true, false, 'center'],
      ['(Ký, ghi rõ họ tên)', 12.5, false, true, 'center'],
    ];
    const rightLines: [string, number, boolean, boolean, string][] = [
      [signerTitle.includes('CHƯA NHẬP') ? 'CHỦ TỌA / CHỦ TRÌ' : signerTitle, 13.5, true, false, 'center'],
      ['(Ký, ghi rõ họ tên)', 12.5, false, true, 'center'],
    ];
    for (let k = 0; k < blanks; k++) {
      leftLines.push(['', 13, false, false, 'center']);
      rightLines.push(['', 13, false, false, 'center']);
    }
    leftLines.push(['[Họ và tên thư ký]', 13.5, true, false, 'center']);
    rightLines.push([signerName, 13.5, true, false, 'center']);
    specs = [
      { w: lw, lines: leftLines },
      { w: printableTwips - lw, lines: rightLines },
    ];
  } else if (tpl.signatureConfig.layout === 'tripleApproval') {
    const cw = Math.floor(printableTwips / 3);
    const c1: [string, number, boolean, boolean, string][] = [
      [(tpl.signatureConfig.leftColumnTitle || 'NGƯỜI ĐỀ XUẤT').toUpperCase(), 13, true, false, 'center'],
      ['(Ký, họ tên)', 12, false, true, 'center'],
    ];
    const c2: [string, number, boolean, boolean, string][] = [
      [(tpl.signatureConfig.middleColumnTitle || 'THẨM ĐỊNH').toUpperCase(), 13, true, false, 'center'],
      ['(Ý kiến & ký)', 12, false, true, 'center'],
    ];
    const c3: [string, number, boolean, boolean, string][] = [
      [signerTitle, 13, true, false, 'center'],
      ['(Ký duyệt)', 12, false, true, 'center'],
    ];
    for (let k = 0; k < blanks; k++) {
      c1.push(['', 13, false, false, 'center']);
      c2.push(['', 13, false, false, 'center']);
      c3.push(['', 13, false, false, 'center']);
    }
    c3.push([signerName, 13, true, false, 'center']);
    specs = [
      { w: cw, lines: c1 },
      { w: cw, lines: c2 },
      { w: printableTwips - cw * 2, lines: c3 },
    ];
  } else {
    const lw = Math.round(printableTwips * 0.45);
    const leftLines: [string, number, boolean, boolean, string][] = [];
    if (tpl.footerConfig.showRecipientList) {
      leftLines.push([tpl.footerConfig.recipientHeaderLabel || 'Nơi nhận:', 12, true, true, 'left']);
      for (const rec of tpl.footerConfig.defaultRecipients) {
        leftLines.push([rFn(rec), 11, false, false, 'left']);
      }
    }
    if (leftLines.length === 0) leftLines.push(['', 12, false, false, 'left']);
    const rightLines: [string, number, boolean, boolean, string][] = [];
    if (tpl.signatureConfig.layout === 'singleRight') {
      rightLines.push([signerTitle, 13.5, true, false, 'center']);
      rightLines.push(['(Ký, ghi rõ họ tên và đóng dấu)', 12.5, false, true, 'center']);
      for (let k = 0; k < blanks; k++) rightLines.push(['', 13, false, false, 'center']);
      rightLines.push([signerName, 13.5, true, false, 'center']);
    } else {
      rightLines.push(['', 12, false, false, 'center']);
    }
    specs = [
      { w: lw, lines: leftLines },
      { w: printableTwips - lw, lines: rightLines },
    ];
  }

  const tbl = elW(doc, 'tbl');
  const tblPr = elW(doc, 'tblPr');
  const tblW = elW(doc, 'tblW');
  tblW.setAttribute('w:w', String(printableTwips));
  tblW.setAttribute('w:type', 'dxa');
  tblPr.appendChild(tblW);
  const borders = elW(doc, 'tblBorders');
  for (const side of ['top', 'left', 'bottom', 'right', 'insideH', 'insideV']) {
    const b = elW(doc, side);
    b.setAttribute('w:val', 'none');
    b.setAttribute('w:sz', '0');
    b.setAttribute('w:space', '0');
    b.setAttribute('w:color', 'auto');
    borders.appendChild(b);
  }
  tblPr.appendChild(borders);
  tbl.appendChild(tblPr);

  const grid = elW(doc, 'tblGrid');
  for (const s of specs) {
    const gc = elW(doc, 'gridCol');
    gc.setAttribute('w:w', String(s.w));
    grid.appendChild(gc);
  }
  tbl.appendChild(grid);

  const tr = elW(doc, 'tr');
  for (const s of specs) {
    const tc = elW(doc, 'tc');
    const tcPr = elW(doc, 'tcPr');
    const tcW = elW(doc, 'tcW');
    tcW.setAttribute('w:w', String(s.w));
    tcW.setAttribute('w:type', 'dxa');
    tcPr.appendChild(tcW);
    tc.appendChild(tcPr);
    for (const [txt, pt, b, i, jc] of s.lines) {
      tc.appendChild(buildParaEl(doc, txt, font, pt, b, i, jc, 0, 20, 40, 240));
    }
    tr.appendChild(tc);
  }
  tbl.appendChild(tr);
  return tbl;
}

function isPureTextRunDom(node: Node): node is Element {
  if (node.nodeType !== 1) return false;
  const el = node as Element;
  if (el.localName !== 'r') return false;
  let hasT = false;
  for (const ch of Array.from(el.childNodes)) {
    if (ch.nodeType !== 1) continue;
    const ln = (ch as Element).localName;
    if (ln === 'rPr') continue;
    if (ln === 't') hasT = true;
    else return false;
  }
  return hasT;
}

function substitutePlaceholdersInXmlDom(doc: Document, values: Record<string, string>): number {
  let hits = 0;
  const containers: Element[] = [
    ...Array.from(doc.getElementsByTagNameNS(W_NS, 'p')),
    ...Array.from(doc.getElementsByTagNameNS(W_NS, 'hyperlink')),
  ];
  for (const p of containers) {
    const children = Array.from(p.childNodes);
    let idx = 0;
    const toRemove: Element[] = [];
    while (idx < children.length) {
      if (!isPureTextRunDom(children[idx])) {
        idx++;
        continue;
      }
      const runGroup: Element[] = [];
      while (idx < children.length) {
        const ch = children[idx];
        if (isPureTextRunDom(ch)) {
          runGroup.push(ch);
          idx++;
        } else if (ch.nodeType === 3 && !(ch.textContent || '').trim()) {
          idx++;
        } else {
          break;
        }
      }
      const segments = runGroup.map((r) => r.textContent || '');
      const combined = segments.join('');
      if (!combined.includes('{{')) continue;

      const re = /\{\{\s*([a-zA-Z0-9_]+)\s*\}\}/g;
      const matches: { start: number; end: number; key: string }[] = [];
      let m: RegExpExecArray | null;
      while ((m = re.exec(combined)) !== null) {
        matches.push({ start: m.index, end: m.index + m[0].length, key: m[1].trim() });
      }
      if (matches.length === 0) continue;

      const starts: number[] = [];
      let cur = 0;
      for (const s of segments) {
        starts.push(cur);
        cur += s.length;
      }
      for (let k = matches.length - 1; k >= 0; k--) {
        const { start, end, key } = matches[k];
        const repl = key in values ? values[key] : `[CHƯA NHẬP: ${key}]`;
        let fSeg = 0;
        for (let sIdx = starts.length - 1; sIdx >= 0; sIdx--) {
          if (starts[sIdx] <= start) {
            fSeg = sIdx;
            break;
          }
        }
        let lSeg = fSeg;
        for (let sIdx = starts.length - 1; sIdx >= 0; sIdx--) {
          if (starts[sIdx] < end) {
            lSeg = sIdx;
            break;
          }
        }
        const lStart = start - starts[fSeg];
        const lEnd = end - starts[lSeg];
        if (fSeg === lSeg) {
          segments[fSeg] = segments[fSeg].slice(0, lStart) + repl + segments[fSeg].slice(lEnd);
        } else {
          const tail = segments[lSeg].slice(lEnd);
          segments[fSeg] = segments[fSeg].slice(0, lStart) + repl;
          for (let mid = fSeg + 1; mid < lSeg; mid++) segments[mid] = '';
          segments[lSeg] = tail;
        }
        hits++;
      }

      for (let offset = 0; offset < runGroup.length; offset++) {
        const rEl = runGroup[offset];
        const segTxt = segments[offset];
        if (!segTxt && offset > 0) {
          toRemove.push(rEl);
          continue;
        }
        for (const ch of Array.from(rEl.childNodes)) {
          if (ch.nodeType === 1 && (ch as Element).localName !== 'rPr') {
            rEl.removeChild(ch);
          }
        }
        const t = elW(doc, 't');
        if (segTxt.startsWith(' ') || segTxt.endsWith(' ') || segTxt.includes('  ')) {
          t.setAttribute('xml:space', 'preserve');
        }
        t.textContent = segTxt;
        rEl.appendChild(t);
      }
    }
    for (const rEl of toRemove) {
      if (rEl.parentNode === p) p.removeChild(rEl);
    }
  }
  return hits;
}

async function generateOrApplyWebPackage(
  tpl: DocumentTemplate,
  req: TemplateApplyRequest,
  resolved: ResolvedWebData,
  targetItem: FileQueueItem | undefined,
  rawMasterMode: boolean,
  logs: string[],
): Promise<{ entries: Map<string, Uint8Array>; outBlob: Blob; outputName: string }> {
  const enc = new TextEncoder();
  const dec = new TextDecoder('utf-8');
  let entries: Map<string, Uint8Array>;
  let isMasterAsset = false;

  if (req.targetMode === 'applyToExistingDocx' && targetItem?.webFile) {
    entries = await readZipEntries(await targetItem.webFile.arrayBuffer());
  } else if (tpl.sourceDocxAssetId) {
    const b64 = getAllDocxAssetsMap()[tpl.sourceDocxAssetId];
    if (!b64) throw new Error('Thiếu dữ liệu DOCX gốc của mẫu nhập ngoài.');
    const rawBytes = base64ToUint8(b64);
    entries = await readZipEntries(new Uint8Array(rawBytes).buffer);
    isMasterAsset = true;
  } else {
    // Reuse Phase 2 Standalone Header Builder to scaffold OOXML package & Header
    const baseRes = await createStandaloneWebHeaderDocx(
      resolved.headerCfg,
      resolved.effectiveOrg,
      `${tpl.docType}_MonWord.docx`,
    );
    if (!baseRes.outputBlob) throw new Error('Không tạo được khung OOXML từ mẫu.');
    entries = await readZipEntries(await baseRes.outputBlob.arrayBuffer());

    const docXml = dec.decode(entries.get('word/document.xml')!);
    const doc = new DOMParser().parseFromString(docXml, 'application/xml');
    const body = doc.getElementsByTagNameNS(W_NS, 'body')[0];
    const sectPr = body.getElementsByTagNameNS(W_NS, 'sectPr')[0];

    // Configure A4/Letter/A3 & margins from template.formatRules
    const pgSz = Array.from(sectPr.childNodes).find((n): n is Element => n.nodeType === 1 && (n as Element).localName === 'pgSz');
    if (pgSz) {
      const dims = tpl.formatRules.paperSize === 'a3' ? ['16838', '23811'] : tpl.formatRules.paperSize === 'letter' ? ['12240', '15840'] : ['11906', '16838'];
      pgSz.setAttribute('w:w', dims[0]);
      pgSz.setAttribute('w:h', dims[1]);
    }
    const pgMar = Array.from(sectPr.childNodes).find((n): n is Element => n.nodeType === 1 && (n as Element).localName === 'pgMar');
    if (pgMar) {
      const mmToTwips = (mm: number) => String(Math.round((mm / 10) * 566.9291));
      pgMar.setAttribute('w:top', mmToTwips(tpl.formatRules.marginTopMm));
      pgMar.setAttribute('w:bottom', mmToTwips(tpl.formatRules.marginBottomMm));
      pgMar.setAttribute('w:left', mmToTwips(tpl.formatRules.marginLeftMm));
      pgMar.setAttribute('w:right', mmToTwips(tpl.formatRules.marginRightMm));
    }

    // Remove trailing empty stub paragraph created by standalone header scaffold
    for (const child of Array.from(body.childNodes)) {
      if (child.nodeType === 1 && (child as Element).localName === 'p') {
        const txt = (child.textContent || '').trim();
        const hasBdr = (child as Element).getElementsByTagNameNS(W_NS, 'pBdr').length > 0;
        if (!txt && !hasBdr) body.removeChild(child);
      }
    }

    const rFn = (s: string) => (rawMasterMode ? s : renderTemplateStr(s, resolved.values));
    const printableTwips = 9071;

    if (tpl.headerConfig.includeControlBox) {
      const ctrlBp: TemplateTableBlueprint = {
        caption: '',
        columns: [
          { header: 'KIỂM SOÁT TÀI LIỆU', widthPct: 50, align: 'left' },
          { header: 'THÔNG TIN HIỆU LỰC', widthPct: 50, align: 'left' },
        ],
        sampleRows: [
          [`Mã tài liệu: ${rawMasterMode ? '{{so_van_ban}}' : resolved.soVaKyHieu}`, `Phiên bản: v${tpl.version}`],
          [`Đơn vị: ${rawMasterMode ? '{{ten_co_quan}}' : resolved.effectiveOrg.issuingOrg}`, `Hiệu lực: ${rawMasterMode ? '{{ngay_thang}}' : resolved.ngayThangLine}`],
        ],
      };
      body.insertBefore(buildDataTableEl(doc, ctrlBp, tpl, printableTwips, (s) => s), sectPr);
    }

    for (const sec of resolveEnabledSections(tpl, req)) {
      if (sec.title.trim()) {
        const tTxt = sec.headingUppercase ? rFn(sec.title).toUpperCase() : rFn(sec.title);
        body.insertBefore(
          buildParaEl(doc, tTxt, tpl.formatRules.fontFamily, tpl.formatRules.bodyFontSizePt, sec.headingBold, sec.headingItalic, sec.headingAlignment || 'left', 0, 100, 60),
          sectPr,
        );
      }
      for (const pLine of sec.bodyParagraphs) {
        const jc =
          sec.kind === 'recipientSalutation' || sec.kind === 'authorityHeading' || sec.kind === 'decisionKeyword'
            ? 'center'
            : sec.kind === 'metadataBlock'
              ? 'left'
              : tpl.formatRules.bodyAlignment || 'both';
        const bold = sec.kind === 'authorityHeading' || sec.kind === 'decisionKeyword';
        const italic = sec.kind === 'preambleBasis';
        const ind = jc === 'center' || sec.kind === 'metadataBlock' ? 0 : Math.round(tpl.formatRules.firstLineIndentCm * 566.93);
        body.insertBefore(
          buildParaEl(doc, rFn(pLine), tpl.formatRules.fontFamily, tpl.formatRules.bodyFontSizePt, bold, italic, jc, ind, 40, 80),
          sectPr,
        );
      }
      if (sec.inlineTable) {
        if (sec.inlineTable.caption && sec.inlineTable.caption !== sec.title) {
          body.insertBefore(buildParaEl(doc, rFn(sec.inlineTable.caption), tpl.formatRules.fontFamily, 13, true, true, 'center', 0, 80, 60), sectPr);
        }
        body.insertBefore(buildDataTableEl(doc, sec.inlineTable, tpl, printableTwips, rFn), sectPr);
      }
    }

    if (tpl.footerConfig.showRecipientList || tpl.signatureConfig.layout !== 'none') {
      body.insertBefore(buildParaEl(doc, '', tpl.formatRules.fontFamily, 13, false, false, 'left', 0, 0, 80), sectPr);
      body.insertBefore(buildSignatureTableEl(doc, tpl, resolved, printableTwips, rawMasterMode), sectPr);
    }

    entries.set('word/document.xml', enc.encode(new XMLSerializer().serializeToString(doc)));
    logs.push(`Đã khởi tạo tài liệu DOCX theo mẫu "${tpl.name}".`);
  }

  // Run-coalesced placeholder substitution on existing/imported DOCX
  if (req.targetMode === 'applyToExistingDocx' || isMasterAsset) {
    let totalHits = 0;
    for (const [partName, bytes] of entries.entries()) {
      const low = partName.toLowerCase();
      if (
        low === 'word/document.xml' ||
        ((low.startsWith('word/header') || low.startsWith('word/footer')) && low.endsWith('.xml'))
      ) {
        const xmlDoc = new DOMParser().parseFromString(dec.decode(bytes), 'application/xml');
        const h = substitutePlaceholdersInXmlDom(xmlDoc, resolved.values);
        if (h > 0) {
          totalHits += h;
          entries.set(partName, enc.encode(new XMLSerializer().serializeToString(xmlDoc)));
        }
      }
    }
    if (totalHits > 0) {
      logs.push(`Đã thay thế ${totalHits} trường động {{...}} mà vẫn bảo toàn định dạng & cấu trúc gốc.`);
    }
  }

  // If applying onto an existing user DOCX, prepend/standardize header safely via Phase 2 & insert framing signature if missing
  if (req.targetMode === 'applyToExistingDocx' && targetItem && !isMasterAsset && tpl.headerConfig.enabled) {
    const curBytes = await writeZipEntries(entries);
    const curCopy = new Uint8Array(curBytes.byteLength);
    curCopy.set(curBytes);
    const curFile = new File([curCopy], targetItem.name, { type: targetItem.webFile?.type || 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    const tempItem: FileQueueItem = { ...targetItem, webFile: curFile };
    const insp = await inspectWebDocxHeader(tempItem, resolved.headerCfg, resolved.effectiveOrg);
    const activeCand = insp.candidates[insp.activeCandidateIndex ?? 0];
    if (activeCand?.layoutKind !== 'customTemplateWithLogo') {
      const mode = insp.candidates.length === 0 ? 'createNew' : 'standardizeExisting';
      const hdrRes = await applyWebDocxHeader(tempItem, resolved.headerCfg, mode, resolved.effectiveOrg);
      if (hdrRes.success && hdrRes.outputBlob) {
        entries = await readZipEntries(await hdrRes.outputBlob.arrayBuffer());
        logs.push(...hdrRes.logs);
      }
    } else {
      logs.push('Giữ nguyên khối đầu trang tùy biến có logo trong tài liệu gốc.');
    }

    if (req.insertFramingOnExisting && tpl.signatureConfig.layout !== 'none') {
      const dXml = dec.decode(entries.get('word/document.xml')!);
      const dDoc = new DOMParser().parseFromString(dXml, 'application/xml');
      const txtLower = (dDoc.documentElement.textContent || '').toLowerCase();
      if (!txtLower.includes('nơi nhận:') && !txtLower.includes('(ký')) {
        const b = dDoc.getElementsByTagNameNS(W_NS, 'body')[0];
        const sects = Array.from(b.getElementsByTagNameNS(W_NS, 'sectPr'));
        const tailSect = sects[sects.length - 1];
        b.insertBefore(buildSignatureTableEl(dDoc, tpl, resolved, 9071, false), tailSect || null);
        entries.set('word/document.xml', enc.encode(new XMLSerializer().serializeToString(dDoc)));
        logs.push('Đã bổ sung khối Nơi nhận & Chữ ký theo mẫu vào cuối tài liệu.');
      }
    }
  }

  const outBytes = await writeZipEntries(entries);
  const copyBuf = new Uint8Array(outBytes.byteLength);
  copyBuf.set(outBytes);
  const ext = targetItem?.ext || 'docx';
  const stem = targetItem ? targetItem.name.replace(/\.[^.]+$/, '') : tpl.docType;
  const outputName =
    req.targetMode === 'applyToExistingDocx'
      ? `${stem}_TheoMau.${ext}`
      : `${tpl.docType}_MonWord.docx`;
  const mime =
    ext.toLowerCase() === 'docm'
      ? 'application/vnd.ms-word.document.macroEnabled.12'
      : 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';

  return {
    entries,
    outBlob: new Blob([copyBuf], { type: mime }),
    outputName,
  };
}

export async function exportWebDocxTemplate(
  templateId: string,
): Promise<{ blob: Blob; blobUrl: string; fileName: string }> {
  const tpl = getWebTemplate(templateId);
  const decMap = getAllDocxAssetsMap();
  if (tpl.sourceDocxAssetId && decMap[tpl.sourceDocxAssetId]) {
    const raw = base64ToUint8(decMap[tpl.sourceDocxAssetId]);
    const copy = new Uint8Array(raw.byteLength);
    copy.set(raw);
    const blob = new Blob([copy], {
      type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    });
    return {
      blob,
      blobUrl: URL.createObjectURL(blob),
      fileName: `${tpl.docType}_MasterTemplate.docx`,
    };
  }
  const literalFields: DynamicTemplateFields = {
    tenCoQuan: '{{ten_co_quan}}',
    coQuanChuQuan: '{{co_quan_chu_quan}}',
    vietTatCoQuan: '{{viet_tat_co_quan}}',
    soVanBan: '{{so_van_ban}}',
    diaDanh: '{{dia_danh}}',
    ngayThang: '{{ngay_thang}}',
    day: '',
    month: '',
    year: '',
    nguoiKy: '{{nguoi_ky}}',
    chucVu: '{{chuc_vu}}',
    nguoiNhan: '{{nguoi_nhan}}',
    trichYeu: '{{trich_yeu}}',
    extraFields: {},
  };
  const resolved = resolveWebTemplateData(tpl, null, literalFields, []);
  const req: TemplateApplyRequest = {
    templateId: tpl.templateId,
    orgProfileId: null,
    orgProfileOverride: null,
    targetMode: 'createNewDocx',
    targetPath: null,
    destPath: null,
    outputMode: 'newCopy',
    enabledSectionIds: [],
    dynamicFields: literalFields,
    insertFramingOnExisting: false,
  };
  const { outBlob } = await generateOrApplyWebPackage(tpl, req, resolved, undefined, true, []);
  return {
    blob: outBlob,
    blobUrl: URL.createObjectURL(outBlob),
    fileName: `${tpl.docType}_MasterTemplate.docx`,
  };
}

export async function previewWebTemplateApplication(
  req: TemplateApplyRequest,
  orgProfile: OrganizationProfile | null,
  targetItem?: FileQueueItem,
): Promise<TemplatePreviewResult> {
  const tpl = getWebTemplate(req.templateId);
  let docPlaceholders: string[] = [];
  if (req.targetMode === 'applyToExistingDocx' && targetItem?.webFile) {
    const entries = await readZipEntries(await targetItem.webFile.arrayBuffer());
    const dec = new TextDecoder('utf-8');
    const set = new Set<string>();
    for (const [k, b] of entries.entries()) {
      if (k.toLowerCase().endsWith('.xml')) {
        extractPlaceholdersFromString(dec.decode(b), set);
      }
    }
    docPlaceholders = Array.from(set);
  }
  const resolved = resolveWebTemplateData(tpl, orgProfile, req.dynamicFields, docPlaceholders);
  const logs: string[] = [];
  const { outBlob, outputName } = await generateOrApplyWebPackage(tpl, req, resolved, targetItem, false, logs);

  const fileObj = new File([outBlob], outputName, { type: outBlob.type });
  const stdReport = await analyzeWebStdDocument(
    { path: outputName, name: outputName, ext: 'docx', sizeBytes: fileObj.size, webFile: fileObj },
    tpl.scope,
    resolved.effectiveOrg,
  );

  const headerLeftLines: string[] = [];
  if (resolved.headerCfg.showGoverningOrg && resolved.effectiveOrg.governingOrg) {
    headerLeftLines.push(resolved.effectiveOrg.governingOrg.toUpperCase());
  }
  if (tpl.headerConfig.showIssuingOrg) headerLeftLines.push(resolved.effectiveOrg.issuingOrg.toUpperCase());
  if (tpl.headerConfig.showNumberSymbol) headerLeftLines.push(`Số: ${resolved.soVaKyHieu}`);
  const subj = resolved.values.trich_yeu || '';
  if (tpl.headerConfig.useLeftCellSubject && subj) {
    headerLeftLines.push(subj.toLowerCase().startsWith('v/v') ? subj : `V/v ${subj}`);
  }

  const headerRightLines: string[] = [];
  if (resolved.headerCfg.showNationalHeader) {
    headerRightLines.push('CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM', 'Độc lập - Tự do - Hạnh phúc');
  }
  if (tpl.headerConfig.showLocationDate) {
    headerRightLines.push(`${resolved.diaDanhLine.replace(/,+$/, '')}, ${resolved.ngayThangLine}`);
  }

  const titleLines: string[] = [];
  if (tpl.headerConfig.showDocTypeAndSubject && !tpl.headerConfig.useLeftCellSubject) {
    titleLines.push((tpl.headerConfig.customDocTypeLabel || tpl.docType).toUpperCase());
    if (subj) titleLines.push(subj);
  }

  const bodyBlocks: TemplatePreviewBlock[] = resolveEnabledSections(tpl, req).map((sec) => ({
    role: sec.kind,
    title: renderTemplateStr(sec.title, resolved.values),
    lines: sec.bodyParagraphs.map((p) => renderTemplateStr(p, resolved.values)),
    tablePreview: sec.inlineTable
      ? {
          caption: renderTemplateStr(sec.inlineTable.caption, resolved.values),
          columns: sec.inlineTable.columns,
          sampleRows: sec.inlineTable.sampleRows.map((row) =>
            row.map((c) => renderTemplateStr(c, resolved.values)),
          ),
        }
      : null,
  }));

  const recipientLines = tpl.footerConfig.showRecipientList
    ? tpl.footerConfig.defaultRecipients.map((r) => renderTemplateStr(r, resolved.values))
    : [];

  const sTitle = renderTemplateStr(tpl.signatureConfig.signerTitlePlaceholder, resolved.values).toUpperCase();
  const sName = renderTemplateStr(tpl.signatureConfig.signerNamePlaceholder, resolved.values);
  const signatureColumns =
    tpl.signatureConfig.layout === 'none'
      ? []
      : tpl.signatureConfig.layout === 'dualSecretaryChair'
        ? [
            [(tpl.signatureConfig.leftColumnTitle || 'THƯ KÝ').toUpperCase(), '(Ký, ghi rõ họ tên)'],
            [sTitle, '(Ký, ghi rõ họ tên)', sName],
          ]
        : tpl.signatureConfig.layout === 'tripleApproval'
          ? [
              [(tpl.signatureConfig.leftColumnTitle || 'NGƯỜI ĐỀ XUẤT').toUpperCase()],
              [(tpl.signatureConfig.middleColumnTitle || 'THẨM ĐỊNH').toUpperCase()],
              [sTitle, sName],
            ]
          : [[sTitle, '(Ký, ghi rõ họ tên và đóng dấu)', sName]];

  const compatibility: TemplateCompatibilityReport = {
    compatible: stdReport.severityCounts.critical + stdReport.severityCounts.error === 0,
    scope: stdReport.effectiveScope,
    passedRules: stdReport.statusCounts.passed,
    failedRules: stdReport.statusCounts.failed,
    needsReviewRules: stdReport.statusCounts.needsReview,
    missingFields: resolved.missingFields,
    preservedAssets: stdReport.assets,
    warnings:
      resolved.missingFields.length > 0
        ? [`Còn ${resolved.missingFields.length} trường động chưa nhập — đã đánh dấu [CHƯA NHẬP: ...].`]
        : [],
    stdFindings: stdReport.findings,
  };

  const pipelineSteps: TemplatePipelineStep[] = [
    { stepNumber: 1, stepName: 'Đọc cấu trúc mẫu', detail: `${tpl.name} (v${tpl.version})`, completed: true },
    { stepNumber: 2, stepName: 'Đọc Organization Profile', detail: orgProfile ? orgProfile.profileName : 'Chưa chọn hồ sơ', completed: true },
    { stepNumber: 3, stepName: 'Xác định loại văn bản', detail: `${tpl.group} · ${tpl.docType}`, completed: true },
    { stepNumber: 4, stepName: 'Tạo cấu trúc phần đầu', detail: resolved.effectiveOrg.issuingOrg, completed: true },
    { stepNumber: 5, stepName: 'Tạo / bảo toàn nội dung', detail: `${bodyBlocks.length} khối nội dung`, completed: true },
    { stepNumber: 6, stepName: 'Tạo cấu trúc kết thúc', detail: `Chữ ký: ${tpl.signatureConfig.layout}`, completed: true },
    { stepNumber: 7, stepName: 'Áp dụng định dạng', detail: `${tpl.formatRules.fontFamily} ${tpl.formatRules.bodyFontSizePt}pt`, completed: true },
    { stepNumber: 8, stepName: 'Kiểm tra tương thích', detail: `${compatibility.passedRules} đạt · ${compatibility.missingFields.length} thiếu`, completed: true },
    { stepNumber: 9, stepName: 'Hiển thị bản xem trước', detail: 'Sẵn sàng kiểm tra', completed: true },
    { stepNumber: 10, stepName: 'Xuất DOCX', detail: 'Chờ xuất tệp', completed: false },
  ];

  return {
    templateId: tpl.templateId,
    templateName: tpl.name,
    docType: tpl.docType,
    resolvedOrgName: resolved.effectiveOrg.issuingOrg,
    headerLeftLines,
    headerRightLines,
    titleLines,
    bodyBlocks,
    recipientLines,
    signatureColumns,
    pipelineSteps,
    compatibility,
  };
}

export async function applyWebTemplateToDocx(
  req: TemplateApplyRequest,
  orgProfile: OrganizationProfile | null,
  targetItem?: FileQueueItem,
): Promise<TemplateApplyResult> {
  if (targetItem) registerWebTemplateTargetSnapshot(targetItem);
  try {
    const tpl = getWebTemplate(req.templateId);
    let docPlaceholders: string[] = [];
    if (req.targetMode === 'applyToExistingDocx' && targetItem?.webFile) {
      const entries = await readZipEntries(await targetItem.webFile.arrayBuffer());
      const dec = new TextDecoder('utf-8');
      const set = new Set<string>();
      for (const [k, b] of entries.entries()) {
        if (k.toLowerCase().endsWith('.xml')) extractPlaceholdersFromString(dec.decode(b), set);
      }
      docPlaceholders = Array.from(set);
    }
    const resolved = resolveWebTemplateData(tpl, orgProfile, req.dynamicFields, docPlaceholders);
    const logs: string[] = [];
    const { outBlob, outputName } = await generateOrApplyWebPackage(tpl, req, resolved, targetItem, false, logs);
    const reEntries = await readZipEntries(await outBlob.arrayBuffer());
    const verified = reEntries.has('word/document.xml');
    const preview = await previewWebTemplateApplication(req, orgProfile, targetItem);
    const step10 = preview.pipelineSteps.find((s) => s.stepNumber === 10);
    if (step10) {
      step10.completed = verified;
      step10.detail = `${outputName} (${outBlob.size} bytes)`;
    }

    return {
      success: verified,
      outputPath: outputName,
      backupPath: targetItem && initialWebTemplateTargetSnapshots.has(targetItem.path) ? 'web-memory-snapshot' : null,
      error: verified ? null : 'Không xác minh được cấu trúc OpenXML đầu ra.',
      postSaveVerified: verified,
      outputSizeBytes: outBlob.size,
      preview,
      logs,
      blobUrl: URL.createObjectURL(outBlob),
      outputName,
      outputBlob: outBlob,
    };
  } catch (err) {
    return {
      success: false,
      outputPath: '',
      backupPath: null,
      error: (err as Error).message || 'Lỗi xử lý mẫu văn bản trên trình duyệt.',
      postSaveVerified: false,
      outputSizeBytes: 0,
      preview: null,
      logs: [],
    };
  }
}
