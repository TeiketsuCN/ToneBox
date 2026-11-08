//! 通用绘制、小控件、数值格式化

#![allow(unused_imports)]

use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use egui::{Align, Align2, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};
use std::path::{Path, PathBuf};
use crate::*;
use crate::badges::*;
use crate::titlebar::*;
use crate::rail::*;
use crate::queue::*;
use crate::player::*;
use crate::spectrum::*;
use crate::detail::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

/// HDR 模式（界面观感）：更亮的强调色 + 发光。只影响渲染，不改任何音频参数。
pub(crate) static HDR_ON: AtomicBool = AtomicBool::new(false);

pub(crate) fn tint(c: Color32, a: u8) -> Color32 {
    let k = a as u16;
    Color32::from_rgba_premultiplied(
        (c.r() as u16 * k / 255) as u8,
        (c.g() as u16 * k / 255) as u8,
        (c.b() as u16 * k / 255) as u8,
        a,
    )
}

pub(crate) fn line(c: Color32, a: u8) -> Stroke {
    Stroke::new(1.0, tint(c, a))
}

/// 两个预乘色按 t 混合
pub(crate) fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        f(a.r(), b.r()),
        f(a.g(), b.g()),
        f(a.b(), b.b()),
        f(a.a(), b.a()),
    )
}

/// 指数逼近：dt 无关的稳定缓动。返回是否还在动。
pub(crate) fn approach(cur: &mut f32, target: f32, dt: f32, speed: f32) -> bool {
    let k = (dt * speed).clamp(0.0, 1.0);
    *cur += (target - *cur) * k;
    if (target - *cur).abs() < 0.002 {
        *cur = target;
        false
    } else {
        true
    }
}

pub(crate) fn fmt_duration(s: f64) -> String {
    let total = s.round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

pub(crate) fn fmt_size(b: u64) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / 1048576.0)
    } else {
        format!("{} KB", b / 1024)
    }
}

/// 44100 → "44.1k"，48000 → "48k"，192000 → "192k"
pub(crate) fn fmt_rate(hz: u32) -> String {
    if hz % 1000 == 0 {
        format!("{}k", hz / 1000)
    } else {
        format!("{:.1}k", hz as f64 / 1000.0)
    }
}

/// 输出列里跟着控件走的小提示（这些提示原来是详情页底部的警告卡）
pub(crate) fn out_hint(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.add_space(3.0);
    para(ui, &format!("· {text}"), 8.5, color);
}

/// 单行裁剪绘制（不会换行把行高撑破）
pub(crate) fn paint_clipped(ui: &egui::Ui, rect: egui::Rect, size: f32, color: Color32, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let galley = ui.painter().layout(
        text.to_string(),
        egui::FontId::proportional(size),
        color,
        rect.width().max(1.0),
    );
    ui.painter().with_clip_rect(rect).galley(rect.min, galley, color);
}

/// 单行裁剪绘制 + 垂直居中（路径那类文字要居中对齐才不"歪"）
pub(crate) fn paint_clipped_v(ui: &egui::Ui, rect: egui::Rect, size: f32, color: Color32, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let galley = ui.painter().layout(
        text.to_string(),
        egui::FontId::proportional(size),
        color,
        rect.width().max(1.0),
    );
    let y = rect.center().y - galley.size().y / 2.0;
    ui.painter()
        .with_clip_rect(rect)
        .galley(egui::pos2(rect.left(), y), galley, color);
}

/// 统一风格的搜索框（输入框视觉在 App::new 里全局调过）
pub(crate) fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str, width: f32) -> bool {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(width)
            .margin(egui::vec2(10.0, 5.0)),
    )
    .changed()
}

/// 很小的文字按钮（导出/导入/＋）
pub(crate) fn tiny_text_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(label).size(9.0).color(MUTED))
            .fill(tint(Color32::WHITE, 8))
            .stroke(line(Color32::WHITE, 14))
            .rounding(Rounding::same(6.0)),
    )
    .clicked()
}

/// 音乐库顶部的小按钮（主按钮是青色实心）
pub(crate) fn lib_button(ui: &mut egui::Ui, label: &str, primary: bool) -> bool {
    let b = egui::Button::new(RichText::new(label).size(10.0).color(if primary {
        Color32::from_rgb(10, 26, 24)
    } else {
        MUTED
    }))
    .fill(if primary {
        ACCENT
    } else {
        tint(Color32::WHITE, 10)
    })
    .stroke(if primary {
        Stroke::NONE
    } else {
        line(Color32::WHITE, 18)
    })
    .rounding(Rounding::same(8.0))
    .min_size(egui::vec2(0.0, 24.0));
    if primary {
        let r = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(ui.spacing().interact_size.x.max(80.0), 24.0),
        );
        accent_glow(ui.painter(), r, Rounding::same(8.0), 50);
    }
    ui.add(b).clicked()
}

/// 在矩形外面套几层很淡的圆角光晕（HDR 模式下才画）
pub(crate) fn accent_glow(p: &egui::Painter, rect: egui::Rect, rounding: Rounding, strength: u8) {
    if !hdr() {
        return;
    }
    for k in 1..=4u8 {
        let grow = k as f32 * 1.7;
        let a = (strength as u16 / (k as u16 + 1)).max(2) as u8;
        p.rect_filled(rect.expand(grow), rounding, tint(ACCENT, a));
    }
}

/// 和整体风格一致的小滑杆：细轨道 + 小圆点 +（可选）刻度 + 右侧数值。
/// 压缩等级 / Vorbis 质量 / 背景暗化都用它，别用 egui 默认 Slider（那个跟界面完全不搭）。
pub(crate) fn bar_slider(ui: &mut egui::Ui, v: &mut i32, min: i32, max: i32, ticks: bool) -> bool {
    let mut changed = false;
    let w = (ui.available_width() - 22.0).max(80.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 18.0), egui::Sense::click_and_drag());
    let p = ui.painter();
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 5.0));
    p.rect_filled(track, Rounding::same(2.5), tint(Color32::WHITE, 14));
    let span = (max - min).max(1) as f32;
    let t = ((*v - min) as f32 / span).clamp(0.0, 1.0);
    if t > 0.0 {
        let mut f = track;
        f.set_width(track.width() * t);
        p.rect_filled(f, Rounding::same(2.5), tint(ACCENT, 220));
    }
    if ticks {
        // 档位少的时候画点，让人知道一共几档
        for i in min..=max {
            let x = track.left() + track.width() * ((i - min) as f32 / span);
            p.circle_filled(egui::pos2(x, track.center().y), 1.0, tint(Color32::WHITE, 34));
        }
    }
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(
        egui::pos2(track.left() + track.width() * t, track.center().y),
        if hot { 6.0 } else { 4.5 },
        if hot { TEXT } else { MUTED },
    );
    if (resp.clicked() || resp.dragged()) && rect.width() > 0.0 {
        if let Some(pp) = resp.interact_pointer_pos() {
            let t2 = ((pp.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let nv = (min as f32 + t2 * span).round() as i32;
            if nv != *v {
                *v = nv;
                changed = true;
            }
        }
    }
    ui.label(RichText::new(format!("{v}")).size(9.0).color(DIM));
    changed
}

/// 带标题的小滑杆（压缩等级 / 质量）
pub(crate) fn level_slider(ui: &mut egui::Ui, label: &str, v: &mut i32, min: i32, max: i32) -> bool {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(9.5).color(MUTED));
        bar_slider(ui, v, min, max, true)
    })
    .inner
}

/// 采样率条：档位离散（「自动」由外面的按钮管），点击/拖动吸附到档位；返回新档位。
pub(crate) fn rate_slider(ui: &mut egui::Ui, choices: &[u32], sel: Option<u32>, active: bool) -> Option<u32> {
    if choices.is_empty() {
        return None;
    }
    let w = (ui.available_width() - 46.0).max(60.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 18.0), egui::Sense::click_and_drag());
    let p = ui.painter();
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 5.0));
    p.rect_filled(track, Rounding::same(2.5), tint(Color32::WHITE, 14));
    let n = choices.len();
    let denom = (n.max(2) - 1) as f32;
    for i in 0..n {
        let x = track.left() + track.width() * (i as f32 / denom);
        p.circle_filled(egui::pos2(x, track.center().y), 1.0, tint(Color32::WHITE, 34));
    }
    // 自动状态下把点停在"自动实际会输出"的那档上，但画成暗色，和手动选中的亮色区分开
    if let Some(t) = sel
        .and_then(|v| choices.iter().position(|c| *c == v))
        .map(|i| i as f32 / denom)
    {
        let mut f = track;
        f.set_width(track.width() * t);
        p.rect_filled(
            f,
            Rounding::same(2.5),
            if active { tint(ACCENT, 200) } else { tint(Color32::WHITE, 40) },
        );
        p.circle_filled(
            egui::pos2(track.left() + track.width() * t, track.center().y),
            4.5,
            if active { ACCENT } else { MUTED },
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if (resp.clicked() || resp.dragged()) && rect.width() > 0.0 {
        if let Some(pp) = resp.interact_pointer_pos() {
            let t = ((pp.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let i = ((t * denom).round() as usize).min(n - 1);
            return Some(choices[i]);
        }
    }
    None
}

/// 开始转换按钮（放在第四列最下面——它是这一列的终点）。
///
/// 转换时它自己就是进度条：左边写"转换中 xx%"，右边一条细轨走进度，
/// 界面上不再另外放一条总进度。
pub(crate) fn start_button(ui: &mut egui::Ui, can: bool, running: bool, progress: f32) -> bool {
    let sense = if running {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), sense);
    let p = ui.painter();
    let rounding = Rounding::same(10.0);

    if running {
        p.rect_filled(rect, rounding, tint(ACCENT, 22));
        p.rect_stroke(rect, rounding, Stroke::new(1.0, tint(ACCENT, 70)));
        p.text(
            egui::pos2(rect.left() + 14.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("转换中 {:.0}%", progress.clamp(0.0, 1.0) * 100.0),
            egui::FontId::proportional(12.0),
            TEXT,
        );
        let bar_w = (rect.width() * 0.36).clamp(70.0, 120.0);
        let track = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 14.0 - bar_w, rect.center().y - 2.5),
            egui::vec2(bar_w, 5.0),
        );
        p.rect_filled(track, Rounding::same(2.5), tint(Color32::WHITE, 26));
        let mut fill = track;
        fill.set_width(track.width() * progress.clamp(0.0, 1.0));
        p.rect_filled(fill, Rounding::same(2.5), ACCENT);
        return false;
    }

    if can {
        accent_glow(p, rect, rounding, 70);
    }
    p.rect_filled(
        rect,
        rounding,
        if can {
            if resp.hovered() {
                blend(ACCENT, Color32::WHITE, 0.12)
            } else {
                ACCENT
            }
        } else {
            tint(Color32::WHITE, 10)
        },
    );
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "开始转换",
        egui::FontId::proportional(13.0),
        if can {
            Color32::from_rgb(10, 26, 24)
        } else {
            DIM
        },
    );
    resp.clicked()
}

/// 分段选择按钮（位深 / 码率这类互斥选项）
pub(crate) fn seg_button(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let b = egui::Button::new(RichText::new(label).size(10.0).color(if on {
        Color32::from_rgb(10, 26, 24)
    } else {
        MUTED
    }))
    .fill(if on { ACCENT } else { tint(Color32::WHITE, 8) })
    .stroke(if on { Stroke::NONE } else { line(Color32::WHITE, 18) })
    .rounding(Rounding::same(8.0))
    .min_size(egui::vec2(0.0, 24.0));
    ui.add(b).clicked()
}

/// 设置页的小节标题
pub(crate) fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(10.0).color(DIM));
    ui.add_space(6.0);
}

pub(crate) fn card(ui: &mut egui::Ui, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(tint(color, 16))
        .stroke(line(color, 50))
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(13.0))
        .show(ui, add);
}

pub(crate) fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(tint(color, 20))
        .stroke(line(color, 50))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(text).size(9.5).color(color))
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
}

/// 段落文字：永远 wrap，杜绝溢出
pub(crate) fn para(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    ui.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
}

pub(crate) fn row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.label(RichText::new(key).size(9.5).color(DIM));
    ui.add(
        egui::Label::new(RichText::new(value).size(10.0).color(TEXT))
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
    ui.end_row();
}

/// 可点击跳转的进度条
/// 和「删除源文件」同一套圆点：空心环 + 选中时实心点。
/// egui 自带的 radio 是另一套观感（小方块感的圆），不要混着用。
pub(crate) fn radio_dot(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 22.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let c = egui::pos2(rect.left() + 8.0, rect.center().y);
    p.circle_stroke(
        c,
        6.0,
        Stroke::new(
            1.2,
            if on {
                ACCENT
            } else if resp.hovered() {
                tint(Color32::WHITE, 110)
            } else {
                tint(Color32::WHITE, 70)
            },
        ),
    );
    if on {
        p.circle_filled(c, 3.3, ACCENT);
    }
    p.text(
        egui::pos2(c.x + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(10.0),
        if on { TEXT } else { MUTED },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

pub(crate) fn seek_bar(ui: &mut egui::Ui, pos: f64, dur: f64) -> Option<f64> {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 12.0), egui::Sense::click_and_drag());
    let frac = ((pos / dur.max(0.001)) as f32).clamp(0.0, 1.0);
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(6.0), tint(Color32::WHITE, 14));
    if frac > 0.0 {
        let mut fill = rect;
        fill.set_width(rect.width() * frac);
        p.rect_filled(fill, Rounding::same(6.0), ACCENT);
    }
    if resp.clicked() || resp.dragged() {
        if let Some(pp) = resp.interact_pointer_pos() {
            let f = ((pp.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            return Some((f as f64) * dur);
        }
    }
    None
}

/// 读一张收款码图片（放程序目录，用户自己换）。没有就返回 None。
pub(crate) fn load_qr(ctx: &egui::Context, name: &str) -> Option<egui::TextureHandle> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let path = dir.join(name);
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let color = egui::ColorImage {
        size: [w, h],
        pixels: rgba
            .pixels()
            .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect(),
    };
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

pub fn hdr() -> bool {
    HDR_ON.load(Ordering::Relaxed)
}

pub(crate) fn set_hdr(on: bool) {
    HDR_ON.store(on, Ordering::Relaxed);
}
