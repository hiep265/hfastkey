"""Sinh ca test đối chiếu cho HFastKey từ các nguồn bên ngoài.

Đầu ra (TSV, mỗi dòng một ca):
    nguồn  kiểu_gõ  dấu_kiểu_mới  khôi_phục_từ_sai  phím  kết_quả_mong_đợi
với kiểu_gõ = telex|vni; các cờ = 0|1; dấu_kiểu_mới = * nghĩa là chấp nhận cả hai kiểu.

Cách dùng:
    python3 -I gen_cases.py --unikey testunikey.cpp --bamboo bamboo_test.go \
        --syllables syllables.txt --words Viet74K.txt > cases.tsv

Dữ liệu nguồn KHÔNG nằm trong repo (khác giấy phép); scripts/conformance.sh tải về khi chạy.
"""

import argparse
import re
import sys
import unicodedata

ROWS = {
    ("a", ""): "aáàảãạ",
    ("a", "breve"): "ăắằẳẵặ",
    ("a", "hat"): "âấầẩẫậ",
    ("e", ""): "eéèẻẽẹ",
    ("e", "hat"): "êếềểễệ",
    ("i", ""): "iíìỉĩị",
    ("o", ""): "oóòỏõọ",
    ("o", "hat"): "ôốồổỗộ",
    ("o", "horn"): "ơớờởỡợ",
    ("u", ""): "uúùủũụ",
    ("u", "horn"): "ưứừửữự",
    ("y", ""): "yýỳỷỹỵ",
}
DECOMP = {}
for (base, mark), row in ROWS.items():
    for tone, ch in enumerate(row):
        DECOMP[ch] = (base, mark, tone)
DECOMP["đ"] = ("d", "stroke", 0)

TELEX_TONE = ["", "s", "f", "r", "x", "j"]
VNI_TONE = ["", "1", "2", "3", "4", "5"]


def decompose(ch):
    """(chữ gốc có hoa/thường, dấu phụ, thanh) hoặc None nếu không phải chữ tiếng Việt."""
    low = ch.lower()
    upper = ch != low
    if low in DECOMP:
        base, mark, tone = DECOMP[low]
    elif "a" <= low <= "z":
        base, mark, tone = low, "", 0
    else:
        return None
    return (base.upper() if upper else base, mark, tone)


def mark_key(method, base, mark):
    if not mark:
        return ""
    if method == "telex":
        return {"hat": base, "breve": "w", "horn": "w", "stroke": "d"}[mark]
    return {"hat": "6", "breve": "8", "horn": "7", "stroke": "9"}[mark]


def syllable_keys(method, syl, variant):
    """Chuỗi phím để gõ một âm tiết. variant: end | after | w_end."""
    parts = [decompose(c) for c in syl]
    if any(p is None for p in parts):
        return None
    tone_keys = TELEX_TONE if method == "telex" else VNI_TONE
    tone = next((p[2] for p in parts if p[2]), 0)
    tone_pos = next((i for i, p in enumerate(parts) if p[2]), None)
    low = syl.lower()
    has_uo = "ươ" in low
    out = []
    for i, (base, mark, _t) in enumerate(parts):
        # Telex: "oo" (xoong) phải gõ "ooo", vì "oo" nghĩa là "ô".
        if (method == "telex" and i > 0 and not mark and not parts[i - 1][1]
                and base.lower() in "aeo" and parts[i - 1][0].lower() == base.lower()):
            out.append(base)
        out.append(base)
        if variant == "w_end" and has_uo and mark == "horn":
            pass  # gõ "uo" rồi một phím w ở cuối: nguoiwf
        else:
            out.append(mark_key(method, base, mark))
        if variant == "after" and i == tone_pos:
            out.append(tone_keys[tone])
    if variant == "w_end" and has_uo:
        out.append("w" if method == "telex" else "7")
    if variant != "after":
        out.append(tone_keys[tone])
    return "".join(out)


def phrase_keys(method, phrase, variant="end"):
    out = []
    for token in re.split(r"(\W+)", phrase):
        if not token:
            continue
        if re.fullmatch(r"\W+", token):
            out.append(token)
            continue
        k = syllable_keys(method, token, variant)
        if k is None:
            return None
        out.append(k)
    return "".join(out)


def emit(src, method, modern, restore, keys, expected):
    print(f"{src}\t{method}\t{modern}\t{restore}\t{keys}\t{expected}")


def from_unikey(path):
    # Bảng expectedTelexData; Unikey chạy với SpellCheck=False, AutoNonVnRestore=False,
    # ModernStyle=False (mặc định). Bỏ các dòng đã bị chính Unikey comment lại.
    for line in open(path, encoding="utf-8"):
        m = re.match(r'^\s*\{"([^"]*)", "([^"]*)"\},', line)
        if m:
            emit("unikey", "telex", 0, 0, m.group(1), m.group(2))


def from_bamboo(path):
    text = open(path, encoding="utf-8").read()
    for func in re.split(r"\nfunc ", text)[1:]:
        if "Microsoft layout" in func:
            continue
        method = "vni" if '"VNI"' in func else "telex"
        modern = 0 if "&^EstdToneStyle" in func else 1
        acc, complex_ = "", False
        for stmt in func.splitlines():
            s = stmt.strip()
            if s.startswith("ng.Reset()"):
                acc, complex_ = "", False
            elif m := re.match(r'ng\.ProcessString\("((?:[^"\\]|\\.)*)", VietnameseMode\)', s):
                acc += m.group(1)
            elif s.startswith("ng.") and not s.startswith(("ng.GetProcessedString", "ng.IsValid")):
                complex_ = True
            elif m := re.match(r'if ng\.GetProcessedString\(VietnameseMode\) != "((?:[^"\\]|\\.)*)"', s):
                if acc and not complex_ and not re.search(r"[\[\]{}]", acc):
                    emit("bamboo", method, modern, 0, acc, m.group(1))


def from_syllables(path):
    for line in open(path, encoding="utf-8"):
        syl = unicodedata.normalize("NFC", line.strip())
        if not syl or " " in syl:
            continue
        for method in ("telex", "vni"):
            variants = ["end", "after"] + (["w_end"] if "ươ" in syl.lower() else [])
            for v in variants:
                keys = syllable_keys(method, syl, v)
                if keys:
                    emit(f"syllable-{method}-{v}", method, "*", 1, keys, syl)
            cap = syl[0].upper() + syl[1:]
            keys = syllable_keys(method, cap, "end")
            if keys:
                emit(f"syllable-{method}-capital", method, "*", 1, keys, cap)


def from_words(path, syllables):
    for line in open(path, encoding="utf-8"):
        phrase = unicodedata.normalize("NFC", line.strip())
        if not phrase:
            continue
        sylls = [t for t in re.split(r"\W+", phrase) if t]
        # Chỉ lấy cụm gồm toàn âm tiết tiếng Việt hợp lệ (bỏ từ mượn như "fax").
        if not sylls or any(s.lower() not in syllables for s in sylls):
            continue
        for method in ("telex", "vni"):
            keys = phrase_keys(method, phrase)
            if keys:
                emit(f"words-{method}", method, "*", 1, keys, phrase)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--unikey")
    ap.add_argument("--bamboo")
    ap.add_argument("--syllables")
    ap.add_argument("--words")
    a = ap.parse_args()
    if a.unikey:
        from_unikey(a.unikey)
    if a.bamboo:
        from_bamboo(a.bamboo)
    syllables = set()
    if a.syllables:
        syllables = {unicodedata.normalize("NFC", l.strip()).lower() for l in open(a.syllables, encoding="utf-8")}
        from_syllables(a.syllables)
    if a.words:
        from_words(a.words, syllables)


if __name__ == "__main__":
    sys.exit(main())
