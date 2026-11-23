//! Tệp cài đặt `~/.config/hfastkey/config` (dạng `khoá = giá trị`), dùng chung
//! cho daemon và hộp thoại cài đặt.

use crate::engine::Method;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToggleKey {
    CtrlShift,
    AltZ,
    CtrlSpace,
}

impl ToggleKey {
    pub fn as_str(self) -> &'static str {
        match self {
            ToggleKey::CtrlShift => "ctrl-shift",
            ToggleKey::AltZ => "alt-z",
            ToggleKey::CtrlSpace => "ctrl-space",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ToggleKey::CtrlShift => "Ctrl+Shift",
            ToggleKey::AltZ => "Alt+Z",
            ToggleKey::CtrlSpace => "Ctrl+Space",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "ctrl-shift" => Some(ToggleKey::CtrlShift),
            "alt-z" => Some(ToggleKey::AltZ),
            "ctrl-space" => Some(ToggleKey::CtrlSpace),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub method: Method,
    /// Đặt dấu kiểu mới: hoà, thuý.
    pub modern_tone: bool,
    /// Tự khôi phục phím đã gõ khi từ không phải tiếng Việt ("class" giữ nguyên).
    pub restore_invalid: bool,
    /// Hiện chữ đang gõ có gạch chân (preedit). Tắt = gõ thẳng như Unikey.
    pub underline: bool,
    pub toggle_key: ToggleKey,
    /// Bật tiếng Việt ngay khi khởi động.
    pub start_enabled: bool,
    /// Hiện thông báo nhỏ mỗi lần bật/tắt tiếng Việt.
    pub notify_toggle: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            method: Method::Telex,
            modern_tone: false,
            restore_invalid: true,
            underline: false,
            toggle_key: ToggleKey::CtrlSpace,
            start_enabled: true,
            notify_toggle: false,
        }
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl Config {
    pub fn path() -> PathBuf {
        #[cfg(windows)]
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("hfastkey").join("config");
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        base.join("hfastkey").join("config")
    }

    pub fn parse(text: &str) -> Self {
        let mut c = Config::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "method" => match v {
                    "telex" => c.method = Method::Telex,
                    "vni" => c.method = Method::Vni,
                    _ => {}
                },
                "modern_tone" => c.modern_tone = parse_bool(v).unwrap_or(c.modern_tone),
                "restore_invalid" => c.restore_invalid = parse_bool(v).unwrap_or(c.restore_invalid),
                "underline" => c.underline = parse_bool(v).unwrap_or(c.underline),
                "toggle_key" => c.toggle_key = ToggleKey::parse(v).unwrap_or(c.toggle_key),
                "start_enabled" => c.start_enabled = parse_bool(v).unwrap_or(c.start_enabled),
                "notify_toggle" => c.notify_toggle = parse_bool(v).unwrap_or(c.notify_toggle),
                _ => {}
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        let method = match self.method {
            Method::Telex => "telex",
            Method::Vni => "vni",
        };
        format!(
            "# Cài đặt HFastKey (sửa bằng hfastkey-settings hoặc trực tiếp ở đây)\n\
             method = {method}\n\
             modern_tone = {}\n\
             restore_invalid = {}\n\
             underline = {}\n\
             toggle_key = {}\n\
             start_enabled = {}\n\
             notify_toggle = {}\n",
            self.modern_tone,
            self.restore_invalid,
            self.underline,
            self.toggle_key.as_str(),
            self.start_enabled,
            self.notify_toggle,
        )
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::path())
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, self.to_text())?;
        fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let c = Config {
            method: Method::Vni,
            modern_tone: true,
            restore_invalid: false,
            underline: true,
            toggle_key: ToggleKey::AltZ,
            start_enabled: false,
            notify_toggle: true,
        };
        assert_eq!(Config::parse(&c.to_text()), c);
        assert_eq!(Config::parse("rác\nmethod = xyz"), Config::default());
    }
}
