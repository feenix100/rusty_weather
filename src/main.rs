//! Entry point for the Cyber Weather Console desktop application.
//!
//! This file wires up eframe and launches the top-level app state defined in `app.rs`.

mod animation;
mod app;
mod date_utils;
mod geocoding_service;
mod history_service;
mod models;
mod weather_code_map;
mod weather_service;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 840.0])
            .with_min_inner_size([1080.0, 720.0])
            .with_title("Cyber Weather Console"),
        ..Default::default()
    };

    eframe::run_native(
        "Cyber Weather Console",
        options,
        Box::new(|cc| Ok(Box::new(app::CyberWeatherApp::new(cc)))),
    )
}
