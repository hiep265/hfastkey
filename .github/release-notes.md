Bộ gõ tiếng Việt Telex **nhẹ nhất thế giới**, viết bằng Rust. Gõ đúng **92.000 bài test**.

| Hệ điều hành | File |
|---|---|
| Windows 10/11 (bộ cài) | `hfastkey-*-windows-x64-setup.exe` |
| Windows (chạy ngay, không cần cài) | `hfastkey-*-windows-x64-portable.zip` |
| Linux x86_64 (Ubuntu 24.04+, GNOME) | `hfastkey-*-linux-x86_64.tar.gz`, giải nén rồi chạy `./install.sh` |

Ctrl+Shift hoặc bấm biểu tượng V/E để bật/tắt tiếng Việt. Tự chạy khi bật máy.

| | HFastKey 0.2 | HFastKey 0.1 | fcitx5 + Unikey |
|---|---|---|---|
| RAM (RSS) | **2,5 MB** | 5 MB | 35 MB |
| RAM thực chiếm (PSS) | **0,6 MB** | | |
| Số luồng | **1** | 8 | |
| File chạy | **0,45 MB** (Linux), **0,13 MB** (Windows) | 2,5 MB, 0,17 MB | |

Bài test: đúng **92.380 / 92.409** ca Telex (Unikey 1.307/1.307, từ điển 69.459/69.459). 29 ca còn lại do dữ liệu nguồn viết sai chính tả hoặc quy ước riêng của bamboo-core.
