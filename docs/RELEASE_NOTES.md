# GHI CHÚ PHÁT HÀNH (RELEASE NOTES) — MONWORD v4.0.0

**Phiên bản:** `v4.0.0` (Windows x64 · Web Edition · Mobile PWA)  
**Ngày phát hành:** 19/11/2026  
**Nhà phát hành:** Hệ sinh thái Văn phòng Mon (*Mon Office Ecosystem*)

---

## 1. Tổng quan phát hành MonWord v4.0.0

MonWord v4.0.0 là phiên bản phát hành hoàn chỉnh tổng hợp toàn bộ kết quả nâng cấp từ Phase 1 đến Phase 9, chuyển đổi ứng dụng từ công cụ định dạng nhanh sang **nền tảng chuẩn hóa, kiểm toán tuân thủ và bảo toàn dữ liệu tài liệu Office OpenXML chuyên nghiệp** trên Windows x64.

---

## 2. Các module và tính năng chính đã hoàn thiện

1. **Organization Profile (Phase 1)**: Quản lý đa hồ sơ cơ quan ban hành, địa danh, liên hệ, phạm vi văn bản mặc định, xuất/nhập cấu hình JSON.
2. **Document Header Builder (Phase 2)**: Tự động tạo mới, bóc tách, phát hiện thiếu/sai Quốc hiệu – Tiêu ngữ – Số/Ký hiệu – Địa danh – Trích yếu và chuẩn hóa theo NĐ 30/2020/NĐ-CP, Văn bản Đảng (36-HD) và Doanh nghiệp.
3. **Document Standardization Engine (Phase 3)**: Bộ máy phân tích IR và sửa lỗi chọn lọc theo từng phát hiện, giữ nguyên khối hành chính được bảo vệ, chuyển mã tiếng Việt cũ (TCVN3/VNI), hậu kiểm bắt buộc bằng cách đọc lại tệp thực tế trên đĩa.
4. **Template Engine (Phase 4)**: Thư viện mẫu hành chính & doanh nghiệp đa dạng, hỗ trợ mở rộng không cần sửa mã nguồn, điền biến động bảo toàn split-run, nhập/xuất `.docx`, quản lý phiên bản mẫu.
5. **Layout Intelligence (Phase 5)**: Nhận diện cấu trúc tài liệu theo ngữ cảnh OpenXML và vị trí hình học, phát hiện bảng tràn trang/tách dòng, ảnh tràn lề, textbox và gợi ý chuẩn hóa bố cục không làm sai lệch nội dung.
6. **Advanced Compliance & Audit (Phase 6)**: Ma trận tuân thủ phân cấp nghĩa vụ pháp lý vs khuyến nghị kỹ thuật, truy xuất đầy đủ mã quy tắc/phiên bản/vị trí/biện pháp khắc phục, lưu lịch sử kiểm toán riêng tư, xuất báo cáo `.docx`, `.pdf`, `.csv`, `.json`.
7. **Live Preview Engine (Phase 7)**: Bộ dựng xem trước A4 dọc/ngang trực tiếp từ OpenXML, cập nhật nhanh khi đổi thông số, so sánh Trước/Sau đồng bộ tọa độ lỗi, kèm tùy chọn đối chiếu qua Microsoft Word COM PDF.
8. **Reliability & Data Preservation (Phase 8)**: Safe Save nguyên tử (tạm + `sync_all` + xác minh trước đổi tên + tự phục hồi giao dịch `.tx_*.json`), Backup đa phiên bản (`InitialBaseline` + `PreEditVersion` + kiểm tra SHA-256), bảo toàn 13 nhóm thành phần DOCX/DOCM, giới hạn tài nguyên & chống Zip Bomb / XML Bomb, timeout & thu hồi tiến trình Word COM, chuẩn hóa 15 mã lỗi `AppErrorCode` và log vận hành an toàn riêng tư.
9. **Integration Hardening, Performance & Release (Phase 9)**:
   - Chặn tức thì tệp đặt mật khẩu mở tệp (`EncryptedPackage` / `[MS-DOC] fEncrypted`) trước khi gọi Word COM để tránh treo tiến trình.
   - Chặn khai báo `<!DOCTYPE` / `<!ENTITY` (XXE/DTD), path traversal `..` trong ZIP, ZIP entry trùng lặp và chạy nhầm tệp thực thi qua IPC.
   - Ẩn toàn bộ đường dẫn thư mục chứa tên tài khoản Windows khỏi log vận hành (`monword_ops.jsonl`).
   - Tối ưu loại bỏ lượt đọc ZIP / dựng IR trùng lặp trong chế độ chuẩn hóa & auto-fix recheck (giảm 1 lần gọi COM `.doc → .docx` dư thừa khi chạy kèm kiểm toán).
   - Chia nhỏ bundle Frontend (`manualChunks`) loại bỏ cảnh báo chunk > 500 kB và hoàn thiện 0 cảnh báo biên dịch Rust + TypeScript.
   - Đóng gói bộ cài đặt Windows x64 tự giải nén `MonWord-Setup-v4.0.0-x64.exe` hỗ trợ nâng cấp bảo toàn 100% dữ liệu người dùng.

---

## 3. Lưu ý nâng cấp từ các phiên bản cũ

- Tương thích hoàn toàn với cấu hình người dùng tại `%APPDATA%\MonWord` và thư mục backup `.chuanhoa_backup` từ các bản MonWord trước.
- Khi nâng cấp bằng `MonWord-Setup-v4.0.0-x64.exe`, bản thực thi cũ trước khi đổi thương hiệu (`ChuanHoaVanBanPro.exe` nếu tồn tại trong cùng thư mục cài đặt) sẽ được dọn tự động; toàn bộ dữ liệu cá nhân và sao lưu của người dùng giữ nguyên trạng.
