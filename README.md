# MonWord — Chuẩn Hóa Văn Bản Pro

Ứng dụng Windows x64 thuộc hệ sinh thái văn phòng Mon để định dạng và làm sạch tài liệu Office theo lô. Phiên bản trong mã nguồn là **3.0.0**. Giao diện React chạy trong WebView2, gửi yêu cầu qua IPC đến lõi Rust. Lõi xử lý các gói Office Open XML trực tiếp trên máy.

## Phạm vi chức năng

| Đầu vào | Xử lý |
| --- | --- |
| `.docx`, `.docm` | Kiểm tra trước và sau xử lý, chuẩn hóa phông chữ, đoạn, bảng và lề, làm sạch văn bản tiếng Việt và chuyển mã TCVN3/VNI, cấu hình số trang và hướng trang, nhận diện tiêu đề và chèn mục lục. Có tùy chọn xử lý macro, thuộc tính tài liệu và bảo vệ chỉnh sửa. |
| `.doc` | Cần bật chuyển đổi và có Microsoft Word desktop để chuyển sang `.docx` qua COM trước khi xử lý. |
| `.xlsx`, `.xlsm` | Xử lý phông chữ qua styles của workbook, kiểm tra và xử lý macro, thuộc tính hoặc bảo vệ tùy cấu hình. Các thao tác dàn trang, mục lục và xuất PDF trong ứng dụng áp dụng cho Word. |

Có bốn cấu hình dựng sẵn: Nghị định 30, Văn bản Đảng, Doanh nghiệp và Luận văn. Điểm audit là chỉ số nội bộ tính theo các quy tắc phát hiện trong mã. Khi dùng cho hồ sơ chính thức, cần kiểm tra lại thể thức trên tài liệu xuất. Khung A4 minh họa cấu hình hiện tại bằng nội dung mẫu.

## Sử dụng

1. Chạy `dist-portable/MonWord.exe`, để `WebView2Loader.dll` cùng thư mục. Máy chạy cần Microsoft Edge WebView2 Runtime. Microsoft Word desktop cần cho chuyển `.doc`, cập nhật mục lục qua COM và xuất PDF.
2. Chọn hoặc kéo thả tệp/thư mục, chọn cấu hình và xem báo cáo kiểm tra. Chạy thử trên bản sao của một số tài liệu đại diện trước khi xử lý cả lô.
3. Chế độ **Tạo bản sao** lưu `<tên>_chuanhoa.<đuôi>` cạnh tệp nguồn. Chế độ **Ghi đè có sao lưu** tạo bản sao trong `.chuanhoa_backup` cạnh nguồn. Khi đầu ra đổi đuôi, ví dụ `.docm` thành `.docx`, đích được tính theo đuôi mới. Kiểm tra tệp đích đã tồn tại trước khi chạy.
4. Xem nhật ký từng tệp và mở kết quả bằng Word hoặc Excel. Với tác vụ PDF hoặc cập nhật mục lục, đọc cả cảnh báo COM vì trạng thái thành công có thể chỉ xác nhận đã lưu tệp Word.

Ứng dụng còn có tiện ích cấu hình Office, quản lý add-in và cài phông chữ người dùng. Thay đổi Protected View hoặc chính sách macro ảnh hưởng đến Office của tài khoản Windows. Giữ các thiết lập bảo vệ mặc định nếu chỉ cần chuẩn hóa tài liệu.

## Phát triển trên Windows

Chuẩn bị Node.js/npm, Rust/rustup, toolchain liên kết tương thích với target `x86_64-pc-windows-gnullvm` và WebView2 Runtime. Nếu máy dùng WinLibs cài qua WinGet, script đóng gói có bước dò thư mục `bin` của WinLibs. Chạy từ thư mục chứa README này:

```powershell
rustup target add x86_64-pc-windows-gnullvm
Push-Location ui
npm.cmd ci
npm.cmd run build
Pop-Location

cargo test --locked
cargo run --locked
```

`build.rs` nhúng `ui/dist` vào chương trình. Hãy dựng lại giao diện trước khi chạy Rust nếu đã sửa frontend. `npm.cmd run dev` trong `ui` dùng để phát triển giao diện trên trình duyệt. Các thao tác tệp cần IPC của ứng dụng desktop.

Để đóng gói, chạy `./build-portable.ps1` sau khi cài dependency npm. Script dựng UI, chạy kiểm thử Rust, dựng release rồi chép EXE và DLL vào `dist-portable`. Nếu gặp lỗi linker, kiểm tra toolchain trên `PATH` và target đang chọn. Mục linker theo đường dẫn tuyệt đối trong `.cargo/config.toml` thuộc target `windows-gnu`, còn target mặc định là `windows-gnullvm`.

## Cấu trúc mã nguồn

| Vị trí | Trách nhiệm |
| --- | --- |
| [`src/main.rs`](src/main.rs), [`src/ipc.rs`](src/ipc.rs) | Cửa sổ desktop, nhúng UI, kéo thả, hộp thoại và điều phối IPC. |
| [`src/models.rs`](src/models.rs), [`ui/src/types.ts`](ui/src/types.ts) | Kiểu cấu hình và dữ liệu trao đổi Rust–TypeScript. |
| [`src/engine/mod.rs`](src/engine/mod.rs) | Quét tệp, điều phối chuỗi xử lý, đặt đường dẫn đầu ra và sao lưu. |
| [`src/engine/`](src/engine/) | Đọc/ghi ZIP và XML, làm sạch tiếng Việt, định dạng, xử lý trang/mục lục, macro và tích hợp Office. |
| [`ui/src/`](ui/src/) | Giao diện React, hàng đợi, xem trước A4, báo cáo và cầu nối IPC. |
| [`src/tests/integration_tests.rs`](src/tests/integration_tests.rs) | Kiểm thử trên gói Word mẫu do mã kiểm thử tạo. |

Luồng chính: chọn tệp → mở gói và audit → sao lưu nếu chọn ghi đè → làm sạch tiếng Việt → định dạng → xử lý trang và mục lục → áp dụng tùy chọn bảo vệ/macro → lưu → cập nhật mục lục hoặc xuất PDF qua Word nếu cần.

## Ưu điểm

- Các bước xử lý được tách thành mô-đun. Chuyển mã tiếng Việt chạy trước khi đổi phông, giúp giữ thông tin nhận diện phông cũ cho bước chuyển mã.
- Định dạng Open XML trực tiếp giảm phụ thuộc vào Word cho luồng chính. Word COM được dùng ở các bước cần Word như chuyển `.doc` và xuất PDF.
- Hàng đợi, audit trước/sau và nhật ký từng tệp giúp rà soát kết quả xử lý theo lô. Có lựa chọn tạo bản sao và cơ chế sao lưu/phục hồi khi ghi đè.
- Đã có kiểm thử tích hợp cho định dạng theo ngữ cảnh, chuyển mã tiếng Việt, số trang/hướng trang/mục lục và xử lý macro/metadata kèm phục hồi bản sao lưu.

## Hạn chế và cách khắc phục

| Ưu tiên | Phát hiện từ mã nguồn | Cách khắc phục |
| --- | --- | --- |
| Cao | [Lưu gói](src/engine/openxml_arch.rs#L137) ghi ra tệp tạm rồi xóa đích trước khi đổi tên. Nếu đổi tên lỗi sau khi xóa, đường dẫn đích sẽ mất. [Tên đầu ra và sao lưu](src/engine/mod.rs#L114) có thể trùng khi chạy lại hoặc sao lưu trong cùng giây. | Dùng thao tác thay thế tệp phù hợp trên Windows sau khi ghi và xác minh gói tạm. Dùng tên tạm và backup duy nhất, tạo theo chế độ độc quyền. Xử lý xung đột đích rõ ràng, kể cả khi đổi đuôi. Kiểm thử tệp đang mở, đích tồn tại và lỗi trong lúc commit. |
| Cao | [Xử lý trang](src/engine/page_toc.rs#L412) có nhánh gỡ tham chiếu header/footer và nhánh tạo tham chiếu mới khi chèn số trang. Nội dung đầu/chân trang sẵn có như logo, chữ và trường khác có thể biến mất. Xoay theo trang dựa vào dấu ngắt hoặc dấu phân trang đã lưu, nên có thể lệch số trang Word thực tế. | Cập nhật trường `PAGE` trong từng phần header/footer hiện có, giữ nội dung và biến thể first/even/default. Giới hạn xoay ở section xác định được, hoặc dùng thông tin phân trang từ Word. Kiểm thử tài liệu nhiều section. |
| Cao | [Tích hợp Office](src/engine/system_office.rs#L113) có tùy chọn thay đổi Protected View/chính sách macro của tài khoản. Các bước COM mở tài liệu tự động và mã hiện chưa đặt chính sách macro riêng cho phiên COM. | Tách chức năng quản trị khỏi luồng tài liệu. Mặc định giữ bảo vệ, yêu cầu xác nhận rõ khi đổi cấu hình tài khoản và lưu giá trị cũ để khôi phục. Đặt chính sách chặn macro cho phiên COM trước khi mở tài liệu. |
| Trung bình | [Đọc Open XML](src/engine/openxml_arch.rs#L34) nạp các phần ZIP đã giải nén vào bộ nhớ, thiếu hạn mức rõ ràng. [Cầu nối IPC](ui/src/bridge.ts#L61) giữ lệnh chờ đến khi có phản hồi. Tiến trình Word/PowerShell hiện chờ đồng bộ và có thể làm treo lô. | Giới hạn số phần ZIP và kích thước đã giải nén theo từng phần và toàn gói. Thêm thời hạn cho COM, trạng thái hủy theo lô, và dọn yêu cầu IPC khi hết hạn hoặc gửi lỗi. |
| Trung bình | [Kết quả xử lý](src/engine/mod.rs#L325) vẫn báo thành công khi cập nhật mục lục hoặc xuất PDF lỗi. Audit sau xử lý đọc gói trong bộ nhớ, nên có thể khác tệp cuối cùng đã được Word lưu lại. [Cấu hình](src/models.rs) có `lock_password`, nhưng [bước khóa Word](src/engine/security_macro.rs#L198) chỉ nhận chế độ chỉnh sửa. | Báo riêng trạng thái lưu Office, cập nhật mục lục và xuất PDF. Đọc lại đầu ra cuối để audit. Bỏ cấu hình mật khẩu chưa có hiệu lực hoặc hoàn thiện cơ chế trước khi công bố hỗ trợ. |
| Trung bình | Các ca kiểm thử hiện dùng Word mẫu tối giản. Quy trình đóng gói còn phụ thuộc dò WinLibs và tìm DLL qua kho Cargo cục bộ. | Bổ sung tài liệu Word/Excel đại diện, kiểm tra tệp xuất bằng Office và chạy build x64 từ môi trường Windows sạch. Xác định chính xác nguồn DLL và kiểm tra đủ thành phần trước khi đóng gói. |

## Kết quả kiểm chứng khi rà soát

- `cargo test --locked` đạt **4/4**.
- Kiểm tra TypeScript với `tsc --noEmit --incremental false` đạt.
- Các nhận xét về bố cục trong Word/Excel, chức năng COM và bộ portable dựa trên việc đọc mã. Đợt rà soát chưa kiểm tra trực tiếp các luồng đó qua giao diện.
