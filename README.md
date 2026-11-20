# MonWord v4.0.0 — Chuẩn Hóa Văn Bản Hành Chính Pro

Ứng dụng Windows x64 (kèm phiên bản Web / PWA xử lý cục bộ) thuộc **Hệ sinh thái Văn phòng Mon** (*MonPDF · MonWord · MonEx*). Nhà phát hành: **Mon Office Ecosystem**.

MonWord 4.0 hỗ trợ chuẩn hóa toàn diện thể thức và kỹ thuật trình bày văn bản theo **Nghị định 30/2020/NĐ-CP**, đồng thời mở rộng cho **Văn bản Đảng**, **Văn bản Doanh nghiệp** và **Luận văn / Báo cáo học thuật**.

---

## Kiến trúc tổng thể (Phases 1 – 9)

- **Giao diện Desktop**: React 19 + TypeScript + Tailwind CSS nhúng trong WebView2 (`tao` + `wry`), giao tiếp IPC hai chiều với lõi Rust.
- **Lõi xử lý Rust (`x86_64-pc-windows-gnullvm`)**:
  - **Phase 1 – Organization Profile** (`src/engine/org_profile.rs`): Quản lý nhiều hồ sơ cơ quan, đơn vị chủ quản, địa danh, địa chỉ, cấu hình mặc định và xuất/nhập JSON.
  - **Phase 2 – Document Header Builder** (`src/engine/header_builder.rs`): Tạo mới, nhận diện và chuẩn hóa Quốc hiệu, Tiêu ngữ, Tên cơ quan ban hành, Số/Ký hiệu, Địa danh – Ngày tháng, Tên loại văn bản và Trích yếu nội dung.
  - **Phase 3 – Document Standardization Engine** (`src/engine/std_engine/`): Phân tích IR đa lớp, đối chiếu quy tắc NĐ 30/Đảng/Doanh nghiệp/Luận văn, cho phép chọn/bỏ qua từng lỗi, tự động sửa chính xác, đối chiếu diff và **bắt buộc mở lại tệp từ đĩa để hậu kiểm**.
  - **Phase 4 – Template Engine** (`src/engine/template_engine/`): Thư viện mẫu văn bản hành chính & doanh nghiệp dựng sẵn (hỗ trợ bổ sung ngoài code), điền trường động, nhập/xuất mẫu `.docx` và quản lý phiên bản mẫu.
  - **Phase 5 – Layout Intelligence** (`src/engine/layout_intelligence/`): Nhận diện ngữ cảnh cấu trúc (7 bản đồ tài liệu), phát hiện bảng tràn lề/tách dòng, ảnh tràn lề, textbox, dấu trống bất thường và đề xuất điều chỉnh bố cục an toàn.
  - **Phase 6 – Advanced Compliance & Audit** (`src/engine/compliance_audit/`): Ma trận tuân thủ phân tách *Bắt buộc / Có điều kiện / Khuyến nghị / Không áp dụng*, truy xuất mã quy tắc + phiên bản + căn cứ, lịch sử kiểm toán riêng tư và xuất báo cáo `.docx`, `.pdf`, `.csv`, `.json`.
  - **Phase 7 – Live Preview Engine** (`src/engine/live_preview/`): Xem trước khổ A4/dọc/ngang theo chỉ số OpenXML, so sánh Trước/Sau đồng bộ vị trí lỗi và tùy chọn đối chiếu trực tiếp qua Microsoft Word COM PDF.
  - **Phase 8 – Reliability & Data Preservation** (`src/engine/reliability/`): Cơ chế ghi nguyên tử **Safe Save** (ghi tạm + `sync_all()` + xác minh trước khi đổi tên + nhật ký giao dịch `.tx_*.json`), sao lưu đa phiên bản chống trùng (`.chuanhoa_backup/backup_manifest.json` + SHA-256), bảo toàn 13 thành phần DOCX/DOCM, giới hạn Zip Bomb / XML Bomb, ngắt/timeout Word COM kèm thu hồi `WINWORD.EXE` mồ côi, mã lỗi chuẩn (`AppErrorCode`) và log vận hành ẩn thông tin nhạy cảm (`monword_ops.jsonl`).
  - **Phase 9 – Security Hardening, Performance & Installer** (`src/installer_core.rs`, `src/bin/installer.rs`): Phát hiện sớm file mã hóa mật khẩu (`EncryptedPackage` / `[MS-DOC] fEncrypted`) không gây treo COM, chặn DTD/XXE/Zip traversal/entry trùng lặp, rút gọn lượt mở tệp & COM dư thừa, chia nhỏ bundle UI < 500 kB, đóng gói bộ cài `MonWord-Setup-v4.0.0-x64.exe` và gói portable `MonWord-v4.0.0-Portable-x64.zip`.

---

## Định dạng hỗ trợ

| Định dạng | Chế độ hỗ trợ | Ghi chú |
| --- | --- | --- |
| `.docx` | Đọc, phân tích, chuẩn hóa, xem trước A4, bảo toàn cấu trúc trực tiếp | Hoạt động không cần cài Microsoft Word |
| `.docm` | Đầy đủ như `.docx` + tùy chọn **bảo toàn VBA Macro** hoặc làm sạch về `.docx` | Bảo toàn `vbaProject.bin`, `.rels`, ContentTypes |
| `.doc` | Chuyển đổi an toàn sang `.docx` qua Microsoft Word Desktop COM trước khi xử lý | Cần Microsoft Word Desktop; chặn file có mật khẩu mở tệp ngay từ đầu |
| `.xlsx`, `.xlsm` | Xử lý theo lô trong hàng đợi (phông chữ workbook, macro, bảo vệ, metadata) | Phân tích thể thức & xem trước A4 chỉ áp dụng cho Word |

---

## Cài đặt & Đóng gói phát hành (Windows x64)

Xem hướng dẫn chi tiết cho người dùng cuối tại:
- [Hướng dẫn cài đặt & sử dụng](docs/HUONG_DAN_CAI_DAT_SU_DUNG.md)
- [Hướng dẫn sao lưu & khôi phục dữ liệu](docs/HUONG_DAN_SAO_LUU_KHOI_PHUC.md)
- [Ghi chú phát hành (Release Notes v4.0.0)](docs/RELEASE_NOTES.md)
- [Danh sách giới hạn kỹ thuật đã biết](docs/KNOWN_LIMITATIONS.md)

### Biên dịch & kiểm thử từ mã nguồn

Yêu cầu: Windows x64, Node.js/npm, Rust toolchain target `x86_64-pc-windows-gnullvm` (cùng WinLibs `windres.exe` / linker) và Microsoft Edge WebView2 Runtime.

```powershell
# Chạy toàn bộ pipeline: Build UI -> 95+ Rust Tests -> Release x64 -> dist-portable & dist-release
powershell -NoProfile -ExecutionPolicy Bypass -File ".\build-portable.ps1"
```

Sản phẩm tạo ra:
- `dist-portable/MonWord.exe` + `WebView2Loader.dll` + tài liệu (chạy trực tiếp không cần cài đặt).
- `dist-release/MonWord-Setup-v4.0.0-x64.exe` (Bộ cài đặt Windows x64 tự giải nén + Uninstaller).
- `dist-release/MonWord-v4.0.0-Portable-x64.zip` & `dist-release/SHA256SUMS.txt`.
