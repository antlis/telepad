//! Thin rofi wrapper: feed lines on stdin, get the chosen index back.

use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

/// Show a dmenu list and return the 0-based index of the selection,
/// or `None` if the user cancelled.
pub fn pick(prompt: &str, lines: &[String]) -> Result<Option<usize>> {
    let mut child = Command::new("rofi")
        .args([
            "-dmenu",
            "-i",
            "-p",
            prompt,
            "-format",
            "i",
            "-theme-str",
            "window { fullscreen: true; } mainbox { padding: 2%; }",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(lines.join("\n").as_bytes())?;
        stdin.flush()?;
    }

    let output = child.wait_with_output()?;
    let picked = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if picked.is_empty() {
        return Ok(None);
    }
    Ok(picked.parse::<usize>().ok())
}
