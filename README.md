# HFastKey

Bộ gõ tiếng Việt **nhẹ và nhanh**, viết bằng **Rust**, chạy trên **Linux** và **Windows**.
Gõ Telex hoặc VNI, dùng giống Unikey: biểu tượng **V/E** trên khay hệ thống, bấm để bật/tắt.

## Điểm chính

- **Gõ thẳng, không gạch chân** (như Unikey): chữ vào ô nhập ngay; khi thêm dấu, HFastKey xoá
  vài ký tự rồi gõ lại. Không còn lỗi *mất chữ cuối khi bấm Enter* hay *phải bấm Enter 2 lần*.
- Telex, VNI; đặt dấu kiểu cũ (`hòa`) hoặc kiểu mới (`hoà`); gõ dấu ở đâu cũng được
  (`vieejt` hay `vieetj` đều ra `việt`).
- **Tự khôi phục từ sai**: gõ tiếng Anh không bị biến thành chữ có dấu (`text`, `window`, `google`).
- Gõ lặp để bỏ dấu giống Unikey: `ss` → `s`, `aaa` → `aa`, `pussh` → `push`, `classs` → `class`.
- Phím chuyển: **Ctrl+Shift**, **Alt+Z** hoặc **Ctrl+Space**.
- Trên Linux: **không cần fcitx5 hay ibus**. HFastKey tự nói chuyện với GNOME (giao thức IBus).

## Dung lượng và bộ nhớ (đo trên Ubuntu 26.04, GNOME Wayland)

| Thành phần | File | RAM |
|---|---|---|
| `hfastkey-daemon` (bộ gõ, chạy nền) | 2,5 MB | **~5 MB** (trong đó chỉ ~0,7 MB là bộ nhớ riêng, phần còn lại là thư viện dùng chung) |
| `hfastkey-settings` (bảng điều khiển, chỉ chạy khi mở) | 0,4 MB | ~37 MB riêng khi đang mở (GTK4) |
| *So sánh: fcitx5 + Unikey* | | *~35 MB* |

## Cài đặt

Tải bản mới nhất ở trang [Releases](https://github.com/hiep265/hfastkey/releases).

### Windows (10/11, 64-bit)

- **Bộ cài**: `hfastkey-<phiên bản>-windows-x64-setup.exe` (không cần quyền admin). Có tuỳ chọn
  khởi động cùng Windows.
- **Bản chạy ngay**: `hfastkey-<phiên bản>-windows-x64-portable.zip`, giải nén rồi chạy `hfastkey.exe`.

Bấm chuột trái vào biểu tượng V/E để bật/tắt, chuột phải để chọn kiểu gõ và các tuỳ chọn.

> Không nên chạy cùng lúc với Unikey/EVKey/OpenKey.

### Linux (x86_64)

Cần: glibc 2.39+ (Ubuntu 24.04 trở lên), GNOME hoặc môi trường dùng giao thức IBus; GTK 4 cho
bảng điều khiển. Không cần sudo.

```sh
tar xzf hfastkey-<phiên bản>-linux-x86_64.tar.gz
cd hfastkey-<phiên bản>-linux-x86_64
./install.sh
```

`install.sh` cài vào `~/.local/bin`, tự chạy khi đăng nhập, tắt fcitx5 tự khởi động (có giữ bản
sao) và chuyển sang HFastKey ngay. Gỡ: `./uninstall.sh` (tự quay lại fcitx5/ibus).

Mở **Bảng điều khiển** bằng cách tìm *HFastKey* trong danh sách ứng dụng, hoặc chuột phải vào
biểu tượng V/E → *Bảng điều khiển...*

## Build từ mã nguồn

```sh
cargo test --release --workspace         # test logic (lõi, Linux, Windows)
scripts/test-ibus.sh                     # test Linux với thư viện IBus thật, trên bus D-Bus riêng
scripts/install.sh                       # Linux: build và cài cho người dùng hiện tại
cargo build --release -p hfastkey-win    # Windows: build hfastkey.exe
```

Linux cần `libgtk-4-dev` để build bảng điều khiển. Bộ cài Windows build bằng Inno Setup
(`installer/windows/hfastkey.iss`); GitHub Actions tự build cả hai khi tạo tag `v*`.

## Cấu trúc

| Thư mục | Nội dung |
|---|---|
| `src/` | Lõi xử lý tiếng Việt (`hfastkey_core`) và tệp cài đặt dùng chung |
| `daemon/` | Linux: `hfastkey-daemon`, tự đóng vai IBus trên D-Bus, biểu tượng V/E |
| `settings/` | Linux: bảng điều khiển GTK4, bố cục kiểu Unikey |
| `windows/` | Windows: `hfastkey.exe`, hook bàn phím + gửi phím, biểu tượng khay hệ thống |
| `fcitx5/` | (Tuỳ chọn) addon cho fcitx5, cho ai vẫn muốn dùng fcitx5 |
| `installer/`, `scripts/` | Bộ cài Windows, script cài/gỡ/đóng gói Linux |
| `tests/` | Test tích hợp với thư viện IBus thật |

Cài đặt lưu ở `~/.config/hfastkey/config` (Linux) hoặc `%APPDATA%\hfastkey\config` (Windows).

## Cách hoạt động

- **Linux**: GNOME và các ứng dụng GTK/Qt tìm bộ gõ qua `~/.config/ibus/bus/*`. `hfastkey-daemon`
  chiếm tên `org.freedesktop.IBus` trên session D-Bus và ghi địa chỉ vào đó, giống cách fcitx5 làm.
  Khi ứng dụng cho phép xoá chữ quanh con trỏ, HFastKey gõ thẳng (`DeleteSurroundingText` +
  `CommitText`); nếu không (ví dụ terminal) thì dùng chữ gạch chân.
- **Windows**: hook bàn phím mức thấp; phím chữ bình thường được để đi qua, khi cần thêm dấu thì chặn
  phím gốc và gửi Backspace + ký tự Unicode bằng `SendInput`. Bấm chuột hoặc đổi cửa sổ thì bắt đầu
  từ mới.

## Hạn chế hiện tại

- Chưa có gõ tắt (macro); chỉ xuất Unicode dựng sẵn.
- Bản Windows mới: logic đã có test tự động, nhưng còn ít người dùng thử trên máy thật. Rất mong nhận
  [báo lỗi](https://github.com/hiep265/hfastkey/issues).

## Giấy phép

MIT. Xem [LICENSE](LICENSE).
