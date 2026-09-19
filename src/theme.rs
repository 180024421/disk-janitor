//! 界面主题：清爽卡片风格（左侧导航 + 大标题 + 圆角卡片 + 蓝色主色）
//!
//! 参考“C盘清理助手”类产品：浅色底、白卡片、蓝色主按钮、大号数字与环形进度。
//! 深色模式保留同一套组件，只换配色。

use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Ui, Vec2, Visuals,
};
use std::sync::atomic::{AtomicBool, Ordering};

// ---------- 主色（蓝色系） ----------
pub const ACCENT: Color32 = Color32::from_rgb(37, 99, 235);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(29, 78, 216);
/// 选中态/浅色徽标底色
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(10, 28, 66, 14);
pub const DANGER: Color32 = Color32::from_rgb(239, 68, 68);
pub const WARN: Color32 = Color32::from_rgb(245, 158, 11);
pub const OK: Color32 = Color32::from_rgb(16, 185, 129);
pub const MUTED: Color32 = Color32::from_rgb(148, 163, 184);
pub const TEXT: Color32 = Color32::from_rgb(241, 245, 249);
pub const BG: Color32 = Color32::from_rgb(15, 20, 34);
pub const PANEL: Color32 = Color32::from_rgb(24, 32, 48);
pub const PANEL2: Color32 = Color32::from_rgb(38, 50, 72);
pub const LINE: Color32 = Color32::from_rgb(46, 58, 80);
pub const SIDEBAR: Color32 = Color32::from_rgb(12, 17, 30);
pub const ROW_HOVER: Color32 = Color32::from_rgb(30, 41, 62);
pub const ROW_ALT: Color32 = Color32::from_rgb(19, 26, 42);

// 浅色模式（默认外观，对齐参考图）
const L_BG: Color32 = Color32::from_rgb(244, 246, 250);
const L_PANEL: Color32 = Color32::from_rgb(255, 255, 255);
const L_TEXT: Color32 = Color32::from_rgb(17, 24, 39);
const L_MUTED: Color32 = Color32::from_rgb(107, 114, 128);
const L_LINE: Color32 = Color32::from_rgb(229, 231, 235);
const L_SIDEBAR: Color32 = Color32::from_rgb(255, 255, 255);
const L_HOVER: Color32 = Color32::from_rgb(243, 244, 246);

static DARK_MODE: AtomicBool = AtomicBool::new(true);

fn dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

pub fn apply_theme(ctx: &egui::Context, dark_mode: bool, compact: bool) {
    DARK_MODE.store(dark_mode, Ordering::Relaxed);
    let mut v = if dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = dark_mode;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = DANGER;
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_stroke = Stroke::new(1.0_f32, line());
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.widgets.open.corner_radius = CornerRadius::same(8);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(37, 99, 235, 45);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);

    if dark_mode {
        v.window_fill = PANEL;
        v.panel_fill = BG;
        v.extreme_bg_color = Color32::from_rgb(10, 14, 24);
        v.faint_bg_color = PANEL2;
        v.code_bg_color = Color32::from_rgb(15, 20, 34);
        v.override_text_color = Some(TEXT);
        v.widgets.noninteractive.bg_fill = PANEL;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        v.widgets.inactive.bg_fill = PANEL2;
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(32, 42, 62);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(226, 232, 240));
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
        v.widgets.hovered.bg_fill = Color32::from_rgb(48, 62, 88);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 62, 88);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        v.widgets.open.bg_fill = PANEL2;
    } else {
        v.panel_fill = L_BG;
        v.window_fill = L_PANEL;
        v.override_text_color = Some(L_TEXT);
        v.widgets.noninteractive.bg_fill = L_PANEL;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, L_TEXT);
        v.widgets.inactive.bg_fill = L_PANEL;
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(243, 244, 246);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(55, 65, 81));
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, L_LINE);
        v.widgets.hovered.bg_fill = Color32::from_rgb(243, 244, 246);
        v.widgets.hovered.weak_bg_fill = L_HOVER;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, L_TEXT);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        v.widgets.open.bg_fill = L_PANEL;
    }
    v.window_shadow = egui::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(if dark_mode { 96 } else { 28 }),
    };

    let mut style = (*ctx.style()).clone();
    style.visuals = v;
    style.spacing.item_spacing = if compact {
        egui::vec2(6.0, 4.0)
    } else {
        egui::vec2(10.0, 8.0)
    };
    style.spacing.button_padding = if compact {
        egui::vec2(10.0, 5.0)
    } else {
        egui::vec2(14.0, 8.0)
    };
    style.spacing.indent = 16.0;
    style.spacing.window_margin = Margin::same(14);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(26.0, egui::FontFamily::Proportional),
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

// ---------- 语义色 ----------
pub fn background() -> Color32 {
    if dark() {
        BG
    } else {
        L_BG
    }
}

pub fn panel() -> Color32 {
    if dark() {
        PANEL
    } else {
        L_PANEL
    }
}

pub fn foreground() -> Color32 {
    if dark() {
        TEXT
    } else {
        L_TEXT
    }
}

/// 次级说明文字
pub fn subtle() -> Color32 {
    if dark() {
        MUTED
    } else {
        L_MUTED
    }
}

pub fn line() -> Color32 {
    if dark() {
        LINE
    } else {
        L_LINE
    }
}

pub fn row_alt() -> Color32 {
    if dark() {
        ROW_ALT
    } else {
        Color32::from_rgb(249, 250, 251)
    }
}

/// 进度条/环形轨道的底色
pub fn track() -> Color32 {
    if dark() {
        Color32::from_rgb(44, 56, 78)
    } else {
        Color32::from_rgb(229, 231, 235)
    }
}

/// 选中态的淡色底
pub fn accent_soft() -> Color32 {
    if dark() {
        Color32::from_rgba_unmultiplied(37, 99, 235, 42)
    } else {
        Color32::from_rgb(239, 246, 255)
    }
}

/// 读取 Windows“应用使用浅色主题”偏好。读取失败时沿用深色外观。
pub fn system_prefers_dark() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        return hkcu
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
            .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
            .map(|value| value == 0)
            .unwrap_or(false);
    }
    #[cfg(not(windows))]
    {
        false
    }
}

// ---------- 框架 ----------
pub fn content_frame() -> Frame {
    Frame::new()
        .fill(background())
        .inner_margin(Margin::symmetric(24, 18))
}

pub fn top_bar_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .inner_margin(Margin::symmetric(18, 12))
        .stroke(Stroke::new(1.0_f32, line()))
}

pub fn sidebar_frame() -> Frame {
    Frame::new()
        .fill(if dark() {
            SIDEBAR
        } else {
            L_SIDEBAR
        })
        .inner_margin(Margin::symmetric(14, 16))
        .stroke(Stroke::new(1.0_f32, line()))
}

pub fn status_bar_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .inner_margin(Margin::symmetric(18, 8))
        .stroke(Stroke::new(1.0_f32, line()))
}

/// 白卡片：大圆角 + 极浅描边
pub fn card_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, line()))
}

pub fn table_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(0, 0))
        .stroke(Stroke::new(1.0_f32, line()))
}

// ---------- 按钮 ----------
pub fn accent_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text.to_owned())
            .color(Color32::WHITE)
            .strong()
            .size(13.5),
    )
    .fill(ACCENT)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(8))
    .min_size(Vec2::new(0.0, 32.0))
}

pub fn ghost_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(foreground()).size(13.0))
        .fill(if dark() { PANEL2 } else { Color32::WHITE })
        .stroke(Stroke::new(1.0_f32, line()))
        .corner_radius(CornerRadius::same(8))
        .min_size(Vec2::new(0.0, 32.0))
}

pub fn danger_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text.to_owned())
            .color(Color32::WHITE)
            .strong()
            .size(13.0),
    )
    .fill(DANGER)
    .corner_radius(CornerRadius::same(8))
    .min_size(Vec2::new(0.0, 32.0))
}

pub fn link_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(ACCENT).size(12.5))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .min_size(Vec2::new(0.0, 24.0))
}

pub fn version_pill(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(accent_soft())
        .stroke(Stroke::new(1.0_f32, line()))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(ACCENT).size(10.5).strong());
        });
}

/// 小徽标（如 NTFS / Admin / 有更新）
pub fn badge(ui: &mut Ui, text: &str, fg: Color32) {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 26))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(fg).size(10.5).strong());
        });
}

// ---------- 排版 ----------
pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).color(foreground()).size(24.0).strong());
    if !subtitle.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(subtitle).color(subtle()).size(13.5));
    }
    ui.add_space(16.0);
}

pub fn section_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).color(foreground()).size(16.0).strong());
    if !subtitle.is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new(subtitle).color(subtle()).size(12.5));
    }
    ui.add_space(10.0);
}

pub fn nav_group_label(ui: &mut Ui, text: &str) {
    ui.add_space(12.0);
    ui.label(
        RichText::new(text)
            .color(if dark() {
                Color32::from_rgb(100, 116, 139)
            } else {
                Color32::from_rgb(156, 163, 175)
            })
            .size(11.0)
            .strong(),
    );
    ui.add_space(4.0);
}

/// 单行导航项（旧样式，保留兼容）
pub fn nav_item(ui: &mut Ui, selected: bool, icon: &str, text: &str) -> egui::Response {
    nav_item_rich(ui, selected, icon, text, "")
}

/// 双行导航项：图标 + 标题 + 说明，选中时淡蓝底 + 左侧蓝条
pub fn nav_item_rich(
    ui: &mut Ui,
    selected: bool,
    icon: &str,
    title: &str,
    subtitle: &str,
) -> egui::Response {
    let height = if subtitle.is_empty() { 38.0 } else { 48.0 };
    let (fill, fg) = if selected {
        (accent_soft(), ACCENT)
    } else {
        (
            Color32::TRANSPARENT,
            if dark() {
                Color32::from_rgb(203, 213, 225)
            } else {
                Color32::from_rgb(55, 65, 81)
            },
        )
    };

    let resp = ui
        .allocate_ui_with_layout(
            Vec2::new(ui.available_width(), height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let rect = ui.max_rect();
                let hovered = ui.rect_contains_pointer(rect);
                let bg = if selected {
                    fill
                } else if hovered {
                    if dark() {
                        ROW_HOVER
                    } else {
                        L_HOVER
                    }
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
                if selected {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.left() + 1.0, rect.top() + 10.0),
                            Vec2::new(3.0, rect.height() - 20.0),
                        ),
                        CornerRadius::same(2),
                        ACCENT,
                    );
                }
                ui.add_space(12.0);
                ui.label(RichText::new(icon).size(15.0).color(fg));
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).color(fg).size(13.5).strong());
                    if !subtitle.is_empty() {
                        ui.add_space(1.0);
                        ui.label(RichText::new(subtitle).color(subtle()).size(11.0));
                    }
                });
                ui.add_space(8.0);
            },
        )
        .response;

    ui.interact(resp.rect, ui.id().with(("nav", title)), Sense::click())
}

// ---------- 数据展示 ----------
/// KPI 指标卡
pub fn metric_card(ui: &mut Ui, label: &str, value: &str, hint: &str, accent: Color32) {
    card_frame().show(ui, |ui| {
        ui.set_min_width(150.0);
        ui.set_width(ui.available_width().clamp(150.0, 280.0));
        ui.label(RichText::new(label).color(subtle()).size(12.0));
        ui.add_space(8.0);
        ui.label(RichText::new(value).color(accent).size(24.0).strong());
        if !hint.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(hint).color(subtle()).size(11.5));
        }
    });
}

pub fn metric_chip(ui: &mut Ui, label: &str, value: &str, hint: &str) {
    metric_card(ui, label, value, hint, ACCENT);
}

/// 环形进度：中间大号数字 + 下方小字说明
pub fn donut(
    ui: &mut Ui,
    ratio: f32,
    size: f32,
    center: &str,
    caption: &str,
    color: Color32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let ratio = ratio.clamp(0.0, 1.0);
    let radius = (size * 0.5 - 9.0).max(8.0);
    let center_pos = rect.center();
    let thickness = (size * 0.09).clamp(6.0, 11.0);
    let painter = ui.painter();
    painter.circle_stroke(center_pos, radius, Stroke::new(thickness, track()));
    if ratio > 0.0 {
        let steps = 64;
        let sweep = std::f32::consts::TAU * ratio;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + sweep * (i as f32 / steps as f32);
                center_pos + Vec2::new(a.cos(), a.sin()) * radius
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(thickness, color)));
    }
    painter.text(
        center_pos + Vec2::new(0.0, -6.0),
        Align2::CENTER_CENTER,
        center,
        FontId::proportional(19.0),
        foreground(),
    );
    if !caption.is_empty() {
        painter.text(
            center_pos + Vec2::new(0.0, 14.0),
            Align2::CENTER_CENTER,
            caption,
            FontId::proportional(10.5),
            subtle(),
        );
    }
    resp
}

/// 列表行：左图标 + 标题/说明 + 右侧数值。返回整行 Response（可点击）。
pub fn item_row(
    ui: &mut Ui,
    icon: &str,
    title: &str,
    subtitle: &str,
    value: &str,
    value_color: Color32,
) -> egui::Response {
    let width = ui.available_width();
    let resp = ui.allocate_ui_with_layout(
        Vec2::new(width, 44.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let rect = ui.max_rect();
            let hovered = ui.rect_contains_pointer(rect);
            if hovered {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    if dark() { ROW_HOVER } else { Color32::from_rgb(249, 250, 251) },
                );
            }
            ui.add_space(10.0);
            ui.label(RichText::new(icon).size(16.0));
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).color(foreground()).size(13.5).strong());
                if !subtitle.is_empty() {
                    ui.add_space(1.0);
                    ui.label(RichText::new(subtitle).color(subtle()).size(11.0));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(value).color(value_color).size(13.5).strong());
                ui.add_space(6.0);
                ui.label(RichText::new("›").color(subtle()).size(15.0));
            });
        },
    )
    .response;
    ui.interact(resp.rect, ui.id().with(("row", title)), Sense::click())
}

/// 图例小圆点 + 文本
pub fn legend(ui: &mut Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(2), color);
        ui.add_space(6.0);
        ui.label(RichText::new(text).color(subtle()).size(11.5));
    });
}

pub fn scan_progress(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).color(subtle()).size(11.5));
    ui.add(
        egui::ProgressBar::new(f32::NAN)
            .animate(true)
            .fill(ACCENT_DIM)
            .desired_height(5.0)
            .desired_width(ui.available_width()),
    );
}

pub fn hairline(ui: &mut Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, line());
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
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width.max(40.0), 8.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), track());
    if ratio > 0.0 {
        let fill =
            egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * ratio, rect.height()));
        ui.painter().rect_filled(fill, CornerRadius::same(4), color);
    }
}

/// 列表占比条（相对同级最大项）
pub fn size_bar(ui: &mut Ui, ratio: f32, width: f32) {
    let ratio = ratio.clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width.max(24.0), 6.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), track());
    if ratio > 0.0 {
        let fill =
            egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * ratio, rect.height()));
        ui.painter().rect_filled(
            fill,
            CornerRadius::same(3),
            Color32::from_rgba_unmultiplied(37, 99, 235, 210),
        );
    }
}

pub fn table_header_cell(ui: &mut Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 30.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(RichText::new(text).color(subtle()).size(11.5).strong());
        },
    );
}

pub fn empty_state(ui: &mut Ui, title: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(48.0);
        ui.label(RichText::new(title).color(foreground()).size(16.0).strong());
        ui.add_space(6.0);
        ui.label(RichText::new(hint).color(subtle()).size(13.0));
        ui.add_space(24.0);
    });
}
