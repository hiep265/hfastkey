#!/bin/sh
# Build lõi Rust + addon fcitx5.
set -e
cd "$(dirname "$0")/.."
cargo test --release
cargo build --release
cmake -S fcitx5 -B build -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build -j"$(nproc)"
echo "Build xong: build/libhfastkey.so. Cài đặt: sudo cmake --install build && fcitx5 -rd"
