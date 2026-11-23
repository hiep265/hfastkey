# HFastKey

Bộ gõ tiếng Việt Telex **nhẹ nhất thế giới**, viết bằng Rust. Gõ đúng **92.000 bài test**.

## Tải xuống

| Hệ điều hành | Tải về |
|---|---|
| Windows 10/11 (bộ cài) | [hfastkey-0.2.0-windows-x64-setup.exe](https://github.com/hiep265/hfastkey/releases/download/v0.2.0/hfastkey-0.2.0-windows-x64-setup.exe) |
| Windows (chạy ngay, không cần cài) | [hfastkey-0.2.0-windows-x64-portable.zip](https://github.com/hiep265/hfastkey/releases/download/v0.2.0/hfastkey-0.2.0-windows-x64-portable.zip) |
| Linux x86_64 (Ubuntu 24.04+, GNOME) | [hfastkey-0.2.0-linux-x86_64.tar.gz](https://github.com/hiep265/hfastkey/releases/download/v0.2.0/hfastkey-0.2.0-linux-x86_64.tar.gz) (giải nén rồi chạy `./install.sh`) |

Ctrl+Shift hoặc bấm biểu tượng V/E để bật/tắt tiếng Việt. Tự chạy khi bật máy.

## Số liệu

Đo trên Ubuntu 26.04, GNOME Wayland, khi bộ gõ đang chạy nền.

| | HFastKey 0.2 | HFastKey 0.1 | fcitx5 + Unikey |
|---|---|---|---|
| RAM (RSS) | **2,5 MB** | 5 MB | 35 MB |
| RAM thực chiếm (PSS, đã chia phần thư viện dùng chung) | **0,6 MB** | | |
| Bộ nhớ riêng | **0,19 MB** | 0,7 MB | |
| Số luồng | **1** | 8 | |
| File chạy (Linux) | **0,45 MB** | 2,5 MB | |
| File chạy (Windows) | **0,13 MB** | 0,17 MB | |
| Tải về | **76 KB** (Windows zip), **223 KB** (Linux) | 90 KB, 1,25 MB | |
| Thư viện ngoài | **0** | zbus, ksni, GTK4 | |

So với fcitx5 + Unikey, HFastKey dùng ít hơn khoảng **14 lần** RAM (RSS).

## Bài test

HFastKey gõ đúng **92.380 / 92.409** ca Telex lấy từ GitHub (`scripts/conformance.sh`):

| Nguồn | Số ca | Đúng |
|---|---|---|
| Bảng test Telex của [fcitx5-unikey](https://github.com/fcitx/fcitx5-unikey) | 1.307 | **1.307 (100%)** |
| ~69.000 từ/cụm từ ([duyet/vietnamese-wordlist](https://github.com/duyet/vietnamese-wordlist)) | 69.459 | **69.459 (100%)** |
| 7.184 âm tiết ([vietnameselanguage/syllable](https://github.com/vietnameselanguage/syllable)), gõ theo nhiều thứ tự, chữ hoa | 21.603 | 21.582 (99,9%) |
| Test Telex của [bamboo-core](https://github.com/BambooEngine/bamboo-core) | 40 | 32 |

29 ca còn lại không phải lỗi gõ: 21 ca do dữ liệu nguồn viết sai chính tả (`qúa`, `gía`; HFastKey ra
đúng `quá`, `giá`), 8 ca do bamboo-core có quy ước riêng.
