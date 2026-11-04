//! The window: owns the sessions, tabs and settings, and routes input.

use crate::draw::{pane_ui, DrawCtx, Hits};
use crate::keys::key_bytes;
use crate::layout::{Node, Tab};
use crate::pane::Pane;
use crate::settings::Settings;
use crate::theme::{Theme, THEMES};
use egui::{vec2, Color32, CursorIcon, Event, FontFamily, FontId, Id, Key, Modifiers, Rect, Sense, Ui, Vec2, ViewportCommand};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Search {
    pub query: String,
    pub last_query: String,
    pub pane: u64,
    pub last_gen: (u64, usize),
    pub matches: Vec<(u64, usize, usize)>,
    pub current: Option<usize>,
    pub want_focus: bool,
}

pub enum Action {
    NewTab,
    Split(bool),
    ClosePane,
    CloseTab(usize),
    SelectTab(usize),
    CycleTab(i32),
    CyclePane(i32),
    FocusPane(u64),
    Find,
    FindStep(bool),
    CloseFind,
    Clear,
    FontDelta(f32),
    FontReset,
    Settings,
    Fullscreen,
    Quit,
}

pub struct App {
    pub panes: HashMap<u64, Pane>,
    pub tabs: Vec<Tab>,
    pub cur: usize,
    next_id: u64,
    pub settings: Settings,
    saved_settings: Settings,
    pub show_settings: bool,
    pub search: Option<Search>,
    input: Vec<Event>,
    pub search_focused: bool,
    screenshot: Option<PathBuf>,
    frame: u64,
    last_title: String,
    last_input_time: f64,
    pub actions: Vec<Action>,
    cell: Vec2,
    demo: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, demo: bool) -> Self {
        setup_fonts(&cc.egui_ctx);
        let settings = Settings::load();
        let mut app = App {
            panes: HashMap::new(),
            tabs: Vec::new(),
            cur: 0,
            next_id: 1,
            saved_settings: settings.clone(),
            settings,
            show_settings: false,
            search: None,
            input: Vec::new(),
            search_focused: false,
            screenshot,
            frame: 0,
            last_title: String::new(),
            last_input_time: 0.0,
            actions: Vec::new(),
            cell: vec2(8.0, 16.0),
            demo,
        };
        app.new_tab(&cc.egui_ctx);
        app
    }

    /// `--demo`: split the window and show some colour so the look can be checked.
    fn run_demo(&mut self, ctx: &egui::Context) {
        let left = self.active_pane_id().unwrap_or(0);
        self.apply(ctx, Action::Split(true));
        let right = self.active_pane_id().unwrap_or(0);
        self.apply(ctx, Action::NewTab);
        self.apply(ctx, Action::SelectTab(0));
        self.apply(ctx, Action::FocusPane(left));
        let colours = "clear; for i in 0 1 2 3 4 5 6 7; do printf '\\e[4%sm  \\e[0m' $i; done; echo; \
                       for i in 0 1 2 3 4 5 6 7; do printf '\\e[10%sm  \\e[0m' $i; done; echo; \
                       printf '\\e[1mbold\\e[0m \\e[3mitalic\\e[0m \\e[4munderline\\e[0m \\e[31mred\\e[0m \\e[32mgreen\\e[0m \\e[34mblue\\e[0m\\n'\r";
        if let Some(p) = self.panes.get_mut(&left) {
            p.write(colours.as_bytes());
        }
        if let Some(p) = self.panes.get_mut(&right) {
            p.write(b"clear; ls -G /\r");
        }
    }

    pub fn theme(&self) -> &'static Theme {
        &THEMES[self.settings.theme.min(THEMES.len() - 1)]
    }

    fn font(&self) -> FontId {
        FontId::new(self.settings.font_size, FontFamily::Monospace)
    }

    fn active_pane_id(&self) -> Option<u64> {
        self.tabs.get(self.cur).map(|t| t.active)
    }

    fn active_pane(&mut self) -> Option<&mut Pane> {
        let id = self.active_pane_id()?;
        self.panes.get_mut(&id)
    }

    fn spawn_pane(&mut self, ctx: &egui::Context) -> u64 {
        let (cols, rows) = self
            .active_pane_id()
            .and_then(|id| self.panes.get(&id))
            .map(|p| (p.term.cols, p.term.rows))
            .unwrap_or((80, 25));
        let id = self.next_id;
        self.next_id += 1;
        let mut p = Pane::spawn(id, cols, rows, ctx.clone());
        p.term.scrollback_limit = self.settings.scrollback;
        self.panes.insert(id, p);
        id
    }

    fn new_tab(&mut self, ctx: &egui::Context) {
        let id = self.spawn_pane(ctx);
        self.tabs.push(Tab { root: Node::Leaf(id), active: id, activity: false, bell: false });
        self.cur = self.tabs.len() - 1;
    }

    fn remove_pane(&mut self, id: u64) {
        self.panes.remove(&id);
        let mut i = 0;
        while i < self.tabs.len() {
            let tab = &mut self.tabs[i];
            let root = std::mem::replace(&mut tab.root, Node::Leaf(0));
            match root.remove(id) {
                Some(r) => {
                    tab.root = r;
                    if tab.active == id {
                        tab.active = tab.leaves()[0];
                    }
                    i += 1;
                }
                None => {
                    self.tabs.remove(i);
                    if self.cur > i || self.cur >= self.tabs.len() {
                        self.cur = self.cur.saturating_sub(1);
                    }
                }
            }
        }
        if self.search.as_ref().is_some_and(|s| s.pane == id) {
            self.search = None;
            self.search_focused = false;
        }
    }

    fn apply(&mut self, ctx: &egui::Context, a: Action) {
        match a {
            Action::NewTab => self.new_tab(ctx),
            Action::Split(side) => {
                if let Some(active) = self.active_pane_id() {
                    let new = self.spawn_pane(ctx);
                    let tab = &mut self.tabs[self.cur];
                    tab.root.split(active, new, side);
                    tab.active = new;
                }
            }
            Action::ClosePane => {
                if let Some(id) = self.active_pane_id() {
                    self.remove_pane(id);
                }
            }
            Action::CloseTab(i) => {
                if let Some(t) = self.tabs.get(i) {
                    for id in t.leaves() {
                        self.remove_pane(id);
                    }
                }
            }
            Action::SelectTab(i) => {
                if i < self.tabs.len() {
                    self.cur = i;
                }
            }
            Action::CycleTab(d) => {
                let n = self.tabs.len() as i32;
                if n > 0 {
                    self.cur = (self.cur as i32 + d).rem_euclid(n) as usize;
                }
            }
            Action::CyclePane(d) => {
                if let Some(tab) = self.tabs.get_mut(self.cur) {
                    let l = tab.leaves();
                    let i = l.iter().position(|x| *x == tab.active).unwrap_or(0) as i32;
                    tab.active = l[(i + d).rem_euclid(l.len() as i32) as usize];
                }
            }
            Action::FocusPane(id) => {
                if let Some(tab) = self.tabs.get_mut(self.cur) {
                    tab.active = id;
                }
            }
            Action::Find => {
                let pane = self.active_pane_id().unwrap_or(0);
                match &mut self.search {
                    Some(s) => {
                        if s.pane != pane {
                            s.pane = pane;
                            s.last_gen = (u64::MAX, 0);
                        }
                        s.want_focus = true;
                    }
                    None => {
                        self.search = Some(Search {
                            query: String::new(),
                            last_query: String::new(),
                            pane,
                            last_gen: (u64::MAX, 0),
                            matches: Vec::new(),
                            current: None,
                            want_focus: true,
                        })
                    }
                }
            }
            Action::FindStep(up) => {
                if let Some(s) = &mut self.search {
                    if !s.matches.is_empty() {
                        let n = s.matches.len();
                        let c = s.current.unwrap_or(n);
                        let c = if up { (c + n - 1) % n } else { (c + 1) % n };
                        s.current = Some(c);
                        let line = s.matches[c].0;
                        if let Some(p) = self.panes.get_mut(&s.pane) {
                            p.reveal(line);
                        }
                    }
                }
            }
            Action::CloseFind => {
                self.search = None;
                self.search_focused = false;
            }
            Action::Clear => {
                if let Some(p) = self.active_pane() {
                    p.term.clear_buffer();
                    p.scroll = 0;
                    p.sel = None;
                }
            }
            Action::FontDelta(d) => self.settings.font_size = (self.settings.font_size + d).clamp(8.0, 32.0),
            Action::FontReset => self.settings.font_size = 13.0,
            Action::Settings => self.show_settings = !self.show_settings,
            Action::Fullscreen => {
                let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fs));
            }
            Action::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
        }
    }

    /// App-level shortcut (⌘ + key). Returns true if the key was used.
    fn shortcut(&mut self, key: Key, m: Modifiers) -> bool {
        let cmd = if cfg!(target_os = "macos") { m.mac_cmd } else { m.ctrl && m.shift };
        if !cmd {
            return false;
        }
        let a = match key {
            Key::T | Key::N => Some(Action::NewTab),
            Key::W => Some(Action::ClosePane),
            Key::D => Some(Action::Split(!m.shift)),
            Key::F => Some(Action::Find),
            Key::G => Some(Action::FindStep(!m.shift)),
            Key::K => Some(Action::Clear),
            Key::Comma => Some(Action::Settings),
            Key::Plus | Key::Equals => Some(Action::FontDelta(1.0)),
            Key::Minus => Some(Action::FontDelta(-1.0)),
            Key::Num0 => Some(Action::FontReset),
            Key::OpenBracket => Some(Action::CyclePane(-1)),
            Key::CloseBracket => Some(Action::CyclePane(1)),
            Key::ArrowLeft | Key::ArrowUp if m.alt => Some(Action::CyclePane(-1)),
            Key::ArrowRight | Key::ArrowDown if m.alt => Some(Action::CyclePane(1)),
            Key::ArrowLeft => Some(Action::CycleTab(-1)),
            Key::ArrowRight => Some(Action::CycleTab(1)),
            Key::Enter => Some(Action::Fullscreen),
            Key::Q => Some(Action::Quit),
            Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 | Key::Num8 => {
                let n = key.name().parse::<usize>().unwrap_or(1);
                Some(Action::SelectTab(n - 1))
            }
            Key::Num9 => Some(Action::SelectTab(self.tabs.len().saturating_sub(1))),
            _ => None,
        };
        if let Some(a) = a {
            self.actions.push(a);
        }
        true // swallow every ⌘ combination so it never reaches the shell
    }

    fn handle_input(&mut self, ctx: &egui::Context, now: f64) {
        let events = std::mem::take(&mut self.input);
        let mut skip_text = false;
        for ev in events {
            match ev {
                Event::Copy | Event::Cut => {
                    if let Some(t) = self.active_pane().and_then(|p| p.selected_text()) {
                        ctx.copy_text(t);
                    }
                }
                Event::Paste(s) => {
                    if let Some(p) = self.active_pane() {
                        p.paste(&s);
                    }
                }
                Event::Text(t) => {
                    if std::mem::take(&mut skip_text) || t.chars().any(|c| c.is_control()) {
                        continue;
                    }
                    self.last_input_time = now;
                    if let Some(p) = self.active_pane() {
                        p.scroll = 0;
                        p.write(t.as_bytes());
                    }
                }
                Event::Ime(egui::ImeEvent::Commit(t)) => {
                    if let Some(p) = self.active_pane() {
                        p.write(t.as_bytes());
                    }
                }
                Event::Key { key, pressed: true, modifiers, .. } => {
                    if self.shortcut(key, modifiers) {
                        continue;
                    }
                    let meta = self.settings.option_meta;
                    let Some(p) = self.active_pane() else { continue };
                    // Shift+PageUp/PageDown/Home/End scroll the history.
                    if modifiers.shift && !p.term.in_alt_screen() {
                        let page = p.term.rows.saturating_sub(1).max(1);
                        let sb = p.term.scrollback.len();
                        let new_scroll = match key {
                            Key::PageUp => Some((p.scroll + page).min(sb)),
                            Key::PageDown => Some(p.scroll.saturating_sub(page)),
                            Key::Home => Some(sb),
                            Key::End => Some(0),
                            _ => None,
                        };
                        if let Some(s) = new_scroll {
                            p.scroll = s;
                            continue;
                        }
                    }
                    if let Some((bytes, skip)) = key_bytes(key, modifiers, p.term.app_cursor, meta) {
                        p.scroll = 0;
                        p.write(&bytes);
                        skip_text |= skip;
                        self.last_input_time = now;
                    }
                }
                _ => {}
            }
        }
    }

    fn reap_and_pump(&mut self, ctx: &egui::Context, now: f64) {
        let mut exited = Vec::new();
        let visible: Vec<u64> = self.tabs.get(self.cur).map(|t| t.leaves()).unwrap_or_default();
        for (id, p) in self.panes.iter_mut() {
            p.term.scrollback_limit = self.settings.scrollback;
            let had_bell = p.bell_at;
            if p.pump(now) && !visible.contains(id) {
                for t in self.tabs.iter_mut() {
                    if t.leaves().contains(id) {
                        t.activity = true;
                        if p.bell_at != had_bell {
                            t.bell = true;
                        }
                    }
                }
            }
            if p.exited {
                exited.push(*id);
            }
        }
        for id in exited {
            self.remove_pane(id);
        }
        if self.tabs.is_empty() {
            if self.screenshot.is_some() {
                self.new_tab(ctx);
            } else {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
        if let Some(t) = self.tabs.get_mut(self.cur) {
            t.activity = false;
            t.bell = false;
        }
    }

    fn panes_ui(&mut self, ui: &mut Ui, now: f64) {
        let rect = ui.max_rect();
        let theme = self.theme();
        ui.painter().rect_filled(rect, 0.0, theme.bg);
        let Some(tab) = self.tabs.get(self.cur) else { return };
        let mut leaves = Vec::new();
        let mut divs = Vec::new();
        tab.root.collect(rect, &mut Vec::new(), &mut leaves, &mut divs);
        let active = tab.active;
        let single = leaves.len() == 1;
        let window_focused = ui.input(|i| i.focused) || self.screenshot.is_some();
        let blink_on = !self.settings.blink || (((now - self.last_input_time) / 0.53) as i64) % 2 == 0;
        let d = DrawCtx {
            theme,
            font: self.font(),
            cell: self.cell,
            cursor: self.settings.cursor,
            blink_on,
            dim_inactive: self.settings.dim_inactive && !single,
            now,
            copy_on_select: self.settings.copy_on_select,
            bright_bold: self.settings.bright_bold,
        };
        for (id, r) in leaves {
            let hits = self.search_hits(id);
            let Some(p) = self.panes.get_mut(&id) else { continue };
            if pane_ui(ui, p, r, id == active && window_focused, id == active, &d, &hits) {
                self.actions.push(Action::FocusPane(id));
            }
        }
        // dividers on top so they win the pointer
        let div_col = if theme.is_light() { Color32::from_gray(0xb8) } else { Color32::from_gray(0x48) };
        for (k, dv) in divs.iter().enumerate() {
            ui.painter().rect_filled(dv.line, 0.0, div_col);
            let resp = ui.interact(dv.line.expand(3.0), Id::new(("divider", k, dv.path.clone())), Sense::drag());
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(if dv.side_by_side { CursorIcon::ResizeHorizontal } else { CursorIcon::ResizeVertical });
            }
            if resp.dragged() {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let r = if dv.side_by_side {
                        (pp.x - dv.parent.left()) / dv.parent.width()
                    } else {
                        (pp.y - dv.parent.top()) / dv.parent.height()
                    };
                    self.tabs[self.cur].root.set_ratio(&dv.path, r.clamp(0.1, 0.9));
                }
            }
        }
    }

    fn search_hits(&self, pane: u64) -> Hits {
        let mut m: Hits = HashMap::new();
        if let Some(s) = &self.search {
            if s.pane == pane && !s.query.is_empty() {
                for (i, (line, col, len)) in s.matches.iter().enumerate() {
                    m.entry(*line).or_default().push((*col, *len, Some(i) == s.current));
                }
            }
        }
        m
    }

    fn take_screenshot(&mut self, ctx: &egui::Context, path: PathBuf) {
        ctx.request_repaint();
        if self.frame == 120 {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let mut bytes = Vec::with_capacity(w * h * 4);
            for p in &img.pixels {
                bytes.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                Some(buf) => {
                    if let Err(e) = buf.save(&path) {
                        eprintln!("could not save screenshot: {e}");
                    }
                }
                None => eprintln!("screenshot had the wrong size"),
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        if self.frame > 2000 {
            eprintln!("screenshot timed out");
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mono = [
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/SFNSMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ];
    for path in mono {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert("mono".into(), Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "mono".into());
            break;
        }
    }
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/Apple Symbols.ttf") {
        fonts.font_data.insert("symbols".into(), Arc::new(egui::FontData::from_owned(bytes)));
        for fam in [FontFamily::Monospace, FontFamily::Proportional] {
            fonts.families.entry(fam).or_default().push("symbols".into());
        }
    }
    ctx.set_fonts(fonts);
}

impl eframe::App for App {
    /// Take keyboard events before egui sees them, so keys like Tab and
    /// Enter go to the shell instead of moving focus between buttons.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let mut keep = Vec::with_capacity(raw.events.len());
        for ev in raw.events.drain(..) {
            let ours = if self.search_focused {
                // the find box has the keyboard, but ⌘ shortcuts keep working
                matches!(&ev, Event::Key { key, pressed: true, modifiers, .. }
                    if modifiers.mac_cmd && !matches!(key, Key::A | Key::C | Key::V | Key::X | Key::Z))
            } else {
                matches!(ev, Event::Key { .. } | Event::Text(_) | Event::Paste(_) | Event::Copy | Event::Cut | Event::Ime(_))
            };
            if ours {
                self.input.push(ev);
            } else {
                keep.push(ev);
            }
        }
        raw.events = keep;
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame += 1;
        let now = ctx.input(|i| i.time);
        ctx.set_visuals(if self.theme().is_light() { egui::Visuals::light() } else { egui::Visuals::dark() });

        let font = self.font();
        self.cell = ctx.fonts(|f| vec2(f.glyph_width(&font, 'M'), f.row_height(&font).ceil()));

        if self.demo && self.frame == 3 {
            self.run_demo(ctx);
        }
        self.handle_input(ctx, now);
        self.reap_and_pump(ctx, now);

        self.tab_bar(ctx);
        let mut central = Rect::NOTHING;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(self.theme().bg)).show(ctx, |ui| {
            central = ui.max_rect();
            self.panes_ui(ui, now);
        });
        self.search_bar(ctx, central);
        self.settings_window(ctx);

        for a in std::mem::take(&mut self.actions) {
            self.apply(ctx, a);
        }
        if self.settings != self.saved_settings && self.screenshot.is_none() {
            self.settings.save();
            self.saved_settings = self.settings.clone();
        }

        // window title follows the active session
        let title = self
            .active_pane_id()
            .and_then(|id| self.panes.get(&id))
            .map(|p| p.title())
            .unwrap_or_else(|| "MyTerm".into());
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        if self.settings.blink {
            ctx.request_repaint_after(std::time::Duration::from_millis(530));
        }

        if let Some(path) = self.screenshot.clone() {
            self.take_screenshot(ctx, path);
        }
    }
}
