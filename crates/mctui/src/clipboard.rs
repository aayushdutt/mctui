//! Copying to the system clipboard: through the terminal with OSC 52 (which
//! works over SSH), and with the platform's clipboard tool where there is
//! one, since not every terminal allows OSC 52. Both are best effort.

use std::io::Write as _;
use std::process::{Command, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// Puts `text` on the clipboard.
pub fn copy(text: &str) {
    let mut out = std::io::stdout();
    if let Err(e) = out
        .write_all(osc52(text).as_bytes())
        .and_then(|()| out.flush())
    {
        tracing::debug!(error = %e, "couldn't write OSC 52");
    }
    if let Some((program, args)) = tool() {
        let text = text.to_owned();
        // A detached thread: the tool may take a moment, and the frame shouldn't wait.
        std::thread::spawn(move || {
            if let Err(e) = run(program, args, &text) {
                tracing::debug!(program, error = %e, "couldn't copy with the clipboard tool");
            }
        });
    }
}

/// The escape sequence that asks the terminal to set its clipboard.
fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", STANDARD.encode(text))
}

/// The platform's clipboard tool and its arguments.
fn tool() -> Option<(&'static str, &'static [&'static str])> {
    if cfg!(target_os = "macos") {
        Some(("pbcopy", &[]))
    } else if cfg!(windows) {
        Some(("clip", &[]))
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Some(("wl-copy", &[]))
    } else if std::env::var_os("DISPLAY").is_some() {
        Some(("xclip", &["-selection", "clipboard"]))
    } else {
        None
    }
}

fn run(program: &str, args: &[&str], text: &str) -> std::io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_carries_base64() {
        assert_eq!(osc52("F7K9ZQ2X"), "\x1b]52;c;RjdLOVpRMlg=\x07");
    }
}
