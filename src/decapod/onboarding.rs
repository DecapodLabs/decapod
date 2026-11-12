//! Presentation and terminal lifecycle for the human-facing setup journey.
//! Project configuration and all mutations remain in the init command.

use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

pub(super) fn interactive(base_init: bool, proof: bool) -> bool {
    base_init && !proof && io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn color_enabled() -> bool {
    io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").is_ok_and(|term| term != "dumb")
}

pub(super) fn paint(text: &str, code: &str) -> String {
    if color_enabled() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub(super) fn accent(text: &str) -> String {
    paint(text, "1;36")
}

pub(super) fn muted(text: &str) -> String {
    paint(text, "90")
}

pub(super) fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

pub(super) fn width() -> usize {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(io::stdout().as_raw_fd(), libc::TIOCGWINSZ, &mut size) } == 0
            && size.ws_col > 0
        {
            return usize::from(size.ws_col).clamp(20, 88);
        }
    }
    80
}

pub(super) fn wrap(text: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in clean(text).split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > columns {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        for ch in word.chars() {
            if line.chars().count() == columns {
                lines.push(std::mem::take(&mut line));
            }
            line.push(ch);
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

pub(super) fn note(text: &str) {
    for line in wrap(text, width().saturating_sub(4)) {
        println!("  {}", muted(&line));
    }
}

pub(super) fn welcome(refresh: bool) {
    println!();
    println!(
        "  {}  {} {}",
        accent("◈"),
        paint("decapod", "1"),
        muted(env!("CARGO_PKG_VERSION"))
    );
    println!();
    println!(
        "  {}",
        paint(
            if refresh {
                "Make room for what's next."
            } else {
                "Good work starts here."
            },
            "1"
        )
    );
    note("A little context. A clear plan. Your agent takes it from here.");
    println!();
}

pub(super) fn step(current: usize, title: &str, description: &str) {
    println!();
    let steps = ["Project", "Preferences", "Review"];
    let trail = steps
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let item = format!(
                "{} {label}",
                if i < current {
                    "✓".to_string()
                } else {
                    format!("{:02}", i + 1)
                }
            );
            if i == current {
                accent(&item)
            } else {
                muted(&item)
            }
        })
        .collect::<Vec<_>>()
        .join("  ·  ");
    if width() >= 52 {
        println!("  {trail}");
    } else {
        println!(
            "  {}",
            accent(&format!("{:02}/03  {}", current + 1, steps[current]))
        );
    }
    println!();
    println!("  {}", paint(title, "1"));
    note(description);
}

pub(super) fn card_lines(rows: &[(&str, String)], columns: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for (label, value) in rows {
        let label = clean(label);
        let prefix = if columns >= 52 {
            format!("  │  {label:<12} ")
        } else {
            format!("  │  {label}: ")
        };
        let available = columns.saturating_sub(prefix.chars().count()).max(1);
        let wrapped = wrap(value, available);
        let continuation = format!(
            "  │{}",
            " ".repeat(prefix.chars().count().saturating_sub(3))
        );
        for (i, line) in wrapped.iter().enumerate() {
            lines.push(format!(
                "{}{line}",
                if i == 0 { &prefix } else { &continuation }
            ));
        }
    }
    lines
}

pub(super) fn card(title: &str, rows: &[(&str, String)]) {
    println!();
    println!("  {} {}", muted("╭─"), paint(&clean(title), "1"));
    for line in card_lines(rows, width()) {
        println!("{line}");
    }
    println!("  {}", muted("╰─"));
}

pub(super) fn field(label: &str, helper: &str, default: &str) {
    println!();
    println!("  {}", paint(&clean(label), "1"));
    if !helper.is_empty() {
        note(helper);
    }
    if !default.is_empty() {
        note(&format!("Enter to keep: {}", clean(default)));
    }
}

pub(super) fn ready(path: &Path, dry_run: bool, repair_count: usize) {
    println!();
    println!(
        "  {}  {}",
        accent(if dry_run { "◇" } else { "✓" }),
        paint(
            if dry_run {
                "Preview complete."
            } else {
                "Your project is ready for your agent."
            },
            "1"
        )
    );
    note(&path.display().to_string());
    println!();
    if dry_run {
        note("No setup changes were applied.");
    } else {
        println!(
            "  {}",
            paint(
                "Open your coding agent here and describe your next task.",
                "1"
            )
        );
        note(
            "For example: \"Read AGENTS.md, orient to this project, and help me build my next feature.\"",
        );
    }
    if repair_count > 0 {
        note(&format!(
            "Your agent has {repair_count} setup/proof follow-up(s) to review. Initialization does not run validation."
        ));
    } else {
        note("Initialization is complete. Required validation has not run.");
    }
    note("To revisit these choices, ask your agent to run decapod init --refresh.");
    println!();
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Key {
    Up,
    Down,
    Enter,
    Backspace,
    Cancel,
    Text(String),
    Ignore,
}

#[cfg(unix)]
pub(super) struct Terminal {
    tty: std::fs::File,
    saved: libc::termios,
}

#[cfg(unix)]
impl Terminal {
    pub(super) fn open() -> Option<Self> {
        use std::os::fd::AsRawFd;
        if !io::stdin().is_terminal()
            || !io::stdout().is_terminal()
            || std::env::var("TERM").is_ok_and(|term| term == "dumb")
        {
            return None;
        }
        let tty = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .ok()?;
        let mut saved = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(tty.as_raw_fd(), &mut saved) } != 0 {
            return None;
        }
        let mut raw = saved;
        unsafe {
            libc::cfmakeraw(&mut raw);
        }
        if unsafe { libc::tcsetattr(tty.as_raw_fd(), libc::TCSANOW, &raw) } != 0 {
            return None;
        }
        print!("\x1b[?25l");
        Some(Self { tty, saved })
    }

    fn byte(&mut self, timeout_ms: Option<i32>) -> io::Result<Option<u8>> {
        use std::os::fd::AsRawFd;
        if let Some(timeout) = timeout_ms {
            let mut poll = libc::pollfd {
                fd: self.tty.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut poll, 1, timeout) };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            if result == 0 {
                return Ok(None);
            }
        }
        let mut byte = [0];
        self.tty.read_exact(&mut byte)?;
        Ok(Some(byte[0]))
    }

    pub(super) fn key(&mut self) -> io::Result<Key> {
        let byte = self.byte(None)?.expect("blocking byte read");
        Ok(match byte {
            3 | 4 => Key::Cancel,
            b'\r' | b'\n' => Key::Enter,
            8 | 127 => Key::Backspace,
            27 => {
                // A lone Escape cancels immediately after a bounded wait.
                // Never wait indefinitely for an incomplete escape sequence.
                match self.byte(Some(60))? {
                    Some(b'[' | b'O') => match self.byte(Some(60))? {
                        Some(b'A') => Key::Up,
                        Some(b'B') => Key::Down,
                        _ => Key::Ignore,
                    },
                    None => Key::Cancel,
                    _ => Key::Ignore,
                }
            }
            32..=126 => Key::Text(char::from(byte).to_string()),
            0xc2..=0xf4 => {
                let len = if byte < 0xe0 {
                    2
                } else if byte < 0xf0 {
                    3
                } else {
                    4
                };
                let mut bytes = vec![byte];
                for _ in 1..len {
                    if let Some(next) = self.byte(Some(60))? {
                        bytes.push(next);
                    }
                }
                String::from_utf8(bytes)
                    .map(Key::Text)
                    .unwrap_or(Key::Ignore)
            }
            _ => Key::Ignore,
        })
    }
}

#[cfg(unix)]
impl Drop for Terminal {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::tcsetattr(self.tty.as_raw_fd(), libc::TCSANOW, &self.saved);
        }
        print!("\x1b[?25h");
        let _ = io::stdout().flush();
    }
}

#[cfg(not(unix))]
pub(super) struct Terminal;

#[cfg(not(unix))]
impl Terminal {
    pub(super) fn open() -> Option<Self> {
        None
    }
    pub(super) fn key(&mut self) -> io::Result<Key> {
        unreachable!("line-input fallback")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_cards_wrap_long_paths_and_preserve_unicode() {
        let rows = [(
            "Project",
            "/a/very/long/project/path/with/naïve/context".into(),
        )];
        for width in [28, 48, 80] {
            let lines = card_lines(&rows, width);
            assert!(lines.iter().all(|line| line.chars().count() <= width));
            let contents = lines
                .iter()
                .map(|line| line.trim_start_matches([' ', '│']))
                .collect::<String>();
            assert!(contents.contains("naïve"));
        }
    }

    #[test]
    fn terminal_content_never_contains_control_sequences() {
        let lines = card_lines(&[("Name", "demo\x1b[2J\n\r\tproject".into())], 80);
        assert!(lines.iter().all(|line| !line.chars().any(char::is_control)));
    }
}
