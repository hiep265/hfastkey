//! HFastKey: lõi xử lý gõ tiếng Việt (Telex / VNI).
//!
//! `engine` chứa toàn bộ logic, không phụ thuộc fcitx5.
//! `ffi` xuất giao diện C để lớp nối C++ (addon fcitx5) gọi vào.

pub mod config;
pub mod engine;
pub mod ffi;

pub use config::{Config, ToggleKey};
pub use engine::{Engine, Method};
