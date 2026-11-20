import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  Building2,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Download,
  ExternalLink,
  FilePlus2,
  FileSearch,
  FolderOpen,
  HelpCircle,
  Image as ImageIcon,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  Scale,
  Share2,
  Sparkles,
  Trash2,
  Upload,
  Wand2,
} from 'lucide-react';
import { bridge } from '../bridge';
import {
  ADMIN_DOC_TYPE_META,
  DEFAULT_HEADER_BUILDER_CONFIG,
  type AdminDocType,
  type FileQueueItem,
  type HeaderApplyResult,
  type HeaderBuilderConfig,
  type HeaderClassification,
  type HeaderDateMode,
  type HeaderInspectionResult,
  type HeaderRuleMeta,
  type OrgSector,
  type OrganizationProfile,
  type RuleObligation,
} from '../types';
import { shareOutputBlob, triggerBrowserDownload } from '../utils/webDocxEngine';
import {
  applyWebDocxHeader,
  createStandaloneWebHeaderDocx,
  deleteWebOrgProfile,
  exportWebOrgProfiles,
  getWebHeaderRules,
  getWebInitialSnapshot,
  importWebOrgProfiles,
  inspectWebDocxHeader,
  listWebOrgProfiles,
  saveWebOrgProfile,
} from '../utils/webHeaderBuilder';

export interface HeaderBuilderWorkspaceProps {
  files: FileQueueItem[];
  onPickFiles: () => void;
  onReplaceWebFile?: (path: string, file: File) => void;
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

const HEADER_CFG_KEY = 'monword_header_builder_cfg_v1';

const SECTOR_META: Record<OrgSector, { label: string; hint: string; badgeClass: string }> = {
  stateAgency: {
    label: 'Cơ quan Nhà nước / ĐVSN',
    hint: 'Áp dụng bắt buộc thể thức Nghị định 30/2020/NĐ-CP',
    badgeClass: 'bg-blue-100 text-blue-700 dark:bg-blue-900/50 dark:text-blue-300',
  },
  partyOrg: {
    label: 'Tổ chức Đảng',
    hint: 'Tiêu đề ĐẢNG CỘNG SẢN VIỆT NAM theo Hướng dẫn 36-HD/VPTW',
    badgeClass: 'bg-red-100 text-red-700 dark:bg-red-900/50 dark:text-red-300',
  },
  enterprise: {
    label: 'Doanh nghiệp',
    hint: 'Linh hoạt mẫu nội bộ; có tùy chọn bật tương thích NĐ 30',
    badgeClass: 'bg-amber-100 text-amber-700 dark:bg-amber-900/50 dark:text-amber-300',
  },
  other: {
    label: 'Tổ chức khác / Hội',
    hint: 'Tùy chỉnh linh hoạt theo quy chế tổ chức',
    badgeClass: 'bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300',
  },
};

const CLASSIFICATION_STYLES: Record<
  HeaderClassification,
  { badge: string; box: string; icon: React.ReactNode }
> = {
  valid: {
    badge: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300',
    box: 'border-emerald-200 bg-emerald-50/60 dark:border-emerald-900/60 dark:bg-emerald-950/20',
    icon: <CheckCircle2 className="w-4 h-4 text-emerald-600" />,
  },
  needsStandardization: {
    badge: 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300',
    box: 'border-amber-200 bg-amber-50/60 dark:border-amber-900/60 dark:bg-amber-950/20',
    icon: <Sparkles className="w-4 h-4 text-amber-600" />,
  },
  missingComponents: {
    badge: 'bg-sky-100 text-sky-800 dark:bg-sky-950/60 dark:text-sky-300',
    box: 'border-sky-200 bg-sky-50/60 dark:border-sky-900/60 dark:bg-sky-950/20',
    icon: <FilePlus2 className="w-4 h-4 text-sky-600" />,
  },
  dataConflict: {
    badge: 'bg-rose-100 text-rose-700 dark:bg-rose-950/60 dark:text-rose-300',
    box: 'border-rose-200 bg-rose-50/70 dark:border-rose-900/60 dark:bg-rose-950/20',
    icon: <AlertTriangle className="w-4 h-4 text-rose-600" />,
  },
  uncertain: {
    badge: 'bg-violet-100 text-violet-700 dark:bg-violet-950/60 dark:text-violet-300',
    box: 'border-violet-200 bg-violet-50/70 dark:border-violet-900/60 dark:bg-violet-950/20',
    icon: <HelpCircle className="w-4 h-4 text-violet-600" />,
  },
};

const OBLIGATION_LABELS: Record<RuleObligation, { text: string; cls: string }> = {
  mandatory: {
    text: 'Bắt buộc (NĐ 30)',
    cls: 'bg-blue-100 text-blue-700 dark:bg-blue-900/50 dark:text-blue-300',
  },
  docTypeConditional: {
    text: 'Theo loại VB',
    cls: 'bg-indigo-100 text-indigo-700 dark:bg-indigo-900/50 dark:text-indigo-300',
  },
  caseSpecific: {
    text: 'Theo ngữ cảnh',
    cls: 'bg-rose-100 text-rose-700 dark:bg-rose-900/50 dark:text-rose-300',
  },
  orgConfigurable: {
    text: 'Theo hồ sơ',
    cls: 'bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-300',
  },
  recommended: {
    text: 'Khuyến nghị',
    cls: 'bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300',
  },
};

function createEmptyOrgDraft(): OrganizationProfile {
  return {
    id: '',
    profileName: '',
    sector: 'stateAgency',
    governingOrg: '',
    issuingOrg: '',
    affiliatedUnit: '',
    departmentOrBranch: '',
    agencyAbbreviation: '',
    defaultLocation: '',
    useNd30LayoutForEnterprise: false,
    showGoverningOrgDefault: true,
    showAffiliatedUnitDefault: false,
    showDepartmentDefault: false,
    updatedAt: '',
  };
}

export const HeaderBuilderWorkspace: React.FC<HeaderBuilderWorkspaceProps> = ({
  files,
  onPickFiles,
  onReplaceWebFile,
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();

  const [profiles, setProfiles] = useState<OrganizationProfile[]>([]);
  const [rules, setRules] = useState<HeaderRuleMeta[]>([]);
  const [showOrgEditor, setShowOrgEditor] = useState(false);
  const [orgDraft, setOrgDraft] = useState<OrganizationProfile>(createEmptyOrgDraft);
  const [showRulesPanel, setShowRulesPanel] = useState(false);

  const [cfg, setCfg] = useState<HeaderBuilderConfig>(() => {
    try {
      const saved = localStorage.getItem(HEADER_CFG_KEY);
      if (saved) {
        return { ...DEFAULT_HEADER_BUILDER_CONFIG, ...JSON.parse(saved) };
      }
    } catch {
      // ignore invalid storage
    }
    return { ...DEFAULT_HEADER_BUILDER_CONFIG };
  });

  const wordFiles = useMemo(
    () =>
      files.filter((f) => {
        const ext = f.ext.toLowerCase();
        return ext === 'docx' || ext === 'docm' || ext === 'doc';
      }),
    [files],
  );

  const [selectedFilePath, setSelectedFilePath] = useState<string>('');
  const [inspecting, setInspecting] = useState(false);
  const [applying, setApplying] = useState(false);
  const [inspection, setInspection] = useState<HeaderInspectionResult | null>(null);
  const [lastResult, setLastResult] = useState<HeaderApplyResult | null>(null);
  const [initialBackups, setInitialBackups] = useState<Record<string, string>>({});

  const importJsonInputRef = useRef<HTMLInputElement>(null);

  // Persist HeaderBuilderConfig locally (excluding temporary selectedCandidateIndex)
  useEffect(() => {
    try {
      const toStore: HeaderBuilderConfig = { ...cfg, selectedCandidateIndex: null };
      localStorage.setItem(HEADER_CFG_KEY, JSON.stringify(toStore));
    } catch {
      // ignore
    }
  }, [cfg]);

  // Load initial profiles & rules
  useEffect(() => {
    let alive = true;
    const loadInit = async () => {
      if (isNative) {
        try {
          const [profList, ruleList] = await Promise.all([
            bridge.invoke<OrganizationProfile[]>('list_org_profiles'),
            bridge.invoke<HeaderRuleMeta[]>('get_header_rules'),
          ]);
          if (!alive) return;
          setProfiles(Array.isArray(profList) ? profList : []);
          setRules(Array.isArray(ruleList) ? ruleList : []);
        } catch (err) {
          if (alive) onStatusToast('error', `Không tải được Hồ sơ Tổ chức: ${(err as Error).message}`);
        }
      } else {
        setProfiles(listWebOrgProfiles());
        setRules(getWebHeaderRules());
      }
    };
    void loadInit();
    return () => {
      alive = false;
    };
  }, [isNative, onStatusToast]);

  // Keep selectedFilePath synced with queued Word files
  useEffect(() => {
    if (wordFiles.length === 0) {
      setSelectedFilePath('');
      setInspection(null);
      return;
    }
    if (!wordFiles.some((f) => f.path === selectedFilePath)) {
      setSelectedFilePath(wordFiles[0].path);
      setCfg((prev) => ({ ...prev, selectedCandidateIndex: null }));
    }
  }, [wordFiles, selectedFilePath]);

  const activeOrg = useMemo(
    () => profiles.find((p) => p.id === cfg.orgProfileId) ?? null,
    [profiles, cfg.orgProfileId],
  );

  const selectedWordFile = useMemo(
    () => wordFiles.find((f) => f.path === selectedFilePath) ?? null,
    [wordFiles, selectedFilePath],
  );

  const selectOrgProfile = (id: string | null) => {
    const found = profiles.find((p) => p.id === id) ?? null;
    setCfg((prev) => ({
      ...prev,
      orgProfileId: id,
      showGoverningOrg: found ? found.showGoverningOrgDefault : prev.showGoverningOrg,
      showAffiliatedUnit: found ? found.showAffiliatedUnitDefault : prev.showAffiliatedUnit,
      showDepartment: found ? found.showDepartmentDefault : prev.showDepartment,
      showNationalHeader: found
        ? found.sector === 'enterprise' || found.sector === 'other'
          ? found.useNd30LayoutForEnterprise
          : true
        : prev.showNationalHeader,
    }));
  };

  // Run inspection on selected file
  const runInspection = useCallback(
    async (overrideCfg?: HeaderBuilderConfig) => {
      if (!selectedWordFile) {
        setInspection(null);
        return;
      }
      const effectiveCfg = overrideCfg ?? cfg;
      setInspecting(true);
      try {
        const res = isNative
          ? await bridge.invoke<HeaderInspectionResult>('inspect_doc_header', {
              path: selectedWordFile.path,
              config: effectiveCfg,
              orgProfile: activeOrg,
            })
          : await inspectWebDocxHeader(selectedWordFile, effectiveCfg, activeOrg);
        setInspection(res);
      } catch (err) {
        onStatusToast('error', `Lỗi nhận diện phần đầu: ${(err as Error).message}`);
      } finally {
        setInspecting(false);
      }
    },
    [selectedWordFile, cfg, isNative, activeOrg, onStatusToast],
  );

  useEffect(() => {
    if (selectedWordFile) {
      void runInspection();
    }
  }, [
    selectedWordFile?.path,
    cfg.orgProfileId,
    cfg.docType,
    cfg.selectedCandidateIndex,
    cfg.allowOverrideExistingNumber,
    cfg.allowOverrideExistingOrg,
    cfg.allowOverrideExistingDate,
    cfg.numberMode,
    cfg.dateMode,
  ]);

  // ============================================================================
  // AREA 1 HANDLERS (ORGANIZATION PROFILE)
  // ============================================================================

  const handleOpenNewOrg = () => {
    setOrgDraft(createEmptyOrgDraft());
    setShowOrgEditor(true);
  };

  const handleEditCurrentOrg = () => {
    if (!activeOrg) return;
    setOrgDraft({ ...activeOrg });
    setShowOrgEditor(true);
  };

  const handleSaveOrg = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!orgDraft.profileName.trim() || !orgDraft.issuingOrg.trim()) {
      onStatusToast('error', 'Vui lòng nhập Tên hồ sơ và Tên cơ quan/tổ chức ban hành.');
      return;
    }
    try {
      const updated = isNative
        ? await bridge.invoke<OrganizationProfile[]>('save_org_profile', orgDraft)
        : saveWebOrgProfile(orgDraft);
      setProfiles(updated);
      const savedItem =
        updated.find((p) => p.id === orgDraft.id) ?? updated[updated.length - 1] ?? null;
      if (savedItem) {
        selectOrgProfile(savedItem.id);
      }
      setShowOrgEditor(false);
      onStatusToast('success', 'Đã lưu Hồ sơ Tổ chức.');
    } catch (err) {
      onStatusToast('error', `Không lưu được Hồ sơ: ${(err as Error).message}`);
    }
  };

  const handleDeleteCurrentOrg = async () => {
    if (!activeOrg) return;
    try {
      const updated = isNative
        ? await bridge.invoke<OrganizationProfile[]>('delete_org_profile', { id: activeOrg.id })
        : deleteWebOrgProfile(activeOrg.id);
      setProfiles(updated);
      setCfg((prev) => ({ ...prev, orgProfileId: null }));
      onStatusToast('info', `Đã xóa hồ sơ "${activeOrg.profileName}".`);
    } catch (err) {
      onStatusToast('error', `Không thể xóa hồ sơ: ${(err as Error).message}`);
    }
  };

  const handleExportOrgs = async () => {
    try {
      const jsonStr = isNative
        ? (await bridge.invoke<{ json: string }>('export_org_profiles')).json
        : exportWebOrgProfiles();
      const blob = new Blob([jsonStr], { type: 'application/json;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      triggerBrowserDownload(url, 'monword_org_profiles.json');
      setTimeout(() => URL.revokeObjectURL(url), 4000);
      onStatusToast('success', 'Đã xuất danh sách Hồ sơ Tổ chức ra JSON.');
    } catch (err) {
      onStatusToast('error', `Lỗi xuất JSON: ${(err as Error).message}`);
    }
  };

  const handleImportOrgsFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    try {
      const text = await file.text();
      const updated = isNative
        ? await bridge.invoke<OrganizationProfile[]>('import_org_profiles', { json: text })
        : importWebOrgProfiles(text);
      setProfiles(updated);
      onStatusToast('success', `Đã nhập thành công ${updated.length} Hồ sơ Tổ chức.`);
    } catch (err) {
      onStatusToast('error', `Không đọc được JSON hồ sơ: ${(err as Error).message}`);
    }
  };

  // ============================================================================
  // AREA 2 & AREA 4 HANDLERS (APPLY / CREATE / RESTORE)
  // ============================================================================

  const handleSuggestSymbolFromProfile = () => {
    const meta = ADMIN_DOC_TYPE_META.find((m) => m.id === cfg.docType);
    const prefix = meta?.symbolPrefix ?? 'CV';
    const abbr = activeOrg?.agencyAbbreviation.trim();
    const suggested = abbr ? `${prefix}-${abbr}` : prefix;
    setCfg((prev) => ({
      ...prev,
      numberMode: 'manualInput',
      docSymbol: suggested,
    }));
  };

  const handleApplyToActiveFile = async (mode: 'standardizeExisting' | 'createNew') => {
    if (!selectedWordFile) {
      onStatusToast('error', 'Vui lòng chọn ít nhất một tệp Word (.docx/.docm/.doc) trước.');
      return;
    }
    setApplying(true);
    try {
      const res = isNative
        ? (
            await bridge.invoke<HeaderApplyResult[]>('apply_doc_header', {
              paths: [selectedWordFile.path],
              config: cfg,
              mode,
              orgProfile: activeOrg,
            })
          )[0]
        : await applyWebDocxHeader(selectedWordFile, cfg, mode, activeOrg);

      setLastResult(res);
      if (res.initialBackupPath) {
        setInitialBackups((prev) => ({
          ...prev,
          [selectedWordFile.path]: prev[selectedWordFile.path] || res.initialBackupPath!,
        }));
      }
      if (res.inspectionAfter) {
        setInspection(res.inspectionAfter);
      }

      if (res.success) {
        if (!isNative && res.outputBlob && res.outputName && onReplaceWebFile) {
          const nextFile = new File([res.outputBlob], selectedWordFile.name, {
            type: res.outputBlob.type,
          });
          onReplaceWebFile(selectedWordFile.path, nextFile);
        }
        onStatusToast(
          'success',
          mode === 'createNew'
            ? 'Đã chèn phần đầu văn bản mới và giữ nguyên thân tài liệu.'
            : 'Đã chuẩn hóa phần đầu văn bản thành công.',
        );
      } else {
        onStatusToast('error', res.error || 'Không thể xử lý phần đầu văn bản.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi xử lý phần đầu: ${(err as Error).message}`);
    } finally {
      setApplying(false);
    }
  };

  const handleCreateStandaloneDocx = async () => {
    setApplying(true);
    try {
      const meta = ADMIN_DOC_TYPE_META.find((m) => m.id === cfg.docType);
      const defaultName = `VanBan_${meta?.symbolPrefix ?? 'Mau'}_ChuanND30.docx`;

      let res: HeaderApplyResult;
      if (isNative) {
        const picked = await bridge.invoke<{ paths: string[] }>('pick_save_docx', {
          defaultName,
        });
        const destPath = picked.paths?.[0];
        if (!destPath) {
          setApplying(false);
          return;
        }
        res = await bridge.invoke<HeaderApplyResult>('create_new_header_docx', {
          destPath,
          config: cfg,
          orgProfile: activeOrg,
        });
      } else {
        res = await createStandaloneWebHeaderDocx(cfg, activeOrg, defaultName);
        if (res.success && res.blobUrl && res.outputName) {
          triggerBrowserDownload(res.blobUrl, res.outputName);
        }
      }

      setLastResult(res);
      if (res.success) {
        onStatusToast('success', `Đã tạo tệp phần đầu mới: ${res.outputName || res.outputPath}`);
      } else {
        onStatusToast('error', res.error || 'Không thể tạo tệp .docx mới.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi tạo tệp mới: ${(err as Error).message}`);
    } finally {
      setApplying(false);
    }
  };

  const handleRestoreInitial = async () => {
    if (!selectedWordFile) return;
    const bak = initialBackups[selectedWordFile.path];
    try {
      if (isNative) {
        if (!bak) {
          onStatusToast('error', 'Chưa có bản sao lưu ban đầu cho tệp này.');
          return;
        }
        const res = await bridge.invoke<{ message: string }>('restore_header_initial', {
          originalPath: selectedWordFile.path,
          backupPath: bak,
        });
        onStatusToast('info', res.message);
        await runInspection();
      } else {
        const snap = getWebInitialSnapshot(selectedWordFile.path);
        if (!snap || !onReplaceWebFile) {
          onStatusToast('error', 'Không tìm thấy bản gốc trong bộ nhớ trình duyệt.');
          return;
        }
        onReplaceWebFile(selectedWordFile.path, snap);
        onStatusToast('info', `Đã khôi phục nội dung ban đầu của "${selectedWordFile.name}".`);
        const restoredItem: FileQueueItem = { ...selectedWordFile, webFile: snap };
        const insp = await inspectWebDocxHeader(restoredItem, cfg, activeOrg);
        setInspection(insp);
      }
      setLastResult(null);
    } catch (err) {
      onStatusToast('error', `Lỗi khôi phục: ${(err as Error).message}`);
    }
  };

  // ============================================================================
  // PREVIEW COMPUTATION (LIVE DATA BINDING: ORG + CONFIG + INSPECTED FIELDS)
  // ============================================================================

  const activeCand = useMemo(() => {
    if (!inspection || inspection.candidates.length === 0) return null;
    const idx =
      inspection.activeCandidateIndex ??
      cfg.selectedCandidateIndex ??
      0;
    return inspection.candidates[Math.min(idx, inspection.candidates.length - 1)] ?? null;
  }, [inspection, cfg.selectedCandidateIndex]);

  const previewData = useMemo(() => {
    const f = activeCand?.fields ?? null;
    const keepOrg = Boolean(f && !cfg.allowOverrideExistingOrg);

    const governing = cfg.showGoverningOrg
      ? (keepOrg && f?.governingOrg) || activeOrg?.governingOrg || ''
      : '';
    const issuing = cfg.showIssuingOrg
      ? (keepOrg && f?.issuingOrg) || activeOrg?.issuingOrg || '[Chưa chọn tên cơ quan]'
      : '';
    const affiliated = cfg.showAffiliatedUnit
      ? (keepOrg && f?.affiliatedUnit) || activeOrg?.affiliatedUnit || ''
      : '';
    const department = cfg.showDepartment
      ? (keepOrg && f?.department) || activeOrg?.departmentOrBranch || ''
      : '';

    const isParty = activeOrg?.sector === 'partyOrg';
    const national = cfg.showNationalHeader
      ? cfg.customNationalHeader.trim() ||
        (isParty ? 'ĐẢNG CỘNG SẢN VIỆT NAM' : 'CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM')
      : '';
    const motto =
      cfg.showNationalHeader && !isParty
        ? cfg.customMotto.trim() || 'Độc lập - Tự do - Hạnh phúc'
        : '';

    let numberLine = '';
    if (cfg.showNumberSymbol) {
      const canKeepNum =
        (cfg.numberMode === 'keepExisting' ||
          (Boolean(f?.numberValue || f?.symbolValue) && !cfg.allowOverrideExistingNumber)) &&
        Boolean(f?.numberRaw);
      if (canKeepNum && f?.numberRaw) {
        numberLine = f.numberRaw;
      } else {
        const n = cfg.docNumber.trim();
        const s = cfg.docSymbol.trim();
        if (!n && !s) numberLine = 'Số:        /';
        else if (!n && s) numberLine = `Số:      /${s}`;
        else if (n && !s) numberLine = `Số: ${n}`;
        else numberLine = `Số: ${n}/${s}`;
      }
    }

    let dateLine = '';
    if (cfg.showLocationDate) {
      const canKeepDate =
        (cfg.dateMode === 'keepExisting' ||
          (Boolean(f?.dateRaw) && !cfg.allowOverrideExistingDate)) &&
        Boolean(f?.dateRaw);
      if (canKeepDate && f?.dateRaw) {
        dateLine = f.dateRaw;
      } else {
        const loc = cfg.locationOverride.trim() || activeOrg?.defaultLocation.trim() || '';
        const prefix = loc ? `${loc}, ` : '';
        if (cfg.dateMode === 'useToday') {
          const now = new Date();
          dateLine = `${prefix}ngày ${String(now.getDate()).padStart(2, '0')} tháng ${String(now.getMonth() + 1).padStart(2, '0')} năm ${now.getFullYear()}`;
        } else if (cfg.dateMode === 'manualDate') {
          dateLine = `${prefix}ngày ${cfg.manualDay.trim() || '...'} tháng ${cfg.manualMonth.trim() || '...'} năm ${cfg.manualYear.trim() || '20...'}`;
        } else {
          dateLine = `${prefix}ngày ... tháng ... năm ...`;
        }
      }
    }

    const meta = ADMIN_DOC_TYPE_META.find((m) => m.id === cfg.docType);
    const docHeading = cfg.showDocTypeAndSubject
      ? cfg.customDocTypeLabel.trim() || f?.docTypeLabel || meta?.docTypeHeading || ''
      : '';
    const subject = cfg.showDocTypeAndSubject
      ? cfg.subjectText.trim() || f?.subject || ''
      : '';

    return {
      governing,
      issuing,
      affiliated,
      department,
      national,
      motto,
      numberLine,
      dateLine,
      docHeading,
      subject,
      summary: cfg.summaryLine.trim() || f?.summaryLine || '',
      classMark: cfg.classificationMark.trim() || f?.classificationMark || '',
      isCongVan: cfg.docType === 'congVan',
      isParty,
      hasLogo: activeCand?.hasLogoOrImage ?? false,
    };
  }, [activeCand, activeOrg, cfg]);

  const applicableRules = useMemo(() => {
    if (inspection?.applicableRules?.length) return inspection.applicableRules;
    const isParty = activeOrg?.sector === 'partyOrg';
    const isEnt = activeOrg?.sector === 'enterprise' || activeOrg?.sector === 'other';
    const entNd30 = activeOrg?.useNd30LayoutForEnterprise ?? false;
    return rules.filter((r) => {
      if (r.ruleId === 'DANG-36HD-01') return isParty;
      if (r.ruleId === 'ND30-PLI-01') return !isParty && (!isEnt || entNd30);
      if (r.ruleId === 'ND30-PLI-05') return cfg.docType !== 'congVan';
      if (r.ruleId === 'ND30-PLI-06') return cfg.docType === 'congVan';
      if (r.ruleId === 'ND30-PLI-09') return isEnt;
      return true;
    });
  }, [inspection, activeOrg, rules, cfg.docType]);

  const hasInitialRestore = Boolean(
    selectedWordFile &&
      (initialBackups[selectedWordFile.path] ||
        (!isNative && getWebInitialSnapshot(selectedWordFile.path))),
  );

  // ============================================================================
  // RENDER
  // ============================================================================

  return (
    <div className="space-y-4">
      {/* Hidden JSON importer */}
      <input
        ref={importJsonInputRef}
        type="file"
        accept=".json,application/json"
        className="hidden"
        onChange={handleImportOrgsFile}
      />

      {/* KHU VỰC 1: CHỌN & QUẢN LÝ HỒ SƠ TỔ CHỨC (PHASE 1) */}
      <section className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 className="text-sm sm:text-base font-bold flex items-center gap-2">
              <Building2 className="w-4 h-4 text-blue-600" />
              1. Hồ sơ Tổ chức (Organization Profile)
            </h2>
            <p className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
              Lưu thông tin cơ quan chủ quản, cơ quan ban hành, địa danh mặc định và phân loại lĩnh vực.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              onClick={handleOpenNewOrg}
              className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-[11px] font-semibold bg-blue-600 text-white hover:bg-blue-700 cursor-pointer"
            >
              <Plus className="w-3.5 h-3.5" />
              Tạo hồ sơ
            </button>
            <button
              type="button"
              onClick={() => importJsonInputRef.current?.click()}
              className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-[11px] font-semibold border border-slate-200 dark:border-slate-700 hover:bg-slate-50 dark:hover:bg-slate-800 cursor-pointer"
            >
              <Upload className="w-3.5 h-3.5" />
              Nhập JSON
            </button>
            {profiles.length > 0 && (
              <button
                type="button"
                onClick={handleExportOrgs}
                className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-[11px] font-semibold border border-slate-200 dark:border-slate-700 hover:bg-slate-50 dark:hover:bg-slate-800 cursor-pointer"
              >
                <Download className="w-3.5 h-3.5" />
                Xuất JSON
              </button>
            )}
          </div>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <select
            value={cfg.orgProfileId ?? ''}
            onChange={(e) => selectOrgProfile(e.target.value || null)}
            className="flex-1 min-w-[220px] px-3 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-xs font-medium"
          >
            <option value="">-- Không chọn hồ sơ (lấy thông tin trực tiếp từ tệp / nhập tay) --</option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.profileName} — {p.issuingOrg}
              </option>
            ))}
          </select>

          {activeOrg && (
            <>
              <button
                type="button"
                onClick={handleEditCurrentOrg}
                className="inline-flex items-center gap-1 px-2.5 py-2 rounded-xl text-xs font-medium border border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer"
                title="Sửa hồ sơ đang chọn"
              >
                <Pencil className="w-3.5 h-3.5" />
                Sửa
              </button>
              <button
                type="button"
                onClick={handleDeleteCurrentOrg}
                className="inline-flex items-center gap-1 px-2.5 py-2 rounded-xl text-xs font-medium border border-rose-200 text-rose-600 dark:border-rose-900/60 dark:text-rose-400 hover:bg-rose-50 dark:hover:bg-rose-950/30 cursor-pointer"
                title="Xóa hồ sơ đang chọn"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            </>
          )}
        </div>

        {profiles.length === 0 && !showOrgEditor && (
          <div className="mt-3 p-3 rounded-xl border border-dashed border-slate-300 dark:border-slate-700 bg-slate-50/70 dark:bg-slate-950/40 text-xs text-slate-600 dark:text-slate-400">
            Chưa có Hồ sơ Tổ chức nào (MonWord không tự tạo dữ liệu giả). Bạn hãy bấm{' '}
            <button
              type="button"
              onClick={handleOpenNewOrg}
              className="font-semibold text-blue-600 dark:text-blue-400 underline cursor-pointer"
            >
              Tạo hồ sơ
            </button>{' '}
            để khai báo thông tin cơ quan hoặc doanh nghiệp của mình.
          </div>
        )}

        {activeOrg && !showOrgEditor && (
          <div className="mt-3 p-3 rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50/80 dark:bg-slate-950/40 grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs">
            <div>
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-bold text-slate-900 dark:text-slate-100">
                  {activeOrg.issuingOrg}
                </span>
                <span
                  className={`text-[10px] font-semibold px-2 py-0.5 rounded-full ${SECTOR_META[activeOrg.sector].badgeClass}`}
                >
                  {SECTOR_META[activeOrg.sector].label}
                </span>
              </div>
              {activeOrg.governingOrg && (
                <div className="text-[11px] text-slate-500 mt-0.5">
                  Chủ quản: <strong className="text-slate-700 dark:text-slate-300">{activeOrg.governingOrg}</strong>
                </div>
              )}
              {(activeOrg.affiliatedUnit || activeOrg.departmentOrBranch) && (
                <div className="text-[11px] text-slate-500 mt-0.5">
                  {[activeOrg.affiliatedUnit, activeOrg.departmentOrBranch].filter(Boolean).join(' · ')}
                </div>
              )}
            </div>
            <div className="text-[11px] text-slate-500 sm:text-right space-y-0.5">
              <div>
                Viết tắt ký hiệu:{' '}
                <strong className="text-slate-800 dark:text-slate-200">
                  {activeOrg.agencyAbbreviation || '(chưa đặt)'}
                </strong>
              </div>
              <div>
                Địa danh mặc định:{' '}
                <strong className="text-slate-800 dark:text-slate-200">
                  {activeOrg.defaultLocation || '(chưa đặt)'}
                </strong>
              </div>
            </div>
          </div>
        )}

        {/* Inline Organization Profile Editor */}
        {showOrgEditor && (
          <form
            onSubmit={handleSaveOrg}
            className="mt-3 p-3.5 rounded-xl border border-blue-200 dark:border-blue-900/60 bg-blue-50/40 dark:bg-blue-950/20 space-y-3 text-xs"
          >
            <div className="flex items-center justify-between">
              <span className="font-bold text-blue-700 dark:text-blue-300">
                {orgDraft.id ? 'Chỉnh sửa Hồ sơ Tổ chức' : 'Thêm Hồ sơ Tổ chức mới'}
              </span>
              <button
                type="button"
                onClick={() => setShowOrgEditor(false)}
                className="text-[11px] text-slate-500 hover:text-slate-800 dark:hover:text-slate-200 cursor-pointer"
              >
                Đóng
              </button>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
              <label className="block">
                <span className="text-[11px] font-semibold">Tên hồ sơ gợi nhớ *</span>
                <input
                  type="text"
                  required
                  value={orgDraft.profileName}
                  onChange={(e) => setOrgDraft({ ...orgDraft, profileName: e.target.value })}
                  placeholder="VD: Cơ quan / Công ty chính"
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                />
              </label>

              <label className="block">
                <span className="text-[11px] font-semibold">Khối / Lĩnh vực tổ chức</span>
                <select
                  value={orgDraft.sector}
                  onChange={(e) =>
                    setOrgDraft({ ...orgDraft, sector: e.target.value as OrgSector })
                  }
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                >
                  <option value="stateAgency">Cơ quan Nhà nước / ĐVSN (NĐ 30 bắt buộc)</option>
                  <option value="partyOrg">Tổ chức Đảng (Hướng dẫn 36-HD/VPTW)</option>
                  <option value="enterprise">Doanh nghiệp (linh hoạt)</option>
                  <option value="other">Tổ chức khác / Hội</option>
                </select>
              </label>

              <label className="block">
                <span className="text-[11px] font-semibold">Tên cơ quan chủ quản (nếu có)</span>
                <input
                  type="text"
                  value={orgDraft.governingOrg}
                  onChange={(e) => setOrgDraft({ ...orgDraft, governingOrg: e.target.value })}
                  placeholder="VD: ỦY BAN NHÂN DÂN ..."
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                />
              </label>

              <label className="block">
                <span className="text-[11px] font-semibold">Tên cơ quan / tổ chức ban hành *</span>
                <input
                  type="text"
                  required
                  value={orgDraft.issuingOrg}
                  onChange={(e) => setOrgDraft({ ...orgDraft, issuingOrg: e.target.value })}
                  placeholder="Nhập đúng tên cơ quan hoặc công ty"
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                />
              </label>

              <label className="block">
                <span className="text-[11px] font-semibold">Đơn vị trực thuộc (nếu sử dụng)</span>
                <input
                  type="text"
                  value={orgDraft.affiliatedUnit}
                  onChange={(e) => setOrgDraft({ ...orgDraft, affiliatedUnit: e.target.value })}
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                />
              </label>

              <label className="block">
                <span className="text-[11px] font-semibold">Phòng / Ban / Chi nhánh</span>
                <input
                  type="text"
                  value={orgDraft.departmentOrBranch}
                  onChange={(e) =>
                    setOrgDraft({ ...orgDraft, departmentOrBranch: e.target.value })
                  }
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                />
              </label>

              <label className="block">
                <span className="text-[11px] font-semibold">Chữ viết tắt ký hiệu văn bản</span>
                <input
                  type="text"
                  value={orgDraft.agencyAbbreviation}
                  onChange={(e) =>
                    setOrgDraft({ ...orgDraft, agencyAbbreviation: e.target.value })
                  }
                  placeholder="VD: UBND, VP, CTCP..."
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                />
              </label>

              <label className="block">
                <span className="text-[11px] font-semibold">Địa danh mặc định</span>
                <input
                  type="text"
                  value={orgDraft.defaultLocation}
                  onChange={(e) => setOrgDraft({ ...orgDraft, defaultLocation: e.target.value })}
                  placeholder="VD: Hà Nội, Huế, TP. Hồ Chí Minh"
                  className="mt-1 w-full px-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                />
              </label>
            </div>

            <div className="flex flex-wrap gap-4 pt-1 text-[11px]">
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={orgDraft.showGoverningOrgDefault}
                  onChange={(e) =>
                    setOrgDraft({ ...orgDraft, showGoverningOrgDefault: e.target.checked })
                  }
                />
                Mặc định hiện Cơ quan chủ quản
              </label>
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={orgDraft.showAffiliatedUnitDefault}
                  onChange={(e) =>
                    setOrgDraft({ ...orgDraft, showAffiliatedUnitDefault: e.target.checked })
                  }
                />
                Hiện Đơn vị trực thuộc
              </label>
              <label className="inline-flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={orgDraft.showDepartmentDefault}
                  onChange={(e) =>
                    setOrgDraft({ ...orgDraft, showDepartmentDefault: e.target.checked })
                  }
                />
                Hiện Phòng/Ban
              </label>
              {(orgDraft.sector === 'enterprise' || orgDraft.sector === 'other') && (
                <label className="inline-flex items-center gap-1.5 cursor-pointer text-amber-700 dark:text-amber-300 font-medium">
                  <input
                    type="checkbox"
                    checked={orgDraft.useNd30LayoutForEnterprise}
                    onChange={(e) =>
                      setOrgDraft({ ...orgDraft, useNd30LayoutForEnterprise: e.target.checked })
                    }
                  />
                  Doanh nghiệp bật tương thích NĐ 30 (Quốc hiệu/Tiêu ngữ)
                </label>
              )}
            </div>

            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setShowOrgEditor(false)}
                className="px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 text-xs font-medium cursor-pointer"
              >
                Hủy
              </button>
              <button
                type="submit"
                className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-blue-600 text-white hover:bg-blue-700 text-xs font-semibold cursor-pointer"
              >
                <Save className="w-3.5 h-3.5" />
                Lưu hồ sơ
              </button>
            </div>
          </form>
        )}
      </section>

      {/* KHU VỰC 2: CẤU HÌNH PHẦN ĐẦU VĂN BẢN */}
      <section className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm space-y-3.5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-sm sm:text-base font-bold flex items-center gap-2">
            <Wand2 className="w-4 h-4 text-blue-600" />
            2. Cấu hình phần đầu văn bản
          </h2>

          {/* Tệp Word đang chọn từ hàng đợi */}
          <div className="flex items-center gap-1.5">
            {wordFiles.length > 0 ? (
              <select
                value={selectedFilePath}
                onChange={(e) => {
                  setSelectedFilePath(e.target.value);
                  setCfg((prev) => ({ ...prev, selectedCandidateIndex: null }));
                }}
                className="max-w-[260px] px-2.5 py-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-[11px] font-medium"
              >
                {wordFiles.map((f) => (
                  <option key={f.path} value={f.path}>
                    Tệp: {f.name}
                  </option>
                ))}
              </select>
            ) : (
              <button
                type="button"
                onClick={onPickFiles}
                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-[11px] font-semibold border border-slate-200 dark:border-slate-700 hover:bg-slate-50 dark:hover:bg-slate-800 cursor-pointer"
              >
                <FolderOpen className="w-3.5 h-3.5" />
                Mở tệp Word để nhận diện
              </button>
            )}
          </div>
        </div>

        {/* Loại văn bản & Chế độ lưu */}
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Loại văn bản
            </span>
            <select
              value={cfg.docType}
              onChange={(e) =>
                setCfg({ ...cfg, docType: e.target.value as AdminDocType })
              }
              className="mt-1 w-full px-3 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-xs font-semibold"
            >
              {ADMIN_DOC_TYPE_META.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label} ({m.subjectInLeftCell ? 'V/v ở ô trái dưới Số' : `${m.docTypeHeading} căn giữa`})
                </option>
              ))}
            </select>
          </label>

          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Chế độ ghi tệp (Windows Desktop)
            </span>
            <select
              value={cfg.outputMode}
              onChange={(e) =>
                setCfg({ ...cfg, outputMode: e.target.value as HeaderBuilderConfig['outputMode'] })
              }
              className="mt-1 w-full px-3 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-xs font-medium"
            >
              <option value="newCopy">Tạo bản sao mới (_ChuanDau.docx) — An toàn nhất</option>
              <option value="overwriteWithBackup">Ghi đè tệp gốc (tự sao lưu .bak &amp; .header_init.bak)</option>
            </select>
          </label>
        </div>

        {/* Các thành phần hiển thị */}
        <div className="p-3 rounded-xl bg-slate-50 dark:bg-slate-950/50 border border-slate-200 dark:border-slate-800 space-y-2">
          <div className="text-[11px] font-bold uppercase tracking-wider text-slate-500">
            Thành phần hiển thị
          </div>
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs">
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showGoverningOrg}
                onChange={(e) => setCfg({ ...cfg, showGoverningOrg: e.target.checked })}
              />
              CQ chủ quản
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showIssuingOrg}
                onChange={(e) => setCfg({ ...cfg, showIssuingOrg: e.target.checked })}
              />
              CQ ban hành
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showAffiliatedUnit}
                onChange={(e) => setCfg({ ...cfg, showAffiliatedUnit: e.target.checked })}
              />
              Đơn vị trực thuộc
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showDepartment}
                onChange={(e) => setCfg({ ...cfg, showDepartment: e.target.checked })}
              />
              Phòng / Ban
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showOrgUnderline}
                onChange={(e) => setCfg({ ...cfg, showOrgUnderline: e.target.checked })}
              />
              Đường kẻ cơ quan
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showNationalHeader}
                onChange={(e) => setCfg({ ...cfg, showNationalHeader: e.target.checked })}
              />
              Quốc hiệu / Đảng
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showMottoUnderline}
                onChange={(e) => setCfg({ ...cfg, showMottoUnderline: e.target.checked })}
              />
              Đường kẻ tiêu ngữ
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showDocTypeAndSubject}
                onChange={(e) => setCfg({ ...cfg, showDocTypeAndSubject: e.target.checked })}
              />
              Tên loại &amp; Trích yếu
            </label>
          </div>
        </div>

        {/* Số, ký hiệu & Địa danh, ngày tháng */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
          {/* Số & Ký hiệu */}
          <div className="p-3 rounded-xl border border-slate-200 dark:border-slate-800 space-y-2">
            <div className="flex items-center justify-between">
              <label className="inline-flex items-center gap-1.5 font-bold cursor-pointer">
                <input
                  type="checkbox"
                  checked={cfg.showNumberSymbol}
                  onChange={(e) => setCfg({ ...cfg, showNumberSymbol: e.target.checked })}
                />
                Số và ký hiệu văn bản
              </label>
              <button
                type="button"
                onClick={handleSuggestSymbolFromProfile}
                className="text-[11px] font-semibold text-blue-600 dark:text-blue-400 hover:underline cursor-pointer"
                title="Chỉ gợi ý phần ký hiệu khi bạn nhấn"
              >
                Gợi ý ký hiệu từ Hồ sơ
              </button>
            </div>

            {cfg.showNumberSymbol && (
              <>
                <div className="flex gap-2">
                  <select
                    value={cfg.numberMode}
                    onChange={(e) =>
                      setCfg({ ...cfg, numberMode: e.target.value as HeaderBuilderConfig['numberMode'] })
                    }
                    className="flex-1 px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-xs"
                  >
                    <option value="keepExisting">Giữ nguyên số/ký hiệu của văn bản gốc (hoặc để trống)</option>
                    <option value="manualInput">Nhập số / ký hiệu thủ công</option>
                  </select>
                </div>

                {cfg.numberMode === 'manualInput' && (
                  <div className="grid grid-cols-2 gap-2">
                    <input
                      type="text"
                      value={cfg.docNumber}
                      onChange={(e) => setCfg({ ...cfg, docNumber: e.target.value })}
                      placeholder="Số (trống nếu dự thảo)"
                      className="px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                    />
                    <input
                      type="text"
                      value={cfg.docSymbol}
                      onChange={(e) => setCfg({ ...cfg, docSymbol: e.target.value })}
                      placeholder="Ký hiệu (VD: CV-ABC)"
                      className="px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                    />
                  </div>
                )}

                <div className="flex flex-col gap-1 text-[11px] text-slate-600 dark:text-slate-400">
                  <label className="inline-flex items-center gap-1.5 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={cfg.allowOverrideExistingNumber}
                      onChange={(e) =>
                        setCfg({ ...cfg, allowOverrideExistingNumber: e.target.checked })
                      }
                    />
                    Cho phép ghi đè nếu tài liệu đã có Số/Ký hiệu
                  </label>
                  <label className="inline-flex items-center gap-1.5 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={cfg.allowOverrideExistingOrg}
                      onChange={(e) =>
                        setCfg({ ...cfg, allowOverrideExistingOrg: e.target.checked })
                      }
                    />
                    Cho phép thay Tên cơ quan trên tệp bằng Hồ sơ đang chọn
                  </label>
                </div>
              </>
            )}
          </div>

          {/* Địa danh & Ngày tháng */}
          <div className="p-3 rounded-xl border border-slate-200 dark:border-slate-800 space-y-2">
            <label className="inline-flex items-center gap-1.5 font-bold cursor-pointer">
              <input
                type="checkbox"
                checked={cfg.showLocationDate}
                onChange={(e) => setCfg({ ...cfg, showLocationDate: e.target.checked })}
              />
              Địa danh và ngày tháng năm
            </label>

            {cfg.showLocationDate && (
              <>
                <div className="grid grid-cols-2 gap-2">
                  <input
                    type="text"
                    value={cfg.locationOverride}
                    onChange={(e) => setCfg({ ...cfg, locationOverride: e.target.value })}
                    placeholder={
                      activeOrg?.defaultLocation
                        ? `Mặc định: ${activeOrg.defaultLocation}`
                        : 'Địa danh (VD: Huế, Hà Nội)'
                    }
                    className="px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                  />
                  <select
                    value={cfg.dateMode}
                    onChange={(e) =>
                      setCfg({ ...cfg, dateMode: e.target.value as HeaderDateMode })
                    }
                    className="px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-xs"
                  >
                    <option value="keepExisting">Giữ ngày gốc trên tệp</option>
                    <option value="blankDraft">Dự thảo (ngày ... tháng ... năm ...)</option>
                    <option value="useToday">Ngày hôm nay</option>
                    <option value="manualDate">Nhập thủ công</option>
                  </select>
                </div>

                {cfg.dateMode === 'manualDate' && (
                  <div className="grid grid-cols-3 gap-2">
                    <input
                      type="text"
                      value={cfg.manualDay}
                      onChange={(e) => setCfg({ ...cfg, manualDay: e.target.value })}
                      placeholder="Ngày"
                      className="px-2 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                    />
                    <input
                      type="text"
                      value={cfg.manualMonth}
                      onChange={(e) => setCfg({ ...cfg, manualMonth: e.target.value })}
                      placeholder="Tháng"
                      className="px-2 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                    />
                    <input
                      type="text"
                      value={cfg.manualYear}
                      onChange={(e) => setCfg({ ...cfg, manualYear: e.target.value })}
                      placeholder="Năm"
                      className="px-2 py-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
                    />
                  </div>
                )}

                <label className="inline-flex items-center gap-1.5 text-[11px] text-slate-600 dark:text-slate-400 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={cfg.allowOverrideExistingDate}
                    onChange={(e) =>
                      setCfg({ ...cfg, allowOverrideExistingDate: e.target.checked })
                    }
                  />
                  Cho phép thay ngày ban hành đã có trên văn bản gốc
                </label>
              </>
            )}
          </div>
        </div>

        {/* Tên loại, Trích yếu, Độ mật/khẩn */}
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5 text-xs">
          <label className="block sm:col-span-2">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              {cfg.docType === 'congVan'
                ? 'Trích yếu Công văn (VD: V/v triển khai nhiệm vụ...)'
                : 'Trích yếu nội dung văn bản'}
            </span>
            <input
              type="text"
              value={cfg.subjectText}
              onChange={(e) => setCfg({ ...cfg, subjectText: e.target.value })}
              placeholder={
                activeCand?.fields.subject
                  ? `Đang giữ trên tệp: ${activeCand.fields.subject}`
                  : cfg.docType === 'congVan'
                    ? 'V/v ... (để trống nếu giữ nguyên trên tệp)'
                    : 'Về việc ... (để trống nếu giữ nguyên trên tệp)'
              }
              className="mt-1 w-full px-3 py-1.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
            />
          </label>

          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Dấu độ mật / độ khẩn (tùy chọn)
            </span>
            <input
              type="text"
              value={cfg.classificationMark}
              onChange={(e) => setCfg({ ...cfg, classificationMark: e.target.value })}
              placeholder="VD: KHẨN, MẬT (hoặc trống)"
              className="mt-1 w-full px-3 py-1.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
            />
          </label>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 text-xs">
          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Tùy chỉnh Tên loại VB (nếu khác mặc định)
            </span>
            <input
              type="text"
              value={cfg.customDocTypeLabel}
              onChange={(e) => setCfg({ ...cfg, customDocTypeLabel: e.target.value })}
              placeholder={`Mặc định: ${ADMIN_DOC_TYPE_META.find((m) => m.id === cfg.docType)?.docTypeHeading}`}
              className="mt-1 w-full px-3 py-1.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
            />
          </label>
          <label className="block">
            <span className="text-[11px] font-semibold text-slate-600 dark:text-slate-300">
              Dòng nội dung tóm tắt bổ sung (tùy chọn)
            </span>
            <input
              type="text"
              value={cfg.summaryLine}
              onChange={(e) => setCfg({ ...cfg, summaryLine: e.target.value })}
              placeholder="Ghi chú ngắn dưới trích yếu (nếu loại VB yêu cầu)"
              className="mt-1 w-full px-3 py-1.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-950 text-xs"
            />
          </label>
        </div>
      </section>

      {/* KHU VỰC 3: XEM TRƯỚC KHỔ A4 & KẾT QUẢ NHẬN DIỆN */}
      <section className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-sm sm:text-base font-bold flex items-center gap-2">
            <Scale className="w-4 h-4 text-blue-600" />
            3. Xem trước phần đầu (Khổ giấy A4) &amp; Kiểm tra thể thức
          </h2>
          <button
            type="button"
            onClick={() => setShowRulesPanel((v) => !v)}
            className="inline-flex items-center gap-1 text-[11px] font-semibold text-slate-600 dark:text-slate-300 hover:text-blue-600 cursor-pointer"
          >
            Quy tắc pháp lý ({applicableRules.length})
            {showRulesPanel ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
          </button>
        </div>

        {/* Mô phỏng Khổ giấy A4 (tỷ lệ thực cột 42% / 58%) */}
        <div className="rounded-xl border border-slate-300 dark:border-slate-700 bg-white text-slate-900 p-4 sm:p-6 shadow-inner overflow-x-auto">
          <div className="min-w-[520px] font-serif leading-snug">
            {previewData.classMark && (
              <div className="text-right text-[11px] font-bold tracking-wide uppercase mb-1">
                {previewData.classMark}
              </div>
            )}

            <div className="grid grid-cols-12 gap-3 items-start">
              {/* Cột trái 5/12 (~42%) */}
              <div className="col-span-5 text-center space-y-0.5">
                {previewData.hasLogo && (
                  <div className="inline-flex items-center gap-1 px-2 py-0.5 mb-1 rounded bg-sky-50 border border-sky-200 text-[10px] font-sans text-sky-700">
                    <ImageIcon className="w-3 h-3" />
                    Logo gốc được bảo toàn
                  </div>
                )}
                {previewData.governing && (
                  <div className="text-[11px] uppercase tracking-tight">
                    {previewData.governing}
                  </div>
                )}
                {previewData.issuing && (
                  <div className="text-[12px] font-bold uppercase tracking-tight">
                    {previewData.issuing}
                  </div>
                )}
                {previewData.affiliated && (
                  <div className="text-[11px]">{previewData.affiliated}</div>
                )}
                {previewData.department && (
                  <div className="text-[11px]">{previewData.department}</div>
                )}
                {cfg.showOrgUnderline &&
                  (previewData.issuing || previewData.affiliated || previewData.department) && (
                    <div className="mx-auto w-2/5 border-b border-slate-900 pt-0.5" />
                  )}
                {previewData.numberLine && (
                  <div className="text-[12px] pt-2">{previewData.numberLine}</div>
                )}
                {previewData.isCongVan && previewData.subject && (
                  <div className="text-[11.5px] text-left pt-1.5 pl-1">
                    {previewData.subject}
                  </div>
                )}
              </div>

              {/* Cột phải 7/12 (~58%) */}
              <div className="col-span-7 text-center space-y-0.5">
                {previewData.national && (
                  <div className="text-[11.5px] font-bold uppercase tracking-tight">
                    {previewData.national}
                  </div>
                )}
                {previewData.motto && (
                  <div className="text-[12.5px] font-bold">{previewData.motto}</div>
                )}
                {cfg.showMottoUnderline && (previewData.motto || previewData.national) && (
                  <div className="mx-auto w-1/2 border-b border-slate-900 pt-0.5" />
                )}
                {previewData.dateLine && (
                  <div className="text-[12px] italic pt-2">{previewData.dateLine}</div>
                )}
              </div>
            </div>

            {/* Tên loại & Trích yếu căn giữa cho văn bản có tên loại */}
            {cfg.showDocTypeAndSubject && !previewData.isCongVan && (
              <div className="mt-4 text-center space-y-1">
                {previewData.docHeading && (
                  <div className="text-[13.5px] font-bold uppercase">
                    {previewData.docHeading}
                  </div>
                )}
                {previewData.subject && (
                  <div className="text-[13px] font-bold">{previewData.subject}</div>
                )}
                {previewData.summary && (
                  <div className="text-[12px] italic">{previewData.summary}</div>
                )}
                {(previewData.docHeading || previewData.subject) && (
                  <div className="mx-auto w-1/3 border-b border-slate-900 pt-1" />
                )}
              </div>
            )}

            {previewData.isCongVan && previewData.summary && (
              <div className="mt-2 text-[12px] italic">{previewData.summary}</div>
            )}
          </div>
        </div>

        {/* Kết quả nhận diện tệp đang chọn */}
        {selectedWordFile && inspection && (
          <div
            className={`rounded-xl border p-3 space-y-2.5 text-xs ${
              CLASSIFICATION_STYLES[inspection.classification].box
            }`}
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2 flex-wrap">
                {CLASSIFICATION_STYLES[inspection.classification].icon}
                <span className="font-bold">{inspection.fileName}</span>
                <span
                  className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                    CLASSIFICATION_STYLES[inspection.classification].badge
                  }`}
                >
                  {inspection.classificationLabel}
                </span>
              </div>
              <div className="flex items-center gap-2 text-[11px] text-slate-600 dark:text-slate-300">
                <span>{inspection.sectionCount} section</span>
                {inspection.hasHeaderFooter && <span>· Có Header/Footer</span>}
                {inspection.hasMacros && <span className="text-amber-600 font-semibold">· Có Macro (.docm)</span>}
              </div>
            </div>

            {/* Chọn ứng viên nếu có >= 1 ứng viên (bắt buộc chọn khi > 1) */}
            {inspection.candidates.length > 0 && (
              <div className="space-y-1.5">
                <div className="text-[11px] font-semibold">
                  {inspection.candidates.length > 1
                    ? 'Phát hiện nhiều vùng nghi là phần đầu — hãy chọn đúng vùng cần xử lý:'
                    : 'Vùng phần đầu đã nhận diện:'}
                </div>
                <div className="grid grid-cols-1 gap-1.5">
                  {inspection.candidates.map((cand) => {
                    const selected =
                      cfg.selectedCandidateIndex === cand.candidateIndex ||
                      (inspection.candidates.length === 1 &&
                        cfg.selectedCandidateIndex === null);
                    return (
                      <label
                        key={cand.candidateIndex}
                        className={`flex items-start gap-2 p-2 rounded-lg border cursor-pointer text-[11px] ${
                          selected
                            ? 'border-blue-500 bg-white/90 dark:bg-slate-900/90'
                            : 'border-slate-200 dark:border-slate-700 bg-white/50 dark:bg-slate-950/40'
                        }`}
                      >
                        <input
                          type="radio"
                          name="header-candidate"
                          checked={selected}
                          onChange={() =>
                            setCfg((prev) => ({
                              ...prev,
                              selectedCandidateIndex: cand.candidateIndex,
                            }))
                          }
                        />
                        <div className="flex-1 min-w-0">
                          <div className="font-semibold flex items-center gap-1.5 flex-wrap">
                            <span>Ứng viên #{cand.candidateIndex + 1}</span>
                            <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-200/80 dark:bg-slate-800">
                              {cand.layoutKind}
                            </span>
                            {cand.hasLogoOrImage && (
                              <span className="text-[10px] px-1.5 py-0.5 rounded bg-sky-100 text-sky-700 dark:bg-sky-900/50 dark:text-sky-300">
                                Có Logo/Ảnh
                              </span>
                            )}
                          </div>
                          <div className="text-slate-600 dark:text-slate-400 truncate mt-0.5">
                            {cand.previewSnippet}
                          </div>
                        </div>
                      </label>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Findings chi tiết */}
            {inspection.findings.length > 0 && (
              <ul className="space-y-1.5 pt-1">
                {inspection.findings.map((fd, idx) => (
                  <li
                    key={`${fd.ruleId}-${idx}`}
                    className="p-2 rounded-lg bg-white/80 dark:bg-slate-900/70 border border-slate-200/70 dark:border-slate-800 text-[11px]"
                  >
                    <div className="flex flex-wrap items-center gap-1.5 mb-0.5">
                      <span className="font-mono font-bold text-[10px] px-1.5 py-0.5 rounded bg-slate-200 dark:bg-slate-800">
                        {fd.ruleId}
                      </span>
                      <span
                        className={`text-[10px] font-semibold px-1.5 py-0.5 rounded ${OBLIGATION_LABELS[fd.obligation].cls}`}
                      >
                        {OBLIGATION_LABELS[fd.obligation].text}
                      </span>
                      <span className="text-[10px] text-slate-500">{fd.legalBasis}</span>
                    </div>
                    <div className="text-slate-800 dark:text-slate-200">{fd.message}</div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {/* Bảng tra cứu quy tắc pháp lý */}
        {showRulesPanel && (
          <div className="rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/60 p-3 space-y-2">
            <div className="text-xs font-bold">
              Quy tắc đối chiếu đang áp dụng ({applicableRules.length} mục)
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
              {applicableRules.map((r) => (
                <div
                  key={r.ruleId}
                  className="p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 text-[11px] space-y-1"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-mono font-bold text-blue-600 dark:text-blue-400">
                      {r.ruleId}
                    </span>
                    <span
                      className={`text-[10px] font-semibold px-1.5 py-0.5 rounded ${OBLIGATION_LABELS[r.obligation].cls}`}
                    >
                      {OBLIGATION_LABELS[r.obligation].text}
                    </span>
                  </div>
                  <div className="font-semibold">{r.title}</div>
                  <div className="text-slate-600 dark:text-slate-400 leading-relaxed">
                    {r.description}
                  </div>
                  <div className="text-[10px] text-slate-500">{r.legalBasis}</div>
                </div>
              ))}
            </div>
          </div>
        )}
      </section>

      {/* KHU VỰC 4: THAO TÁC THỰC TẾ */}
      <section className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-4 shadow-sm space-y-3">
        <h2 className="text-sm sm:text-base font-bold">4. Thực thi trên văn bản</h2>

        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            disabled={!selectedWordFile || inspecting || applying}
            onClick={() => void runInspection()}
            className="inline-flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-semibold border border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800 disabled:opacity-45 cursor-pointer disabled:cursor-not-allowed"
          >
            <FileSearch className="w-4 h-4" />
            {inspecting ? 'Đang nhận diện...' : 'Nhận diện phần đầu'}
          </button>

          <button
            type="button"
            disabled={!selectedWordFile || applying || !inspection?.canStandardize}
            onClick={() => void handleApplyToActiveFile('standardizeExisting')}
            className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl text-xs font-bold bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-45 cursor-pointer disabled:cursor-not-allowed"
            title={
              inspection?.requiresUserChoice
                ? 'Vui lòng chọn 1 ứng viên ở mục 3 trước'
                : !inspection?.canStandardize
                  ? 'Tệp chưa có phần đầu để chuẩn hóa — dùng Tạo/Chèn phần đầu mới'
                  : 'Chuẩn hóa phần đầu hiện có và giữ nguyên thân văn bản'
            }
          >
            <Sparkles className="w-4 h-4" />
            Chuẩn hóa phần đầu hiện có
          </button>

          {selectedWordFile && inspection?.canCreateNew && (
            <button
              type="button"
              disabled={applying}
              onClick={() => void handleApplyToActiveFile('createNew')}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl text-xs font-bold bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-45 cursor-pointer disabled:cursor-not-allowed"
            >
              <FilePlus2 className="w-4 h-4" />
              Chèn phần đầu mới vào tệp
            </button>
          )}

          <button
            type="button"
            disabled={applying}
            onClick={() => void handleCreateStandaloneDocx()}
            className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-xl text-xs font-semibold border border-emerald-300 dark:border-emerald-800 text-emerald-700 dark:text-emerald-300 bg-emerald-50/70 dark:bg-emerald-950/30 hover:bg-emerald-100 dark:hover:bg-emerald-950/50 disabled:opacity-45 cursor-pointer"
          >
            <FilePlus2 className="w-4 h-4" />
            Tạo tệp .docx mới từ đầu
          </button>

          {hasInitialRestore && (
            <button
              type="button"
              disabled={applying}
              onClick={() => void handleRestoreInitial()}
              className="inline-flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-semibold border border-amber-300 dark:border-amber-800 text-amber-700 dark:text-amber-300 hover:bg-amber-50 dark:hover:bg-amber-950/30 cursor-pointer"
            >
              <RotateCcw className="w-4 h-4" />
              Khôi phục nội dung ban đầu
            </button>
          )}
        </div>

        {/* Kết quả xử lý gần nhất */}
        {lastResult && (
          <div
            className={`p-3 rounded-xl border text-xs space-y-2 ${
              lastResult.success
                ? 'border-emerald-200 bg-emerald-50/60 dark:border-emerald-900/60 dark:bg-emerald-950/20'
                : 'border-rose-200 bg-rose-50/60 dark:border-rose-900/60 dark:bg-rose-950/20'
            }`}
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="font-bold">
                {lastResult.success
                  ? `Hoàn tất: ${lastResult.outputName || lastResult.outputPath}`
                  : `Không thực hiện: ${lastResult.error}`}
              </div>

              {lastResult.success && (
                <div className="flex items-center gap-2">
                  {isNative ? (
                    <button
                      type="button"
                      onClick={() =>
                        void bridge.invoke('open_path', {
                          path: lastResult.outputPath,
                          reveal: false,
                        })
                      }
                      className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-emerald-600 text-white hover:bg-emerald-700 text-[11px] font-semibold cursor-pointer"
                    >
                      <ExternalLink className="w-3.5 h-3.5" />
                      Mở trong Word
                    </button>
                  ) : (
                    lastResult.blobUrl &&
                    lastResult.outputName && (
                      <>
                        <button
                          type="button"
                          onClick={() =>
                            triggerBrowserDownload(lastResult.blobUrl!, lastResult.outputName!)
                          }
                          className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-emerald-600 text-white hover:bg-emerald-700 text-[11px] font-semibold cursor-pointer"
                        >
                          <Download className="w-3.5 h-3.5" />
                          Tải xuống
                        </button>
                        {lastResult.outputBlob && (
                          <button
                            type="button"
                            onClick={() =>
                              void shareOutputBlob(
                                lastResult.outputBlob!,
                                lastResult.outputName!,
                                lastResult.blobUrl,
                              )
                            }
                            className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg border border-emerald-400 text-emerald-700 dark:text-emerald-300 text-[11px] font-semibold cursor-pointer"
                          >
                            <Share2 className="w-3.5 h-3.5" />
                            Chia sẻ
                          </button>
                        )}
                      </>
                    )
                  )}
                </div>
              )}
            </div>

            {lastResult.logs.length > 0 && (
              <ul className="list-disc list-inside text-[11px] text-slate-600 dark:text-slate-300 space-y-0.5">
                {lastResult.logs.map((lg, i) => (
                  <li key={i}>{lg}</li>
                ))}
              </ul>
            )}

            {lastResult.warnings.length > 0 && (
              <ul className="list-disc list-inside text-[11px] text-amber-700 dark:text-amber-300 space-y-0.5">
                {lastResult.warnings.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            )}
          </div>
        )}
      </section>
    </div>
  );
};
