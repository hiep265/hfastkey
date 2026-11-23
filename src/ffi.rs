//! Giao diện C cho addon fcitx5 (xem `fcitx5/hfastkey.h`).

use crate::engine::{Engine, Method};
use std::ffi::{c_char, CString};

pub struct VnEngine {
    engine: Engine,
    out: CString,
}

fn method_from(m: u32) -> Method {
    if m == 1 {
        Method::Vni
    } else {
        Method::Telex
    }
}

/// # Safety
/// `p` phải là con trỏ do `hfastkey_new` trả về và chưa bị giải phóng.
unsafe fn get<'a>(p: *mut VnEngine) -> &'a mut VnEngine {
    &mut *p
}

#[no_mangle]
pub extern "C" fn hfastkey_new(method: u32) -> *mut VnEngine {
    Box::into_raw(Box::new(VnEngine {
        engine: Engine::new(method_from(method)),
        out: CString::default(),
    }))
}

/// # Safety
/// `p` là con trỏ từ `hfastkey_new` (hoặc NULL).
#[no_mangle]
pub unsafe extern "C" fn hfastkey_free(p: *mut VnEngine) {
    if !p.is_null() {
        drop(Box::from_raw(p));
    }
}

/// # Safety
/// Xem `get`.
#[no_mangle]
pub unsafe extern "C" fn hfastkey_set_method(p: *mut VnEngine, method: u32) {
    get(p).engine.set_method(method_from(method));
}

/// # Safety
/// Xem `get`.
#[no_mangle]
pub unsafe extern "C" fn hfastkey_set_modern_tone(p: *mut VnEngine, modern: bool) {
    get(p).engine.set_modern_tone(modern);
}

/// # Safety
/// Xem `get`.
#[no_mangle]
pub unsafe extern "C" fn hfastkey_process(p: *mut VnEngine, ch: u32) -> bool {
    match char::from_u32(ch) {
        Some(c) => get(p).engine.process(c),
        None => false,
    }
}

/// # Safety
/// Xem `get`.
#[no_mangle]
pub unsafe extern "C" fn hfastkey_backspace(p: *mut VnEngine) -> bool {
    get(p).engine.backspace()
}

/// # Safety
/// Xem `get`.
#[no_mangle]
pub unsafe extern "C" fn hfastkey_reset(p: *mut VnEngine) {
    get(p).engine.reset();
}

/// # Safety
/// Xem `get`.
#[no_mangle]
pub unsafe extern "C" fn hfastkey_is_empty(p: *mut VnEngine) -> bool {
    get(p).engine.is_empty()
}

/// Chuỗi UTF-8 của từ đang gõ. Con trỏ còn hợp lệ đến lần gọi kế tiếp.
///
/// # Safety
/// Xem `get`.
#[no_mangle]
pub unsafe extern "C" fn hfastkey_text(p: *mut VnEngine) -> *const c_char {
    let e = get(p);
    e.out = CString::new(e.engine.text()).unwrap_or_default();
    e.out.as_ptr()
}
