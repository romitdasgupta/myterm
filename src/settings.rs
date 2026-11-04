//! The person's preferences, saved as a small text file.

use crate::term::CursorShape;
use crate::theme::THEMES;
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
pub struct Settings {
    pub theme: usize,
    pub font_size: f32,
    pub cursor: CursorShape,
    pub blink: bool,
    pub option_meta: bool,
    pub dim_inactive: bool,
    pub copy_on_select: bool,
    pub bright_bold: bool,
    pub scrollback: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: 0,
            font_size: 13.0,
            cursor: CursorShape::Block,
            blink: false,
            option_meta: false,
            dim_inactive: true,
            copy_on_select: true,
            bright_bold: true,
            scrollback: 10_000,
        }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join("Library/Application Support/MyTerm/settings.txt"))
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Some(text) = Settings::path().and_then(|p| std::fs::read_to_string(p).ok()) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "theme" => s.theme = v.parse().unwrap_or(0usize).min(THEMES.len() - 1),
                "font_size" => s.font_size = v.parse().unwrap_or(13.0f32).clamp(8.0, 32.0),
                "cursor" => {
                    s.cursor = match v {
                        "bar" => CursorShape::Bar,
                        "underline" => CursorShape::Underline,
                        _ => CursorShape::Block,
                    }
                }
                "blink" => s.blink = v == "true",
                "option_meta" => s.option_meta = v == "true",
                "dim_inactive" => s.dim_inactive = v == "true",
                "copy_on_select" => s.copy_on_select = v == "true",
                "bright_bold" => s.bright_bold = v == "true",
                "scrollback" => s.scrollback = v.parse().unwrap_or(10_000usize).clamp(1000, 100_000),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let Some(p) = Settings::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let cursor = match self.cursor {
            CursorShape::Block => "block",
            CursorShape::Bar => "bar",
            CursorShape::Underline => "underline",
        };
        let text = format!(
            "theme={}\nfont_size={}\ncursor={}\nblink={}\noption_meta={}\ndim_inactive={}\ncopy_on_select={}\nbright_bold={}\nscrollback={}\n",
            self.theme,
            self.font_size,
            cursor,
            self.blink,
            self.option_meta,
            self.dim_inactive,
            self.copy_on_select,
            self.bright_bold,
            self.scrollback
        );
        let _ = std::fs::write(p, text);
    }
}
