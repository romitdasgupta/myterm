mod app;
mod chrome;
mod draw;
mod keys;
mod layout;
mod pane;
mod settings;
mod term;
mod theme;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let mut screenshot = None;
    let mut demo = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            // Splits the window and prints a colour sample (for checking the look).
            "--demo" => demo = true,
            _ => {}
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MyTerm")
            .with_inner_size([740.0, 470.0])
            .with_min_inner_size([320.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "MyTerm",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, screenshot, demo)))),
    )
}
