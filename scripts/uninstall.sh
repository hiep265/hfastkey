#!/bin/sh
# Gỡ HFastKey và quay lại bộ gõ cũ (fcitx5 hoặc ibus).
AUTOSTART="$HOME/.config/autostart"
pkill -x hfastkey-daemon 2>/dev/null || true
rm -f "$AUTOSTART/hfastkey.desktop" "$HOME/.local/bin/hfastkey-daemon" "$HOME/.local/bin/hfastkey-settings" \
      "$HOME/.local/share/applications/hfastkey-settings.desktop"
if [ -f "$AUTOSTART/org.fcitx.Fcitx5.desktop.hfastkey-backup" ]; then
    mv "$AUTOSTART/org.fcitx.Fcitx5.desktop.hfastkey-backup" "$AUTOSTART/org.fcitx.Fcitx5.desktop"
fi
if command -v fcitx5 >/dev/null; then
    setsid fcitx5 -rd >/dev/null 2>&1 < /dev/null &
    echo "Đã gỡ HFastKey, quay lại fcitx5."
elif command -v ibus-daemon >/dev/null; then
    setsid ibus-daemon -drx >/dev/null 2>&1 < /dev/null &
    echo "Đã gỡ HFastKey, quay lại ibus."
else
    echo "Đã gỡ HFastKey."
fi
