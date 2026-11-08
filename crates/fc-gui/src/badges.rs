//! 音质档位与徽章（Hi-Res / CD / DSD / 有损）

#![allow(unused_imports)]

use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use egui::{Align, Align2, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};
use std::path::{Path, PathBuf};
use crate::*;
use crate::util::*;
use crate::titlebar::*;
use crate::rail::*;
use crate::queue::*;
use crate::player::*;
use crate::spectrum::*;
use crate::detail::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

/// 音质档位：列表/详情里的小标识
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quality {
    /// DSD 的采样率档位（64 / 128 / 256 / 512 …），0 = 认不出来
    Dsd(u32),
    HiRes,
    Cd,
    Hq,
    Std,
    Lq,
}

/// 扫描出来的一首歌（只放界面要用的字段）
/// 编码名 -> 是否有损（老缓存没有 lossy 字段时用）
pub(crate) fn label_is_lossy(label: &str) -> bool {
    matches!(
        label,
        "MP3" | "AAC" | "Vorbis" | "Opus" | "WMA" | "AC-3" | "E-AC-3" | "MP2" | "ATRAC1"
            | "ATRAC3" | "ATRAC3+" | "Musepack" | "Speex"
    )
}

/// 从"有没有损 + 采样率 + 位深 + 码率"判档（音乐库的缓存里只有这些字段）
pub(crate) fn quality_from(lossy: bool, sr: u32, bits: Option<u32>, kbps: u64) -> Option<Quality> {
    if sr == 0 {
        return None;
    }
    if !lossy {
        let bits = bits.unwrap_or(16);
        return Some(if sr > 44100 || bits > 16 {
            Quality::HiRes
        } else {
            Quality::Cd
        });
    }
    Some(if kbps == 0 {
        // 有损但拿不到音轨码率（比如 MKV 里的 AAC）：别硬猜档位，"有损"就够了
        Quality::Std
    } else if kbps >= 192 {
        Quality::Hq
    } else if kbps >= 96 {
        Quality::Std
    } else {
        Quality::Lq
    })
}

pub(crate) fn quality_of(m: &fc_core::MediaInfo) -> Option<Quality> {
    if m.codec.is_empty() || m.sample_rate == 0 {
        return None;
    }
    if m.codec.starts_with("dsd") {
        return Some(Quality::Dsd(fc_core::dsd_tier(
            &m.codec,
            m.sample_rate,
            m.bitrate_kbps,
        )));
    }
    quality_from(m.is_lossy(), m.sample_rate, m.bits, m.bitrate_kbps)
}

/// 徽章上写什么：无损的格式把"无损"写出来，有损的给码率档
pub(crate) fn quality_label(q: Quality, kbps: u64) -> String {
    match q {
        Quality::Dsd(rate) => {
            if rate > 0 {
                format!("DSD{rate}")
            } else {
                "DSD".to_string()
            }
        }
        Quality::HiRes => "Hi-Res 无损".to_string(),
        Quality::Cd => "CD 无损".to_string(),
        Quality::Hq | Quality::Std | Quality::Lq => {
            if kbps > 0 {
                format!("有损 {kbps}k")
            } else {
                "有损".to_string()
            }
        }
    }
}

/// 徽章宽度按实际文字量出来：中英文混排、多两个字"无损"，写死宽度会溢出或者留一大块白
pub(crate) fn quality_width(ctx: &egui::Context, q: Quality, kbps: u64) -> f32 {
    let text = quality_label(q, kbps);
    let w = ctx.fonts(|f| {
        f.layout_no_wrap(text, egui::FontId::proportional(8.5), Color32::WHITE)
            .size()
            .x
    });
    match q {
        // DSD 左边有一段码流点串，CD 左边有个小光盘，都要额外留位
        Quality::Dsd(_) => 41.0 + w,
        Quality::Cd => 25.0 + w,
        _ => (w + 14.0).max(30.0),
    }
}

/// 圆角矩形在某个 y 处的左右内缩量。
/// 色带要按这个收边，否则方角色块会从圆角描边外面露出来。
pub(crate) fn corner_inset(rect: egui::Rect, radius: f32, y: f32) -> f32 {
    let r = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);
    let d = (y - rect.top()).min(rect.bottom() - y).max(0.0);
    if d >= r {
        0.0
    } else {
        r - (r * r - (r - d) * (r - d)).sqrt()
    }
}

/// 一条水平色带裁进圆角矩形后的轮廓（左边界 → 底边 → 右边界 → 顶边）。
/// 只给 Hi-Res 的渐变色带用：`rect_filled` 是方的，圆角得自己收。
pub(crate) fn rounded_band(rect: egui::Rect, radius: f32, y0: f32, y1: f32) -> Vec<egui::Pos2> {
    let y0 = y0.max(rect.top());
    let y1 = y1.min(rect.bottom());
    let n = 5;
    let mut pts = Vec::with_capacity(2 * n + 2);
    for i in 0..=n {
        let y = y0 + (y1 - y0) * i as f32 / n as f32;
        pts.push(egui::pos2(rect.left() + corner_inset(rect, radius, y), y));
    }
    for i in (0..=n).rev() {
        let y = y0 + (y1 - y0) * i as f32 / n as f32;
        pts.push(egui::pos2(rect.right() - corner_inset(rect, radius, y), y));
    }
    pts
}

/// 一条斜向高光带裁进圆角矩形：整枚徽章都能扫过，又不会从圆角外面冒出白角。
/// 高光带是斜的（上边比下边靠右 slant），所以每行单独和圆角轮廓求交。
pub(crate) fn clipped_glint(
    rect: egui::Rect,
    radius: f32,
    x0: f32,
    w: f32,
    slant: f32,
) -> Vec<egui::Pos2> {
    let n = 16;
    let mut left = Vec::with_capacity(n + 1);
    let mut right = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let y = rect.top() + rect.height() * f;
        let gx = x0 - slant * f;
        let lo = rect.left() + corner_inset(rect, radius, y);
        let hi = rect.right() - corner_inset(rect, radius, y);
        let l = gx.max(lo);
        let r = (gx + w).min(hi);
        if r - l > 0.05 {
            left.push(egui::pos2(l, y));
            right.push(egui::pos2(r, y));
        }
    }
    if left.len() < 2 {
        return Vec::new();
    }
    let mut pts = left;
    pts.extend(right.into_iter().rev());
    pts
}

/// 在给定矩形里画标识。返回 true = 有动画（调用方负责请求重绘）。
pub(crate) fn paint_quality(p: &egui::Painter, rect: egui::Rect, q: Quality, kbps: u64, t: f64) -> bool {
    let r = Rounding::same(8.5);
    let label = quality_label(q, kbps);
    let label = label.as_str();
    match q {
        Quality::HiRes => {
            // 金属金：细切的纵向渐变 + 上沿高光/下沿暗边做倒角 + 慢扫光
            let pulse = 0.5 + 0.5 * (t * 1.7).sin() as f32;
            let gold = Color32::from_rgb(247, 199, 108);
            let gold_hi = Color32::from_rgb(255, 238, 186);
            let deep = Color32::from_rgb(92, 62, 18);
            p.rect_filled(rect, r, deep);
            let clip = p.with_clip_rect(rect);
            let steps = 14;
            for i in 0..steps {
                let f = i as f32 / (steps - 1) as f32;
                let y0 = rect.top() + i as f32 / steps as f32 * rect.height();
                let y1 = rect.top() + (i + 1) as f32 / steps as f32 * rect.height() + 0.9;
                // 色带也要贴着圆角收边，不然四角会露出方角
                clip.add(egui::Shape::convex_polygon(
                    rounded_band(rect, 8.5, y0, y1),
                    blend(deep, gold, 0.22 + f * 0.86),
                    Stroke::NONE,
                ));
            }
            // 上沿一条细高光、下沿一条暗边：两道倒角叠起来才像金属片
            clip.add(egui::Shape::convex_polygon(
                rounded_band(rect, 8.5, rect.top(), rect.top() + 1.3),
                tint(gold_hi, 90),
                Stroke::NONE,
            ));
            clip.add(egui::Shape::convex_polygon(
                rounded_band(rect, 8.5, rect.bottom() - 1.2, rect.bottom()),
                tint(deep, 110),
                Stroke::NONE,
            ));
            // 高光从徽章左端扫到右端；每一行都和圆角求交，扫过圆角时自动收窄，
            // 所以整条都能扫到，又不会在圆角外面露出白角
            let phase = ((t * 0.4) % 1.0) as f32;
            let slant = 7.0;
            let bx = rect.left() - 16.0 + phase * (rect.width() + 16.0);
            for (i, w) in [16.0f32, 10.0, 5.0].iter().enumerate() {
                let pts = clipped_glint(rect, 8.5, bx, *w, slant);
                if pts.len() < 3 {
                    continue;
                }
                let a = (14.0 + 10.0 * pulse) as u8 / (i as u8 + 1);
                clip.add(egui::Shape::convex_polygon(
                    pts,
                    tint(Color32::WHITE, a.max(4)),
                    Stroke::NONE,
                ));
            }
            p.rect_stroke(
                rect,
                r,
                Stroke::new(1.0, tint(gold_hi, (150.0 + 80.0 * pulse) as u8)),
            );
            p.text(
                rect.center() + egui::vec2(0.0, -0.2),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(8.5),
                Color32::from_rgb(38, 24, 4),
            );
            true
        }
        Quality::Dsd(_) => {
            // 深紫底 + 一条流动的"1-bit 码流"（点串），右边写档位（DSD64 / DSD128…）
            let violet = Color32::from_rgb(186, 160, 255);
            p.rect_filled(rect, r, Color32::from_rgb(30, 22, 54));
            p.rect_stroke(rect, r, Stroke::new(1.0, tint(violet, 140)));
            let font = egui::FontId::proportional(8.5);
            let label_w = p.layout_no_wrap(label.to_string(), font.clone(), violet).size().x;
            let dots_l = rect.left() + 7.0;
            let dots_r = rect.right() - 6.5 - label_w - 5.0;
            let span = (dots_r - dots_l).max(6.0);
            let clip = p.with_clip_rect(rect.shrink(1.5));
            let n = 12;
            for i in 0..n {
                let f = i as f32 / (n - 1) as f32;
                let phase = t as f32 * 3.2 + f * 9.0;
                let y = rect.center().y + phase.sin() * 4.0;
                let x = dots_l + f * span;
                let a = (110.0 + 120.0 * phase.sin().abs()) as u8;
                clip.circle_filled(egui::pos2(x, y), 1.15, tint(violet, a));
            }
            p.text(
                egui::pos2(rect.right() - 6.5, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                label,
                font,
                violet,
            );
            true
        }
        Quality::Cd => {
            // 小 CD：银底 + 中心孔 + 转动的反光条
            p.rect_filled(rect, r, tint(Color32::WHITE, 18));
            p.rect_stroke(rect, r, Stroke::new(1.0, tint(Color32::WHITE, 80)));
            let disc = egui::pos2(rect.left() + 9.5, rect.center().y);
            p.circle_filled(disc, 5.6, Color32::from_rgb(214, 223, 234));
            p.circle_filled(disc, 1.7, BG);
            let ang = (t * 2.2) as f32;
            let tip = disc + egui::vec2(ang.cos() * 4.3, ang.sin() * 4.3);
            p.line_segment([disc, tip], Stroke::new(1.5, tint(Color32::WHITE, 200)));
            p.text(
                egui::pos2(rect.left() + 19.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(8.5),
                MUTED,
            );
            true
        }
        Quality::Hq | Quality::Std | Quality::Lq => {
            let c = match q {
                Quality::Hq => ACCENT,
                Quality::Std => MUTED,
                _ => DIM,
            };
            p.rect_filled(rect, r, tint(c, 16));
            p.rect_stroke(rect, r, Stroke::new(1.0, tint(c, 60)));
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(8.5),
                c,
            );
            false
        }
    }
}

/// 布局里放一个标识（会占位），返回 true 表示这一帧有动画
pub(crate) fn quality_badge(ui: &mut egui::Ui, q: Quality, kbps: u64) -> bool {
    let w = quality_width(ui.ctx(), q, kbps);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 17.0), egui::Sense::hover());
    let t = ui.input(|i| i.time);
    let anim = paint_quality(ui.painter(), rect, q, kbps, t);
    if anim {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(45));
    }
    anim
}

/// 是不是 DSD（ffprobe 的 codec 名是 dsd_lsbf / dsd_msbf / *_planar）
pub(crate) fn is_dsd(codec: &str) -> bool {
    codec.starts_with("dsd")
}

/// 采样率写法：44100 -> "44.1 kHz"，48000 -> "48 kHz"
pub(crate) fn fmt_khz(sr: u32) -> String {
    if sr % 1000 == 0 {
        format!("{} kHz", sr / 1000)
    } else {
        format!("{:.1} kHz", sr as f64 / 1000.0)
    }
}

/// DSD 的采样率写法：ffprobe 报的是"字节率"（DSD64 = 352.8k），×8 才是真正的 1-bit 频率。
/// 用 MHz 写（2.8224 MHz），别跟 PCM 的 kHz 混在一起。
pub(crate) fn fmt_mhz(hz: f64) -> String {
    let s = format!("{:.4}", hz / 1_000_000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s} MHz")
}

pub(crate) fn fmt_dsd_mhz(sr: u32) -> String {
    fmt_mhz(fc_core::dsd_bit_rate_hz(sr))
}

/// 队列里更短的 DSD 采样率写法：2.8M
pub(crate) fn fmt_dsd_m(sr: u32) -> String {
    format!("{:.1}M", fc_core::dsd_bit_rate_hz(sr) / 1_000_000.0)
}

/// DSD 的档位标签（DSD64 / DSD128…），认不出来就空
pub(crate) fn dsd_tier_label(codec: &str, sr: u32, kbps: u64) -> String {
    if !is_dsd(codec) {
        return String::new();
    }
    let rate = fc_core::dsd_tier(codec, sr, kbps);
    if rate > 0 {
        format!("DSD{rate}")
    } else {
        String::new()
    }
}

/// 更短的采样率写法：44100 -> "44.1k"，给列表里省地方
pub(crate) fn fmt_k(sr: u32) -> String {
    if sr % 1000 == 0 {
        format!("{}k", sr / 1000)
    } else {
        format!("{:.1}k", sr as f64 / 1000.0)
    }
}
