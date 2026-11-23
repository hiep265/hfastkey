//! Phần gọi API Windows: hook bàn phím/chuột, gửi phím, biểu tượng khay hệ thống.

use crate::logic::{Key, Mods, State};
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_BACK,
    VK_CAPITAL, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT,
    VK_RWIN, VK_SHIFT, VK_SPACE,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetForegroundWindow, GetMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetWindowsHookExW, TrackPopupMenu, TranslateMessage, HICON, HWND_MESSAGE, ICONINFO,
    KBDLLHOOKSTRUCT, MF_STRING, MSG, TPM_BOTTOMALIGN,
    TPM_RIGHTBUTTON, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_COMMAND, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSW,
};

/// Đánh dấu phím do chính HFastKey gửi, để hook bỏ qua.
const MAGIC: usize = 0x4846_4b59; // "HFKY"
const WM_TRAY: u32 = WM_APP + 1;
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "HFastKey";

const ID_EXIT: usize = 1;

struct App {
    state: State,
    enabled: bool,
    hwnd: HWND,
    icon_on: HICON,
    icon_off: HICON,
    last_foreground: HWND,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn key_down(vk: u16) -> bool {
    unsafe { (GetKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

fn mods() -> Mods {
    Mods {
        ctrl: key_down(VK_CONTROL),
        shift: key_down(VK_SHIFT),
        alt: key_down(VK_MENU),
        win: key_down(VK_LWIN) || key_down(VK_RWIN),
    }
}

fn classify(vk: u16, shift: bool) -> Key {
    match vk {
        VK_LCONTROL | VK_RCONTROL | VK_CONTROL => Key::Ctrl,
        VK_LSHIFT | VK_RSHIFT | VK_SHIFT => Key::Shift,
        VK_LMENU | VK_RMENU | VK_MENU => Key::Alt,
        VK_LWIN | VK_RWIN => Key::Win,
        VK_BACK => Key::Backspace,
        VK_SPACE => Key::Space,
        0x41..=0x5A => {
            let caps = unsafe { GetKeyState(VK_CAPITAL as i32) & 1 != 0 };
            let c = (vk as u8) as char;
            Key::Char(if shift ^ caps { c } else { c.to_ascii_lowercase() })
        }
        0x30..=0x39 if !shift => Key::Char((vk as u8) as char),
        0x60..=0x69 => Key::Char((b'0' + (vk - 0x60) as u8) as char),
        _ => Key::Other,
    }
}

fn kb_input(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: MAGIC },
        },
    }
}

/// Gửi `n` phím Backspace rồi gõ chuỗi `text` (Unicode).
fn send(backspaces: usize, text: &str) {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(backspaces * 2 + text.len() * 4);
    for _ in 0..backspaces {
        inputs.push(kb_input(VK_BACK, 0, 0));
        inputs.push(kb_input(VK_BACK, 0, KEYEVENTF_KEYUP));
    }
    for unit in text.encode_utf16() {
        inputs.push(kb_input(0, unit, KEYEVENTF_UNICODE));
        inputs.push(kb_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    if !inputs.is_empty() {
        unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
        }
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam as *const KBDLLHOOKSTRUCT);
        if info.dwExtraInfo != MAGIC {
            let msg = wparam as u32;
            let vk = info.vkCode as u16;
            let block = with_app(|app| {
                let m = mods();
                let key = classify(vk, m.shift);
                match msg {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        // Đổi cửa sổ thì bắt đầu từ mới.
                        let fg = GetForegroundWindow();
                        if fg != app.last_foreground {
                            app.last_foreground = fg;
                            app.state.reset();
                        }
                        let action = app.state.key_down(&mut app.enabled, key, m);
                        if let Some(on) = action.toggled {
                            app.set_icon(on);
                        }
                        if action.block {
                            send(action.backspaces, &action.text);
                        }
                        action.block
                    }
                    WM_KEYUP | WM_SYSKEYUP => {
                        if let Some(on) = app.state.key_up(&mut app.enabled, key) {
                            app.set_icon(on);
                        }
                        false
                    }
                    _ => false,
                }
            })
            .unwrap_or(false);
            if block {
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wparam as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
        // Bấm chuột có thể đã chuyển con trỏ: bắt đầu từ mới.
        with_app(|app| app.state.reset());
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

const GLYPH_V: [&str; 7] = ["X...X", "X...X", "X...X", "X...X", ".X.X.", ".X.X.", "..X.."];
const GLYPH_E: [&str; 7] = ["XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "XXXXX"];

/// Vẽ biểu tượng V (đỏ) / E (xanh) 32x32.
fn make_icon(vietnamese: bool) -> HICON {
    const SIZE: i32 = 32;
    let (glyph, bg) = if vietnamese { (&GLYPH_V, [0x28u8, 0x28, 0xc6]) } else { (&GLYPH_E, [0xc0u8, 0x65, 0x15]) };
    let scale = 4;
    let (gw, gh) = (5 * scale, 7 * scale);
    let (x0, y0) = ((SIZE - gw) / 2, (SIZE - gh) / 2);
    let r = SIZE / 5;
    let mut bgra = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let cx = if x < r { r - x } else if x >= SIZE - r { x - (SIZE - r - 1) } else { 0 };
            let cy = if y < r { r - y } else if y >= SIZE - r { y - (SIZE - r - 1) } else { 0 };
            let outside = cx > 0 && cy > 0 && cx * cx + cy * cy > r * r;
            let in_glyph = x >= x0
                && x < x0 + gw
                && y >= y0
                && y < y0 + gh
                && glyph[((y - y0) / scale) as usize].as_bytes()[((x - x0) / scale) as usize] == b'X';
            let px = if outside {
                [0, 0, 0, 0]
            } else if in_glyph {
                [0xff, 0xff, 0xff, 0xff]
            } else {
                [bg[0], bg[1], bg[2], 0xff]
            };
            bgra.extend_from_slice(&px);
        }
    }
    let mask = vec![0u8; (SIZE * SIZE / 8) as usize];
    unsafe {
        let color = CreateBitmap(SIZE, SIZE, 1, 32, bgra.as_ptr() as *const _);
        let mask_bmp = CreateBitmap(SIZE, SIZE, 1, 1, mask.as_ptr() as *const _);
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask_bmp, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask_bmp);
        icon
    }
}

fn tray_data(hwnd: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    for (i, c) in tip.encode_utf16().take(nid.szTip.len() - 1).enumerate() {
        nid.szTip[i] = c;
    }
    nid
}

impl App {
    fn tooltip(&self) -> &'static str {
        if self.enabled {
            "HFastKey: Tiếng Việt (Telex). Bấm hoặc Ctrl+Shift để tắt"
        } else {
            "HFastKey: English. Bấm hoặc Ctrl+Shift để bật"
        }
    }

    fn set_icon(&mut self, on: bool) {
        self.enabled = on;
        let icon = if on { self.icon_on } else { self.icon_off };
        let nid = tray_data(self.hwnd, icon, self.tooltip());
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }
}

/// Ghi HFastKey vào danh sách tự chạy khi đăng nhập Windows.
fn register_autostart() {
    let mut key: HKEY = null_mut();
    let path = wide(RUN_KEY);
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_SET_VALUE, &mut key) != 0 {
            return;
        }
        if let Ok(exe) = std::env::current_exe() {
            let name = wide(RUN_VALUE);
            let val = wide(&format!("\"{}\"", exe.display()));
            RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, val.as_ptr() as *const u8, (val.len() * 2) as u32);
        }
        RegCloseKey(key);
    }
}

/// Chuột phải: chỉ có mục Thoát.
fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        let t = wide("Thoát HFastKey");
        AppendMenuW(menu, MF_STRING, ID_EXIT, t.as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, null());
        DestroyMenu(menu);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP => {
                    with_app(|a| {
                        let on = !a.enabled;
                        a.state.reset();
                        a.set_icon(on);
                    });
                }
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            if wparam & 0xffff == ID_EXIT {
                PostQuitMessage(0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn run() {
    unsafe {
        // Chỉ cho chạy một bản.
        let mutex_name = wide("Local\\HFastKeySingleInstance");
        CreateMutexW(null(), 0, mutex_name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        let hinst = GetModuleHandleW(null());
        let class = wide("HFastKeyWindow");
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let title = wide("HFastKey");
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            hinst,
            null(),
        );

        register_autostart();
        let app = App {
            state: State::new(),
            enabled: true,
            hwnd,
            icon_on: make_icon(true),
            icon_off: make_icon(false),
            last_foreground: null_mut(),
        };
        let nid = tray_data(hwnd, if app.enabled { app.icon_on } else { app.icon_off }, app.tooltip());
        Shell_NotifyIconW(NIM_ADD, &nid);
        APP.with(|a| *a.borrow_mut() = Some(app));

        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), hinst, 0);
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), hinst, 0);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let nid = tray_data(hwnd, null_mut(), "");
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}
