# MyTerm

A small terminal app for macOS, written in Rust with [egui](https://github.com/emilk/egui).

## Features

- Runs your real login shell (xterm-256color, true colour, bold/italic/underline)
- Tabs (⌘T, ⌘1–9, ⌘← / ⌘→) with activity and bell markers
- Split panes side by side (⌘D) or top and bottom (⇧⌘D), with draggable dividers
- Find in the screen and history (⌘F, ⌘G / ⇧⌘G)
- Copy on select, double-click for a word, triple-click for a line, paste with ⌘V
- Scrollback history (mouse wheel, ⇧PageUp / ⇧PageDown), clear with ⌘K
- Full-screen programs (alternate screen, mouse reporting, bracketed paste)
- Settings (⌘,): five colour themes, text size, cursor shape and blink, Option-as-Meta

## Build and run

```sh
cargo run --release
```

`--screenshot out.png` draws about 120 frames, saves the window as a PNG and exits.
