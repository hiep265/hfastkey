#!/bin/sh
# Đóng gói bản Linux đã build sẵn: dist/hfastkey-<phiên bản>-linux-x86_64.tar.gz
set -e
cd "$(dirname "$0")/.."
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
NAME="hfastkey-$VERSION-linux-x86_64"
rm -rf "dist/$NAME" && mkdir -p "dist/$NAME/bin"
cp target/release/hfastkey-daemon "dist/$NAME/bin/"
cp scripts/install.sh scripts/uninstall.sh README.md LICENSE "dist/$NAME/"
# Trong gói, install.sh nằm ngay cạnh bin/.
sed -i 's#^ROOT="$(cd "$(dirname "$0")/.." \&\& pwd)"#ROOT="$(cd "$(dirname "$0")" \&\& pwd)"#' "dist/$NAME/install.sh"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "dist/$NAME"
echo "dist/$NAME.tar.gz"
