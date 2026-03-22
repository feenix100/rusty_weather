//! Main GUI application state and rendering logic.
//!
//! This file demonstrates an idiomatic pattern for keeping egui responsive:
//! - UI thread collects input and starts background work.
//! - Worker thread performs network requests.
//! - Message channel sends success/error back into app state.

use crate::animation;
use crate::date_utils;
use crate::history_service;
use crate::models::{
    AnimationMode, HistoryEntry, InputMode, LocationQuery, WeatherQueryRequest, WeatherQueryResult,
    QUICK_CITIES,
};
use crate::weather_code_map;
use crate::weather_service;
use chrono::NaiveDate;
use eframe::egui::{self, Color32, RichText, Sense};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

enum WorkerMessage {
    Success(WeatherQueryResult),
    Error(String),
}

pub struct CyberWeatherApp {
    input_mode: InputMode,
    city_input: String,
    state_input: String,
    latitude_input: String,
    longitude_input: String,
    start_date_input: String,
    end_date_input: String,

    loading: bool,
    status_text: String,
    latest_result: Option<WeatherQueryResult>,

    animation_override: AnimationMode,
    auto_animation_mode: AnimationMode,
    animation_started_at: Instant,
    text_scale: f32,
    applied_text_scale: f32,

    history_entries: Vec<HistoryEntry>,
    history_path: PathBuf,
    show_export_popup: bool,
    export_popup_message: String,

    worker_rx: Option<Receiver<WorkerMessage>>,
}

impl CyberWeatherApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_cyber_theme(&cc.egui_ctx);

        let history_path = history_service::history_file_path();
        let history_entries = history_service::load_history(&history_path).unwrap_or_else(|_| Vec::new());

        CyberWeatherApp {
            input_mode: InputMode::CityState,
            city_input: "Phoenix".to_owned(),
            state_input: "AZ".to_owned(),
            latitude_input: "33.4484".to_owned(),
            longitude_input: "-112.0740".to_owned(),
            start_date_input: date_utils::today_string(),
            end_date_input: date_utils::today_string(),
            loading: false,
            status_text: "Ready".to_owned(),
            latest_result: None,
            animation_override: AnimationMode::Auto,
            auto_animation_mode: AnimationMode::Cloud,
            animation_started_at: Instant::now(),
            text_scale: 1.0,
            applied_text_scale: 1.0,
            history_entries,
            history_path,
            show_export_popup: false,
            export_popup_message: String::new(),
            worker_rx: None,
        }
    }

    fn poll_worker(&mut self) {
        let mut clear_receiver = false;
        if let Some(rx) = &self.worker_rx {
            match rx.try_recv() {
                Ok(WorkerMessage::Success(result)) => {
                    self.loading = false;
                    self.status_text = "Weather loaded".to_owned();
                    self.auto_animation_mode = self.pick_auto_animation_mode(&result);
                    self.latest_result = Some(result.clone());

                    self.history_entries.push(HistoryEntry::from_result(&result));
                    if let Err(err) = history_service::save_history(&self.history_path, &self.history_entries)
                    {
                        self.status_text = format!(
                            "Weather loaded, but failed to save history: {err}"
                        );
                    }
                    clear_receiver = true;
                }
                Ok(WorkerMessage::Error(err)) => {
                    self.loading = false;
                    self.status_text = format!("Error: {err}");
                    clear_receiver = true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.loading = false;
                    if self.status_text == "Fetching weather..." {
                        self.status_text = "Error: worker channel disconnected".to_owned();
                    }
                    clear_receiver = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        if clear_receiver {
            self.worker_rx = None;
        }
    }

    fn pick_auto_animation_mode(&self, result: &WeatherQueryResult) -> AnimationMode {
        if let Some(code) = result.days.first().and_then(|d| d.weather_code) {
            return weather_code_map::map_weather_code(code).1;
        }
        if let Some(code) = result.current_weather_code {
            return weather_code_map::map_weather_code(code).1;
        }
        AnimationMode::Cloud
    }

    fn parse_date_range(&self) -> Result<(NaiveDate, NaiveDate), String> {
        if self.start_date_input.trim().is_empty() {
            return Err("Start date is required.".to_owned());
        }
        if self.end_date_input.trim().is_empty() {
            return Err("End date is required.".to_owned());
        }

        let start = date_utils::parse_date(&self.start_date_input)?;
        let end = date_utils::parse_date(&self.end_date_input)?;
        if start > end {
            return Err("Start date must be on or before End date.".to_owned());
        }

        Ok((start, end))
    }

    fn build_request(&self) -> Result<WeatherQueryRequest, String> {
        let (start_date, end_date) = self.parse_date_range()?;

        let location = match self.input_mode {
            InputMode::CityState => {
                let city = self.city_input.trim();
                let state = self.state_input.trim();
                if city.is_empty() || state.is_empty() {
                    return Err("City and state are required in City/State mode.".to_owned());
                }
                LocationQuery::CityState {
                    city: city.to_owned(),
                    state: state.to_owned(),
                }
            }
            InputMode::Coordinates => {
                let lat_text = self.latitude_input.trim();
                let lon_text = self.longitude_input.trim();
                if lat_text.is_empty() || lon_text.is_empty() {
                    return Err("Latitude and longitude are required in coordinate mode.".to_owned());
                }

                let latitude = lat_text
                    .parse::<f64>()
                    .map_err(|_| "Invalid latitude. Enter a numeric value.".to_owned())?;
                let longitude = lon_text
                    .parse::<f64>()
                    .map_err(|_| "Invalid longitude. Enter a numeric value.".to_owned())?;

                LocationQuery::Coordinates {
                    latitude,
                    longitude,
                }
            }
        };

        Ok(WeatherQueryRequest {
            location,
            start_date,
            end_date,
        })
    }

    fn start_fetch(&mut self) {
        if self.loading {
            return;
        }

        let request = match self.build_request() {
            Ok(req) => req,
            Err(err) => {
                self.status_text = format!("Error: {err}");
                return;
            }
        };

        let (tx, rx) = mpsc::channel::<WorkerMessage>();
        self.worker_rx = Some(rx);
        self.loading = true;
        self.status_text = "Fetching weather...".to_owned();

        thread::spawn(move || {
            let message = match weather_service::fetch_weather(request) {
                Ok(result) => WorkerMessage::Success(result),
                Err(err) => WorkerMessage::Error(err),
            };
            let _ = tx.send(message);
        });
    }

    fn use_today_for_both(&mut self) {
        let today = date_utils::today_string();
        self.start_date_input = today.clone();
        self.end_date_input = today;
    }

    fn draw_header(&self, ui: &mut egui::Ui) {
        ui.heading(
            RichText::new("CYBER WEATHER CONSOLE")
                .color(Color32::from_rgb(0, 255, 210))
                .strong()
                .size(28.0),
        );
        ui.label(
            RichText::new("Open-Meteo powered desktop dashboard with geocoding, date-range lookup, and neon weather visuals.")
                .color(Color32::from_rgb(145, 230, 255)),
        );
    }

    fn draw_quick_city_buttons(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("Quick Cities").strong().color(Color32::from_rgb(255, 80, 220)));
            ui.horizontal_wrapped(|ui| {
                for entry in QUICK_CITIES {
                    let label = format!("{}, {}", entry.city, entry.state);
                    if ui.button(label).clicked() {
                        self.input_mode = InputMode::CityState;
                        self.city_input = entry.city.to_owned();
                        self.state_input = entry.state.to_owned();
                        self.status_text = "Quick city selected".to_owned();
                    }
                }
            });
        });
    }

    fn draw_inputs(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("Query Controls").strong().color(Color32::from_rgb(0, 255, 210)));

            ui.horizontal(|ui| {
                ui.label("Input mode:");
                ui.selectable_value(&mut self.input_mode, InputMode::CityState, "City / State");
                ui.selectable_value(&mut self.input_mode, InputMode::Coordinates, "Latitude / Longitude");
            });

            ui.add_space(4.0);

            match self.input_mode {
                InputMode::CityState => {
                    ui.horizontal(|ui| {
                        ui.label("City:");
                        ui.text_edit_singleline(&mut self.city_input);
                        ui.label("State:");
                        ui.text_edit_singleline(&mut self.state_input);
                    });
                }
                InputMode::Coordinates => {
                    ui.horizontal(|ui| {
                        ui.label("Latitude:");
                        ui.text_edit_singleline(&mut self.latitude_input);
                        ui.label("Longitude:");
                        ui.text_edit_singleline(&mut self.longitude_input);
                    });
                }
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Start Date:");
                ui.text_edit_singleline(&mut self.start_date_input);
                ui.label("End Date:");
                ui.text_edit_singleline(&mut self.end_date_input);
            });

            ui.horizontal(|ui| {
                if ui.button("Today").clicked() {
                    self.use_today_for_both();
                    self.status_text = "Date range set to today".to_owned();
                }
                if ui.button("Use Today For Both").clicked() {
                    self.use_today_for_both();
                    self.status_text = "Date range set to today".to_owned();
                }
            });

            ui.horizontal(|ui| {
                // Place Get Weather directly under the Today row and keep default button sizing.
                let get_weather_text = if self.loading {
                    RichText::new("Fetching Weather...")
                        .strong()
                        .color(Color32::from_rgb(12, 20, 26))
                } else {
                    RichText::new("Get Weather")
                        .strong()
                        .color(Color32::from_rgb(8, 24, 26))
                };
                let get_weather_btn = egui::Button::new(get_weather_text)
                    .fill(if self.loading {
                        Color32::from_rgb(120, 155, 160)
                    } else {
                        Color32::from_rgb(98, 206, 198)
                    })
                    .stroke(egui::Stroke::new(2.0, Color32::from_rgb(255, 120, 230)));
                if ui
                    .add_enabled(
                        !self.loading,
                        get_weather_btn.min_size(egui::vec2(200.0, 44.0)),
                    )
                    .clicked()
                {
                    self.start_fetch();
                }
            });

            if self.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Fetching weather...").color(Color32::from_rgb(255, 120, 220)));
                });
            }

            ui.label(
                RichText::new(format!("Status: {}", self.status_text))
                    .color(Color32::from_rgb(170, 255, 190)),
            );

            let range_label = format!(
                "Selected range: {} -> {}",
                self.start_date_input, self.end_date_input
            );
            ui.label(RichText::new(range_label).color(Color32::from_rgb(130, 200, 250)));
        });
    }

    fn draw_display_scale_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(
                RichText::new("Display Scale")
                    .strong()
                    .color(Color32::from_rgb(255, 80, 220)),
            );
            ui.horizontal(|ui| {
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut self.text_scale, 0.8..=1.8).text("Scale"));
                ui.label(format!("{:.2}x", self.text_scale));
            });
        });
    }

    fn draw_latest_result(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("Latest Result").strong().color(Color32::from_rgb(0, 255, 210)));

            let Some(result) = &self.latest_result else {
                ui.label("No weather result yet.");
                return;
            };

            let location_text = match (&result.city, &result.state) {
                (Some(city), Some(state)) => format!("{}, {}", city, state),
                _ => "Manual coordinates".to_owned(),
            };

            ui.group(|ui| {
                ui.label(
                    RichText::new(format!("Location: {location_text}"))
                        .strong()
                        .color(Color32::from_rgb(130, 230, 255)),
                );
                ui.label(
                    RichText::new(format!(
                        "Range: {} -> {}",
                        result.start_date, result.end_date
                    ))
                    .color(Color32::from_rgb(130, 230, 255)),
                );
                ui.label(
                    RichText::new(format!(
                        "Coordinates: {:.4}, {:.4} | Timezone: {}",
                        result.latitude, result.longitude, result.timezone
                    ))
                    .color(Color32::from_rgb(130, 230, 255)),
                );
            });

            if let Some(temp) = result.current_temperature_c {
                let feels = result
                    .current_apparent_temperature_c
                    .map(fmt_temp_f_then_c)
                    .unwrap_or_else(|| "n/a".to_owned());
                let desc = result
                    .current_weather_description
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_owned());

                ui.label(
                    RichText::new(format!(
                        "Current: {} (Feels {}) | {}",
                        fmt_temp_f_then_c(temp),
                        feels,
                        desc
                    ))
                    .color(Color32::from_rgb(255, 160, 100)),
                );
            }

            ui.add_space(4.0);
            if result.days.len() == 1 {
                let day = &result.days[0];
                ui.group(|ui| {
                    ui.label(
                        RichText::new(format!("{} | {}", day.day_date, day.weather_description))
                            .strong()
                            .color(Color32::from_rgb(255, 140, 220)),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("High: {}", fmt_temp_opt_f_then_c(day.temperature_max_c)));
                        ui.label(format!("Low: {}", fmt_temp_opt_f_then_c(day.temperature_min_c)));
                        ui.label(format!("Precip: {}", fmt_precip(day.precipitation_mm)));
                        ui.label(format!("Wind: {}", fmt_wind(day.wind_speed_max_kmh)));
                        ui.label(format!(
                            "Code: {}",
                            day.weather_code
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "n/a".to_owned())
                        ));
                        ui.label(format!("Source: {}", day.source));
                    });
                });
            } else {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_height(260.0)
                    .show(ui, |ui| {
                        // Keep a minimum table width so a horizontal scrollbar appears on narrow panes.
                        ui.set_min_width(860.0);
                        egui::Grid::new("daily_weather_grid")
                            .num_columns(8)
                            .striped(true)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Date").strong());
                                ui.label(RichText::new("Condition").strong());
                                ui.label(RichText::new("High").strong());
                                ui.label(RichText::new("Low").strong());
                                ui.label(RichText::new("Precip").strong());
                                ui.label(RichText::new("Wind").strong());
                                ui.label(RichText::new("Code").strong());
                                ui.label(RichText::new("Source").strong());
                                ui.end_row();

                                for day in &result.days {
                                    ui.label(&day.day_date);
                                    ui.label(&day.weather_description);
                                    ui.label(fmt_temp_opt_f_then_c(day.temperature_max_c));
                                    ui.label(fmt_temp_opt_f_then_c(day.temperature_min_c));
                                    ui.label(fmt_precip(day.precipitation_mm));
                                    ui.label(fmt_wind(day.wind_speed_max_kmh));
                                    ui.label(
                                        day.weather_code
                                            .map(|c| c.to_string())
                                            .unwrap_or_else(|| "n/a".to_owned()),
                                    );
                                    ui.label(&day.source);
                                    ui.end_row();
                                }
                            });
                    });
            }
        });
    }

    fn draw_animation_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("Weather Visualization").strong().color(Color32::from_rgb(255, 80, 220)));

            ui.horizontal(|ui| {
                ui.label("Animation mode:");
                egui::ComboBox::from_id_salt("anim_mode_combo")
                    .selected_text(self.animation_override.label())
                    .show_ui(ui, |ui| {
                        for mode in AnimationMode::all() {
                            ui.selectable_value(&mut self.animation_override, mode, mode.label());
                        }
                    });

                ui.label(
                    RichText::new(
                        "Auto uses the first returned day's weather code (or current code fallback).",
                    )
                    .color(Color32::from_rgb(130, 200, 250)),
                );
            });

            let effective_mode = if self.animation_override == AnimationMode::Auto {
                self.auto_animation_mode
            } else {
                self.animation_override
            };

            let t = self.animation_started_at.elapsed().as_secs_f32();
            animation::draw_weather_animation(ui, effective_mode, t);
        });
    }

    fn draw_history_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("History / Log").strong().color(Color32::from_rgb(0, 255, 210)));
                if ui.button("Export CSV").clicked() {
                    match history_service::export_history_csv(&self.history_entries) {
                        Ok(path) => {
                            self.status_text = format!("Export complete: {}", path.display());
                            self.export_popup_message =
                                format!("CSV export complete.\n\nSaved to:\n{}", path.display());
                            self.show_export_popup = true;
                        }
                        Err(err) => {
                            self.status_text = format!("Error: CSV export failed: {err}");
                        }
                    }
                }
                if ui.button("Clear Log").clicked() {
                    match history_service::clear_history(&self.history_path) {
                        Ok(()) => {
                            self.history_entries.clear();
                            self.status_text = "Log cleared".to_owned();
                        }
                        Err(err) => {
                            self.status_text = format!("Error: failed to clear log: {err}");
                        }
                    }
                }
            });

            ui.label(
                RichText::new(format!("Entries: {}", self.history_entries.len()))
                    .color(Color32::from_rgb(130, 200, 250)),
            );

            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for entry in self.history_entries.iter().rev() {
                        let row = format!(
                            "{} | {}, {} | {} -> {} | {:.4}, {:.4} | {} | {}",
                            entry.timestamp,
                            entry.city,
                            entry.state,
                            entry.start_date,
                            entry.end_date,
                            entry.latitude,
                            entry.longitude,
                            entry.timezone,
                            entry.source,
                        );
                        ui.label(row);
                        ui.add_space(2.0);
                    }
                });
        });
    }
}

impl eframe::App for CyberWeatherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();

        // Smoothly interpolate text scale to avoid abrupt redraw jumps while dragging.
        let delta = self.text_scale - self.applied_text_scale;
        if delta.abs() > 0.0005 {
            self.applied_text_scale += delta * 0.22;
            if (self.text_scale - self.applied_text_scale).abs() < 0.002 {
                self.applied_text_scale = self.text_scale;
            }
        }
        ctx.set_pixels_per_point(self.applied_text_scale);

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.draw_header(ui);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| self.draw_display_scale_panel(ui),
                        );
                    });
                    ui.add_space(8.0);
                    self.draw_quick_city_buttons(ui);
                    ui.add_space(8.0);

                    ui.columns(2, |columns| {
                        self.draw_inputs(&mut columns[0]);
                        self.draw_latest_result(&mut columns[1]);
                    });

                    ui.add_space(8.0);
                    self.draw_animation_panel(ui);
                    ui.add_space(8.0);
                    self.draw_history_panel(ui);

                    // Invisible hover area encourages frequent repaints for smooth animation,
                    // while still allowing egui to remain event-driven for the rest of the UI.
                    let _ = ui.allocate_response(egui::vec2(1.0, 1.0), Sense::hover());
                });
        });

        if self.show_export_popup {
            let mut open = self.show_export_popup;
            egui::Window::new("Export Complete")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(&self.export_popup_message);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        self.show_export_popup = false;
                    }
                });
            self.show_export_popup = self.show_export_popup && open;
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn fmt_precip(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.1} mm"))
        .unwrap_or_else(|| "n/a".to_owned())
}

fn fmt_wind(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.1} km/h"))
        .unwrap_or_else(|| "n/a".to_owned())
}

fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn fmt_temp_f_then_c(celsius: f64) -> String {
    format!("{:.1}F ({:.1}C)", c_to_f(celsius), celsius)
}

fn fmt_temp_opt_f_then_c(value: Option<f64>) -> String {
    value
        .map(fmt_temp_f_then_c)
        .unwrap_or_else(|| "n/a".to_owned())
}

fn apply_cyber_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(Color32::from_rgb(190, 245, 255));
    visuals.panel_fill = Color32::from_rgb(7, 10, 18);
    visuals.window_fill = Color32::from_rgb(10, 14, 24);
    visuals.faint_bg_color = Color32::from_rgb(12, 20, 32);
    visuals.extreme_bg_color = Color32::from_rgb(3, 6, 11);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(14, 26, 42);
    visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(140, 220, 240);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(25, 55, 80);
    visuals.widgets.hovered.fg_stroke.color = Color32::from_rgb(0, 255, 210);
    visuals.widgets.active.bg_fill = Color32::from_rgb(45, 20, 60);
    visuals.widgets.active.fg_stroke.color = Color32::from_rgb(255, 90, 220);

    visuals.selection.bg_fill = Color32::from_rgb(0, 120, 140);
    visuals.selection.stroke.color = Color32::from_rgb(0, 255, 210);

    ctx.set_visuals(visuals);
}
