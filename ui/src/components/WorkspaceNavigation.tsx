import {
  BookOpen,
  Building2,
  ChevronRight,
  Eye,
  FileCheck2,
  History,
  Home,
  LayoutTemplate,
  ScanSearch,
  SearchCheck,
  Shield,
  SlidersHorizontal,
  Sparkles,
  Type,
  Wrench,
  Zap,
  type LucideIcon,
} from 'lucide-react';
import type { NavTab } from '../types';

export interface PrimaryNavItem {
  id: NavTab;
  label: string;
  icon: LucideIcon;
}

export interface AdvancedNavItem {
  id: NavTab;
  label: string;
  description: string;
  icon: LucideIcon;
}

export interface AdvancedNavGroup {
  id: string;
  title: string;
  subtitle: string;
  items: readonly AdvancedNavItem[];
}

export const PRIMARY_NAV_ITEMS: readonly PrimaryNavItem[] = [
  { id: 'home', label: 'Trang chủ', icon: Home },
  { id: 'standardizer', label: 'Chuẩn hóa văn bản', icon: FileCheck2 },
  { id: 'complianceAudit', label: 'Kiểm tra văn bản', icon: SearchCheck },
  { id: 'templateEngine', label: 'Mẫu văn bản', icon: LayoutTemplate },
  { id: 'history', label: 'Lịch sử', icon: History },
];

export const ADVANCED_NAV_GROUPS: readonly AdvancedNavGroup[] = [
  {
    id: 'orgHeader',
    title: 'Hồ sơ cơ quan & Phần đầu văn bản',
    subtitle: 'Lưu sẵn thông tin đơn vị, quốc hiệu, tiêu ngữ, chữ ký và nơi nhận.',
    items: [
      {
        id: 'headerBuilder',
        label: 'Hồ sơ đơn vị & Phần đầu',
        description: 'Thiết lập tên cơ quan, quốc hiệu, tiêu ngữ, chữ ký và nơi nhận.',
        icon: Building2,
      },
    ],
  },
  {
    id: 'formatting',
    title: 'Quy tắc định dạng chi tiết',
    subtitle: 'Tùy chỉnh phông chữ, căn lề, đánh số trang, mục lục và làm sạch chữ viết.',
    items: [
      {
        id: 'format',
        label: 'Phông chữ, cỡ chữ & Bảng',
        description: 'Chỉnh phông chữ, cỡ chữ, giãn dòng, thụt đầu dòng và bảng biểu.',
        icon: Type,
      },
      {
        id: 'pageToc',
        label: 'Khổ giấy, lề, số trang & Mục lục',
        description: 'Thiết lập khổ giấy A4, căn lề, đánh số trang, tiêu đề đầu/chân trang.',
        icon: BookOpen,
      },
      {
        id: 'cleaner',
        label: 'Làm sạch chữ & Bảng mã',
        description: 'Xóa khoảng trắng thừa, sửa dấu câu, dấu ngoặc kép và đổi font cũ.',
        icon: Sparkles,
      },
    ],
  },
  {
    id: 'batch',
    title: 'Chuẩn hóa nhiều tệp cùng lúc',
    subtitle: 'Áp dụng nhanh quy chuẩn cho cả danh sách tài liệu hoặc toàn bộ thư mục.',
    items: [
      {
        id: 'oneClick',
        label: 'Chuẩn hóa hàng loạt',
        description: 'Chọn bộ quy chuẩn và xử lý nhiều tệp Word cùng một lúc.',
        icon: Zap,
      },
    ],
  },
  {
    id: 'inspect',
    title: 'Xem trước & Phân tích chuyên sâu',
    subtitle: 'Kiểm tra chi tiết bố cục trang A4, lỗi trình bày phức tạp và thông tin ẩn.',
    items: [
      {
        id: 'livePreview',
        label: 'Xem trước A4 trực quan',
        description: 'Xem trước từng trang A4 và so sánh bản gốc với bản đã chuẩn hóa.',
        icon: Eye,
      },
      {
        id: 'layoutIntelligence',
        label: 'Phân tích bố cục',
        description: 'Nhận diện phần mở đầu, bảng biểu, chữ ký và phát hiện nội dung tràn trang.',
        icon: ScanSearch,
      },
      {
        id: 'security',
        label: 'Bảo mật & Thông tin ẩn',
        description: 'Xóa thông tin tác giả ẩn, dọn macro và xem nhật ký xử lý hệ thống.',
        icon: Shield,
      },
    ],
  },
  {
    id: 'system',
    title: 'Hệ thống & Microsoft Word',
    subtitle: 'Kiểm tra Microsoft Word trên máy, quản lý Add-in và cài thêm phông chữ.',
    items: [
      {
        id: 'system',
        label: 'Hệ thống & Microsoft Word',
        description: 'Kiểm tra kết nối Microsoft Word, quản lý Add-in và cài phông chữ.',
        icon: Wrench,
      },
    ],
  },
];

const ADVANCED_TAB_SET = new Set<NavTab>(
  ADVANCED_NAV_GROUPS.flatMap((group) => group.items.map((item) => item.id)),
);

export function isAdvancedSubTab(tab: NavTab): boolean {
  return ADVANCED_TAB_SET.has(tab);
}

export function getAdvancedTabMeta(tab: NavTab): {
  group: AdvancedNavGroup;
  item: AdvancedNavItem;
} | null {
  for (const group of ADVANCED_NAV_GROUPS) {
    const found = group.items.find((item) => item.id === tab);
    if (found) return { group, item: found };
  }
  return null;
}

interface WorkspaceNavigationProps {
  activeTab: NavTab;
  onSelect: (tab: NavTab) => void;
  hideHome?: boolean;
}

export function WorkspaceNavigation({ activeTab, onSelect, hideHome }: WorkspaceNavigationProps) {
  const inAdvancedArea = activeTab === 'advancedSettings' || isAdvancedSubTab(activeTab);
  const visiblePrimaryItems = PRIMARY_NAV_ITEMS.filter(({ id }) => !hideHome || id !== 'home');
  const advancedItems = ADVANCED_NAV_GROUPS.flatMap((group) => group.items);

  return (
    <nav aria-label="Menu chính MonWord" className="flex flex-col gap-3">
      <div role="group" aria-label="Chức năng chính" className="flex flex-wrap gap-1.5 lg:flex-col lg:gap-1">
        {visiblePrimaryItems.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            aria-current={activeTab === id ? 'page' : undefined}
            className="mw-workspace-link lg:w-full"
            onClick={() => onSelect(id)}
          >
            <Icon className="size-4 shrink-0" aria-hidden="true" />
            <span className="truncate">{label}</span>
          </button>
        ))}
      </div>

      <div
        role="group"
        aria-label="Cài đặt và nâng cao"
        className="border-t border-[var(--mw-border)] pt-2.5 flex flex-col gap-1"
      >
        <button
          type="button"
          aria-current={activeTab === 'advancedSettings' ? 'page' : undefined}
          className={`mw-workspace-link lg:w-full justify-between ${
            inAdvancedArea && activeTab !== 'advancedSettings'
              ? 'bg-[var(--mw-tint)]/70 text-[var(--mw-ink)]'
              : ''
          }`}
          onClick={() => onSelect('advancedSettings')}
        >
          <span className="flex items-center gap-2 min-w-0">
            <SlidersHorizontal className="size-4 shrink-0" aria-hidden="true" />
            <span className="truncate">Cài đặt và nâng cao</span>
          </span>
          <ChevronRight
            className={`size-3.5 shrink-0 transition-transform ${inAdvancedArea ? 'rotate-90' : ''}`}
            aria-hidden="true"
          />
        </button>

        {inAdvancedArea && (
          <div className="flex flex-wrap gap-1 lg:flex-col lg:pl-2 lg:border-l lg:border-[var(--mw-border)] lg:ml-2">
            {advancedItems.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                aria-current={activeTab === id ? 'page' : undefined}
                className="mw-workspace-link text-xs py-1 lg:w-full"
                onClick={() => onSelect(id)}
              >
                <Icon className="size-3.5 shrink-0" aria-hidden="true" />
                <span className="truncate">{label}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </nav>
  );
}
