//! 界面主题：深空岩灰 + 青绿点缀（偏「产品感」）

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Vec2, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(64, 220, 196);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(38, 160, 142);
pub const DANGER: Color32 = Color32::from_rgb(235, 98, 92);
pub const WARN: Color32 = Color32::from_rgb(230, 176, 72);
pub const OK: Color32 = Color32::from_rgb(96, 205, 140);
pub const MUTED: Color32 = Color32::from_rgb(132, 148, 168);
pub const TEXT: Color32 = Color32::from_rgb(232, 238, 246);
pub const BG: Color32 = Color32::from_rgb(10, 13, 20);
pub const PANEL: Color32 = Color32::from_rgb(18, 23, 32);
pub const PANEL2: Color32 = Color32::from_rgb(26, 33, 46);
pub const LINE: Color32 = Color32::from_rgb(48, 60, 80);

pub fn apply_theme(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.window_fill = PANEL;
    v.panel_fill = BG;
    v.extreme_bg_color = Color32::from_rgb(6, 8, 12);
    v.faint_bg_color = PANEL2;
    v.code_bg_color = Color32::from_rgb(14, 18, 28);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = DANGER;
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.bg_fill = PANEL2;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(22, 28, 40);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(210, 220, 235));
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.hovered.bg_fill = Color32::from_rgb(40, 52, 70);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 52, 70);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_DIM);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(8, 16, 20));
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.widgets.open.bg_fill = PANEL2;
    v.widgets.open.corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(64, 220, 196, 70);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.window_shadow = egui::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };

    let mut style = (*ctx.style()).clone();
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
    style.spacing.indent = 18.0;
    style.spacing.window_margin = Margin::same(14);
    ctx.set_style(style);
}

pub fn top_bar_frame() -> Frame {
    Frame::new()
        .fill(PANEL)
        .inner_margin(Margin::symmetric(18, 14))
        .stroke(Stroke::new(1.0_f32, LINE))
}

pub fn card_frame() -> Frame {
    Frame::new()
        .fill(PANEL)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, LINE))
        .shadow(egui::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(60),
        })
}

pub fn accent_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(Color32::from_rgb(6, 14, 18)).strong())
        .fill(ACCENT)
        .corner_radius(CornerRadius::same(8))
        .min_size(Vec2::new(0.0, 30.0))
}

pub fn ghost_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(TEXT))
        .fill(PANEL2)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(CornerRadius::same(8))
        .min_size(Vec2::new(0.0, 30.0))
}

pub fn version_pill(ui: &mut egui::Ui, text: &str) {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(64, 220, 196, 28))
        .stroke(Stroke::new(1.0_f32, ACCENT_DIM))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(ACCENT).size(12.0).strong());
        });
}

pub fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).color(TEXT).size(20.0).strong());
    if !subtitle.is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new(subtitle).color(MUTED).size(13.0));
    }
    ui.add_space(10.0);
}

pub fn tab_label(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let (fill, fg, stroke) = if selected {
        (
            Color32::from_rgba_unmultiplied(64, 220, 196, 36),
            ACCENT,
            Stroke::new(1.0_f32, ACCENT_DIM),
        )
    } else {
        (Color32::TRANSPARENT, MUTED, Stroke::NONE)
    };
    ui.add(
        egui::Button::new(RichText::new(text.to_owned()).color(fg).size(13.0))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

/// 细分割线
pub fn hairline(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, LINE.linear_multiply(0.55));
}
