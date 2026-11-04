//! A small VT100/xterm-style terminal grid driven by the `vte` parser.

use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attrs {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strike: bool,
}

impl Default for Attrs {
    fn default() -> Self {
        Attrs {
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            strike: false,
        }
    }
}

/// `ch == '\0'` marks the right half of a double-width character.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub a: Attrs,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', a: Attrs::default() }
    }
}

pub type Row = Vec<Cell>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CursorShape {
    Block,
    Underline,
    Bar,
}

#[derive(Clone, Copy)]
struct Saved {
    x: usize,
    y: usize,
    attrs: Attrs,
    g0_lines: bool,
}

pub struct Term {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<Row>,
    pub scrollback: VecDeque<Row>,
    pub scrollback_limit: usize,
    /// How many lines were ever dropped off the top of the scrollback,
    /// so absolute line numbers stay stable.
    pub dropped: u64,
    pub cx: usize,
    pub cy: usize,
    wrap_pending: bool,
    pub attrs: Attrs,
    saved: Option<Saved>,
    alt_saved: Option<(Vec<Row>, Saved)>,
    top: usize,
    bot: usize,
    tabs: Vec<bool>,
    g0_lines: bool,
    last_char: char,
    // modes
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    pub cursor_visible: bool,
    pub autowrap: bool,
    pub insert_mode: bool,
    pub mouse_mode: u16,
    pub sgr_mouse: bool,
    pub cursor_shape: Option<CursorShape>,
    // outputs
    pub title: String,
    pub responses: Vec<u8>,
    pub bell: bool,
    pub dirty: bool,
}

fn blank_row(cols: usize, a: Attrs) -> Row {
    let mut c = Cell::default();
    c.a.bg = a.bg;
    vec![c; cols]
}

impl Term {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(2);
        let rows = rows.max(1);
        Term {
            cols,
            rows,
            lines: vec![blank_row(cols, Attrs::default()); rows],
            scrollback: VecDeque::new(),
            scrollback_limit: 10_000,
            dropped: 0,
            cx: 0,
            cy: 0,
            wrap_pending: false,
            attrs: Attrs::default(),
            saved: None,
            alt_saved: None,
            top: 0,
            bot: rows - 1,
            tabs: (0..cols).map(|i| i % 8 == 0).collect(),
            g0_lines: false,
            last_char: ' ',
            app_cursor: false,
            app_keypad: false,
            bracketed_paste: false,
            cursor_visible: true,
            autowrap: true,
            insert_mode: false,
            mouse_mode: 0,
            sgr_mouse: false,
            cursor_shape: None,
            title: String::new(),
            responses: Vec::new(),
            bell: false,
            dirty: true,
        }
    }

    pub fn in_alt_screen(&self) -> bool {
        self.alt_saved.is_some()
    }

    /// Total number of lines in history + screen.
    pub fn total_lines(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    /// Line by index into (scrollback ++ screen).
    pub fn line(&self, idx: usize) -> Option<&Row> {
        if idx < self.scrollback.len() {
            self.scrollback.get(idx)
        } else {
            self.lines.get(idx - self.scrollback.len())
        }
    }

    pub fn abs_to_idx(&self, abs: u64) -> Option<usize> {
        if abs < self.dropped {
            None
        } else {
            let i = (abs - self.dropped) as usize;
            (i < self.total_lines()).then_some(i)
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(2);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let a = Attrs::default();
        for r in self.lines.iter_mut() {
            r.resize(cols, Cell::default());
        }
        if let Some((saved, _)) = self.alt_saved.as_mut() {
            for r in saved.iter_mut() {
                r.resize(cols, Cell::default());
            }
            saved.resize(rows, blank_row(cols, a));
        }
        if rows < self.rows {
            // Push lines off the top (keeping the cursor visible), then trim the bottom.
            let push = if self.cy >= rows { self.cy + 1 - rows } else { 0 };
            for _ in 0..push {
                let l = self.lines.remove(0);
                if !self.in_alt_screen() {
                    self.push_scrollback(l);
                }
            }
            self.cy -= push;
            self.lines.truncate(rows);
        } else {
            // Pull history back down when the window grows (like most terminals do).
            let mut grow = rows - self.rows;
            while grow > 0 && !self.in_alt_screen() && !self.scrollback.is_empty() {
                let mut l = self.scrollback.pop_back().unwrap();
                l.resize(cols, Cell::default());
                self.lines.insert(0, l);
                self.cy += 1;
                grow -= 1;
            }
            while self.lines.len() < rows {
                self.lines.push(blank_row(cols, a));
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.top = 0;
        self.bot = rows - 1;
        self.tabs = (0..cols).map(|i| i % 8 == 0).collect();
        self.cx = self.cx.min(cols - 1);
        self.cy = self.cy.min(rows - 1);
        self.wrap_pending = false;
        self.dirty = true;
    }

    fn push_scrollback(&mut self, l: Row) {
        self.scrollback.push_back(l);
        while self.scrollback.len() > self.scrollback_limit {
            self.scrollback.pop_front();
            self.dropped += 1;
        }
    }

    /// Clear history and screen, keeping the cursor's line at the top.
    pub fn clear_buffer(&mut self) {
        self.dropped += self.scrollback.len() as u64;
        self.scrollback.clear();
        let keep = self.lines[self.cy].clone();
        self.dropped += self.cy as u64;
        for l in self.lines.iter_mut() {
            *l = blank_row(self.cols, Attrs::default());
        }
        self.lines[0] = keep;
        self.cy = 0;
        self.dirty = true;
    }

    fn blank(&self) -> Cell {
        let mut c = Cell::default();
        c.a.bg = self.attrs.bg;
        c
    }

    fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            let l = self.lines.remove(self.top);
            if self.top == 0 && !self.in_alt_screen() {
                self.push_scrollback(l);
            }
            self.lines.insert(self.bot, blank_row(self.cols, self.attrs));
        }
    }

    fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            self.lines.remove(self.bot);
            self.lines.insert(self.top, blank_row(self.cols, self.attrs));
        }
    }

    fn linefeed(&mut self) {
        if self.cy == self.bot {
            self.scroll_up(1);
        } else if self.cy + 1 < self.rows {
            self.cy += 1;
        }
    }

    fn reverse_index(&mut self) {
        if self.cy == self.top {
            self.scroll_down(1);
        } else if self.cy > 0 {
            self.cy -= 1;
        }
    }

    fn put_char(&mut self, c: char) {
        let c = if self.g0_lines { dec_line_drawing(c) } else { c };
        let w = c.width().unwrap_or(1);
        if w == 0 {
            return; // combining marks are not drawn separately
        }
        if self.wrap_pending && self.autowrap {
            self.cx = 0;
            self.linefeed();
        }
        self.wrap_pending = false;
        if w == 2 && self.cx + 1 >= self.cols {
            if self.autowrap {
                let b = self.blank();
                self.lines[self.cy][self.cx] = b;
                self.cx = 0;
                self.linefeed();
            } else {
                return;
            }
        }
        if self.insert_mode {
            let row = &mut self.lines[self.cy];
            for _ in 0..w {
                row.insert(self.cx, Cell::default());
                row.pop();
            }
        }
        let a = self.attrs;
        self.lines[self.cy][self.cx] = Cell { ch: c, a };
        if w == 2 {
            self.lines[self.cy][self.cx + 1] = Cell { ch: '\0', a };
        }
        self.last_char = c;
        if self.cx + w >= self.cols {
            self.cx = self.cols - 1;
            self.wrap_pending = true;
        } else {
            self.cx += w;
        }
    }

    fn erase_cells(&mut self, y: usize, from: usize, to: usize) {
        let b = self.blank();
        let row = &mut self.lines[y];
        for c in row.iter_mut().take(to.min(self.cols)).skip(from) {
            *c = b;
        }
    }

    fn save_cursor(&mut self) -> Saved {
        Saved { x: self.cx, y: self.cy, attrs: self.attrs, g0_lines: self.g0_lines }
    }

    fn restore(&mut self, s: Saved) {
        self.cx = s.x.min(self.cols - 1);
        self.cy = s.y.min(self.rows - 1);
        self.attrs = s.attrs;
        self.g0_lines = s.g0_lines;
        self.wrap_pending = false;
    }

    fn enter_alt(&mut self, save: bool) {
        if self.alt_saved.is_some() {
            return;
        }
        let cur = self.save_cursor();
        let screen = std::mem::replace(&mut self.lines, vec![blank_row(self.cols, Attrs::default()); self.rows]);
        self.alt_saved = Some((screen, cur));
        if !save {
            // keep cursor where it is
        }
        self.top = 0;
        self.bot = self.rows - 1;
    }

    fn leave_alt(&mut self, restore: bool) {
        if let Some((screen, cur)) = self.alt_saved.take() {
            self.lines = screen;
            if restore {
                self.restore(cur);
            }
            self.top = 0;
            self.bot = self.rows - 1;
        }
    }

    fn reset(&mut self) {
        let (c, r, sb, dropped, limit) = (self.cols, self.rows, std::mem::take(&mut self.scrollback), self.dropped, self.scrollback_limit);
        *self = Term::new(c, r);
        self.scrollback = sb;
        self.dropped = dropped;
        self.scrollback_limit = limit;
    }

    fn set_mode(&mut self, private: bool, mode: u16, on: bool) {
        if private {
            match mode {
                1 => self.app_cursor = on,
                7 => self.autowrap = on,
                25 => self.cursor_visible = on,
                47 | 1047 => {
                    if on { self.enter_alt(false) } else { self.leave_alt(false) }
                }
                1049 => {
                    if on { self.enter_alt(true) } else { self.leave_alt(true) }
                }
                1000 | 1002 | 1003 => self.mouse_mode = if on { mode } else { 0 },
                1006 => self.sgr_mouse = on,
                2004 => self.bracketed_paste = on,
                _ => {}
            }
        } else if mode == 4 {
            self.insert_mode = on;
        }
    }

    fn sgr(&mut self, params: &Params) {
        let v: Vec<Vec<u16>> = params.iter().map(|p| p.to_vec()).collect();
        if v.is_empty() {
            self.attrs = Attrs::default();
            return;
        }
        let mut i = 0;
        while i < v.len() {
            let p = &v[i];
            let code = p[0];
            match code {
                0 => self.attrs = Attrs::default(),
                1 => self.attrs.bold = true,
                2 => self.attrs.dim = true,
                3 => self.attrs.italic = true,
                4 => self.attrs.underline = !(p.len() > 1 && p[1] == 0),
                7 => self.attrs.inverse = true,
                9 => self.attrs.strike = true,
                21 => self.attrs.underline = true,
                22 => {
                    self.attrs.bold = false;
                    self.attrs.dim = false;
                }
                23 => self.attrs.italic = false,
                24 => self.attrs.underline = false,
                27 => self.attrs.inverse = false,
                29 => self.attrs.strike = false,
                30..=37 => self.attrs.fg = Color::Idx((code - 30) as u8),
                39 => self.attrs.fg = Color::Default,
                40..=47 => self.attrs.bg = Color::Idx((code - 40) as u8),
                49 => self.attrs.bg = Color::Default,
                90..=97 => self.attrs.fg = Color::Idx((code - 90 + 8) as u8),
                100..=107 => self.attrs.bg = Color::Idx((code - 100 + 8) as u8),
                38 | 48 | 58 => {
                    let mut col = None;
                    if p.len() > 1 {
                        // colon form 38:5:n or 38:2:[cs:]r:g:b
                        match p[1] {
                            5 if p.len() > 2 => col = Some(Color::Idx(p[2] as u8)),
                            2 if p.len() >= 6 => col = Some(Color::Rgb(p[3] as u8, p[4] as u8, p[5] as u8)),
                            2 if p.len() == 5 => col = Some(Color::Rgb(p[2] as u8, p[3] as u8, p[4] as u8)),
                            _ => {}
                        }
                    } else if i + 1 < v.len() {
                        match v[i + 1][0] {
                            5 if i + 2 < v.len() => {
                                col = Some(Color::Idx(v[i + 2][0] as u8));
                                i += 2;
                            }
                            2 if i + 4 < v.len() => {
                                col = Some(Color::Rgb(v[i + 2][0] as u8, v[i + 3][0] as u8, v[i + 4][0] as u8));
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                    if let Some(c) = col {
                        if code == 38 {
                            self.attrs.fg = c;
                        } else if code == 48 {
                            self.attrs.bg = c;
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

fn dec_line_drawing(c: char) -> char {
    match c {
        '`' => '◆',
        'a' => '▒',
        'f' => '°',
        'g' => '±',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        _ => c,
    }
}

impl Perform for Term {
    fn print(&mut self, c: char) {
        self.put_char(c);
        self.dirty = true;
    }

    fn execute(&mut self, byte: u8) {
        self.dirty = true;
        match byte {
            0x07 => self.bell = true,
            0x08 => {
                if self.wrap_pending {
                    self.wrap_pending = false;
                } else if self.cx > 0 {
                    self.cx -= 1;
                }
            }
            0x09 => {
                let mut x = self.cx + 1;
                while x < self.cols && !self.tabs[x] {
                    x += 1;
                }
                self.cx = x.min(self.cols - 1);
                self.wrap_pending = false;
            }
            0x0A..=0x0C => {
                self.linefeed();
                self.wrap_pending = false;
            }
            0x0D => {
                self.cx = 0;
                self.wrap_pending = false;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        self.dirty = true;
        let private = intermediates.first() == Some(&b'?');
        let ps: Vec<u16> = params.iter().map(|p| p[0]).collect();
        let arg = |i: usize, d: u16| -> usize {
            let v = ps.get(i).copied().unwrap_or(0);
            (if v == 0 { d } else { v }) as usize
        };
        let raw = |i: usize| -> usize { ps.get(i).copied().unwrap_or(0) as usize };
        let (cols, rows) = (self.cols, self.rows);
        match action {
            '@' => {
                let n = arg(0, 1).min(cols - self.cx);
                let b = self.blank();
                let row = &mut self.lines[self.cy];
                for _ in 0..n {
                    row.insert(self.cx, b);
                    row.pop();
                }
            }
            'A' => {
                let lim = if self.cy >= self.top { self.top } else { 0 };
                self.cy = self.cy.saturating_sub(arg(0, 1)).max(lim);
                self.wrap_pending = false;
            }
            'B' | 'e' => {
                let lim = if self.cy <= self.bot { self.bot } else { rows - 1 };
                self.cy = (self.cy + arg(0, 1)).min(lim);
                self.wrap_pending = false;
            }
            'C' | 'a' => {
                self.cx = (self.cx + arg(0, 1)).min(cols - 1);
                self.wrap_pending = false;
            }
            'D' => {
                self.cx = self.cx.saturating_sub(arg(0, 1));
                self.wrap_pending = false;
            }
            'E' => {
                self.cy = (self.cy + arg(0, 1)).min(rows - 1);
                self.cx = 0;
                self.wrap_pending = false;
            }
            'F' => {
                self.cy = self.cy.saturating_sub(arg(0, 1));
                self.cx = 0;
                self.wrap_pending = false;
            }
            'G' | '`' => {
                self.cx = (arg(0, 1) - 1).min(cols - 1);
                self.wrap_pending = false;
            }
            'H' | 'f' => {
                self.cy = (arg(0, 1) - 1).min(rows - 1);
                self.cx = (arg(1, 1) - 1).min(cols - 1);
                self.wrap_pending = false;
            }
            'd' => {
                self.cy = (arg(0, 1) - 1).min(rows - 1);
                self.wrap_pending = false;
            }
            'I' => {
                for _ in 0..arg(0, 1) {
                    let mut x = self.cx + 1;
                    while x < cols && !self.tabs[x] {
                        x += 1;
                    }
                    self.cx = x.min(cols - 1);
                }
            }
            'Z' => {
                for _ in 0..arg(0, 1) {
                    let mut x = self.cx.saturating_sub(1);
                    while x > 0 && !self.tabs[x] {
                        x -= 1;
                    }
                    self.cx = x;
                }
            }
            'J' => match raw(0) {
                0 => {
                    self.erase_cells(self.cy, self.cx, cols);
                    for y in self.cy + 1..rows {
                        self.erase_cells(y, 0, cols);
                    }
                }
                1 => {
                    for y in 0..self.cy {
                        self.erase_cells(y, 0, cols);
                    }
                    self.erase_cells(self.cy, 0, self.cx + 1);
                }
                2 => {
                    for y in 0..rows {
                        self.erase_cells(y, 0, cols);
                    }
                }
                3 => {
                    self.dropped += self.scrollback.len() as u64;
                    self.scrollback.clear();
                }
                _ => {}
            },
            'K' => match raw(0) {
                0 => self.erase_cells(self.cy, self.cx, cols),
                1 => self.erase_cells(self.cy, 0, self.cx + 1),
                2 => self.erase_cells(self.cy, 0, cols),
                _ => {}
            },
            'L' => {
                if self.cy >= self.top && self.cy <= self.bot {
                    for _ in 0..arg(0, 1).min(self.bot - self.cy + 1) {
                        self.lines.remove(self.bot);
                        self.lines.insert(self.cy, blank_row(cols, self.attrs));
                    }
                    self.cx = 0;
                }
            }
            'M' => {
                if self.cy >= self.top && self.cy <= self.bot {
                    for _ in 0..arg(0, 1).min(self.bot - self.cy + 1) {
                        self.lines.remove(self.cy);
                        self.lines.insert(self.bot, blank_row(cols, self.attrs));
                    }
                    self.cx = 0;
                }
            }
            'P' => {
                let n = arg(0, 1).min(cols - self.cx);
                let b = self.blank();
                let row = &mut self.lines[self.cy];
                for _ in 0..n {
                    row.remove(self.cx);
                    row.push(b);
                }
            }
            'X' => {
                let n = arg(0, 1);
                self.erase_cells(self.cy, self.cx, self.cx + n);
            }
            'S' if !private => self.scroll_up(arg(0, 1)),
            'T' if !private => self.scroll_down(arg(0, 1)),
            'b' => {
                let c = self.last_char;
                for _ in 0..arg(0, 1).min(2000) {
                    self.put_char(c);
                }
            }
            'g' => match raw(0) {
                0 => {
                    if self.cx < cols {
                        self.tabs[self.cx] = false
                    }
                }
                3 => self.tabs.iter_mut().for_each(|t| *t = false),
                _ => {}
            },
            'm' if intermediates.is_empty() => self.sgr(params),
            'r' if !private => {
                let t = arg(0, 1) - 1;
                let b = arg(1, rows as u16) - 1;
                if t < b && b < rows {
                    self.top = t;
                    self.bot = b;
                    self.cx = 0;
                    self.cy = 0;
                }
            }
            'h' => {
                for &m in &ps {
                    self.set_mode(private, m, true);
                }
            }
            'l' => {
                for &m in &ps {
                    self.set_mode(private, m, false);
                }
            }
            'n' => match raw(0) {
                5 => self.responses.extend_from_slice(b"\x1b[0n"),
                6 => {
                    let s = format!("\x1b[{};{}R", self.cy + 1, self.cx + 1);
                    self.responses.extend_from_slice(s.as_bytes());
                }
                _ => {}
            },
            'c' => {
                if intermediates.first() == Some(&b'>') {
                    self.responses.extend_from_slice(b"\x1b[>0;95;0c");
                } else if intermediates.is_empty() {
                    self.responses.extend_from_slice(b"\x1b[?1;2c");
                }
            }
            's' if !private => self.saved = Some(self.save_cursor()),
            'u' if !private => {
                if let Some(s) = self.saved {
                    self.restore(s);
                }
            }
            'q' if intermediates.first() == Some(&b' ') => {
                self.cursor_shape = match raw(0) {
                    0 => None,
                    1 | 2 => Some(CursorShape::Block),
                    3 | 4 => Some(CursorShape::Underline),
                    5 | 6 => Some(CursorShape::Bar),
                    _ => None,
                };
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        self.dirty = true;
        match (intermediates.first(), byte) {
            (None, b'7') => self.saved = Some(self.save_cursor()),
            (None, b'8') => {
                if let Some(s) = self.saved {
                    self.restore(s);
                }
            }
            (None, b'D') => self.linefeed(),
            (None, b'E') => {
                self.cx = 0;
                self.linefeed();
            }
            (None, b'M') => self.reverse_index(),
            (None, b'H') => {
                if self.cx < self.cols {
                    self.tabs[self.cx] = true;
                }
            }
            (None, b'c') => self.reset(),
            (None, b'=') => self.app_keypad = true,
            (None, b'>') => self.app_keypad = false,
            (Some(b'('), b'0') => self.g0_lines = true,
            (Some(b'('), _) => self.g0_lines = false,
            (Some(b'#'), b'8') => {
                for row in self.lines.iter_mut() {
                    for c in row.iter_mut() {
                        *c = Cell { ch: 'E', a: Attrs::default() };
                    }
                }
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.len() >= 2 && (params[0] == b"0" || params[0] == b"2") {
            self.title = String::from_utf8_lossy(params[1]).to_string();
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(t: &mut Term, s: &str) {
        let mut p = vte::Parser::new();
        p.advance(t, s.as_bytes());
    }

    fn text(t: &Term, y: usize) -> String {
        t.lines[y].iter().map(|c| c.ch).collect::<String>().trim_end().to_string()
    }

    #[test]
    fn prints_and_wraps() {
        let mut t = Term::new(5, 3);
        feed(&mut t, "hello world");
        assert_eq!(text(&t, 0), "hello");
        assert_eq!(text(&t, 1), " worl");
        assert_eq!(text(&t, 2), "d");
    }

    #[test]
    fn scrolls_into_history() {
        let mut t = Term::new(10, 2);
        feed(&mut t, "a\r\nb\r\nc");
        assert_eq!(t.scrollback.len(), 1);
        assert_eq!(text(&t, 0), "b");
        assert_eq!(text(&t, 1), "c");
    }

    #[test]
    fn cursor_moves_and_erases() {
        let mut t = Term::new(10, 3);
        feed(&mut t, "abcdef\x1b[1;3H\x1b[K");
        assert_eq!(text(&t, 0), "ab");
        feed(&mut t, "\x1b[2J\x1b[2;2HX");
        assert_eq!(text(&t, 1), " X");
    }

    #[test]
    fn colours() {
        let mut t = Term::new(10, 1);
        feed(&mut t, "\x1b[1;31mR\x1b[38;2;1;2;3mG\x1b[0mN");
        assert_eq!(t.lines[0][0].a.fg, Color::Idx(1));
        assert!(t.lines[0][0].a.bold);
        assert_eq!(t.lines[0][1].a.fg, Color::Rgb(1, 2, 3));
        assert_eq!(t.lines[0][2].a, Attrs::default());
    }

    #[test]
    fn alt_screen_restores() {
        let mut t = Term::new(10, 2);
        feed(&mut t, "keep\x1b[?1049h\x1b[2Jvim\x1b[?1049l");
        assert_eq!(text(&t, 0), "keep");
    }

    #[test]
    fn cursor_report() {
        let mut t = Term::new(10, 5);
        feed(&mut t, "\x1b[3;4H\x1b[6n");
        assert_eq!(t.responses, b"\x1b[3;4R");
    }
}
