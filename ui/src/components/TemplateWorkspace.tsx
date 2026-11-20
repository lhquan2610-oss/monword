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
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

type SubTab = 'apply' | 'designer' | 'versions';
type GroupFilter = 'all' | 'administrative' | 'enterprise' | 'mine';

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
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();
  const webImportDocxInputRef = useRef<HTMLInputElement>(null);

  const [subTab, setSubTab] = useState<SubTab>('apply');
  const [groupFilter, setGroupFilter] = useState<GroupFilter>('all');
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
  const [dynamicFields, setDynamicFields] = useState<DynamicTemplateFields>(EMPTY_DYNAMIC_FIELDS);

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
      if (isNative) {
        const [cat, orgs] = await Promise.all([
          bridge.invoke<TemplateCatalogResponse>('list_templates', {}),
          bridge.invoke<OrganizationProfile[]>('list_org_profiles', {}),
        ]);
        setCatalog(cat);
        setOrgProfiles(orgs);
      } else {
        setCatalog(listWebTemplates());
        setOrgProfiles(listWebOrgProfiles());
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
    return catalog.templates.filter((t) => {
      if (groupFilter === 'administrative' && t.group !== 'administrative') return false;
      if (groupFilter === 'enterprise' && t.group !== 'enterprise') return false;
      if (groupFilter === 'mine' && t.isSystem) return false;
      if (!q) return true;
      return (
        t.name.toLowerCase().includes(q) ||
        t.docType.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q)
      );
    });
  }, [catalog.templates, groupFilter, searchQuery]);

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
      onStatusToast('error', 'Vui lòng chọn tệp DOCX từ hàng đợi để áp dụng mẫu.');
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
      onStatusToast('info', 'Đã hoàn tất phân tích 10 bước & kiểm tra tương thích mẫu.');
    } catch (err) {
      onStatusToast('error', (err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const handleApplyAndExport = async () => {
    if (!selectedTemplate) return;
    if (targetMode === 'applyToExistingDocx' && !selectedTargetPath) {
      onStatusToast('error', 'Vui lòng chọn tệp DOCX mục tiêu trước khi áp dụng mẫu.');
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
          onStatusToast('success', `Đã xuất tệp DOCX hoàn chỉnh: ${res.outputPath}`);
        } else {
          onStatusToast('error', res.error || 'Không thể áp dụng mẫu.');
        }
      } else {
        const targetItem = docxQueueFiles.find((f) => f.path === selectedTargetPath);
        const res = await applyWebTemplateToDocx(req, selectedOrgProfile, targetItem);
        setApplyResult(res);
        if (res.preview) setPreviewResult(res.preview);
        if (res.success && res.blobUrl && res.outputName) {
          triggerBrowserDownload(res.blobUrl, res.outputName);
          onStatusToast('success', `Đã tạo & tải xuống: ${res.outputName}`);
        } else if (!res.success) {
          onStatusToast('error', res.error || 'Không thể áp dụng mẫu.');
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
      <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-bold flex items-center gap-2">
              <LayoutTemplate className="w-5 h-5 text-blue-600" />
              Template Engine — Thư viện &amp; Thiết kế Mẫu văn bản
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
              Kế thừa Organization Profile (Phase 1), Header Builder (Phase 2) và Kiểm chuẩn NĐ 30 (Phase 3) · Hiện có{' '}
              <strong>{catalog.templates.length}</strong> mẫu.
            </p>
          </div>
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
        </div>

        {/* Group Filter + Quick Designer Bar */}
        <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-slate-100 dark:border-slate-800">
          <div className="flex flex-wrap items-center gap-1.5">
            {(
              [
                { id: 'all', label: `Tất cả (${catalog.templates.length})` },
                {
                  id: 'administrative',
                  label: `Hành chính (${catalog.templates.filter((t) => t.group === 'administrative').length})`,
                },
                {
                  id: 'enterprise',
                  label: `Doanh nghiệp (${catalog.templates.filter((t) => t.group === 'enterprise').length})`,
                },
                {
                  id: 'mine',
                  label: `Mẫu người dùng (${catalog.templates.filter((t) => !t.isSystem).length})`,
                },
              ] as { id: GroupFilter; label: string }[]
            ).map((g) => (
              <button
                key={g.id}
                type="button"
                onClick={() => setGroupFilter(g.id)}
                className={`px-2.5 py-1 rounded-lg text-xs font-semibold ${
                  groupFilter === g.id
                    ? 'bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900'
                    : 'bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300'
                }`}
              >
                {g.label}
              </button>
            ))}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Tìm tên mẫu, ký hiệu..."
              className="px-2.5 py-1 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs w-44"
            />
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
          </div>
        </div>

        {/* Inline Import DOCX Metadata Box */}
        {importModalOpen && (
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

      {/* Template Grid Selector */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-2.5 max-h-64 overflow-y-auto pr-1">
        {filteredTemplates.map((tpl) => {
          const active = selectedTemplate?.templateId === tpl.templateId;
          return (
            <button
              key={tpl.templateId}
              type="button"
              onClick={() => setSelectedId(tpl.templateId)}
              className={`text-left rounded-xl border p-3 transition-all ${
                active
                  ? 'border-blue-600 bg-blue-50/70 dark:bg-blue-950/30 ring-1 ring-blue-500'
                  : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 hover:border-blue-300'
              }`}
            >
              <div className="flex items-start justify-between gap-2">
                <span className="text-xs font-bold text-slate-900 dark:text-slate-100 line-clamp-1">
                  {tpl.name}
                </span>
                <div className="flex items-center gap-1 shrink-0">
                  {tpl.isDefaultForDocType && (
                    <span title="Mặc định cho loại văn bản này" className="text-amber-500">
                      <Star className="w-3.5 h-3.5 fill-current" />
                    </span>
                  )}
                  <span
                    className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${
                      tpl.isSystem
                        ? 'bg-slate-200 dark:bg-slate-800 text-slate-700 dark:text-slate-300'
                        : 'bg-emerald-100 dark:bg-emerald-950 text-emerald-700 dark:text-emerald-300'
                    }`}
                  >
                    {tpl.isSystem ? 'Hệ thống' : 'Người dùng'}
                  </span>
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-1.5 mt-1 text-[10px] text-slate-500 dark:text-slate-400">
                <span className="font-mono font-semibold text-blue-600 dark:text-blue-400">
                  {tpl.headerConfig.symbolPrefix || tpl.docType}
                </span>
                <span>·</span>
                <span>{GROUP_LABELS[tpl.group]}</span>
                <span>·</span>
                <span>v{tpl.version}</span>
                {tpl.sourceDocxAssetId && (
                  <>
                    <span>·</span>
                    <span className="text-indigo-600 dark:text-indigo-400 font-semibold">DOCX gốc</span>
                  </>
                )}
              </div>
              <p className="text-[11px] text-slate-500 dark:text-slate-400 mt-1 line-clamp-2">
                {tpl.description || `${tpl.contentStructure.length} thành phần nội dung`}
              </p>
            </button>
          );
        })}
      </div>

      {/* =================================================================== */}
      {/* SUBTAB 1: APPLY & PREVIEW WORKSPACE                                 */}
      {/* =================================================================== */}
      {subTab === 'apply' && selectedTemplate && (
        <div className="grid grid-cols-1 xl:grid-cols-12 gap-4">
          {/* Left Configuration Column */}
          <div className="xl:col-span-6 space-y-3">
            <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-bold">1. Hồ sơ Tổ chức &amp; Đích áp dụng</h3>
                <div className="flex items-center gap-1.5">
                  <button
                    type="button"
                    onClick={() => void handleCloneTemplate(selectedTemplate.templateId)}
                    className="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 text-[11px] font-semibold"
                    title="Sao chép mẫu để tùy chỉnh riêng"
                  >
                    <Copy className="w-3 h-3" /> Sao chép
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleExportDocxMaster(selectedTemplate.templateId)}
                    className="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 text-[11px] font-semibold"
                    title="Xuất tệp DOCX mẫu gốc"
                  >
                    <Download className="w-3 h-3" /> Xuất mẫu DOCX
                  </button>
                </div>
              </div>

              <div>
                <label className="block text-[11px] font-semibold text-slate-600 dark:text-slate-300 mb-1">
                  Hồ sơ Tổ chức (Phase 1 — hỗ trợ đa hồ sơ)
                </label>
                <select
                  value={selectedOrgId}
                  onChange={(e) => setSelectedOrgId(e.target.value)}
                  className="w-full px-2.5 py-1.5 rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                >
                  <option value="">-- Chưa chọn Hồ sơ (dùng nhập tay / đánh dấu trường thiếu) --</option>
                  {orgProfiles.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.profileName} — {o.issuingOrg} ({o.sector})
                    </option>
                  ))}
                </select>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => setTargetMode('createNewDocx')}
                  className={`p-2.5 rounded-xl border text-left text-xs ${
                    targetMode === 'createNewDocx'
                      ? 'border-blue-600 bg-blue-50/70 dark:bg-blue-950/30 font-bold text-blue-700 dark:text-blue-300'
                      : 'border-slate-200 dark:border-slate-700'
                  }`}
                >
                  <div className="flex items-center gap-1.5">
                    <FilePlus2 className="w-3.5 h-3.5" /> Tạo tệp DOCX mới
                  </div>
                  <div className="text-[10px] font-normal text-slate-500 mt-0.5">
                    Khởi tạo văn bản hoàn chỉnh từ mẫu
                  </div>
                </button>
                <button
                  type="button"
                  onClick={() => setTargetMode('applyToExistingDocx')}
                  className={`p-2.5 rounded-xl border text-left text-xs ${
                    targetMode === 'applyToExistingDocx'
                      ? 'border-blue-600 bg-blue-50/70 dark:bg-blue-950/30 font-bold text-blue-700 dark:text-blue-300'
                      : 'border-slate-200 dark:border-slate-700'
                  }`}
                >
                  <div className="flex items-center gap-1.5">
                    <FolderOpen className="w-3.5 h-3.5" /> Áp dụng vào DOCX sẵn có
                  </div>
                  <div className="text-[10px] font-normal text-slate-500 mt-0.5">
                    Giữ nguyên bảng, logo, section &amp; điền biến
                  </div>
                </button>
              </div>

              {targetMode === 'applyToExistingDocx' && (
                <div className="rounded-xl bg-slate-50 dark:bg-slate-950/60 border border-slate-200 dark:border-slate-800 p-2.5 space-y-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[11px] font-semibold">Tệp DOCX mục tiêu:</span>
                    <button
                      type="button"
                      onClick={onPickFiles}
                      className="text-[11px] font-bold text-blue-600 hover:underline"
                    >
                      + Mở thêm DOCX
                    </button>
                  </div>
                  <select
                    value={selectedTargetPath}
                    onChange={(e) => setSelectedTargetPath(e.target.value)}
                    className="w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
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
                      Chèn khung chữ ký nếu tệp gốc chưa có
                    </label>
                    <select
                      value={outputMode}
                      onChange={(e) => setOutputMode(e.target.value as OutputMode)}
                      className="px-2 py-1 rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900"
                    >
                      <option value="newCopy">Tạo bản sao (_TheoMau)</option>
                      <option value="overwriteWithBackup">Ghi đè + Backup (.bak)</option>
                    </select>
                  </div>
                </div>
              )}
            </div>

            {/* Dynamic Fields */}
            <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-2.5">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-bold">2. Dữ liệu động (Dynamic Fields)</h3>
                <button
                  type="button"
                  onClick={() => setDynamicFields(EMPTY_DYNAMIC_FIELDS)}
                  className="text-[11px] text-slate-500 hover:text-slate-800"
                >
                  Xóa trắng để kiểm tra đánh dấu thiếu
                </button>
              </div>
              <p className="text-[11px] text-slate-500 dark:text-slate-400">
                Trường nào chưa nhập (và không có trong Hồ sơ Tổ chức) sẽ được đánh dấu{' '}
                <code className="px-1 rounded bg-amber-100 dark:bg-amber-950 text-amber-800 dark:text-amber-200">
                  [CHƯA NHẬP: ...]
                </code>
                , không tự điền thông tin giả.
              </p>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs">
                <input
                  type="text"
                  value={dynamicFields.tenCoQuan}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, tenCoQuan: e.target.value })}
                  placeholder={selectedOrgProfile?.issuingOrg || 'Tên cơ quan (lấy từ Hồ sơ nếu trống)'}
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                />
                <input
                  type="text"
                  value={dynamicFields.soVanBan}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, soVanBan: e.target.value })}
                  placeholder="Số văn bản (vd: 125 hoặc 125/QĐ-UBND)"
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                />
                <input
                  type="text"
                  value={dynamicFields.diaDanh}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, diaDanh: e.target.value })}
                  placeholder={selectedOrgProfile?.defaultLocation || 'Địa danh ban hành'}
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                />
                <input
                  type="text"
                  value={dynamicFields.ngayThang}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, ngayThang: e.target.value })}
                  placeholder="Ngày tháng (vd: 05/02/2026)"
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                />
                <input
                  type="text"
                  value={dynamicFields.chucVu}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, chucVu: e.target.value })}
                  placeholder="Chức vụ người ký"
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                />
                <input
                  type="text"
                  value={dynamicFields.nguoiKy}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, nguoiKy: e.target.value })}
                  placeholder="Họ và tên người ký"
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950"
                />
                <input
                  type="text"
                  value={dynamicFields.nguoiNhan}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, nguoiNhan: e.target.value })}
                  placeholder="Người nhận / Kính gửi"
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 sm:col-span-2"
                />
                <input
                  type="text"
                  value={dynamicFields.trichYeu}
                  onChange={(e) => setDynamicFields({ ...dynamicFields, trichYeu: e.target.value })}
                  placeholder="Trích yếu nội dung văn bản"
                  className="px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-950 sm:col-span-2"
                />
                {customExtraKeys.map((ek) => (
                  <input
                    key={ek}
                    type="text"
                    value={dynamicFields.extraFields[ek] || ''}
                    onChange={(e) =>
                      setDynamicFields({
                        ...dynamicFields,
                        extraFields: { ...dynamicFields.extraFields, [ek]: e.target.value },
                      })
                    }
                    placeholder={`Trường tùy chỉnh: {{${ek}}}`}
                    className="px-2.5 py-1.5 rounded-lg border border-indigo-300 dark:border-indigo-700 bg-white dark:bg-slate-950"
                  />
                ))}
              </div>
            </div>

            {/* Required vs Optional Sections */}
            <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-2">
              <h3 className="text-sm font-bold">3. Thành phần bắt buộc &amp; tùy chọn</h3>
              <div className="space-y-1.5">
                {selectedTemplate.contentStructure.map((sec) => {
                  const checked = sec.required || enabledSectionIds.includes(sec.sectionId);
                  return (
                    <label
                      key={sec.sectionId}
                      className={`flex items-start justify-between gap-2 p-2 rounded-lg border text-xs ${
                        checked
                          ? 'border-slate-200 dark:border-slate-700 bg-slate-50/70 dark:bg-slate-950/40'
                          : 'border-dashed border-slate-200 dark:border-slate-800 opacity-65'
                      }`}
                    >
                      <span className="flex items-start gap-2">
                        <input
                          type="checkbox"
                          checked={checked}
                          disabled={sec.required}
                          onChange={() => toggleSection(sec)}
                          className="mt-0.5"
                        />
                        <span>
                          <span className="font-semibold block">
                            {sec.title || `(${sec.kind})`}
                          </span>
                          <span className="text-[11px] text-slate-500 line-clamp-1">
                            {sec.inlineTable
                              ? `Bảng biểu: ${sec.inlineTable.columns.map((c) => c.header).join(' | ')}`
                              : sec.bodyParagraphs[0] || ''}
                          </span>
                        </span>
                      </span>
                      <span
                        className={`px-1.5 py-0.5 rounded text-[10px] font-bold shrink-0 inline-flex items-center gap-1 ${
                          sec.required
                            ? 'bg-rose-100 text-rose-700 dark:bg-rose-950 dark:text-rose-300'
                            : 'bg-sky-100 text-sky-700 dark:bg-sky-950 dark:text-sky-300'
                        }`}
                      >
                        {sec.required && <Lock className="w-2.5 h-2.5" />}
                        {sec.required ? 'Bắt buộc' : 'Tùy chọn'}
                      </span>
                    </label>
                  );
                })}
              </div>

              <div className="flex flex-wrap items-center gap-2 pt-2">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handlePreview()}
                  className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 hover:bg-slate-50 text-xs font-bold"
                >
                  <Eye className="w-4 h-4 text-indigo-500" />
                  Xem trước &amp; Kiểm tra (Bước 1–9)
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handleApplyAndExport()}
                  className="inline-flex items-center gap-1.5 px-4 py-2 rounded-xl bg-blue-600 hover:bg-blue-700 text-white text-xs font-bold shadow-sm disabled:opacity-50"
                >
                  <Play className="w-3.5 h-3.5 fill-current" />
                  {targetMode === 'createNewDocx' ? 'Xuất DOCX từ mẫu (Bước 10)' : 'Áp dụng vào DOCX'}
                </button>
              </div>
            </div>
          </div>

          {/* Right Preview & Result Column */}
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
                    {applyResult.success ? 'Xuất DOCX & Xác minh Hậu lưu Đạt' : 'Lỗi xuất mẫu'}
                  </span>
                  <span className="text-[11px] font-mono">
                    {Math.max(1, Math.round(applyResult.outputSizeBytes / 1024))} KB
                  </span>
                </div>
                <div className="text-xs break-all text-slate-700 dark:text-slate-200">
                  {applyResult.outputPath || applyResult.error}
                </div>
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  {isNative && applyResult.success && applyResult.outputPath && (
                    <button
                      type="button"
                      onClick={() =>
                        void bridge.invoke('open_file', { path: applyResult.outputPath })
                      }
                      className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-900 text-white text-[11px] font-bold"
                    >
                      <ExternalLink className="w-3 h-3" /> Mở bằng Word
                    </button>
                  )}
                  {!isNative && applyResult.blobUrl && applyResult.outputName && (
                    <>
                      <button
                        type="button"
                        onClick={() =>
                          triggerBrowserDownload(applyResult.blobUrl!, applyResult.outputName!)
                        }
                        className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-blue-600 text-white text-[11px] font-bold"
                      >
                        <Download className="w-3 h-3" /> Tải lại DOCX
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
                          className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg border border-slate-300 bg-white text-slate-800 text-[11px] font-bold"
                        >
                          <Share2 className="w-3 h-3" /> Chia sẻ
                        </button>
                      )}
                    </>
                  )}
                  {applyResult.backupPath && (
                    <button
                      type="button"
                      onClick={() => void handleRestoreTargetBackup()}
                      className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg border border-amber-400 bg-amber-100 text-amber-900 text-[11px] font-bold"
                    >
                      <RotateCcw className="w-3 h-3" /> Khôi phục bản gốc
                    </button>
                  )}
                </div>
              </div>
            )}

            {previewResult ? (
              <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 space-y-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <h3 className="text-sm font-bold">Bản xem trước 10 bước &amp; Tương thích</h3>
                  <span
                    className={`px-2 py-0.5 rounded-full text-[11px] font-bold ${
                      previewResult.compatibility.compatible
                        ? 'bg-emerald-100 text-emerald-700'
                        : 'bg-amber-100 text-amber-800'
                    }`}
                  >
                    {previewResult.compatibility.passedRules} đạt ·{' '}
                    {previewResult.compatibility.failedRules} cảnh báo
                  </span>
                </div>

                {previewResult.compatibility.missingFields.length > 0 && (
                  <div className="rounded-xl border border-amber-300 bg-amber-50 dark:border-amber-800 dark:bg-amber-950/30 p-3 space-y-1">
                    <div className="text-xs font-bold text-amber-800 dark:text-amber-200 flex items-center gap-1.5">
                      <AlertTriangle className="w-4 h-4 shrink-0" />
                      Các trường chưa có dữ liệu ({previewResult.compatibility.missingFields.length})
                    </div>
                    <ul className="text-[11px] text-amber-900 dark:text-amber-300 space-y-0.5">
                      {previewResult.compatibility.missingFields.map((m) => (
                        <li key={m.fieldKey}>
                          <strong>{m.labelVi}:</strong>{' '}
                          <span className="font-mono">{m.markerText}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {/* Mini A4 Preview Sheet */}
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

                {/* 10-step trace */}
                <details className="text-xs">
                  <summary className="cursor-pointer font-semibold text-slate-600 dark:text-slate-300">
                    Chi tiết 10 bước áp dụng mẫu &amp; Tài sản OpenXML bảo toàn
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
                    <div className="pt-1 border-t border-slate-200 dark:border-slate-700 font-mono text-[10px]">
                      Media: {previewResult.compatibility.preservedAssets.mediaCount} · Drawings:{' '}
                      {previewResult.compatibility.preservedAssets.drawingCount} · Hyperlinks:{' '}
                      {previewResult.compatibility.preservedAssets.hyperlinkCount} · VBA:{' '}
                      {previewResult.compatibility.preservedAssets.hasVbaMacros ? 'Có (giữ nguyên)' : 'Không'}
                    </div>
                  </div>
                </details>
              </div>
            ) : (
              <div className="rounded-2xl border border-dashed border-slate-300 dark:border-slate-700 bg-slate-50/70 dark:bg-slate-900/40 p-6 text-center space-y-2">
                <LayoutTemplate className="w-8 h-8 text-blue-500 mx-auto" />
                <div className="text-sm font-bold">{selectedTemplate.name}</div>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  Bấm <strong>Xem trước &amp; Kiểm tra (Bước 1–9)</strong> để kiểm tra cấu trúc bố cục, đánh dấu dữ liệu thiếu và tính tương thích NĐ 30 trước khi xuất DOCX.
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
