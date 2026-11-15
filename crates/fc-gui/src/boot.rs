//! 启动过场：窗口先出来，慢活儿丢后台，加载完再切主界面。
//!
//! 起因：`App::new` 里原来同步跑 `fc_core::locate()` 和 `list_encoders()`，
//! 后者要起一次 `ffmpeg -encoders` 子进程（冷启动 300~800ms）。
//! eframe 的窗口是**先**建好的，所以这段时间用户面对的是一个空壳窗口。
//! 现在第一步挪到后台线程，第一帧就画过场，最短停留 650ms 再切主界面。
//!
//! 过场是画在 `Order::Foreground` 层上的，不影响下面任何面板的布局。

use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Align2, Color32, FontId, Rect, Vec2};

use crate::util::{accent, tint};
use crate::{APP_NAME, BG, TEXT, VERSION};

pub(crate) struct BootData {
    pub tools: Option<fc_core::Tools>,
    pub encoders: HashSet<String>,
}

pub(crate) struct Boot {
    rx: Receiver<BootData>,
    started: Instant,
    data: Option<BootData>,
    logo: Option<egui::TextureHandle>,
    /// 过场底图（照片）
    shot: Option<egui::TextureHandle>,
}

/// 过场停留时长。
///
/// 真实启动只要 300~500ms（探测 ffmpeg/ffprobe + 取编码器表），这里多留一会儿是
/// **故意的**：太快切过去像闪屏，也不符合"一个程序被打开了"的心理预期。
/// 想调手感就改这一个数：800ms 利落，1800ms 有仪式感，3000ms 以上开始像卡住。
const SPLASH: Duration = Duration::from_millis(1800);
/// 兜底：后台如果卡住（ffmpeg 挂起之类），到点先放行，不能把窗口钉死在过场。
const TIMEOUT: Duration = Duration::from_secs(6);

impl Boot {
    /// 起后台线程去摸 ffmpeg / ffprobe 和编码器表。
    pub fn start() -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let tools = fc_core::locate();
            let encoders = tools
                .as_ref()
                .map(|t| fc_core::list_encoders(&t.ffmpeg))
                .unwrap_or_default();
            // 接收端已经没了（窗口刚开就被关掉）也无所谓，静默丢掉
            let _ = tx.send(BootData { tools, encoders });
        });
        Self {
            rx,
            started: Instant::now(),
            data: None,
            logo: None,
            shot: None,
        }
    }

    /// 每帧调一次；`Some` 表示可以进主界面了。
    pub fn poll(&mut self) -> Option<BootData> {
        if self.data.is_none() {
            if let Ok(d) = self.rx.try_recv() {
                self.data = Some(d);
            }
        }
        // 结果到了也要走满停留时间；结果没到就一直等（但受 TIMEOUT 保护）
        if self.data.is_some() && self.started.elapsed() >= SPLASH {
            return self.data.take();
        }
        if self.started.elapsed() >= TIMEOUT {
            return Some(BootData {
                tools: None,
                encoders: HashSet::new(),
            });
        }
        None
    }

    /// 时间轴进度：真进度拿不到（`ffmpeg -encoders` 是一次性子进程，没有中间量），
    /// 就按停留时长匀速走；结果还没到手时最多到 92%，避免"100% 了还在等"。
    fn progress(&self) -> f32 {
        let f = (self.started.elapsed().as_secs_f32() / SPLASH.as_secs_f32()).min(1.0);
        if self.data.is_some() {
            f
        } else {
            f * 0.92
        }
    }

    fn status(&self) -> &'static str {
        let f = (self.started.elapsed().as_secs_f32() / SPLASH.as_secs_f32()).min(1.0);
        if self.data.is_none() {
            return if f < 0.45 {
                "正在检测 ffmpeg 与 ffprobe…"
            } else {
                "仍在检测 ffmpeg…"
            };
        }
        match f {
            x if x < 0.45 => "正在检测 ffmpeg 与 ffprobe…",
            x if x < 0.80 => "正在载入编码器表…",
            _ => "正在准备界面…",
        }
    }

    /// 画过场。调用方负责每帧 `request_repaint`。
    pub fn paint(&mut self, ctx: &egui::Context) {
        let area = ctx.screen_rect();
        let p = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("boot-splash"),
        ));

        // ── 底图：铺满整个窗口（cover），不留黑边 ──
        p.rect_filled(area, egui::Rounding::ZERO, BG);
        let shot = self.shot.get_or_insert_with(|| load_shot(ctx));
        let [sw, sh] = shot.size();
        let ts = Vec2::new(sw as f32, sh as f32);
        let k = (area.width() / ts.x).max(area.height() / ts.y);
        p.image(
            shot.id(),
            Rect::from_center_size(area.center(), ts * k),
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        // 左边压暗（标题要读得清），底部再压一道（许可小字）
        hgrad(&p, area, tint(BG, 178), tint(BG, 0));
        vgrad(&p, area, tint(BG, 0), tint(BG, 150));

        let margin = 46.0;

        // ── 左上角：logo + 名字 ──
        let logo = self.logo.get_or_insert_with(|| load_logo(ctx));
        let logo_rect = Rect::from_min_size(
            area.min + Vec2::new(margin, margin - 6.0),
            Vec2::splat(40.0),
        );
        p.image(
            logo.id(),
            logo_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        p.text(
            egui::pos2(logo_rect.right() + 13.0, logo_rect.center().y),
            Align2::LEFT_CENTER,
            APP_NAME,
            FontId::proportional(18.0),
            tint(TEXT, 225),
        );

        // ── 大标题压在照片左侧（达芬奇那种排法）──
        let ty = area.top() + area.height() * 0.52;
        p.text(
            egui::pos2(area.left() + margin, ty),
            Align2::LEFT_CENTER,
            APP_NAME,
            FontId::proportional(46.0),
            TEXT,
        );
        p.text(
            egui::pos2(area.left() + margin + 2.0, ty + 38.0),
            Align2::LEFT_CENTER,
            format!("FORMAT WORKBENCH · v{VERSION}"),
            FontId::proportional(12.0),
            tint(TEXT, 150),
        );

        // ── 进度条 + 状态：跟着标题往下排 ──
        let bar_w = 250.0;
        let bar = Rect::from_min_size(
            egui::pos2(area.left() + margin + 2.0, ty + 68.0),
            Vec2::new(bar_w, 2.0),
        );
        p.rect_filled(bar, egui::Rounding::same(1.0), tint(Color32::WHITE, 28));
        let w = bar_w * self.progress().clamp(0.0, 1.0);
        if w > 0.5 {
            p.rect_filled(
                Rect::from_min_size(bar.min, Vec2::new(w, 2.0)),
                egui::Rounding::same(1.0),
                accent(),
            );
        }
        p.text(
            egui::pos2(bar.left(), bar.bottom() + 11.0),
            Align2::LEFT_TOP,
            self.status(),
            FontId::proportional(11.0),
            tint(TEXT, 140),
        );

        // ── 左下角：免责 + 许可（达芬奇左下那块小字的位置）──
        p.text(
            egui::pos2(area.left() + margin, area.bottom() - margin - 9.0),
            Align2::LEFT_BOTTOM,
            "建议所有操作备份 · 慎防设备损坏，请自己负责",
            FontId::proportional(9.5),
            tint(TEXT, 80),
        );
        p.text(
            egui::pos2(area.left() + margin, area.bottom() - margin + 8.0),
            Align2::LEFT_BOTTOM,
            "© 2026 ToneBox · MIT License · 随包 FFmpeg 为 LGPL 构建",
            FontId::proportional(9.5),
            tint(TEXT, 100),
        );
    }
}

/// 水平渐变：左 → 右。
fn hgrad(p: &egui::Painter, r: Rect, left: Color32, right: Color32) {
    let mut m = egui::epaint::Mesh::default();
    for (pos, color) in [
        (r.left_top(), left),
        (r.right_top(), right),
        (r.right_bottom(), right),
        (r.left_bottom(), left),
    ] {
        m.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::Pos2::ZERO,
            color,
        });
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(m));
}

/// 垂直渐变：上 → 下。
fn vgrad(p: &egui::Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = egui::epaint::Mesh::default();
    for (pos, color) in [
        (r.left_top(), top),
        (r.right_top(), top),
        (r.right_bottom(), bottom),
        (r.left_bottom(), bottom),
    ] {
        m.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::Pos2::ZERO,
            color,
        });
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(m));
}

/// 过场底图。要换照片就替换 `assets/splash.jpg`，代码一个字都不用动。
///
/// 现在的图是程序生成的占位（暗房 + 双屏波形 + 人形剪影）：DeepSeek 这边
/// 拉不到 Unsplash/Pexels 那类素材站，而且把人家的照片塞进发行版还有肖像权
/// 问题。正式图最好用自己的照片，或者明确写明授权来源。
fn load_shot(ctx: &egui::Context) -> egui::TextureHandle {
    const JPG: &[u8] = include_bytes!("../assets/splash.jpg");
    let img = image::load_from_memory(JPG)
        .expect("内置 splash.jpg 读不出来")
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    ctx.load_texture("boot-shot", ci, egui::TextureOptions::LINEAR)
}
fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    const PNG: &[u8] = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(PNG)
        .expect("内置 icon.png 读不出来")
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    ctx.load_texture("boot-logo", ci, egui::TextureOptions::LINEAR)
}
