//! Windows Terminal backend.
//!
//! Windows Terminal has no API to focus a tab by PID, so a PowerShell helper
//! (`wt.ps1`) reads the agent's console title, finds the hosting window through
//! the pane's ConPTY window, and selects the tab with that title via UI
//! Automation. A tab shows only its focused pane's title, so a session in an
//! unfocused split pane (or in a renamed tab) surfaces as `Failed`.

use super::{interpret_wt_helper, JumpAttempt, TerminalJumper};
use std::os::windows::process::CommandExt;
use std::process::Command;

/// Keeps the helper off abtop's console so PowerShell startup cannot touch
/// the TUI's console state.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct WindowsTerminalJumper;

impl TerminalJumper for WindowsTerminalJumper {
    fn name(&self) -> &'static str {
        "wt"
    }

    fn try_jump(&self, pid: u32) -> JumpAttempt {
        let script = format!("$targetPid = {pid}\n{}", include_str!("wt.ps1"));
        match Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            Ok(o) if o.status.success() => {
                interpret_wt_helper(&String::from_utf8_lossy(&o.stdout))
            }
            Ok(o) => JumpAttempt::Failed(format!(
                "powershell error: {}",
                String::from_utf8_lossy(&o.stderr)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
            )),
            Err(e) => JumpAttempt::Failed(format!("powershell not runnable ({e})")),
        }
    }
}
