import React, { useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  AlertTriangle,
  Check,
  CheckCircle2,
  Columns3,
  Download,
  ExternalLink,
  FileSearch,
  FolderOpen,
  Image as ImageIcon,
  Info,
  Layers,
  LayoutGrid,
  Play,
  RotateCcw,
  ScanSearch,
  Search,
  Share2,
  ShieldCheck,
  Sparkles,
  Table as TableIcon,
  Type,
  X,
} from 'lucide-react';
import { bridge } from '../bridge';
import type {
  DocumentMapNode,
  ElementLocator,
  FileQueueItem,
  HeaderFooterMapEntry,
  LayoutExecutionRequest,
  LayoutExecutionResult,
  LayoutIntelligenceReport,
  LayoutProposal,
  OutputMode,
  PageLayoutEntry,
  ParagraphMapEntry,
  RecognizedStructureKind,
  SectionMapEntry,
  StdDocScope,
  StdSeverity,
  StyleMapEntry,
  TableMapEntry,
  VisualObjectMapEntry,
} from '../types';
import { shareOutputBlob, triggerBrowserDownload } from '../utils/webDocxEngine';
import {
  analyzeWebLayoutIntelligence,
  executeWebLayoutProposals,
  getWebLayoutSnapshot,
  registerWebLayoutSnapshot,
} from '../utils/webLayoutIntelligence';

export interface LayoutIntelligenceWorkspaceProps {
  files: FileQueueItem[];
  onPickFiles: () => void;
  onReplaceWebFile?: (path: string, file: File) => void;
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

type MapTab =
  | 'document'
  | 'paragraph'
  | 'table'
  | 'section'
  | 'style'
  | 'headerFooter'
  | 'pageLayout';

type InspectedPayload =
  | { source: 'node'; title: string; subtitle: string; locator?: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string }
  | { source: 'paragraph'; title: string; subtitle: string; locator: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string }
  | { source: 'table'; title: string; subtitle: string; locator: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string }
  | { source: 'section'; title: string; subtitle: string; locator?: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string }
  | { source: 'style'; title: string; subtitle: string; locator?: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string }
  | { source: 'hf'; title: string; subtitle: string; locator: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string }
  | { source: 'page'; title: string; subtitle: string; locator?: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string }
  | { source: 'visual'; title: string; subtitle: string; locator: ElementLocator; meta: Record<string, string | number | boolean | null>; targetId: string };

const SCOPE_OPTIONS: { id: StdDocScope; label: string }[] = [
  { id: 'autoDetect', label: 'Tự động nhận diện phạm vi' },
  { id: 'adminNd30', label: 'Hành chính (NĐ 30/2020/NĐ-CP)' },
  { id: 'enterpriseInternal', label: 'Doanh nghiệp / Nội bộ' },
  { id: 'partyDocument', label: 'Văn bản Đảng (36-HD)' },
  { id: 'academicThesis', label: 'Luận văn / Đề án' },
  { id: 'freeform', label: 'Tài liệu tự do' },
];

const KIND_LABELS: Record<RecognizedStructureKind, string> = {
  documentHeader: 'Phần đầu VB',
  docTypeTitle: 'Tên loại VB',
  docSubject: 'Trích yếu',
  mainTitle: 'Tiêu đề chính',
  mainBody: 'Nội dung chính',
  headingLevel1: 'Tiêu đề cấp 1',
  headingLevel2: 'Tiêu đề cấp 2',
  headingLevel3: 'Tiêu đề cấp 3',
  headingLevel4: 'Tiêu đề cấp 4',
  dataTable: 'Bảng biểu',
  imageAsset: 'Hình ảnh',
  signatureBlock: 'Chữ ký',
  recipientBlock: 'Nơi nhận',
  appendixBlock: 'Phụ lục',
  headerFooter: 'Header/Footer',
  textBox: 'Text Box',
};

const SEV_BADGE: Record<StdSeverity, string> = {
  critical: 'bg-rose-100 text-rose-800 dark:bg-rose-950/70 dark:text-rose-300',
  error: 'bg-orange-100 text-orange-800 dark:bg-orange-950/70 dark:text-orange-300',
  warning: 'bg-amber-100 text-amber-800 dark:bg-amber-950/70 dark:text-amber-300',
  information: 'bg-sky-100 text-sky-800 dark:bg-sky-950/70 dark:text-sky-300',
};

export const LayoutIntelligenceWorkspace: React.FC<LayoutIntelligenceWorkspaceProps> = ({
  files,
  onPickFiles,
  onReplaceWebFile,
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();
  const [selectedPath, setSelectedPath] = useState<string>('');
  const [scope, setScope] = useState<StdDocScope>('autoDetect');
  const [outputMode, setOutputMode] = useState<OutputMode>('newCopy');
  const [applyAutoHighConf, setApplyAutoHighConf] = useState<boolean>(true);
  const [busy, setBusy] = useState<boolean>(false);

  const [report, setReport] = useState<LayoutIntelligenceReport | null>(null);
  const [execResult, setExecResult] = useState<LayoutExecutionResult | null>(null);

  // Proposal individual disposition: 'accepted' | 'rejected' | 'pending'
  const [decisions, setDecisions] = useState<Record<string, 'accepted' | 'rejected' | 'pending'>>({});

  // Inspector & Map Tabs
  const [activeMapTab, setActiveMapTab] = useState<MapTab>('document');
  const [mapFilter, setMapFilter] = useState<string>('');
  const [inspected, setInspected] = useState<InspectedPayload | null>(null);
  const [snapshotMap, setSnapshotMap] = useState<Record<string, File>>({});

  const wordFiles = useMemo(
    () => files.filter((f) => ['docx', 'docm', 'doc'].includes(f.ext.toLowerCase())),
    [files],
  );

  useEffect(() => {
    if (!selectedPath && wordFiles.length > 0) {
      setSelectedPath(wordFiles[0].path);
    } else if (selectedPath && !wordFiles.some((f) => f.path === selectedPath)) {
      setSelectedPath(wordFiles[0]?.path ?? '');
    }
  }, [wordFiles, selectedPath]);

  const currentFileItem = useMemo(
    () => wordFiles.find((f) => f.path === selectedPath) ?? null,
    [wordFiles, selectedPath],
  );

  const initProposalDecisions = (rep: LayoutIntelligenceReport) => {
    const next: Record<string, 'accepted' | 'rejected' | 'pending'> = {};
    for (const p of rep.proposals) {
      next[p.proposalId] = p.tier === 'highConfidenceAuto' ? 'accepted' : 'pending';
    }
    setDecisions(next);
  };

  const handleAnalyze = async () => {
    if (!currentFileItem) {
      onStatusToast('info', 'Hãy chọn hoặc mở ít nhất 1 tệp Word (.docx, .docm).');
      return;
    }
    setBusy(true);
    setExecResult(null);
    try {
      let rep: LayoutIntelligenceReport;
      if (isNative) {
        rep = await bridge.invoke<LayoutIntelligenceReport>('analyze_layout_intelligence', {
          path: currentFileItem.path,
          scope,
        });
      } else {
        if (!currentFileItem.webFile) {
          throw new Error('Không tìm thấy dữ liệu tệp trên trình duyệt.');
        }
        registerWebLayoutSnapshot(currentFileItem.path, currentFileItem.webFile);
        setSnapshotMap((prev) => ({
          ...prev,
          [currentFileItem.path]: prev[currentFileItem.path] ?? currentFileItem.webFile!,
        }));
        rep = await analyzeWebLayoutIntelligence(currentFileItem.webFile, scope);
      }
      setReport(rep);
      initProposalDecisions(rep);
      if (rep.maps.documentMap.length > 0) {
        selectDocumentNode(rep.maps.documentMap[0]);
      } else {
        setInspected(null);
      }
      onStatusToast(
        'success',
        `Đã phân tích ${rep.summary.sectionCount} section, ${rep.summary.estimatedPageCount} trang ước tính, ${rep.summary.warningCount} cảnh báo.`,
      );
    } catch (err) {
      onStatusToast('error', `Lỗi phân tích bố cục: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleExecuteProposals = async () => {
    if (!currentFileItem || !report) return;
    const acceptedProposalIds = report.proposals
      .filter((p) => decisions[p.proposalId] === 'accepted')
      .map((p) => p.proposalId);
    const rejectedProposalIds = report.proposals
      .filter((p) => decisions[p.proposalId] === 'rejected')
      .map((p) => p.proposalId);

    const req: LayoutExecutionRequest = {
      path: currentFileItem.path,
      scope,
      outputMode,
      applyHighConfidenceAuto: applyAutoHighConf,
      acceptedProposalIds,
      rejectedProposalIds,
    };

    setBusy(true);
    try {
      let res: LayoutExecutionResult;
      if (isNative) {
        res = await bridge.invoke<LayoutExecutionResult>('execute_layout_proposals', req);
      } else {
        const srcFile = currentFileItem.webFile;
        if (!srcFile) throw new Error('Không tìm thấy tệp trình duyệt.');
        res = await executeWebLayoutProposals(srcFile, req);
        if (res.success && res.outputBlob && outputMode === 'overwriteWithBackup' && onReplaceWebFile) {
          const updated = new File([res.outputBlob], currentFileItem.name, { type: srcFile.type });
          onReplaceWebFile(currentFileItem.path, updated);
        }
      }
      setExecResult(res);
      if (res.success && res.postSaveReport) {
        setReport(res.postSaveReport);
        initProposalDecisions(res.postSaveReport);
        onStatusToast(
          'success',
          `Đã áp dụng ${res.appliedProposalIds.length} đề xuất · Bảo toàn nội dung 100%.`,
        );
      } else if (!res.success) {
        onStatusToast('error', res.error ?? 'Thực thi đề xuất bố cục thất bại.');
      }
    } catch (err) {
      onStatusToast('error', `Lỗi thực thi Smart Formatting: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleRestoreBackup = async () => {
    if (!currentFileItem || !execResult?.backupPath) return;
    setBusy(true);
    try {
      if (isNative) {
        const reply = await bridge.invoke<{ message: string }>('restore_layout_backup', {
          originalPath: currentFileItem.path,
          backupPath: execResult.backupPath,
        });
        onStatusToast('success', reply.message);
        await handleAnalyze();
      } else {
        const snap = snapshotMap[currentFileItem.path] ?? getWebLayoutSnapshot(currentFileItem.path);
        if (snap && onReplaceWebFile) {
          onReplaceWebFile(currentFileItem.path, snap);
          const rep = await analyzeWebLayoutIntelligence(snap, scope);
          setReport(rep);
          initProposalDecisions(rep);
          setExecResult(null);
          onStatusToast('success', 'Đã khôi phục nguyên trạng bản gốc trước khi điều chỉnh.');
        }
      }
    } catch (err) {
      onStatusToast('error', `Lỗi khôi phục backup: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  // Select helpers for Component Inspector
  const selectDocumentNode = (n: DocumentMapNode) => {
    setInspected({
      source: 'node',
      targetId: n.nodeId,
      title: `${n.label} (${KIND_LABELS[n.kind]})`,
      subtitle: n.excerpt || n.locator.elementPath,
      locator: n.locator,
      meta: {
        'Mã định danh': n.nodeId,
        'Loại cấu trúc': KIND_LABELS[n.kind],
        'Độ tin cậy': `${Math.round(n.confidence * 100)}%`,
        'Cấp tiêu đề': n.level > 0 ? `Cấp ${n.level}` : 'Khối cấu trúc',
        'Tín hiệu OpenXML/Ngữ cảnh': n.signals.join(' · '),
        'Số node con': n.childrenIds.length,
      },
    });
  };

  const selectParagraph = (p: ParagraphMapEntry) => {
    setInspected({
      source: 'paragraph',
      targetId: p.paraId,
      title: `Đoạn #${p.globalIndex + 1} · ${KIND_LABELS[p.kind]}`,
      subtitle: p.excerpt || '(Đoạn trống)',
      locator: p.locator,
      meta: {
        'Mã đoạn': p.paraId,
        Style: p.styleId ?? '(Direct / Normal)',
        'Phông / Cỡ chữ': `${p.fontName || 'Mặc định'} · ${p.fontSizePt ? `${p.fontSizePt} pt` : 'kế thừa'}`,
        'Căn chỉnh': p.alignment,
        'Thụt lề (Tr/Ph/Đầu dòng mm)': `${p.indentLeftMm} / ${p.indentRightMm} / ${p.indentFirstLineMm}`,
        'Khoảng cách Trước/Sau (pt)': `${p.spaceBeforePt} / ${p.spaceAfterPt} (dãn ${p.lineSpacingMult}×)`,
        'Số dòng ước tính': p.estimatedLines,
        'WidowControl / KeepNext / KeepLines': `${p.widowControl ? 'Bật' : 'Tắt'} / ${p.keepNext ? 'Có' : 'Không'} / ${p.keepLines ? 'Có' : 'Không'}`,
        'Trong Bảng / Text Box': p.inTableId ?? p.inTextBoxId ?? 'Luồng thân bài',
      },
    });
  };

  const selectTable = (t: TableMapEntry) => {
    setInspected({
      source: 'table',
      targetId: t.tableId,
      title: `Bảng ${t.tableIndex + 1} (${t.rowCount} hàng × ${t.colCount} cột)`,
      subtitle: `Vai trò: ${t.tableRole} · Tổng rộng ${t.totalWidthMm} mm`,
      locator: t.locator,
      meta: {
        'Mã bảng': t.tableId,
        'Vai trò nhận diện': t.tableRole,
        'Rộng tổng / Khổ in (mm)': `${t.totalWidthMm} / ${t.printableWidthMm} mm`,
        'Tràn lề ngang': t.overflowsWidth ? `CÓ (+${t.overflowMm} mm)` : 'Không',
        'Độ rộng từng cột (mm)': t.colWidthsMm.join(' | '),
        'Căn lề / Thụt trái': `${t.alignment} · ${t.indentLeftMm} mm`,
        'Lặp header (tblHeader)': t.hasRepeatHeader ? 'Có' : 'Không',
        'Ô gộp / Hàng khóa dài': `${t.hasMergedCells ? 'Có gộp ô' : 'Không gộp'} · ${t.lockedLongRowCount} hàng khóa`,
        'Hướng giấy khuyến nghị': t.recommendedOrientation === 'landscape' ? 'Landscape (Ngang)' : 'Portrait (Dọc)',
      },
    });
  };

  const selectSection = (s: SectionMapEntry) => {
    setInspected({
      source: 'section',
      targetId: s.sectionId,
      title: `Section ${s.sectionIndex} · ${s.orientation === 'landscape' ? 'Khổ Ngang' : 'Khổ Dọc'}`,
      subtitle: `Trang ${s.pageStart}–${s.pageEnd} · ${s.columnCount} cột`,
      meta: {
        'Mã Section': s.sectionId,
        'Kiểu ngắt (break)': s.breakType,
        'Khổ giấy (Rộng×Cao mm)': `${s.paperWidthMm} × ${s.paperHeightMm} mm`,
        'Lề T/D/Tr/Ph (mm)': `${s.marginTopMm} / ${s.marginBottomMm} / ${s.marginLeftMm} / ${s.marginRightMm}`,
        'Khổ vùng in (mm)': `${s.printableWidthMm} × ${s.printableHeightMm} mm`,
        'Số cột / Khe cột': `${s.columnCount} cột · khe ${s.columnSpaceMm} mm`,
        'Độ rộng từng cột (mm)': s.colWidthsMm.join(' | '),
        'Header / Footer liên kết': `${s.headerIds.length} header · ${s.footerIds.length} footer`,
      },
    });
  };

  const selectStyle = (st: StyleMapEntry) => {
    setInspected({
      source: 'style',
      targetId: st.styleId,
      title: `Style "${st.styleName}" (${st.styleId})`,
      subtitle: `Loại: ${st.styleType} · Kế thừa: ${st.basedOn ?? '(gốc)'}`,
      meta: {
        'Style ID': st.styleId,
        'Tên hiển thị': st.styleName,
        'Mặc định': st.isDefault ? 'Có' : 'Không',
        'Phông / Cỡ chữ': `${st.fontName ?? '(kế thừa)'} · ${st.fontSizePt ? `${st.fontSizePt}pt` : '(kế thừa)'}`,
        'Căn chỉnh / Outline': `${st.alignment ?? 'trái'} · level ${st.outlineLevel ?? '-'}`,
        'Số đoạn sử dụng': st.usageCount,
        'Xung đột Direct Formatting': st.directConflictCount,
      },
    });
  };

  const selectHeaderFooter = (hf: HeaderFooterMapEntry) => {
    setInspected({
      source: 'hf',
      targetId: hf.hfId,
      title: `${hf.hfKind.toUpperCase()} · ${hf.partName}`,
      subtitle: hf.textPreview || '(Không có chữ tĩnh)',
      locator: hf.locator,
      meta: {
        'Mã HF': hf.hfId,
        'Phần OpenXML': hf.partName,
        'Section liên kết': hf.linkedSections.join(', ') || '(Chưa gắn)',
        'Kiểu gắn': hf.bindTypes.join(', '),
        'Số đoạn / Bảng': `${hf.paragraphCount} đoạn · ${hf.tableCount} bảng`,
        'Trường PAGE / NUMPAGES': `${hf.hasPageField ? 'Có PAGE' : 'Không'} / ${hf.hasNumpagesField ? 'Có NUMPAGES' : 'Không'}`,
        'Có hình/logo': hf.hasImages ? 'Có' : 'Không',
      },
    });
  };

  const selectPage = (pg: PageLayoutEntry) => {
    setInspected({
      source: 'page',
      targetId: pg.pageId,
      title: `Trang ước tính #${pg.pageNumber} (Section ${pg.sectionIndex})`,
      subtitle: `${pg.orientation === 'landscape' ? 'Ngang' : 'Dọc'} · Tỷ lệ lấp đầy ${pg.fillRatioPct}%`,
      meta: {
        'Trang số': pg.pageNumber,
        Section: `Section ${pg.sectionIndex}`,
        'Cấu trúc cột': `${pg.columnCount} cột`,
        'Chiều cao đã dùng / Khổ in': `${pg.usedHeightMm} / ${pg.printableHeightMm} mm (${pg.fillRatioPct}%)`,
        'Số thành phần trên trang': pg.elementIds.length,
        'Tràn lề / Tiêu đề mồ côi / Trống bất thường': `${pg.hasOverflow ? 'CÓ tràn' : 'Không'} / ${pg.hasOrphanOrStrandedHeading ? 'CÓ rủi ro' : 'Ổn'} / ${pg.hasAbnormalWhitespace ? 'CÓ khoảng trắng lạ' : 'Ổn'}`,
      },
    });
  };

  const selectVisual = (vis: VisualObjectMapEntry) => {
    setInspected({
      source: 'visual',
      targetId: vis.objectId,
      title: `${vis.objectKind === 'textBox' ? 'Text Box' : 'Hình ảnh'} (${vis.objectId})`,
      subtitle: vis.textExcerpt || vis.mediaTarget || vis.anchorMode,
      locator: vis.locator,
      meta: {
        'Mã đối tượng': vis.objectId,
        'Chế độ neo': vis.anchorMode,
        'Kích thước (Rộng×Cao mm)': `${vis.widthMm} × ${vis.heightMm} mm`,
        'Tràn vùng in': vis.overflowsPrintableArea ? 'CÓ' : 'Không',
        'Quan hệ rId / Media': vis.relationshipId ? `${vis.relationshipId} -> ${vis.mediaTarget ?? '?'}` : '(Embedded Shape)',
        'Số đoạn bên trong Text Box': vis.innerParagraphCount,
      },
    });
  };

  const jumpToTargetId = (targetId: string) => {
    if (!report) return;
    const p = report.maps.paragraphMap.find((x) => x.paraId === targetId);
    if (p) {
      setActiveMapTab('paragraph');
      selectParagraph(p);
      return;
    }
    const t = report.maps.tableMap.find((x) => x.tableId === targetId);
    if (t) {
      setActiveMapTab('table');
      selectTable(t);
      return;
    }
    const s = report.maps.sectionMap.find((x) => x.sectionId === targetId);
    if (s) {
      setActiveMapTab('section');
      selectSection(s);
      return;
    }
    const v = report.maps.visualObjects.find((x) => x.objectId === targetId);
    if (v) {
      selectVisual(v);
      return;
    }
    const n = report.maps.documentMap.find((x) => x.nodeId === targetId);
    if (n) {
      setActiveMapTab('document');
      selectDocumentNode(n);
    }
  };

  // Hierarchical tree rendering for Document Map
  const rootDocNodes = useMemo(
    () => (report ? report.maps.documentMap.filter((n) => !n.parentId) : []),
    [report],
  );
  const nodeById = useMemo(() => {
    const m = new Map<string, DocumentMapNode>();
    for (const n of report?.maps.documentMap ?? []) m.set(n.nodeId, n);
    return m;
  }, [report]);

  const renderTreeNodes = (ids: string[], depth = 0): React.ReactNode =>
    ids.map((id) => {
      const node = nodeById.get(id);
      if (!node) return null;
      const isSelected = inspected?.targetId === node.nodeId;
      return (
        <div key={node.nodeId} className="space-y-1">
          <button
            type="button"
            onClick={() => selectDocumentNode(node)}
            style={{ marginLeft: `${Math.min(depth, 4) * 14}px` }}
            className={`w-full text-left px-2.5 py-1.5 rounded-lg border text-xs transition-colors flex items-center justify-between gap-2 ${
              isSelected
                ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50 text-blue-900 dark:text-blue-200'
                : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 hover:bg-slate-50 dark:hover:bg-slate-800/70'
            }`}
          >
            <div className="min-w-0">
              <div className="flex items-center gap-1.5 flex-wrap">
                <span className="px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px] font-semibold">
                  {KIND_LABELS[node.kind]}
                </span>
                <span className="font-semibold truncate">{node.excerpt || node.label}</span>
              </div>
              <div className="text-[10px] text-slate-500 truncate mt-0.5">
                Sec {node.locator.sectionIndex} · Tr.{node.locator.estimatedPageStart} · Độ tin cậy {Math.round(node.confidence * 100)}%
              </div>
            </div>
            <span className="text-[10px] font-mono text-slate-400 shrink-0">{node.nodeId}</span>
          </button>
          {node.childrenIds.length > 0 && renderTreeNodes(node.childrenIds, depth + 1)}
        </div>
      );
    });

  const filterLower = mapFilter.trim().toLowerCase();

  return (
    <div className="space-y-4">
      {/* Control Header */}
      <div className="mw-panel p-4 sm:p-5 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-base font-bold flex items-center gap-2">
              <ScanSearch className="w-5 h-5 text-blue-600" />
              Layout Intelligence · Phân tích Bố cục Thông minh
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400 mt-1">
              Nhận diện ngữ cảnh OpenXML, dựng 7 bản đồ cấu trúc tài liệu, phát hiện tràn trang/lệch/mồ côi và đề xuất điều chỉnh theo từng mục.
            </p>
          </div>
          <button type="button" onClick={onPickFiles} className="mw-button mw-button-secondary">
            <FolderOpen className="w-4 h-4" /> Mở tệp Word
          </button>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3 items-end">
          <div>
            <label className="block text-xs font-semibold mb-1">Tài liệu phân tích</label>
            <select
              value={selectedPath}
              onChange={(e) => setSelectedPath(e.target.value)}
              className="w-full rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-3 py-2 text-xs"
            >
              {wordFiles.length === 0 ? (
                <option value="">-- Chưa có tệp Word trong hàng đợi --</option>
              ) : (
                wordFiles.map((f) => (
                  <option key={f.path} value={f.path}>
                    {f.name} (.{f.ext})
                  </option>
                ))
              )}
            </select>
          </div>

          <div>
            <label className="block text-xs font-semibold mb-1">Phạm vi tài liệu</label>
            <select
              value={scope}
              onChange={(e) => setScope(e.target.value as StdDocScope)}
              className="w-full rounded-xl border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 px-3 py-2 text-xs"
            >
              {SCOPE_OPTIONS.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </div>

          <div>
            <button
              type="button"
              disabled={busy || !currentFileItem}
              onClick={() => void handleAnalyze()}
              className="mw-button mw-button-primary w-full justify-center"
            >
              <FileSearch className="w-4 h-4" />
              {busy ? 'Đang phân tích...' : 'Phân tích Cấu trúc & Bố cục'}
            </button>
          </div>
        </div>
      </div>

      {report && (
        <>
          {/* Metrics Strip */}
          <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2">
            {[
              { label: 'Section', value: report.summary.sectionCount },
              { label: 'Trang ước tính', value: report.summary.estimatedPageCount },
              { label: 'Tiêu đề', value: report.summary.headingCount },
              { label: 'Đoạn văn', value: report.summary.paragraphCount },
              { label: 'Bảng biểu', value: report.summary.tableCount },
              { label: 'Hình ảnh', value: report.summary.imageCount },
              { label: 'Text Box', value: report.summary.textBoxCount },
              { label: 'Cảnh báo', value: report.summary.warningCount },
            ].map((m) => (
              <div key={m.label} className="mw-panel p-2.5 text-center">
                <div className="text-lg font-extrabold text-slate-900 dark:text-slate-100">{m.value}</div>
                <div className="text-[11px] text-slate-500">{m.label}</div>
              </div>
            ))}
          </div>

          {/* Interactive Page Strip */}
          <div className="mw-panel p-3 sm:p-4 space-y-2.5">
            <div className="flex items-center justify-between gap-2">
              <div className="text-xs font-bold flex items-center gap-1.5">
                <LayoutGrid className="w-4 h-4 text-blue-600" />
                Page Layout Map ({report.maps.pageLayoutMap.length} trang)
              </div>
              <span className="text-[11px] text-slate-500">Bấm vào trang để kiểm tra mật độ & cảnh báo</span>
            </div>
            <div className="flex gap-2 overflow-x-auto pb-1">
              {report.maps.pageLayoutMap.map((pg) => (
                <button
                  key={pg.pageId}
                  type="button"
                  onClick={() => selectPage(pg)}
                  className={`shrink-0 rounded-xl border p-2 text-left transition-all ${
                    pg.orientation === 'landscape' ? 'w-36' : 'w-28'
                  } ${
                    inspected?.targetId === pg.pageId
                      ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/40'
                      : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 hover:border-blue-300'
                  }`}
                >
                  <div className="flex items-center justify-between text-[11px] font-bold">
                    <span>Tr. {pg.pageNumber}</span>
                    <span className="text-[10px] font-normal text-slate-500">
                      {pg.orientation === 'landscape' ? 'Ngang' : 'Dọc'}
                    </span>
                  </div>
                  <div className="text-[10px] text-slate-500 mt-0.5">
                    Sec {pg.sectionIndex} · {pg.columnCount} cột
                  </div>
                  <div className="w-full h-1.5 rounded-full bg-slate-200 dark:bg-slate-800 mt-1.5 overflow-hidden">
                    <div
                      className={`h-full rounded-full ${pg.hasOverflow ? 'bg-rose-500' : 'bg-blue-500'}`}
                      style={{ width: `${Math.min(100, pg.fillRatioPct)}%` }}
                    />
                  </div>
                  <div className="flex items-center justify-between text-[10px] mt-1">
                    <span>{pg.fillRatioPct}%</span>
                    {(pg.hasOverflow || pg.hasOrphanOrStrandedHeading || pg.hasAbnormalWhitespace) && (
                      <AlertTriangle className="w-3 h-3 text-amber-500" />
                    )}
                  </div>
                </button>
              ))}
            </div>
          </div>

          {/* Split Area: Left = 7 Intelligent Maps & Tree; Right = Component Inspector */}
          <div className="grid grid-cols-1 xl:grid-cols-12 gap-4">
            <div className="xl:col-span-7 mw-panel p-3 sm:p-4 space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap gap-1">
                  {(
                    [
                      ['document', `Cấu trúc (${report.maps.documentMap.length})`],
                      ['paragraph', `Đoạn (${report.maps.paragraphMap.length})`],
                      ['table', `Bảng (${report.maps.tableMap.length})`],
                      ['section', `Section (${report.maps.sectionMap.length})`],
                      ['style', `Style (${report.maps.styleMap.length})`],
                      ['headerFooter', `H/F (${report.maps.headerFooterMap.length})`],
                      ['pageLayout', `Trang (${report.maps.pageLayoutMap.length})`],
                    ] as [MapTab, string][]
                  ).map(([id, label]) => (
                    <button
                      key={id}
                      type="button"
                      onClick={() => setActiveMapTab(id)}
                      className={`px-2.5 py-1 rounded-lg text-xs font-semibold transition-colors ${
                        activeMapTab === id
                          ? 'bg-blue-600 text-white'
                          : 'bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200'
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                <div className="relative flex-1 min-w-[160px] max-w-xs">
                  <Search className="w-3.5 h-3.5 text-slate-400 absolute left-2.5 top-2.5" />
                  <input
                    type="text"
                    value={mapFilter}
                    onChange={(e) => setMapFilter(e.target.value)}
                    placeholder="Lọc thành phần..."
                    className="w-full pl-8 pr-2.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs"
                  />
                </div>
              </div>

              <div className="max-h-[380px] overflow-y-auto space-y-1.5 pr-1">
                {activeMapTab === 'document' && (
                  rootDocNodes.length > 0 ? (
                    <div className="space-y-1.5">
                      {filterLower
                        ? renderTreeNodes(
                            report.maps.documentMap
                              .filter(
                                (n) =>
                                  n.excerpt.toLowerCase().includes(filterLower) ||
                                  n.label.toLowerCase().includes(filterLower),
                              )
                              .map((n) => n.nodeId),
                          )
                        : renderTreeNodes(rootDocNodes.map((n) => n.nodeId))}
                    </div>
                  ) : (
                    <p className="text-xs text-slate-500">Không có nút cấu trúc nào.</p>
                  )
                )}

                {activeMapTab === 'paragraph' &&
                  report.maps.paragraphMap
                    .filter(
                      (p) =>
                        !filterLower ||
                        p.excerpt.toLowerCase().includes(filterLower) ||
                        p.kind.toLowerCase().includes(filterLower),
                    )
                    .slice(0, 200)
                    .map((p) => (
                      <button
                        key={p.paraId}
                        type="button"
                        onClick={() => selectParagraph(p)}
                        className={`w-full text-left px-2.5 py-1.5 rounded-lg border text-xs flex items-center justify-between gap-2 ${
                          inspected?.targetId === p.paraId
                            ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50'
                            : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900'
                        }`}
                      >
                        <div className="min-w-0">
                          <div className="flex items-center gap-1.5">
                            <Type className="w-3.5 h-3.5 text-slate-400 shrink-0" />
                            <span className="font-semibold text-[11px] text-blue-600">{KIND_LABELS[p.kind]}</span>
                            <span className="truncate">{p.excerpt || '(Đoạn trống)'}</span>
                          </div>
                          <div className="text-[10px] text-slate-500 mt-0.5">
                            {p.fontName} {p.fontSizePt ? `${p.fontSizePt}pt` : ''} · {p.alignment} · Tr.{p.locator.estimatedPageStart}
                            {p.inTextBoxId ? ` · ${p.inTextBoxId}` : ''}
                            {p.inTableId ? ` · ${p.inTableId}` : ''}
                          </div>
                        </div>
                        <span className="text-[10px] font-mono text-slate-400 shrink-0">{p.paraId}</span>
                      </button>
                    ))}

                {activeMapTab === 'table' && (
                  <>
                    {report.maps.tableMap.map((t) => (
                      <button
                        key={t.tableId}
                        type="button"
                        onClick={() => selectTable(t)}
                        className={`w-full text-left px-2.5 py-2 rounded-lg border text-xs flex items-center justify-between gap-2 ${
                          inspected?.targetId === t.tableId
                            ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50'
                            : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900'
                        }`}
                      >
                        <div>
                          <div className="font-semibold flex items-center gap-1.5">
                            <TableIcon className="w-3.5 h-3.5 text-indigo-500" />
                            Bảng {t.tableIndex + 1} ({t.rowCount}×{t.colCount}) · {t.tableRole}
                            {t.overflowsWidth && (
                              <span className="px-1.5 py-0.5 rounded bg-rose-100 text-rose-700 text-[10px]">
                                Tràn +{t.overflowMm}mm
                              </span>
                            )}
                          </div>
                          <div className="text-[10px] text-slate-500 mt-0.5">
                            Rộng {t.totalWidthMm}/{t.printableWidthMm} mm · Sec {t.locator.sectionIndex} · Tr.{t.locator.estimatedPageStart}
                          </div>
                        </div>
                        <span className="text-[10px] font-mono text-slate-400">{t.tableId}</span>
                      </button>
                    ))}
                    {report.maps.visualObjects.map((vis) => (
                      <button
                        key={vis.objectId}
                        type="button"
                        onClick={() => selectVisual(vis)}
                        className={`w-full text-left px-2.5 py-2 rounded-lg border text-xs flex items-center justify-between gap-2 ${
                          inspected?.targetId === vis.objectId
                            ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50'
                            : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900'
                        }`}
                      >
                        <div>
                          <div className="font-semibold flex items-center gap-1.5">
                            <ImageIcon className="w-3.5 h-3.5 text-emerald-500" />
                            {vis.objectKind === 'textBox' ? 'Text Box' : 'Hình ảnh'} · {vis.widthMm}×{vis.heightMm} mm
                            {vis.overflowsPrintableArea && (
                              <span className="px-1.5 py-0.5 rounded bg-rose-100 text-rose-700 text-[10px]">Tràn vùng in</span>
                            )}
                          </div>
                          <div className="text-[10px] text-slate-500 mt-0.5 truncate">
                            {vis.textExcerpt || vis.mediaTarget || vis.anchorMode}
                          </div>
                        </div>
                        <span className="text-[10px] font-mono text-slate-400">{vis.objectId}</span>
                      </button>
                    ))}
                  </>
                )}

                {activeMapTab === 'section' &&
                  report.maps.sectionMap.map((s) => (
                    <button
                      key={s.sectionId}
                      type="button"
                      onClick={() => selectSection(s)}
                      className={`w-full text-left px-3 py-2 rounded-lg border text-xs flex items-center justify-between ${
                        inspected?.targetId === s.sectionId
                          ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50'
                          : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900'
                      }`}
                    >
                      <div>
                        <div className="font-bold flex items-center gap-1.5">
                          <Columns3 className="w-3.5 h-3.5 text-blue-600" />
                          Section {s.sectionIndex} · {s.orientation === 'landscape' ? 'Ngang' : 'Dọc'} ({s.paperWidthMm}×{s.paperHeightMm} mm)
                        </div>
                        <div className="text-[10px] text-slate-500 mt-0.5">
                          {s.columnCount} cột · Break: {s.breakType} · Trang {s.pageStart}–{s.pageEnd} · Vùng in {s.printableWidthMm}×{s.printableHeightMm} mm
                        </div>
                      </div>
                      <span className="text-[10px] font-mono text-slate-400">{s.sectionId}</span>
                    </button>
                  ))}

                {activeMapTab === 'style' &&
                  report.maps.styleMap
                    .filter((s) => !filterLower || s.styleName.toLowerCase().includes(filterLower) || s.styleId.toLowerCase().includes(filterLower))
                    .map((st) => (
                      <button
                        key={st.styleId}
                        type="button"
                        onClick={() => selectStyle(st)}
                        className={`w-full text-left px-3 py-2 rounded-lg border text-xs flex items-center justify-between ${
                          inspected?.targetId === st.styleId
                            ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50'
                            : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900'
                        }`}
                      >
                        <div>
                          <div className="font-semibold">
                            {st.styleName} <span className="text-[10px] text-slate-400">({st.styleId})</span>
                          </div>
                          <div className="text-[10px] text-slate-500 mt-0.5">
                            Dùng {st.usageCount} lần · Kế thừa: {st.basedOn ?? 'Gốc'} · Xung đột: {st.directConflictCount}
                          </div>
                        </div>
                        <span className="text-[10px] text-slate-500">{st.styleType}</span>
                      </button>
                    ))}

                {activeMapTab === 'headerFooter' &&
                  (report.maps.headerFooterMap.length === 0 ? (
                    <p className="text-xs text-slate-500">Tài liệu chưa có phần Header/Footer riêng biệt.</p>
                  ) : (
                    report.maps.headerFooterMap.map((hf) => (
                      <button
                        key={hf.hfId}
                        type="button"
                        onClick={() => selectHeaderFooter(hf)}
                        className={`w-full text-left px-3 py-2 rounded-lg border text-xs flex items-center justify-between ${
                          inspected?.targetId === hf.hfId
                            ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50'
                            : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900'
                        }`}
                      >
                        <div className="min-w-0">
                          <div className="font-semibold">
                            {hf.hfKind.toUpperCase()} · {hf.partName}
                          </div>
                          <div className="text-[10px] text-slate-500 truncate mt-0.5">
                            Sec {hf.linkedSections.join(', ') || '-'} · {hf.textPreview || '(trống)'}
                          </div>
                        </div>
                        <span className="text-[10px] font-mono text-slate-400">{hf.hfId}</span>
                      </button>
                    ))
                  ))}

                {activeMapTab === 'pageLayout' &&
                  report.maps.pageLayoutMap.map((pg) => (
                    <button
                      key={pg.pageId}
                      type="button"
                      onClick={() => selectPage(pg)}
                      className={`w-full text-left px-3 py-2 rounded-lg border text-xs flex items-center justify-between ${
                        inspected?.targetId === pg.pageId
                          ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/50'
                          : 'border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900'
                      }`}
                    >
                      <div>
                        <div className="font-semibold">
                          Trang {pg.pageNumber} · Section {pg.sectionIndex} ({pg.orientation})
                        </div>
                        <div className="text-[10px] text-slate-500 mt-0.5">
                          Lấp đầy {pg.fillRatioPct}% ({pg.usedHeightMm}/{pg.printableHeightMm} mm) · {pg.elementIds.length} thành phần
                        </div>
                      </div>
                      <span className="text-[10px] font-mono text-slate-400">{pg.pageId}</span>
                    </button>
                  ))}
              </div>
            </div>

            {/* Component Inspector */}
            <div className="xl:col-span-5 mw-panel p-3 sm:p-4 space-y-3">
              <div className="text-xs font-bold flex items-center gap-1.5">
                <Layers className="w-4 h-4 text-indigo-500" />
                Kiểm tra Thành phần (Component Inspector)
              </div>
              {inspected ? (
                <div className="space-y-3">
                  <div className="rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/50 p-3">
                    <div className="text-sm font-bold">{inspected.title}</div>
                    <div className="text-xs text-slate-600 dark:text-slate-400 mt-0.5 break-words">{inspected.subtitle}</div>
                    {inspected.locator && (
                      <div className="mt-2 pt-2 border-t border-slate-200 dark:border-slate-800 text-[11px] font-mono text-slate-500 space-y-0.5">
                        <div>Path: {inspected.locator.partName} :: {inspected.locator.elementPath}</div>
                        <div>
                          Sec {inspected.locator.sectionIndex} · Trang {inspected.locator.estimatedPageStart}
                          {inspected.locator.estimatedPageEnd > inspected.locator.estimatedPageStart
                            ? `–${inspected.locator.estimatedPageEnd}`
                            : ''} · Cột {inspected.locator.columnIndex + 1} · ({inspected.locator.xLeftMm}, {inspected.locator.yTopMm}) mm · {inspected.locator.widthMm}×{inspected.locator.heightMm} mm
                        </div>
                      </div>
                    )}
                  </div>

                  <dl className="divide-y divide-slate-200 dark:divide-slate-800 text-xs">
                    {Object.entries(inspected.meta).map(([k, v]) => (
                      <div key={k} className="py-1.5 flex justify-between gap-3">
                        <dt className="text-slate-500 shrink-0">{k}</dt>
                        <dd className="font-medium text-right break-words">{String(v ?? '-')}</dd>
                      </div>
                    ))}
                  </dl>
                </div>
              ) : (
                <p className="text-xs text-slate-500 py-8 text-center">
                  Chọn một thành phần bất kỳ từ Sơ đồ Cấu trúc hoặc 7 Bản đồ bên trái để xem tọa độ & thông số chi tiết.
                </p>
              )}
            </div>
          </div>

          {/* Layout Warnings & Smart Proposals (Per-proposal Accept/Reject) */}
          <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
            {/* Warnings Panel */}
            <div className="mw-panel p-3 sm:p-4 space-y-2.5">
              <div className="text-xs font-bold flex items-center gap-1.5">
                <AlertCircle className="w-4 h-4 text-amber-500" />
                Cảnh báo Bố cục ({report.warnings.length})
              </div>
              {report.warnings.length === 0 ? (
                <div className="text-xs text-emerald-600 flex items-center gap-1.5 py-4">
                  <CheckCircle2 className="w-4 h-4" /> Không phát hiện cảnh báo bố cục nghiêm trọng nào.
                </div>
              ) : (
                <div className="max-h-80 overflow-y-auto space-y-2 pr-1">
                  {report.warnings.map((w) => (
                    <div
                      key={w.warningId}
                      className="rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-2.5 text-xs space-y-1"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="font-semibold flex items-center gap-1.5">
                          <span className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${SEV_BADGE[w.severity]}`}>
                            {w.severity}
                          </span>
                          <span>{w.title}</span>
                        </div>
                        <button
                          type="button"
                          onClick={() => jumpToTargetId(w.targetElementId)}
                          className="text-[11px] text-blue-600 hover:underline shrink-0"
                        >
                          Xem
                        </button>
                      </div>
                      <p className="text-[11px] text-slate-500">{w.detail}</p>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* Proposals Panel */}
            <div className="mw-panel p-3 sm:p-4 space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="text-xs font-bold flex items-center gap-1.5">
                  <Sparkles className="w-4 h-4 text-blue-600" />
                  Đề xuất Điều chỉnh Thông minh ({report.proposals.length})
                </div>
                <label className="inline-flex items-center gap-1.5 text-[11px] font-medium cursor-pointer">
                  <input
                    type="checkbox"
                    checked={applyAutoHighConf}
                    onChange={(e) => setApplyAutoHighConf(e.target.checked)}
                    className="rounded border-slate-300 text-blue-600"
                  />
                  Tự áp dụng đề xuất độ tin cậy cao
                </label>
              </div>

              {report.proposals.length === 0 ? (
                <p className="text-xs text-slate-500 py-4">Bố cục hiện tại đã cân đối, không có đề xuất cần chỉnh thêm.</p>
              ) : (
                <div className="max-h-80 overflow-y-auto space-y-2 pr-1">
                  {report.proposals.map((p: LayoutProposal) => {
                    const state = decisions[p.proposalId] ?? 'pending';
                    const isConfirm = p.tier === 'requiresConfirmation';
                    return (
                      <div
                        key={p.proposalId}
                        className={`rounded-xl border p-2.5 text-xs space-y-1.5 ${
                          state === 'accepted'
                            ? 'border-emerald-300 bg-emerald-50/40 dark:border-emerald-800 dark:bg-emerald-950/20'
                            : state === 'rejected'
                            ? 'border-slate-200 bg-slate-100/60 dark:border-slate-800 dark:bg-slate-900/40 opacity-70'
                            : 'border-amber-200 bg-white dark:border-amber-900/60 dark:bg-slate-900'
                        }`}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div className="space-y-0.5">
                            <div className="flex items-center gap-1.5 flex-wrap">
                              <span
                                className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${
                                  isConfirm
                                    ? 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300'
                                    : 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300'
                                }`}
                              >
                                {isConfirm ? 'Cần xác nhận' : `Tin cậy ${Math.round(p.confidence * 100)}%`}
                              </span>
                              {p.affectsPagination && <span className="px-1 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px]">Phân trang</span>}
                              {p.affectsSection && <span className="px-1 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px]">Section</span>}
                              {p.affectsTable && <span className="px-1 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px]">Bảng</span>}
                              {p.affectsSignature && <span className="px-1 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px]">Chữ ký</span>}
                            </div>
                            <div className="font-bold">{p.title}</div>
                          </div>

                          <div className="flex items-center gap-1 shrink-0">
                            <button
                              type="button"
                              onClick={() => setDecisions((prev) => ({ ...prev, [p.proposalId]: 'accepted' }))}
                              className={`inline-flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-semibold ${
                                state === 'accepted'
                                  ? 'bg-emerald-600 text-white'
                                  : 'border border-emerald-300 text-emerald-700 hover:bg-emerald-50'
                              }`}
                            >
                              <Check className="w-3 h-3" /> Chấp nhận
                            </button>
                            <button
                              type="button"
                              onClick={() => setDecisions((prev) => ({ ...prev, [p.proposalId]: 'rejected' }))}
                              className={`inline-flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-semibold ${
                                state === 'rejected'
                                  ? 'bg-rose-600 text-white'
                                  : 'border border-slate-300 text-slate-600 hover:bg-slate-100'
                              }`}
                            >
                              <X className="w-3 h-3" /> Từ chối
                            </button>
                          </div>
                        </div>
                        <p className="text-[11px] text-slate-600 dark:text-slate-400">{p.reason}</p>
                        <div className="text-[10px] text-slate-500 flex items-center justify-between gap-2">
                          <span>Thao tác: {p.suggestedChange}</span>
                          {p.targetElementIds[0] && (
                            <button
                              type="button"
                              onClick={() => jumpToTargetId(p.targetElementIds[0])}
                              className="text-blue-600 hover:underline shrink-0"
                            >
                              Kiểm tra
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}

              {/* Execution Footer */}
              <div className="pt-2 border-t border-slate-200 dark:border-slate-800 flex flex-wrap items-center justify-between gap-2">
                <div className="inline-flex items-center rounded-lg border border-slate-200 dark:border-slate-700 p-0.5 text-[11px]">
                  <button
                    type="button"
                    onClick={() => setOutputMode('newCopy')}
                    className={`px-2 py-1 rounded font-semibold ${outputMode === 'newCopy' ? 'bg-blue-600 text-white' : ''}`}
                  >
                    Tạo bản sao (_layout)
                  </button>
                  <button
                    type="button"
                    onClick={() => setOutputMode('overwriteWithBackup')}
                    className={`px-2 py-1 rounded font-semibold ${outputMode === 'overwriteWithBackup' ? 'bg-amber-600 text-white' : ''}`}
                  >
                    Ghi đè + Backup
                  </button>
                </div>

                <button
                  type="button"
                  disabled={busy || report.proposals.length === 0}
                  onClick={() => void handleExecuteProposals()}
                  className="mw-button mw-button-primary"
                >
                  <Play className="w-3.5 h-3.5 fill-current" />
                  Áp dụng Đề xuất Đã chọn
                </button>
              </div>
            </div>
          </div>

          {/* Post-Save Execution Verification */}
          {execResult && (
            <div className="mw-panel p-3.5 space-y-2 border-emerald-300 dark:border-emerald-800">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2 text-xs font-bold text-emerald-700 dark:text-emerald-400">
                  <ShieldCheck className="w-4 h-4" />
                  {execResult.success
                    ? `Hoàn tất: đã áp dụng ${execResult.appliedProposalIds.length} đề xuất · Bảo toàn chữ ${execResult.contentPreservedVerified ? '100%' : 'LỖI'} · Bảo toàn OpenXML ${execResult.relationshipsPreservedVerified ? '100%' : 'Cảnh báo'}`
                    : execResult.error}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  {execResult.backupPath && (
                    <button type="button" onClick={() => void handleRestoreBackup()} className="mw-button mw-button-secondary">
                      <RotateCcw className="w-3.5 h-3.5" /> Khôi phục Backup
                    </button>
                  )}
                  {isNative && execResult.outputPath && (
                    <button
                      type="button"
                      onClick={() => void bridge.invoke('open_file', { path: execResult.outputPath })}
                      className="mw-button mw-button-secondary"
                    >
                      <ExternalLink className="w-3.5 h-3.5" /> Mở tệp kết quả
                    </button>
                  )}
                  {!isNative && execResult.blobUrl && (
                    <button
                      type="button"
                      onClick={() => triggerBrowserDownload(execResult.blobUrl!, execResult.outputName || execResult.outputPath)}
                      className="mw-button mw-button-primary"
                    >
                      <Download className="w-3.5 h-3.5" /> Tải tệp về
                    </button>
                  )}
                  {!isNative && execResult.outputBlob && (
                    <button
                      type="button"
                      onClick={() => void shareOutputBlob(execResult.outputBlob!, execResult.outputName || execResult.outputPath)}
                      className="mw-button mw-button-secondary"
                    >
                      <Share2 className="w-3.5 h-3.5" /> Chia sẻ
                    </button>
                  )}
                </div>
              </div>
              {execResult.logs.length > 0 && (
                <div className="text-[11px] text-slate-600 dark:text-slate-400 space-y-0.5">
                  {execResult.logs.map((l, idx) => (
                    <div key={idx} className="flex items-center gap-1">
                      <Info className="w-3 h-3 text-slate-400 shrink-0" />
                      <span>{l}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
};
