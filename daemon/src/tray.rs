//! Biểu tượng V/E trên thanh trạng thái (giống Unikey) và menu chuột phải.

use crate::ibus::Shared;
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem};
use ksni::{Icon, MenuItem, ToolTip};
use std::sync::Arc;
use hfastkey_core::Method;

pub struct VnTray {
    pub shared: Arc<Shared>,
}

const GLYPH_V: [&str; 7] = ["X...X", "X...X", "X...X", "X...X", ".X.X.", ".X.X.", "..X.."];
const GLYPH_E: [&str; 7] = ["XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "XXXXX"];

/// Vẽ ô vuông bo góc có chữ V (đỏ) hoặc E (xanh), định dạng ARGB32.
fn draw(size: i32, vietnamese: bool) -> Icon {
    let (glyph, bg): (&[&str; 7], [u8; 3]) = if vietnamese {
        (&GLYPH_V, [0xc6, 0x28, 0x28])
    } else {
        (&GLYPH_E, [0x15, 0x65, 0xc0])
    };
    let scale = (size / 8).max(1);
    let gw = 5 * scale;
    let gh = 7 * scale;
    let x0 = (size - gw) / 2;
    let y0 = (size - gh) / 2;
    let r = size / 5;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            // Bo góc: bỏ điểm nằm ngoài cung tròn ở 4 góc.
            let cx = if x < r { r - x } else if x >= size - r { x - (size - r - 1) } else { 0 };
            let cy = if y < r { r - y } else if y >= size - r { y - (size - r - 1) } else { 0 };
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
                [0xff, bg[0], bg[1], bg[2]]
            };
            data.extend_from_slice(&px);
        }
    }
    Icon { width: size, height: size, data }
}

/// Mở hộp thoại cài đặt (hfastkey-settings nằm cùng thư mục với daemon).
pub fn open_settings() {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("hfastkey-settings")))
        .filter(|p| p.exists())
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "hfastkey-settings".into());
    if let Err(e) = std::process::Command::new(&exe).spawn() {
        eprintln!("hfastkey: không mở được hfastkey-settings: {e}");
    }
}

impl VnTray {
    fn change_config(&self, f: impl FnOnce(&mut hfastkey_core::Config)) {
        let mut cfg = self.shared.config.write().unwrap();
        f(&mut cfg);
        if let Err(e) = cfg.save() {
            eprintln!("hfastkey: không lưu được cài đặt: {e}");
        }
    }
}

impl ksni::Tray for VnTray {
    fn id(&self) -> String {
        "hfastkey".into()
    }

    fn title(&self) -> String {
        "HFastKey".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let on = self.shared.enabled();
        vec![draw(16, on), draw(24, on), draw(32, on), draw(48, on)]
    }

    fn tool_tip(&self) -> ToolTip {
        let cfg = self.shared.config();
        let method = match cfg.method {
            Method::Telex => "Telex",
            Method::Vni => "VNI",
        };
        let state = if self.shared.enabled() { "Tiếng Việt" } else { "English" };
        ToolTip {
            title: format!("HFastKey: {state}"),
            description: format!("Kiểu gõ {method}. {} để chuyển.", cfg.toggle_key.label()),
            ..Default::default()
        }
    }

    /// Bấm chuột trái: bật/tắt tiếng Việt.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.shared.store_enabled(!self.shared.enabled());
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let cfg = self.shared.config();
        vec![
            CheckmarkItem {
                label: format!("Bật tiếng Việt ({})", cfg.toggle_key.label()),
                checked: self.shared.enabled(),
                activate: Box::new(|t: &mut Self| t.shared.store_enabled(!t.shared.enabled())),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            RadioGroup {
                selected: match cfg.method {
                    Method::Telex => 0,
                    Method::Vni => 1,
                },
                select: Box::new(|t: &mut Self, i| {
                    t.change_config(|c| c.method = if i == 1 { Method::Vni } else { Method::Telex })
                }),
                options: vec![
                    RadioItem { label: "Kiểu gõ Telex".into(), ..Default::default() },
                    RadioItem { label: "Kiểu gõ VNI".into(), ..Default::default() },
                ],
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Tự khôi phục phím với từ sai".into(),
                checked: cfg.restore_invalid,
                activate: Box::new(|t: &mut Self| t.change_config(|c| c.restore_invalid = !c.restore_invalid)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Đặt dấu kiểu mới (oà, uý)".into(),
                checked: cfg.modern_tone,
                activate: Box::new(|t: &mut Self| t.change_config(|c| c.modern_tone = !c.modern_tone)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Hiện gạch chân khi gõ".into(),
                checked: cfg.underline,
                activate: Box::new(|t: &mut Self| t.change_config(|c| c.underline = !c.underline)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Bảng điều khiển...".into(),
                icon_name: "preferences-desktop-keyboard".into(),
                activate: Box::new(|_: &mut Self| open_settings()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Thoát HFastKey".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_: &mut Self| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}
