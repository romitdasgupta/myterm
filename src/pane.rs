//! One shell session: a pseudo-terminal, the child shell and its screen.

use crate::term::Term;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

/// A position in the buffer: absolute line number and column.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Pos {
    pub line: u64,
    pub col: usize,
}

pub struct Pane {
    pub id: u64,
    pub term: Term,
    parser: vte::Parser,
    writer: Option<Box<dyn Write + Send>>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn Child + Send + Sync>>,
    rx: Option<Receiver<Vec<u8>>>,
    pub exited: bool,
    /// Lines scrolled up from the bottom (0 = following output).
    pub scroll: usize,
    pub sel: Option<(Pos, Pos)>,
    pub selecting: bool,
    pub wheel_acc: f32,
    pub bell_at: f64,
    pub shell_name: String,
    pub error: Option<String>,
    /// Bumped whenever new output arrives.
    pub gen: usize,
}

impl Pane {
    pub fn spawn(id: u64, cols: usize, rows: usize, ctx: egui::Context) -> Pane {
        let mut pane = Pane {
            id,
            term: Term::new(cols, rows),
            parser: vte::Parser::new(),
            writer: None,
            master: None,
            child: None,
            rx: None,
            exited: false,
            scroll: 0,
            sel: None,
            selecting: false,
            wheel_acc: 0.0,
            bell_at: -10.0,
            shell_name: std::env::var("SHELL")
                .ok()
                .and_then(|s| s.rsplit('/').next().map(|s| s.to_string()))
                .unwrap_or_else(|| "shell".into()),
            error: None,
            gen: 0,
        };
        if let Err(e) = pane.start(cols, rows, ctx) {
            pane.error = Some(e.to_string());
            pane.feed(format!("\x1b[31mCould not start a shell: {e}\x1b[0m\r\n").as_bytes());
        }
        pane
    }

    fn start(&mut self, cols: usize, rows: usize, ctx: egui::Context) -> anyhow_lite::Result<()> {
        let pty = native_pty_system();
        let pair = pty.openpty(PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 })?;
        let mut cmd = CommandBuilder::new_default_prog();
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "MyTerm");
        if std::env::var("LANG").is_err() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        if let Ok(home) = std::env::var("HOME") {
            cmd.cwd(home);
        }
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let (tx, rx) = channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                }
            }
            drop(tx);
            ctx.request_repaint();
        });
        self.writer = Some(writer);
        self.master = Some(pair.master);
        self.child = Some(child);
        self.rx = Some(rx);
        Ok(())
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    /// Drain shell output. Returns true if anything changed.
    pub fn pump(&mut self, now: f64) -> bool {
        let mut got = false;
        let mut done = false;
        if let Some(rx) = &self.rx {
            let mut chunks = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(b) => chunks.push(b),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
            for b in chunks {
                got = true;
                self.gen += 1;
                let before = self.term.total_lines() as u64 + self.term.dropped;
                self.feed(&b);
                // Keep the view steady while the person is reading history.
                if self.scroll > 0 {
                    let after = self.term.total_lines() as u64 + self.term.dropped;
                    self.scroll = (self.scroll + (after - before) as usize).min(self.term.scrollback.len());
                }
            }
        }
        if !self.term.responses.is_empty() {
            let r = std::mem::take(&mut self.term.responses);
            self.write(&r);
        }
        if self.term.bell {
            self.term.bell = false;
            self.bell_at = now;
        }
        if done {
            self.exited = true;
            self.rx = None;
        }
        got
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if let Some(w) = self.writer.as_mut() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if self.term.bracketed_paste {
            self.write(b"\x1b[200~");
            self.write(text.as_bytes());
            self.write(b"\x1b[201~");
        } else {
            self.write(text.as_bytes());
        }
        self.scroll = 0;
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols == self.term.cols && rows == self.term.rows {
            return;
        }
        self.term.resize(cols, rows);
        if let Some(m) = &self.master {
            let _ = m.resize(PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 });
        }
        self.scroll = self.scroll.min(self.term.scrollback.len());
    }

    pub fn title(&self) -> String {
        if !self.term.title.is_empty() {
            self.term.title.clone()
        } else {
            self.shell_name.clone()
        }
    }

    /// Absolute line number shown at screen row `row`.
    pub fn view_line(&self, row: usize) -> u64 {
        let idx = self.term.scrollback.len() - self.scroll + row;
        self.term.dropped + idx as u64
    }

    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.sel?;
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        if a == b {
            return None;
        }
        let mut out = String::new();
        for line in a.line..=b.line {
            let Some(idx) = self.term.abs_to_idx(line) else { continue };
            let row = self.term.line(idx).unwrap();
            let from = if line == a.line { a.col } else { 0 };
            let to = if line == b.line { b.col } else { row.len() };
            let s: String = row
                .iter()
                .skip(from)
                .take(to.saturating_sub(from))
                .filter(|c| c.ch != '\0')
                .map(|c| c.ch)
                .collect();
            out.push_str(s.trim_end());
            if line != b.line {
                out.push('\n');
            }
        }
        Some(out)
    }

    /// Select the word under `p` (double-click).
    pub fn select_word(&mut self, p: Pos) {
        let Some(idx) = self.term.abs_to_idx(p.line) else { return };
        let row = self.term.line(idx).unwrap();
        let is_word = |c: char| c.is_alphanumeric() || "-_./~:@%+".contains(c);
        if p.col >= row.len() || !is_word(row[p.col].ch) {
            return;
        }
        let mut s = p.col;
        while s > 0 && is_word(row[s - 1].ch) {
            s -= 1;
        }
        let mut e = p.col;
        while e + 1 < row.len() && is_word(row[e + 1].ch) {
            e += 1;
        }
        self.sel = Some((Pos { line: p.line, col: s }, Pos { line: p.line, col: e + 1 }));
    }

    pub fn select_line(&mut self, p: Pos) {
        let cols = self.term.cols;
        self.sel = Some((Pos { line: p.line, col: 0 }, Pos { line: p.line, col: cols }));
    }

    /// Find all (case-insensitive) matches of `needle`. Returns (line, col, len).
    pub fn find(&self, needle: &str) -> Vec<(u64, usize, usize)> {
        let needle: Vec<char> = needle.to_lowercase().chars().collect();
        let mut out = Vec::new();
        if needle.is_empty() {
            return out;
        }
        for idx in 0..self.term.total_lines() {
            let row = self.term.line(idx).unwrap();
            let chars: Vec<char> = row.iter().map(|c| c.ch.to_lowercase().next().unwrap_or(c.ch)).collect();
            if chars.len() < needle.len() {
                continue;
            }
            let mut i = 0;
            while i + needle.len() <= chars.len() {
                if chars[i..i + needle.len()] == needle[..] {
                    out.push((self.term.dropped + idx as u64, i, needle.len()));
                    i += needle.len();
                } else {
                    i += 1;
                }
            }
        }
        out
    }

    /// Scroll so that absolute `line` is visible.
    pub fn reveal(&mut self, line: u64) {
        let Some(idx) = self.term.abs_to_idx(line) else { return };
        let sb = self.term.scrollback.len();
        let top = sb - self.scroll;
        let rows = self.term.rows;
        if idx < top || idx >= top + rows {
            let want_top = idx.saturating_sub(rows / 2).min(sb);
            self.scroll = sb - want_top;
        }
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
        }
    }
}

/// Tiny error helper so we don't need another dependency.
mod anyhow_lite {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
}
