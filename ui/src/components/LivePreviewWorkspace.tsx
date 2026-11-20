import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertCircle,
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  BookOpen,
  CheckCircle2,
  Columns2,
  ExternalLink,
  Eye,
  FileCheck2,
  FileSearch,
  FileText,
  FolderOpen,
  Highlighter,
  Layers,
  Loader2,
  Monitor,
  Play,
  Printer,
  RefreshCw,
  RotateCcw,
  Search,
  Sliders,
  X,
} from 'lucide-react';
import { bridge } from '../bridge';
import {
  DEFAULT_HEADER_BUILDER_CONFIG,
  type AdminDocType,
  type AuditSyncMarker,
  type DocumentTemplate,
  type FileProcessResult,
  type FileQueueItem,
  type HeaderActionMode,
  type OrganizationProfile,
  type PaperSize,
  type PreviewBodyAlignment,
  type PreviewBuildRequest,
  type PreviewDocumentRender,
  type PreviewImageRef,
  type PreviewOrientationOverride,
  type PreviewOverrideConfig,
  type PreviewPageModel,
  type PreviewParagraphBlock,
  type PreviewSessionPayload,
  type PreviewTableBlock,
  type PreviewTableBorders,
  type ProcessConfig,
  type StdDocScope,
  type TemplateCatalogResponse,
  type WordComPreviewVerification,
} from '../types';
import { listWebOrgProfiles } from '../utils/webHeaderBuilder';
import {
  PREVIEW_FIDELITY_DISCLAIMER_VI,
  applyClientIncrementalPatch,
  buildWebLivePreview,
  cancelWebPreviewRender,
  loadWebPreviewMediaUrl,
} from '../utils/webLivePreview';
import { listWebTemplates } from '../utils/webTemplateEngine';

export interface LivePreviewFocusTarget {
  filePath: string;
  elementPath?: string;
}

export interface LivePreviewWorkspaceProps {
  files: FileQueueItem[];
  results: Record<string, FileProcessResult>;
  config: ProcessConfig;
  focusTarget?: LivePreviewFocusTarget | null;
  onClearFocusTarget?: () => void;
  onPickFiles: () => void;
  onProcessCurrentFile?: (filePath?: string, overrides?: PreviewOverrideConfig) => void;
  onStatusToast: (kind: 'success' | 'error' | 'info', msg: string) => void;
}

type ViewMode = 'before' | 'after' | 'split';
type LeftTab = 'pages' | 'search' | 'audit';
type RightTab = 'controls' | 'print';
type ZoomMode = number | 'fitWidth' | 'fullPage';

const DEFAULT_OVERRIDES: PreviewOverrideConfig = {
  enabled: false,
  fontName: null,
  fontSizePt: null,
  tableFontSizePt: null,
  bodyAlignment: null,
  lineSpacingMult: null,
  spaceBeforePt: null,
  spaceAfterPt: null,
  firstLineIndentCm: null,
  paperSize: null,
  orientationOverride: null,
  marginTopCm: null,
  marginBottomCm: null,
  marginLeftCm: null,
  marginRightCm: null,
  autoSwapBindingMargin: true,
  templateId: null,
  orgProfileId: null,
  headerConfig: null,
  applyHeaderMode: null,
  applyVnCleaner: false,
  applyStdGroupA: false,
};

const SCOPE_OPTIONS: { id: StdDocScope; label: string }[] = [
  { id: 'autoDetect', label: 'Tự động nhận diện' },
  { id: 'adminNd30', label: 'Hành chính (NĐ 30/2020)' },
  { id: 'partyDocument', label: 'Văn bản Đảng (36-HD)' },
  { id: 'enterpriseInternal', label: 'Doanh nghiệp / Nội bộ' },
  { id: 'specializedLegal', label: 'Quy phạm / Chuyên ngành' },
  { id: 'academicThesis', label: 'Luận văn / Học thuật' },
  { id: 'freeform', label: 'Tài liệu tự do' },
];

const MEDIA_MEM_LRU = new Map<string, string>();
function cacheMediaDataUrl(key: string, url: string) {
  if (MEDIA_MEM_LRU.size >= 45) {
    const oldest = MEDIA_MEM_LRU.keys().next().value;
    if (oldest) MEDIA_MEM_LRU.delete(oldest);
  }
  MEDIA_MEM_LRU.set(key, url);
}

const LazyPreviewImage: React.FC<{
  img: PreviewImageRef;
  fileItem?: FileQueueItem;
  isNative: boolean;
}> = ({ img, fileItem, isNative }) => {
  const cacheKey = `${fileItem?.path || ''}::${img.mediaEntry}`;
  const [src, setSrc] = useState<string | null>(() => MEDIA_MEM_LRU.get(cacheKey) || null);
  const [err, setErr] = useState(false);

  useEffect(() => {
    const hit = MEDIA_MEM_LRU.get(cacheKey);
    if (hit) {
      setSrc(hit);
      return;
    }
    if (!fileItem) return;
    let cancelled = false;
    const load = async () => {
      try {
        if (isNative && !fileItem.webFile) {
          const res = await bridge.invoke<{ dataUrl: string }>('get_preview_media', {
            path: fileItem.path,
            mediaEntry: img.mediaEntry,
          });
          if (!cancelled && res?.dataUrl) {
            cacheMediaDataUrl(cacheKey, res.dataUrl);
            setSrc(res.dataUrl);
          }
        } else {
          const url = await loadWebPreviewMediaUrl(fileItem, img.mediaEntry);
          if (!cancelled && url) {
            cacheMediaDataUrl(cacheKey, url);
            setSrc(url);
          }
        }
      } catch {
        if (!cancelled) setErr(true);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [cacheKey, fileItem, img.mediaEntry, isNative]);

  const style: React.CSSProperties = {
    width: `${img.widthMm}mm`,
    height: `${img.heightMm}mm`,
    maxWidth: '100%',
  };

  if (src) {
    return (
      <img
        src={src}
        alt={img.altText}
        style={style}
        className={`block object-contain my-1 ${img.overflowsPrintableArea ? 'ring-2 ring-rose-500' : ''}`}
      />
    );
  }

  return (
    <div
      style={style}
      className="my-1 border border-dashed border-slate-300 bg-slate-100 text-slate-500 flex items-center justify-center text-[9px] px-1 text-center"
    >
      {err ? `Không tải được ảnh (${img.mediaEntry})` : `Đang tải ảnh ${img.widthMm}×${img.heightMm}mm...`}
    </div>
  );
};

function borderCss(edge: PreviewTableBorders['top']): string {
  if (!edge.visible) return 'none';
  const style = edge.style === 'dashed' ? 'dashed' : edge.style === 'dotted' ? 'dotted' : 'solid';
  return `${Math.max(0.5, edge.widthPt)}pt ${style} #${edge.colorHex || '334155'}`;
}

function alignToCss(al: string): React.CSSProperties['textAlign'] {
  if (al === 'both' || al === 'distribute') return 'justify';
  if (al === 'center') return 'center';
  if (al === 'right') return 'right';
  return 'left';
}

function highlightQueryText(text: string, query: string): React.ReactNode {
  if (!query.trim()) return text;
  const q = query.trim();
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx < 0) return text;
  return (
    <>
      {text.slice(0, idx)}
      <mark className="bg-amber-300 text-slate-900 px-0.5 rounded-[1px]">
        {text.slice(idx, idx + q.length)}
      </mark>
      {highlightQueryText(text.slice(idx + q.length), q)}
    </>
  );
}

const ParagraphBlockView: React.FC<{
  para: PreviewParagraphBlock;
  fileItem?: FileQueueItem;
  isNative: boolean;
  isSelected: boolean;
  isDiffHighlighted: boolean;
  hasAuditMarker: boolean;
  searchQuery: string;
  onSelect: (elementPath: string) => void;
}> = ({
  para,
  fileItem,
  isNative,
  isSelected,
  isDiffHighlighted,
  hasAuditMarker,
  searchQuery,
  onSelect,
}) => {
  const lh = para.exactLinePt
    ? `${para.exactLinePt}pt`
    : `${Math.max(1.0, para.lineSpacingMult * 1.12)}`;

  const ringCls = isSelected
    ? 'ring-2 ring-blue-500 bg-blue-50/40'
    : hasAuditMarker
      ? 'ring-1 ring-rose-400 bg-rose-50/35'
      : isDiffHighlighted
        ? 'ring-1 ring-amber-400 bg-amber-50/40'
        : 'hover:bg-sky-50/30';

  return (
    <div
      data-mw-el={para.elementPath}
      onClick={(e) => {
        e.stopPropagation();
        onSelect(para.elementPath);
      }}
      title={`${para.elementPath} (${para.alignment})`}
      style={{
        textAlign: alignToCss(para.alignment),
        marginLeft: para.indentLeftMm ? `${para.indentLeftMm}mm` : undefined,
        marginRight: para.indentRightMm ? `${para.indentRightMm}mm` : undefined,
        textIndent: para.firstLineIndentMm
          ? `${para.firstLineIndentMm}mm`
          : para.hangingIndentMm
            ? `-${para.hangingIndentMm}mm`
            : undefined,
        marginTop: para.spaceBeforePt ? `${para.spaceBeforePt}pt` : undefined,
        marginBottom: `${Math.max(0, para.spaceAfterPt)}pt`,
        lineHeight: lh,
        minHeight: '3.5mm',
      }}
      className={`cursor-pointer rounded-[2px] transition-colors ${ringCls}`}
    >
      {para.listPrefix && (
        <span
          style={{
            fontFamily: `"${para.spans[0]?.fontFamily || 'Times New Roman'}", serif`,
            fontSize: `${para.spans[0]?.fontSizePt || 14}pt`,
          }}
        >
          {para.listPrefix}
        </span>
      )}
      {para.spans.map((sp, i) => (
        <span
          key={i}
          style={{
            fontFamily: `"${sp.fontFamily}", "Times New Roman", serif`,
            fontSize: `${sp.fontSizePt}pt`,
            fontWeight: sp.bold ? 700 : 400,
            fontStyle: sp.italic ? 'italic' : 'normal',
            textDecoration: [sp.underline ? 'underline' : '', sp.strike ? 'line-through' : '']
              .filter(Boolean)
              .join(' ') || undefined,
            color: sp.colorHex ? `#${sp.colorHex}` : '#0f172a',
            backgroundColor: sp.isDynamicPageField ? 'rgba(59,130,246,0.08)' : undefined,
            verticalAlign:
              sp.vertAlign === 'superscript'
                ? 'super'
                : sp.vertAlign === 'subscript'
                  ? 'sub'
                  : undefined,
            whiteSpace: 'pre-wrap',
          }}
        >
          {highlightQueryText(sp.text, searchQuery)}
        </span>
      ))}
      {para.images.map((im, idx) => (
        <LazyPreviewImage key={`${im.relId}_${idx}`} img={im} fileItem={fileItem} isNative={isNative} />
      ))}
    </div>
  );
};

const TableBlockView: React.FC<{
  tbl: PreviewTableBlock;
  fileItem?: FileQueueItem;
  isNative: boolean;
  selectedPath: string | null;
  diffPathSet: Set<string>;
  auditPathSet: Set<string>;
  showDiffHighlight: boolean;
  searchQuery: string;
  onSelect: (elementPath: string) => void;
}> = ({
  tbl,
  fileItem,
  isNative,
  selectedPath,
  diffPathSet,
  auditPathSet,
  showDiffHighlight,
  searchQuery,
  onSelect,
}) => {
  const isSelected = selectedPath === tbl.elementPath;
  const hasDiff = showDiffHighlight && diffPathSet.has(tbl.elementPath);
  const hasAudit = auditPathSet.has(tbl.elementPath) || auditPathSet.has(`tbl[${tbl.tableIndex}]`);

  return (
    <div
      data-mw-el={tbl.elementPath}
      onClick={(e) => {
        e.stopPropagation();
        onSelect(tbl.elementPath);
      }}
      className={`relative my-1 cursor-pointer ${
        tbl.overflowsPrintableWidth
          ? 'ring-2 ring-rose-500'
          : isSelected
            ? 'ring-2 ring-blue-500'
            : hasAudit
              ? 'ring-1 ring-rose-400'
              : hasDiff
                ? 'ring-1 ring-amber-400'
                : ''
      }`}
      style={{
        marginLeft: tbl.indentLeftMm ? `${tbl.indentLeftMm}mm` : undefined,
        width: tbl.totalWidthMm > 5 ? `${tbl.totalWidthMm}mm` : '100%',
        maxWidth: tbl.overflowsPrintableWidth ? 'none' : '100%',
      }}
    >
      {tbl.rowSliceRange[0] > 0 && (
        <div className="text-[8px] font-mono text-sky-700 bg-sky-50 px-1 border border-sky-200 mb-0.5">
          Bảng #{tbl.tableIndex + 1} (tiếp theo từ dòng {tbl.rowSliceRange[0] + 1})
        </div>
      )}
      {tbl.overflowsPrintableWidth && (
        <div className="text-[8px] font-bold text-rose-700 bg-rose-50 px-1 border border-rose-300 mb-0.5">
          Cảnh báo bản in: Bảng tràn lề phải ({tbl.totalWidthMm}mm)
        </div>
      )}
      <table
        style={{
          width: '100%',
          borderCollapse: 'collapse',
          tableLayout: 'fixed',
        }}
      >
        <tbody>
          {tbl.rows.map((row) => (
            <tr key={row.rowIndex} className={row.isHeaderRow && tbl.tableRole === 'dataTable' ? 'bg-slate-100/70' : undefined}>
              {row.cells.map((cell) => {
                if (cell.mergedContinuation) return null;
                return (
                  <td
                    key={cell.colIndex}
                    colSpan={cell.colSpan > 1 ? cell.colSpan : undefined}
                    rowSpan={cell.rowSpan > 1 ? cell.rowSpan : undefined}
                    style={{
                      width: cell.widthMm > 2 ? `${cell.widthMm}mm` : undefined,
                      verticalAlign: cell.vAlign === 'center' ? 'middle' : cell.vAlign === 'bottom' ? 'bottom' : 'top',
                      backgroundColor: cell.shadingHex ? `#${cell.shadingHex}` : undefined,
                      borderTop: borderCss(cell.borders.top),
                      borderBottom: borderCss(cell.borders.bottom),
                      borderLeft: borderCss(cell.borders.left),
                      borderRight: borderCss(cell.borders.right),
                      padding: tbl.tableRole === 'dataTable' ? '1.2mm 1.8mm' : '0.4mm 1.2mm',
                    }}
                  >
                    {cell.paragraphs.map((cp) => (
                      <ParagraphBlockView
                        key={cp.elementPath}
                        para={cp}
                        fileItem={fileItem}
                        isNative={isNative}
                        isSelected={selectedPath === cp.elementPath}
                        isDiffHighlighted={showDiffHighlight && diffPathSet.has(cp.elementPath)}
                        hasAuditMarker={auditPathSet.has(cp.elementPath)}
                        searchQuery={searchQuery}
                        onSelect={onSelect}
                      />
                    ))}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};

const A4PhysicalSheet: React.FC<{
  page: PreviewPageModel;
  zoom: number;
  isVirtualActive: boolean;
  isActivePage: boolean;
  showPrintGuides: boolean;
  showDiffHighlight: boolean;
  fileItem?: FileQueueItem;
  isNative: boolean;
  selectedPath: string | null;
  diffPathSet: Set<string>;
  auditPathSet: Set<string>;
  searchQuery: string;
  onActivatePage: (pageNum: number) => void;
  onSelectElement: (elementPath: string, pageNum: number) => void;
}> = ({
  page,
  zoom,
  isVirtualActive,
  isActivePage,
  showPrintGuides,
  showDiffHighlight,
  fileItem,
  isNative,
  selectedPath,
  diffPathSet,
  auditPathSet,
  searchQuery,
  onActivatePage,
  onSelectElement,
}) => {
  // 1mm = 3.7795275 CSS px at 96dpi
  const pxPerMm = 3.7795275 * zoom;
  const wPx = page.widthMm * pxPerMm;
  const hPx = page.heightMm * pxPerMm;

  return (
    <div
      className="flex flex-col items-center gap-1"
      onClick={() => onActivatePage(page.pageNumber)}
    >
      <div className="flex items-center justify-between w-full max-w-full text-[10px] text-slate-500 px-1">
        <span>
          Trang {page.pageNumber}/{page.totalPages} · S{page.sectionIndex} · {page.paperLabel}{' '}
          {page.orientation === 'landscape' ? 'Ngang' : 'Dọc'} ({page.widthMm}×{page.heightMm}mm)
        </span>
        {page.hasOverflowWarning && (
          <span className="inline-flex items-center gap-0.5 text-rose-600 font-semibold">
            <AlertTriangle className="w-3 h-3" /> Tràn lề / cắt mép
          </span>
        )}
      </div>

      {/* Outer scaled layout box preserves exact scrollbar height even when unmounted outside window */}
      <div
        style={{ width: `${wPx}px`, height: `${hPx}px` }}
        className={`relative bg-white text-slate-900 shadow-lg border transition-shadow ${
          isActivePage ? 'border-blue-500 ring-2 ring-blue-500/25' : 'border-slate-300'
        } overflow-hidden shrink-0`}
      >
        {isVirtualActive ? (
          <div
            style={{
              width: `${page.widthMm}mm`,
              height: `${page.heightMm}mm`,
              transform: `scale(${zoom})`,
              transformOrigin: 'top left',
              paddingTop: `${page.marginTopMm}mm`,
              paddingBottom: `${page.marginBottomMm}mm`,
              paddingLeft: `${page.marginLeftMm}mm`,
              paddingRight: `${page.marginRightMm}mm`,
            }}
            className="relative flex flex-col justify-between bg-white text-slate-950 box-border select-text"
          >
            {/* Optional Print Safety Zones (5mm unprintable edge & dashed margin guides) */}
            {showPrintGuides && (
              <>
                <div
                  className="pointer-events-none absolute inset-0 border-[5mm] border-rose-500/10"
                  title="Vùng mép cắt 5mm khuyến cáo không để chữ tràn vào"
                />
                <div
                  className="pointer-events-none absolute border border-dashed border-sky-400/80"
                  style={{
                    top: `${page.marginTopMm}mm`,
                    bottom: `${page.marginBottomMm}mm`,
                    left: `${page.marginLeftMm}mm`,
                    right: `${page.marginRightMm}mm`,
                  }}
                />
                <span className="pointer-events-none absolute top-1 left-1/2 -translate-x-1/2 text-[7px] font-mono bg-sky-50 text-sky-700 px-1 rounded">
                  Trên {page.marginTopMm}mm
                </span>
                <span className="pointer-events-none absolute bottom-1 left-1/2 -translate-x-1/2 text-[7px] font-mono bg-sky-50 text-sky-700 px-1 rounded">
                  Dưới {page.marginBottomMm}mm
                </span>
                <span className="pointer-events-none absolute left-1 top-1/2 -translate-y-1/2 -rotate-90 text-[7px] font-mono bg-sky-50 text-sky-700 px-1 rounded">
                  Trái {page.marginLeftMm}mm
                </span>
                <span className="pointer-events-none absolute right-1 top-1/2 -translate-y-1/2 rotate-90 text-[7px] font-mono bg-sky-50 text-sky-700 px-1 rounded">
                  Phải {page.marginRightMm}mm
                </span>
              </>
            )}

            {/* Header zone positioned inside top margin */}
            {page.headerBlocks.length > 0 && (
              <div
                style={{
                  position: 'absolute',
                  top: `${Math.max(4, page.marginTopMm * 0.45)}mm`,
                  left: `${page.marginLeftMm}mm`,
                  right: `${page.marginRightMm}mm`,
                }}
                className="pointer-events-auto opacity-90"
              >
                {page.headerBlocks.map((hb) => (
                  <ParagraphBlockView
                    key={hb.elementPath}
                    para={hb}
                    fileItem={fileItem}
                    isNative={isNative}
                    isSelected={selectedPath === hb.elementPath}
                    isDiffHighlighted={false}
                    hasAuditMarker={false}
                    searchQuery={searchQuery}
                    onSelect={(ep) => onSelectElement(ep, page.pageNumber)}
                  />
                ))}
              </div>
            )}

            {/* Printable Body Blocks */}
            <div className="flex-1 min-h-0 overflow-hidden relative z-10">
              {page.blocks.map((blk, idx) =>
                blk.kind === 'paragraph' ? (
                  <ParagraphBlockView
                    key={`${blk.elementPath}_${idx}`}
                    para={blk}
                    fileItem={fileItem}
                    isNative={isNative}
                    isSelected={selectedPath === blk.elementPath}
                    isDiffHighlighted={showDiffHighlight && diffPathSet.has(blk.elementPath)}
                    hasAuditMarker={auditPathSet.has(blk.elementPath)}
                    searchQuery={searchQuery}
                    onSelect={(ep) => onSelectElement(ep, page.pageNumber)}
                  />
                ) : (
                  <TableBlockView
                    key={`${blk.elementPath}_${blk.rowSliceRange[0]}_${idx}`}
                    tbl={blk}
                    fileItem={fileItem}
                    isNative={isNative}
                    selectedPath={selectedPath}
                    diffPathSet={diffPathSet}
                    auditPathSet={auditPathSet}
                    showDiffHighlight={showDiffHighlight}
                    searchQuery={searchQuery}
                    onSelect={(ep) => onSelectElement(ep, page.pageNumber)}
                  />
                ),
              )}
            </div>

            {/* Footer zone positioned inside bottom margin */}
            {page.footerBlocks.length > 0 && (
              <div
                style={{
                  position: 'absolute',
                  bottom: `${Math.max(4, page.marginBottomMm * 0.4)}mm`,
                  left: `${page.marginLeftMm}mm`,
                  right: `${page.marginRightMm}mm`,
                }}
                className="pointer-events-auto opacity-90"
              >
                {page.footerBlocks.map((fb) => (
                  <ParagraphBlockView
                    key={fb.elementPath}
                    para={fb}
                    fileItem={fileItem}
                    isNative={isNative}
                    isSelected={selectedPath === fb.elementPath}
                    isDiffHighlighted={false}
                    hasAuditMarker={false}
                    searchQuery={searchQuery}
                    onSelect={(ep) => onSelectElement(ep, page.pageNumber)}
                  />
                ))}
              </div>
            )}
          </div>
        ) : (
          <div className="w-full h-full flex flex-col items-center justify-center bg-slate-50 text-slate-400 text-xs">
            <span>Trang {page.pageNumber} ({page.paperLabel})</span>
            <span className="text-[10px]">Tự động nạp chi tiết khi cuộn tới</span>
          </div>
        )}
      </div>
    </div>
  );
};

export const LivePreviewWorkspace: React.FC<LivePreviewWorkspaceProps> = ({
  files,
  results,
  config,
  focusTarget,
  onClearFocusTarget,
  onPickFiles,
  onProcessCurrentFile,
  onStatusToast,
}) => {
  const isNative = bridge.isNativeHost();
  const wordFiles = useMemo(
    () => files.filter((f) => ['docx', 'docm'].includes(f.ext.toLowerCase())),
    [files],
  );

  const [selectedPath, setSelectedPath] = useState<string>('');
  const [scope, setScope] = useState<StdDocScope>('autoDetect');
  const [viewMode, setViewMode] = useState<ViewMode>('split');
  const [showDiffHighlight, setShowDiffHighlight] = useState(true);
  const [showPrintGuides, setShowPrintGuides] = useState(true);
  const [leftTab, setLeftTab] = useState<LeftTab>('pages');
  const [rightTab, setRightTab] = useState<RightTab>('controls');
  const [zoomMode, setZoomMode] = useState<ZoomMode>('fitWidth');
  const [containerWidthPx, setContainerWidthPx] = useState(760);
  const [containerHeightPx, setContainerHeightPx] = useState(640);
  const [currentPage, setCurrentPage] = useState(1);
  const [selectedElementPath, setSelectedElementPath] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const [overrides, setOverrides] = useState<PreviewOverrideConfig>(DEFAULT_OVERRIDES);
  const [enableLiveHeader, setEnableLiveHeader] = useState(false);
  const [headerDocType, setHeaderDocType] = useState<AdminDocType>('congVan');
  const [headerMode, setHeaderMode] = useState<HeaderActionMode>('standardizeExisting');
  const [includeAuditSync, setIncludeAuditSync] = useState(true);

  const [templates, setTemplates] = useState<DocumentTemplate[]>([]);
  const [orgProfiles, setOrgProfiles] = useState<OrganizationProfile[]>([]);
  const [session, setSession] = useState<PreviewSessionPayload | null>(null);
  const [rendering, setRendering] = useState(false);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [diffCursor, setDiffCursor] = useState(0);

  // Tier 2 Microsoft Word COM Ground Truth PDF
  const [wordVerifying, setWordVerifying] = useState(false);
  const [wordPdf, setWordPdf] = useState<WordComPreviewVerification | null>(null);
  const [showWordPdfPane, setShowWordPdfPane] = useState(false);

  const viewportRef = useRef<HTMLDivElement | null>(null);
  const sheetRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const baselineCacheRef = useRef<PreviewDocumentRender | null>(null);

  const activeFile = useMemo(
    () => wordFiles.find((f) => f.path === selectedPath) || wordFiles[0] || null,
    [selectedPath, wordFiles],
  );

  // Sync focusTarget from ComplianceAuditWorkspace / StandardizationWorkspace
  useEffect(() => {
    if (!focusTarget) return;
    setSelectedPath(focusTarget.filePath);
    if (focusTarget.elementPath) {
      setSelectedElementPath(focusTarget.elementPath);
      setLeftTab('audit');
      setIncludeAuditSync(true);
    }
    onClearFocusTarget?.();
  }, [focusTarget, onClearFocusTarget]);

  // Load templates & org profiles for live template/header preview dropdowns
  useEffect(() => {
    if (isNative) {
      bridge
        .invoke<TemplateCatalogResponse>('list_templates')
        .then((res) => setTemplates(res?.templates || []))
        .catch(() => {});
      bridge
        .invoke<OrganizationProfile[]>('list_org_profiles')
        .then((list) => setOrgProfiles(list || []))
        .catch(() => {});
    } else {
      setTemplates(listWebTemplates().templates);
      setOrgProfiles(listWebOrgProfiles());
    }
  }, [isNative]);

  // Auto-select first file when queue changes
  useEffect(() => {
    if (!selectedPath && wordFiles.length > 0) {
      setSelectedPath(wordFiles[0].path);
    }
  }, [selectedPath, wordFiles]);

  // Measure viewport for FitWidth & FullPage zoom modes
  useEffect(() => {
    const el = viewportRef.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerWidthPx(entry.contentRect.width);
        setContainerHeightPx(entry.contentRect.height);
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const savedOutputAfterPath = useMemo(() => {
    if (!activeFile) return null;
    const r = results[activeFile.path];
    if (r?.success && r.outputPath && !r.blobUrl) return r.outputPath;
    return null;
  }, [activeFile, results]);

  const effectiveOverrides: PreviewOverrideConfig = useMemo(() => {
    if (!enableLiveHeader) return overrides;
    return {
      ...overrides,
      enabled: true,
      applyHeaderMode: headerMode,
      headerConfig: {
        ...DEFAULT_HEADER_BUILDER_CONFIG,
        orgProfileId: overrides.orgProfileId,
        docType: headerDocType,
      },
    };
  }, [enableLiveHeader, headerDocType, headerMode, overrides]);

  const buildRequestObject = useCallback(
    (jobId: string): PreviewBuildRequest => ({
      jobId,
      path: activeFile?.path || '',
      afterFilePath: effectiveOverrides.enabled ? null : savedOutputAfterPath,
      includeAfter:
        effectiveOverrides.enabled || Boolean(savedOutputAfterPath) || viewMode !== 'before',
      includeAuditSync,
      scope,
      processConfig: config,
      overrides: effectiveOverrides,
      preferIncremental: true,
    }),
    [activeFile?.path, config, effectiveOverrides, includeAuditSync, savedOutputAfterPath, scope, viewMode],
  );

  const runPreviewRender = useCallback(
    async (preferClientFastPath = false) => {
      if (!activeFile) {
        setSession(null);
        return;
      }

      // Ultra-fast incremental patch in UI while keeping backend rebuild optional for heavy ops
      const canFastUiPatch =
        preferClientFastPath &&
        baselineCacheRef.current &&
        effectiveOverrides.enabled &&
        !effectiveOverrides.templateId &&
        !effectiveOverrides.applyHeaderMode &&
        !effectiveOverrides.applyStdGroupA &&
        !effectiveOverrides.applyVnCleaner;

      if (canFastUiPatch && baselineCacheRef.current && session) {
        const patchedAfter = applyClientIncrementalPatch(
          baselineCacheRef.current,
          effectiveOverrides,
          scope,
        );
        setSession((prev) =>
          prev
            ? {
                ...prev,
                status: 'completed',
                updateMode: 'incrementalPatch',
                after: patchedAfter,
              }
            : prev,
        );
      }

      const jobId = `pv_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`;
      setActiveJobId(jobId);
      setRendering(true);
      const req = buildRequestObject(jobId);

      try {
        const payload =
          isNative && !activeFile.webFile
            ? await bridge.invoke<PreviewSessionPayload>('build_live_preview', req)
            : await buildWebLivePreview(activeFile, req);

        if (payload.status === 'cancelled') {
          onStatusToast('info', 'Đã hủy phiên dựng xem trước A4.');
          return;
        }
        if (payload.status.startsWith('error:')) {
          onStatusToast('error', payload.status);
          return;
        }
        if (payload.before) {
          baselineCacheRef.current = payload.before;
        }
        setSession(payload);
      } catch (e) {
        onStatusToast('error', (e as Error).message);
      } finally {
        setRendering(false);
        setActiveJobId(null);
      }
    },
    [activeFile, buildRequestObject, effectiveOverrides, isNative, onStatusToast, scope, session],
  );

  // Debounced auto-rebuild whenever active file, overrides, or audit sync changes
  useEffect(() => {
    if (!activeFile) {
      setSession(null);
      return;
    }
    const timer = setTimeout(() => {
      void runPreviewRender(effectiveOverrides.enabled);
    }, effectiveOverrides.enabled ? 180 : 40);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeFile?.path, activeFile?.sizeBytes, effectiveOverrides, includeAuditSync, scope, viewMode]);

  const handleCancelRender = async () => {
    if (!activeJobId) return;
    if (isNative && !activeFile?.webFile) {
      await bridge.invoke('cancel_preview_render', { jobId: activeJobId }).catch(() => {});
    } else {
      cancelWebPreviewRender(activeJobId);
    }
    setRendering(false);
  };

  const handleVerifyWordPdf = async (variant: 'before' | 'after') => {
    if (!activeFile || !isNative || activeFile.webFile) return;
    setWordVerifying(true);
    try {
      const req = buildRequestObject(`word_${Date.now()}`);
      const ver = await bridge.invoke<WordComPreviewVerification>('verify_preview_word_pdf', {
        request: req,
        targetVariant: variant,
      });
      setWordPdf(ver);
      setShowWordPdfPane(true);
      onStatusToast('success', ver.note);
    } catch (e) {
      onStatusToast('error', (e as Error).message);
    } finally {
      setWordVerifying(false);
    }
  };

  const handleOpenInWord = async (variant: 'before' | 'after') => {
    if (!activeFile || !isNative || activeFile.webFile) return;
    try {
      const req = buildRequestObject(`open_word_${Date.now()}`);
      const res = await bridge.invoke<{ message: string }>('open_preview_in_word', {
        request: req,
        targetVariant: variant,
      });
      onStatusToast('success', res.message);
    } catch (e) {
      onStatusToast('error', (e as Error).message);
    }
  };

  const activeDocRender = useMemo(() => {
    if (viewMode === 'after' && session?.after) return session.after;
    return session?.before || session?.after || null;
  }, [session, viewMode]);

  const totalPages = Math.max(
    1,
    activeDocRender?.totalPages || 1,
    viewMode === 'split' ? session?.after?.totalPages || 1 : 1,
  );

  // Jump to focused audit marker page once markers load
  useEffect(() => {
    if (!selectedElementPath || !activeDocRender) return;
    const hitPage = activeDocRender.pages.find((pg) =>
      pg.elementPathsOnPage.some(
        (ep) => ep === selectedElementPath || selectedElementPath.startsWith(ep),
      ),
    );
    if (hitPage && hitPage.pageNumber !== currentPage) {
      setCurrentPage(hitPage.pageNumber);
    }
  }, [activeDocRender, currentPage, selectedElementPath]);

  const scrollToPage = useCallback(
    (pgNum: number) => {
      const clamped = Math.max(1, Math.min(totalPages, pgNum));
      setCurrentPage(clamped);
      const node = sheetRefs.current.get(clamped);
      if (node) {
        node.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    },
    [totalPages],
  );

  const computedZoom = useMemo(() => {
    if (typeof zoomMode === 'number') return zoomMode;
    const samplePage = activeDocRender?.pages[currentPage - 1] || activeDocRender?.pages[0];
    const wMm = samplePage?.widthMm || 210;
    const hMm = samplePage?.heightMm || 297;
    const mmToPx = 3.7795275;
    const availW = viewMode === 'split' ? Math.max(240, (containerWidthPx - 72) / 2) : Math.max(280, containerWidthPx - 48);
    const fitW = availW / (wMm * mmToPx);
    if (zoomMode === 'fitWidth') return Math.max(0.35, Math.min(1.6, fitW));
    const fitH = Math.max(240, containerHeightPx - 56) / (hMm * mmToPx);
    return Math.max(0.32, Math.min(1.6, Math.min(fitW, fitH)));
  }, [activeDocRender, containerHeightPx, containerWidthPx, currentPage, viewMode, zoomMode]);

  const diffPathSet = useMemo(() => {
    const set = new Set<string>();
    for (const d of session?.diffs || []) set.add(d.elementPath);
    return set;
  }, [session?.diffs]);

  const auditPathSet = useMemo(() => {
    const set = new Set<string>();
    for (const m of session?.auditMarkers || []) {
      if (m.elementPath) set.add(m.elementPath);
    }
    return set;
  }, [session?.auditMarkers]);

  // Search hits across active render
  const searchHits = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q || !activeDocRender) return [];
    const hits: { pageNumber: number; elementPath: string; snippet: string }[] = [];
    for (const pg of activeDocRender.pages) {
      for (const blk of pg.blocks) {
        if (blk.kind === 'paragraph') {
          if (blk.plainText.toLowerCase().includes(q)) {
            hits.push({
              pageNumber: pg.pageNumber,
              elementPath: blk.elementPath,
              snippet: blk.plainText.slice(0, 80),
            });
          }
        } else {
          for (const r of blk.rows) {
            for (const c of r.cells) {
              for (const cp of c.paragraphs) {
                if (cp.plainText.toLowerCase().includes(q)) {
                  hits.push({
                    pageNumber: pg.pageNumber,
                    elementPath: cp.elementPath,
                    snippet: cp.plainText.slice(0, 80),
                  });
                }
              }
            }
          }
        }
      }
    }
    return hits.slice(0, 120);
  }, [activeDocRender, searchQuery]);

  const selectedElementDetail = useMemo(() => {
    if (!selectedElementPath || !activeDocRender) return null;
    for (const pg of activeDocRender.pages) {
      for (const b of pg.blocks) {
        if (b.elementPath === selectedElementPath) {
          return { page: pg.pageNumber, block: b };
        }
        if (b.kind === 'table') {
          for (const r of b.rows) {
            for (const c of r.cells) {
              for (const cp of c.paragraphs) {
                if (cp.elementPath === selectedElementPath) {
                  return { page: pg.pageNumber, block: cp };
                }
              }
            }
          }
        }
      }
    }
    return null;
  }, [activeDocRender, selectedElementPath]);

  const patchOv = <K extends keyof PreviewOverrideConfig>(key: K, value: PreviewOverrideConfig[K]) => {
    setOverrides((prev) => ({
      ...prev,
      enabled: true,
      [key]: value,
    }));
  };

  const applyQuickNd30ToPreview = () => {
    setOverrides((prev) => ({
      ...prev,
      enabled: true,
      fontName: config.format.fontName,
      fontSizePt: config.format.fontSizePt,
      tableFontSizePt: config.format.tableFontSizePt,
      bodyAlignment: 'justify',
      lineSpacingMult: config.format.lineSpacing,
      spaceBeforePt: config.format.spaceBeforePt,
      spaceAfterPt: config.format.spaceAfterPt,
      firstLineIndentCm: config.format.firstLineIndentCm,
      paperSize: config.format.paperSize,
      marginTopCm: config.format.marginTopCm,
      marginBottomCm: config.format.marginBottomCm,
      marginLeftCm: config.format.marginLeftCm,
      marginRightCm: config.format.marginRightCm,
      applyStdGroupA: true,
      applyVnCleaner: true,
    }));
  };

  const resetAllOverrides = () => {
    setOverrides(DEFAULT_OVERRIDES);
    setEnableLiveHeader(false);
    setWordPdf(null);
    setShowWordPdfPane(false);
  };

  const jumpDiff = (delta: number) => {
    const list = session?.diffs || [];
    if (!list.length) return;
    const nextIdx = (diffCursor + delta + list.length) % list.length;
    setDiffCursor(nextIdx);
    const d = list[nextIdx];
    setSelectedElementPath(d.elementPath);
    scrollToPage(d.afterPage || d.beforePage || 1);
  };

  if (wordFiles.length === 0) {
    return (
      <div className="mw-panel p-8 text-center space-y-3">
        <Eye className="w-10 h-10 text-blue-500 mx-auto" />
        <h2 className="text-base font-bold">Chưa có tài liệu Word trong hàng đợi</h2>
        <p className="text-xs text-slate-500 max-w-md mx-auto">
          Mở hoặc kéo thả tệp <code>.docx</code> / <code>.docm</code> để xem trước khổ A4 sát cấu trúc OpenXML,
          so sánh Trước/Sau song song và kiểm tra bản in trước khi lưu.
        </p>
        <button type="button" onClick={onPickFiles} className="mw-button mw-button-primary">
          <FolderOpen className="w-4 h-4" /> Mở tài liệu Word
        </button>
      </div>
    );
  }

  const pageNumbersList = Array.from({ length: totalPages }, (_, i) => i + 1);
  const windowMin = Math.max(1, currentPage - 2);
  const windowMax = Math.min(totalPages, currentPage + 2);

  return (
    <div className="flex flex-col gap-3 h-full min-h-[700px]">
      {/* Fidelity Disclaimer Banner */}
      <div className="rounded-xl border border-amber-300/80 dark:border-amber-800 bg-amber-50/80 dark:bg-amber-950/35 px-3 py-2 text-[11px] text-amber-900 dark:text-amber-200 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <AlertCircle className="w-4 h-4 shrink-0 text-amber-600" />
          <span>{session?.fidelityDisclaimer || PREVIEW_FIDELITY_DISCLAIMER_VI}</span>
        </div>
        {session && (
          <span className="shrink-0 font-mono text-[10px] px-2 py-0.5 rounded bg-white/80 dark:bg-slate-900 border border-amber-200 dark:border-amber-800">
            {session.updateMode === 'incrementalPatch'
              ? 'Cập nhật tức thì'
              : session.updateMode === 'fullRebuild'
                ? 'Dựng lại trong bộ nhớ'
                : 'Khởi tạo'}{' '}
            · {session.elapsedMs}ms
          </span>
        )}
      </div>

      {/* Top Toolbar */}
      <div className="mw-panel p-2.5 flex flex-wrap items-center justify-between gap-2 text-xs">
        <div className="flex flex-wrap items-center gap-2">
          <select
            value={activeFile?.path || ''}
            onChange={(e) => {
              setSelectedPath(e.target.value);
              setCurrentPage(1);
              setSelectedElementPath(null);
              setWordPdf(null);
            }}
            className="mw-input max-w-[210px] py-1 text-xs font-semibold"
          >
            {wordFiles.map((f) => (
              <option key={f.path} value={f.path}>
                {f.name}
              </option>
            ))}
          </select>

          <select
            value={scope}
            onChange={(e) => setScope(e.target.value as StdDocScope)}
            className="mw-input max-w-[165px] py-1 text-xs"
            title="Phạm vi kiểm tra Bản in & Audit"
          >
            {SCOPE_OPTIONS.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>

          {/* Before / After / Split Switcher */}
          <div className="inline-flex rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100 dark:bg-slate-800 p-0.5">
            {(
              [
                { id: 'before', label: 'Bản gốc', icon: FileText },
                { id: 'after', label: 'Sau chuẩn hóa', icon: FileCheck2 },
                { id: 'split', label: 'Song song', icon: Columns2 },
              ] as const
            ).map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                onClick={() => setViewMode(id)}
                className={`inline-flex items-center gap-1 px-2 py-1 rounded-md text-[11px] font-semibold transition-all ${
                  viewMode === id
                    ? 'bg-white dark:bg-slate-900 text-blue-600 shadow-sm'
                    : 'text-slate-600 dark:text-slate-300'
                }`}
              >
                <Icon className="w-3.5 h-3.5" />
                {label}
              </button>
            ))}
          </div>

          <button
            type="button"
            onClick={() => setShowDiffHighlight((v) => !v)}
            className={`inline-flex items-center gap-1 px-2 py-1 rounded-lg border text-[11px] font-medium ${
              showDiffHighlight
                ? 'bg-amber-50 border-amber-300 text-amber-800 dark:bg-amber-950/50 dark:border-amber-700 dark:text-amber-300'
                : 'border-slate-200 dark:border-slate-700 text-slate-500'
            }`}
            title="Đánh dấu vị trí thay đổi"
          >
            <Highlighter className="w-3.5 h-3.5" />
            Thay đổi ({session?.diffs.length || 0})
          </button>

          {(session?.diffs.length || 0) > 0 && (
            <div className="inline-flex items-center gap-0.5">
              <button
                type="button"
                onClick={() => jumpDiff(-1)}
                className="p-1 rounded border border-slate-200 dark:border-slate-700 hover:bg-slate-100"
                title="Thay đổi trước"
              >
                <ArrowLeft className="w-3 h-3" />
              </button>
              <button
                type="button"
                onClick={() => jumpDiff(1)}
                className="p-1 rounded border border-slate-200 dark:border-slate-700 hover:bg-slate-100"
                title="Thay đổi tiếp"
              >
                <ArrowRight className="w-3 h-3" />
              </button>
            </div>
          )}

          <button
            type="button"
            onClick={() => setShowPrintGuides((v) => !v)}
            className={`inline-flex items-center gap-1 px-2 py-1 rounded-lg border text-[11px] font-medium ${
              showPrintGuides
                ? 'bg-sky-50 border-sky-300 text-sky-800 dark:bg-sky-950/50 dark:border-sky-700 dark:text-sky-300'
                : 'border-slate-200 dark:border-slate-700 text-slate-500'
            }`}
          >
            <Printer className="w-3.5 h-3.5" />
            Bản in
          </button>
        </div>

        {/* Right Toolbar: Page Nav, Zoom, Cancel, Word COM */}
        <div className="flex flex-wrap items-center gap-2">
          <div className="inline-flex items-center gap-1 border border-slate-200 dark:border-slate-700 rounded-lg px-1.5 py-0.5 bg-white dark:bg-slate-900">
            <button
              type="button"
              disabled={currentPage <= 1}
              onClick={() => scrollToPage(currentPage - 1)}
              className="p-0.5 disabled:opacity-30"
              title="Trang trước"
            >
              <ArrowLeft className="w-3.5 h-3.5" />
            </button>
            <input
              type="number"
              min={1}
              max={totalPages}
              value={currentPage}
              onChange={(e) => scrollToPage(Number(e.target.value) || 1)}
              className="w-10 text-center bg-transparent text-xs font-semibold outline-none"
            />
            <span className="text-slate-400">/ {totalPages}</span>
            <button
              type="button"
              disabled={currentPage >= totalPages}
              onClick={() => scrollToPage(currentPage + 1)}
              className="p-0.5 disabled:opacity-30"
              title="Trang sau"
            >
              <ArrowRight className="w-3.5 h-3.5" />
            </button>
          </div>

          <select
            value={String(zoomMode)}
            onChange={(e) => {
              const v = e.target.value;
              setZoomMode(v === 'fitWidth' || v === 'fullPage' ? v : Number(v));
            }}
            className="mw-input py-1 text-xs w-[112px]"
          >
            <option value="fitWidth">Vừa chiều ngang</option>
            <option value="fullPage">Xem toàn trang</option>
            <option value="0.5">50%</option>
            <option value="0.75">75%</option>
            <option value="1">100% (Chuẩn)</option>
            <option value="1.25">125%</option>
            <option value="1.5">150%</option>
            <option value="2">200%</option>
          </select>

          {rendering ? (
            <button
              type="button"
              onClick={() => void handleCancelRender()}
              className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-rose-600 text-white text-[11px] font-semibold"
            >
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
              Hủy render
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void runPreviewRender(false)}
              className="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800 text-[11px] font-medium"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              Dựng lại
            </button>
          )}

          {isNative && !activeFile?.webFile && session?.wordInstalled && (
            <>
              <button
                type="button"
                disabled={wordVerifying}
                onClick={() =>
                  void handleVerifyWordPdf(
                    viewMode === 'before' ? 'before' : 'after',
                  )
                }
                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-indigo-600 hover:bg-indigo-700 text-white text-[11px] font-semibold disabled:opacity-50"
              >
                {wordVerifying ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Monitor className="w-3.5 h-3.5" />
                )}
                Đối chiếu PDF Word
              </button>
              <button
                type="button"
                onClick={() =>
                  void handleOpenInWord(
                    viewMode === 'before' ? 'before' : 'after',
                  )
                }
                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg border border-indigo-300 dark:border-indigo-700 text-indigo-700 dark:text-indigo-300 hover:bg-indigo-50 dark:hover:bg-indigo-950/40 text-[11px] font-semibold"
              >
                <ExternalLink className="w-3.5 h-3.5" />
                Mở bằng Word
              </button>
            </>
          )}
        </div>
      </div>

      {/* Main 3-Column Studio */}
      <div className="grid grid-cols-1 xl:grid-cols-[225px_minmax(0,1fr)_275px] gap-3 flex-1 min-h-0">
        {/* Left Navigation Panel: Pages / Search / Audit Sync */}
        <aside className="mw-panel flex flex-col min-h-[420px] max-h-[76vh] overflow-hidden">
          <div className="grid grid-cols-3 border-b border-slate-200 dark:border-slate-800 text-[11px] font-semibold">
            <button
              type="button"
              onClick={() => setLeftTab('pages')}
              className={`py-2 flex items-center justify-center gap-1 border-b-2 ${
                leftTab === 'pages'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-slate-500'
              }`}
            >
              <Layers className="w-3.5 h-3.5" /> Trang
            </button>
            <button
              type="button"
              onClick={() => setLeftTab('search')}
              className={`py-2 flex items-center justify-center gap-1 border-b-2 ${
                leftTab === 'search'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-slate-500'
              }`}
            >
              <Search className="w-3.5 h-3.5" /> Tìm
            </button>
            <button
              type="button"
              onClick={() => setLeftTab('audit')}
              className={`py-2 flex items-center justify-center gap-1 border-b-2 ${
                leftTab === 'audit'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-slate-500'
              }`}
            >
              <FileSearch className="w-3.5 h-3.5" />
              Lỗi ({session?.auditMarkers.length || 0})
            </button>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-2">
            {leftTab === 'pages' &&
              (activeDocRender?.pages || []).map((pg) => {
                const diffsOnPg = (session?.diffs || []).filter(
                  (d) => d.afterPage === pg.pageNumber || d.beforePage === pg.pageNumber,
                ).length;
                const auditsOnPg = (session?.auditMarkers || []).filter(
                  (m) => m.pageNumber === pg.pageNumber,
                ).length;
                const thumbLandscape = pg.orientation === 'landscape';
                return (
                  <button
                    key={pg.pageNumber}
                    type="button"
                    onClick={() => scrollToPage(pg.pageNumber)}
                    className={`w-full p-2 rounded-xl border text-left transition-all ${
                      currentPage === pg.pageNumber
                        ? 'border-blue-500 bg-blue-50/70 dark:bg-blue-950/35'
                        : 'border-slate-200 dark:border-slate-800 hover:border-slate-300'
                    }`}
                  >
                    <div className="flex items-start gap-2">
                      <div
                        className={`bg-white border border-slate-300 shadow-xs shrink-0 flex flex-col justify-between p-1 text-[6px] text-slate-400 ${
                          thumbLandscape ? 'w-12 h-8' : 'w-8 h-11'
                        }`}
                      >
                        <div className="h-0.5 bg-slate-300 w-3/4 mx-auto" />
                        <div className="space-y-0.5">
                          <div className="h-0.5 bg-slate-200 w-full" />
                          <div className="h-0.5 bg-slate-200 w-5/6" />
                          <div className="h-0.5 bg-slate-200 w-2/3" />
                        </div>
                        <div className="text-center font-bold text-slate-500">{pg.pageNumber}</div>
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="text-xs font-bold">Trang {pg.pageNumber}</div>
                        <div className="text-[10px] text-slate-500">
                          S{pg.sectionIndex} · {pg.paperLabel}{' '}
                          {thumbLandscape ? 'Ngang' : 'Dọc'}
                        </div>
                        <div className="flex flex-wrap gap-1 mt-1">
                          {diffsOnPg > 0 && (
                            <span className="px-1 py-0.5 rounded bg-amber-100 text-amber-800 text-[9px] font-semibold">
                              {diffsOnPg} sửa
                            </span>
                          )}
                          {auditsOnPg > 0 && (
                            <span className="px-1 py-0.5 rounded bg-rose-100 text-rose-800 text-[9px] font-semibold">
                              {auditsOnPg} lỗi
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                  </button>
                );
              })}

            {leftTab === 'search' && (
              <div className="space-y-2">
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder="Nhập từ khóa tìm trong văn bản..."
                  className="mw-input w-full text-xs py-1.5"
                />
                <div className="text-[10px] text-slate-500">
                  {searchQuery.trim()
                    ? `Tìm thấy ${searchHits.length} đoạn khớp.`
                    : 'Gõ nội dung để nhảy trang và tô sáng trực tiếp.'}
                </div>
                {searchHits.map((hit, idx) => (
                  <button
                    key={`${hit.elementPath}_${idx}`}
                    type="button"
                    onClick={() => {
                      setSelectedElementPath(hit.elementPath);
                      scrollToPage(hit.pageNumber);
                    }}
                    className="w-full p-2 rounded-lg border border-slate-200 dark:border-slate-800 hover:bg-slate-50 dark:hover:bg-slate-800 text-left"
                  >
                    <div className="text-[10px] font-bold text-blue-600">
                      Trang {hit.pageNumber} · {hit.elementPath}
                    </div>
                    <div className="text-[11px] text-slate-700 dark:text-slate-300 line-clamp-2">
                      {hit.snippet}
                    </div>
                  </button>
                ))}
              </div>
            )}

            {leftTab === 'audit' && (
              <div className="space-y-2">
                <label className="flex items-center gap-1.5 text-[11px] font-medium px-1">
                  <input
                    type="checkbox"
                    checked={includeAuditSync}
                    onChange={(e) => setIncludeAuditSync(e.target.checked)}
                  />
                  Đồng bộ lỗi từ Audit Report
                </label>
                {(session?.auditMarkers || []).length === 0 ? (
                  <div className="text-xs text-slate-500 p-2 text-center">
                    Không có lỗi / cảnh báo Audit gắn vị trí.
                  </div>
                ) : (
                  (session?.auditMarkers || []).map((m: AuditSyncMarker) => (
                    <button
                      key={m.markerId}
                      type="button"
                      onClick={() => {
                        setSelectedElementPath(m.elementPath);
                        scrollToPage(m.pageNumber);
                      }}
                      className={`w-full p-2 rounded-lg border text-left space-y-0.5 ${
                        selectedElementPath === m.elementPath
                          ? 'border-blue-500 bg-blue-50/70 dark:bg-blue-950/35'
                          : 'border-slate-200 dark:border-slate-800 hover:border-rose-300'
                      }`}
                    >
                      <div className="flex items-center justify-between gap-1">
                        <span className="text-[10px] font-bold text-rose-700 dark:text-rose-300">
                          {m.ruleOrIssueId}
                        </span>
                        <span className="text-[9px] font-semibold px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800">
                          Trang {m.pageNumber}
                        </span>
                      </div>
                      <div className="text-[11px] font-semibold line-clamp-1">{m.title}</div>
                      <div className="text-[10px] text-slate-500 line-clamp-2">{m.remediation}</div>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
        </aside>

        {/* Center Document Sheet Viewport (Windowed Virtualization) */}
        <section
          ref={viewportRef}
          className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-slate-200/75 dark:bg-slate-950/85 overflow-auto p-4 space-y-6 max-h-[76vh] relative"
        >
          {showWordPdfPane && wordPdf?.pdfUrlPath && (
            <div className="rounded-xl border border-indigo-300 dark:border-indigo-700 bg-white dark:bg-slate-900 p-2 space-y-2">
              <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                <div className="font-bold text-indigo-700 dark:text-indigo-300 flex items-center gap-1.5">
                  <Monitor className="w-4 h-4" />
                  Bản PDF kết xuất trực tiếp bởi Microsoft Word ({wordPdf.wordPageCount} trang · {wordPdf.elapsedMs}ms)
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[11px] text-slate-500">{wordPdf.note}</span>
                  <button
                    type="button"
                    onClick={() => setShowWordPdfPane(false)}
                    className="p-1 rounded hover:bg-slate-100 dark:hover:bg-slate-800"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
              <iframe
                title="Word Ground Truth PDF"
                src={wordPdf.pdfUrlPath}
                className="w-full h-[520px] rounded-lg border border-slate-200 dark:border-slate-700 bg-white"
              />
            </div>
          )}

          {viewMode === 'split' ? (
            <div className="space-y-6">
              {pageNumbersList.map((pgNum) => {
                const bPage = session?.before?.pages[pgNum - 1];
                const aPage = (session?.after || session?.before)?.pages[pgNum - 1];
                const inWindow = pgNum >= windowMin && pgNum <= windowMax;
                return (
                  <div
                    key={pgNum}
                    ref={(el) => {
                      if (el) sheetRefs.current.set(pgNum, el);
                      else sheetRefs.current.delete(pgNum);
                    }}
                    className="grid grid-cols-1 lg:grid-cols-2 gap-4 items-start justify-items-center"
                  >
                    <div className="space-y-1">
                      <div className="text-[10px] font-bold uppercase tracking-wider text-slate-600 dark:text-slate-400 text-center">
                        Trước chuẩn hóa (Bản gốc)
                      </div>
                      {bPage ? (
                        <A4PhysicalSheet
                          page={bPage}
                          zoom={computedZoom}
                          isVirtualActive={inWindow}
                          isActivePage={currentPage === pgNum}
                          showPrintGuides={showPrintGuides}
                          showDiffHighlight={showDiffHighlight}
                          fileItem={activeFile || undefined}
                          isNative={isNative}
                          selectedPath={selectedElementPath}
                          diffPathSet={diffPathSet}
                          auditPathSet={auditPathSet}
                          searchQuery={searchQuery}
                          onActivatePage={setCurrentPage}
                          onSelectElement={(ep, p) => {
                            setSelectedElementPath(ep);
                            setCurrentPage(p);
                          }}
                        />
                      ) : (
                        <div className="p-6 text-xs text-slate-400 italic">Trang không tồn tại ở bản gốc</div>
                      )}
                    </div>

                    <div className="space-y-1">
                      <div className="text-[10px] font-bold uppercase tracking-wider text-blue-700 dark:text-blue-400 text-center">
                        Sau chuẩn hóa / Chỉnh trực tiếp
                      </div>
                      {aPage ? (
                        <A4PhysicalSheet
                          page={aPage}
                          zoom={computedZoom}
                          isVirtualActive={inWindow}
                          isActivePage={currentPage === pgNum}
                          showPrintGuides={showPrintGuides}
                          showDiffHighlight={showDiffHighlight}
                          fileItem={activeFile || undefined}
                          isNative={isNative}
                          selectedPath={selectedElementPath}
                          diffPathSet={diffPathSet}
                          auditPathSet={auditPathSet}
                          searchQuery={searchQuery}
                          onActivatePage={setCurrentPage}
                          onSelectElement={(ep, p) => {
                            setSelectedElementPath(ep);
                            setCurrentPage(p);
                          }}
                        />
                      ) : (
                        <div className="p-6 text-xs text-slate-400 italic">Trang kết thúc ở bản sau</div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="flex flex-col items-center space-y-6">
              {(activeDocRender?.pages || []).map((pg) => {
                const inWindow = pg.pageNumber >= windowMin && pg.pageNumber <= windowMax;
                return (
                  <div
                    key={pg.pageNumber}
                    ref={(el) => {
                      if (el) sheetRefs.current.set(pg.pageNumber, el);
                      else sheetRefs.current.delete(pg.pageNumber);
                    }}
                  >
                    <A4PhysicalSheet
                      page={pg}
                      zoom={computedZoom}
                      isVirtualActive={inWindow}
                      isActivePage={currentPage === pg.pageNumber}
                      showPrintGuides={showPrintGuides}
                      showDiffHighlight={showDiffHighlight}
                      fileItem={activeFile || undefined}
                      isNative={isNative}
                      selectedPath={selectedElementPath}
                      diffPathSet={diffPathSet}
                      auditPathSet={auditPathSet}
                      searchQuery={searchQuery}
                      onActivatePage={setCurrentPage}
                      onSelectElement={(ep, p) => {
                        setSelectedElementPath(ep);
                        setCurrentPage(p);
                      }}
                    />
                  </div>
                );
              })}
            </div>
          )}
        </section>

        {/* Right Panel: Live Controls & Print Readiness Inspector */}
        <aside className="mw-panel flex flex-col min-h-[420px] max-h-[76vh] overflow-hidden">
          <div className="grid grid-cols-2 border-b border-slate-200 dark:border-slate-800 text-[11px] font-semibold">
            <button
              type="button"
              onClick={() => setRightTab('controls')}
              className={`py-2 flex items-center justify-center gap-1 border-b-2 ${
                rightTab === 'controls'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-slate-500'
              }`}
            >
              <Sliders className="w-3.5 h-3.5" /> Chỉnh trực tiếp
            </button>
            <button
              type="button"
              onClick={() => setRightTab('print')}
              className={`py-2 flex items-center justify-center gap-1 border-b-2 ${
                rightTab === 'print'
                  ? 'border-blue-600 text-blue-600'
                  : 'border-transparent text-slate-500'
              }`}
            >
              <Printer className="w-3.5 h-3.5" /> Bản in &amp; Chi tiết
            </button>
          </div>

          <div className="flex-1 overflow-y-auto p-3 space-y-3 text-xs">
            {rightTab === 'controls' && (
              <>
                <div className="flex items-center justify-between gap-2">
                  <button
                    type="button"
                    onClick={applyQuickNd30ToPreview}
                    className="flex-1 py-1.5 px-2 rounded-lg bg-blue-600 hover:bg-blue-700 text-white font-semibold text-[11px]"
                  >
                    Xem thử NĐ 30/2020
                  </button>
                  <button
                    type="button"
                    onClick={resetAllOverrides}
                    className="p-1.5 rounded-lg border border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800"
                    title="Đặt lại thông số xem thử"
                  >
                    <RotateCcw className="w-3.5 h-3.5" />
                  </button>
                </div>

                <p className="text-[10px] text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-950/35 px-2 py-1 rounded-lg">
                  Mọi chỉnh sửa tại đây chỉ chạy tạm trong bộ nhớ — <b>tệp gốc trên ổ đĩa giữ nguyên 100%</b>.
                </p>

                <div className="space-y-2">
                  <div>
                    <label className="text-[11px] font-semibold">Phông chữ</label>
                    <select
                      value={overrides.fontName || ''}
                      onChange={(e) => patchOv('fontName', e.target.value || null)}
                      className="mw-input w-full mt-0.5 text-xs"
                    >
                      <option value="">Giữ nguyên theo tài liệu</option>
                      <option value="Times New Roman">Times New Roman</option>
                      <option value="Arial">Arial</option>
                      <option value="Calibri">Calibri</option>
                    </select>
                  </div>

                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="text-[11px] font-semibold">Cỡ chữ (pt)</label>
                      <input
                        type="number"
                        step={0.5}
                        min={8}
                        max={28}
                        value={overrides.fontSizePt ?? ''}
                        placeholder="14"
                        onChange={(e) =>
                          patchOv('fontSizePt', e.target.value ? Number(e.target.value) : null)
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      />
                    </div>
                    <div>
                      <label className="text-[11px] font-semibold">Cỡ bảng (pt)</label>
                      <input
                        type="number"
                        step={0.5}
                        min={8}
                        max={20}
                        value={overrides.tableFontSizePt ?? ''}
                        placeholder="13"
                        onChange={(e) =>
                          patchOv(
                            'tableFontSizePt',
                            e.target.value ? Number(e.target.value) : null,
                          )
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      />
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="text-[11px] font-semibold">Căn chỉnh lời</label>
                      <select
                        value={overrides.bodyAlignment || ''}
                        onChange={(e) =>
                          patchOv(
                            'bodyAlignment',
                            (e.target.value || null) as PreviewBodyAlignment | null,
                          )
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      >
                        <option value="">Giữ nguyên</option>
                        <option value="justify">Căn đều (Justify)</option>
                        <option value="left">Căn trái</option>
                        <option value="center">Căn giữa</option>
                        <option value="right">Căn phải</option>
                      </select>
                    </div>
                    <div>
                      <label className="text-[11px] font-semibold">Giãn dòng (x)</label>
                      <input
                        type="number"
                        step={0.05}
                        min={1}
                        max={2.5}
                        value={overrides.lineSpacingMult ?? ''}
                        placeholder="1.15"
                        onChange={(e) =>
                          patchOv(
                            'lineSpacingMult',
                            e.target.value ? Number(e.target.value) : null,
                          )
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      />
                    </div>
                  </div>

                  <div className="grid grid-cols-3 gap-1.5">
                    <div>
                      <label className="text-[10px] font-semibold">Trước (pt)</label>
                      <input
                        type="number"
                        step={1}
                        min={0}
                        max={36}
                        value={overrides.spaceBeforePt ?? ''}
                        placeholder="0"
                        onChange={(e) =>
                          patchOv(
                            'spaceBeforePt',
                            e.target.value ? Number(e.target.value) : null,
                          )
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      />
                    </div>
                    <div>
                      <label className="text-[10px] font-semibold">Sau (pt)</label>
                      <input
                        type="number"
                        step={1}
                        min={0}
                        max={36}
                        value={overrides.spaceAfterPt ?? ''}
                        placeholder="6"
                        onChange={(e) =>
                          patchOv(
                            'spaceAfterPt',
                            e.target.value ? Number(e.target.value) : null,
                          )
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      />
                    </div>
                    <div>
                      <label className="text-[10px] font-semibold">Thụt đầu (cm)</label>
                      <input
                        type="number"
                        step={0.05}
                        min={0}
                        max={3}
                        value={overrides.firstLineIndentCm ?? ''}
                        placeholder="1.27"
                        onChange={(e) =>
                          patchOv(
                            'firstLineIndentCm',
                            e.target.value ? Number(e.target.value) : null,
                          )
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      />
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-2 pt-1 border-t border-slate-200 dark:border-slate-800">
                    <div>
                      <label className="text-[11px] font-semibold">Khổ giấy</label>
                      <select
                        value={overrides.paperSize || ''}
                        onChange={(e) =>
                          patchOv('paperSize', (e.target.value || null) as PaperSize | null)
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      >
                        <option value="">Giữ nguyên</option>
                        <option value="a4">A4 (21×29.7cm)</option>
                        <option value="a3">A3 (29.7×42cm)</option>
                        <option value="letter">US Letter</option>
                      </select>
                    </div>
                    <div>
                      <label className="text-[11px] font-semibold">Hướng giấy</label>
                      <select
                        value={overrides.orientationOverride || ''}
                        onChange={(e) =>
                          patchOv(
                            'orientationOverride',
                            (e.target.value || null) as PreviewOrientationOverride | null,
                          )
                        }
                        className="mw-input w-full mt-0.5 text-xs"
                      >
                        <option value="">Giữ từng Section</option>
                        <option value="allPortrait">Tất cả Dọc</option>
                        <option value="allLandscape">Tất cả Ngang</option>
                      </select>
                    </div>
                  </div>

                  <div className="grid grid-cols-4 gap-1">
                    {(
                      [
                        ['marginTopCm', 'Trên'],
                        ['marginBottomCm', 'Dưới'],
                        ['marginLeftCm', 'Trái'],
                        ['marginRightCm', 'Phải'],
                      ] as const
                    ).map(([k, lbl]) => (
                      <div key={k}>
                        <label className="text-[10px] font-semibold">{lbl} (cm)</label>
                        <input
                          type="number"
                          step={0.1}
                          min={0.2}
                          max={6}
                          value={overrides[k] ?? ''}
                          placeholder="-"
                          onChange={(e) =>
                            patchOv(k, e.target.value ? Number(e.target.value) : null)
                          }
                          className="mw-input w-full mt-0.5 text-xs px-1"
                        />
                      </div>
                    ))}
                  </div>

                  <label className="flex items-center gap-1.5 text-[11px]">
                    <input
                      type="checkbox"
                      checked={overrides.autoSwapBindingMargin}
                      onChange={(e) => patchOv('autoSwapBindingMargin', e.target.checked)}
                    />
                    Tự đảo lề gáy khi gặp trang Ngang
                  </label>
                </div>

                {/* Live Template & Header & Cleaner triggers */}
                <div className="pt-2 border-t border-slate-200 dark:border-slate-800 space-y-2">
                  <div>
                    <label className="text-[11px] font-semibold">Áp thử Mẫu văn bản (Phase 4)</label>
                    <select
                      value={overrides.templateId || ''}
                      onChange={(e) => patchOv('templateId', e.target.value || null)}
                      className="mw-input w-full mt-0.5 text-xs"
                    >
                      <option value="">Không áp mẫu</option>
                      {templates.map((t) => (
                        <option key={t.templateId} value={t.templateId}>
                          {t.name}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div>
                    <label className="text-[11px] font-semibold">Hồ sơ Tổ chức (Phase 1)</label>
                    <select
                      value={overrides.orgProfileId || ''}
                      onChange={(e) => patchOv('orgProfileId', e.target.value || null)}
                      className="mw-input w-full mt-0.5 text-xs"
                    >
                      <option value="">Mặc định</option>
                      {orgProfiles.map((o) => (
                        <option key={o.id} value={o.id}>
                          {o.profileName} ({o.agencyAbbreviation})
                        </option>
                      ))}
                    </select>
                  </div>

                  <label className="flex items-center gap-1.5 text-[11px] font-semibold">
                    <input
                      type="checkbox"
                      checked={enableLiveHeader}
                      onChange={(e) => setEnableLiveHeader(e.target.checked)}
                    />
                    Áp thử Phần đầu văn bản (Phase 2)
                  </label>
                  {enableLiveHeader && (
                    <div className="grid grid-cols-2 gap-1.5 pl-4">
                      <select
                        value={headerDocType}
                        onChange={(e) => setHeaderDocType(e.target.value as AdminDocType)}
                        className="mw-input text-[11px] py-1"
                      >
                        <option value="congVan">Công văn</option>
                        <option value="quyetDinh">Quyết định</option>
                        <option value="toTrinh">Tờ trình</option>
                        <option value="baoCao">Báo cáo</option>
                        <option value="thongBao">Thông báo</option>
                      </select>
                      <select
                        value={headerMode}
                        onChange={(e) => setHeaderMode(e.target.value as HeaderActionMode)}
                        className="mw-input text-[11px] py-1"
                      >
                        <option value="standardizeExisting">Chuẩn hóa có sẵn</option>
                        <option value="createNew">Chèn phần đầu mới</option>
                      </select>
                    </div>
                  )}

                  <div className="flex flex-wrap gap-3 text-[11px]">
                    <label className="flex items-center gap-1">
                      <input
                        type="checkbox"
                        checked={overrides.applyStdGroupA}
                        onChange={(e) => patchOv('applyStdGroupA', e.target.checked)}
                      />
                      Auto-Fix Nhóm A
                    </label>
                    <label className="flex items-center gap-1">
                      <input
                        type="checkbox"
                        checked={overrides.applyVnCleaner}
                        onChange={(e) => patchOv('applyVnCleaner', e.target.checked)}
                      />
                      Làm sạch TV
                    </label>
                  </div>
                </div>

                {onProcessCurrentFile && (
                  <button
                    type="button"
                    onClick={() => onProcessCurrentFile(activeFile?.path, effectiveOverrides)}
                    className="w-full mw-button mw-button-primary justify-center mt-2"
                  >
                    <Play className="w-3.5 h-3.5 fill-current" />
                    Lưu kết quả chuẩn hóa thực tế
                  </button>
                )}
              </>
            )}

            {rightTab === 'print' && (
              <>
                {selectedElementDetail && (
                  <div className="p-2.5 rounded-xl border border-blue-200 dark:border-blue-800 bg-blue-50/50 dark:bg-blue-950/30 space-y-1">
                    <div className="flex items-center justify-between">
                      <span className="font-bold text-blue-700 dark:text-blue-300 text-[11px]">
                        Phần tử đang chọn (Trang {selectedElementDetail.page})
                      </span>
                      <button
                        type="button"
                        onClick={() => setSelectedElementPath(null)}
                        className="text-[10px] text-slate-500 hover:text-slate-700"
                      >
                        Bỏ chọn
                      </button>
                    </div>
                    <div className="font-mono text-[10px] text-slate-600 dark:text-slate-400 break-all">
                      {selectedElementDetail.block.elementPath}
                    </div>
                    {'spans' in selectedElementDetail.block ? (
                      <div className="text-[11px] text-slate-700 dark:text-slate-300">
                        <div>
                          Vai trò: <b>{selectedElementDetail.block.role}</b> · Căn:{' '}
                          <b>{selectedElementDetail.block.alignment}</b>
                        </div>
                        <div>
                          Font:{' '}
                          <b>
                            {selectedElementDetail.block.spans[0]?.fontFamily || 'Normal'}{' '}
                            {selectedElementDetail.block.spans[0]?.fontSizePt || 14}pt
                          </b>{' '}
                          · Dãn dòng <b>{selectedElementDetail.block.lineSpacingMult}x</b>
                        </div>
                      </div>
                    ) : (
                      <div className="text-[11px] text-slate-700 dark:text-slate-300">
                        Bảng #{selectedElementDetail.block.tableIndex + 1} · Rộng{' '}
                        <b>{selectedElementDetail.block.totalWidthMm}mm</b>
                      </div>
                    )}
                  </div>
                )}

                {activeDocRender?.printCheck && (
                  <div className="space-y-2">
                    <div
                      className={`p-2.5 rounded-xl border text-[11px] flex items-center gap-2 font-semibold ${
                        activeDocRender.printCheck.isPrintReady
                          ? 'bg-emerald-50 border-emerald-200 text-emerald-800 dark:bg-emerald-950/40 dark:border-emerald-800 dark:text-emerald-300'
                          : 'bg-rose-50 border-rose-200 text-rose-800 dark:bg-rose-950/40 dark:border-rose-800 dark:text-rose-300'
                      }`}
                    >
                      {activeDocRender.printCheck.isPrintReady ? (
                        <CheckCircle2 className="w-4 h-4 shrink-0" />
                      ) : (
                        <AlertTriangle className="w-4 h-4 shrink-0" />
                      )}
                      {activeDocRender.printCheck.isPrintReady
                        ? 'Sẵn sàng in ấn (không phát hiện cắt/tràn lề)'
                        : `Cảnh báo bản in (${activeDocRender.printCheck.clippedCount} cắt mép · ${activeDocRender.printCheck.overflowTableCount} tràn bảng)`}
                    </div>

                    <div className="space-y-1 text-[11px] text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-900/70 p-2 rounded-xl border border-slate-200 dark:border-slate-800">
                      <div><b>Khổ giấy:</b> {activeDocRender.printCheck.paperSummary}</div>
                      <div><b>Chiều giấy:</b> {activeDocRender.printCheck.orientationSummary}</div>
                      <div><b>Lề in:</b> {activeDocRender.printCheck.marginSummary}</div>
                      <div><b>Đầu/Chân trang:</b> {activeDocRender.printCheck.headerFooterSummary}</div>
                      <div><b>Số trang:</b> {activeDocRender.printCheck.pageNumberSummary}</div>
                    </div>

                    <div className="space-y-1.5">
                      <div className="font-bold text-[11px]">
                        Chi tiết chẩn đoán bản in ({activeDocRender.printCheck.diagnostics.length})
                      </div>
                      {activeDocRender.printCheck.diagnostics.map((d) => (
                        <button
                          key={d.itemId}
                          type="button"
                          onClick={() => {
                            setSelectedElementPath(d.elementPath);
                            scrollToPage(d.pageNumber);
                          }}
                          className="w-full p-2 rounded-lg border border-slate-200 dark:border-slate-800 text-left hover:border-amber-400"
                        >
                          <div className="flex items-center justify-between">
                            <span className="font-bold text-[10px] text-amber-700 dark:text-amber-300">
                              {d.kind}
                            </span>
                            <span className="text-[9px] text-slate-500">Trang {d.pageNumber}</span>
                          </div>
                          <div className="text-[11px] font-semibold">{d.title}</div>
                          <div className="text-[10px] text-slate-500">{d.detail}</div>
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                {(session?.diffs || []).length > 0 && (
                  <div className="space-y-1.5 pt-2 border-t border-slate-200 dark:border-slate-800">
                    <div className="font-bold text-[11px] flex items-center gap-1">
                      <BookOpen className="w-3.5 h-3.5 text-amber-600" />
                      Vị trí thay đổi ({session?.diffs.length})
                    </div>
                    {(session?.diffs || []).slice(0, 60).map((d) => (
                      <button
                        key={d.diffId}
                        type="button"
                        onClick={() => {
                          setSelectedElementPath(d.elementPath);
                          scrollToPage(d.afterPage || d.beforePage || 1);
                        }}
                        className="w-full p-1.5 rounded-lg border border-slate-200 dark:border-slate-800 text-left hover:bg-amber-50/40"
                      >
                        <div className="text-[10px] font-bold text-amber-800 dark:text-amber-300">
                          {d.summaryVi}
                        </div>
                        <div className="text-[10px] text-slate-500 line-clamp-1">
                          Trước: {d.beforeSummary}
                        </div>
                        <div className="text-[10px] text-blue-600 line-clamp-1">
                          Sau: {d.afterSummary}
                        </div>
                      </button>
                    ))}
                  </div>
                )}
              </>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
};
