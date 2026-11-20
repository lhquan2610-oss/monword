# HƯỚNG DẪN CÀI ĐẶT VÀ SỬ DỤNG MONWORD v4.0.0

**Phiên bản:** `4.0.0` (Windows x64)  
**Nhà phát hành:** Hệ sinh thái Văn phòng Mon (*Mon Office Ecosystem*)

---

## PHẦN 1: YÊU CẦU HỆ THỐNG

- **Hệ điều hành:** Windows 10 hoặc Windows 11 (64-bit / x64).
- **Thành phần nền tảng:** Microsoft Edge WebView2 Runtime (có sẵn trên Windows 11 và hầu hết Windows 10 cập nhật).
- **Microsoft Word Desktop (Khuyến nghị):** Bắt buộc nếu dùng các tính năng mở rộng qua Word COM:
  - Chuyển đổi tệp Word nhị phân cũ (`.doc` → `.docx`).
  - Xuất báo cáo hoặc văn bản kèm định dạng `.pdf` chính xác theo Word.
  - Cập nhật trường mục lục tự động (`Update Fields / TOC`) bằng Word thật.
  - Chức năng **Đối chiếu PDF Microsoft Word** trong màn hình Xem trước A4.
- **Lưu ý bảo mật:** Toàn bộ quá trình đọc, phân tích, chuẩn hóa và lưu tệp diễn ra **cục bộ 100% trên máy người dùng**, không gửi nội dung tài liệu lên máy chủ bên ngoài (Live OTA chỉ kiểm tra metadata phiên bản và giao diện khi người dùng bấm kiểm tra cập nhật).

---

## PHẦN 2: HƯỚNG DẪN CÀI ĐẶT, NÂNG CẤP VÀ GỠ BỎ

### Cách 1: Cài đặt bằng Bộ cài đặt chính thức (`MonWord-Setup-v4.0.0-x64.exe`)
1. Chạy tệp `MonWord-Setup-v4.0.0-x64.exe`.
2. Xác nhận hộp thoại cài đặt. Chương trình sẽ giải nén an toàn vào `%LOCALAPPDATA%\Programs\MonWord`, tạo lối tắt trên **Desktop**, **Start Menu** và đăng ký mục gỡ cài đặt trong **Windows Apps / Add or Remove Programs**.
3. Sau khi hoàn tất, chọn **Yes** để khởi chạy MonWord ngay hoặc mở từ Desktop.

### Cài đặt không tương tác (Silent Install cho IT / Doanh nghiệp)
```powershell
.\MonWord-Setup-v4.0.0-x64.exe --silent
# Tùy chọn thư mục cài riêng:
.\MonWord-Setup-v4.0.0-x64.exe --silent --dir "D:\Tools\MonWord"
```

### Cách 2: Sử dụng bản Portable (`MonWord-v4.0.0-Portable-x64.zip`)
1. Giải nén toàn bộ thư mục ZIP vào một thư mục cố định (ví dụ `D:\Apps\MonWord`).
2. Giữ `MonWord.exe` và `WebView2Loader.dll` nằm **cùng một thư mục**, sau đó chạy `MonWord.exe`.

### Nâng cấp từ phiên bản trước
- Chỉ cần chạy `MonWord-Setup-v4.0.0-x64.exe` đè lên thư mục cài đặt cũ (hoặc thay thế tệp trong bản Portable).
- Bộ cài đặt sẽ dọn tệp thực thi cũ (`ChuanHoaVanBanPro.exe` nếu có) và cập nhật `MonWord.exe` theo cơ chế nguyên tử.
- **Cam kết bảo toàn dữ liệu:** Quá trình cài đặt, nâng cấp hoặc gỡ bỏ **KHÔNG bao giờ xóa hoặc ghi đè** cấu hình tại `%APPDATA%\MonWord` (Hồ sơ tổ chức, mẫu tùy chỉnh, gói quy tắc, lịch sử kiểm toán, nhật ký) cũng như mọi bản sao lưu `.chuanhoa_backup` cạnh tài liệu của bạn.

### Gỡ cài đặt
- Mở **Settings → Apps → Installed apps → MonWord → Uninstall**, hoặc chạy `uninstall.exe` trong thư mục cài đặt (`%LOCALAPPDATA%\Programs\MonWord\uninstall.exe`).

---

## PHẦN 3: HƯỚNG DẪN SỬ DỤNG CÁC CHỨC NĂNG CHÍNH

### Bước 1: Thiết lập Hồ sơ Tổ chức (`Hồ sơ tổ chức`)
- Khai báo Tên cơ quan chủ quản, Tên cơ quan/tổ chức ban hành, Ký hiệu viết tắt, Địa danh, Địa chỉ – SĐT – Email, và chọn phạm vi quy chuẩn mặc định (**Nghị định 30/2020/NĐ-CP**, **Văn bản Đảng**, hoặc **Doanh nghiệp**).
- Dữ liệu hồ sơ được sử dụng tự động cho phần đầu văn bản, kiểm tra chéo tên cơ quan/địa danh và sinh mẫu văn bản.

### Bước 2: Tạo hoặc chuẩn hóa Phần đầu văn bản (`Phần đầu`)
- Chọn tài liệu `.docx` hoặc tạo mới tệp trắng từ phần đầu chuẩn.
- Chọn loại văn bản (*Công văn, Quyết định, Tờ trình, Thông báo, Báo cáo...*), nhập Số/Ký hiệu, Ngày ban hành và Trích yếu.
- Nhấn **Kiểm tra phần đầu** để phát hiện thiếu dòng Quốc hiệu/Tiêu ngữ, sai cỡ chữ hoặc thiếu gạch chân ngang, sau đó nhấn **Áp dụng** để chuẩn hóa.

### Bước 3: Thư viện Mẫu văn bản (`Mẫu văn bản`)
- Duyệt 20+ mẫu hành chính & doanh nghiệp dựng sẵn; có thể **Nhân bản**, **Tùy chỉnh**, **Nhập từ DOCX**, **Xuất DOCX**, hoặc khôi phục lại phiên bản mẫu cũ.
- Điền các trường động theo hồ sơ tổ chức và áp dụng tạo tệp mới hoặc chuẩn hóa khung tệp hiện có.

### Bước 4: Chuẩn hóa thể thức & Kiểm toán tuân thủ (`Chuẩn hóa` / `Kiểm toán`)
- Tải tệp lên để phân tích cấu trúc OpenXML (hiển thị loại văn bản, số đoạn, số bảng, section, ảnh).
- Xem danh sách lỗi phân tách rõ **Bắt buộc**, **Có điều kiện**, **Khuyến nghị** cùng căn cứ quy định.
- Đánh dấu chọn/bỏ qua từng lỗi hoặc chọn chế độ **Ghi bản mới** / **Ghi đè có sao lưu**.
- Hệ thống tự động sửa lỗi, **mở lại tệp từ đĩa** để xác nhận XML hợp lệ + bảo toàn 13 nhóm thành phần + tính lại điểm sau chuẩn hóa.
- Có thể xuất báo cáo kiểm toán ra `.docx`, `.pdf`, `.csv`, `.json`.

### Bước 5: Phân tích Bố cục & Xem trước A4 (`Bố cục` / `Xem trước A4`)
- Phát hiện bảng tràn trang, thiếu lặp tiêu đề bảng dài, ảnh vượt vùng in, dòng đơn độc hoặc textbox lệch.
- Bật chế độ **So sánh Trước / Sau** trên khổ A4 (hỗ trợ đa section Dọc/Ngang); bấm vào đánh dấu lỗi để nhảy tới vị trí tương ứng.
- Nếu máy có Microsoft Word Desktop, có thể bấm **Đối chiếu PDF Word** để kiểm tra số trang thực tế của Microsoft Word.

### Bước 6: Xử lý hàng loạt một chạm (`Một chạm`)
- Kéo thả nhiều tệp hoặc cả thư mục chứa `.docx`, `.docm`, `.doc`, `.xlsx`, `.xlsm`.
- Bấm **Kiểm tra lỗi** để rà soát nhanh trước khi chạy, sau đó bấm **Chuẩn hóa ngay**. Khi cần dừng giữa chừng đối với lô lớn, bấm **Dừng tác vụ**.
