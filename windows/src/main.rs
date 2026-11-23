//! HFastKey cho Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod logic;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("hfastkey (bản Windows) chỉ chạy trên Windows. Trên Linux hãy dùng hfastkey-daemon.");
    std::process::exit(1);
}
