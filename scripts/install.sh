#!/bin/sh
# Cài HFastKey cho người dùng hiện tại (không cần sudo).
# - Trong gói phát hành (có thư mục bin/): dùng luôn file đã build sẵn.
# - Trong mã nguồn: tự build bằng cargo.
# Script sẽ tắt fcitx5/ibus tự khởi động (có giữ bản sao) và chuyển sang HFastKey ngay.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [ -x "$ROOT/bin/hfastkey-daemon" ]; then
    SRC="$ROOT/bin"
else
    command -v cargo >/dev/null || { echo "Cần cài Rust (cargo) để build từ mã nguồn." >&2; exit 1; }
    cd "$ROOT"
    cargo build --release -p hfastkey-daemon
    SRC="$ROOT/target/release"
fi

BINDIR="$HOME/.local/bin"
AUTOSTART="$HOME/.config/autostart"
LOG="$HOME/.cache/hfastkey.log"
mkdir -p "$BINDIR" "$AUTOSTART" "$HOME/.cache"

install -m755 "$SRC/hfastkey-daemon" "$BINDIR/hfastkey-daemon"
# Bản cũ có bảng điều khiển và tệp cài đặt, bản này không cần nữa.
rm -f "$BINDIR/hfastkey-settings" "$HOME/.local/share/applications/hfastkey-settings.desktop"
rm -rf "$HOME/.config/hfastkey"

cat > "$AUTOSTART/hfastkey.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=HFastKey
Comment=Bộ gõ tiếng Việt Telex viết bằng Rust
Exec=sh -c 'exec "$BINDIR/hfastkey-daemon" >> "$LOG" 2>&1'
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
X-GNOME-AutoRestart=true
DESKTOP

# Tắt fcitx5 tự khởi động (giữ bản sao để gỡ ra là quay lại được).
if [ -f "$AUTOSTART/org.fcitx.Fcitx5.desktop" ]; then
    mv "$AUTOSTART/org.fcitx.Fcitx5.desktop" "$AUTOSTART/org.fcitx.Fcitx5.desktop.hfastkey-backup"
fi

# Chuyển ngay bây giờ.
pkill -x fcitx5 2>/dev/null || true
pkill -x hfastkey-daemon 2>/dev/null || true
sleep 1
setsid "$BINDIR/hfastkey-daemon" >> "$LOG" 2>&1 < /dev/null &
sleep 2
if pgrep -x hfastkey-daemon >/dev/null; then
    echo "HFastKey đang chạy và sẽ tự chạy mỗi lần đăng nhập. Ctrl+Shift hoặc bấm biểu tượng V/E để bật/tắt; log: $LOG"
else
    echo "HFastKey không khởi động được, xem $LOG. Chạy uninstall.sh để quay lại bộ gõ cũ." >&2
    exit 1
fi
