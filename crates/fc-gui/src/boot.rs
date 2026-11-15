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

        // 底色 + 中间一团很淡的强调色光晕（呼应主界面的极光壁纸）
        p.rect_filled(area, egui::Rounding::ZERO, BG);
        let c = area.center();
        let glow_r = area.width().min(area.height()) * 0.62;
        // 用顶点渐变画，不是画一圈圈实心圆——叠加实心圆会出现明显的同心色阶
        const SEG: usize = 72;
        let mut mesh = egui::epaint::Mesh::default();
        mesh.vertices.push(egui::epaint::Vertex {
            pos: c,
            uv: egui::Pos2::ZERO,
            color: tint(accent(), 15),
        });
        for i in 0..=SEG {
            let ang = std::f32::consts::TAU * i as f32 / SEG as f32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: c + Vec2::new(ang.cos(), ang.sin()) * glow_r,
                uv: egui::Pos2::ZERO,
                color: Color32::TRANSPARENT,
            });
            if i > 0 {
                mesh.add_triangle(0, i as u32, i as u32 + 1);
            }
        }
        p.add(egui::Shape::mesh(mesh));

        // logo
        let logo = self.logo.get_or_insert_with(|| load_logo(ctx));
        let logo_rect = Rect::from_center_size(c - Vec2::new(0.0, 78.0), Vec2::splat(84.0));
        p.image(
            logo.id(),
            logo_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        // 名字 / 版本
        p.text(
            c - Vec2::new(0.0, 22.0),
            Align2::CENTER_CENTER,
            APP_NAME,
            FontId::proportional(23.0),
            TEXT,
        );
        p.text(
            c - Vec2::new(0.0, 0.0),
            Align2::CENTER_CENTER,
            format!("v{VERSION}"),
            FontId::proportional(11.0),
            tint(TEXT, 110),
        );

        // 进度条
        let bar_w = 240.0;
        let bar_h = 2.0;
        let bar = Rect::from_center_size(c + Vec2::new(0.0, 34.0), Vec2::new(bar_w, bar_h));
        p.rect_filled(bar, egui::Rounding::same(1.0), tint(Color32::WHITE, 20));
        let w = bar_w * self.progress().clamp(0.0, 1.0);
        if w > 0.5 {
            p.rect_filled(
                Rect::from_min_size(bar.min, Vec2::new(w, bar_h)),
                egui::Rounding::same(1.0),
                accent(),
            );
        }
        p.text(
            c + Vec2::new(0.0, 52.0),
            Align2::CENTER_CENTER,
            self.status(),
            FontId::proportional(11.0),
            tint(TEXT, 130),
        );

        // 底部小字
        p.text(
            egui::pos2(c.x, area.bottom() - 34.0),
            Align2::CENTER_CENTER,
            "建议所有操作备份 · 慎防设备损坏，请自己负责",
            FontId::proportional(10.0),
            tint(TEXT, 70),
        );
    }
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
