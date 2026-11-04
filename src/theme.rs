//! Colour schemes.

use crate::term::Color;
use egui::Color32;

pub struct Theme {
    pub name: &'static str,
    pub bg: Color32,
    pub fg: Color32,
    pub bold: Color32,
    pub cursor: Color32,
    pub cursor_text: Color32,
    pub selection: Color32,
    pub selection_text: Color32,
    pub ansi: [Color32; 16],
}

const fn c(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Midnight",
        bg: c(0x000000),
        fg: c(0xc7c7c7),
        bold: c(0xfeffff),
        cursor: c(0xc7c7c7),
        cursor_text: c(0x000000),
        selection: c(0xc6dcfc),
        selection_text: c(0x000000),
        ansi: [
            c(0x000000), c(0xc91b00), c(0x00c200), c(0xc7c400),
            c(0x0225c7), c(0xca30c7), c(0x00c5c7), c(0xc7c7c7),
            c(0x686868), c(0xff6e67), c(0x5ffa68), c(0xfffc67),
            c(0x6871ff), c(0xff77ff), c(0x60fdff), c(0xffffff),
        ],
    },
    Theme {
        name: "Deep Sea",
        bg: c(0x0f1720),
        fg: c(0xc5d1de),
        bold: c(0xeef4fb),
        cursor: c(0x6fc3df),
        cursor_text: c(0x0f1720),
        selection: c(0x2c4a63),
        selection_text: c(0xeef4fb),
        ansi: [
            c(0x1c2733), c(0xe0646b), c(0x8fcf7a), c(0xe8c66d),
            c(0x5fa8e8), c(0xb48ce6), c(0x5fcfcf), c(0xc5d1de),
            c(0x4a5a6b), c(0xf0868c), c(0xaee39b), c(0xf3d88f),
            c(0x86c1f2), c(0xcbaaf0), c(0x86e0e0), c(0xffffff),
        ],
    },
    Theme {
        name: "Solar Dark",
        bg: c(0x002b36),
        fg: c(0x839496),
        bold: c(0x93a1a1),
        cursor: c(0x93a1a1),
        cursor_text: c(0x002b36),
        selection: c(0x073642),
        selection_text: c(0x93a1a1),
        ansi: [
            c(0x073642), c(0xdc322f), c(0x859900), c(0xb58900),
            c(0x268bd2), c(0xd33682), c(0x2aa198), c(0xeee8d5),
            c(0x002b36), c(0xcb4b16), c(0x586e75), c(0x657b83),
            c(0x839496), c(0x6c71c4), c(0x93a1a1), c(0xfdf6e3),
        ],
    },
    Theme {
        name: "Paper Light",
        bg: c(0xfafafa),
        fg: c(0x2b2b2b),
        bold: c(0x000000),
        cursor: c(0x2b2b2b),
        cursor_text: c(0xfafafa),
        selection: c(0xb5d5ff),
        selection_text: c(0x000000),
        ansi: [
            c(0x000000), c(0xc0392b), c(0x2e8b3a), c(0x9a7b00),
            c(0x1f5fbf), c(0x9b3cad), c(0x16808a), c(0xbfbfbf),
            c(0x666666), c(0xe74c3c), c(0x3cb24b), c(0xb89400),
            c(0x3b7fe0), c(0xbb55cc), c(0x1fa3ae), c(0xffffff),
        ],
    },
    Theme {
        name: "Green Screen",
        bg: c(0x020a03),
        fg: c(0x3cf06b),
        bold: c(0x9bffb5),
        cursor: c(0x3cf06b),
        cursor_text: c(0x020a03),
        selection: c(0x1b5e2b),
        selection_text: c(0xd8ffe2),
        ansi: [
            c(0x062b10), c(0x2fc45a), c(0x3cf06b), c(0x6af58f),
            c(0x27a84b), c(0x4fd977), c(0x5ef088), c(0x3cf06b),
            c(0x14632c), c(0x5ef088), c(0x8affaa), c(0xa6ffc0),
            c(0x45d06e), c(0x75f297), c(0x93ffb3), c(0xd8ffe2),
        ],
    },
];

impl Theme {
    pub fn indexed(&self, i: u8) -> Color32 {
        match i {
            0..=15 => self.ansi[i as usize],
            16..=231 => {
                let i = i - 16;
                let lv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Color32::from_rgb(lv(i / 36), lv((i / 6) % 6), lv(i % 6))
            }
            _ => {
                let g = 8 + (i - 232) * 10;
                Color32::from_rgb(g, g, g)
            }
        }
    }

    pub fn fg_color(&self, col: Color, bold: bool, bright_bold: bool) -> Color32 {
        match col {
            Color::Default => {
                if bold {
                    self.bold
                } else {
                    self.fg
                }
            }
            Color::Idx(i) if bold && bright_bold && i < 8 => self.ansi[i as usize + 8],
            Color::Idx(i) => self.indexed(i),
            Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        }
    }

    pub fn bg_color(&self, col: Color) -> Option<Color32> {
        match col {
            Color::Default => None,
            Color::Idx(i) => Some(self.indexed(i)),
            Color::Rgb(r, g, b) => Some(Color32::from_rgb(r, g, b)),
        }
    }

    pub fn is_light(&self) -> bool {
        let [r, g, b, _] = self.bg.to_array();
        (r as u32 + g as u32 + b as u32) > 384
    }
}
