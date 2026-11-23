//! hfastkey-settings: Bảng điều khiển của HFastKey, bố cục theo kiểu Unikey.
//! Mỗi thay đổi được lưu ngay vào ~/.config/hfastkey/config; hfastkey-daemon tự đọc lại.

use gtk::prelude::*;
use gtk::{glib, Align, Application, ApplicationWindow, Box as GBox, Button, CheckButton, DropDown, Frame, Grid, Label, Orientation};
use std::cell::RefCell;
use std::rc::Rc;
use hfastkey_core::{Config, Method, ToggleKey};

type Shared = Rc<RefCell<Config>>;

fn save(cfg: &Shared) {
    if let Err(e) = cfg.borrow().save() {
        eprintln!("hfastkey-settings: không lưu được cài đặt: {e}");
    }
}

fn daemon_running() -> bool {
    std::fs::read_dir("/proc")
        .map(|it| {
            it.flatten().any(|e| {
                std::fs::read_to_string(e.path().join("comm"))
                    .map(|c| c.trim() == "hfastkey-daemon")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn start_daemon() {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("hfastkey-daemon")))
        .filter(|p| p.exists())
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "hfastkey-daemon".into());
    if let Err(e) = std::process::Command::new(exe).spawn() {
        eprintln!("hfastkey-settings: không chạy được hfastkey-daemon: {e}");
    }
}

fn label(text: &str) -> Label {
    Label::builder().label(text).halign(Align::Start).build()
}

fn frame(title: &str, child: &impl IsA<gtk::Widget>) -> Frame {
    let f = Frame::builder().label(title).child(child).build();
    f.set_margin_bottom(6);
    f
}

/// Một ô tích gắn với một trường bool của Config.
fn check(cfg: &Shared, text: &str, get: fn(&Config) -> bool, set: fn(&mut Config, bool)) -> CheckButton {
    let b = CheckButton::with_label(text);
    b.set_active(get(&cfg.borrow()));
    let cfg = cfg.clone();
    b.connect_toggled(move |b| {
        set(&mut cfg.borrow_mut(), b.is_active());
        save(&cfg);
    });
    b
}

fn build(app: &Application) {
    let cfg: Shared = Rc::new(RefCell::new(Config::load()));

    // --- Điều khiển ---
    let grid = Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .margin_top(10)
        .margin_bottom(10)
        .margin_start(12)
        .margin_end(12)
        .build();

    let charset = DropDown::from_strings(&["Unicode"]);
    charset.set_sensitive(false);
    charset.set_tooltip_text(Some("HFastKey luôn gõ ra Unicode dựng sẵn"));
    grid.attach(&label("Bảng mã:"), 0, 0, 1, 1);
    grid.attach(&charset, 1, 0, 1, 1);

    let method = DropDown::from_strings(&["Telex", "VNI"]);
    method.set_selected(match cfg.borrow().method {
        Method::Telex => 0,
        Method::Vni => 1,
    });
    {
        let cfg = cfg.clone();
        method.connect_selected_notify(move |dd| {
            cfg.borrow_mut().method = if dd.selected() == 1 { Method::Vni } else { Method::Telex };
            save(&cfg);
        });
    }
    grid.attach(&label("Kiểu gõ:"), 0, 1, 1, 1);
    grid.attach(&method, 1, 1, 1, 1);

    let toggles = [ToggleKey::CtrlShift, ToggleKey::AltZ, ToggleKey::CtrlSpace];
    let toggle_box = GBox::new(Orientation::Horizontal, 10);
    let mut toggle_buttons: Vec<CheckButton> = Vec::new();
    for t in toggles {
        let b = CheckButton::with_label(t.label());
        if let Some(first) = toggle_buttons.first() {
            b.set_group(Some(first));
        }
        b.set_active(cfg.borrow().toggle_key == t);
        let cfg2 = cfg.clone();
        b.connect_toggled(move |b| {
            if b.is_active() {
                cfg2.borrow_mut().toggle_key = t;
                save(&cfg2);
            }
        });
        toggle_box.append(&b);
        toggle_buttons.push(b);
    }
    grid.attach(&label("Phím chuyển:"), 0, 2, 1, 1);
    grid.attach(&toggle_box, 1, 2, 1, 1);

    // --- Tuỳ chọn khác ---
    let opts = GBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();
    let c_restore = check(&cfg, "Tự động khôi phục phím với từ sai (gõ tiếng Anh dễ hơn)", |c| c.restore_invalid, |c, v| c.restore_invalid = v);
    let c_modern = check(&cfg, "Đặt dấu kiểu mới: oà, uý (thay vì òa, úy)", |c| c.modern_tone, |c, v| c.modern_tone = v);
    let c_underline = check(&cfg, "Hiện gạch chân khi đang gõ (cách cũ, không khuyến khích)", |c| c.underline, |c, v| c.underline = v);
    let c_start = check(&cfg, "Bật tiếng Việt khi khởi động", |c| c.start_enabled, |c, v| c.start_enabled = v);
    let c_notify = check(&cfg, "Hiện thông báo khi bật/tắt tiếng Việt", |c| c.notify_toggle, |c, v| c.notify_toggle = v);
    for c in [&c_restore, &c_modern, &c_underline, &c_start, &c_notify] {
        opts.append(c);
    }

    // --- Nút ---
    let status = Label::builder().halign(Align::Start).hexpand(true).build();
    status.add_css_class("dim-label");
    let run_btn = Button::with_label("Chạy HFastKey");
    let refresh_status = {
        let status = status.clone();
        let run_btn = run_btn.clone();
        move || {
            let running = daemon_running();
            status.set_label(if running { "HFastKey đang chạy" } else { "HFastKey chưa chạy" });
            run_btn.set_visible(!running);
        }
    };
    refresh_status();
    {
        let refresh_status = refresh_status.clone();
        run_btn.connect_clicked(move |_| {
            start_daemon();
            let r = refresh_status.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || r());
        });
    }

    let defaults = Button::with_label("Mặc định");
    {
        let method = method.clone();
        let toggle_buttons = toggle_buttons.clone();
        let checks = [c_restore.clone(), c_modern.clone(), c_underline.clone(), c_start.clone(), c_notify.clone()];
        defaults.connect_clicked(move |_| {
            let d = Config::default();
            method.set_selected(if d.method == Method::Vni { 1 } else { 0 });
            for (b, t) in toggle_buttons.iter().zip(toggles) {
                if t == d.toggle_key {
                    b.set_active(true);
                }
            }
            let vals = [d.restore_invalid, d.modern_tone, d.underline, d.start_enabled, d.notify_toggle];
            for (b, v) in checks.iter().zip(vals) {
                b.set_active(v);
            }
        });
    }
    let close = Button::with_label("Đóng");
    close.add_css_class("suggested-action");

    let buttons = GBox::new(Orientation::Horizontal, 8);
    buttons.append(&status);
    buttons.append(&run_btn);
    buttons.append(&defaults);
    buttons.append(&close);

    let root = GBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    root.append(&frame("Điều khiển", &grid));
    root.append(&frame("Tuỳ chọn khác", &opts));
    root.append(&buttons);

    let win = ApplicationWindow::builder()
        .application(app)
        .title("HFastKey - Bảng điều khiển")
        .resizable(false)
        .child(&root)
        .build();
    {
        let win = win.clone();
        close.connect_clicked(move |_| win.close());
    }
    win.present();
}

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("io.github.hiep265.hfastkey.Settings")
        .build();
    app.connect_activate(build);
    app.run()
}
