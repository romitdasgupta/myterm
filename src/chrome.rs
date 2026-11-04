//! Tab bar, find bar and the settings window.

use crate::app::{Action, App};
use crate::term::CursorShape;
use crate::theme::THEMES;
use egui::{pos2, vec2, Align2, Color32, FontId, Id, Key, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

pub const TAB_H: f32 = 30.0;
const ACCENT: Color32 = Color32::from_rgb(0x4a, 0x90, 0xe2);

fn elide(ui: &Ui, s: &str, font: &FontId, max: f32) -> String {
    let w = |t: &str| ui.fonts(|f| f.layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE).size().x);
    if w(s) <= max {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        n -= 1;
        let t: String = chars[..n].iter().collect::<String>() + "…";
        if w(&t) <= max {
            return t;
        }
    }
    "…".into()
}

impl App {
    pub fn tab_bar(&mut self, ctx: &egui::Context) {
        let light = self.theme().is_light();
        let pick = |l: u32, d: u32| {
            let h = if light { l } else { d };
            Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
        };
        let bar_bg = pick(0xdcdcdc, 0x1c1c1e);
        let tab_active = pick(0xf6f6f6, 0x3a3a3c);
        let tab_hover = pick(0xe8e8e8, 0x2a2a2c);
        let text_on = pick(0x1a1a1a, 0xf0f0f0);
        let text_off = pick(0x707070, 0x98989c);
        let sep = pick(0xc4c4c4, 0x0e0e10);
        let font = FontId::proportional(12.5);
        let small = FontId::proportional(11.0);

        egui::TopBottomPanel::top("tabbar")
            .exact_height(TAB_H)
            .frame(egui::Frame::new().fill(bar_bg))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.horizontal(|ui| {
                    let n = self.tabs.len().max(1);
                    let w = ((ui.available_width() - 66.0) / n as f32).max(40.0);
                    for i in 0..self.tabs.len() {
                        let (r, resp) = ui.allocate_exact_size(vec2(w, TAB_H), Sense::click());
                        let active = i == self.cur;
                        let hovered = resp.hovered();
                        let p = ui.painter();
                        p.rect_filled(r, 0.0, if active { tab_active } else if hovered { tab_hover } else { bar_bg });
                        if i > 0 {
                            p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0f32, sep));
                        }
                        if active && self.tabs.len() > 1 {
                            p.line_segment([r.left_bottom() - vec2(0.0, 1.0), r.right_bottom() - vec2(0.0, 1.0)], Stroke::new(2.0f32, ACCENT));
                        }
                        let tab = &self.tabs[i];
                        let mut label = self.panes.get(&tab.active).map(|p| p.title()).unwrap_or_default();
                        let count = tab.leaves().len();
                        if count > 1 {
                            label = format!("{label}  ·  {count} panes");
                        }
                        // close button on the left, shown on hover
                        let close = Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(16.0, 16.0));
                        let over_close = ui.input(|i| i.pointer.hover_pos()).is_some_and(|q| close.contains(q));
                        if hovered {
                            if over_close {
                                p.rect_filled(close, 3.0, text_off.gamma_multiply(0.35));
                            }
                            p.text(close.center(), Align2::CENTER_CENTER, "×", FontId::proportional(14.0), text_off);
                        } else if tab.bell {
                            p.circle_filled(close.center(), 3.5, Color32::from_rgb(0xe8, 0xa0, 0x30));
                        } else if tab.activity {
                            p.circle_filled(close.center(), 3.0, ACCENT);
                        }
                        // ⌘-number hint on the right
                        let mut right = r.right() - 8.0;
                        if i < 9 && w > 110.0 {
                            let g = p.layout_no_wrap(format!("⌘{}", i + 1), small.clone(), text_off);
                            right -= g.size().x;
                            p.galley(pos2(right, r.center().y - g.size().y / 2.0), g, text_off);
                            right -= 6.0;
                        }
                        let label = elide(ui, &label, &font, (right - (r.left() + 26.0)).max(10.0));
                        let g = p.layout_no_wrap(label, font.clone(), if active { text_on } else { text_off });
                        let lo = r.left() + 26.0 + g.size().x / 2.0;
                        let hi = (right - g.size().x / 2.0).max(lo);
                        let cx = r.center().x.clamp(lo, hi);
                        p.galley(pos2(cx - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, text_on);

                        if resp.clicked() {
                            self.actions.push(if over_close { Action::CloseTab(i) } else { Action::SelectTab(i) });
                        }
                        if resp.middle_clicked() {
                            self.actions.push(Action::CloseTab(i));
                        }
                    }
                    // new tab
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, TAB_H), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r.shrink(4.0), 4.0, tab_hover);
                    }
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), text_off);
                    if resp.on_hover_text("New tab (⌘T)").clicked() {
                        self.actions.push(Action::NewTab);
                    }
                    // menu
                    let (r, resp) = ui.allocate_exact_size(vec2(32.0, TAB_H), Sense::click());
                    let popup = Id::new("main-menu");
                    if resp.hovered() || ui.memory(|m| m.is_popup_open(popup)) {
                        ui.painter().rect_filled(r.shrink(4.0), 4.0, tab_hover);
                    }
                    for k in -1..=1 {
                        let y = r.center().y + k as f32 * 4.5;
                        ui.painter().line_segment([pos2(r.center().x - 6.5, y), pos2(r.center().x + 6.5, y)], Stroke::new(1.5f32, text_off));
                    }
                    let resp = resp.on_hover_text("Menu");
                    if resp.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(popup));
                    }
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    egui::popup::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                        ui.set_min_width(230.0);
                        let mut item = |ui: &mut Ui, label: &str, keys: &str, a: Action| {
                            if ui.add(egui::Button::new(label).shortcut_text(keys)).clicked() {
                                self.actions.push(a);
                            }
                        };
                        item(ui, "New Tab", "⌘T", Action::NewTab);
                        item(ui, "Split Side by Side", "⌘D", Action::Split(true));
                        item(ui, "Split Top and Bottom", "⇧⌘D", Action::Split(false));
                        item(ui, "Close Pane", "⌘W", Action::ClosePane);
                        ui.separator();
                        item(ui, "Next Pane", "⌘]", Action::CyclePane(1));
                        item(ui, "Previous Tab", "⌘←", Action::CycleTab(-1));
                        item(ui, "Next Tab", "⌘→", Action::CycleTab(1));
                        ui.separator();
                        item(ui, "Find…", "⌘F", Action::Find);
                        item(ui, "Clear Buffer", "⌘K", Action::Clear);
                        ui.separator();
                        item(ui, "Bigger Text", "⌘+", Action::FontDelta(1.0));
                        item(ui, "Smaller Text", "⌘−", Action::FontDelta(-1.0));
                        item(ui, "Full Screen", "⌘↩", Action::Fullscreen);
                        ui.separator();
                        item(ui, "Settings…", "⌘,", Action::Settings);
                    });
                });
            });
    }

    pub fn search_bar(&mut self, ctx: &egui::Context, area: Rect) {
        let light = self.theme().is_light();
        let Some(s) = self.search.as_mut() else {
            self.search_focused = false;
            return;
        };
        // recompute matches when the text or the pane's contents change
        if let Some(p) = self.panes.get(&s.pane) {
            let gen = (p.term.dropped, p.gen);
            if s.query != s.last_query || gen != s.last_gen {
                let fresh = s.query != s.last_query;
                s.matches = p.find(&s.query);
                s.last_query = s.query.clone();
                s.last_gen = gen;
                if fresh || s.current.is_none_or(|c| c >= s.matches.len()) {
                    s.current = s.matches.len().checked_sub(1);
                    if let Some(c) = s.current {
                        let line = s.matches[c].0;
                        if let Some(p) = self.panes.get_mut(&s.pane) {
                            p.reveal(line);
                        }
                    }
                }
            }
        }
        let mut step = None;
        let mut close = false;
        let mut focused = false;
        egui::Area::new(Id::new("findbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(area.right() - 350.0, area.top() + 6.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(if light { Color32::from_rgb(0xee, 0xee, 0xee) } else { Color32::from_rgb(0x2c, 0x2c, 0x2e) })
                    .stroke(Stroke::new(1.0f32, Color32::from_gray(if light { 0xbb } else { 0x50 })))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(80) })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let r = ui.add(egui::TextEdit::singleline(&mut s.query).hint_text("Find").desired_width(170.0));
                            if std::mem::take(&mut s.want_focus) {
                                r.request_focus();
                            }
                            if r.lost_focus() {
                                if ui.input(|i| i.key_pressed(Key::Enter)) {
                                    step = Some(!ui.input(|i| i.modifiers.shift));
                                    r.request_focus();
                                } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                                    close = true;
                                }
                            }
                            focused = r.has_focus();
                            let label = if s.query.is_empty() {
                                String::new()
                            } else if s.matches.is_empty() {
                                "No results".to_string()
                            } else {
                                format!("{}/{}", s.current.map(|c| c + 1).unwrap_or(0), s.matches.len())
                            };
                            ui.add_sized([66.0, 18.0], egui::Label::new(egui::RichText::new(label).size(11.0).weak()));
                            if ui.small_button("↑").on_hover_text("Previous (⌘G)").clicked() {
                                step = Some(true);
                            }
                            if ui.small_button("↓").on_hover_text("Next (⇧⌘G)").clicked() {
                                step = Some(false);
                            }
                            if ui.small_button("×").on_hover_text("Close (Esc)").clicked() {
                                close = true;
                            }
                        });
                    });
            });
        self.search_focused = focused && !close;
        if let Some(up) = step {
            self.actions.push(Action::FindStep(up));
        }
        if close {
            self.actions.push(Action::CloseFind);
        }
    }

    pub fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(ctx.screen_rect().center() - vec2(170.0, 190.0))
            .show(ctx, |ui| {
                ui.heading("Colours");
                for (i, t) in THEMES.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(112.0, 18.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(r, 3.0, t.bg);
                        for k in 0..8 {
                            let cr = Rect::from_min_size(pos2(r.left() + 6.0 + k as f32 * 13.0, r.top() + 4.0), vec2(10.0, 10.0));
                            p.rect_filled(cr, 2.0, t.ansi[k + 8]);
                        }
                        p.rect_stroke(r, 3.0, Stroke::new(1.0f32, Color32::from_gray(110)), StrokeKind::Inside);
                        ui.add_space(6.0);
                        ui.radio_value(&mut self.settings.theme, i, t.name);
                    });
                }
                ui.add_space(8.0);
                ui.heading("Text");
                ui.horizontal(|ui| {
                    ui.label("Size");
                    ui.add(egui::Slider::new(&mut self.settings.font_size, 8.0..=32.0).step_by(1.0).show_value(false));
                    ui.label(format!("{} pt", self.settings.font_size as i32));
                });
                ui.checkbox(&mut self.settings.bright_bold, "Show bold text in bright colours");
                ui.add_space(8.0);
                ui.heading("Cursor");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.settings.cursor, CursorShape::Block, "Box");
                    ui.radio_value(&mut self.settings.cursor, CursorShape::Bar, "Line");
                    ui.radio_value(&mut self.settings.cursor, CursorShape::Underline, "Underscore");
                });
                ui.checkbox(&mut self.settings.blink, "Blinking cursor");
                ui.add_space(8.0);
                ui.heading("Behaviour");
                ui.checkbox(&mut self.settings.copy_on_select, "Copy text as soon as it's selected");
                ui.checkbox(&mut self.settings.option_meta, "Option key acts as Meta (Esc+)");
                ui.checkbox(&mut self.settings.dim_inactive, "Dim panes that aren't in use");
                ui.horizontal(|ui| {
                    ui.label("History");
                    let mut v = self.settings.scrollback as f32;
                    ui.add(egui::Slider::new(&mut v, 1000.0..=100_000.0).logarithmic(true).show_value(false));
                    self.settings.scrollback = (((v / 1000.0).round() * 1000.0) as usize).max(1000);
                    ui.label(format!("{} lines", self.settings.scrollback));
                });
            });
        if !open {
            self.show_settings = false;
        }
    }
}
