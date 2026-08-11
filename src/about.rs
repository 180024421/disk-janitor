//! 关于 / 赞助 / 联系（对齐 DeskReader）

use eframe::egui::{self, ColorImage, RichText, TextureHandle};
use std::path::PathBuf;
use std::sync::OnceLock;

pub const AUTHOR_QQ: &str = "180024421";
pub const AUTHOR_EMAIL: &str = "180024421@qq.com";
pub const QQ_GROUP: &str = "914577057";
pub const QQ_GROUP_NAME: &str = "大帅阅读开源交流群";
pub const GITHUB_URL: &str = "https://github.com/180024421/disk-janitor";
pub const GITEE_URL: &str = "https://gitee.com/lidashuai123/disk-janitor";
pub const LICENSE_SUMMARY: &str =
    "本软件为完全免费的开源项目，仅供个人学习与非商业使用；禁止商用、禁止收费分发。";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AboutPanel {
    About,
    Sponsor,
    Contact,
}

pub struct AboutAssets {
    pub wechat: Option<TextureHandle>,
    pub alipay: Option<TextureHandle>,
    pub qq_group: Option<TextureHandle>,
}

impl AboutAssets {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            wechat: load_texture(ctx, "sponsor-wechat", "sponsor-wechat.png"),
            alipay: load_texture(ctx, "sponsor-alipay", "sponsor-alipay.png"),
            qq_group: load_texture(ctx, "qq-group-qr", "qq-group-qr.png"),
        }
    }
}

fn resource_candidates(name: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("resources").join(name));
            v.push(dir.join(name));
        }
    }
    v.push(PathBuf::from("resources").join(name));
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        v.push(PathBuf::from(manifest).join("resources").join(name));
    }
    v
}

fn embedded_png(name: &str) -> Option<&'static [u8]> {
    match name {
        "sponsor-wechat.png" => Some(include_bytes!("../resources/sponsor-wechat.png").as_slice()),
        "sponsor-alipay.png" => Some(include_bytes!("../resources/sponsor-alipay.png").as_slice()),
        "qq-group-qr.png" => Some(include_bytes!("../resources/qq-group-qr.png").as_slice()),
        _ => None,
    }
}

fn load_texture(ctx: &egui::Context, id: &str, file: &str) -> Option<TextureHandle> {
    let bytes = resource_candidates(file)
        .into_iter()
        .find_map(|p| std::fs::read(p).ok())
        .or_else(|| embedded_png(file).map(|b| b.to_vec()))?;
    let img = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(id, color, Default::default()))
}

static CLIP_OK: OnceLock<()> = OnceLock::new();

pub fn copy_text(text: &str) -> Result<(), String> {
    // Windows: clip.exe
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("cmd")
        .args(["/C", "clip"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        let _ = CLIP_OK.set(());
        Ok(())
    } else {
        Err("复制失败".into())
    }
}

pub fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn draw_about(
    ui: &mut egui::Ui,
    panel: &mut AboutPanel,
    assets: &AboutAssets,
    version: &str,
    version_code: u32,
    tip: &mut String,
) {
    ui.horizontal(|ui| {
        for (p, label) in [
            (AboutPanel::About, "关于"),
            (AboutPanel::Sponsor, "赞助"),
            (AboutPanel::Contact, "联系"),
        ] {
            let on = *panel == p;
            if ui.selectable_label(on, label).clicked() {
                *panel = p;
                tip.clear();
            }
        }
    });
    ui.add_space(8.0);
    if !tip.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(90, 200, 130), tip.as_str());
    }

    match *panel {
        AboutPanel::About => {
            ui.heading(RichText::new("大帅清理器（disk-janitor）").strong());
            ui.label(format!("版本 {version}  ·  #{version_code}"));
            ui.add_space(6.0);
            ui.label(LICENSE_SUMMARY);
            ui.label("· 完全免费开源，欢迎个人学习与非商业使用");
            ui.label("· 禁止出售、收费分发、捆绑或任何商业用途");
            ui.label("· 商业合作请先联系作者取得书面授权");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("GitHub").clicked() {
                    let _ = open_url(GITHUB_URL);
                }
                if ui.button("Gitee").clicked() {
                    let _ = open_url(GITEE_URL);
                }
                if ui.button("交流群").clicked() {
                    *panel = AboutPanel::Contact;
                }
                if ui.button("赞助").clicked() {
                    *panel = AboutPanel::Sponsor;
                }
            });
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.strong(format!("交流群 · {QQ_GROUP_NAME}"));
                ui.horizontal(|ui| {
                    ui.label("QQ群");
                    ui.monospace(QQ_GROUP);
                    if ui.button("复制").clicked() {
                        *tip = match copy_text(QQ_GROUP) {
                            Ok(()) => "已复制群号".into(),
                            Err(e) => e,
                        };
                    }
                });
            });
        }
        AboutPanel::Sponsor => {
            ui.heading(RichText::new("赞助支持").strong());
            ui.label("若本软件对你有帮助，可通过下方收款码自愿赞助。感谢支持！");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                qr_block(ui, "微信支付", assets.wechat.as_ref());
                ui.add_space(16.0);
                qr_block(ui, "支付宝", assets.alipay.as_ref());
            });
            if assets.wechat.is_none() && assets.alipay.is_none() {
                ui.weak("收款码加载失败，请联系作者获取。");
            }
        }
        AboutPanel::Contact => {
            ui.heading(RichText::new("联系与交流").strong());
            ui.group(|ui| {
                ui.strong(format!("交流群 · {QQ_GROUP_NAME}"));
                ui.label(format!("QQ群 {QQ_GROUP} · 扫码加入"));
                ui.horizontal(|ui| {
                    ui.monospace(QQ_GROUP);
                    if ui.button("复制群号").clicked() {
                        *tip = match copy_text(QQ_GROUP) {
                            Ok(()) => "已复制群号".into(),
                            Err(e) => e,
                        };
                    }
                });
                if let Some(tex) = &assets.qq_group {
                    ui.add(egui::Image::new(tex).max_width(220.0));
                }
            });
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.strong("作者");
                ui.horizontal(|ui| {
                    ui.label("QQ");
                    ui.monospace(AUTHOR_QQ);
                    if ui.button("复制").clicked() {
                        *tip = match copy_text(AUTHOR_QQ) {
                            Ok(()) => "已复制 QQ".into(),
                            Err(e) => e,
                        };
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("邮箱");
                    ui.monospace(AUTHOR_EMAIL);
                    if ui.button("复制").clicked() {
                        *tip = match copy_text(AUTHOR_EMAIL) {
                            Ok(()) => "已复制邮箱".into(),
                            Err(e) => e,
                        };
                    }
                });
            });
        }
    }
}

fn qr_block(ui: &mut egui::Ui, caption: &str, tex: Option<&TextureHandle>) {
    ui.vertical(|ui| {
        if let Some(t) = tex {
            ui.add(egui::Image::new(t).max_width(200.0));
        } else {
            ui.allocate_ui([200.0, 200.0].into(), |ui| {
                ui.centered_and_justified(|ui| ui.weak("无图"));
            });
        }
        ui.label(caption);
    });
}
