//! Procedural weather animation renderer for the cyber visualization panel.
//!
//! Networking and business logic never touch this module. It only reads an animation mode
//! and paints a dynamic scene each frame.

use crate::models::AnimationMode;
use egui::{pos2, vec2, Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind, Ui};

pub fn draw_weather_animation(ui: &mut Ui, mode: AnimationMode, time_seconds: f32) {
    let desired_size = vec2(ui.available_width(), 240.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    draw_background(&painter, rect, time_seconds);

    match mode {
        AnimationMode::Auto => draw_cloud_scene(&painter, rect, time_seconds),
        AnimationMode::Sunny => draw_sunny_scene(&painter, rect, time_seconds),
        AnimationMode::Rain => draw_rain_scene(&painter, rect, time_seconds),
        AnimationMode::Snow => draw_snow_scene(&painter, rect, time_seconds),
        AnimationMode::Cloud => draw_cloud_scene(&painter, rect, time_seconds),
    }

    draw_hud_overlay(&painter, rect, time_seconds);
}

fn draw_background(painter: &Painter, rect: Rect, t: f32) {
    let pulse = ((t * 0.8).sin() * 0.5 + 0.5) as f32;
    let top = Color32::from_rgb(8, 18 + (pulse * 12.0) as u8, 26);
    let bottom = Color32::from_rgb(2, 6, 12);

    painter.rect_filled(rect, CornerRadius::same(10), top);

    let gradient_steps = 18;
    for i in 0..gradient_steps {
        let frac = i as f32 / gradient_steps as f32;
        let y0 = egui::lerp(rect.top()..=rect.bottom(), frac);
        let y1 = egui::lerp(rect.top()..=rect.bottom(), frac + (1.0 / gradient_steps as f32));
        let alpha = (40.0 * (1.0 - frac)) as u8;
        painter.rect_filled(
            Rect::from_min_max(pos2(rect.left(), y0), pos2(rect.right(), y1)),
            0.0,
            Color32::from_rgba_unmultiplied(bottom.r(), bottom.g(), bottom.b(), alpha),
        );
    }
}

fn draw_hud_overlay(painter: &Painter, rect: Rect, t: f32) {
    for i in 0..14 {
        let y = rect.top() + i as f32 * (rect.height() / 14.0);
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 255, 220, 12)),
        );
    }

    // Subtle moving scanline for a terminal-console vibe.
    let scan_y = rect.top() + (t * 45.0).rem_euclid(rect.height());
    painter.rect_filled(
        Rect::from_min_size(pos2(rect.left(), scan_y), vec2(rect.width(), 2.0)),
        0.0,
        Color32::from_rgba_unmultiplied(255, 0, 180, 34),
    );

    painter.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.5, Color32::from_rgb(0, 255, 210)),
        StrokeKind::Middle,
    );
}

fn draw_sunny_scene(p: &Painter, rect: Rect, t: f32) {
    let center = pos2(rect.center().x, rect.center().y - 20.0);
    let core_radius = 26.0 + (t * 2.0).sin() * 2.5;

    p.circle_filled(center, core_radius + 16.0, Color32::from_rgba_unmultiplied(255, 220, 40, 35));
    p.circle_filled(center, core_radius, Color32::from_rgb(255, 220, 30));

    for i in 0..12 {
        let angle = (i as f32 / 12.0) * std::f32::consts::TAU + t * 0.7;
        let dir = vec2(angle.cos(), angle.sin());
        let a = center + dir * (core_radius + 8.0);
        let b = center + dir * (core_radius + 28.0 + (t * 3.0 + i as f32).sin() * 3.0);
        p.line_segment([a, b], Stroke::new(2.0, Color32::from_rgb(255, 100, 240)));
    }
}

fn draw_cloud_scene(p: &Painter, rect: Rect, t: f32) {
    let base_y = rect.center().y;
    for i in 0..3 {
        let offset = i as f32 * 110.0;
        let x = rect.left() + (t * (22.0 + i as f32 * 7.0) + offset).rem_euclid(rect.width() + 180.0) - 90.0;
        draw_cloud(p, pos2(x, base_y - 35.0 + i as f32 * 22.0), 1.0 + i as f32 * 0.2);
    }
}

fn draw_cloud(p: &Painter, center: egui::Pos2, scale: f32) {
    let color = Color32::from_rgba_unmultiplied(120, 235, 255, 95);
    p.circle_filled(center + vec2(-22.0, 0.0) * scale, 20.0 * scale, color);
    p.circle_filled(center + vec2(0.0, -10.0) * scale, 24.0 * scale, color);
    p.circle_filled(center + vec2(26.0, 2.0) * scale, 18.0 * scale, color);
    p.rect_filled(
        Rect::from_center_size(center + vec2(0.0, 9.0) * scale, vec2(68.0, 22.0) * scale),
        10.0 * scale,
        color,
    );
}

fn draw_rain_scene(p: &Painter, rect: Rect, t: f32) {
    draw_cloud_scene(p, rect, t * 0.65);

    for i in 0..120 {
        let seed = i as f32 * 12.345;
        let x = rect.left() + (seed * 9.31).sin().abs() * rect.width();
        let speed = 130.0 + (i % 8) as f32 * 14.0;
        let y = rect.top() + (t * speed + seed * 17.0).rem_euclid(rect.height() + 30.0) - 15.0;
        p.line_segment(
            [pos2(x, y), pos2(x - 4.0, y + 12.0)],
            Stroke::new(1.4, Color32::from_rgba_unmultiplied(0, 255, 240, 180)),
        );
    }
}

fn draw_snow_scene(p: &Painter, rect: Rect, t: f32) {
    draw_cloud_scene(p, rect, t * 0.4);

    for i in 0..80 {
        let seed = i as f32 * 7.13;
        let drift = (t * 0.9 + seed).sin() * 10.0;
        let x = rect.left() + ((seed * 17.0).sin().abs() * rect.width()) + drift;
        let speed = 34.0 + (i % 5) as f32 * 7.0;
        let y = rect.top() + (t * speed + seed * 9.0).rem_euclid(rect.height() + 14.0) - 7.0;
        p.circle_filled(pos2(x, y), 1.8 + (i % 3) as f32 * 0.55, Color32::from_rgb(210, 250, 255));
    }
}
