//! 界面主题：Web 管理后台风格（侧栏 + 内容区 + 表格）

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, Vec2, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(45, 168, 196);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(32, 128, 152);
pub const DANGER: Color32 = Color32::from_rgb(239, 104, 104);
pub const WARN: Color32 = Color32::from_rgb(234, 179, 8);
pub const OK: Color32 = Color32::from_rgb(74, 196, 140);
pub const MUTED: Color32 = Color32::from_rgb(148, 163, 184);
pub const TEXT: Color32 = Color32::from_rgb(241, 245, 249);
pub const BG: Color32 = Color32::from_rgb(15, 23, 42);
pub const PANEL: Color32 = Color32::from_rgb(30, 41, 59);
pub const PANEL2: Color32 = Color32::from_rgb(51, 65, 85);
pub const LINE: Color32 = Color32::from_rgb(51, 65, 85);
pub const SIDEBAR: Color32 = Color32::from_rgb(11, 17, 32);
pub const ROW_HOVER: Color32 = Color32::from_rgb(36, 48, 68);
pub const ROW_ALT: Color32 = Color32::from_rgb(22, 32, 48);

pub fn apply_theme(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.window_fill = PANEL;
    v.panel_fill = BG;
    v.extreme_bg_color = Color32::from_rgb(8, 12, 22);
    v.faint_bg_color = PANEL2;
    v.code_bg_color = Color32::from_rgb(15, 23, 42);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = DANGER;
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.bg_fill = PANEL2;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(40, 52, 72);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(226, 232, 240));
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.inactive.corner_radius = CornerRadius::same(6);
    v.widgets.hovered.bg_fill = Color32::from_rgb(56, 72, 96);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(56, 72, 96);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_DIM);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.hovered.corner_radius = CornerRadius::same(6);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(8, 16, 24));
    v.widgets.active.corner_radius = CornerRadius::same(6);
    v.widgets.open.bg_fill = PANEL2;
    v.widgets.open.corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(45, 168, 196, 55);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(90),
    };

    let mut style = (*ctx.style()).clone();
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.indent = 16.0;
    style.spacing.window_margin = Margin::same(12);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(13.5, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(12.5, egui::FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(11.5, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

pub fn content_frame() -> Frame {
    Frame::new()
        .fill(BG)
        .inner_margin(Margin::symmetric(20, 16))
}

pub fn top_bar_frame() -> Frame {
    Frame::new()
        .fill(Color32::from_rgb(17, 27, 46))
        .inner_margin(Margin::symmetric(16, 10))
        .stroke(Stroke::new(1.0_f32, LINE))
}

pub fn sidebar_frame() -> Frame {
    Frame::new()
        .fill(SIDEBAR)
        .inner_margin(Margin::symmetric(12, 14))
        .stroke(Stroke::new(1.0_f32, LINE))
}

pub fn status_bar_frame() -> Frame {
    Frame::new()
        .fill(Color32::from_rgb(17, 27, 46))
        .inner_margin(Margin::symmetric(16, 8))
        .stroke(Stroke::new(1.0_f32, LINE))
}

pub fn card_frame() -> Frame {
    Frame::new()
        .fill(PANEL)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .stroke(Stroke::new(1.0_f32, LINE))
}

pub fn table_frame() -> Frame {
    Frame::new()
        .fill(PANEL)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(0, 0))
        .stroke(Stroke::new(1.0_f32, LINE))
}

pub fn accent_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text.to_owned())
            .color(Color32::from_rgb(8, 18, 28))
            .strong()
            .size(13.0),
    )
    .fill(ACCENT)
    .corner_radius(CornerRadius::same(6))
    .min_size(Vec2::new(0.0, 30.0))
}

pub fn ghost_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(TEXT).size(13.0))
        .fill(PANEL2)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(0.0, 30.0))
}

pub fn danger_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text.to_owned())
            .color(Color32::WHITE)
            .strong()
            .size(13.0),
    )
    .fill(DANGER)
    .corner_radius(CornerRadius::same(6))
    .min_size(Vec2::new(0.0, 30.0))
}

pub fn link_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(ACCENT).size(12.0))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .min_size(Vec2::new(0.0, 22.0))
}

pub fn version_pill(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(45, 168, 196, 28))
        .stroke(Stroke::new(1.0_f32, ACCENT_DIM))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(ACCENT).size(10.5).strong());
        });
}

pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).color(TEXT).size(22.0).strong());
            if !subtitle.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(subtitle).color(MUTED).size(12.5));
            }
        });
    });
    ui.add_space(14.0);
}

pub fn section_title(ui: &mut Ui, title: &str, subtitle: &str) {
    page_header(ui, title, subtitle);
}

pub fn nav_group_label(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(
        RichText::new(text.to_ascii_uppercase())
            .color(Color32::from_rgb(100, 116, 139))
            .size(10.0)
            .strong(),
    );
    ui.add_space(4.0);
}

pub fn nav_item(ui: &mut Ui, selected: bool, icon: &str, text: &str) -> egui::Response {
    let (fill, fg) = if selected {
        (
            Color32::from_rgba_unmultiplied(45, 168, 196, 28),
            ACCENT,
        )
    } else {
        (Color32::TRANSPARENT, Color32::from_rgb(203, 213, 225))
    };

    let resp = ui.add(
        egui::Button::new(
            RichText::new(format!("  {icon}   {text}"))
                .color(fg)
                .size(13.0),
        )
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(8))
        .min_size(Vec2::new(ui.available_width(), 36.0)),
    );

    if selected {
        let r = resp.rect;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(r.left() + 1.0, r.top() + 6.0),
                Vec2::new(3.0, r.height() - 12.0),
            ),
            CornerRadius::same(2),
            ACCENT,
        );
    }
    resp
}

/// KPI 指标卡（Web 仪表盘风格）
pub fn metric_card(ui: &mut Ui, label: &str, value: &str, hint: &str, accent: Color32) {
    card_frame().show(ui, |ui| {
        ui.set_min_width(150.0);
        ui.set_width(ui.available_width().clamp(150.0, 260.0));
        ui.label(RichText::new(label).color(MUTED).size(12.0));
        ui.add_space(6.0);
        ui.label(RichText::new(value).color(accent).size(22.0).strong());
        if !hint.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(hint).color(MUTED).size(11.0));
        }
    });
}

pub fn metric_chip(ui: &mut Ui, label: &str, value: &str, hint: &str) {
    metric_card(ui, label, value, hint, ACCENT);
}

pub fn scan_progress(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).color(MUTED).size(11.5));
    ui.add(
        egui::ProgressBar::new(f32::NAN)
            .animate(true)
            .fill(ACCENT_DIM)
            .desired_height(4.0)
            .desired_width(ui.available_width()),
    );
}

pub fn hairline(ui: &mut Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, LINE.linear_multiply(0.7));
}

pub fn drive_usage_bar(ui: &mut Ui, ratio: f32, width: f32) {
    let ratio = ratio.clamp(0.0, 1.0);
    let color = if ratio >= 0.9 {
        DANGER
    } else if ratio >= 0.75 {
        WARN
    } else {
        ACCENT
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width.max(40.0), 6.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(40, 52, 72));
    if ratio > 0.0 {
        let fill = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * ratio, rect.height()));
        ui.painter().rect_filled(fill, CornerRadius::same(3), color);
    }
}

/// 列表占比条（相对同级最大项）
pub fn size_bar(ui: &mut Ui, ratio: f32, width: f32) {
    let ratio = ratio.clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width.max(24.0), 6.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(40, 52, 72));
    if ratio > 0.0 {
        let fill = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * ratio, rect.height()));
        ui.painter().rect_filled(
            fill,
            CornerRadius::same(3),
            Color32::from_rgba_unmultiplied(45, 168, 196, 200),
        );
    }
}

pub fn table_header_cell(ui: &mut Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 28.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                RichText::new(text)
                    .color(MUTED)
                    .size(11.5)
                    .strong(),
            );
        },
    );
}

pub fn empty_state(ui: &mut Ui, title: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(48.0);
        ui.label(RichText::new(title).color(TEXT).size(16.0).strong());
        ui.add_space(6.0);
        ui.label(RichText::new(hint).color(MUTED).size(13.0));
        ui.add_space(24.0);
    });
}
