import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  Archive,
  CheckCircle2,
  Copy,
  Download,
  ExternalLink,
  Eye,
  FilePlus2,
  FolderOpen,
  History,
  LayoutTemplate,
  Lock,
  Pencil,
  Play,
  Plus,
  RotateCcw,
  Save,
  Share2,
  Star,
  Trash2,
  Upload,
} from 'lucide-react';
import { bridge } from '../bridge';
import { isAdminToolsEnabled, loadDefaultDocInfo } from '../utils/platform';
import type {
  AdminDocType,
  DocumentTemplate,
  DynamicTemplateFields,
  FileQueueItem,
  OrganizationProfile,
  OutputMode,
  PageNumPosition,
  PaperSize,
  SignatureLayoutKind,
  StdDocScope,
  TemplateApplyRequest,
  TemplateApplyResult,
  TemplateBlockKind,
  TemplateCatalogResponse,
  TemplateGroup,
  TemplatePreviewResult,
  TemplateSectionBlock,
  TemplateTargetMode,
  TemplateUsageStatus,
  TemplateVersionRecord,
} from '../types';
import { ADMIN_DOC_TYPE_META } from '../types';
import { shareOutputBlob, triggerBrowserDownload } from '../utils/webDocxEngine';
import { listWebOrgProfiles } from '../utils/webHeaderBuilder';
import {
  applyWebTemplateToDocx,
  cloneWebTemplate,
  deleteWebTemplate,
  exportWebDocxTemplate,
  getBuiltinTemplates,
  getWebTemplateTargetSnapshot,
  importWebDocxTemplate,
  listWebTemplates,
  listWebTemplateVersions,
  previewWebTemplateApplication,
  restoreWebTemplateVersion,
  saveWebTemplate,
  setWebDefaultTemplate,
} from '../utils/webTemplateEngine';

export interface TemplateWorkspaceProps {
  files: FileQueueItem[];
  onPickFiles: () => void;
  onReplaceWebFile?: (path: string, file: File) => void;
  onTemplateCreated?: (res: TemplateApplyResult, tpl: DocumentTemplate) => void;
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

type SubTab = 'apply' | 'designer' | 'versions';
type GroupFilter = 'popular' | 'administrative' | 'enterprise' | 'all' | 'mine';

const POPULAR_TEMPLATE_ORDER: string[] = [
  'adm-cong-van',
  'adm-to-trinh',
  'adm-bao-cao',
  'adm-quyet-dinh',
  'adm-thong-bao',
  'adm-bien-ban',
  'adm-ke-hoach',
  'adm-giay-moi',
];

const POPULAR_TEMPLATE_SET = new Set<string>(POPULAR_TEMPLATE_ORDER);

interface TemplateFieldSpec {
  key: keyof Omit<DynamicTemplateFields, 'extraFields'>;
  label: string;
  placeholder: string;
  required: boolean;
  fullWidth?: boolean;
}

function getTemplateFieldSpecs(
  tpl: DocumentTemplate | null,
  org: OrganizationProfile | null,
): TemplateFieldSpec[] {
  if (!tpl) return [];
  const ph = new Set(tpl.detectedPlaceholders || []);
  const textBlob = JSON.stringify(tpl.contentStructure || []) + JSON.stringify(tpl.footerConfig || {});
  const hasPlaceholder = (k: string) => ph.has(k) || textBlob.includes(`{{${k}}}`);

  const specs: TemplateFieldSpec[] = [];
  if (tpl.headerConfig.showIssuingOrg || hasPlaceholder('ten_co_quan')) {
    specs.push({
      key: 'tenCoQuan',
      label: 'Tên cơ quan / đơn vị ban hành',
      placeholder: org?.issuingOrg || 'Ví dụ: ỦY BAN NHÂN DÂN TỈNH...',
      required: true,
    });
  }
  if (tpl.headerConfig.showNumberSymbol || hasPlaceholder('so_van_ban')) {
    specs.push({
      key: 'soVanBan',
      label: 'Số và ký hiệu văn bản',
      placeholder: 'Ví dụ: 125/QĐ-UBND',
      required: false,
    });
  }
  if (tpl.headerConfig.showLocationDate || hasPlaceholder('dia_danh')) {
    specs.push({
      key: 'diaDanh',
      label: 'Địa danh ban hành',
      placeholder: org?.defaultLocation || 'Ví dụ: Hà Nội',
      required: true,
    });
  }
  if (tpl.headerConfig.showLocationDate || hasPlaceholder('ngay_thang')) {
    specs.push({
      key: 'ngayThang',
      label: 'Ngày tháng ban hành',
      placeholder: 'Ví dụ: 20/11/2026',
      required: false,
    });
  }
  if (tpl.signatureConfig?.layout !== 'none' || hasPlaceholder('chuc_vu')) {
    specs.push({
      key: 'chucVu',
      label: 'Chức vụ người ký',
      placeholder: 'Ví dụ: GIÁM ĐỐC / CHỦ TỊCH',
      required: true,
    });
  }
  if (tpl.signatureConfig?.layout !== 'none' || hasPlaceholder('nguoi_ky')) {
    specs.push({
      key: 'nguoiKy',
      label: 'Họ và tên người ký',
      placeholder: 'Ví dụ: Nguyễn Văn An',
      required: false,
    });
  }
  if (tpl.footerConfig?.showRecipientList || hasPlaceholder('nguoi_nhan')) {
    specs.push({
      key: 'nguoiNhan',
      label: 'Nơi nhận / Kính gửi',
      placeholder: 'Ví dụ: Các phòng ban trực thuộc',
      required: false,
      fullWidth: true,
    });
  }
  if (tpl.headerConfig.showDocTypeAndSubject || hasPlaceholder('trich_yeu')) {
    specs.push({
      key: 'trichYeu',
      label: 'Trích yếu nội dung văn bản',
      placeholder: 'Ví dụ: Về việc triển khai kế hoạch công tác năm 2026',
      required: true,
      fullWidth: true,
    });
  }
  return specs;
}

const EMPTY_DYNAMIC_FIELDS: DynamicTemplateFields = {
  tenCoQuan: '',
  coQuanChuQuan: '',
  vietTatCoQuan: '',
  soVanBan: '',
  diaDanh: '',
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

const GROUP_LABELS: Record<TemplateGroup, string> = {
  administrative: 'Hành chính (NĐ 30)',
  enterprise: 'Doanh nghiệp',
  party: 'Văn bản Đảng',
  academic: 'Học thuật',
  custom: 'Tùy chỉnh / Nhập DOCX',
};

const SIG_LABELS: Record<SignatureLayoutKind, string> = {
  singleRight: '1 cột góc phải (Cơ bản)',
  dualSecretaryChair: '2 cột (Thư ký / Chủ trì - Biên bản)',
  tripleApproval: '3 cột (Đề xuất / Thẩm định / Phê duyệt)',
  None: 'Không chèn khung chữ ký',
} as unknown as Record<SignatureLayoutKind, string>;
SIG_LABELS.none = 'Không chèn khung chữ ký';

export const TemplateWorkspace: React.FC<TemplateWorkspaceProps> = ({
  files,
  onPickFiles,
  onReplaceWebFile,
  onTemplateCreated,
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();
  const adminMode = isAdminToolsEnabled();
  const webImportDocxInputRef = useRef<HTMLInputElement>(null);

  const [subTab, setSubTab] = useState<SubTab>('apply');
  const [groupFilter, setGroupFilter] = useState<GroupFilter>('popular');
  const [searchQuery, setSearchQuery] = useState('');

  const [catalog, setCatalog] = useState<TemplateCatalogResponse>(() => ({
    templates: getBuiltinTemplates(),
    customDropinDir: '',
    defaults: {},
  }));
  const [selectedId, setSelectedId] = useState<string>('adm-cong-van');
  const [orgProfiles, setOrgProfiles] = useState<OrganizationProfile[]>([]);
  const [selectedOrgId, setSelectedOrgId] = useState<string>('');

  const [targetMode, setTargetMode] = useState<TemplateTargetMode>('createNewDocx');
  const [selectedTargetPath, setSelectedTargetPath] = useState<string>('');
  const [outputMode, setOutputMode] = useState<OutputMode>('newCopy');
  const [insertFramingOnExisting, setInsertFramingOnExisting] = useState<boolean>(true);
  const [enabledSectionIds, setEnabledSectionIds] = useState<string[]>([]);
  const [dynamicFields, setDynamicFields] = useState<DynamicTemplateFields>(() => {
    const def = loadDefaultDocInfo();
    return {
      ...EMPTY_DYNAMIC_FIELDS,
      chucVu: def.defaultSignerTitle,
      nguoiKy: def.defaultSignerName,
    };
  });

  const [previewResult, setPreviewResult] = useState<TemplatePreviewResult | null>(null);
  const [applyResult, setApplyResult] = useState<TemplateApplyResult | null>(null);
  const [busy, setBusy] = useState<boolean>(false);

  // Designer state
  const [draftTpl, setDraftTpl] = useState<DocumentTemplate | null>(null);
  const [confirmSystemOverride, setConfirmSystemOverride] = useState<boolean>(false);
  const [changeNote, setChangeNote] = useState<string>('');
  const [versions, setVersions] = useState<TemplateVersionRecord[]>([]);

  // Import DOCX dialog metadata
  const [importModalOpen, setImportModalOpen] = useState<boolean>(false);
  const [importName, setImportName] = useState<string>('');
  const [importGroup, setImportGroup] = useState<TemplateGroup>('custom');
  const [importDocType, setImportDocType] = useState<string>('customDocx');
  const [importScope, setImportScope] = useState<StdDocScope>('adminNd30');

  const selectedTemplate = useMemo(
    () => catalog.templates.find((t) => t.templateId === selectedId) || catalog.templates[0] || null,
    [catalog.templates, selectedId],
  );

  const selectedOrgProfile = useMemo(
    () => orgProfiles.find((o) => o.id === selectedOrgId) || null,
    [orgProfiles, selectedOrgId],
  );

  const docxQueueFiles = useMemo(
    () => files.filter((f) => f.ext === 'docx' || f.ext === 'docm'),
    [files],
  );

  const loadCatalogAndOrgs = useCallback(async () => {
    try {
      const defOrgId = loadDefaultDocInfo().defaultOrgProfileId;
      let loadedOrgs: OrganizationProfile[] = [];
      if (isNative) {
        const [cat, orgs] = await Promise.all([
          bridge.invoke<TemplateCatalogResponse>('list_templates', {}),
          bridge.invoke<OrganizationProfile[]>('list_org_profiles', {}),
        ]);
        setCatalog(cat);
        loadedOrgs = Array.isArray(orgs) ? orgs : [];
      } else {
        setCatalog(listWebTemplates());
        loadedOrgs = listWebOrgProfiles();
      }
      setOrgProfiles(loadedOrgs);
      if (defOrgId && loadedOrgs.some((o) => o.id === defOrgId)) {
        setSelectedOrgId((prev) => prev || defOrgId);
      }
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  }, [isNative, onStatusToast]);

  useEffect(() => {
    void loadCatalogAndOrgs();
  }, [loadCatalogAndOrgs]);

  useEffect(() => {
    if (docxQueueFiles.length > 0 && !docxQueueFiles.some((f) => f.path === selectedTargetPath)) {
      setSelectedTargetPath(docxQueueFiles[0].path);
    }
  }, [docxQueueFiles, selectedTargetPath]);

  // Sync section checkboxes & designer draft when switching active template
  useEffect(() => {
    if (!selectedTemplate) return;
    setEnabledSectionIds(
      selectedTemplate.contentStructure
        .filter((s) => s.required || s.enabledByDefault)
        .map((s) => s.sectionId),
    );
    setDraftTpl(JSON.parse(JSON.stringify(selectedTemplate)) as DocumentTemplate);
    setConfirmSystemOverride(false);
    setChangeNote('');
    setPreviewResult(null);
    setApplyResult(null);
  }, [selectedTemplate]);

  const loadVersions = useCallback(
    async (templateId: string) => {
      try {
        if (isNative) {
          const list = await bridge.invoke<TemplateVersionRecord[]>('list_template_versions', {
            templateId,
          });
          setVersions(list);
        } else {
          setVersions(listWebTemplateVersions(templateId));
        }
      } catch {
        setVersions([]);
      }
    },
    [isNative],
  );

  useEffect(() => {
    if (selectedTemplate) {
      void loadVersions(selectedTemplate.templateId);
    }
  }, [selectedTemplate, loadVersions]);

  const filteredTemplates = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    const list = catalog.templates.filter((t) => {
      if (q) {
        return (
          t.name.toLowerCase().includes(q) ||
          t.docType.toLowerCase().includes(q) ||
          t.description.toLowerCase().includes(q)
        );
      }
      if (groupFilter === 'popular') return POPULAR_TEMPLATE_SET.has(t.templateId);
      if (groupFilter === 'administrative') return t.group === 'administrative';
      if (groupFilter === 'enterprise') return t.group === 'enterprise';
      if (groupFilter === 'mine') return !t.isSystem;
      return true;
    });
    if (!q && groupFilter === 'popular') {
      return [...list].sort(
        (a, b) =>
          POPULAR_TEMPLATE_ORDER.indexOf(a.templateId) -
          POPULAR_TEMPLATE_ORDER.indexOf(b.templateId),
      );
    }
    return list;
  }, [catalog.templates, groupFilter, searchQuery]);

  // Auto-fill editable agency & location from saved Hồ sơ cơ quan when blank
  useEffect(() => {
    if (!selectedOrgProfile) return;
    setDynamicFields((prev) => ({
      ...prev,
      tenCoQuan: prev.tenCoQuan || selectedOrgProfile.issuingOrg || '',
      coQuanChuQuan: prev.coQuanChuQuan || selectedOrgProfile.governingOrg || '',
      vietTatCoQuan: prev.vietTatCoQuan || selectedOrgProfile.agencyAbbreviation || '',
      diaDanh: prev.diaDanh || selectedOrgProfile.defaultLocation || '',
    }));
  }, [selectedOrgProfile]);

  const fieldSpecs = useMemo(
    () => getTemplateFieldSpecs(selectedTemplate, selectedOrgProfile),
    [selectedTemplate, selectedOrgProfile],
  );

  const customExtraKeys = useMemo(() => {
    const stdKeys = new Set([
      'ten_co_quan',
      'co_quan_chu_quan',
      'viet_tat_co_quan',
      'so_van_ban',
      'dia_danh',
      'ngay_thang',
      'nguoi_ky',
      'chuc_vu',
      'nguoi_nhan',
      'trich_yeu',
      'phien_ban',
      'ten_mau',
    ]);
    return (selectedTemplate?.detectedPlaceholders || []).filter((k) => !stdKeys.has(k));
  }, [selectedTemplate]);

  const buildApplyRequest = (destPath: string | null = null): TemplateApplyRequest => ({
    templateId: selectedTemplate?.templateId || 'adm-cong-van',
    orgProfileId: selectedOrgProfile?.id || null,
    orgProfileOverride: selectedOrgProfile,
    targetMode,
    targetPath: targetMode === 'applyToExistingDocx' ? selectedTargetPath || null : null,
    destPath,
    outputMode,
    enabledSectionIds,
    dynamicFields,
    insertFramingOnExisting,
  });

  const handlePreview = async () => {
    if (!selectedTemplate) return;
    if (targetMode === 'applyToExistingDocx' && !selectedTargetPath) {
      onStatusToast('error', 'Vui lòng chọn tệp Word để áp dụng mẫu.');
      return;
    }
    setBusy(true);
    try {
      const req = buildApplyRequest();
      if (isNative) {
        const res = await bridge.invoke<TemplatePreviewResult>('preview_template_application', req);
        setPreviewResult(res);
      } else {
        const targetItem = docxQueueFiles.find((f) => f.path === selectedTargetPath);
        const res = await previewWebTemplateApplication(req, selectedOrgProfile, targetItem);
        setPreviewResult(res);
      }
      onStatusToast('info', 'Đã cập nhật bản xem trước văn bản.');
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const handleApplyAndExport = async () => {
    if (!selectedTemplate) return;
    if (targetMode === 'applyToExistingDocx' && !selectedTargetPath) {
      onStatusToast('error', 'Vui lòng chọn tệp Word trước khi áp dụng mẫu.');
      return;
    }
    setBusy(true);
    try {
      let destPath: string | null = null;
      if (isNative && targetMode === 'createNewDocx') {
        const saveRes = await bridge.invoke<{ paths: string[] }>('pick_save_docx', {
          defaultName: `${selectedTemplate.docType}_MonWord.docx`,
        });
        if (!saveRes.paths?.length) {
          setBusy(false);
          return;
        }
        destPath = saveRes.paths[0];
      }

      const req = buildApplyRequest(destPath);
      if (isNative) {
        const res = await bridge.invoke<TemplateApplyResult>('apply_template_to_docx', req);
        setApplyResult(res);
        if (res.preview) setPreviewResult(res.preview);
        if (res.success) {
          onTemplateCreated?.(res, selectedTemplate);
          onStatusToast('success', `Đã tạo và lưu tệp Word: ${res.outputPath}`);
        } else {
          onStatusToast('error', res.error || 'Không thể tạo văn bản từ mẫu.');
        }
      } else {
        const targetItem = docxQueueFiles.find((f) => f.path === selectedTargetPath);
        const res = await applyWebTemplateToDocx(req, selectedOrgProfile, targetItem);
        setApplyResult(res);
        if (res.preview) setPreviewResult(res.preview);
        if (res.success && res.blobUrl && res.outputName) {
          onTemplateCreated?.(res, selectedTemplate);
          triggerBrowserDownload(res.blobUrl, res.outputName);
          onStatusToast('success', `Đã tạo & tải xuống tệp Word: ${res.outputName}`);
        } else if (!res.success) {
          onStatusToast('error', res.error || 'Không thể tạo văn bản từ mẫu.');
        }
      }
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const handleRestoreTargetBackup = async () => {
    if (!applyResult?.backupPath || !selectedTargetPath) return;
    try {
      if (isNative) {
        const res = await bridge.invoke<{ message: string }>('restore_template_backup', {
          originalPath: selectedTargetPath,
          backupPath: applyResult.backupPath,
        });
        onStatusToast('success', res.message);
      } else {
        const snap = getWebTemplateTargetSnapshot(selectedTargetPath);
        if (!snap || !onReplaceWebFile) {
          throw new Error('Không tìm thấy bản gốc trong bộ nhớ trình duyệt.');
        }
        onReplaceWebFile(selectedTargetPath, snap);
        onStatusToast('success', `Đã khôi phục tệp gốc ${snap.name}.`);
      }
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const handleCreateBlankUserTemplate = () => {
    const base = JSON.parse(JSON.stringify(selectedTemplate || getBuiltinTemplates()[0])) as DocumentTemplate;
    base.templateId = '';
    base.name = 'Mẫu mới do người dùng tạo';
    base.group = 'custom';
    base.docType = `custom_${Date.now().toString().slice(-4)}`;
    base.isSystem = false;
    base.isDefaultForDocType = false;
    base.sourceDocxAssetId = null;
    base.version = '1.0.0';
    base.status = 'active';
    setDraftTpl(base);
    setConfirmSystemOverride(false);
    setChangeNote('Khởi tạo mẫu mới');
    setSubTab('designer');
    onStatusToast('info', 'Đang soạn mẫu mới — chỉnh sửa bên dưới rồi bấm Lưu mẫu.');
  };

  const handleCloneTemplate = async (sourceId: string) => {
    try {
      if (isNative) {
        const res = await bridge.invoke<{ template: DocumentTemplate; catalog: TemplateCatalogResponse }>(
          'clone_template',
          { sourceId },
        );
        setCatalog(res.catalog);
        setSelectedId(res.template.templateId);
        onStatusToast('success', `Đã sao chép thành mẫu "${res.template.name}".`);
      } else {
        const res = cloneWebTemplate(sourceId);
        setCatalog(res.catalog);
        setSelectedId(res.template.templateId);
        onStatusToast('success', `Đã sao chép thành mẫu "${res.template.name}".`);
      }
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const handleSaveDesigner = async () => {
    if (!draftTpl) return;
    try {
      const payload = {
        template: draftTpl,
        confirmSystemOverride,
        changeNote: changeNote.trim() || 'Cập nhật trong Template Designer',
      };
      let nextCatalog: TemplateCatalogResponse;
      if (isNative) {
        nextCatalog = await bridge.invoke<TemplateCatalogResponse>('save_template', payload);
      } else {
        nextCatalog = saveWebTemplate(payload);
      }
      setCatalog(nextCatalog);
      const saved =
        nextCatalog.templates.find((t) => t.name === draftTpl.name && t.docType === draftTpl.docType) ||
        nextCatalog.templates.find((t) => t.templateId === draftTpl.templateId);
      if (saved) {
        setSelectedId(saved.templateId);
        void loadVersions(saved.templateId);
      }
      setConfirmSystemOverride(false);
      onStatusToast('success', `Đã lưu mẫu "${draftTpl.name}" thành công.`);
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const handleDeleteUserTemplate = async (templateId: string) => {
    try {
      const next = isNative
        ? await bridge.invoke<TemplateCatalogResponse>('delete_template', { templateId })
        : deleteWebTemplate(templateId);
      setCatalog(next);
      setSelectedId(next.templates[0]?.templateId || 'adm-cong-van');
      onStatusToast('success', 'Đã xóa mẫu người dùng.');
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const handleSetDefault = async (templateId: string) => {
    try {
      const next = isNative
        ? await bridge.invoke<TemplateCatalogResponse>('set_default_template', { templateId })
        : setWebDefaultTemplate(templateId);
      setCatalog(next);
      onStatusToast('success', 'Đã đặt mẫu làm mặc định cho loại văn bản này.');
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const handleExportDocxMaster = async (templateId: string) => {
    try {
      const tpl = catalog.templates.find((t) => t.templateId === templateId);
      if (isNative) {
        const saveRes = await bridge.invoke<{ paths: string[] }>('pick_save_docx', {
          defaultName: `${tpl?.docType || 'Template'}_Master.docx`,
        });
        if (!saveRes.paths?.length) return;
        const out = await bridge.invoke<{ outputPath: string }>('export_docx_template', {
          templateId,
          destPath: saveRes.paths[0],
        });
        onStatusToast('success', `Đã xuất mẫu DOCX tới: ${out.outputPath}`);
      } else {
        const res = await exportWebDocxTemplate(templateId);
        triggerBrowserDownload(res.blobUrl, res.fileName);
        onStatusToast('success', `Đã xuất mẫu DOCX: ${res.fileName}`);
      }
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const handleStartImportDocx = async () => {
    if (isNative) {
      try {
        const pick = await bridge.invoke<{ paths: string[] }>('pick_template_docx', {});
        if (!pick.paths?.length) return;
        const sourcePath = pick.paths[0];
        const res = await bridge.invoke<{ template: DocumentTemplate; catalog: TemplateCatalogResponse }>(
          'import_docx_template',
          {
            sourcePath,
            name: importName.trim() || sourcePath.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, '') || 'Mẫu DOCX nhập ngoài',
            group: importGroup,
            docType: importDocType.trim() || 'customDocx',
            scope: importScope,
          },
        );
        setCatalog(res.catalog);
        setSelectedId(res.template.templateId);
        setImportModalOpen(false);
        onStatusToast('success', `Đã nhập mẫu DOCX "${res.template.name}" (giữ nguyên bảng, logo & section).`);
      } catch (err) {
        onStatusToast('error', (err as Error).message);
      }
    } else {
      webImportDocxInputRef.current?.click();
    }
  };

  const handleWebImportDocxFile = async (file: File) => {
    try {
      const res = await importWebDocxTemplate(file, {
        name: importName.trim() || file.name.replace(/\.[^.]+$/, ''),
        group: importGroup,
        docType: importDocType.trim() || 'customDocx',
        scope: importScope,
      });
      setCatalog(res.catalog);
      setSelectedId(res.template.templateId);
      setImportModalOpen(false);
      onStatusToast('success', `Đã nhập mẫu DOCX "${res.template.name}".`);
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const handleRestoreVersion = async (ver: string) => {
    if (!selectedTemplate) return;
    try {
      if (isNative) {
        const res = await bridge.invoke<{ template: DocumentTemplate; catalog: TemplateCatalogResponse }>(
          'restore_template_version',
          { templateId: selectedTemplate.templateId, version: ver },
        );
        setCatalog(res.catalog);
        setDraftTpl(JSON.parse(JSON.stringify(res.template)) as DocumentTemplate);
      } else {
        const res = restoreWebTemplateVersion(selectedTemplate.templateId, ver);
        setCatalog(res.catalog);
        setDraftTpl(JSON.parse(JSON.stringify(res.template)) as DocumentTemplate);
      }
      void loadVersions(selectedTemplate.templateId);
      onStatusToast('success', `Đã khôi phục mẫu về phiên bản v${ver}.`);
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    }
  };

  const toggleSection = (sec: TemplateSectionBlock) => {
    if (sec.required) return;
    setEnabledSectionIds((prev) =>
      prev.includes(sec.sectionId) ? prev.filter((id) => id !== sec.sectionId) : [...prev, sec.sectionId],
    );
  };

  const updateDesignerSection = (idx: number, patch: Partial<TemplateSectionBlock>) => {
    if (!draftTpl) return;
    const next = [...draftTpl.contentStructure];
    next[idx] = { ...next[idx], ...patch };
    setDraftTpl({ ...draftTpl, contentStructure: next });
  };

  return (
    <div className="space-y-4">
      <input
        ref={webImportDocxInputRef}
        type="file"
        accept=".docx,.docm"
        className="hidden"
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) {
            void handleWebImportDocxFile(f);
            e.target.value = '';
          }
        }}
      />

      {/* Top Header + Sub-Navigation */}
      <div className="mw-panel p-4 sm:p-5 space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-bold flex items-center gap-2 text-[var(--mw-ink)]">
              <LayoutTemplate className="w-5 h-5 text-[var(--mw-accent)]" />
              {adminMode ? 'Template Engine — Thư viện & Thiết kế Mẫu văn bản' : 'Mẫu văn bản'}
            </h2>
            <p className="text-xs text-[var(--mw-muted)] mt-0.5">
              {adminMode
                ? `Kế thừa Organization Profile, Header Builder và Kiểm chuẩn NĐ 30 · Hiện có ${catalog.templates.length} mẫu.`
                : 'Chọn mẫu văn bản phổ biến, điền các thông tin cần thiết, xem trước và lưu thành tệp Word.'}
            </p>
          </div>
          {adminMode && (
            <div className="flex flex-wrap items-center gap-1.5">
              {(
                [
                  { id: 'apply', label: 'Thư viện & Áp dụng', icon: Play },
                  { id: 'designer', label: 'Template Designer', icon: Pencil },
                  { id: 'versions', label: `Phiên bản (${versions.length})`, icon: History },
                ] as const
              ).map((tab) => {
                const Icon = tab.icon;
                return (
                  <button
                    key={tab.id}
                    type="button"
                    onClick={() => setSubTab(tab.id)}
                    className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-bold transition-colors ${
                      subTab === tab.id
                        ? 'bg-blue-600 text-white shadow-sm'
                        : 'bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700'
                    }`}
                  >
                    <Icon className="w-3.5 h-3.5" />
                    {tab.label}
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* Step 1 Toolbar: Popular + Group Filter + Search */}
        <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-[var(--mw-border)]">
          <div className="flex flex-wrap items-center gap-1.5">
            {(
              [
                { id: 'popular', label: `Mẫu phổ biến (${POPULAR_TEMPLATE_ORDER.length})` },
                {
                  id: 'administrative',
                  label: `Hành chính (${catalog.templates.filter((t) => t.group === 'administrative').length})`,
                },
                {
                  id: 'enterprise',
                  label: `Doanh nghiệp (${catalog.templates.filter((t) => t.group === 'enterprise').length})`,
                },
                ...(adminMode
                  ? [
                      { id: 'all' as GroupFilter, label: `Tất cả (${catalog.templates.length})` },
                      {
                        id: 'mine' as GroupFilter,
                        label: `Mẫu người dùng (${catalog.templates.filter((t) => !t.isSystem).length})`,
                      },
                    ]
                  : []),
              ] as { id: GroupFilter; label: string }[]
            ).map((g) => (
              <button
                key={g.id}
                type="button"
                onClick={() => setGroupFilter(g.id)}
                className={`px-2.5 py-1 rounded-lg text-xs font-semibold ${
                  groupFilter === g.id
                    ? 'bg-[var(--mw-accent)] text-[var(--mw-accent-ink)]'
                    : 'bg-[var(--mw-canvas)] text-[var(--mw-muted)] hover:text-[var(--mw-ink)]'
                }`}
              >
                {g.label}
              </button>
            ))}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <input
              type="search"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Tìm mẫu văn bản..."
              className="px-2.5 py-1 rounded-lg border border-[var(--mw-border)] text-xs w-52"
            />
            {adminMode && (
              <>
                <button
                  type="button"
                  onClick={handleCreateBlankUserTemplate}
                  className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-bold"
                >
                  <Plus className="w-3.5 h-3.5" /> Tạo mẫu
                </button>
                <button
                  type="button"
                  onClick={() => setImportModalOpen((o) => !o)}
                  className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 text-xs font-semibold"
                >
                  <Upload className="w-3.5 h-3.5 text-indigo-500" /> Nhập mẫu DOCX
                </button>
              </>
            )}
          </div>
        </div>

        {/* Inline Import DOCX Metadata Box */}
        {adminMode && importModalOpen && (
          <div className="rounded-xl border border-indigo-200 dark:border-indigo-900 bg-indigo-50/70 dark:bg-indigo-950/30 p-3 space-y-2.5">
            <div className="flex items-center justify-between">
              <div className="text-xs font-bold text-indigo-800 dark:text-indigo-200">
                Nhập mẫu DOCX sẵn có (bảo toàn 100% bảng biểu, logo, header/footer &amp; section)
              </div>
              <button
                type="button"
                onClick={() => setImportModalOpen(false)}
                className="text-xs text-slate-500 hover:text-slate-800"
              >
                Đóng
              </button>
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-4 gap-2">
              <input
                type="text"
                value={importName}
                onChange={(e) => setImportName(e.target.value)}
                placeholder="Tên mẫu (để trống lấy tên tệp)"
                className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
              <select
                value={importGroup}
                onChange={(e) => setImportGroup(e.target.value as TemplateGroup)}
                className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value="custom">Tùy chỉnh / DOCX ngoài</option>
                <option value="administrative">Hành chính</option>
                <option value="enterprise">Doanh nghiệp</option>
              </select>
              <input
                type="text"
                value={importDocType}
                onChange={(e) => setImportDocType(e.target.value)}
                placeholder="Mã loại văn bản (vd: hopDong)"
                className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              />
              <select
                value={importScope}
                onChange={(e) => setImportScope(e.target.value as StdDocScope)}
                className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
              >
                <option value="adminNd30">Phạm vi NĐ 30/2020</option>
                <option value="enterpriseInternal">Nội bộ Doanh nghiệp</option>
                <option value="freeform">Tài liệu tự do</option>
              </select>
            </div>
            <button
              type="button"
              onClick={() => void handleStartImportDocx()}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-700 text-white text-xs font-bold"
            >
              <FolderOpen className="w-3.5 h-3.5" /> Chọn tệp .DOCX / .DOCM để nhập mẫu
            </button>
          </div>
        )}
      </div>

      {/* 1. CHỌN MẪU */}
      <section className="space-y-2">
        <div className="text-xs font-bold text-[var(--mw-ink)]">
          1. Chọn mẫu văn bản ({filteredTemplates.length})
        </div>
        {filteredTemplates.length === 0 ? (
          <div className="mw-panel p-5 text-center text-xs text-[var(--mw-muted)]">
            Không tìm thấy mẫu phù hợp với từ khóa &quot;{searchQuery}&quot;.
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-2.5 max-h-64 overflow-y-auto pr-1">
            {filteredTemplates.map((tpl) => {
              const active = selectedTemplate?.templateId === tpl.templateId;
              return (
                <button
                  key={tpl.templateId}
                  type="button"
                  onClick={() => setSelectedId(tpl.templateId)}
                  className={`text-left rounded-xl border p-3 transition-colors ${
                    active
                      ? 'border-[var(--mw-accent)] bg-[var(--mw-tint)] ring-1 ring-[var(--mw-focus)]'
                      : 'border-[var(--mw-border)] bg-[var(--mw-surface)] hover:border-[var(--mw-focus)]'
                  }`}
                >
                  <div className="flex items-start justify-between gap-1.5">
                    <span className="text-xs font-bold text-[var(--mw-ink)] line-clamp-1">
                      {tpl.name}
                    </span>
                    {adminMode && tpl.isDefaultForDocType && (
                      <span title="Mặc định" className="text-amber-500 shrink-0">
                        <Star className="w-3.5 h-3.5 fill-current" />
                      </span>
                    )}
                  </div>
                  <div className="mt-1 text-[10px] font-semibold text-[var(--mw-accent)]">
                    {GROUP_LABELS[tpl.group]}
                    {adminMode ? ` · ${tpl.headerConfig.symbolPrefix || tpl.docType} · v${tpl.version}` : ''}
                  </div>
                  <p className="text-[11px] text-[var(--mw-muted)] mt-1 line-clamp-2">
                    {tpl.description || `${tpl.contentStructure.length} thành phần nội dung`}
                  </p>
                </button>
              );
            })}
          </div>
        )}
      </section>

      {/* =================================================================== */}
      {/* SUBTAB 1: APPLY & PREVIEW WORKSPACE                                 */}
      {/* =================================================================== */}
      {subTab === 'apply' && selectedTemplate && (
        <div className="grid grid-cols-1 xl:grid-cols-12 gap-4">
          {/* Cột trái: Giới thiệu mẫu & Điền thông tin */}
          <div className="xl:col-span-6 space-y-3">
            {/* 2. Xem giới thiệu mẫu */}
            <div className="mw-panel p-4 space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-bold text-[var(--mw-ink)]">
                  2. Giới thiệu mẫu: {selectedTemplate.name}
                </h3>
                {adminMode && (
                  <div className="flex items-center gap-1.5">
                    <button
                      type="button"
                      onClick={() => void handleCloneTemplate(selectedTemplate.templateId)}
                      className="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 text-[11px] font-semibold"
                    >
                      <Copy className="w-3 h-3" /> Sao chép
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleExportDocxMaster(selectedTemplate.templateId)}
                      className="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 text-[11px] font-semibold"
                    >
                      <Download className="w-3 h-3" /> Xuất mẫu DOCX
                    </button>
                  </div>
                )}
              </div>

              <p className="text-xs text-[var(--mw-muted)] leading-5">
                {selectedTemplate.description || 'Mẫu văn bản chuẩn sẵn sàng điền thông tin và xuất ra tệp Word.'}
              </p>

              {/* Hồ sơ cơ quan tự động áp dụng */}
              <div className="grid grid-cols-1 gap-2">
                <label className="block text-xs space-y-1">
                  <span className="font-semibold text-[var(--mw-ink)]">
                    Hồ sơ cơ quan áp dụng
                  </span>
                  <select
                    value={selectedOrgId}
                    onChange={(e) => setSelectedOrgId(e.target.value)}
                    className="w-full px-2.5 py-1.5 rounded-lg border border-[var(--mw-border)] text-xs"
                  >
                    <option value="">-- Tự nhập thông tin bên dưới --</option>
                    {orgProfiles.map((o) => (
                      <option key={o.id} value={o.id}>
                        {o.profileName} — {o.issuingOrg}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              {/* Thành phần bố cục trong mẫu */}
              <div className="space-y-1.5 pt-1">
                <div className="text-xs font-semibold text-[var(--mw-ink)]">
                  Thành phần trong mẫu ({selectedTemplate.contentStructure.length} phần)
                </div>
                <div className="space-y-1 max-h-44 overflow-y-auto pr-1">
                  {selectedTemplate.contentStructure.map((sec) => {
                    const checked = sec.required || enabledSectionIds.includes(sec.sectionId);
                    return (
                      <label
                        key={sec.sectionId}
                        className={`flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-lg border text-xs ${
                          checked
                            ? 'border-[var(--mw-border)] bg-[var(--mw-canvas)]'
                            : 'border-dashed border-[var(--mw-border)] opacity-65'
                        }`}
                      >
                        <span className="flex items-center gap-2 min-w-0">
                          <input
                            type="checkbox"
                            checked={checked}
                            disabled={sec.required}
                            onChange={() => toggleSection(sec)}
                          />
                          <span className="font-medium truncate">
                            {sec.title ||
                              (sec.kind === 'recipientSalutation'
                                ? 'Phần Kính gửi / Nơi nhận'
                                : sec.kind === 'preambleBasis'
                                  ? 'Căn cứ ban hành'
                                  : sec.kind === 'authorityHeading'
                                    ? 'Thẩm quyền ký ban hành'
                                    : sec.kind === 'decisionKeyword'
                                      ? 'Từ khóa Quyết định'
                                      : sec.inlineTable
                                        ? `Bảng: ${sec.inlineTable.columns.map((c) => c.header).join(' · ')}`
                                        : (sec.bodyParagraphs[0] || 'Phần nội dung')
                                            .replace(/\{\{[^}]+\}\}/g, '...')
                                            .replace(/^\[Hướng dẫn nhập[^:]*:\s*/i, '')
                                            .replace(/\]$/, '')
                                            .slice(0, 60))}
                          </span>
                        </span>
                        <span
                          className={`px-1.5 py-0.5 rounded text-[10px] font-bold shrink-0 inline-flex items-center gap-1 ${
                            sec.required
                              ? 'bg-slate-200 dark:bg-slate-800 text-slate-700 dark:text-slate-300'
                              : 'bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300'
                          }`}
                        >
                          {sec.required && <Lock className="w-2.5 h-2.5" />}
                          {sec.required ? 'Bắt buộc' : 'Tùy chọn'}
                        </span>
                      </label>
                    );
                  })}
                </div>
              </div>

              {adminMode && (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 pt-2 border-t border-[var(--mw-border)]">
                  <button
                    type="button"
                    onClick={() => setTargetMode('createNewDocx')}
                    className={`p-2 rounded-xl border text-left text-xs ${
                      targetMode === 'createNewDocx'
                        ? 'border-blue-600 bg-blue-50/70 dark:bg-blue-950/30 font-bold text-blue-700'
                        : 'border-slate-200 dark:border-slate-700'
                    }`}
                  >
                    <div className="flex items-center gap-1.5">
                      <FilePlus2 className="w-3.5 h-3.5" /> Tạo tệp DOCX mới
                    </div>
                  </button>
                  <button
                    type="button"
                    onClick={() => setTargetMode('applyToExistingDocx')}
                    className={`p-2 rounded-xl border text-left text-xs ${
                      targetMode === 'applyToExistingDocx'
                        ? 'border-blue-600 bg-blue-50/70 dark:bg-blue-950/30 font-bold text-blue-700'
                        : 'border-slate-200 dark:border-slate-700'
                    }`}
                  >
                    <div className="flex items-center gap-1.5">
                      <FolderOpen className="w-3.5 h-3.5" /> Áp dụng vào DOCX sẵn có
                    </div>
                  </button>
                </div>
              )}

              {adminMode && targetMode === 'applyToExistingDocx' && (
                <div className="rounded-xl bg-slate-50 dark:bg-slate-950/60 border border-slate-200 dark:border-slate-800 p-2.5 space-y-2 text-xs">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-semibold">Tệp DOCX mục tiêu:</span>
                    <button
                      type="button"
                      onClick={onPickFiles}
                      className="font-bold text-blue-600 hover:underline"
                    >
                      + Mở thêm DOCX
                    </button>
                  </div>
                  <select
                    value={selectedTargetPath}
                    onChange={(e) => setSelectedTargetPath(e.target.value)}
                    className="w-full px-2.5 py-1.5 rounded-lg border text-xs"
                  >
                    {docxQueueFiles.length === 0 && (
                      <option value="">(Hàng đợi chưa có tệp .docx / .docm)</option>
                    )}
                    {docxQueueFiles.map((f) => (
                      <option key={f.path} value={f.path}>
                        {f.name} ({Math.max(1, Math.round(f.sizeBytes / 1024))} KB)
                      </option>
                    ))}
                  </select>
                  <div className="flex flex-wrap items-center justify-between gap-2 text-[11px]">
                    <label className="inline-flex items-center gap-1.5 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={insertFramingOnExisting}
                        onChange={(e) => setInsertFramingOnExisting(e.target.checked)}
                      />
                      Chèn khung chữ ký nếu chưa có
                    </label>
                    <select
                      value={outputMode}
                      onChange={(e) => setOutputMode(e.target.value as OutputMode)}
                      className="px-2 py-1 rounded border"
                    >
                      <option value="newCopy">Tạo bản sao</option>
                      <option value="overwriteWithBackup">Ghi đè + Backup</option>
                    </select>
                  </div>
                </div>
              )}
            </div>

            {/* 3. Điền các thông tin cần thiết */}
            <div className="mw-panel p-4 space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <h3 className="text-sm font-bold text-[var(--mw-ink)]">
                    3. Điền các thông tin cần thiết
                  </h3>
                  <p className="text-[11px] text-[var(--mw-muted)]">
                    Các trường có dấu <span className="text-rose-600 font-bold">*</span> là thông tin cốt lõi của mẫu. Thông tin cơ quan đã lưu được tự điền sẵn để bạn chỉnh sửa nhanh.
                  </p>
                </div>
                {adminMode && (
                  <button
                    type="button"
                    onClick={() => setDynamicFields(EMPTY_DYNAMIC_FIELDS)}
                    className="text-[11px] text-[var(--mw-muted)] hover:underline"
                  >
                    Xóa trắng trường
                  </button>
                )}
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
                {fieldSpecs.map((spec) => (
                  <label
                    key={spec.key}
                    className={`block space-y-1 ${spec.fullWidth ? 'sm:col-span-2' : ''}`}
                  >
                    <span className="font-semibold text-[var(--mw-ink)]">
                      {spec.label}
                      {spec.required && (
                        <span className="ml-1 text-rose-600" aria-label="Bắt buộc">
                          *
                        </span>
                      )}
                    </span>
                    <input
                      type="text"
                      value={String(dynamicFields[spec.key] || '')}
                      onChange={(e) =>
                        setDynamicFields({ ...dynamicFields, [spec.key]: e.target.value })
                      }
                      placeholder={spec.placeholder}
                      className="w-full px-2.5 py-1.5 rounded-lg border border-[var(--mw-border)] text-xs"
                    />
                  </label>
                ))}
                {customExtraKeys.map((ek) => (
                  <label key={ek} className="block space-y-1">
                    <span className="font-semibold text-[var(--mw-ink)]">
                      Thông tin bổ sung ({ek})
                    </span>
                    <input
                      type="text"
                      value={dynamicFields.extraFields[ek] || ''}
                      onChange={(e) =>
                        setDynamicFields({
                          ...dynamicFields,
                          extraFields: { ...dynamicFields.extraFields, [ek]: e.target.value },
                        })
                      }
                      placeholder={`Nhập nội dung cho mục ${ek}`}
                      className="w-full px-2.5 py-1.5 rounded-lg border border-[var(--mw-border)] text-xs"
                    />
                  </label>
                ))}
              </div>

              <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-[var(--mw-border)]">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handlePreview()}
                  className="mw-button mw-button-secondary"
                >
                  <Eye className="w-4 h-4 text-[var(--mw-accent)]" />
                  Xem trước văn bản
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handleApplyAndExport()}
                  className="mw-button mw-button-primary px-4 py-2"
                >
                  <Save className="w-4 h-4" />
                  Tạo và lưu file Word
                </button>
              </div>
            </div>
          </div>

          {/* Cột phải: 4. Xem trước & 5. Kết quả file Word */}
          <div className="xl:col-span-6 space-y-3">
            {applyResult && (
              <div
                className={`rounded-2xl border p-4 space-y-2 ${
                  applyResult.success
                    ? 'border-emerald-300 bg-emerald-50/70 dark:border-emerald-900 dark:bg-emerald-950/30'
                    : 'border-rose-300 bg-rose-50/70 dark:border-rose-900 dark:bg-rose-950/30'
                }`}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs font-bold flex items-center gap-1.5">
                    <CheckCircle2 className="w-4 h-4 text-emerald-600" />
                    {applyResult.success ? '5. Đã tạo và lưu file Word thành công' : 'Không thể tạo tệp'}
                  </span>
                  <span className="text-[11px] font-mono">
                    {Math.max(1, Math.round(applyResult.outputSizeBytes / 1024))} KB
                  </span>
                </div>
                <div className="text-xs break-all text-slate-700 dark:text-slate-200">
                  {applyResult.outputPath || applyResult.outputName || applyResult.error}
                </div>
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  {isNative && applyResult.success && applyResult.outputPath && (
                    <>
                      <button
                        type="button"
                        onClick={() =>
                          void bridge
                            .invoke('open_path', { path: applyResult.outputPath, reveal: false })
                            .catch((err) => onStatusToast('error', String(err)))
                        }
                        className="mw-button mw-button-primary text-xs"
                      >
                        <ExternalLink className="w-3.5 h-3.5" /> Mở bằng Word
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          void bridge
                            .invoke('open_path', { path: applyResult.outputPath, reveal: true })
                            .catch((err) => onStatusToast('error', String(err)))
                        }
                        className="mw-button mw-button-secondary text-xs"
                      >
                        <FolderOpen className="w-3.5 h-3.5" /> Mở thư mục
                      </button>
                    </>
                  )}
                  {!isNative && applyResult.blobUrl && applyResult.outputName && (
                    <>
                      <button
                        type="button"
                        onClick={() =>
                          triggerBrowserDownload(applyResult.blobUrl!, applyResult.outputName!)
                        }
                        className="mw-button mw-button-primary text-xs"
                      >
                        <Download className="w-3.5 h-3.5" /> Tải lại file Word
                      </button>
                      {applyResult.outputBlob && (
                        <button
                          type="button"
                          onClick={() =>
                            void shareOutputBlob(
                              applyResult.outputBlob!,
                              applyResult.outputName!,
                              applyResult.blobUrl,
                            )
                          }
                          className="mw-button mw-button-secondary text-xs"
                        >
                          <Share2 className="w-3.5 h-3.5" /> Chia sẻ
                        </button>
                      )}
                    </>
                  )}
                  {applyResult.backupPath && (
                    <button
                      type="button"
                      onClick={() => void handleRestoreTargetBackup()}
                      className="mw-button mw-button-secondary text-xs"
                    >
                      <RotateCcw className="w-3.5 h-3.5" /> Khôi phục bản gốc
                    </button>
                  )}
                </div>
              </div>
            )}

            {previewResult ? (
              <div className="mw-panel p-4 space-y-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <h3 className="text-sm font-bold text-[var(--mw-ink)]">
                    4. Xem trước nội dung văn bản
                  </h3>
                  <span
                    className={`px-2 py-0.5 rounded-full text-[11px] font-bold ${
                      previewResult.compatibility.missingFields.length === 0
                        ? 'bg-emerald-100 text-emerald-700'
                        : 'bg-amber-100 text-amber-800'
                    }`}
                  >
                    {previewResult.compatibility.missingFields.length === 0
                      ? 'Thông tin đầy đủ'
                      : `Còn ${previewResult.compatibility.missingFields.length} mục chưa điền`}
                  </span>
                </div>

                {previewResult.compatibility.missingFields.length > 0 && (
                  <div className="rounded-xl border border-amber-300 bg-amber-50 dark:border-amber-800 dark:bg-amber-950/30 p-3 space-y-1">
                    <div className="text-xs font-bold text-amber-800 dark:text-amber-200 flex items-center gap-1.5">
                      <AlertTriangle className="w-4 h-4 shrink-0" />
                      Các mục chưa nhập thông tin ({previewResult.compatibility.missingFields.length})
                    </div>
                    <ul className="text-[11px] text-amber-900 dark:text-amber-300 space-y-0.5">
                      {previewResult.compatibility.missingFields.map((m) => (
                        <li key={m.fieldKey}>
                          <strong>{m.labelVi}:</strong> sẽ giữ chỗ để bạn bổ sung sau trong Word
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {/* Bản xem trước bố cục trang văn bản */}
                <div className="rounded-xl border border-slate-300 dark:border-slate-700 bg-white text-slate-900 p-5 font-serif text-[12px] space-y-3 shadow-inner">
                  <div className="grid grid-cols-2 gap-3 text-center">
                    <div className="space-y-0.5">
                      {previewResult.headerLeftLines.map((l, idx) => (
                        <div key={idx} className={idx <= 1 ? 'font-bold text-[11px]' : 'text-[11px]'}>
                          {l}
                        </div>
                      ))}
                    </div>
                    <div className="space-y-0.5">
                      {previewResult.headerRightLines.map((l, idx) => (
                        <div
                          key={idx}
                          className={
                            idx < 2
                              ? 'font-bold text-[11px]'
                              : 'italic text-[11px] text-slate-700'
                          }
                        >
                          {l}
                        </div>
                      ))}
                    </div>
                  </div>

                  {previewResult.titleLines.length > 0 && (
                    <div className="text-center font-bold space-y-0.5 pt-2">
                      {previewResult.titleLines.map((l, idx) => (
                        <div key={idx}>{l}</div>
                      ))}
                    </div>
                  )}

                  <div className="space-y-2 pt-1">
                    {previewResult.bodyBlocks.map((b, idx) => (
                      <div key={idx} className="space-y-1">
                        {b.title && <div className="font-bold">{b.title}</div>}
                        {b.lines.map((ln, lIdx) => (
                          <div key={lIdx} className="leading-relaxed">
                            {ln}
                          </div>
                        ))}
                        {b.tablePreview && (
                          <div className="overflow-x-auto pt-1">
                            <table className="w-full border-collapse border border-slate-400 text-[10px]">
                              <thead>
                                <tr className="bg-slate-100">
                                  {b.tablePreview.columns.map((c, ci) => (
                                    <th key={ci} className="border border-slate-400 p-1">
                                      {c.header}
                                    </th>
                                  ))}
                                </tr>
                              </thead>
                              <tbody>
                                {b.tablePreview.sampleRows.map((row, ri) => (
                                  <tr key={ri}>
                                    {row.map((cell, ci) => (
                                      <td key={ci} className="border border-slate-400 p-1">
                                        {cell}
                                      </td>
                                    ))}
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>

                  <div className="grid grid-cols-2 gap-4 pt-3 text-[11px]">
                    <div>
                      {previewResult.recipientLines.length > 0 && (
                        <>
                          <div className="font-bold italic">Nơi nhận:</div>
                          {previewResult.recipientLines.map((r, i) => (
                            <div key={i}>{r}</div>
                          ))}
                        </>
                      )}
                    </div>
                    <div
                      className={`grid gap-2 text-center ${
                        previewResult.signatureColumns.length >= 2 ? 'grid-cols-2' : 'grid-cols-1'
                      }`}
                    >
                      {previewResult.signatureColumns.map((col, ci) => (
                        <div key={ci} className="space-y-1">
                          {col.map((line, li) => (
                            <div key={li} className={li === 0 ? 'font-bold' : ''}>
                              {line}
                            </div>
                          ))}
                        </div>
                      ))}
                    </div>
                  </div>
                </div>

                {adminMode && (
                  <details className="text-xs">
                    <summary className="cursor-pointer font-semibold text-slate-600 dark:text-slate-300">
                      Chi tiết 10 bước &amp; Tài sản OpenXML bảo toàn (?admin=1)
                    </summary>
                    <div className="mt-2 space-y-1 text-[11px] text-slate-600 dark:text-slate-300">
                      {previewResult.pipelineSteps.map((s) => (
                        <div key={s.stepNumber} className="flex items-start gap-2">
                          <span className="font-mono font-bold text-blue-600">{s.stepNumber}.</span>
                          <span>
                            <strong>{s.stepName}:</strong> {s.detail}
                          </span>
                        </div>
                      ))}
                    </div>
                  </details>
                )}
              </div>
            ) : (
              <div className="mw-panel p-6 text-center space-y-2">
                <LayoutTemplate className="w-8 h-8 text-[var(--mw-accent)] mx-auto" />
                <div className="text-sm font-bold text-[var(--mw-ink)]">
                  4. Xem trước &amp; 5. Tạo file Word
                </div>
                <p className="text-xs text-[var(--mw-muted)]">
                  Sau khi điền thông tin ở cột bên trái, bấm <strong>Xem trước văn bản</strong> để rà soát hoặc bấm <strong>Tạo và lưu file Word</strong> để xuất tệp ngay.
                </p>
              </div>
            )}
          </div>
        </div>
      )}

      {/* =================================================================== */}
      {/* SUBTAB 2: TEMPLATE DESIGNER                                         */}
      {/* =================================================================== */}
      {subTab === 'designer' && draftTpl && (
        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <h3 className="text-sm font-bold">
                Template Designer — {draftTpl.templateId ? draftTpl.name : 'Soạn mẫu mới'}
              </h3>
              <p className="text-[11px] text-slate-500 dark:text-slate-400">
                Mã mẫu: <span className="font-mono">{draftTpl.templateId || '(Tự sinh khi lưu)'}</span> · Phiên bản hiện tại: v{draftTpl.version}
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              {draftTpl.templateId && (
                <>
                  <button
                    type="button"
                    onClick={() => void handleCloneTemplate(draftTpl.templateId)}
                    className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 text-xs font-semibold"
                  >
                    <Copy className="w-3.5 h-3.5" /> Sao chép
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleSetDefault(draftTpl.templateId)}
                    className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg border border-amber-300 bg-amber-50 dark:bg-amber-950/30 text-amber-800 dark:text-amber-300 text-xs font-semibold"
                  >
                    <Star className="w-3.5 h-3.5" /> Đặt mặc định
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleExportDocxMaster(draftTpl.templateId)}
                    className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 text-xs font-semibold"
                  >
                    <Download className="w-3.5 h-3.5" /> Xuất DOCX
                  </button>
                  {!draftTpl.isSystem && (
                    <button
                      type="button"
                      onClick={() => void handleDeleteUserTemplate(draftTpl.templateId)}
                      className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg border border-rose-300 bg-rose-50 text-rose-700 text-xs font-semibold"
                    >
                      <Trash2 className="w-3.5 h-3.5" /> Xóa mẫu
                    </button>
                  )}
                </>
              )}
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs">
            <div>
              <label className="block font-semibold mb-1">Tên mẫu</label>
              <input
                type="text"
                value={draftTpl.name}
                onChange={(e) => setDraftTpl({ ...draftTpl, name: e.target.value })}
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
              />
            </div>
            <div>
              <label className="block font-semibold mb-1">Nhóm văn bản</label>
              <select
                value={draftTpl.group}
                onChange={(e) =>
                  setDraftTpl({ ...draftTpl, group: e.target.value as TemplateGroup })
                }
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
              >
                <option value="administrative">Hành chính</option>
                <option value="enterprise">Doanh nghiệp</option>
                <option value="party">Văn bản Đảng</option>
                <option value="academic">Luận văn / Học thuật</option>
                <option value="custom">Tùy chỉnh</option>
              </select>
            </div>
            <div>
              <label className="block font-semibold mb-1">Loại văn bản (docType)</label>
              <input
                type="text"
                value={draftTpl.docType}
                onChange={(e) => setDraftTpl({ ...draftTpl, docType: e.target.value })}
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 font-mono"
              />
            </div>
            <div>
              <label className="block font-semibold mb-1">Phạm vi áp dụng</label>
              <select
                value={draftTpl.scope}
                onChange={(e) => setDraftTpl({ ...draftTpl, scope: e.target.value as StdDocScope })}
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
              >
                <option value="adminNd30">NĐ 30/2020/NĐ-CP</option>
                <option value="enterpriseInternal">Nội bộ Doanh nghiệp</option>
                <option value="partyDocument">Văn bản Đảng</option>
                <option value="academicThesis">Luận văn / Học thuật</option>
                <option value="freeform">Tài liệu tự do</option>
              </select>
            </div>
            <div>
              <label className="block font-semibold mb-1">Loại đầu trang cơ sở</label>
              <select
                value={draftTpl.headerConfig.baseAdminDocType}
                onChange={(e) =>
                  setDraftTpl({
                    ...draftTpl,
                    headerConfig: {
                      ...draftTpl.headerConfig,
                      baseAdminDocType: e.target.value as AdminDocType,
                    },
                  })
                }
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
              >
                {ADMIN_DOC_TYPE_META.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="block font-semibold mb-1">Trạng thái sử dụng</label>
              <select
                value={draftTpl.status}
                onChange={(e) =>
                  setDraftTpl({ ...draftTpl, status: e.target.value as TemplateUsageStatus })
                }
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
              >
                <option value="active">Đang sử dụng (Active)</option>
                <option value="draft">Bản nháp (Draft)</option>
                <option value="archived">Lưu trữ (Archived)</option>
              </select>
            </div>
            <div>
              <label className="block font-semibold mb-1">Ký hiệu viết tắt (Số/Ký hiệu)</label>
              <input
                type="text"
                value={draftTpl.headerConfig.symbolPrefix}
                onChange={(e) =>
                  setDraftTpl({
                    ...draftTpl,
                    headerConfig: { ...draftTpl.headerConfig, symbolPrefix: e.target.value },
                  })
                }
                placeholder="Trống với Công văn; QĐ / TTr / BC..."
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
              />
            </div>
            <div>
              <label className="block font-semibold mb-1">Kiểu khối chữ ký</label>
              <select
                value={draftTpl.signatureConfig.layout}
                onChange={(e) =>
                  setDraftTpl({
                    ...draftTpl,
                    signatureConfig: {
                      ...draftTpl.signatureConfig,
                      layout: e.target.value as SignatureLayoutKind,
                    },
                  })
                }
                className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
              >
                <option value="singleRight">{SIG_LABELS.singleRight}</option>
                <option value="dualSecretaryChair">{SIG_LABELS.dualSecretaryChair}</option>
                <option value="tripleApproval">{SIG_LABELS.tripleApproval}</option>
                <option value="none">{SIG_LABELS.none}</option>
              </select>
            </div>
            <div>
              <label className="block font-semibold mb-1">Khổ giấy &amp; Đánh số trang</label>
              <div className="flex gap-1.5">
                <select
                  value={draftTpl.formatRules.paperSize}
                  onChange={(e) =>
                    setDraftTpl({
                      ...draftTpl,
                      formatRules: {
                        ...draftTpl.formatRules,
                        paperSize: e.target.value as PaperSize,
                      },
                    })
                  }
                  className="px-2 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                >
                  <option value="a4">A4</option>
                  <option value="a3">A3</option>
                  <option value="letter">Letter</option>
                </select>
                <select
                  value={draftTpl.footerConfig.pageNumberPosition}
                  onChange={(e) =>
                    setDraftTpl({
                      ...draftTpl,
                      footerConfig: {
                        ...draftTpl.footerConfig,
                        pageNumberPosition: e.target.value as PageNumPosition,
                      },
                    })
                  }
                  className="flex-1 px-2 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                >
                  <option value="topCenter">Giữa trên (NĐ 30)</option>
                  <option value="topRight">Phải trên</option>
                  <option value="bottomCenter">Giữa dưới</option>
                  <option value="bottomRight">Phải dưới</option>
                </select>
              </div>
            </div>
          </div>

          {/* Format Rules Margins */}
          <div className="grid grid-cols-2 sm:grid-cols-5 gap-2 text-xs">
            {(
              [
                ['Lề Trên (mm)', 'marginTopMm'],
                ['Lề Dưới (mm)', 'marginBottomMm'],
                ['Lề Trái (mm)', 'marginLeftMm'],
                ['Lề Phải (mm)', 'marginRightMm'],
                ['Cỡ chữ (pt)', 'bodyFontSizePt'],
              ] as const
            ).map(([label, key]) => (
              <div key={key}>
                <label className="block font-semibold text-[11px] mb-1">{label}</label>
                <input
                  type="number"
                  value={draftTpl.formatRules[key]}
                  onChange={(e) =>
                    setDraftTpl({
                      ...draftTpl,
                      formatRules: {
                        ...draftTpl.formatRules,
                        [key]: parseFloat(e.target.value) || 0,
                      },
                    })
                  }
                  className="w-full px-2 py-1 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                />
              </div>
            ))}
          </div>

          {/* Section Content Builder */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold">Cấu trúc các phần nội dung ({draftTpl.contentStructure.length})</span>
              <button
                type="button"
                onClick={() =>
                  setDraftTpl({
                    ...draftTpl,
                    contentStructure: [
                      ...draftTpl.contentStructure,
                      {
                        sectionId: `sec-${Date.now()}`,
                        title: 'Mục bổ sung mới',
                        kind: 'headingSection',
                        required: false,
                        enabledByDefault: true,
                        headingAlignment: 'left',
                        headingBold: true,
                        headingItalic: false,
                        headingUppercase: false,
                        bodyParagraphs: ['[Hướng dẫn nhập nội dung mục này.]'],
                        inlineTable: null,
                      },
                    ],
                  })
                }
                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-100 dark:bg-slate-800 text-xs font-bold"
              >
                <Plus className="w-3.5 h-3.5" /> Thêm khối nội dung
              </button>
            </div>
            <div className="space-y-2 max-h-72 overflow-y-auto pr-1">
              {draftTpl.contentStructure.map((sec, idx) => (
                <div
                  key={sec.sectionId}
                  className="rounded-xl border border-slate-200 dark:border-slate-800 p-3 space-y-2 text-xs"
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <input
                      type="text"
                      value={sec.title}
                      onChange={(e) => updateDesignerSection(idx, { title: e.target.value })}
                      placeholder="Tiêu đề mục (có thể để trống)"
                      className="flex-1 px-2 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 font-semibold"
                    />
                    <select
                      value={sec.kind}
                      onChange={(e) =>
                        updateDesignerSection(idx, { kind: e.target.value as TemplateBlockKind })
                      }
                      className="px-2 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                    >
                      <option value="recipientSalutation">Kính gửi</option>
                      <option value="authorityHeading">Thẩm quyền ban hành</option>
                      <option value="preambleBasis">Căn cứ (Nghiêng)</option>
                      <option value="decisionKeyword">Từ khóa QUYẾT ĐỊNH:</option>
                      <option value="headingSection">Đề mục nội dung</option>
                      <option value="articleClause">Điều khoản</option>
                      <option value="paragraphBlock">Đoạn văn thường</option>
                      <option value="tableBlock">Khối bảng biểu</option>
                      <option value="metadataBlock">Thông tin thời gian/địa điểm</option>
                    </select>
                    <label className="inline-flex items-center gap-1">
                      <input
                        type="checkbox"
                        checked={sec.required}
                        onChange={(e) =>
                          updateDesignerSection(idx, { required: e.target.checked })
                        }
                      />
                      Bắt buộc
                    </label>
                    <button
                      type="button"
                      onClick={() =>
                        setDraftTpl({
                          ...draftTpl,
                          contentStructure: draftTpl.contentStructure.filter((_, i) => i !== idx),
                        })
                      }
                      className="p-1 text-rose-500 hover:bg-rose-50 rounded"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                  <textarea
                    rows={2}
                    value={sec.bodyParagraphs.join('\n')}
                    onChange={(e) =>
                      updateDesignerSection(idx, {
                        bodyParagraphs: e.target.value ? e.target.value.split('\n') : [],
                      })
                    }
                    placeholder="Mỗi dòng là một đoạn gợi ý hoặc {{bien_dong}}..."
                    className="w-full px-2 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 font-mono text-[11px]"
                  />
                </div>
              ))}
            </div>
          </div>

          {draftTpl.isSystem && (
            <label className="flex items-start gap-2 rounded-xl border border-amber-300 bg-amber-50 dark:border-amber-800 dark:bg-amber-950/30 p-3 text-xs cursor-pointer">
              <input
                type="checkbox"
                checked={confirmSystemOverride}
                onChange={(e) => setConfirmSystemOverride(e.target.checked)}
                className="mt-0.5"
              />
              <span>
                <strong>Xác nhận ghi đè mẫu hệ thống:</strong> Thay đổi trực tiếp mẫu hệ thống (tạo phiên bản mới để có thể khôi phục v1.0.0 bất cứ lúc nào). Nếu không muốn sửa mẫu gốc, hãy bấm <strong>Sao chép</strong>.
              </span>
            </label>
          )}

          <div className="flex flex-wrap items-center gap-2 pt-1">
            <input
              type="text"
              value={changeNote}
              onChange={(e) => setChangeNote(e.target.value)}
              placeholder="Ghi chú phiên bản thay đổi (tùy chọn)..."
              className="flex-1 min-w-48 px-2.5 py-1.5 rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
            />
            <button
              type="button"
              onClick={() => void handleSaveDesigner()}
              className="inline-flex items-center gap-1.5 px-4 py-2 rounded-xl bg-blue-600 hover:bg-blue-700 text-white text-xs font-bold"
            >
              <Save className="w-3.5 h-3.5" /> Lưu mẫu &amp; Tăng phiên bản
            </button>
          </div>
        </div>
      )}

      {/* =================================================================== */}
      {/* SUBTAB 3: VERSION HISTORY & RESTORE                                 */}
      {/* =================================================================== */}
      {subTab === 'versions' && selectedTemplate && (
        <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-3">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-bold flex items-center gap-2">
                <Archive className="w-4 h-4 text-blue-600" />
                Lịch sử phiên bản: {selectedTemplate.name}
              </h3>
              <p className="text-xs text-slate-500">
                Phiên bản đang kích hoạt: <strong>v{selectedTemplate.version}</strong>
              </p>
            </div>
          </div>

          <div className="divide-y divide-slate-200 dark:divide-slate-800 border border-slate-200 dark:border-slate-800 rounded-xl">
            {versions.length === 0 ? (
              <div className="p-4 text-xs text-slate-500">Chưa có bản ghi lịch sử nào.</div>
            ) : (
              versions
                .slice()
                .reverse()
                .map((ver) => (
                  <div
                    key={ver.version}
                    className="p-3 flex flex-wrap items-center justify-between gap-2 text-xs"
                  >
                    <div>
                      <div className="font-bold flex items-center gap-2">
                        <span className="font-mono text-blue-600">v{ver.version}</span>
                        <span>{ver.changeNote}</span>
                        {ver.version === selectedTemplate.version && (
                          <span className="px-1.5 py-0.5 rounded bg-emerald-100 text-emerald-700 text-[10px]">
                            Hiện tại
                          </span>
                        )}
                      </div>
                      <div className="text-[11px] text-slate-500 mt-0.5">
                        Lưu lúc: {ver.savedAt || 'Mặc định'} · {ver.snapshot.contentStructure.length} khối nội dung
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => void handleRestoreVersion(ver.version)}
                      className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800 text-xs font-semibold"
                    >
                      <RotateCcw className="w-3.5 h-3.5" /> Khôi phục bản này
                    </button>
                  </div>
                ))
            )}
          </div>
        </div>
      )}
    </div>
  );
};
