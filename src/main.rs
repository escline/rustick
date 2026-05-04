#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{Local, Timelike};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// Palette sampled from the app icon
const TIME_COLOR:  Color32 = Color32::from_rgb(225, 88,  20); // bright burnt orange
const BG_DARK:     Color32 = Color32::from_rgb( 22,  9,   0); // near-black warm brown
const BG_MENU:     Color32 = Color32::from_rgb( 32, 13,   0); // slightly lighter for menu
const BORDER_COL:  Color32 = Color32::from_rgb(115, 48,  12); // dark burnt orange border
const DATE_COLOR:  Color32 = Color32::from_rgb(118, 72,  30); // muted warm brown
const PIN_COLOR:   Color32 = Color32::from_rgb(200, 68,  16); // pin indicator

#[cfg(has_icon_png)]
fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/icon.png");
    image::load_from_memory(bytes).ok().map(|img| {
        let img = img.into_rgba8();
        let (w, h) = (img.width(), img.height());
        egui::IconData { rgba: img.into_raw(), width: w, height: h }
    })
}

#[cfg(not(has_icon_png))]
fn load_icon() -> Option<egui::IconData> {
    None
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        persist_window: true,
        viewport: {
            let mut vp = egui::ViewportBuilder::default()
                .with_title("Rustick")
                .with_inner_size([320.0, 140.0])
                .with_min_inner_size([240.0, 100.0]);
            if let Some(icon) = load_icon() {
                vp = vp.with_icon(icon);
            }
            vp
        },
        ..Default::default()
    };

    eframe::run_native(
        "Rustick",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let app: ClockApp = cc.storage
                .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
                .unwrap_or_default();
            Ok(Box::new(app))
        }),
    )
}

#[derive(Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum DisplayMode {
    #[default]
    Normal,
    Compact,
    Tiny,
}

impl DisplayMode {
    fn window_size(self) -> Vec2 {
        match self {
            Self::Normal => Vec2::new(320.0, 140.0),
            Self::Compact => Vec2::new(230.0, 65.0),
            Self::Tiny => Vec2::new(116.0, 36.0),
        }
    }

    fn is_borderless(self) -> bool {
        matches!(self, Self::Compact | Self::Tiny)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct ClockApp {
    always_on_top: bool,
    display_mode: DisplayMode,
    show_seconds: bool,
    // These fields are runtime-only and must not be persisted
    #[serde(skip)]
    applied_always_on_top: Option<bool>,
    #[serde(skip)]
    applied_display_mode: Option<DisplayMode>,
    #[serde(skip)]
    popup_pos: Option<Pos2>,
    #[serde(skip)]
    popup_frames: u32,
}

#[derive(Clone, Copy)]
enum Action {
    ToggleAlwaysOnTop,
    SetMode(DisplayMode),
}

impl eframe::App for ClockApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Local::now();
        let repaint_ms = if self.show_seconds {
            1000 - now.timestamp_subsec_millis() as u64
        } else {
            let ms_into_minute =
                now.second() as u64 * 1000 + now.timestamp_subsec_millis() as u64;
            60_000u64.saturating_sub(ms_into_minute).max(1)
        };
        ctx.request_repaint_after(Duration::from_millis(repaint_ms));

        let time_str = if self.show_seconds {
            now.format("%H:%M:%S").to_string()
        } else {
            now.format("%H:%M").to_string()
        };
        let date_str = now.format("%a, %b %d %Y").to_string();

        if self.applied_always_on_top != Some(self.always_on_top) {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                if self.always_on_top {
                    egui::viewport::WindowLevel::AlwaysOnTop
                } else {
                    egui::viewport::WindowLevel::Normal
                },
            ));
            self.applied_always_on_top = Some(self.always_on_top);
        }

        if self.applied_display_mode != Some(self.display_mode) {
            let mode = self.display_mode;
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!mode.is_borderless()));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(mode.window_size()));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(
                mode == DisplayMode::Normal,
            ));
            self.applied_display_mode = Some(mode);
        }

        match self.display_mode {
            DisplayMode::Normal => show_normal(ctx, &time_str, &date_str, self.always_on_top),
            DisplayMode::Compact => show_compact(ctx, &time_str),
            DisplayMode::Tiny => show_tiny(ctx, &time_str),
        }

        // Detect right-click in any mode and open the floating popup menu.
        // show_viewport_immediate creates a real OS-level window so the menu
        // is never clipped by the small clock window bounds.
        if ctx.input(|i| i.pointer.secondary_clicked()) {
            let screen_pos = ctx.input(|i| {
                let inner_origin = i.viewport().inner_rect.map(|r| r.min).unwrap_or(Pos2::ZERO);
                let ptr = i.pointer.interact_pos().unwrap_or(Pos2::ZERO);
                Pos2::new(inner_origin.x + ptr.x, inner_origin.y + ptr.y)
            });
            self.popup_pos = Some(screen_pos);
            self.popup_frames = 0;
        }

        if let Some(popup_pos) = self.popup_pos {
            self.popup_frames += 1;
            let frames = self.popup_frames;
            let always_on_top = self.always_on_top;
            let display_mode = self.display_mode;
            let mut close_menu = false;
            let mut popup_action: Option<Action> = None;
            let mut should_exit = false;

            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("context_popup"),
                egui::ViewportBuilder::default()
                    .with_decorations(false)
                    .with_inner_size([185.0, 130.0])
                    .with_position(popup_pos)
                    .with_resizable(false)
                    .with_window_level(egui::viewport::WindowLevel::AlwaysOnTop),
                |ctx, _class| {
                    // Close when focus moves elsewhere (give a few frames to gain focus first)
                    if frames > 3 && ctx.input(|i| i.viewport().focused == Some(false)) {
                        close_menu = true;
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close_menu = true;
                    }

                    egui::CentralPanel::default()
                        .frame(
                            Frame::new()
                                .fill(BG_MENU)
                                .stroke(Stroke::new(1.0, BORDER_COL))
                                .inner_margin(Margin::same(4)),
                        )
                        .show(ctx, |ui| {
                            ui.set_min_width(175.0);
                            // Buttons blend with the menu background; keep a warm hover highlight
                            ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                            ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
                            ui.visuals_mut().widgets.hovered.weak_bg_fill =
                                Color32::from_rgb(80, 32, 8);
                            ui.visuals_mut().widgets.active.weak_bg_fill =
                                Color32::from_rgb(110, 44, 10);

                            let aot_label = if always_on_top {
                                "✔  Always on Top"
                            } else {
                                "     Always on Top"
                            };
                            if ui.button(aot_label).clicked() {
                                popup_action = Some(Action::ToggleAlwaysOnTop);
                                close_menu = true;
                            }

                            ui.separator();

                            for (label, variant) in [
                                ("Normal", DisplayMode::Normal),
                                ("Compact", DisplayMode::Compact),
                                ("Tiny", DisplayMode::Tiny),
                            ] {
                                let text = if display_mode == variant {
                                    format!("✔  {label}")
                                } else {
                                    format!("     {label}")
                                };
                                if ui.button(text).clicked() {
                                    popup_action = Some(Action::SetMode(variant));
                                    close_menu = true;
                                }
                            }

                            ui.separator();

                            if ui.button("Exit").clicked() {
                                should_exit = true;
                                close_menu = true;
                            }
                        });
                },
            );

            if close_menu {
                self.popup_pos = None;
            }
            if let Some(a) = popup_action {
                match a {
                    Action::ToggleAlwaysOnTop => self.always_on_top = !self.always_on_top,
                    Action::SetMode(mode) => self.display_mode = mode,
                }
            }
            if should_exit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Left-click toggles seconds display (guard against clicks that dismiss the menu)
        if self.popup_pos.is_none() && ctx.input(|i| i.pointer.primary_clicked()) {
            self.show_seconds = !self.show_seconds;
        }
    }
}

fn show_normal(ctx: &egui::Context, time_str: &str, date_str: &str, always_on_top: bool) {

    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_DARK))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            // Allocate the full rect so drag-to-move works in compact (not used in normal,
            // but keeps the pattern consistent and prevents egui from inserting extra padding).
            let _ = ui.allocate_rect(rect, Sense::hover());
            let painter = ui.painter();
            let center = rect.center();

            painter.text(
                (center.x, center.y - 12.0).into(),
                Align2::CENTER_CENTER,
                time_str,
                FontId::monospace(48.0),
                TIME_COLOR,
            );

            painter.text(
                (center.x, center.y + 36.0).into(),
                Align2::CENTER_CENTER,
                date_str,
                FontId::proportional(14.0),
                DATE_COLOR,
            );

            if always_on_top {
                painter.text(
                    (rect.right() - 8.0, rect.top() + 8.0).into(),
                    Align2::RIGHT_TOP,
                    "📌",
                    FontId::proportional(12.0),
                    PIN_COLOR,
                );
            }
        });
}

fn show_compact(ctx: &egui::Context, time_str: &str) {
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(BG_DARK)
                .stroke(Stroke::new(1.5, BORDER_COL))
                .corner_radius(CornerRadius::same(5))
                .inner_margin(Margin::symmetric(12, 8)),
        )
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(rect, Sense::click_and_drag());

            if response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                time_str,
                FontId::monospace(32.0),
                TIME_COLOR,
            );
        });
}

fn show_tiny(ctx: &egui::Context, time_str: &str) {
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(BG_DARK)
                .stroke(Stroke::new(1.0, BORDER_COL))
                .corner_radius(CornerRadius::same(3))
                .inner_margin(Margin::symmetric(5, 2)),
        )
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(rect, Sense::click_and_drag());

            if response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                time_str,
                FontId::monospace(16.0),
                TIME_COLOR,
            );
        });
}
