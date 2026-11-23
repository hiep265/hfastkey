#!/bin/sh
# Test đối chiếu HFastKey với dữ liệu bên ngoài (khoảng 183.000 ca):
#   - bảng test Telex của fcitx5-unikey (GPL, chỉ tải về khi chạy)
#   - test của bamboo-core (MIT)
#   - 7.184 âm tiết tiếng Việt (vietnameselanguage/syllable)
#   - ~74.000 từ (duyet/vietnamese-wordlist, GPL, chỉ tải về khi chạy)
set -e
cd "$(dirname "$0")/.."
DATA="${CONFORMANCE_DATA:-target/conformance-data}"
mkdir -p "$DATA"
get() { [ -s "$DATA/$2" ] || curl -fsSL "$1" -o "$DATA/$2"; }
get https://raw.githubusercontent.com/fcitx/fcitx5-unikey/master/test/testunikey.cpp testunikey.cpp
get https://raw.githubusercontent.com/BambooEngine/bamboo-core/master/bamboo_test.go bamboo_test.go
get https://raw.githubusercontent.com/vietnameselanguage/syllable/master/vietnamesesyllable_7184.txt syllables.txt
get https://raw.githubusercontent.com/duyet/vietnamese-wordlist/master/Viet74K.txt Viet74K.txt
python3 -I tests/conformance/gen_cases.py \
    --unikey "$DATA/testunikey.cpp" --bamboo "$DATA/bamboo_test.go" \
    --syllables "$DATA/syllables.txt" --words "$DATA/Viet74K.txt" > "$DATA/cases.tsv"
cargo build -q --release --example conformance
KNOWN=tests/conformance/known_differences.tsv ./target/release/examples/conformance < "$DATA/cases.tsv"
