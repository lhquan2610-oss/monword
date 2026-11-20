import React, { useState } from 'react';
import { Eye, RotateCw, ShieldCheck } from 'lucide-react';
import { ProcessConfig } from '../types';

interface Props {
  config: ProcessConfig;
  fileCount?: number;
  onOpenFullPreview?: () => void;
}

export const A4LivePreview: React.FC<Props> = ({ config, fileCount, onOpenFullPreview }) => {
  const { format, pageToc } = config;
  const [previewLandscapePage, setPreviewLandscapePage] = useState(false);

  const isLandscape =
    pageToc.orientationMode === 'allLandscape' ||
    (pageToc.orientationMode === 'rotateRanges' && previewLandscapePage);

  // Swap binding margin visually when landscape + autoSwapBindingMargin
  const mTop =
    isLandscape && pageToc.autoSwapBindingMargin
      ? format.marginLeftCm
      : format.marginTopCm;
  const mBottom =
    isLandscape && pageToc.autoSwapBindingMargin
      ? format.marginRightCm
      : format.marginBottomCm;
  const mLeft =
    isLandscape && pageToc.autoSwapBindingMargin
      ? format.marginBottomCm
      : format.marginLeftCm;
  const mRight =
    isLandscape && pageToc.autoSwapBindingMargin
      ? format.marginTopCm
      : format.marginRightCm;

  const pageWidthCm = isLandscape ? 29.7 : 21.0;
  const pageHeightCm = isLandscape ? 21.0 : 29.7;

  const padTopPct = `${((mTop / pageHeightCm) * 100).toFixed(2)}%`;
  const padBottomPct = `${((mBottom / pageHeightCm) * 100).toFixed(2)}%`;
  const padLeftPct = `${((mLeft / pageWidthCm) * 100).toFixed(2)}%`;
  const padRightPct = `${((mRight / pageWidthCm) * 100).toFixed(2)}%`;

  const showPageNumTop =
    pageToc.pageNumbersEnabled &&
    (pageToc.pageNumPosition === 'topCenter' || pageToc.pageNumPosition === 'topRight');
  const showPageNumBottom =
    pageToc.pageNumbersEnabled &&
    (pageToc.pageNumPosition === 'bottomCenter' || pageToc.pageNumPosition === 'bottomRight');
  const pageNumAlign =
    pageToc.pageNumPosition === 'topCenter' || pageToc.pageNumPosition === 'bottomCenter'
      ? 'text-center'
      : 'text-right';

  const scaledBodyPx = Math.max(6.5, Math.min(10.5, format.fontSizePt * 0.56));
  const scaledTablePx = Math.max(6, Math.min(9.5, format.tableFontSizePt * 0.54));
  const indentPx = Math.max(0, Math.min(24, format.firstLineIndentCm * 10));

  return (
    <div className="rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-3 shadow-sm">
      <div className="flex items-center justify-between mb-2.5">
        <div>
          <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
            Mô phỏng trang {format.paperSize.toUpperCase()}
          </h3>
          <p className="text-[11px] text-slate-500 dark:text-slate-400">
            {isLandscape ? 'Khổ Ngang (29.7 × 21 cm)' : 'Khổ Dọc (21 × 29.7 cm)'} ·{' '}
            {pageToc.skipFirstPage ? 'Trang 2+ (bỏ số trang 1)' : 'Trang 1'}
          </p>
        </div>

        {pageToc.orientationMode === 'rotateRanges' && (
          <button
            type="button"
            onClick={() => setPreviewLandscapePage((v) => !v)}
            className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium bg-amber-50 text-amber-700 border border-amber-200 hover:bg-amber-100 dark:bg-amber-950/50 dark:text-amber-300 dark:border-amber-800"
            title="Chuyển xem trang dọc / trang phụ lục xoay ngang"
          >
            <RotateCw className="w-3 h-3" />
            {previewLandscapePage ? 'Xem trang dọc' : 'Xem trang ngang'}
          </button>
        )}
      </div>

      {/* Sheet Container */}
      <div className="flex justify-center py-1 bg-slate-100/80 dark:bg-slate-950/70 rounded-lg border border-dashed border-slate-200 dark:border-slate-800">
        <div
          className={`relative bg-white text-slate-900 shadow-md border border-slate-300 transition-all duration-300 overflow-hidden ${
            isLandscape ? 'w-[248px] aspect-[297/210]' : 'w-[186px] aspect-[210/297]'
          }`}
          style={{
            fontFamily: `"${format.fontName}", "Times New Roman", serif`,
          }}
        >
          {/* Margin Guides */}
          <div
            className="absolute inset-0 pointer-events-none"
            style={{
              paddingTop: padTopPct,
              paddingBottom: padBottomPct,
              paddingLeft: padLeftPct,
              paddingRight: padRightPct,
            }}
          >
            <div className="w-full h-full border border-dashed border-sky-300/80 relative flex flex-col justify-between p-1">
              {/* Top Header / Page Number */}
              <div className="min-h-[10px]">
                {pageToc.headerText.trim() && (
                  <div className="text-[5.5px] text-slate-400 truncate">
                    {pageToc.headerText}
                  </div>
                )}
                {showPageNumTop && (
                  <div
                    className={`font-semibold text-slate-700 leading-none ${pageNumAlign}`}
                    style={{ fontSize: `${Math.max(6, pageToc.pageNumFontSizePt * 0.5)}px` }}
                  >
                    {pageToc.skipFirstPage ? pageToc.startAt + 1 : pageToc.startAt}
                  </div>
                )}
              </div>

              {/* Document Content Sample */}
              <div className="flex-1 flex flex-col justify-start gap-1 overflow-hidden mt-0.5">
                {!isLandscape && (
                  <div className="relative text-center leading-tight py-0.5 rounded bg-emerald-50/60 border border-emerald-200/70">
                    {format.protectAdminBlocks && (
                      <ShieldCheck
                        className="w-2.5 h-2.5 text-emerald-600 absolute top-0.5 right-0.5"
                        aria-label="Khối hành chính được bảo vệ"
                      />
                    )}
                    <div className="font-bold text-[5.5px] tracking-tight">
                      CỘNG HÒA XÃ HỘI CHỦ NGHĨA VIỆT NAM
                    </div>
                    <div className="font-bold text-[5.5px] underline decoration-slate-500 underline-offset-1">
                      Độc lập - Tự do - Hạnh phúc
                    </div>
                  </div>
                )}

                {pageToc.insertToc && (
                  <div className="border border-indigo-200 bg-indigo-50/50 rounded px-1 py-0.5 text-[5px] text-indigo-800">
                    <div className="font-bold text-center">MỤC LỤC (Cấp 1–{pageToc.tocLevels})</div>
                    <div className="flex justify-between">
                      <span>CHƯƠNG I ............</span>
                      <span>2</span>
                    </div>
                  </div>
                )}

                <p
                  className="text-justify text-slate-800"
                  style={{
                    fontSize: `${scaledBodyPx}px`,
                    lineHeight: format.lineSpacing,
                    textIndent: `${indentPx}px`,
                    marginBottom: `${Math.min(6, format.spaceAfterPt * 0.4)}px`,
                  }}
                >
                  Nội dung văn bản chuẩn hóa theo quy chuẩn {format.fontName} {format.fontSizePt}pt,
                  giãn dòng {format.lineSpacing} và thụt đầu dòng {format.firstLineIndentCm} cm.
                </p>

                {/* Sample Table */}
                <div className="border border-slate-400 rounded-[1px] overflow-hidden mt-0.5">
                  <div
                    className={`grid grid-cols-3 font-bold border-b border-slate-400 px-1 py-0.5 ${
                      format.repeatTableHeaders ? 'bg-sky-50 text-sky-900' : 'bg-slate-100'
                    }`}
                    style={{ fontSize: `${scaledTablePx}px` }}
                  >
                    <span>STT</span>
                    <span>Nội dung</span>
                    <span>Ghi chú</span>
                  </div>
                  <div
                    className="grid grid-cols-3 px-1 py-0.5 text-slate-700"
                    style={{ fontSize: `${scaledTablePx}px` }}
                  >
                    <span>1</span>
                    <span>Bảng {format.tableFontSizePt}pt</span>
                    <span>AutoFit</span>
                  </div>
                </div>
              </div>

              {/* Bottom Footer / Page Number */}
              <div className="min-h-[10px]">
                {showPageNumBottom && (
                  <div
                    className={`font-semibold text-slate-700 leading-none ${pageNumAlign}`}
                    style={{ fontSize: `${Math.max(6, pageToc.pageNumFontSizePt * 0.5)}px` }}
                  >
                    {pageToc.skipFirstPage ? pageToc.startAt + 1 : pageToc.startAt}
                  </div>
                )}
                {pageToc.footerText.trim() && (
                  <div className="text-[5.5px] text-slate-400 truncate">
                    {pageToc.footerText}
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Edge Margin Badges */}
          <span
            className={`absolute top-0.5 left-1/2 -translate-x-1/2 px-1 rounded text-[8px] font-mono ${
              isLandscape && pageToc.autoSwapBindingMargin
                ? 'bg-amber-100 text-amber-800 font-bold'
                : 'bg-slate-100/90 text-slate-600'
            }`}
          >
            {mTop}cm{isLandscape && pageToc.autoSwapBindingMargin ? ' (gáy)' : ''}
          </span>
          <span className="absolute bottom-0.5 left-1/2 -translate-x-1/2 px-1 rounded text-[8px] font-mono bg-slate-100/90 text-slate-600">
            {mBottom}cm
          </span>
          <span className="absolute left-0.5 top-1/2 -translate-y-1/2 px-0.5 rounded text-[8px] font-mono bg-sky-100/90 text-sky-800 font-semibold">
            {mLeft}
          </span>
          <span className="absolute right-0.5 top-1/2 -translate-y-1/2 px-0.5 rounded text-[8px] font-mono bg-slate-100/90 text-slate-600">
            {mRight}
          </span>
        </div>
      </div>

      {/* Legend */}
      <div className="mt-2 flex flex-wrap gap-1.5 text-[10px] text-slate-600 dark:text-slate-400">
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300">
          <ShieldCheck className="w-3 h-3" />
          {format.protectAdminBlocks ? 'Bảo vệ Quốc hiệu' : 'Không bảo vệ Quốc hiệu'}
        </span>
        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-sky-50 dark:bg-sky-950/40 text-sky-700 dark:text-sky-300">
          {format.smartTables ? `Bảng ${format.tableFontSizePt}pt` : 'Bảng nguyên trạng'}
        </span>
      </div>

      {onOpenFullPreview && (
        <button
          type="button"
          onClick={onOpenFullPreview}
          className="mt-2.5 w-full inline-flex items-center justify-center gap-1.5 rounded-lg bg-blue-50 hover:bg-blue-100 dark:bg-blue-950/40 dark:hover:bg-blue-900/50 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800 px-2.5 py-1.5 text-[11px] font-semibold transition-colors"
        >
          <Eye className="w-3.5 h-3.5" />
          Mở Xem trước A4 đầy đủ{fileCount ? ` (${fileCount} tệp)` : ''}
        </button>
      )}
    </div>
  );
};
