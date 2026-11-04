//! What each key press sends to the shell.

use egui::{Key, Modifiers};

/// Bytes a key press sends to the shell. The bool says "ignore the text
/// event that follows" (used for Control and Option-as-Meta combinations).
pub fn key_bytes(key: Key, m: Modifiers, app_cursor: bool, option_meta: bool) -> Option<(Vec<u8>, bool)> {
    let esc = |s: &str| Some((s.as_bytes().to_vec(), false));
    let modn = 1 + (m.shift as u8) + 2 * (m.alt as u8) + 4 * (m.ctrl as u8);
    let arrow = |c: char| {
        if modn > 1 {
            Some((format!("\x1b[1;{modn}{c}").into_bytes(), false))
        } else if app_cursor {
            Some((format!("\x1bO{c}").into_bytes(), false))
        } else {
            Some((format!("\x1b[{c}").into_bytes(), false))
        }
    };
    let tilde = |n: u8| {
        if modn > 1 {
            Some((format!("\x1b[{n};{modn}~").into_bytes(), false))
        } else {
            Some((format!("\x1b[{n}~").into_bytes(), false))
        }
    };
    match key {
        Key::ArrowUp => return arrow('A'),
        Key::ArrowDown => return arrow('B'),
        Key::ArrowRight => return arrow('C'),
        Key::ArrowLeft => return arrow('D'),
        Key::Home => return arrow('H'),
        Key::End => return arrow('F'),
        Key::PageUp => return tilde(5),
        Key::PageDown => return tilde(6),
        Key::Insert => return tilde(2),
        Key::Delete => return tilde(3),
        Key::Enter => return Some((if m.alt && option_meta { b"\x1b\r".to_vec() } else { b"\r".to_vec() }, false)),
        Key::Tab => return if m.shift { esc("\x1b[Z") } else { esc("\t") },
        Key::Backspace => {
            let v = if m.ctrl {
                vec![0x08]
            } else if m.alt {
                b"\x1b\x7f".to_vec()
            } else {
                vec![0x7f]
            };
            return Some((v, false));
        }
        Key::Escape => return esc("\x1b"),
        Key::F1 => return esc("\x1bOP"),
        Key::F2 => return esc("\x1bOQ"),
        Key::F3 => return esc("\x1bOR"),
        Key::F4 => return esc("\x1bOS"),
        Key::F5 => return tilde(15),
        Key::F6 => return tilde(17),
        Key::F7 => return tilde(18),
        Key::F8 => return tilde(19),
        Key::F9 => return tilde(20),
        Key::F10 => return tilde(21),
        Key::F11 => return tilde(23),
        Key::F12 => return tilde(24),
        _ => {}
    }
    let name = key.name();
    let letter = (name.len() == 1)
        .then(|| name.chars().next().unwrap())
        .filter(|c| c.is_ascii_alphanumeric());
    if m.ctrl {
        let b = match key {
            Key::Space => Some(0u8),
            Key::OpenBracket => Some(0x1b),
            Key::Backslash => Some(0x1c),
            Key::CloseBracket => Some(0x1d),
            Key::Slash | Key::Minus => Some(0x1f),
            _ => letter
                .filter(|c| c.is_ascii_alphabetic())
                .map(|c| (c.to_ascii_uppercase() as u8) - b'A' + 1),
        };
        if let Some(b) = b {
            let mut v = Vec::new();
            if m.alt && option_meta {
                v.push(0x1b);
            }
            v.push(b);
            return Some((v, true));
        }
    }
    if m.alt && option_meta {
        if let Some(c) = letter {
            let c = if m.shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() };
            return Some((vec![0x1b, c as u8], true));
        }
        if key == Key::Space {
            return Some((b"\x1b ".to_vec(), true));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_c_is_etx() {
        let m = Modifiers { ctrl: true, ..Default::default() };
        assert_eq!(key_bytes(Key::C, m, false, false).unwrap().0, vec![3]);
    }

    #[test]
    fn arrows_follow_cursor_mode() {
        let m = Modifiers::default();
        assert_eq!(key_bytes(Key::ArrowUp, m, false, false).unwrap().0, b"\x1b[A");
        assert_eq!(key_bytes(Key::ArrowUp, m, true, false).unwrap().0, b"\x1bOA");
    }
}
