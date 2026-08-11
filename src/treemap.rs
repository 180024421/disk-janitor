//! 基于 ScanIndex 的简易 Treemap（矩形占比）

use crate::model::{format_bytes, ScanIndex};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use std::path::PathBuf;

const PALETTE: &[Color32] = &[
    Color32::from_rgb(56, 180, 170),
    Color32::from_rgb(72, 140, 210),
    Color32::from_rgb(210, 140, 70),
    Color32::from_rgb(160, 110, 210),
    Color32::from_rgb(90, 190, 110),
    Color32::from_rgb(210, 90, 110),
    Color32::from_rgb(100, 160, 190),
    Color32::from_rgb(190, 170, 80),
];

/// 绘制当前目录子项 treemap；点击目录返回该路径
pub fn show_treemap(
    ui: &mut egui::Ui,
    index: &ScanIndex,
    dir: &PathBuf,
    desired_height: f32,
) -> Option<PathBuf> {
    let mut children: Vec<_> = index.children_of(dir);
    children.retain(|e| e.size > 0);
    children.sort_by(|a, b| b.size.cmp(&a.size));
    if children.is_empty() {
        ui.weak("当前目录无可视化占用（或子项为 0）");
        return None;
    }
    let total: u64 = children.iter().map(|e| e.size).sum::<u64>().max(1);
    let full = ui.available_width();
    let height = desired_height.clamp(160.0, 420.0);
    let (resp, painter) = ui.allocate_painter(Vec2::new(full, height), Sense::click());
    let rect = resp.rect;
    let mut click: Option<PathBuf> = None;

    let mut slices: Vec<(Rect, usize)> = Vec::new();
    squarify(
        &children.iter().map(|e| e.size).collect::<Vec<_>>(),
        rect,
        &mut slices,
    );

    for (i, (r, idx)) in slices.into_iter().enumerate() {
        let e = children[idx];
        let color = PALETTE[i % PALETTE.len()];
        painter.rect_filled(r, 3.0, color.gamma_multiply(0.85));
        painter.rect_stroke(
            r,
            3.0,
            egui::Stroke::new(1.0_f32, Color32::from_black_alpha(80)),
            egui::StrokeKind::Inside,
        );
        if r.width() > 48.0 && r.height() > 28.0 {
            let name = truncate(&e.name, 18);
            let label = format!("{}\n{}", name, format_bytes(e.size));
            painter.text(
                Pos2::new(r.left() + 6.0, r.top() + 6.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if r.contains(pos) && e.is_dir {
                    click = Some(e.path.clone());
                }
            }
        }
        let hover = resp.hover_pos().map(|p| r.contains(p)).unwrap_or(false);
        if hover {
            egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), egui::Id::new(("tm", i)), |ui| {
                ui.label(&e.name);
                ui.label(format_bytes(e.size));
                ui.weak(format!("{:.1}%", e.size as f64 * 100.0 / total as f64));
                if e.is_dir {
                    ui.weak("点击进入");
                }
            });
        }
    }
    click
}

fn truncate(s: &str, max: usize) -> String {
    let mut n = 0;
    let mut out = String::new();
    for ch in s.chars() {
        if n >= max {
            out.push('…');
            break;
        }
        out.push(ch);
        n += 1;
    }
    out
}

/// 简易横向/纵向交替切片（非完美 squarified，够用）
fn squarify(sizes: &[u64], rect: Rect, out: &mut Vec<(Rect, usize)>) {
    if sizes.is_empty() || rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let total: u64 = sizes.iter().sum::<u64>().max(1);
    let horizontal = rect.width() >= rect.height();
    let mut cursor = if horizontal { rect.left() } else { rect.top() };
    let end = if horizontal { rect.right() } else { rect.bottom() };
    let span = (end - cursor).max(1.0);

    for (i, &sz) in sizes.iter().enumerate() {
        let frac = sz as f32 / total as f32;
        let len = span * frac;
        let r = if horizontal {
            let next = if i + 1 == sizes.len() {
                end
            } else {
                (cursor + len).min(end)
            };
            Rect::from_min_max(
                Pos2::new(cursor, rect.top()),
                Pos2::new(next, rect.bottom()),
            )
        } else {
            let next = if i + 1 == sizes.len() {
                end
            } else {
                (cursor + len).min(end)
            };
            Rect::from_min_max(
                Pos2::new(rect.left(), cursor),
                Pos2::new(rect.right(), next),
            )
        };
        out.push((r, i));
        cursor = if horizontal { r.right() } else { r.bottom() };
    }
}
