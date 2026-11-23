//! Logic bộ gõ cho Windows (không gọi API Windows, test được trên mọi hệ điều hành).
//!
//! Cách làm giống Unikey/OpenKey: bắt phím toàn hệ thống; phím chữ bình
//! thường thì để đi qua; khi cần thêm dấu thì chặn phím gốc, gửi vài phím
//! Backspace để xoá phần chữ cần đổi rồi gửi chữ mới (ký tự Unicode).

use hfastkey_core::{Config, Engine, ToggleKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Ctrl,
    Shift,
    Alt,
    Win,
    /// Chữ cái hoặc chữ số (đã tính Shift/CapsLock).
    Char(char),
    Backspace,
    Space,
    /// Mọi phím khác: Enter, Tab, mũi tên, dấu câu...
    Other,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

/// Kết quả xử lý một phím.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Action {
    /// Chặn phím gốc (không cho ứng dụng nhận).
    pub block: bool,
    /// Số phím Backspace cần gửi.
    pub backspaces: usize,
    /// Chữ cần gửi sau khi xoá.
    pub text: String,
    /// Đã bật/tắt tiếng Việt (giá trị mới).
    pub toggled: Option<bool>,
}

pub struct State {
    engine: Engine,
    /// Phần chữ của từ hiện tại đang nằm trong ứng dụng.
    committed: String,
    ctrl_shift_armed: bool,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        State {
            engine: Engine::new(hfastkey_core::Method::Telex),
            committed: String::new(),
            ctrl_shift_armed: false,
        }
    }

    /// Quên từ đang gõ (bấm chuột, đổi cửa sổ, phím điều hướng...).
    pub fn reset(&mut self) {
        self.engine.reset();
        self.committed.clear();
    }

    fn apply_config(&mut self, cfg: &Config) {
        self.engine.set_method(cfg.method);
        self.engine.set_modern_tone(cfg.modern_tone);
        self.engine.set_restore_invalid(cfg.restore_invalid);
    }

    fn toggle(&mut self, enabled: &mut bool) -> Action {
        self.reset();
        *enabled = !*enabled;
        Action {
            block: true,
            toggled: Some(*enabled),
            ..Default::default()
        }
    }

    pub fn key_up(&mut self, cfg: &Config, enabled: &mut bool, key: Key) -> Option<bool> {
        if cfg.toggle_key == ToggleKey::CtrlShift
            && self.ctrl_shift_armed
            && matches!(key, Key::Ctrl | Key::Shift)
        {
            self.ctrl_shift_armed = false;
            return self.toggle(enabled).toggled;
        }
        None
    }

    pub fn key_down(&mut self, cfg: &Config, enabled: &mut bool, key: Key, m: Mods) -> Action {
        if self.engine.is_empty() {
            self.apply_config(cfg);
        }
        let pass = Action::default();

        // Ctrl+Shift: nhấn cả hai (không kèm phím khác) rồi nhả.
        if cfg.toggle_key == ToggleKey::CtrlShift {
            self.ctrl_shift_armed = match key {
                Key::Ctrl => m.shift && !m.alt && !m.win,
                Key::Shift => m.ctrl && !m.alt && !m.win,
                _ => false,
            };
        }
        let is_toggle = match cfg.toggle_key {
            ToggleKey::CtrlSpace => key == Key::Space && m.ctrl && !m.alt && !m.win,
            ToggleKey::AltZ => matches!(key, Key::Char('z' | 'Z')) && m.alt && !m.ctrl && !m.win,
            ToggleKey::CtrlShift => false,
        };
        if is_toggle {
            return self.toggle(enabled);
        }

        if !*enabled {
            self.reset();
            return pass;
        }
        if matches!(key, Key::Ctrl | Key::Shift | Key::Alt | Key::Win) {
            return pass;
        }
        if m.ctrl || m.alt || m.win {
            // Phím tắt: kết thúc từ, để phím đi qua.
            self.reset();
            return pass;
        }

        match key {
            Key::Backspace => {
                if self.engine.is_empty() {
                    return pass;
                }
                self.engine.backspace();
                self.output(None)
            }
            Key::Char(c) => {
                if self.engine.process(c) {
                    self.output(Some(c))
                } else {
                    self.reset();
                    pass
                }
            }
            _ => {
                self.reset();
                pass
            }
        }
    }

    /// So sánh chữ đang có trong ứng dụng với chữ mới, quyết định chặn phím
    /// hay để phím gốc đi qua.
    fn output(&mut self, typed: Option<char>) -> Action {
        let new = self.engine.text();
        let keep = self
            .committed
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let backspaces = self.committed.chars().count() - keep;
        let text: String = new.chars().skip(keep).collect();
        self.committed = new;
        if self.engine.is_empty() {
            self.committed.clear();
        }

        // Trường hợp phím gốc tự làm đúng việc: để nó đi qua cho tự nhiên.
        let natural = match typed {
            Some(c) => backspaces == 0 && text.len() == c.len_utf8() && text.starts_with(c),
            None => backspaces == 1 && text.is_empty(),
        };
        if natural {
            return Action::default();
        }
        Action {
            block: true,
            backspaces,
            text,
            toggled: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mô phỏng ô nhập liệu để kiểm tra kết quả cuối cùng.
    struct Sim {
        st: State,
        cfg: Config,
        on: bool,
        app: String,
    }

    impl Sim {
        fn new() -> Self {
            Sim { st: State::new(), cfg: Config::default(), on: true, app: String::new() }
        }

        fn press(&mut self, key: Key, m: Mods) -> Action {
            let a = self.st.key_down(&self.cfg, &mut self.on, key, m);
            if a.block {
                for _ in 0..a.backspaces {
                    self.app.pop();
                }
                self.app.push_str(&a.text);
            } else if !(m.ctrl || m.alt || m.win) {
                match key {
                    Key::Char(c) => self.app.push(c),
                    Key::Space => self.app.push(' '),
                    Key::Backspace => {
                        self.app.pop();
                    }
                    _ => {}
                }
            }
            a
        }

        fn type_str(&mut self, s: &str) {
            for c in s.chars() {
                let key = match c {
                    ' ' => Key::Space,
                    '.' | ',' | '?' => {
                        self.press(Key::Other, Mods::default());
                        self.app.push(c);
                        continue;
                    }
                    c => Key::Char(c),
                };
                self.press(key, Mods::default());
            }
        }
    }

    #[test]
    fn types_vietnamese() {
        let mut s = Sim::new();
        s.type_str("Tieengs Vieejt raast ddepj, nguowif ddaau?");
        assert_eq!(s.app, "Tiếng Việt rất đẹp, người đâu?");
    }

    #[test]
    fn plain_letters_pass_through() {
        let mut s = Sim::new();
        let a = s.press(Key::Char('x'), Mods::default());
        assert!(!a.block, "chữ thường không cần chặn");
        let a = s.press(Key::Char('a'), Mods::default());
        assert!(!a.block);
        let a = s.press(Key::Char('a'), Mods::default());
        assert_eq!(a, Action { block: true, backspaces: 1, text: "â".into(), toggled: None });
    }

    #[test]
    fn english_and_undo_like_unikey() {
        let mut s = Sim::new();
        s.type_str("pussh classs text window ");
        assert_eq!(s.app, "push class text window ");
    }

    #[test]
    fn backspace_inside_word() {
        let mut s = Sim::new();
        s.type_str("vieejt");
        s.press(Key::Backspace, Mods::default());
        assert_eq!(s.app, "việ");
        s.type_str("c");
        assert_eq!(s.app, "việc");
    }

    #[test]
    fn enter_and_shortcuts_pass_through() {
        let mut s = Sim::new();
        s.type_str("chaof");
        assert!(!s.press(Key::Other, Mods::default()).block);
        s.type_str("dd");
        let ctrl = Mods { ctrl: true, ..Default::default() };
        assert!(!s.press(Key::Char('c'), ctrl).block);
        // Sau phím tắt, từ mới bắt đầu lại.
        s.type_str("a");
        assert_eq!(s.app, "chàođa");
    }

    #[test]
    fn toggles() {
        let mut s = Sim::new();
        let ctrl = Mods { ctrl: true, ..Default::default() };
        let a = s.press(Key::Space, ctrl);
        assert_eq!(a.toggled, Some(false));
        s.type_str("aa");
        assert_eq!(s.app, "aa", "tắt thì gõ chữ thường");

        let mut s = Sim::new();
        s.cfg.toggle_key = ToggleKey::CtrlShift;
        s.press(Key::Ctrl, Mods::default());
        s.press(Key::Shift, ctrl);
        assert_eq!(s.st.key_up(&s.cfg, &mut s.on, Key::Shift), Some(false));
        // Ctrl+Shift+T là phím tắt, không chuyển.
        s.press(Key::Ctrl, Mods::default());
        s.press(Key::Shift, ctrl);
        s.press(Key::Char('T'), Mods { ctrl: true, shift: true, ..Default::default() });
        assert_eq!(s.st.key_up(&s.cfg, &mut s.on, Key::Shift), None);

        let mut s = Sim::new();
        s.cfg.toggle_key = ToggleKey::AltZ;
        let alt = Mods { alt: true, ..Default::default() };
        assert_eq!(s.press(Key::Char('z'), alt).toggled, Some(false));
    }

    #[test]
    fn vni() {
        let mut s = Sim::new();
        s.cfg.method = hfastkey_core::Method::Vni;
        s.type_str("Vie65t Nam");
        assert_eq!(s.app, "Việt Nam");
    }
}
