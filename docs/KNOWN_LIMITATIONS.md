# DANH SÁCH GIỚI HẠN KỸ THUẬT ĐÃ BIẾT — MONWORD v4.0.0

Tài liệu này liệt kê minh bạch các giới hạn kỹ thuật hiện hành của MonWord v4.0.0 để người dùng và quản trị viên nắm rõ phạm vi vận hành an toàn:

1. **Phụ thuộc Microsoft Word Desktop đối với một số định dạng/tác vụ chuyên biệt**:
   - Đọc/xử lý chuẩn trực tiếp `.docx` và `.docm` **không cần** Microsoft Word.
   - Tuy nhiên, chuyển đổi tệp Word 97–2003 nhị phân (`.doc` → `.docx`), xuất PDF chuẩn xác từ văn bản Word (`ExportAsFixedFormat`), cập nhật số trang mục lục tự động thực tế qua COM (`TablesOfContents.Update`) và tính năng đối chiếu PDF trực tiếp trong Live Preview **yêu cầu máy Windows đã cài Microsoft Word Desktop** (khuyến nghị Office 2016/2019/2021/365). Nếu máy không cài Word, các tính năng này sẽ báo lỗi `[ERR_WORD_COM]` / `[ERR_DOC_CONVERT]` có hướng xử lý, không làm hỏng tệp nguồn.

2. **Tệp có mật khẩu mở tệp (Encrypt with Password)**:
   - MonWord phát hiện và từ chối ngay tệp `.docx`/`.doc` bị mã hóa bằng mật khẩu mở tệp (`[ERR_FILE_LOCKED_OR_CONFLICT]`) vì không tự động bẻ khóa hoặc lưu trữ mật khẩu người dùng và để tránh tiến trình `WINWORD.EXE` nền bị kẹt hộp thoại nhập mật khẩu. Người dùng cần mở tệp bằng Word và gỡ mật khẩu trước khi chuẩn hóa.
   - Đối với khóa chỉnh sửa dạng **Restrict Editing** không mã hóa toàn gói (`w:documentProtection` trong `.docx`), MonWord hỗ trợ nhận diện và tùy chọn gỡ bỏ khi được bật cấu hình.

3. **Ngưỡng bảo vệ tài nguyên (Chống Zip Bomb / Treo RAM)**:
   - Kích thước tệp nén tối đa: **150 MB** (128 MB trên bản Web).
   - Tổng kích thước giải nén tối đa trong bộ nhớ: **512 MB** (384 MB trên bản Web).
   - Kích thước 1 thành phần XML tối đa: **32 MB** (tối đa **250.000** phần tử XML, độ sâu lồng thẻ **256**).
   - Thành phần có kích thước giải nén thực tế từ **16 MB** trở lên và tỷ lệ nén bất thường (> **120:1**) bị từ chối với mã `[ERR_ZIP_BOMB]`.
   - Thời gian chờ tối đa mỗi tác vụ Word COM: **45 giây** (mặc định, sẽ tự hủy tiến trình COM mồ côi khi quá hạn).

4. **Độ trung thực phân trang của Live Preview OpenXML**:
   - Bản xem trước A4 đọc trực tiếp kích thước trang, lề, bảng, ảnh, khoảng cách từ OpenXML để dựng bố cục tức thì mà không cần Word. Tuy nhiên, thuật toán dàn dòng/ngắt trang theo metric nội bộ có thể chênh lệch nhẹ so với engine dàn chữ *Line-Services / Kerning* độc quyền của Microsoft Word trên văn bản dài hoặc phông đặc thù. Khi cần chính xác tuyệt đối để in phát hành, hãy dùng nút **Đối chiếu PDF Word** hoặc mở xem bằng Microsoft Word.

5. **Phạm vi xử lý tệp Excel (`.xlsx` / `.xlsm`)**:
   - Lỗi và chuẩn hóa thể thức NĐ 30/2020/NĐ-CP, Header Builder, Template Library, Layout Intelligence, Compliance Audit và Live Preview A4 chỉ áp dụng cho tài liệu Microsoft Word. Với Excel, ứng dụng hỗ trợ chuẩn hóa phông chữ trong styles, làm sạch Macro/Metadata và gỡ khóa sheet/workbook theo hàng đợi.

6. **Miễn trừ trách nhiệm pháp lý**:
   - Điểm kiểm toán và báo cáo tuân thủ do MonWord tạo ra là **chỉ số tham chiếu kỹ thuật** dựa trên bộ quy tắc tin học hóa Nghị định 30/2020/NĐ-CP và Hướng dẫn 36-HD/VPTW; không thay thế công tác thẩm định thể thức, nội dung và hiệu lực pháp lý của văn thư cơ quan khi phát hành văn bản chính thức.

7. **Chữ ký số Authenticode (Code Signing)**:
   - Bộ cài đặt `MonWord-Setup-v4.0.0-x64.exe` và `MonWord.exe` được phát hành kèm mã băm kiểm chứng `SHA256SUMS.txt` và Windows PE `VERSIONINFO` nhưng chưa gắn chứng thư số EV thương mại (`signtool` chưa cài trên máy build). Trên Windows SmartScreen lần đầu tải từ Internet, quản trị viên nội bộ cần kiểm tra đúng SHA-256 trước khi chạy/triển khai.
