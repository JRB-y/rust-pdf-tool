//! A small window around the pdftool library: drop PDFs in, edit their
//! metadata, merge them, or keep, delete and rotate pages.

mod app;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([940.0, 640.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("pdftool"),
        ..Default::default()
    };

    eframe::run_native("pdftool", options, Box::new(|_cc| Ok(Box::<app::App>::default())))
}
