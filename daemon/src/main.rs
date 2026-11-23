//! hfastkey-daemon: bộ gõ tiếng Việt Telex cho Linux, viết hoàn toàn bằng Rust.
//!
//! Chương trình tự đóng vai IBus trên session D-Bus (giống cách fcitx5 làm),
//! nên gnome-shell và các ứng dụng GTK/Qt gửi phím thẳng tới đây, không cần
//! fcitx5 hay ibus-daemon. Chạy một luồng, không dùng thư viện ngoài, để tốn
//! ít bộ nhớ nhất.
//!
//! Bật/tắt tiếng Việt: Ctrl+Shift, hoặc bấm vào biểu tượng V/E.

mod dbus;
mod keys;

use dbus::{Conn, Msg, METHOD_CALL, SIGNAL, V};
use keys::{KeyState, Out};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const USAGE: &str = "\
hfastkey-daemon: bộ gõ tiếng Việt Telex viết bằng Rust

Bật/tắt tiếng Việt: Ctrl+Shift, hoặc bấm vào biểu tượng V/E.

Cách dùng: hfastkey-daemon [tuỳ chọn]
  --off               khởi động ở chế độ tắt tiếng Việt
  --no-tray           không hiện biểu tượng V/E
  --debug             in log từng phím
  --no-address-file   không ghi ~/.config/ibus/bus/* (dùng khi test)";

const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const IBUS_NAME: &str = "org.freedesktop.IBus";
const PORTAL_NAME: &str = "org.freedesktop.portal.IBus";
const IBUS_PATH: &str = "/org/freedesktop/IBus";
const IC_PREFIX: &str = "/org/freedesktop/IBus/InputContext_";
const IC_IFACE: &str = "org.freedesktop.IBus.InputContext";
const PROPS: &str = "org.freedesktop.DBus.Properties";
const SNI_PATH: &str = "/StatusNotifierItem";
const SNI_IFACE: &str = "org.kde.StatusNotifierItem";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const GNOME_IBUS_SERVICE: &str = "org.freedesktop.IBus.session.GNOME.service";
const CAP_SURROUNDING_TEXT: u32 = 1 << 5;
const ERR_UNKNOWN: &str = "org.freedesktop.DBus.Error.UnknownMethod";
const ERR_PROP: &str = "org.freedesktop.DBus.Error.UnknownProperty";

macro_rules! log {
    ($($t:tt)*) => { eprintln!("hfastkey: {}", format_args!($($t)*)) };
}

struct Args {
    off: bool,
    tray: bool,
    debug: bool,
    address_file: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        off: false,
        tray: true,
        debug: std::env::var_os("HFASTKEY_DEBUG").is_some(),
        address_file: true,
    };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--off" => a.off = true,
            "--no-tray" => a.tray = false,
            "--debug" => a.debug = true,
            "--no-address-file" => a.address_file = false,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => {
                eprintln!("Tuỳ chọn không hợp lệ: {other}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    a
}

// ---------- ô nhập liệu ----------

struct Ic {
    id: u32,
    /// Tên kết nối của ứng dụng (tín hiệu chỉ gửi cho nó).
    owner: String,
    keys: KeyState,
    caps: u32,
    client_commit_preedit: bool,
    post_process: bool,
    content_type: (u32, u32),
}

/// IBusText: (sa{sv}sv) với v = IBusAttrList (sa{sv}av).
fn ibus_text(s: &str, underline: bool) -> V {
    let n = s.chars().count() as u32;
    let mut attrs = Vec::new();
    if underline && n > 0 {
        // IBUS_ATTR_TYPE_UNDERLINE, IBUS_ATTR_UNDERLINE_SINGLE, từ 0 đến n.
        attrs.push(V::var(V::Struct(vec![
            V::s("IBusAttribute"),
            V::empty_dict(),
            V::U32(1),
            V::U32(1),
            V::U32(0),
            V::U32(n),
        ])));
    }
    let attr_list = V::Struct(vec![V::s("IBusAttrList"), V::empty_dict(), V::Array("v".into(), attrs)]);
    V::var(V::Struct(vec![V::s("IBusText"), V::empty_dict(), V::s(s), V::var(attr_list)]))
}

/// Lấy chuỗi trong IBusText.
fn ibus_text_string(v: &V) -> Option<&str> {
    match v.inner() {
        V::Struct(f) => f.get(2).and_then(V::as_str),
        _ => None,
    }
}

fn ic_signal(conn: &mut Conn, ic: &Ic, member: &str, body: &[V]) -> io::Result<()> {
    let path = format!("{IC_PREFIX}{}", ic.id);
    conn.signal(&ic.owner, &path, IC_IFACE, member, body)
}

/// Gửi các việc cần báo cho ứng dụng. Trả về Some(bật/tắt) nếu có đổi trạng thái.
fn emit(conn: &mut Conn, ic: &Ic, out: Vec<Out>) -> io::Result<Option<bool>> {
    let mut toggled = None;
    for o in out {
        match o {
            Out::Delete(n) => ic_signal(conn, ic, "DeleteSurroundingText", &[V::I32(-(n as i32)), V::U32(n)])?,
            Out::Commit(s) => ic_signal(conn, ic, "CommitText", &[ibus_text(&s, false)])?,
            Out::Preedit(s) => {
                let cursor = V::U32(s.chars().count() as u32);
                let visible = V::Bool(!s.is_empty());
                let text = ibus_text(&s, true);
                if ic.client_commit_preedit {
                    // mode 1 = COMMIT: ứng dụng tự chốt chữ khi mất focus/bấm chỗ khác.
                    ic_signal(conn, ic, "UpdatePreeditTextWithMode", &[text, cursor, visible, V::U32(1)])?
                } else {
                    ic_signal(conn, ic, "UpdatePreeditText", &[text, cursor, visible])?
                }
            }
            Out::Forward { keyval, keycode, state } => {
                ic_signal(conn, ic, "ForwardKeyEvent", &[V::U32(keyval), V::U32(keycode), V::U32(state)])?
            }
            Out::Toggled(on) => toggled = Some(on),
        }
    }
    Ok(toggled)
}

/// Mất focus / reset. Gõ thẳng: chữ đã nằm trong ứng dụng, chỉ cần quên từ.
/// Gạch chân: ứng dụng nào tự chốt chữ (gnome-shell) thì thôi, ứng dụng khác
/// thì mình chốt hộ để không mất chữ.
fn finish_word(conn: &mut Conn, ic: &mut Ic) -> io::Result<()> {
    if !ic.keys.has_word() || ic.keys.word_is_direct() {
        ic.keys.clear();
        return Ok(());
    }
    let word = ic.keys.take_word();
    ic.keys.clear();
    if !ic.client_commit_preedit {
        emit(conn, ic, vec![Out::Commit(word), Out::Preedit(String::new())])?;
    }
    Ok(())
}

const IC_PROPS: [&str; 4] = ["ContentType", "ClientCommitPreedit", "EffectivePostProcessKeyEvent", "PostProcessKeyEvent"];

fn ic_property(ic: &Ic, name: &str) -> Option<V> {
    Some(match name {
        "ContentType" => V::Struct(vec![V::U32(ic.content_type.0), V::U32(ic.content_type.1)]),
        "ClientCommitPreedit" => V::Struct(vec![V::Bool(ic.client_commit_preedit)]),
        "EffectivePostProcessKeyEvent" => V::Struct(vec![V::Bool(ic.post_process)]),
        "PostProcessKeyEvent" => V::Struct(vec![V::Array("(yv)".into(), Vec::new())]),
        _ => return None,
    })
}

fn bool_field(v: &V) -> bool {
    match v.inner() {
        V::Struct(f) => matches!(f.first(), Some(V::Bool(true))),
        V::Bool(b) => *b,
        _ => false,
    }
}

fn props_dict(names: &[&str], get: impl Fn(&str) -> Option<V>) -> V {
    let items = names
        .iter()
        .filter_map(|n| get(n).map(|v| V::Struct(vec![V::s(n), V::var(v)])))
        .collect();
    V::Array("{sv}".into(), items)
}

// ---------- biểu tượng V/E ----------

const GLYPH_V: [&str; 7] = ["X...X", "X...X", "X...X", "X...X", ".X.X.", ".X.X.", "..X.."];
const GLYPH_E: [&str; 7] = ["XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "XXXXX"];

/// Ô vuông bo góc có chữ V (đỏ) hoặc E (xanh), ARGB32 (iiay).
fn icon(size: i32, vietnamese: bool) -> V {
    let (glyph, bg) = if vietnamese { (&GLYPH_V, [0xc6, 0x28, 0x28]) } else { (&GLYPH_E, [0x15, 0x65, 0xc0]) };
    let scale = (size / 8).max(1);
    let (gw, gh) = (5 * scale, 7 * scale);
    let (x0, y0) = ((size - gw) / 2, (size - gh) / 2);
    let r = size / 5;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let cx = if x < r { r - x } else if x >= size - r { x - (size - r - 1) } else { 0 };
            let cy = if y < r { r - y } else if y >= size - r { y - (size - r - 1) } else { 0 };
            let outside = cx > 0 && cy > 0 && cx * cx + cy * cy > r * r;
            let in_glyph = x >= x0
                && x < x0 + gw
                && y >= y0
                && y < y0 + gh
                && glyph[((y - y0) / scale) as usize].as_bytes()[((x - x0) / scale) as usize] == b'X';
            data.extend_from_slice(&if outside {
                [0, 0, 0, 0]
            } else if in_glyph {
                [0xff, 0xff, 0xff, 0xff]
            } else {
                [0xff, bg[0], bg[1], bg[2]]
            });
        }
    }
    V::Struct(vec![V::I32(size), V::I32(size), V::Bytes(data)])
}

const SNI_PROPS: [&str; 16] = [
    "Category", "Id", "Title", "Status", "WindowId", "IconThemePath", "IconName", "IconPixmap",
    "OverlayIconName", "OverlayIconPixmap", "AttentionIconName", "AttentionIconPixmap", "AttentionMovieName",
    "ToolTip", "ItemIsMenu", "Menu",
];

fn sni_property(name: &str, on: bool) -> Option<V> {
    let state = if on { "Tiếng Việt (Telex)" } else { "English" };
    Some(match name {
        "Category" => V::s("ApplicationStatus"),
        "Id" => V::s("hfastkey"),
        "Title" => V::s("HFastKey"),
        "Status" => V::s("Active"),
        "WindowId" => V::I32(0),
        "IconThemePath" | "IconName" | "OverlayIconName" | "AttentionIconName" | "AttentionMovieName" => V::s(""),
        "IconPixmap" => V::Array("(iiay)".into(), vec![icon(16, on), icon(32, on)]),
        "OverlayIconPixmap" | "AttentionIconPixmap" => V::Array("(iiay)".into(), Vec::new()),
        "ToolTip" => V::Struct(vec![
            V::s(""),
            V::Array("(iiay)".into(), Vec::new()),
            V::s(&format!("HFastKey: {state}")),
            V::s("Bấm vào đây hoặc Ctrl+Shift để bật/tắt"),
        ]),
        "ItemIsMenu" => V::Bool(false),
        "Menu" => V::Path("/NO_DBUSMENU".into()),
        _ => return None,
    })
}

// ---------- introspection ----------

const INTROSPECT_ROOT: &str = r#"<node><interface name="org.freedesktop.IBus"><method name="CreateInputContext"><arg type="s" direction="in"/><arg type="o" direction="out"/></method></interface><interface name="org.freedesktop.IBus.Portal"><method name="CreateInputContext"><arg type="s" direction="in"/><arg type="o" direction="out"/></method></interface></node>"#;

const INTROSPECT_IC: &str = r#"<node><interface name="org.freedesktop.IBus.InputContext"><method name="ProcessKeyEvent"><arg type="u" direction="in"/><arg type="u" direction="in"/><arg type="u" direction="in"/><arg type="b" direction="out"/></method><method name="SetCursorLocation"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="SetCursorLocationRelative"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="ProcessHandWritingEvent"><arg type="ad" direction="in"/></method><method name="CancelHandWriting"><arg type="u" direction="in"/></method><method name="FocusIn"/><method name="FocusOut"/><method name="Reset"/><method name="Enable"/><method name="Disable"/><method name="IsEnabled"><arg type="b" direction="out"/></method><method name="SetCapabilities"><arg type="u" direction="in"/></method><method name="PropertyActivate"><arg type="s" direction="in"/><arg type="u" direction="in"/></method><method name="SetEngine"><arg type="s" direction="in"/></method><method name="GetEngine"><arg type="v" direction="out"/></method><method name="SetSurroundingText"><arg type="v" direction="in"/><arg type="u" direction="in"/><arg type="u" direction="in"/></method><signal name="CommitText"><arg type="v"/></signal><signal name="ForwardKeyEvent"><arg type="u"/><arg type="u"/><arg type="u"/></signal><signal name="UpdatePreeditText"><arg type="v"/><arg type="u"/><arg type="b"/></signal><signal name="UpdatePreeditTextWithMode"><arg type="v"/><arg type="u"/><arg type="b"/><arg type="u"/></signal><signal name="DeleteSurroundingText"><arg type="i"/><arg type="u"/></signal><signal name="RequireSurroundingText"/><property name="ContentType" type="(uu)" access="readwrite"/><property name="ClientCommitPreedit" type="(b)" access="readwrite"/><property name="EffectivePostProcessKeyEvent" type="(b)" access="readwrite"/><property name="PostProcessKeyEvent" type="(a(yv))" access="read"/></interface><interface name="org.freedesktop.IBus.Service"><method name="Destroy"/></interface></node>"#;

const INTROSPECT_SNI: &str = r#"<node><interface name="org.kde.StatusNotifierItem"><method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method><signal name="NewIcon"/><signal name="NewToolTip"/><signal name="NewStatus"><arg type="s"/></signal><property name="Category" type="s" access="read"/><property name="Id" type="s" access="read"/><property name="Title" type="s" access="read"/><property name="Status" type="s" access="read"/><property name="WindowId" type="i" access="read"/><property name="IconName" type="s" access="read"/><property name="IconPixmap" type="a(iiay)" access="read"/><property name="ToolTip" type="(sa(iiay)ss)" access="read"/><property name="ItemIsMenu" type="b" access="read"/><property name="Menu" type="o" access="read"/></interface></node>"#;

// ---------- chương trình ----------

struct App {
    conn: Conn,
    ics: Vec<Ic>,
    next_id: u32,
    enabled: bool,
    debug: bool,
    tray: bool,
    sni_name: String,
}

impl App {
    fn set_enabled(&mut self, on: bool) -> io::Result<()> {
        self.enabled = on;
        log!("tiếng Việt {}", if on { "bật" } else { "tắt" });
        if self.tray {
            self.conn.signal("", SNI_PATH, SNI_IFACE, "NewIcon", &[])?;
            self.conn.signal("", SNI_PATH, SNI_IFACE, "NewToolTip", &[])?;
        }
        Ok(())
    }

    fn register_tray(&mut self) -> io::Result<()> {
        let name = V::s(&self.sni_name);
        self.conn.call_noreply(WATCHER, "/StatusNotifierWatcher", WATCHER, "RegisterStatusNotifierItem", &[name])
    }

    fn handle(&mut self, m: Msg) -> io::Result<()> {
        match m.kind {
            METHOD_CALL => self.handle_call(m),
            SIGNAL => self.handle_signal(m),
            _ => Ok(()),
        }
    }

    fn handle_signal(&mut self, m: Msg) -> io::Result<()> {
        if m.sender != BUS {
            return Ok(());
        }
        match m.member.as_str() {
            "NameOwnerChanged" => {
                let (name, new_owner) = (m.arg_str(0), m.arg_str(2));
                if name == IBUS_NAME && new_owner != self.conn.unique {
                    log!("chương trình khác ({new_owner}) đã chiếm {IBUS_NAME}, thoát.");
                    std::process::exit(0);
                }
                if name == WATCHER && !new_owner.is_empty() && self.tray {
                    self.register_tray()?;
                }
                if new_owner.is_empty() && name.starts_with(':') {
                    // Ứng dụng đã thoát: bỏ các ô nhập liệu của nó.
                    self.ics.retain(|ic| ic.owner != name);
                }
            }
            "NameLost" if m.arg_str(0) == IBUS_NAME => {
                log!("mất tên {IBUS_NAME}, thoát.");
                std::process::exit(0);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_call(&mut self, m: Msg) -> io::Result<()> {
        let member = m.member.as_str();
        if m.iface == "org.freedesktop.DBus.Peer" {
            return match member {
                "Ping" => self.conn.reply(&m, &[]),
                "GetMachineId" => self.conn.reply(&m, &[V::s(&machine_id())]),
                _ => self.conn.error(&m, ERR_UNKNOWN, member),
            };
        }
        if member == "Introspect" {
            let xml = if m.path == IBUS_PATH {
                INTROSPECT_ROOT
            } else if m.path.starts_with(IC_PREFIX) {
                INTROSPECT_IC
            } else if m.path == SNI_PATH && self.tray {
                INTROSPECT_SNI
            } else {
                "<node/>"
            };
            return self.conn.reply(&m, &[V::s(xml)]);
        }
        if m.path == IBUS_PATH && member == "CreateInputContext" {
            let id = self.next_id;
            self.next_id += 1;
            if self.debug {
                log!("tạo ô nhập liệu {id} cho {:?} ({})", m.arg_str(0), m.sender);
            }
            self.ics.push(Ic {
                id,
                owner: m.sender.clone(),
                keys: KeyState::new(),
                caps: 0,
                client_commit_preedit: false,
                post_process: false,
                content_type: (0, 0),
            });
            return self.conn.reply(&m, &[V::Path(format!("{IC_PREFIX}{id}"))]);
        }
        if let Some(id) = m.path.strip_prefix(IC_PREFIX).and_then(|s| s.parse::<u32>().ok()) {
            if let Some(idx) = self.ics.iter().position(|ic| ic.id == id) {
                return self.handle_ic(idx, m);
            }
        }
        if m.path == SNI_PATH && self.tray {
            return self.handle_sni(m);
        }
        self.conn.error(&m, ERR_UNKNOWN, &format!("{} {}", m.path, member))
    }

    fn handle_sni(&mut self, m: Msg) -> io::Result<()> {
        let on = self.enabled;
        match m.member.as_str() {
            "Activate" => {
                // Bấm chuột trái: bật/tắt tiếng Việt.
                self.conn.reply(&m, &[])?;
                for ic in &mut self.ics {
                    ic.keys.clear();
                }
                self.set_enabled(!on)
            }
            "SecondaryActivate" | "ContextMenu" | "Scroll" => self.conn.reply(&m, &[]),
            "GetAll" => self.conn.reply(&m, &[props_dict(&SNI_PROPS, |n| sni_property(n, on))]),
            "Get" => match sni_property(m.arg_str(1), on) {
                Some(v) => self.conn.reply(&m, &[V::var(v)]),
                None => self.conn.error(&m, ERR_PROP, m.arg_str(1)),
            },
            other => self.conn.error(&m, ERR_UNKNOWN, other),
        }
    }

    fn handle_ic(&mut self, idx: usize, m: Msg) -> io::Result<()> {
        let debug = self.debug;
        let ic = &mut self.ics[idx];
        let conn = &mut self.conn;
        let member = m.member.as_str();
        if m.iface == PROPS {
            return match member {
                "Get" => match ic_property(ic, m.arg_str(1)) {
                    Some(v) => conn.reply(&m, &[V::var(v)]),
                    None => conn.error(&m, ERR_PROP, m.arg_str(1)),
                },
                "GetAll" => conn.reply(&m, &[props_dict(&IC_PROPS, |n| ic_property(ic, n))]),
                "Set" => {
                    let v = m.body.get(2).map(V::inner).cloned().unwrap_or(V::Bool(false));
                    match m.arg_str(1) {
                        "ClientCommitPreedit" => ic.client_commit_preedit = bool_field(&v),
                        "EffectivePostProcessKeyEvent" => ic.post_process = bool_field(&v),
                        "ContentType" => {
                            if let V::Struct(f) = &v {
                                let g = |i: usize| f.get(i).and_then(V::as_u32).unwrap_or(0);
                                ic.content_type = (g(0), g(1));
                            }
                        }
                        _ => {}
                    }
                    conn.reply(&m, &[])
                }
                other => conn.error(&m, ERR_UNKNOWN, other),
            };
        }
        match member {
            "ProcessKeyEvent" => {
                let (keyval, keycode, state) = (m.arg_u32(0), m.arg_u32(1), m.arg_u32(2));
                let surrounding = ic.caps & CAP_SURROUNDING_TEXT != 0;
                let mut on = self.enabled;
                let mut out = Vec::new();
                let handled = ic.keys.key(surrounding, &mut on, keyval, keycode, state, &mut out);
                if debug {
                    log!("phím {keyval:#x} mã {keycode} trạng thái {state:#x} caps {:#x} -> {handled} {out:?}", ic.caps);
                }
                // Tín hiệu được ghi ra trước câu trả lời, nên ứng dụng luôn nhận
                // chữ trước khi biết kết quả xử lý phím.
                let toggled = emit(conn, ic, out)?;
                conn.reply(&m, &[V::Bool(handled)])?;
                if let Some(on) = toggled {
                    self.set_enabled(on)?;
                }
                Ok(())
            }
            "FocusIn" => {
                ic.keys.forget_surrounding();
                conn.reply(&m, &[])
            }
            "FocusOut" => {
                finish_word(conn, ic)?;
                ic.keys.forget_surrounding();
                conn.reply(&m, &[])
            }
            "Reset" => {
                finish_word(conn, ic)?;
                conn.reply(&m, &[])
            }
            "SetCapabilities" => {
                let caps = m.arg_u32(0);
                let had = ic.caps & CAP_SURROUNDING_TEXT != 0;
                ic.caps = caps;
                if caps & CAP_SURROUNDING_TEXT != 0 && !had {
                    // Thư viện IBus chỉ gửi chữ quanh con trỏ sau khi bộ gõ yêu cầu.
                    ic_signal(conn, ic, "RequireSurroundingText", &[])?;
                }
                conn.reply(&m, &[])
            }
            "SetSurroundingText" => {
                if let Some(t) = m.body.first().and_then(ibus_text_string) {
                    let before: String = t.chars().take(m.arg_u32(1) as usize).collect();
                    if debug {
                        log!("chữ trước con trỏ: {before:?}");
                    }
                    ic.keys.on_surrounding(before);
                }
                conn.reply(&m, &[])
            }
            "IsEnabled" => conn.reply(&m, &[V::Bool(true)]),
            "GetEngine" => conn.reply(&m, &[V::var(V::I32(0))]),
            "Destroy" => {
                conn.reply(&m, &[])?;
                self.ics.remove(idx);
                Ok(())
            }
            "SetCursorLocation" | "SetCursorLocationRelative" | "ProcessHandWritingEvent" | "CancelHandWriting"
            | "Enable" | "Disable" | "PropertyActivate" | "SetEngine" => conn.reply(&m, &[]),
            other => conn.error(&m, ERR_UNKNOWN, other),
        }
    }
}

// ---------- giành tên IBus, file địa chỉ ----------

fn machine_id() -> String {
    for p in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(s) = fs::read_to_string(p) {
            let s = s.trim();
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    "machine-id".to_string()
}

/// Các file mà thư viện IBus đọc để tìm địa chỉ bus (cả Wayland lẫn X11).
fn address_files() -> Vec<PathBuf> {
    if let Some(p) = std::env::var_os("IBUS_ADDRESS_FILE") {
        return vec![PathBuf::from(p)];
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    let dir = config.join("ibus").join("bus");
    let mid = machine_id();
    let wayland = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .and_then(|d| d.rsplit('/').next().map(str::to_string))
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "wayland-0".to_string());
    let (host, num) = match std::env::var("DISPLAY") {
        Ok(d) => match d.split_once(':') {
            Some((h, rest)) => (h.to_string(), rest.split('.').next().unwrap_or("0").to_string()),
            None => (d, "0".to_string()),
        },
        Err(_) => (String::new(), "0".to_string()),
    };
    let host = if host.is_empty() { "unix".to_string() } else { host };
    let mut v = vec![dir.join(format!("{mid}-unix-{wayland}")), dir.join(format!("{mid}-{host}-{num}"))];
    v.dedup();
    v
}

fn write_address_files() -> String {
    let mut addr = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| {
        let rt = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".into());
        format!("unix:path={rt}/bus")
    });
    while addr.ends_with(';') {
        addr.pop();
    }
    // Thêm một tham số ngẫu nhiên để thư viện IBus thấy địa chỉ đổi và kết nối lại.
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let pid = std::process::id();
    let content = format!("IBUS_ADDRESS={addr},hfastkey_random_string={nanos:x}{pid:x}\nIBUS_DAEMON_PID={pid}\n");
    write_files(&content);
    content
}

fn write_files(content: &str) {
    for path in address_files() {
        let res = (|| -> io::Result<()> {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("hfastkey-tmp");
            let mut f = fs::File::create(&tmp)?;
            f.set_permissions(fs::Permissions::from_mode(0o600))?;
            f.write_all(content.as_bytes())?;
            f.sync_all()?;
            fs::rename(&tmp, &path)
        })();
        match res {
            Ok(()) => log!("đã ghi địa chỉ vào {}", path.display()),
            Err(e) => log!("không ghi được {}: {e}", path.display()),
        }
    }
}

fn file_owned_by_ibus_daemon(content: &str) -> bool {
    let pid = content
        .lines()
        .find_map(|l| l.strip_prefix("IBUS_DAEMON_PID="))
        .and_then(|p| p.trim().parse::<u32>().ok());
    match pid {
        Some(pid) if pid > 0 => fs::read(format!("/proc/{pid}/cmdline"))
            .map(|c| String::from_utf8_lossy(&c).contains("ibus-daemon"))
            .unwrap_or(false),
        _ => false,
    }
}

/// Giành lại file địa chỉ nếu ibus-daemon khởi động sau và ghi đè.
fn check_address_files(ours: &str) {
    let mut stale = false;
    for path in address_files() {
        let current = fs::read_to_string(&path).unwrap_or_default();
        if current != ours {
            if file_owned_by_ibus_daemon(&current) {
                log!("ibus-daemon vừa ghi đè {}, yêu cầu nó thoát.", path.display());
                stop_gnome_ibus_service();
                let _ = Command::new("ibus").arg("exit").output();
            }
            stale = true;
        }
    }
    if stale {
        write_files(ours);
    }
}

/// Dừng ibus-daemon mà GNOME tự bật (dịch vụ systemd của người dùng).
fn stop_gnome_ibus_service() {
    let active = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", GNOME_IBUS_SERVICE])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if active {
        log!("dừng ibus của GNOME ({GNOME_IBUS_SERVICE})");
        let _ = Command::new("systemctl").args(["--user", "stop", GNOME_IBUS_SERVICE]).status();
    }
}

fn bus_call(conn: &mut Conn, member: &str, body: &[V]) -> io::Result<Msg> {
    conn.call(BUS, BUS_PATH, BUS, member, body)
}

/// Giành tên org.freedesktop.IBus. Xếp hàng chờ tên (không DoNotQueue): khi
/// chương trình đang giữ tên thoát, D-Bus chuyển tên ngay cho HFastKey, không
/// có khoảng trống để D-Bus tự bật lại ibus-daemon.
fn acquire_ibus_name(conn: &mut Conn) -> io::Result<bool> {
    // 2 = REPLACE_EXISTING. Trả lời: 1 = chủ chính, 2 = xếp hàng, 3 = đã có chủ, 4 = đã là chủ.
    let r = bus_call(conn, "RequestName", &[V::s(IBUS_NAME), V::U32(2)])?;
    match r.arg_u32(0) {
        1 | 4 => return Ok(true),
        2 => {}
        x => {
            log!("không xin được tên {IBUS_NAME} (mã {x})");
            return Ok(false);
        }
    }
    for attempt in 0..40 {
        let owner = bus_call(conn, "GetNameOwner", &[V::s(IBUS_NAME)])
            .map(|m| m.arg_str(0).to_string())
            .unwrap_or_default();
        if owner == conn.unique {
            log!("đã giành được {IBUS_NAME}");
            return Ok(true);
        }
        if attempt % 10 == 0 && !owner.is_empty() {
            let pid = bus_call(conn, "GetConnectionUnixProcessID", &[V::s(&owner)])
                .map(|m| m.arg_u32(0))
                .unwrap_or(0);
            let comm = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            match comm.trim() {
                "ibus-daemon" => {
                    log!("ibus-daemon (pid {pid}) đang giữ {IBUS_NAME}, dừng nó...");
                    stop_gnome_ibus_service();
                    if attempt > 0 {
                        let _ = Command::new("kill").arg(pid.to_string()).status();
                    }
                }
                "fcitx5" => {
                    log!("fcitx5 đang chạy và giữ {IBUS_NAME}; hãy tắt fcitx5 trước (`pkill fcitx5`).");
                    return Ok(false);
                }
                other => log!("{IBUS_NAME} đang bị {other:?} (pid {pid}) giữ, chờ..."),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    log!("hết thời gian chờ {IBUS_NAME}.");
    Ok(false)
}

fn run(args: Args) -> io::Result<()> {
    let mut conn = Conn::session()?;
    if !acquire_ibus_name(&mut conn)? {
        std::process::exit(1);
    }
    stop_gnome_ibus_service();
    // 6 = REPLACE_EXISTING | DO_NOT_QUEUE.
    match bus_call(&mut conn, "RequestName", &[V::s(PORTAL_NAME), V::U32(6)]) {
        Ok(r) if matches!(r.arg_u32(0), 1 | 4) => {}
        _ => log!("không chiếm được {PORTAL_NAME}; ứng dụng Flatpak có thể không gõ được."),
    }
    bus_call(
        &mut conn,
        "AddMatch",
        &[V::s("type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'")],
    )?;

    let sni_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let mut app = App { conn, ics: Vec::new(), next_id: 1, enabled: !args.off, debug: args.debug, tray: args.tray, sni_name };
    if app.tray {
        // 4 = DO_NOT_QUEUE.
        bus_call(&mut app.conn, "RequestName", &[V::s(&app.sni_name), V::U32(4)])?;
        app.register_tray()?;
    }

    let ours = args.address_file.then(write_address_files);
    log!(
        "đang chạy (Telex, tiếng Việt {}). Ctrl+Shift hoặc bấm biểu tượng để bật/tắt.",
        if app.enabled { "bật" } else { "tắt" }
    );

    const CHECK_EVERY: Duration = Duration::from_secs(5);
    let mut last_check = Instant::now();
    loop {
        let wait = CHECK_EVERY.saturating_sub(last_check.elapsed()).max(Duration::from_millis(10));
        if let Some(m) = app.conn.read(Some(wait))? {
            app.handle(m)?;
        }
        if last_check.elapsed() >= CHECK_EVERY {
            last_check = Instant::now();
            if let Some(ours) = &ours {
                check_address_files(ours);
            }
            // Trả lại bộ nhớ thừa sau khi nhiều ô nhập liệu bị đóng.
            if app.ics.capacity() > 16 && app.ics.len() < app.ics.capacity() / 4 {
                app.ics.shrink_to_fit();
            }
        }
    }
}

fn main() {
    let args = parse_args();
    if let Err(e) = run(args) {
        log!("lỗi D-Bus: {e}");
        std::process::exit(1);
    }
}
