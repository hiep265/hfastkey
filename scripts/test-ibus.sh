#!/bin/sh
# Test daemon bằng thư viện IBus thật, trên một bus D-Bus riêng (không ảnh hưởng máy đang dùng).
set -e
cd "$(dirname "$0")/.."
cargo build --release -p hfastkey-daemon
DIR="$(pwd)"
TMPCFG="$(mktemp -d)"
trap 'rm -rf "$TMPCFG"' EXIT
XDG_CONFIG_HOME="$TMPCFG" dbus-run-session -- sh -c "
  '$DIR/target/release/hfastkey-daemon' --no-address-file --no-tray &
  sleep 1
  IBUS_ADDRESS=\"\$DBUS_SESSION_BUS_ADDRESS\" python3 -I '$DIR/tests/ibus_client_test.py' 2>/dev/null || exit 1
  echo \"RAM (RSS): \$(ps -o rss= -C hfastkey-daemon) KB\"
"
