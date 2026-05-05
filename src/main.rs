#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{Local, Timelike};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{MouseButton, TrayIconBuilder, TrayIconEvent};

// --- Color scheme ---

fn scale_color(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 * f).min(255.0) as u8,
        (c.g() as f32 * f).min(255.0) as u8,
        (c.b() as f32 * f).min(255.0) as u8,
    )
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

#[derive(Clone, Copy)]
struct ColorScheme {
    time_color: Color32,
    bg_dark: Color32,
    bg_menu: Color32,
    border_col: Color32,
    date_color: Color32,
    pin_color: Color32,
    hover_color: Color32,
    active_color: Color32,
}

impl Default for ColorScheme {
    fn default() -> Self {
        // Fallback: original burnt-orange palette sampled from the app icon
        Self::from_accent(Color32::from_rgb(225, 88, 20), false)
    }
}

impl ColorScheme {
    fn from_accent(accent: Color32, light_mode: bool) -> Self {
        if light_mode {
            let text = scale_color(accent, 0.70); // darken for contrast on light bg
            Self {
                time_color: text,
                bg_dark: Color32::from_rgb(245, 245, 245),
                bg_menu: Color32::from_rgb(232, 232, 232),
                border_col: scale_color(accent, 0.55),
                date_color: Color32::from_rgb(90, 90, 90),
                pin_color: text,
                hover_color: lerp_color(Color32::from_rgb(220, 220, 220), accent, 0.20),
                active_color: lerp_color(Color32::from_rgb(200, 200, 200), accent, 0.25),
            }
        } else {
            Self {
                time_color: accent,
                bg_dark: scale_color(accent, 0.07),
                bg_menu: scale_color(accent, 0.12),
                border_col: scale_color(accent, 0.45),
                date_color: scale_color(accent, 0.50),
                pin_color: scale_color(accent, 0.88),
                hover_color: scale_color(accent, 0.30),
                active_color: scale_color(accent, 0.42),
            }
        }
    }

    fn from_windows() -> Self {
        let accent = read_accent_color().unwrap_or(Color32::from_rgb(225, 88, 20));
        let light_mode = read_light_mode();
        Self::from_accent(accent, light_mode)
    }
}

fn read_accent_color() -> Option<Color32> {
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let dwm = hkcu.open_subkey("Software\\Microsoft\\Windows\\DWM").ok()?;
    // Prefer AccentColor; fall back to ColorizationColor
    let raw: u32 = dwm
        .get_value("AccentColor")
        .or_else(|_| dwm.get_value("ColorizationColor"))
        .ok()?;
    // Windows stores as 0xAABBGGRR
    Some(Color32::from_rgb(
        (raw & 0xFF) as u8,
        ((raw >> 8) & 0xFF) as u8,
        ((raw >> 16) & 0xFF) as u8,
    ))
}

fn read_light_mode() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
    )
    .ok()
    .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme").ok())
    .map(|v| v != 0)
    .unwrap_or(false)
}

// --- System tray ---

struct TrayHandle {
    _tray: tray_icon::TrayIcon,
    exit_id: tray_icon::menu::MenuId,
}

fn load_tray_icon() -> tray_icon::Icon {
    #[cfg(has_icon_png)]
    {
        let bytes = include_bytes!("../assets/icon.png");
        if let Ok(img) = image::load_from_memory(bytes) {
            let img = img.into_rgba8();
            let (w, h) = (img.width(), img.height());
            if let Ok(icon) = tray_icon::Icon::from_rgba(img.into_raw(), w, h) {
                return icon;
            }
        }
    }
    let rgba: Vec<u8> = (0..16 * 16).flat_map(|_| [225u8, 88, 20, 255]).collect();
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("fallback tray icon")
}

fn init_tray() -> Option<TrayHandle> {
    let exit_item = MenuItem::new("Exit Rustick", true, None);
    let exit_id = exit_item.id().clone();
    let menu = Menu::new();
    menu.append(&exit_item).ok()?;
    let tray = TrayIconBuilder::new()
        .with_icon(load_tray_icon())
        .with_tooltip("Rustick")
        .with_menu(Box::new(menu))
        .build()
        .ok()?;
    Some(TrayHandle { _tray: tray, exit_id })
}

// --- Icon loading ---

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
                .with_min_inner_size([240.0, 100.0])
                .with_taskbar(false);
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
            let light_mode = read_light_mode();
            cc.egui_ctx.set_visuals(if light_mode {
                egui::Visuals::light()
            } else {
                egui::Visuals::dark()
            });
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
    #[serde(skip)]
    applied_always_on_top: Option<bool>,
    #[serde(skip)]
    applied_display_mode: Option<DisplayMode>,
    #[serde(skip)]
    popup_pos: Option<Pos2>,
    #[serde(skip)]
    popup_frames: u32,
    #[serde(skip)]
    colors: Option<ColorScheme>,
    #[serde(skip)]
    tray: Option<TrayHandle>,
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

        let colors = *self.colors.get_or_insert_with(ColorScheme::from_windows);

        if self.tray.is_none() {
            self.tray = init_tray();
        }
        if let Some(ref tray_handle) = self.tray {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == tray_handle.exit_id {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
            }
        }

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
            DisplayMode::Normal => {
                show_normal(ctx, &time_str, &date_str, self.always_on_top, &colors)
            }
            DisplayMode::Compact => show_compact(ctx, &time_str, &colors),
            DisplayMode::Tiny => show_tiny(ctx, &time_str, &colors),
        }

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
                    if frames > 3 && ctx.input(|i| i.viewport().focused == Some(false)) {
                        close_menu = true;
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close_menu = true;
                    }

                    egui::CentralPanel::default()
                        .frame(
                            Frame::new()
                                .fill(colors.bg_menu)
                                .stroke(Stroke::new(1.0, colors.border_col))
                                .inner_margin(Margin::same(4)),
                        )
                        .show(ctx, |ui| {
                            ui.set_min_width(175.0);
                            ui.visuals_mut().widgets.inactive.weak_bg_fill =
                                Color32::TRANSPARENT;
                            ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
                            ui.visuals_mut().widgets.hovered.weak_bg_fill = colors.hover_color;
                            ui.visuals_mut().widgets.active.weak_bg_fill = colors.active_color;

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

        if self.popup_pos.is_none() && ctx.input(|i| i.pointer.primary_clicked()) {
            self.show_seconds = !self.show_seconds;
        }
    }
}

fn show_normal(
    ctx: &egui::Context,
    time_str: &str,
    date_str: &str,
    always_on_top: bool,
    colors: &ColorScheme,
) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(colors.bg_dark))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let _ = ui.allocate_rect(rect, Sense::hover());
            let painter = ui.painter();
            let center = rect.center();

            painter.text(
                (center.x, center.y - 12.0).into(),
                Align2::CENTER_CENTER,
                time_str,
                FontId::monospace(48.0),
                colors.time_color,
            );

            painter.text(
                (center.x, center.y + 36.0).into(),
                Align2::CENTER_CENTER,
                date_str,
                FontId::proportional(14.0),
                colors.date_color,
            );

            if always_on_top {
                painter.text(
                    (rect.right() - 8.0, rect.top() + 8.0).into(),
                    Align2::RIGHT_TOP,
                    "📌",
                    FontId::proportional(12.0),
                    colors.pin_color,
                );
            }
        });
}

fn show_compact(ctx: &egui::Context, time_str: &str, colors: &ColorScheme) {
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(colors.bg_dark)
                .stroke(Stroke::new(1.5, colors.border_col))
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
                colors.time_color,
            );
        });
}

fn show_tiny(ctx: &egui::Context, time_str: &str, colors: &ColorScheme) {
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(colors.bg_dark)
                .stroke(Stroke::new(1.0, colors.border_col))
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
                colors.time_color,
            );
        });
}
