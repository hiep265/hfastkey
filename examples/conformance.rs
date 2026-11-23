//! Chạy ca test đối chiếu (TSV từ tests/conformance/gen_cases.py) qua lõi HFastKey.
//!
//!     cargo run --release --example conformance < cases.tsv
//!
//! In tỉ lệ đúng theo từng nguồn và các ca sai (tối đa N ca mỗi nguồn, đặt bằng
//! biến môi trường SHOW, mặc định 15). Biến KNOWN trỏ tới danh sách khác biệt
//! đã biết (tests/conformance/known_differences.tsv): các ca này không tính là lỗi.
//! Thoát với mã 1 nếu có ca sai ngoài danh sách đó.

use hfastkey_core::{Engine, Method};
use std::collections::{BTreeMap, HashSet};
use std::io::{self, BufRead};

/// Gõ cả chuỗi phím như người dùng thật: chữ cái/chữ số vào từ đang gõ,
/// ký tự khác (cách, dấu câu) kết thúc từ.
fn type_keys(method: Method, modern: bool, restore: bool, keys: &str) -> String {
    let mut e = Engine::new(method);
    e.set_modern_tone(modern);
    e.set_restore_invalid(restore);
    let mut out = String::new();
    for c in keys.chars() {
        if !e.process(c) {
            out.push_str(&e.text());
            e.reset();
            out.push(c);
        }
    }
    out.push_str(&e.text());
    out
}

#[derive(Default)]
struct Stat {
    total: usize,
    ok: usize,
    known: usize,
    fails: Vec<String>,
}

fn main() {
    let show: usize = std::env::var("SHOW").ok().and_then(|s| s.parse().ok()).unwrap_or(15);
    let known: HashSet<(String, String)> = std::env::var("KNOWN")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| {
            t.lines()
                .filter(|l| !l.starts_with('#'))
                .filter_map(|l| {
                    let mut f = l.split('\t');
                    Some((f.next()?.to_string(), f.next()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut stats: BTreeMap<String, Stat> = BTreeMap::new();
    for line in io::stdin().lock().lines() {
        let line = line.expect("đọc stdin");
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 6 {
            continue;
        }
        let (src, method, modern, restore, keys, expected) = (f[0], f[1], f[2], f[3], f[4], f[5]);
        let method = if method == "vni" { Method::Vni } else { Method::Telex };
        let restore = restore == "1";
        let got = match modern {
            "*" => {
                let old = type_keys(method, false, restore, keys);
                if old == expected {
                    old
                } else {
                    type_keys(method, true, restore, keys)
                }
            }
            m => type_keys(method, m == "1", restore, keys),
        };
        let st = stats.entry(src.to_string()).or_default();
        st.total += 1;
        if got == expected {
            st.ok += 1;
        } else if known.contains(&(src.to_string(), keys.to_string())) {
            st.known += 1;
        } else {
            st.fails.push(format!("    {keys:<22} mong đợi {expected:<16} HFastKey ra {got}"));
        }
    }

    let mut any_fail = false;
    let (mut all, mut all_ok) = (0, 0);
    println!("{:<26} {:>8} {:>8} {:>8} {:>8}", "Nguồn", "Số ca", "Đúng", "Đã biết", "Tỉ lệ");
    for (src, st) in &stats {
        all += st.total;
        all_ok += st.ok;
        println!(
            "{:<26} {:>8} {:>8} {:>8} {:>7.2}%",
            src,
            st.total,
            st.ok,
            st.known,
            100.0 * st.ok as f64 / st.total.max(1) as f64
        );
    }
    let all_known: usize = stats.values().map(|s| s.known).sum();
    println!(
        "{:<26} {:>8} {:>8} {:>8} {:>7.2}%",
        "TỔNG",
        all,
        all_ok,
        all_known,
        100.0 * all_ok as f64 / all.max(1) as f64
    );
    for (src, st) in &stats {
        if st.fails.is_empty() {
            continue;
        }
        any_fail = true;
        println!("\n[{src}] {} ca sai, ví dụ:", st.fails.len());
        for f in st.fails.iter().take(show) {
            println!("{f}");
        }
    }
    std::process::exit(if any_fail { 1 } else { 0 });
}
