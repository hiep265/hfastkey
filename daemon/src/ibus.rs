//! Các đối tượng D-Bus giả lập IBus (giao thức mà gnome-shell, GTK, Qt dùng).

use crate::keys::{Ctx, EnterMode, KeyState, Out};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use hfastkey_core::Config;
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, StructureBuilder, Value};
use zbus::{fdo, interface, Connection};

pub const IC_PATH_PREFIX: &str = "/org/freedesktop/IBus/InputContext_";

/// Trạng thái dùng chung giữa các ô nhập liệu.
pub struct Shared {
    pub config: RwLock<Config>,
    pub enter: EnterMode,
    enabled: AtomicBool,
    pub debug: bool,
    next_id: AtomicU32,
    notify_id: AtomicU32,
    /// tên kết nối của ứng dụng -> các đường dẫn ô nhập liệu (kèm cờ portal).
    pub clients: Mutex<HashMap<String, Vec<(bool, String)>>>,
    /// Gọi mỗi khi bật/tắt tiếng Việt (cập nhật biểu tượng V/E).
    pub on_toggle: Mutex<Option<Box<dyn Fn(bool) + Send>>>,
}

impl Shared {
    pub fn new(config: Config, enter: EnterMode, enabled: bool, debug: bool) -> Arc<Self> {
        Arc::new(Shared {
            config: RwLock::new(config),
            enter,
            enabled: AtomicBool::new(enabled),
            debug,
            next_id: AtomicU32::new(1),
            notify_id: AtomicU32::new(0),
            clients: Mutex::new(HashMap::new()),
            on_toggle: Mutex::new(None),
        })
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Bật/tắt và báo cho biểu tượng V/E vẽ lại.
    pub fn set_enabled(&self, on: bool) {
        self.store_enabled(on);
        self.refresh_ui();
    }

    /// Bật/tắt mà không báo (dùng từ chính biểu tượng V/E).
    pub fn store_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn refresh_ui(&self) {
        if let Some(cb) = self.on_toggle.lock().unwrap().as_ref() {
            cb(self.enabled());
        }
    }

    pub fn config(&self) -> Config {
        self.config.read().unwrap().clone()
    }
}

const CAP_SURROUNDING_TEXT: u32 = 1 << 5;

/// Lấy chuỗi trong IBusText (sa{sv}sv).
fn ibus_text_string(v: &Value<'_>) -> Option<String> {
    match v {
        Value::Value(inner) => ibus_text_string(inner),
        Value::Structure(st) => match st.fields().get(2) {
            Some(Value::Str(s)) => Some(s.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn empty_dict() -> HashMap<String, Value<'static>> {
    HashMap::new()
}

/// Dựng IBusText: (sa{sv}sv) với v = IBusAttrList (sa{sv}av).
fn ibus_text(s: &str, underline: bool) -> Value<'static> {
    let n = s.chars().count() as u32;
    let mut attrs: Vec<Value<'static>> = Vec::new();
    if underline && n > 0 {
        let attr = StructureBuilder::new()
            .add_field("IBusAttribute")
            .add_field(empty_dict())
            .add_field(1u32) // IBUS_ATTR_TYPE_UNDERLINE
            .add_field(1u32) // IBUS_ATTR_UNDERLINE_SINGLE
            .add_field(0u32)
            .add_field(n)
            .build()
            .expect("IBusAttribute");
        attrs.push(Value::from(attr));
    }
    let attr_list = StructureBuilder::new()
        .add_field("IBusAttrList")
        .add_field(empty_dict())
        .add_field(attrs)
        .build()
        .expect("IBusAttrList");
    let text = StructureBuilder::new()
        .add_field("IBusText")
        .add_field(empty_dict())
        .add_field(s.to_string())
        .append_field(Value::Value(Box::new(Value::from(attr_list))))
        .build()
        .expect("IBusText");
    Value::from(text)
}

async fn create_context(
    shared: &Arc<Shared>,
    portal: bool,
    client_name: &str,
    hdr: &Header<'_>,
    server: &ObjectServer,
) -> fdo::Result<OwnedObjectPath> {
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    let path = format!("{IC_PATH_PREFIX}{id}");
    let owner = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
    if shared.debug {
        eprintln!("hfastkey: tạo ô nhập liệu {path} cho {client_name:?} ({owner})");
    }
    server
        .at(path.as_str(), InputContext::new(shared.clone()))
        .await?;
    server.at(path.as_str(), IcService).await?;
    shared
        .clients
        .lock()
        .unwrap()
        .entry(owner)
        .or_default()
        .push((portal, path.clone()));
    OwnedObjectPath::try_from(path).map_err(|e| fdo::Error::Failed(e.to_string()))
}

/// /org/freedesktop/IBus trên tên org.freedesktop.IBus.
pub struct IBusRoot {
    pub shared: Arc<Shared>,
}

#[interface(name = "org.freedesktop.IBus")]
impl IBusRoot {
    async fn create_input_context(
        &self,
        client_name: &str,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        create_context(&self.shared, false, client_name, &hdr, server).await
    }
}

/// /org/freedesktop/IBus trên tên org.freedesktop.portal.IBus (ứng dụng Flatpak).
pub struct IBusPortal {
    pub shared: Arc<Shared>,
}

#[interface(name = "org.freedesktop.IBus.Portal")]
impl IBusPortal {
    async fn create_input_context(
        &self,
        client_name: &str,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        create_context(&self.shared, true, client_name, &hdr, server).await
    }
}

/// org.freedesktop.IBus.Service: ứng dụng gọi Destroy khi đóng ô nhập liệu.
pub struct IcService;

#[interface(name = "org.freedesktop.IBus.Service")]
impl IcService {
    async fn destroy(&self, #[zbus(header)] hdr: Header<'_>, #[zbus(connection)] conn: &Connection) {
        if let Some(path) = hdr.path().map(|p| p.to_string()) {
            let c = conn.clone();
            // Gỡ đối tượng sau khi trả lời xong, tránh tự khoá chính mình.
            conn.executor()
                .spawn(
                    async move {
                        remove_context(&c, &path).await;
                    },
                    "hfastkey-destroy",
                )
                .detach();
        }
    }
}

pub async fn remove_context(conn: &Connection, path: &str) {
    let server = conn.object_server();
    let _ = server.remove::<InputContext, _>(path).await;
    let _ = server.remove::<IcService, _>(path).await;
}

pub struct InputContext {
    shared: Arc<Shared>,
    keys: KeyState,
    caps: u32,
    client_commit_preedit: bool,
    effective_post_process: bool,
    content_type: (u32, u32),
}

impl InputContext {
    fn new(shared: Arc<Shared>) -> Self {
        let keys = KeyState::new(&shared.config());
        InputContext {
            shared,
            keys,
            caps: 0,
            client_commit_preedit: false,
            effective_post_process: false,
            content_type: (0, 0),
        }
    }

    async fn emit(&self, em: &SignalEmitter<'_>, conn: &Connection, out: Vec<Out>) {
        for o in out {
            let r = match o {
                Out::Delete(n) => Self::delete_surrounding_text(em, -(n as i32), n).await,
                Out::Commit(s) => Self::commit_text(em, ibus_text(&s, false)).await,
                Out::Preedit(s) => {
                    let cursor = s.chars().count() as u32;
                    let visible = !s.is_empty();
                    let text = ibus_text(&s, true);
                    if self.client_commit_preedit {
                        // mode 1 = COMMIT: ứng dụng tự chốt chữ khi mất focus/click chỗ khác.
                        Self::update_preedit_text_with_mode(em, text, cursor, visible, 1).await
                    } else {
                        Self::update_preedit_text(em, text, cursor, visible).await
                    }
                }
                Out::Forward { keyval, keycode, state } => {
                    Self::forward_key_event(em, keyval, keycode, state).await
                }
                Out::Toggled(on) => {
                    self.shared.set_enabled(on);
                    if self.shared.config().notify_toggle {
                        notify_toggle(&self.shared, conn, on);
                    }
                    Ok(())
                }
            };
            if let Err(e) = r {
                eprintln!("hfastkey: lỗi gửi tín hiệu: {e}");
            }
        }
    }

    /// Mất focus / reset. Chế độ gõ thẳng: chữ đã nằm trong ứng dụng, chỉ cần
    /// quên từ. Chế độ gạch chân: ứng dụng nào tự chốt chữ (gnome-shell) thì
    /// thôi, ứng dụng khác thì mình chốt hộ để không mất chữ.
    async fn finish_word(&mut self, em: &SignalEmitter<'_>, conn: &Connection) {
        if !self.keys.has_word() || self.keys.word_is_direct() {
            self.keys.clear();
            return;
        }
        let word = self.keys.take_word();
        self.keys.clear();
        if !self.client_commit_preedit {
            self.emit(em, conn, vec![Out::Commit(word), Out::Preedit(String::new())])
                .await;
        }
    }
}

fn notify_toggle(shared: &Arc<Shared>, conn: &Connection, on: bool) {
    let conn = conn.clone();
    let shared = shared.clone();
    conn.clone()
        .executor()
        .spawn(
            async move {
                let summary = if on { "Tiếng Việt: BẬT" } else { "Tiếng Việt: TẮT" };
                let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
                hints.insert("transient", Value::from(true));
                let replaces = shared.notify_id.load(Ordering::Relaxed);
                let reply = conn
                    .call_method(
                        Some("org.freedesktop.Notifications"),
                        "/org/freedesktop/Notifications",
                        Some("org.freedesktop.Notifications"),
                        "Notify",
                        &(
                            "HFastKey",
                            replaces,
                            "input-keyboard",
                            summary,
                            "Ctrl+Space để chuyển",
                            Vec::<&str>::new(),
                            hints,
                            1500i32,
                        ),
                    )
                    .await;
                if let Ok(msg) = reply {
                    if let Ok(id) = msg.body().deserialize::<u32>() {
                        shared.notify_id.store(id, Ordering::Relaxed);
                    }
                }
            },
            "hfastkey-notify",
        )
        .detach();
}

#[interface(name = "org.freedesktop.IBus.InputContext")]
impl InputContext {
    async fn process_key_event(
        &mut self,
        keyval: u32,
        keycode: u32,
        state: u32,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> bool {
        let cfg = self.shared.config();
        let ctx = Ctx {
            cfg: &cfg,
            surrounding: self.caps & CAP_SURROUNDING_TEXT != 0,
            enter: self.shared.enter,
        };
        let mut enabled = self.shared.enabled();
        let mut out = Vec::new();
        let handled = self.keys.key(&ctx, &mut enabled, keyval, keycode, state, &mut out);
        if self.shared.debug {
            eprintln!("hfastkey: key {keyval:#x} code {keycode} state {state:#x} caps {:#x} -> {handled} {out:?}", self.caps);
        }
        // Tín hiệu được ghi ra socket trước câu trả lời, nên ứng dụng luôn
        // nhận chữ trước khi nhận kết quả xử lý phím.
        self.emit(&em, conn, out).await;
        handled
    }

    fn set_cursor_location(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}
    fn set_cursor_location_relative(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}
    fn process_hand_writing_event(&self, _coordinates: Vec<f64>) {}
    fn cancel_hand_writing(&self, _n_strokes: u32) {}

    async fn focus_in(&mut self) {
        self.keys.forget_surrounding();
    }

    async fn focus_out(
        &mut self,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
        #[zbus(connection)] conn: &Connection,
    ) {
        self.finish_word(&em, conn).await;
        self.keys.forget_surrounding();
    }

    async fn reset(
        &mut self,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
        #[zbus(connection)] conn: &Connection,
    ) {
        self.finish_word(&em, conn).await;
    }

    fn enable(&self) {}
    fn disable(&self) {}
    fn is_enabled(&self) -> bool {
        true
    }
    async fn set_capabilities(&mut self, caps: u32, #[zbus(signal_emitter)] em: SignalEmitter<'_>) {
        let had = self.caps & CAP_SURROUNDING_TEXT != 0;
        self.caps = caps;
        if caps & CAP_SURROUNDING_TEXT != 0 && !had {
            // Thư viện IBus chỉ gửi chữ quanh con trỏ sau khi bộ gõ yêu cầu.
            if let Err(e) = Self::require_surrounding_text(&em).await {
                eprintln!("hfastkey: lỗi gửi RequireSurroundingText: {e}");
            }
        }
    }
    fn property_activate(&self, _name: &str, _state: u32) {}
    fn set_engine(&self, _name: &str) {}
    fn get_engine(&self) -> OwnedValue {
        OwnedValue::from(0i32)
    }
    fn set_surrounding_text(&mut self, text: Value<'_>, cursor: u32, _anchor: u32) {
        if let Some(t) = ibus_text_string(&text) {
            let before: String = t.chars().take(cursor as usize).collect();
            if self.shared.debug {
                eprintln!("hfastkey: surrounding trước con trỏ: {before:?}");
            }
            self.keys.on_surrounding(before);
        }
    }

    #[zbus(property)]
    fn content_type(&self) -> (u32, u32) {
        self.content_type
    }
    #[zbus(property)]
    fn set_content_type(&mut self, v: (u32, u32)) {
        self.content_type = v;
    }

    #[zbus(property)]
    fn client_commit_preedit(&self) -> (bool,) {
        (self.client_commit_preedit,)
    }
    #[zbus(property)]
    fn set_client_commit_preedit(&mut self, v: (bool,)) {
        self.client_commit_preedit = v.0;
    }

    #[zbus(property)]
    fn effective_post_process_key_event(&self) -> (bool,) {
        (self.effective_post_process,)
    }
    #[zbus(property)]
    fn set_effective_post_process_key_event(&mut self, v: (bool,)) {
        self.effective_post_process = v.0;
    }

    #[zbus(property)]
    fn post_process_key_event(&self) -> (Vec<(u8, OwnedValue)>,) {
        (Vec::new(),)
    }

    #[zbus(signal)]
    async fn require_surrounding_text(em: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn delete_surrounding_text(em: &SignalEmitter<'_>, offset: i32, nchars: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn commit_text(em: &SignalEmitter<'_>, text: Value<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn forward_key_event(em: &SignalEmitter<'_>, keyval: u32, keycode: u32, state: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_preedit_text(em: &SignalEmitter<'_>, text: Value<'_>, cursor_pos: u32, visible: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_preedit_text_with_mode(
        em: &SignalEmitter<'_>,
        text: Value<'_>,
        cursor_pos: u32,
        visible: bool,
        mode: u32,
    ) -> zbus::Result<()>;
}
