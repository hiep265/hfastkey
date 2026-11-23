//! hfastkey-daemon: bộ gõ tiếng Việt độc lập, viết hoàn toàn bằng Rust.
//!
//! Chương trình tự đóng vai IBus trên session D-Bus (giống cách fcitx5 làm),
//! nên gnome-shell và các ứng dụng GTK/Qt gửi phím thẳng tới đây, không cần
//! fcitx5 hay ibus-daemon.

mod ibus;
mod keys;
mod tray;

use futures_util::StreamExt;
use ibus::{remove_context, IBusPortal, IBusRoot, Shared};
use keys::EnterMode;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use hfastkey_core::{Config, Method};
use zbus::fdo::{self, RequestNameFlags, RequestNameReply};

const USAGE: &str = "\
hfastkey-daemon: bộ gõ tiếng Việt (Telex/VNI) viết bằng Rust

Cách dùng: hfastkey-daemon [tuỳ chọn]
Cài đặt nằm trong ~/.config/hfastkey/config (sửa bằng hfastkey-settings hoặc menu
biểu tượng V/E); daemon tự đọc lại khi tệp thay đổi.

  --off               khởi động ở chế độ tắt tiếng Việt
  --enter=passthrough (chế độ gạch chân) chốt chữ rồi để phím Enter gốc đi tiếp
  --no-tray           không hiện biểu tượng V/E
  --debug             in log từng phím
  --no-address-file   không ghi ~/.config/ibus/bus/* (dùng khi test)";

struct Args {
    enter: EnterMode,
    force_off: bool,
    tray: bool,
    debug: bool,
    address_file: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        enter: EnterMode::Forward,
        force_off: false,
        tray: true,
        debug: std::env::var_os("HFASTKEY_DEBUG").is_some(),
        address_file: true,
    };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--enter=passthrough" => a.enter = EnterMode::Passthrough,
            "--enter=forward" => a.enter = EnterMode::Forward,
            "--off" => a.force_off = true,
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

    let mut v = vec![
        dir.join(format!("{mid}-unix-{wayland}")),
        dir.join(format!("{mid}-{host}-{num}")),
    ];
    v.dedup();
    v
}

fn session_address() -> String {
    std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| {
        let rt = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".into());
        format!("unix:path={rt}/bus")
    })
}

/// Gọi `ibus exit` nếu có ibus-daemon thật đang chạy.
const IBUS_NAME: &str = "org.freedesktop.IBus";
const GNOME_IBUS_SERVICE: &str = "org.freedesktop.IBus.session.GNOME.service";

/// Dừng ibus-daemon mà GNOME tự bật (dịch vụ systemd của người dùng).
fn stop_gnome_ibus_service() {
    let active = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", GNOME_IBUS_SERVICE])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if active {
        eprintln!("hfastkey: dừng ibus của GNOME ({GNOME_IBUS_SERVICE})");
        let _ = Command::new("systemctl").args(["--user", "stop", GNOME_IBUS_SERVICE]).status();
    }
}

/// Giành tên org.freedesktop.IBus trên session bus.
///
/// Xếp hàng chờ tên (không dùng DoNotQueue): khi chương trình đang giữ tên
/// thoát, D-Bus chuyển tên ngay cho HFastKey, không có khoảng trống nào để
/// D-Bus tự bật lại ibus-daemon.
async fn acquire_ibus_name(conn: &zbus::Connection) -> zbus::Result<bool> {
    match conn
        .request_name_with_flags(IBUS_NAME, RequestNameFlags::ReplaceExisting.into())
        .await
    {
        Ok(RequestNameReply::PrimaryOwner) | Ok(RequestNameReply::AlreadyOwner) => return Ok(true),
        Ok(RequestNameReply::InQueue) => {}
        other => {
            eprintln!("hfastkey: không xin được tên {IBUS_NAME}: {other:?}");
            return Ok(false);
        }
    }
    let me = conn.unique_name().map(|n| n.to_string()).unwrap_or_default();
    let dbus = fdo::DBusProxy::new(conn).await?;
    let name = zbus::names::BusName::try_from(IBUS_NAME)?;
    for attempt in 0..40 {
        let owner = dbus.get_name_owner(name.clone()).await.map(|o| o.to_string()).unwrap_or_default();
        if owner == me {
            eprintln!("hfastkey: đã giành được {IBUS_NAME}");
            return Ok(true);
        }
        if attempt % 10 == 0 && !owner.is_empty() {
            let pid = match zbus::names::BusName::try_from(owner.as_str()) {
                Ok(b) => dbus.get_connection_unix_process_id(b).await.unwrap_or(0),
                Err(_) => 0,
            };
            let comm = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            match comm.trim() {
                "ibus-daemon" => {
                    eprintln!("hfastkey: ibus-daemon (pid {pid}) đang giữ {IBUS_NAME}, dừng nó...");
                    stop_gnome_ibus_service();
                    if attempt > 0 {
                        let _ = Command::new("kill").arg(pid.to_string()).status();
                    }
                }
                "fcitx5" => {
                    eprintln!("hfastkey: fcitx5 đang chạy và giữ {IBUS_NAME}; hãy tắt fcitx5 trước (`pkill fcitx5`).");
                    return Ok(false);
                }
                other => eprintln!("hfastkey: {IBUS_NAME} đang bị {other:?} (pid {pid}) giữ, chờ..."),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("hfastkey: hết thời gian chờ {IBUS_NAME}.");
    Ok(false)
}

fn ask_ibus_to_exit() {
    match Command::new("ibus").arg("exit").output() {
        Ok(_) => {}
        Err(e) => eprintln!("hfastkey: không chạy được `ibus exit`: {e}"),
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

fn config_mtime() -> Option<SystemTime> {
    fs::metadata(Config::path()).and_then(|m| m.modified()).ok()
}

/// Luồng nền: đọc lại cài đặt khi tệp đổi; giành lại file địa chỉ nếu
/// ibus-daemon khởi động sau và ghi đè.
fn spawn_watcher(shared: Arc<Shared>, ours: Option<String>) {
    std::thread::Builder::new()
        .name("hfastkey-watch".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            let mut mtime = config_mtime();
            let mut tick = 0u32;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                tick = tick.wrapping_add(1);

                let now = config_mtime();
                if now != mtime {
                    mtime = now;
                    let cfg = Config::load();
                    let changed = *shared.config.read().unwrap() != cfg;
                    if changed {
                        eprintln!("hfastkey: đã đọc lại cài đặt");
                        *shared.config.write().unwrap() = cfg;
                        shared.refresh_ui();
                    }
                }

                let Some(ours) = ours.as_deref() else { continue };
                if tick % 5 != 0 {
                    continue;
                }
                let mut stale = false;
                for path in address_files() {
                    let current = fs::read_to_string(&path).unwrap_or_default();
                    if current != ours {
                        if file_owned_by_ibus_daemon(&current) {
                            eprintln!("hfastkey: ibus-daemon vừa ghi đè {}, yêu cầu nó thoát.", path.display());
                            stop_gnome_ibus_service();
                            ask_ibus_to_exit();
                        }
                        stale = true;
                    }
                }
                if stale {
                    write_files(ours);
                }
            }
        })
        .ok();
}

fn write_address_files() -> String {
    let mut addr = session_address();
    while addr.ends_with(';') {
        addr.pop();
    }
    // Thêm một tham số ngẫu nhiên để thư viện IBus thấy địa chỉ đổi và kết nối lại.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let content = format!("IBUS_ADDRESS={addr},hfastkey_random_string={nanos:x}{pid:x}\nIBUS_DAEMON_PID={pid}\n");
    write_files(&content);
    content
}

fn write_files(content: &str) {
    for path in address_files() {
        let res = (|| -> std::io::Result<()> {
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
            Ok(()) => eprintln!("hfastkey: đã ghi địa chỉ vào {}", path.display()),
            Err(e) => eprintln!("hfastkey: không ghi được {}: {e}", path.display()),
        }
    }
}

async fn run(args: Args) -> zbus::Result<()> {
    let config = Config::load();
    let enabled = config.start_enabled && !args.force_off;
    let method = match config.method {
        Method::Telex => "Telex",
        Method::Vni => "VNI",
    };
    let toggle = config.toggle_key.label();
    let shared: Arc<Shared> = Shared::new(config, args.enter, enabled, args.debug);

    let conn = zbus::connection::Builder::session()?
        .serve_at("/org/freedesktop/IBus", IBusRoot { shared: shared.clone() })?
        .build()
        .await?;
    if !acquire_ibus_name(&conn).await? {
        std::process::exit(1);
    }
    stop_gnome_ibus_service();
    let flags = RequestNameFlags::ReplaceExisting | RequestNameFlags::DoNotQueue;

    let portal = zbus::connection::Builder::session()?
        .serve_at("/org/freedesktop/IBus", IBusPortal { shared: shared.clone() })?
        .build()
        .await?;
    if let Err(e) = portal
        .request_name_with_flags("org.freedesktop.portal.IBus", flags)
        .await
    {
        eprintln!("hfastkey: không chiếm được org.freedesktop.portal.IBus ({e}); ứng dụng Flatpak có thể không gõ được.");
    }

    let ours = args.address_file.then(write_address_files);
    spawn_watcher(shared.clone(), ours);

    if args.tray {
        use ksni::blocking::TrayMethods;
        // Chờ thanh trạng thái (tiện ích AppIndicator) sẵn sàng nếu nó bật sau HFastKey.
        match (tray::VnTray { shared: shared.clone() }).assume_sni_available(true).spawn() {
            Ok(handle) => {
                *shared.on_toggle.lock().unwrap() = Some(Box::new(move |_on| {
                    handle.update(|_| {});
                }));
            }
            Err(e) => eprintln!("hfastkey: không tạo được biểu tượng V/E: {e}"),
        }
    }

    eprintln!(
        "hfastkey: đang chạy ({method}, tiếng Việt {}). {toggle} để bật/tắt.",
        if enabled { "bật" } else { "tắt" }
    );

    // Dọn ô nhập liệu khi ứng dụng thoát; thoát nếu bị chương trình khác chiếm tên.
    let me = conn.unique_name().map(|n| n.to_string()).unwrap_or_default();
    let dbus = fdo::DBusProxy::new(&conn).await?;
    let mut changes = dbus.receive_name_owner_changed().await?;
    while let Some(sig) = changes.next().await {
        let Ok(a) = sig.args() else { continue };
        let name = a.name().to_string();
        if name == "org.freedesktop.IBus" {
            let new_owner = a.new_owner().as_ref().map(|n| n.to_string()).unwrap_or_default();
            if new_owner != me {
                eprintln!("hfastkey: chương trình khác ({new_owner}) đã chiếm org.freedesktop.IBus, thoát.");
                std::process::exit(0);
            }
            continue;
        }
        if a.new_owner().is_none() {
            let gone = shared.clients.lock().unwrap().remove(&name);
            for (is_portal, path) in gone.unwrap_or_default() {
                remove_context(if is_portal { &portal } else { &conn }, &path).await;
            }
        }
    }
    Ok(())
}

fn main() {
    let args = parse_args();
    if let Err(e) = zbus::block_on(run(args)) {
        eprintln!("hfastkey: lỗi D-Bus: {e}");
        std::process::exit(1);
    }
}
