//! Painting one pane and handling its mouse.

use crate::pane::{Pane, Pos};
use crate::term::CursorShape;
use crate::theme::Theme;
use egui::{pos2, text::LayoutJob, vec2, Color32, CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind, TextFormat, Ui, Vec2};
use std::collections::HashMap;

pub const PAD: f32 = 5.0;

pub struct DrawCtx<'a> {
    pub theme: &'a Theme,
    pub font: FontId,
    pub cell: Vec2,
    pub cursor: CursorShape,
    pub blink_on: bool,
    pub dim_inactive: bool,
    pub now: f64,
    pub copy_on_select: bool,
    pub bright_bold: bool,
}

/// Search matches per absolute line: (column, length, is_current).
pub type Hits = HashMap<u64, Vec<(usize, usize, bool)>>;

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

fn mouse_report(p: &mut Pane, button: u8, col: usize, row: usize, press: bool) {
    if p.term.sgr_mouse {
        let s = format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, if press { 'M' } else { 'm' });
        p.write(s.as_bytes());
    } else {
        let b = if press { button } else { 3 };
        let v = [0x1b, b'[', b'M', 32 + b, (33 + col.min(222)) as u8, (33 + row.min(222)) as u8];
        p.write(&v);
    }
}

type RunKey = (Color32, bool, bool, bool, bool);

/// Draw one pane and handle its mouse. Returns true when it was clicked.
pub fn pane_ui(ui: &mut Ui, pane: &mut Pane, rect: Rect, focused: bool, active: bool, d: &DrawCtx, hits: &Hits) -> bool {
    let cell = d.cell;
    let inner = rect.shrink2(vec2(PAD + 2.0, PAD));
    let cols = ((inner.width() / cell.x).floor() as usize).max(2);
    let rows = ((inner.height() / cell.y).floor() as usize).max(1);
    pane.resize(cols, rows);

    let clicked = mouse(ui, pane, rect, inner, cols, rows, d);

    // ---- paint
    let th = d.theme;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, th.bg);
    let t = &pane.term;
    let sel = pane.sel.map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
    let find_bg = Color32::from_rgb(0xff, 0xe0, 0x66);
    let find_cur = Color32::from_rgb(0xff, 0x96, 0x32);
    let cursor_cell = (pane.scroll == 0 && t.cursor_visible).then_some((t.cx, t.cy));
    let shape = t.cursor_shape.unwrap_or(d.cursor);

    let paint_run = |text: String, start: usize, y: f32, key: RunKey| {
        let (fg, bold, italic, underline, strike) = key;
        if text.trim().is_empty() && !underline && !strike {
            return;
        }
        let fmt = TextFormat {
            font_id: d.font.clone(),
            color: fg,
            italics: italic,
            underline: if underline { Stroke::new(1.0f32, fg) } else { Stroke::NONE },
            strikethrough: if strike { Stroke::new(1.0f32, fg) } else { Stroke::NONE },
            ..Default::default()
        };
        let g = painter.layout_job(LayoutJob::single_section(text, fmt));
        let pos = pos2(inner.left() + start as f32 * cell.x, y);
        if bold {
            painter.galley(pos + vec2(0.6, 0.0), g.clone(), fg);
        }
        painter.galley(pos, g, fg);
    };

    for row in 0..t.rows {
        let abs = pane.view_line(row);
        let Some(idx) = t.abs_to_idx(abs) else { continue };
        let line = t.line(idx).unwrap();
        let y = inner.top() + row as f32 * cell.y;
        let hit = hits.get(&abs);

        // resolve colours
        let mut resolved: Vec<(Color32, Option<Color32>)> = Vec::with_capacity(line.len());
        for (col, c) in line.iter().enumerate().take(t.cols) {
            let mut fg = th.fg_color(c.a.fg, c.a.bold, d.bright_bold);
            let mut bg = th.bg_color(c.a.bg);
            if c.a.dim {
                fg = mix(fg, bg.unwrap_or(th.bg), 0.4);
            }
            if c.a.inverse {
                let b = bg.unwrap_or(th.bg);
                bg = Some(fg);
                fg = b;
            }
            let p = Pos { line: abs, col };
            if let Some((a, b)) = sel {
                if a <= p && p < b {
                    bg = Some(th.selection);
                    fg = th.selection_text;
                }
            }
            if let Some(h) = hit {
                for &(hc, hl, cur) in h {
                    if col >= hc && col < hc + hl {
                        bg = Some(if cur { find_cur } else { find_bg });
                        fg = Color32::BLACK;
                    }
                }
            }
            resolved.push((fg, bg));
        }
        // backgrounds (merged runs)
        let mut col = 0;
        while col < resolved.len() {
            if let Some(bg) = resolved[col].1 {
                let start = col;
                while col < resolved.len() && resolved[col].1 == Some(bg) {
                    col += 1;
                }
                let r = Rect::from_min_size(pos2(inner.left() + start as f32 * cell.x, y), vec2((col - start) as f32 * cell.x, cell.y));
                painter.rect_filled(r, 0.0, bg);
            } else {
                col += 1;
            }
        }
        // text: ASCII of one style together; anything else one cell at a time
        let mut run = String::new();
        let mut run_start = 0usize;
        let mut run_key: Option<RunKey> = None;
        for (col, c) in line.iter().enumerate().take(t.cols) {
            if c.ch == '\0' {
                continue;
            }
            let key = (resolved[col].0, c.a.bold, c.a.italic, c.a.underline, c.a.strike);
            if !c.ch.is_ascii() {
                if let Some(k) = run_key.take() {
                    paint_run(std::mem::take(&mut run), run_start, y, k);
                }
                paint_run(c.ch.to_string(), col, y, key);
                continue;
            }
            if run_key != Some(key) {
                if let Some(k) = run_key {
                    paint_run(std::mem::take(&mut run), run_start, y, k);
                }
                run_key = Some(key);
                run_start = col;
            }
            run.push(c.ch);
        }
        if let Some(k) = run_key {
            paint_run(run, run_start, y, k);
        }

        // cursor
        if let Some((cx, cy)) = cursor_cell {
            if cy == row {
                let r = Rect::from_min_size(pos2(inner.left() + cx as f32 * cell.x, y), cell);
                if !focused {
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0f32, th.cursor), StrokeKind::Inside);
                } else if d.blink_on {
                    match shape {
                        CursorShape::Block => {
                            painter.rect_filled(r, 0.0, th.cursor);
                            let ch = line.get(cx).map(|c| c.ch).unwrap_or(' ');
                            if ch != ' ' && ch != '\0' {
                                let g = painter.layout_no_wrap(ch.to_string(), d.font.clone(), th.cursor_text);
                                painter.galley(r.min, g, th.cursor_text);
                            }
                        }
                        CursorShape::Bar => {
                            painter.rect_filled(Rect::from_min_size(r.min, vec2(2.0, cell.y)), 0.0, th.cursor);
                        }
                        CursorShape::Underline => {
                            painter.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 2.0), r.max), 0.0, th.cursor);
                        }
                    }
                }
            }
        }
    }

    // scroll position indicator
    if pane.scroll > 0 {
        let total = t.total_lines() as f32;
        let top = (t.scrollback.len() - pane.scroll) as f32;
        let h = rect.height() - 8.0;
        let bar = Rect::from_min_size(
            pos2(rect.right() - 7.0, rect.top() + 4.0 + h * top / total),
            vec2(4.0, (h * t.rows as f32 / total).max(16.0)),
        );
        painter.rect_filled(bar, 2.0, th.fg.gamma_multiply(0.45));
    }
    if d.dim_inactive && !active {
        painter.rect_filled(rect, 0.0, th.bg.gamma_multiply(0.35));
    }
    // visual bell
    if d.now - pane.bell_at < 0.15 {
        painter.rect_filled(rect, 0.0, th.fg.gamma_multiply(0.18));
        ui.ctx().request_repaint();
    }
    clicked
}

/// Selection, clicks for programs that want the mouse, and the scroll wheel.
fn mouse(ui: &mut Ui, pane: &mut Pane, rect: Rect, inner: Rect, cols: usize, rows: usize, d: &DrawCtx) -> bool {
    let cell = d.cell;
    let resp = ui.interact(rect, Id::new(("pane", pane.id)), Sense::click_and_drag());
    let clicked = resp.clicked() || resp.drag_started() || resp.secondary_clicked() || resp.middle_clicked();
    let cell_at = |p: egui::Pos2, round: bool| -> (usize, usize) {
        let fx = (p.x - inner.left()) / cell.x;
        let col = if round { fx.round() } else { fx.floor() }.clamp(0.0, cols as f32) as usize;
        let row = ((p.y - inner.top()) / cell.y).floor().clamp(0.0, rows as f32 - 1.0) as usize;
        (col, row)
    };
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Text);
    }
    let shift = ui.input(|i| i.modifiers.shift);
    if pane.term.mouse_mode != 0 && !shift {
        let (pressed, released, pos) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.interact_pos()));
        if let Some(pos) = pos {
            let (c, r) = cell_at(pos, false);
            if pressed && rect.contains(pos) {
                mouse_report(pane, 0, c.min(cols - 1), r, true);
                pane.selecting = true;
            }
            if released && pane.selecting {
                mouse_report(pane, 0, c.min(cols - 1), r, false);
                pane.selecting = false;
            }
        }
    } else {
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.min);
            let (c, r) = cell_at(origin, true);
            let p = Pos { line: pane.view_line(r), col: c };
            pane.sel = Some((p, p));
            pane.selecting = true;
        }
        if pane.selecting && resp.dragged() {
            if let Some(pp) = resp.interact_pointer_pos() {
                if pp.y < inner.top() {
                    pane.scroll = (pane.scroll + 1).min(pane.term.scrollback.len());
                } else if pp.y > inner.bottom() {
                    pane.scroll = pane.scroll.saturating_sub(1);
                }
                let (c, r) = cell_at(pp, true);
                let p = Pos { line: pane.view_line(r), col: c };
                if let Some(s) = pane.sel.as_mut() {
                    s.1 = p;
                }
            }
        }
        if resp.drag_stopped() && pane.selecting {
            pane.selecting = false;
            if pane.sel.is_some_and(|(a, b)| a == b) {
                pane.sel = None;
            } else if d.copy_on_select {
                if let Some(t) = pane.selected_text() {
                    ui.ctx().copy_text(t);
                }
            }
        }
        if let Some(pp) = resp.interact_pointer_pos() {
            let (c, r) = cell_at(pp, false);
            let p = Pos { line: pane.view_line(r), col: c.min(cols - 1) };
            if resp.triple_clicked() {
                pane.select_line(p);
            } else if resp.double_clicked() {
                pane.select_word(p);
            } else if resp.clicked() {
                pane.sel = None;
            }
            if (resp.double_clicked() || resp.triple_clicked()) && d.copy_on_select {
                if let Some(t) = pane.selected_text() {
                    ui.ctx().copy_text(t);
                }
            }
        }
    }
    // ---- wheel
    if resp.hovered() {
        let dy = ui.input(|i| i.raw_scroll_delta.y);
        if dy != 0.0 {
            pane.wheel_acc += dy / cell.y;
            let lines = pane.wheel_acc.trunc() as i32;
            pane.wheel_acc -= lines as f32;
            if lines != 0 {
                if pane.term.mouse_mode != 0 {
                    if let Some(pp) = resp.hover_pos() {
                        let (c, r) = cell_at(pp, false);
                        for _ in 0..lines.abs() {
                            mouse_report(pane, if lines > 0 { 64 } else { 65 }, c.min(cols - 1), r, true);
                        }
                    }
                } else if pane.term.in_alt_screen() {
                    let seq: &[u8] = match (lines > 0, pane.term.app_cursor) {
                        (true, true) => b"\x1bOA",
                        (true, false) => b"\x1b[A",
                        (false, true) => b"\x1bOB",
                        (false, false) => b"\x1b[B",
                    };
                    for _ in 0..lines.abs() {
                        pane.write(seq);
                    }
                } else {
                    let sb = pane.term.scrollback.len();
                    pane.scroll = (pane.scroll as i64 + lines as i64).clamp(0, sb as i64) as usize;
                }
            }
        }
    }
    clicked
}
