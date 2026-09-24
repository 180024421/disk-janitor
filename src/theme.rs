//! 界面主题：清爽卡片风格（左侧导航 + 大标题 + 圆角卡片 + 蓝色主色）
//!
//! 参考“C盘清理助手”类产品：浅色底、白卡片、蓝色主按钮、大号数字与环形进度。
//! 深色模式保留同一套组件，只换配色。

use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Ui, Vec2, Visuals,
};
use std::sync::atomic::{AtomicBool, Ordering};

// ---------- 色板 ----------
// 两套数值都按 WCAG 逐一实测过：正文对底色 ≥4.5:1，描边与图形 ≥3:1。
// 语义色只留一档，填充和文字共用，亮色取深、暗色取亮，避免同色两用顾此失彼。
const D_BG: Color32 = Color32::from_rgb(32, 32, 32);
const D_SURFACE: Color32 = Color32::from_rgb(43, 43, 43);
const D_RAISED: Color32 = Color32::from_rgb(55, 55, 55);
const D_TEXT: Color32 = Color32::from_rgb(255, 255, 255);
const D_SECONDARY: Color32 = Color32::from_rgb(207, 207, 207);
const D_SIDEBAR: Color32 = Color32::from_rgb(25, 25, 25);
const D_ROW_ALT: Color32 = Color32::from_rgb(37, 37, 37);
const D_ROW_HOVER: Color32 = Color32::from_rgb(53, 53, 53);
const D_DIVIDER: Color32 = Color32::from_rgb(53, 53, 53);
const D_BORDER: Color32 = Color32::from_rgb(138, 138, 138);
const D_TRACK: Color32 = Color32::from_rgb(62, 62, 62);
const D_INPUT: Color32 = Color32::from_rgb(26, 26, 26);
const D_ACCENT: Color32 = Color32::from_rgb(76, 194, 255);
const D_ACCENT_HOVER: Color32 = Color32::from_rgb(104, 205, 255);
const D_ACCENT_PRESSED: Color32 = Color32::from_rgb(58, 175, 228);
const D_ON_ACCENT: Color32 = Color32::from_rgb(10, 10, 10);
const D_ACCENT_SOFT: Color32 = Color32::from_rgb(35, 52, 64);
const D_DANGER: Color32 = Color32::from_rgb(255, 153, 164);
const D_DANGER_HOVER: Color32 = Color32::from_rgb(255, 176, 184);
const D_DANGER_PRESSED: Color32 = Color32::from_rgb(226, 132, 143);
const D_OK: Color32 = Color32::from_rgb(108, 203, 95);
const D_WARN: Color32 = Color32::from_rgb(250, 223, 145);

const L_BG: Color32 = Color32::from_rgb(243, 243, 243);
const L_SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
const L_RAISED: Color32 = Color32::from_rgb(252, 252, 252);
const L_TEXT: Color32 = Color32::from_rgb(27, 27, 27);
const L_SECONDARY: Color32 = Color32::from_rgb(97, 97, 97);
const L_SIDEBAR: Color32 = Color32::from_rgb(250, 250, 250);
const L_ROW_ALT: Color32 = Color32::from_rgb(249, 249, 249);
const L_ROW_HOVER: Color32 = Color32::from_rgb(245, 245, 245);
const L_DIVIDER: Color32 = Color32::from_rgb(229, 229, 229);
const L_BORDER: Color32 = Color32::from_rgb(138, 138, 138);
const L_TRACK: Color32 = Color32::from_rgb(229, 229, 229);
const L_INPUT: Color32 = Color32::from_rgb(255, 255, 255);
const L_ACCENT: Color32 = Color32::from_rgb(15, 108, 189);
const L_ACCENT_HOVER: Color32 = Color32::from_rgb(28, 121, 201);
const L_ACCENT_PRESSED: Color32 = Color32::from_rgb(12, 90, 156);
const L_ON_ACCENT: Color32 = Color32::from_rgb(255, 255, 255);
const L_ACCENT_SOFT: Color32 = Color32::from_rgb(239, 246, 252);
const L_DANGER: Color32 = Color32::from_rgb(196, 43, 28);
const L_DANGER_HOVER: Color32 = Color32::from_rgb(212, 56, 40);
const L_DANGER_PRESSED: Color32 = Color32::from_rgb(167, 34, 21);
const L_OK: Color32 = Color32::from_rgb(14, 112, 14);
const L_WARN: Color32 = Color32::from_rgb(138, 90, 0);

static DARK_MODE: AtomicBool = AtomicBool::new(true);

fn dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

macro_rules! dual {
    ($name:ident, $dark:expr, $light:expr) => {
        pub fn $name() -> Color32 {
            if dark() {
                $dark
            } else {
                $light
            }
        }
    };
}

dual!(background, D_BG, L_BG);
dual!(panel, D_SURFACE, L_SURFACE);
dual!(raised, D_RAISED, L_RAISED);
dual!(foreground, D_TEXT, L_TEXT);
// 次级说明文字
dual!(subtle, D_SECONDARY, L_SECONDARY);
dual!(sidebar, D_SIDEBAR, L_SIDEBAR);
dual!(row_alt, D_ROW_ALT, L_ROW_ALT);
dual!(row_hover, D_ROW_HOVER, L_ROW_HOVER);
// 分隔线（纯装饰，不计对比度）
dual!(line, D_DIVIDER, L_DIVIDER);
// 控件描边（输入框、次按钮），需 ≥3:1
dual!(border, D_BORDER, L_BORDER);
// 进度条/环形轨道底色
dual!(track, D_TRACK, L_TRACK);
// 输入框底色
dual!(input_fill, D_INPUT, L_INPUT);
dual!(accent, D_ACCENT, L_ACCENT);
dual!(accent_hover, D_ACCENT_HOVER, L_ACCENT_HOVER);
dual!(accent_pressed, D_ACCENT_PRESSED, L_ACCENT_PRESSED);
// 主色底上的文字：暗色用近黑，亮色用白
dual!(on_accent, D_ON_ACCENT, L_ON_ACCENT);
// 选中态淡底
dual!(accent_soft, D_ACCENT_SOFT, L_ACCENT_SOFT);
dual!(danger, D_DANGER, L_DANGER);
dual!(danger_hover, D_DANGER_HOVER, L_DANGER_HOVER);
dual!(danger_pressed, D_DANGER_PRESSED, L_DANGER_PRESSED);
// 危险色底上的文字
dual!(on_danger, D_ON_ACCENT, L_ON_ACCENT);
dual!(ok, D_OK, L_OK);
dual!(warn, D_WARN, L_WARN);

pub fn apply_theme(ctx: &egui::Context, dark_mode: bool, compact: bool) {
    DARK_MODE.store(dark_mode, Ordering::Relaxed);
    let mut v = if dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = dark_mode;
    v.hyperlink_color = accent();
    v.warn_fg_color = warn();
    v.error_fg_color = danger();
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0_f32, border());
    v.widgets.inactive.corner_radius = CornerRadius::same(6);
    v.widgets.hovered.corner_radius = CornerRadius::same(6);
    v.widgets.active.corner_radius = CornerRadius::same(6);
    v.widgets.open.corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = tint(accent(), if dark_mode { 76 } else { 38 });
    v.selection.stroke = Stroke::new(1.0_f32, accent());

    // 控件三态：rest → hover 提亮 → pressed 压暗，跟随主色
    v.widgets.inactive.bg_fill = raised();
    v.widgets.inactive.weak_bg_fill = panel();
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, foreground());
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border());
    v.widgets.hovered.bg_fill = accent_hover();
    v.widgets.hovered.weak_bg_fill = if dark_mode {
        D_RAISED
    } else {
        Color32::from_rgb(249, 249, 249)
    };
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, foreground());
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent());
    v.widgets.active.bg_fill = accent_pressed();
    v.widgets.active.weak_bg_fill = accent_pressed();
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, on_accent());
    v.widgets.open.bg_fill = raised();

    v.window_fill = panel();
    v.panel_fill = background();
    v.extreme_bg_color = input_fill();
    v.faint_bg_color = row_alt();
    v.code_bg_color = row_alt();
    v.override_text_color = Some(foreground());
    v.widgets.noninteractive.bg_fill = panel();
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, foreground());
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, line());

    v.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if dark_mode { 112 } else { 28 }),
    };

    let mut style = (*ctx.style()).clone();
    style.visuals = v;
    style.spacing.item_spacing = if compact {
        egui::vec2(6.0, 4.0)
    } else {
        egui::vec2(8.0, 8.0)
    };
    style.spacing.button_padding = if compact {
        egui::vec2(9.0, 4.0)
    } else {
        egui::vec2(12.0, 6.0)
    };
    style.spacing.indent = 16.0;
    style.spacing.window_margin = Margin::same(14);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(24.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.5, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

// ---------- 派生外观 ----------
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
/// 把任意语义色压成淡底（徽标/提示条背景），两种模式共用同一入口
pub fn tint(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// 页面统一容器：所有 CentralPanel 内容页走这一个边距
pub fn page_frame() -> Frame {
    Frame::new()
        .fill(background())
        .inner_margin(Margin::same(16))
}

pub fn top_bar_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .inner_margin(Margin::symmetric(18, 12))
        .stroke(Stroke::new(1.0_f32, line()))
}

pub fn sidebar_frame() -> Frame {
    Frame::new()
        .fill(sidebar())
        .inner_margin(Margin::symmetric(12, 16))
        .stroke(Stroke::new(1.0_f32, line()))
}

pub fn status_bar_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .inner_margin(Margin::symmetric(18, 8))
        .stroke(Stroke::new(1.0_f32, line()))
}

/// 白卡片：圆角 8 + 极浅描边
pub fn card_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.0_f32, line()))
}

pub fn table_frame() -> Frame {
    Frame::new()
        .fill(panel())
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(0, 0))
        .stroke(Stroke::new(1.0_f32, line()))
}

// ---------- 弹窗 ----------
/// 模态确认框：不可折叠、居中。保留可缩放，长路径预览不能被裁掉。
pub fn dialog(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
}

// ---------- 按钮 ----------
// egui 的 Button::fill 会覆盖 widgets.hovered/active，按下和悬停态就丢了；
// 这里自己分配矩形绘制，换来 Fluent 的 rest/hover/pressed/disabled 四态 + 焦点环。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Accent,
    Neutral,
    Danger,
    Link,
}

fn kind_visuals(kind: Kind, hovered: bool, pressed: bool) -> (Color32, Color32, Stroke) {
    match kind {
        Kind::Accent => {
            let bg = if pressed {
                accent_pressed()
            } else if hovered {
                accent_hover()
            } else {
                accent()
            };
            (bg, on_accent(), Stroke::NONE)
        }
        Kind::Danger => {
            let bg = if pressed {
                danger_pressed()
            } else if hovered {
                danger_hover()
            } else {
                danger()
            };
            (bg, on_danger(), Stroke::NONE)
        }
        Kind::Neutral => {
            let bg = if pressed {
                if dark() {
                    Color32::from_rgb(34, 34, 34)
                } else {
                    Color32::from_rgb(240, 240, 240)
                }
            } else if hovered {
                raised()
            } else {
                panel()
            };
            (bg, foreground(), Stroke::new(1.0_f32, border()))
        }
        Kind::Link => {
            let fg = if pressed {
                accent_pressed()
            } else if hovered {
                accent_hover()
            } else {
                accent()
            };
            (Color32::TRANSPARENT, fg, Stroke::NONE)
        }
    }
}

fn button(ui: &mut Ui, text: &str, kind: Kind, disabled: bool, min: Vec2) -> egui::Response {
    let font_id = FontId::proportional(if kind == Kind::Link { 13.0 } else { 13.5 });
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_id, Color32::PLACEHOLDER);
    let pad = if kind == Kind::Link {
        Vec2::new(4.0, 3.0)
    } else {
        Vec2::new(12.0, 6.0)
    };
    let min_h = if kind == Kind::Link { 24.0 } else { 32.0 };
    let radius = CornerRadius::same(if kind == Kind::Link { 4 } else { 6 });
    let mut desired = Vec2::new(
        galley.size().x + 2.0 * pad.x,
        (galley.size().y + 2.0 * pad.y).max(min_h),
    );
    if kind != Kind::Link {
        desired.x = desired.x.max(56.0);
    }
    desired = desired.max(min);
    let enabled = ui.is_enabled() && !disabled;
    let (rect, resp) = ui.allocate_at_least(
        desired,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text)
    });

    if ui.is_rect_visible(rect) {
        let pressed = resp.is_pointer_button_down_on() || resp.clicked();
        let (mut bg, mut fg, mut stroke) = kind_visuals(kind, resp.hovered(), pressed);
        if !enabled {
            let (flat, dim) = if dark() {
                (Color32::from_rgb(40, 40, 40), Color32::from_rgb(122, 122, 122))
            } else {
                (Color32::from_rgb(248, 248, 248), Color32::from_rgb(168, 168, 168))
            };
            bg = if kind == Kind::Link {
                Color32::TRANSPARENT
            } else {
                flat
            };
            fg = dim;
            stroke = if kind == Kind::Neutral {
                Stroke::new(1.0_f32, line())
            } else {
                Stroke::NONE
            };
        }
        if bg != Color32::TRANSPARENT || stroke.width > 0.0 {
            ui.painter().rect(
                rect,
                radius,
                bg,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        ui.painter().galley_with_override_text_color(
            rect.center() - galley.size() * 0.5,
            galley,
            fg,
        );
        if resp.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(1.0),
                CornerRadius::same(7),
                Stroke::new(1.5_f32, background()),
                egui::StrokeKind::Inside,
            );
            ui.painter().rect_stroke(
                rect.expand(2.5),
                CornerRadius::same(8),
                Stroke::new(1.5_f32, foreground()),
                egui::StrokeKind::Inside,
            );
        }
    }
    resp
}

pub fn accent_button(ui: &mut Ui, text: &str) -> egui::Response {
    button(ui, text, Kind::Accent, false, Vec2::ZERO)
}

pub fn accent_button_if(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    button(ui, text, Kind::Accent, !enabled, Vec2::ZERO)
}

pub fn accent_button_sized(ui: &mut Ui, text: &str, w: f32, h: f32) -> egui::Response {
    button(ui, text, Kind::Accent, false, Vec2::new(w, h))
}

pub fn ghost_button(ui: &mut Ui, text: &str) -> egui::Response {
    button(ui, text, Kind::Neutral, false, Vec2::ZERO)
}

pub fn ghost_button_if(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    button(ui, text, Kind::Neutral, !enabled, Vec2::ZERO)
}

pub fn danger_button(ui: &mut Ui, text: &str) -> egui::Response {
    button(ui, text, Kind::Danger, false, Vec2::ZERO)
}

pub fn danger_button_if(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    button(ui, text, Kind::Danger, !enabled, Vec2::ZERO)
}

pub fn link_button(ui: &mut Ui, text: &str) -> egui::Response {
    button(ui, text, Kind::Link, false, Vec2::ZERO)
}

pub fn link_button_if(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    button(ui, text, Kind::Link, !enabled, Vec2::ZERO)
}

pub fn version_pill(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(accent_soft())
        .stroke(Stroke::new(1.0_f32, line()))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(accent()).size(11.5).strong());
        });
}

/// 小徽标（如 NTFS / Admin / 有更新）
pub fn badge(ui: &mut Ui, text: &str, fg: Color32) {
    Frame::new()
        .fill(tint(fg, if dark() { 38 } else { 26 }))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(fg).size(11.5).strong());
        });
}

// ---------- 导航与容器组件 ----------
/// 段选（Fluent Pivot）：同一页内切换子视图，选中项下方一条主色横线。
pub fn pivot(ui: &mut Ui, items: &[&str], current: &mut usize) {
    let height = 32.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, item) in items.iter().enumerate() {
            let selected = i == *current;
            let (rect, resp) = ui.allocate_at_least(
                Vec2::new(
                    ui.painter()
                        .layout_no_wrap(
                            item.to_string(),
                            FontId::proportional(13.5),
                            Color32::PLACEHOLDER,
                        )
                        .size()
                        .x
                        + 24.0,
                    height,
                ),
                Sense::click(),
            );
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, item.to_string())
            });
            if ui.is_rect_visible(rect) {
                if resp.hovered() && !selected {
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(4), row_hover());
                }
                let fg = if selected { accent() } else { foreground() };
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    item.to_string(),
                    FontId::proportional(13.5),
                    fg,
                );
                if selected {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.left() + 8.0, rect.bottom() - 2.0),
                            Vec2::new(rect.width() - 16.0, 2.0),
                        ),
                        CornerRadius::same(1),
                        accent(),
                    );
                }
            }
            if resp.clicked() {
                *current = i;
            }
        }
    });
    ui.add_space(12.0);
    hairline(ui);
    ui.add_space(12.0);
}

/// 单选段控件：主题、密度、扫描模式这类互斥设置项共用一套外观。
/// 返回本次点击的下标；未点击返回 None。
pub fn choice(ui: &mut Ui, items: &[&str], selected: usize) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (i, item) in items.iter().enumerate() {
            let on = i == selected;
            let galley = ui.painter().layout_no_wrap(
                item.to_string(),
                FontId::proportional(13.0),
                Color32::PLACEHOLDER,
            );
            let (rect, resp) = ui.allocate_at_least(
                Vec2::new(galley.size().x + 20.0, 28.0),
                Sense::click(),
            );
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, item.to_string())
            });
            if ui.is_rect_visible(rect) {
                let bg = if on {
                    accent_soft()
                } else if resp.hovered() {
                    row_hover()
                } else {
                    Color32::TRANSPARENT
                };
                let stroke = if on {
                    Stroke::new(1.0_f32, accent())
                } else {
                    Stroke::new(1.0_f32, border())
                };
                ui.painter().rect(
                    rect,
                    CornerRadius::same(4),
                    bg,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                ui.painter().galley_with_override_text_color(
                    rect.center() - galley.size() * 0.5,
                    galley,
                    if on { accent() } else { foreground() },
                );
            }
            if resp.clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// 页底动作栏：左侧永远是选择与容量摘要，右侧永远是按钮组（主操作最右）。
/// 六个页面各写一套按钮位置，是用户肌肉记忆建立不起来的根因。
pub fn action_bar<R>(ui: &mut Ui, summary: &str, buttons: impl FnOnce(&mut Ui) -> R) -> R {
    let row = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 40.0));
    let text_row = Rect::from_min_size(
        Pos2::new(row.left(), row.center().y - 8.0),
        Vec2::new(row.width() * 0.5, 16.0),
    );
    ui.allocate_ui_at_rect(text_row, |ui| {
        ui.label(RichText::new(summary).color(subtle()).size(12.5));
    });
    ui.allocate_ui_at_rect(row, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons)
            .inner
    })
    .inner
}

/// 语义色淡底提示条（授权提示、风险说明等）
pub fn notice(ui: &mut Ui, color: Color32, text: &str) {
    Frame::new()
        .fill(tint(color, 22))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(12.5));
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




/// 双行导航项：图标 + 标题 + 说明，选中时淡蓝底 + 左侧蓝条
pub fn nav_item_rich(
    ui: &mut Ui,
    selected: bool,
    icon: &str,
    title: &str,
    subtitle: &str,
) -> egui::Response {
    let height = if subtitle.is_empty() { 40.0 } else { 48.0 };
    let (fill, fg) = if selected {
        (accent_soft(), accent())
    } else {
        (Color32::TRANSPARENT, foreground())
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
                    row_hover()
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
                if selected {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.left() + 1.0, rect.center().y - 8.0),
                            Vec2::new(3.0, 16.0),
                        ),
                        CornerRadius::same(2),
                        accent(),
                    );
                }
                ui.add_space(12.0);
                ui.label(RichText::new(icon).size(15.0).color(fg));
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).color(fg).size(13.5).strong());
                    if !subtitle.is_empty() {
                        ui.add_space(1.0);
                        ui.label(RichText::new(subtitle).color(subtle()).size(11.5));
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
        ui.label(
            RichText::new(value).color(accent).size(28.0).strong().monospace(),
        );
        if !hint.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(hint).color(subtle()).size(11.5));
        }
    });
}

pub fn metric_chip(ui: &mut Ui, label: &str, value: &str, hint: &str) {
    metric_card(ui, label, value, hint, accent());
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
        FontId::monospace(19.0),
        foreground(),
    );
    if !caption.is_empty() {
        painter.text(
            center_pos + Vec2::new(0.0, 14.0),
            Align2::CENTER_CENTER,
            caption,
            FontId::proportional(11.5),
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
        Vec2::new(width, 48.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let rect = ui.max_rect();
            let hovered = ui.rect_contains_pointer(rect);
            if hovered {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), row_hover());
            }
            ui.add_space(10.0);
            ui.label(RichText::new(icon).size(16.0));
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).color(foreground()).size(13.5).strong());
                if !subtitle.is_empty() {
                    ui.add_space(1.0);
                    ui.label(RichText::new(subtitle).color(subtle()).size(11.5));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(value).color(value_color).size(13.5).strong().monospace());
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
            .fill(accent())
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
        danger()
    } else if ratio >= 0.75 {
        warn()
    } else {
        accent()
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
            tint(accent(), 210),
        );
    }
}

pub fn table_header_cell(ui: &mut Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 32.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(RichText::new(text).color(subtle()).size(12.5).strong());
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
