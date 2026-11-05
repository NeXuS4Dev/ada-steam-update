//! OS integration: launch at login, desktop notifications, single instance.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use anyhow::{Context, Result};

pub const APP_NAME: &str = "Ada Steam Update";
/// Argument passed when the OS launches us at login.
pub const AUTOSTART_ARG: &str = "--autostart";

// ---------------------------------------------------------------------------
// Launch at login
// ---------------------------------------------------------------------------

fn auto_launch() -> Result<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().context("cannot locate executable")?;
    let exe = exe.to_string_lossy().to_string();
    // The Windows Run key is a command line: quote the path so installs under
    // "Program Files" (or any path with spaces) launch correctly.
    #[cfg(windows)]
    let exe = format!("\"{exe}\"");
    let mut builder = auto_launch::AutoLaunchBuilder::new();
    builder.set_app_name(APP_NAME).set_app_path(&exe).set_args(&[AUTOSTART_ARG]);
    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);
    builder.build().context("autostart is not supported on this system")
}

pub fn set_launch_at_startup(enabled: bool) -> Result<()> {
    let al = auto_launch()?;
    let current = al.is_enabled().unwrap_or(false);
    if enabled && !current {
        al.enable().context("could not register for launch at startup")?;
    } else if !enabled && current {
        al.disable().context("could not unregister launch at startup")?;
    } else if enabled {
        // Re-register so the entry follows the executable if it was moved.
        let _ = al.disable();
        al.enable().context("could not register for launch at startup")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

#[cfg(windows)]
const AUMID: &str = "io.github.AdaSteamUpdate";

/// Register an AppUserModelID so Windows toasts show our name and icon
/// instead of "Windows PowerShell". Per-user, no admin rights needed.
#[cfg(windows)]
pub fn register_notification_identity(icon_png: &std::path::Path) {
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    match key.create_subkey(format!(r"Software\Classes\AppUserModelId\{AUMID}")) {
        Ok((k, _)) => {
            let _ = k.set_value("DisplayName", &APP_NAME);
            let _ = k.set_value("IconUri", &icon_png.to_string_lossy().to_string());
            let _ = k.set_value("IconBackgroundColor", &"FF5B6CFF");
        }
        Err(e) => log::warn!("could not register notification identity: {e}"),
    }
}

#[cfg(not(windows))]
pub fn register_notification_identity(_icon_png: &std::path::Path) {}

pub fn notify(summary: &str, body: &str) {
    let mut n = notify_rust::Notification::new();
    n.summary(summary).body(body).appname(APP_NAME);
    #[cfg(windows)]
    n.app_id(AUMID);
    #[cfg(all(unix, not(target_os = "macos")))]
    n.icon("software-update-available");
    // Notifications are fire-and-forget; never block the worker on the OS.
    std::thread::spawn(move || {
        if let Err(e) = n.show() {
            log::warn!("notification failed: {e}");
        }
    });
}

// ---------------------------------------------------------------------------
// Single instance
// ---------------------------------------------------------------------------

const INSTANCE_PORT: u16 = 47_613;
const PROTO: &str = "ADA-STEAM-UPDATE/1";

/// A request from a second launch to the running instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// Bring the window to the front.
    Show,
    /// Exit the running instance (`--quit`).
    Quit,
}

impl Signal {
    fn as_str(self) -> &'static str {
        match self {
            Signal::Show => "SHOW",
            Signal::Quit => "QUIT",
        }
    }

    fn parse(line: &str) -> Option<Self> {
        match line.trim().strip_prefix(PROTO)?.trim() {
            "SHOW" => Some(Signal::Show),
            "QUIT" => Some(Signal::Quit),
            _ => None,
        }
    }
}

pub enum Instance {
    /// We are the primary instance; signals from later launches arrive on this listener.
    Primary(Option<TcpListener>),
    /// Another instance is running and has received our signal.
    Secondary,
}

fn instance_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], INSTANCE_PORT))
}

/// Ensure only one instance runs. A second launch asks the first to come to the front.
///
/// Uses a loopback socket with a tiny handshake, which works identically on every
/// platform and cleans itself up when the process dies (unlike lock files).
pub fn acquire_instance() -> Instance {
    match TcpListener::bind(instance_addr()) {
        Ok(l) => Instance::Primary(Some(l)),
        Err(_) => {
            if send_signal(Signal::Show).unwrap_or(false) {
                Instance::Secondary
            } else {
                // Port taken by something else: run without single-instance support.
                log::warn!("port {INSTANCE_PORT} is in use by another program");
                Instance::Primary(None)
            }
        }
    }
}

/// Send a signal to the running instance. `Ok(true)` if it acknowledged.
pub fn send_signal(signal: Signal) -> Result<bool> {
    let mut s = TcpStream::connect_timeout(&instance_addr(), Duration::from_millis(500))?;
    s.set_read_timeout(Some(Duration::from_millis(1500)))?;
    writeln!(s, "{PROTO} {}", signal.as_str())?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line)?;
    Ok(line.trim() == format!("{PROTO} OK"))
}

/// Serve signals from later launches forever.
pub fn serve_signals(listener: TcpListener, on_signal: impl Fn(Signal) + Send + 'static) {
    std::thread::Builder::new()
        .name("ada-instance".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                if let Some(signal) = Signal::parse(&line) {
                    let mut w = &stream;
                    let _ = writeln!(w, "{PROTO} OK");
                    on_signal(signal);
                }
            }
        })
        .expect("spawn instance thread");
}

/// One toast summarising a check run.
pub fn notify_updates(found: &[&crate::worker::FoundUpdate]) {
    match found {
        [] => {}
        [one] => notify(&format!("{} was updated", one.name), &format!("{} · {}", one.kind.label(), one.title)),
        many => {
            let names: Vec<&str> = many.iter().take(3).map(|f| f.name.as_str()).collect();
            let more = many.len().saturating_sub(3);
            let mut body = names.join(", ");
            if more > 0 {
                body.push_str(&format!(" and {more} more"));
            }
            notify(&format!("{} games were updated", many.len()), &body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_roundtrip() {
        for sig in [Signal::Show, Signal::Quit] {
            assert_eq!(
                Signal::parse(&format!(
                    "{PROTO} {}
",
                    sig.as_str()
                )),
                Some(sig)
            );
        }
        assert_eq!(Signal::parse("GET / HTTP/1.1"), None);
        assert_eq!(Signal::parse("ADA-STEAM-UPDATE/1 NOPE"), None);
    }
}
