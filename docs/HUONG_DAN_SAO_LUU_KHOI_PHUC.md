# HƯỚNG DẪN SAO LƯU VÀ KHÔI PHỤC DỮ LIỆU MONWORD v4.0.0

MonWord được thiết kế theo nguyên tắc **"Không bao giờ xóa bản gốc trước khi bản mới được ghi hoàn chỉnh và xác minh hợp lệ"**. Tài liệu này hướng dẫn cơ chế bảo vệ tự động và quy trình khôi phục khi xảy ra sự cố.

---

## 1. CƠ CHẾ GHI AN TOÀN (SAFE SAVE ENGINE)

Mỗi khi lưu tệp tài liệu (`.docx`, `.docm`, `.xlsx`, `.xlsm`) hoặc tệp cấu hình (`.json`), MonWord thực hiện 5 bước bắt buộc:

1. **Ghi ra tệp tạm độc quyền**: Tạo tệp tạm `.<tên>.mwtmp.<pid>.<seq>` cùng ổ đĩa với tệp đích.
2. **Đẩy dữ liệu xuống đĩa (`sync_all`)**: Ép hệ điều hành ghi toàn bộ dữ liệu từ bộ nhớ đệm xuống đĩa vật lý.
3. **Kiểm tra tệp tạm trước khi đụng tệp đích**: Mở đọc lại tệp tạm, xác minh cấu trúc ZIP OpenXML và đối chiếu bảo toàn 13 thành phần (XML, `.rels`, Media, Styles, Numbering, Sections, Header/Footer, Fields, VBA Macro, Embedded OLE, Comments, Footnotes, Endnotes).
4. **Thay thế hai pha có nhật ký giao dịch**: Tạo tệp nhật ký `.chuanhoa_backup/.tx_<tên>.json`, chuyển tệp gốc sang tên trung gian tạm thời rồi đổi tên tệp mới sang tệp đích. Nếu gặp lỗi khóa tệp (tệp đang mở trong Word) hoặc lỗi đổi tên, MonWord **tự động hoàn nguyên tệp gốc** và báo mã lỗi `[ERR_FILE_LOCKED_OR_CONFLICT]` hoặc `[ERR_FILE_WRITE]`.
5. **Tự động phục hồi sau ngắt điện / sập tiến trình**: Khi quét lại thư mục hoặc chạy lệnh `recover_interrupted_saves`, nếu phát hiện `.tx_*.json` còn sót và tệp đích bị mất giữa pha đổi tên, MonWord tự động khôi phục nguyên trạng bản gốc.

---

## 2. HỆ THỐNG SAO LƯU ĐA PHIÊN BẢN (`.chuanhoa_backup`)

Khi bạn chọn chế độ **Ghi đè có sao lưu** (hoặc thao tác trên tệp gốc):

- Các bản sao lưu được tạo cạnh thư mục chứa tài liệu tại thư mục ẩn `.chuanhoa_backup\`, kèm chỉ mục toàn vẹn `backup_manifest.json` (lưu mã băm **SHA-256**, kích thước byte, thời điểm, module thực hiện).
- Mỗi tệp backup có tên duy nhất chống ghi đè bằng cờ tạo tệp độc quyền (`create_new`), không bao giờ ghi đè lẫn nhau dù chạy nhiều lần trong cùng một giây.
- Phân loại hai loại bản sao lưu:
  - **`InitialBaseline` (Bản gốc ban đầu)**: Ghi nhận trạng thái nguyên thủy trước lần chỉnh sửa đầu tiên. **Luôn được giữ lại** (chính sách xoay vòng không bao giờ xóa `InitialBaseline`).
  - **`PreEditVersion` (Bản trước chỉnh sửa)**: Ghi nhận trạng thái ngay trước mỗi thao tác tiếp theo. Lưu tối đa **15 phiên bản** gần nhất cho mỗi tệp; các bản cũ hơn sẽ được dọn tự động khi vượt ngưỡng.

---

## 3. QUY TRÌNH KHÔI PHỤC TÀI LIỆU KHI CÓ LỖI

### Cách 1: Khôi phục trực tiếp ngay trên giao diện MonWord
Trên thanh hàng đợi tệp hoặc màn hình kết quả sau khi chạy xong:
- Nhấn nút **`Hoàn tác`**: Khôi phục ngay bản **`PreEditVersion`** gần nhất (trạng thái ngay trước thao tác vừa chạy).
- Nhấn nút **`Bản gốc`**: Khôi phục về bản **`InitialBaseline`** ban đầu (nguyên trạng trước mọi lần chỉnh sửa của MonWord).

Trước khi ghi đè khôi phục, MonWord tự động **kiểm tra SHA-256 + mở kiểm thử cấu trúc OpenXML của tệp sao lưu**. Nếu phát hiện tệp sao lưu bị hỏng hoặc rỗng, MonWord sẽ từ chối và báo `[ERR_BACKUP_INVALID]` để không làm hỏng thêm dữ liệu hiện có.

### Cách 2: Khôi phục thủ công từ thư mục `.chuanhoa_backup`
Trong trường hợp không dùng giao diện MonWord:
1. Mở thư mục chứa tài liệu cần khôi phục.
2. Bật hiển thị tệp/thư mục ẩn trong Windows Explorer (hoặc gõ `\ .chuanhoa_backup` vào thanh địa chỉ).
3. Mở `backup_manifest.json` (nếu cần xem chính xác mốc thời gian và `kind`: `"InitialBaseline"` hay `"PreEditVersion"`) để xác định đúng tên tệp sao lưu tương ứng.
4. Sao chép tệp trong `.chuanhoa_backup` ra ngoài, đổi lại đúng tên tệp gốc và mở kiểm tra bằng Microsoft Word.

---

## 4. SAO LƯU & KHÔI PHỤC HỒ SƠ, MẪU VĂN BẢN VÀ NHẬT KÝ

Dữ liệu người dùng của MonWord được lưu tách biệt hoàn toàn với thư mục chương trình tại:
- Hồ sơ tổ chức, mẫu tùy chỉnh, rule pack & lịch sử kiểm toán: `%APPDATA%\MonWord\`
- Nhật ký vận hành an toàn riêng tư (chỉ lưu tên tệp cơ sở, thời gian xử lý, mã lỗi; không lưu nội dung tài liệu): `%APPDATA%\MonWord\logs\monword_ops.jsonl`

**Khuyến nghị:** Để sao lưu cấu hình sang máy tính mới, bạn có thể dùng nút **Xuất JSON** trong tab *Hồ sơ tổ chức* hoặc sao chép toàn bộ thư mục `%APPDATA%\MonWord\`.
