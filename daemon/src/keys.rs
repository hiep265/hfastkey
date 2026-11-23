//! Xử lý phím cho một ô nhập liệu (không phụ thuộc D-Bus, để test được).
//!
//! Hai chế độ:
//! - Gõ thẳng (giống Unikey): chữ được đưa ngay vào ứng dụng; khi cần thêm dấu
//!   thì xoá vài ký tự trước con trỏ rồi gõ lại. Không có chữ "treo" nên
//!   Enter/Tab/... đi thẳng tới ứng dụng, không thể mất chữ.
//! - Gạch chân (preedit): chỉ dùng khi ứng dụng không cho xoá chữ quanh con
//!   trỏ (terminal...).
//!
//! Bật/tắt tiếng Việt: nhấn Ctrl+Shift (không kèm phím khác) rồi nhả.

use hfastkey_core::Engine;

// Mặt nạ trạng thái phím theo IBus.
pub const SHIFT_MASK: u32 = 1 << 0;
pub const CONTROL_MASK: u32 = 1 << 2;
pub const MOD1_MASK: u32 = 1 << 3; // Alt
pub const MOD4_MASK: u32 = 1 << 6;
pub const SUPER_MASK: u32 = 1 << 26;
pub const HYPER_MASK: u32 = 1 << 27;
pub const META_MASK: u32 = 1 << 28;
pub const RELEASE_MASK: u32 = 1 << 30;
const COMBO_MASK: u32 = CONTROL_MASK | MOD1_MASK | MOD4_MASK | SUPER_MASK | HYPER_MASK | META_MASK;

const KEY_BACKSPACE: u32 = 0xff08;
const KEY_SHIFT_L: u32 = 0xffe1;
const KEY_SHIFT_R: u32 = 0xffe2;
const KEY_CONTROL_L: u32 = 0xffe3;
const KEY_CONTROL_R: u32 = 0xffe4;

/// Việc cần báo cho ứng dụng, theo đúng thứ tự.
#[derive(Debug, PartialEq, Eq)]
pub enum Out {
    /// Xoá `n` ký tự ngay trước con trỏ.
    Delete(u32),
    Commit(String),
    Preedit(String),
    Forward { keyval: u32, keycode: u32, state: u32 },
    Toggled(bool),
}

#[derive(Default)]
pub struct KeyState {
    engine: Engine,
    swallow_release: Option<u32>,
    /// Từ hiện tại đang được gõ thẳng (true) hay bằng gạch chân (false).
    word_direct: bool,
    /// Phần chữ của từ hiện tại đã nằm trong ứng dụng (chế độ gõ thẳng).
    committed: String,
    /// Chữ trước con trỏ lần gần nhất ứng dụng báo.
    last_before: Option<String>,
    /// Chữ trước con trỏ lúc bắt đầu từ + các trạng thái của từ đã gửi đi,
    /// để phát hiện người dùng bấm chuột sang chỗ khác.
    word_start_before: Option<String>,
    snapshots: Vec<String>,
    /// Có từ trước (gõ thẳng, đã kết thúc bằng dấu cách/dấu câu) để xoá lùi
    /// quay lại gõ tiếp; kèm chữ đứng trước từ đó.
    prev_active: bool,
    prev_start: Option<String>,
    /// Đang chờ nhả Ctrl/Shift để bật/tắt.
    ctrl_shift_armed: bool,
}

fn is_modifier(keyval: u32) -> bool {
    matches!(keyval, 0xffe1..=0xffee | 0xfe01..=0xfe13 | 0xff7e | 0xff7f)
}

fn common_prefix_chars(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

impl KeyState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_word(&self) -> bool {
        !self.engine.is_empty()
    }

    pub fn word_is_direct(&self) -> bool {
        self.word_direct
    }

    /// Quên từ hiện tại và từ trước (chữ đã gõ thẳng vẫn nằm trong ứng dụng).
    fn drop_word(&mut self) {
        self.engine.reset();
        self.committed.clear();
        self.snapshots.clear();
        self.prev_active = false;
        self.prev_start = None;
    }

    /// Lấy chữ đang gõ (chế độ gạch chân) ra và xoá trạng thái.
    pub fn take_word(&mut self) -> String {
        let s = self.engine.text();
        self.drop_word();
        s
    }

    pub fn clear(&mut self) {
        self.drop_word();
        self.swallow_release = None;
        self.ctrl_shift_armed = false;
    }

    /// Quên chữ quanh con trỏ (đổi ứng dụng/ô nhập): chờ ứng dụng báo lại.
    pub fn forget_surrounding(&mut self) {
        self.last_before = None;
    }

    /// Ứng dụng báo chữ quanh con trỏ. `before` = phần chữ đứng trước con trỏ.
    pub fn on_surrounding(&mut self, before: String) {
        if let (true, true, Some(start)) = (self.has_word(), self.word_direct, self.word_start_before.as_deref()) {
            let ok = std::iter::once("")
                .chain(self.snapshots.iter().map(String::as_str))
                .any(|s| {
                    let expected = format!("{start}{s}");
                    expected.ends_with(&before) || (!expected.is_empty() && before.ends_with(&expected))
                });
            if !ok {
                // Con trỏ đã chuyển sang chỗ khác: bắt đầu từ mới.
                self.drop_word();
            }
        }
        self.last_before = Some(before);
    }

    /// Báo cho ứng dụng nội dung mới của từ đang gõ.
    fn update(&mut self, out: &mut Vec<Out>) {
        let text = self.engine.text();
        if !self.word_direct {
            out.push(Out::Preedit(text));
            return;
        }
        let keep = common_prefix_chars(&self.committed, &text);
        let del = self.committed.chars().count() - keep;
        let add: String = text.chars().skip(keep).collect();
        if del > 0 {
            out.push(Out::Delete(del as u32));
        }
        if !add.is_empty() {
            out.push(Out::Commit(add));
        }
        if text.is_empty() {
            // Xoá hết từ: vẫn nhớ từ trước (nếu có) để xoá tiếp quay lại.
            self.committed.clear();
            self.snapshots.clear();
        } else {
            self.snapshots.push(text.clone());
            self.committed = text;
        }
    }

    fn toggle(&mut self, enabled: &mut bool, out: &mut Vec<Out>) {
        self.finish(out);
        *enabled = !*enabled;
        out.push(Out::Toggled(*enabled));
    }

    /// Kết thúc từ hiện tại (không kèm phím nào).
    pub fn finish(&mut self, out: &mut Vec<Out>) {
        if self.has_word() && !self.word_direct {
            out.push(Out::Commit(self.take_word()));
            out.push(Out::Preedit(String::new()));
        }
        self.drop_word();
    }

    /// Trả về true nếu phím đã được bộ gõ xử lý (ứng dụng không nhận phím gốc).
    pub fn key(
        &mut self,
        surrounding: bool,
        enabled: &mut bool,
        keyval: u32,
        keycode: u32,
        state: u32,
        out: &mut Vec<Out>,
    ) -> bool {
        let release = state & RELEASE_MASK != 0;
        let mods = state & !RELEASE_MASK;

        if release {
            if self.swallow_release == Some(keycode) {
                self.swallow_release = None;
                return true;
            }
            let ctrl_or_shift = matches!(keyval, KEY_SHIFT_L | KEY_SHIFT_R | KEY_CONTROL_L | KEY_CONTROL_R);
            if self.ctrl_shift_armed && ctrl_or_shift {
                self.ctrl_shift_armed = false;
                self.toggle(enabled, out);
            }
            return false;
        }

        // Ctrl+Shift: nhấn Ctrl và Shift (không kèm phím nào khác) rồi nhả.
        self.ctrl_shift_armed = (matches!(keyval, KEY_CONTROL_L | KEY_CONTROL_R)
            && mods & COMBO_MASK == 0
            && mods & SHIFT_MASK != 0)
            || (matches!(keyval, KEY_SHIFT_L | KEY_SHIFT_R) && mods & COMBO_MASK == CONTROL_MASK);

        if !*enabled {
            self.drop_word();
            return false;
        }
        if is_modifier(keyval) {
            return false;
        }

        let has_word = self.has_word();

        if mods & COMBO_MASK != 0 {
            // Phím tắt (Ctrl+C, Alt+Tab...): kết thúc từ rồi để phím đi tiếp.
            return self.finish_then_key(keyval, keycode, mods, out);
        }

        if keyval == KEY_BACKSPACE {
            if has_word {
                self.engine.backspace();
                self.update(out);
                return true;
            }
            self.backspace_outside_word(surrounding);
            return false;
        }

        if (0x20..0x7f).contains(&keyval) {
            let ch = char::from_u32(keyval).unwrap_or(' ');
            if self.engine.process(ch) {
                if !has_word {
                    self.word_direct = surrounding;
                    self.word_start_before = self.last_before.clone();
                    self.snapshots.clear();
                    self.committed.clear();
                }
                self.update(out);
                return true;
            }
            // Dấu cách/dấu câu: kết thúc từ nhưng vẫn nhớ (gõ thẳng) để xoá lùi quay lại.
            if !has_word {
                if self.prev_active {
                    self.engine.separator();
                } else {
                    self.drop_word();
                }
                return false;
            }
            if self.word_direct {
                self.prev_active = true;
                self.prev_start = self.word_start_before.take();
                self.engine.separator();
                self.committed.clear();
                self.snapshots.clear();
                // Gửi qua cùng kênh với chữ để giữ đúng thứ tự.
                out.push(Out::Commit(ch.to_string()));
            } else {
                let mut s = self.take_word();
                s.push(ch);
                out.push(Out::Commit(s));
                out.push(Out::Preedit(String::new()));
            }
            return true;
        }

        // Enter, Tab, Escape, mũi tên, phím số bên phải, ký tự ngoài ASCII...
        self.finish_then_key(keyval, keycode, mods, out)
    }

    /// Backspace khi không có từ đang gõ: ứng dụng tự xoá; nếu xoá hết các ký tự
    /// ngăn cách sau từ trước thì quay lại từ đó (gõ thêm dấu được).
    fn backspace_outside_word(&mut self, surrounding: bool) {
        if !self.prev_active || !surrounding {
            self.drop_word();
            return;
        }
        self.engine.backspace();
        if self.has_word() {
            let text = self.engine.text();
            self.word_direct = true;
            self.word_start_before = self.prev_start.take();
            // Ứng dụng có thể báo chậm trạng thái còn dấu cách: vẫn coi là đúng chỗ.
            self.snapshots = vec![text.clone(), format!("{text} ")];
            self.committed = text;
            self.prev_active = false;
        }
    }

    fn finish_then_key(&mut self, keyval: u32, keycode: u32, mods: u32, out: &mut Vec<Out>) -> bool {
        if !self.has_word() || self.word_direct {
            // Chữ đã nằm sẵn trong ứng dụng: phím đi thẳng, không cần làm gì.
            self.drop_word();
            return false;
        }
        out.push(Out::Commit(self.take_word()));
        out.push(Out::Preedit(String::new()));
        // Chế độ gạch chân: tự gửi lại phím sau khi chốt chữ để giữ đúng thứ tự.
        out.push(Out::Forward { keyval, keycode, state: mods });
        out.push(Out::Forward { keyval, keycode, state: mods | RELEASE_MASK });
        self.swallow_release = Some(keycode);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTER: u32 = 0xff0d;
    const ENTER_CODE: u32 = 28;
    const BS: u32 = KEY_BACKSPACE;

    struct T {
        s: KeyState,
        on: bool,
        surrounding: bool,
        /// Mô phỏng ô nhập liệu: nội dung trước con trỏ.
        app: String,
    }

    impl T {
        fn new(surrounding: bool) -> Self {
            let mut s = KeyState::new();
            if surrounding {
                s.on_surrounding(String::new());
            }
            T { s, on: true, surrounding, app: String::new() }
        }

        fn key(&mut self, keyval: u32, keycode: u32, state: u32) -> (bool, Vec<Out>) {
            let mut out = Vec::new();
            let handled = self.s.key(self.surrounding, &mut self.on, keyval, keycode, state, &mut out);
            for o in &out {
                match o {
                    Out::Delete(n) => {
                        for _ in 0..*n {
                            self.app.pop();
                        }
                    }
                    Out::Commit(t) => self.app.push_str(t),
                    _ => {}
                }
            }
            if !handled && state & RELEASE_MASK == 0 && state & COMBO_MASK == 0 {
                if (0x20..0x7f).contains(&keyval) {
                    self.app.push(char::from_u32(keyval).unwrap());
                } else if keyval == BS {
                    self.app.pop();
                }
            }
            if self.surrounding && state & RELEASE_MASK == 0 {
                // Ứng dụng báo lại chữ trước con trỏ sau mỗi thay đổi.
                self.s.on_surrounding(self.app.clone());
            }
            (handled, out)
        }

        fn type_str(&mut self, text: &str) {
            for c in text.chars() {
                if c == '<' {
                    self.key(BS, 14, 0);
                    self.key(BS, 14, RELEASE_MASK);
                    continue;
                }
                let st = if c.is_ascii_uppercase() { SHIFT_MASK } else { 0 };
                self.key(c as u32, 30, st);
                self.key(c as u32, 30, st | RELEASE_MASK);
            }
        }
    }

    #[test]
    fn direct_mode_types_into_app_without_preedit() {
        let mut t = T::new(true);
        t.type_str("xin chaof");
        assert_eq!(t.app, "xin chào");
        let (handled, out) = t.key(ENTER, ENTER_CODE, 0);
        assert!(!handled, "Enter phải đi thẳng tới ứng dụng");
        assert!(out.is_empty());
        t.type_str("Tieengs Vieejt nheS.");
        assert_eq!(t.app, "xin chàoTiếng Việt nhé.");
    }

    #[test]
    fn direct_mode_never_uses_preedit() {
        let mut t = T::new(true);
        for c in "dduwowngf".chars() {
            let (_, out) = t.key(c as u32, 30, 0);
            assert!(out.iter().all(|o| !matches!(o, Out::Preedit(_))), "{out:?}");
        }
        assert_eq!(t.app, "đường");
    }

    #[test]
    fn direct_mode_tone_rewrites_only_changed_tail() {
        let mut t = T::new(true);
        t.type_str("vie");
        let (_, out) = t.key('e' as u32, 30, 0);
        assert_eq!(out, vec![Out::Delete(1), Out::Commit("ê".into())]);
        let (_, out) = t.key('j' as u32, 30, 0);
        assert_eq!(out, vec![Out::Delete(1), Out::Commit("ệ".into())]);
    }

    #[test]
    fn direct_mode_backspace_and_restore() {
        let mut t = T::new(true);
        t.type_str("vieejt");
        t.key(BS, 14, 0);
        assert_eq!(t.app, "việ");
        t.type_str("c ");
        assert_eq!(t.app, "việc ");
        t.type_str("classs pussh uuw ");
        assert_eq!(t.app, "việc class push ưu ");
    }

    #[test]
    fn backspace_over_space_resumes_previous_word() {
        let mut t = T::new(true);
        t.type_str("nua <wx");
        assert_eq!(t.app, "nữa");
        t.type_str(" vuong  ab<<<<f");
        assert_eq!(t.app, "nữa vuòng");
        t.type_str("w nhes");
        assert_eq!(t.app, "nữa vường nhé");

        // Bấm Enter/mũi tên (có thể đã đổi chỗ) thì không quay lại nữa.
        let mut t = T::new(true);
        t.type_str("nua ");
        t.key(ENTER, ENTER_CODE, 0);
        t.type_str("<s");
        assert_eq!(t.app, "nuas");
    }

    #[test]
    fn cursor_moved_starts_new_word() {
        let mut t = T::new(true);
        t.type_str("vie");
        // Ứng dụng báo trạng thái cũ (chậm): không được coi là đổi chỗ.
        t.s.on_surrounding("vi".into());
        assert!(t.s.has_word());
        // Người dùng bấm chuột sang chỗ khác.
        t.s.on_surrounding("hello ".into());
        assert!(!t.s.has_word());
    }

    #[test]
    fn falls_back_to_preedit_without_surrounding() {
        let mut t = T::new(false);
        let (_, out) = t.key('a' as u32, 30, 0);
        assert_eq!(out, vec![Out::Preedit("a".into())]);
        let (handled, out) = t.key(ENTER, ENTER_CODE, 0);
        assert!(handled);
        assert_eq!(out[0], Out::Commit("a".into()));
        assert!(matches!(out[2], Out::Forward { .. }));
        // Nhả Enter thật bị nuốt.
        assert!(t.key(ENTER, ENTER_CODE, RELEASE_MASK).0);
    }

    #[test]
    fn ctrl_shift_toggles() {
        let mut t = T::new(true);
        assert!(!t.key(KEY_CONTROL_L, 29, 0).0);
        assert!(!t.key(KEY_SHIFT_L, 42, CONTROL_MASK).0);
        t.key(KEY_SHIFT_L, 42, CONTROL_MASK | SHIFT_MASK | RELEASE_MASK);
        assert!(!t.on);
        assert!(!t.key('a' as u32, 30, 0).0, "tắt thì không xử lý");
        // Ctrl+Shift+T (phím tắt) thì không chuyển.
        t.key(KEY_CONTROL_L, 29, 0);
        t.key(KEY_SHIFT_L, 42, CONTROL_MASK);
        t.key('T' as u32, 20, CONTROL_MASK | SHIFT_MASK);
        t.key(KEY_SHIFT_L, 42, CONTROL_MASK | SHIFT_MASK | RELEASE_MASK);
        assert!(!t.on);
        // Shift rồi Ctrl cũng được.
        t.key(KEY_SHIFT_L, 42, 0);
        t.key(KEY_CONTROL_L, 29, SHIFT_MASK);
        t.key(KEY_CONTROL_L, 29, CONTROL_MASK | SHIFT_MASK | RELEASE_MASK);
        assert!(t.on);
    }

    #[test]
    fn ctrl_shortcut_in_direct_mode_passes_through() {
        let mut t = T::new(true);
        t.type_str("dduwowngf");
        let (handled, out) = t.key('c' as u32, 46, CONTROL_MASK);
        assert!(!handled);
        assert!(out.is_empty());
        assert!(!t.s.has_word());
    }
}
