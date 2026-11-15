//! 启动过场：一个小窗口居中弹出，边检查边推进，检查完再进主界面。
//!
//! 为什么要这么多检查项：ffmpeg 是关键依赖，用户机器上的构建跟开发机可能不一样
//! （常见的是 LGPL 共享版，没有 `libspeex`）。与其等用户第一次转换才失败，
//! 不如开机把这些问清楚，顺便让这 2 秒有内容可看。
//!
//! 检查在后台线程里跑（`ffmpeg -encoders` / `-decoders` / `-version` 各是一次子进程），
//! 主线程只管按收到的进度重绘，所以窗口不会卡住。

use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Align2, Color32, FontId, Rect, Stroke, Vec2};

use crate::util::{accent, tint};
use crate::{APP_NAME, BG, TEXT, VERSION};

/// 过场窗口尺寸（逻辑点）。小窗口先看一眼，检查完再铺满屏幕。
pub(crate) const SPLASH_W: f32 = 760.0;
pub(crate) const SPLASH_H: f32 = 440.0;

pub(crate) struct BootData {
    pub tools: Option<fc_core::Tools>,
    pub encoders: HashSet<String>,
    /// ffmpeg 版本串（设置页要显示）
    pub version: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Todo,
    Running,
    Ok,
    /// 这项没装，但不影响使用（比如 atracdenc 是可选的）
    Skip,
    Warn,
}

struct Item {
    label: &'static str,
    state: Step,
    detail: String,
}

enum Msg {
    Step(usize, Step, String),
    Done(BootData),
}

/// 检查项。加一项就在这里加一行，线程里对应加一段。
const LABELS: [&str; 12] = [
    "定位 ffmpeg / ffprobe",
    "读取 ffmpeg 版本",
    "读取编码器表",
    "读取解码器表",
    "检查开源解码库",
    "检查无损编码器",
    "检查有损编码器",
    "检查视频编码器",
    "检查音频输出设备",
    "检查 ATRAC 编码器",
    "检查 Speex 编码器",
    "读取配置与音乐库",
];

pub(crate) struct Boot {
    rx: Receiver<Msg>,
    items: Vec<Item>,
    data: Option<BootData>,
    started: Instant,
    /// 检查全部完成、进入「正在加载项目」的时刻
    data_at: Option<Instant>,
    logo: Option<egui::TextureHandle>,
    shot: Option<egui::TextureHandle>,
}

/// 「正在加载项目」停留时长。达芬奇也在这句上停一会儿，切过去太突兀。
const PROJECT_HOLD: Duration = Duration::from_millis(2000);
/// 兜底：检查卡死（ffmpeg 挂起之类）也别把窗口钉在过场。
const TIMEOUT: Duration = Duration::from_secs(8);

impl Boot {
    pub fn start() -> Self {
        let (tx, rx) = channel();
        let txc = tx.clone();
        std::thread::spawn(move || {
            let send = move |i: usize, s: Step, d: String| {
                let _ = txc.send(Msg::Step(i, s, d));
                // 每项开跑前停一下：一行一行往下走才看得清，
                // 否则 7 项会在几十毫秒里闪过去
                if s == Step::Running {
                    std::thread::sleep(Duration::from_millis(200));
                }
            };

            // 0 定位
            send(0, Step::Running, String::new());
            let tools = fc_core::locate();
            match &tools {
                Some(t) => send(
                    0,
                    Step::Ok,
                    t.ffmpeg
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "已在 PATH".to_string()),
                ),
                None => send(0, Step::Warn, "没找到，转换不可用".to_string()),
            }

            // 1 版本
            send(1, Step::Running, String::new());
            let ver = tools
                .as_ref()
                .and_then(|t| fc_core::ffmpeg_version(&t.ffmpeg));
            match &ver {
                Some(v) => send(1, Step::Ok, v.clone()),
                None => send(1, Step::Skip, "读不到".to_string()),
            }

            // 2 编码器表
            send(2, Step::Running, String::new());
            let encoders = tools
                .as_ref()
                .map(|t| fc_core::list_encoders(&t.ffmpeg))
                .unwrap_or_default();
            send(
                2,
                if encoders.is_empty() {
                    Step::Warn
                } else {
                    Step::Ok
                },
                format!("{} 个", encoders.len()),
            );

            // 3 解码器表
            send(3, Step::Running, String::new());
            let decoders = tools
                .as_ref()
                .map(|t| fc_core::list_decoders(&t.ffmpeg))
                .unwrap_or_default();
            send(
                3,
                if decoders.is_empty() {
                    Step::Warn
                } else {
                    Step::Ok
                },
                format!("{} 个", decoders.len()),
            );

            // 4 开源解码库：ffmpeg 里名字带 lib 前缀的都是外挂开源库
            send(4, Step::Running, String::new());
            let mut libs: Vec<String> = decoders
                .iter()
                .filter(|n| n.starts_with("lib"))
                .map(|n| n.trim_start_matches("lib").to_string())
                .collect();
            libs.sort();
            libs.dedup();
            let detail = if libs.is_empty() {
                "没发现外挂解码库".to_string()
            } else {
                format!(
                    "{} … 共 {} 个",
                    libs.iter().take(4).cloned().collect::<Vec<_>>().join(" / "),
                    libs.len()
                )
            };
            send(
                4,
                if libs.is_empty() {
                    Step::Skip
                } else {
                    Step::Ok
                },
                detail,
            );

            // 5 无损编码器：这三种齐了，界面上的"转无损"才算真有路
            send(5, Step::Running, String::new());
            let lossless = ["flac", "alac", "pcm_s24le"];
            let miss: Vec<&str> = lossless
                .iter()
                .copied()
                .filter(|n| !encoders.contains(*n))
                .collect();
            send(
                5,
                if miss.is_empty() {
                    Step::Ok
                } else if miss.len() == lossless.len() {
                    Step::Warn
                } else {
                    Step::Skip
                },
                if miss.is_empty() {
                    "FLAC / ALAC / PCM 可用".to_string()
                } else {
                    format!("缺 {}", miss.join(" / "))
                },
            );

            // 6 有损编码器
            send(6, Step::Running, String::new());
            let lossy = ["libmp3lame", "libopus", "libvorbis"];
            let have: Vec<&str> = lossy
                .iter()
                .copied()
                .filter(|n| encoders.contains(*n))
                .collect();
            send(
                6,
                if have.is_empty() {
                    Step::Warn
                } else {
                    Step::Ok
                },
                if have.is_empty() {
                    "没有 MP3 / Opus / Vorbis 编码器".to_string()
                } else {
                    format!("{} 个可用", have.len())
                },
            );

            // 7 视频编码器。LGPL 构建通常没有 x264/x265，缺了只记一条，不当故障
            send(7, Step::Running, String::new());
            let video = ["libx264", "libx265", "libvpx-vp9", "libaom-av1"];
            let vhave: Vec<&str> = video
                .iter()
                .copied()
                .filter(|n| encoders.contains(*n))
                .collect();
            send(
                7,
                if vhave.is_empty() {
                    Step::Skip
                } else {
                    Step::Ok
                },
                if vhave.is_empty() {
                    "这个构建没有外挂视频编码器".to_string()
                } else {
                    format!("{} / 共 {} 个", vhave.join(" / "), vhave.len())
                },
            );

            // 8 音频输出设备：cpal 真去问系统，能问出来说明声卡链路通
            send(8, Step::Running, String::new());
            match fc_player::devices::list() {
                Ok(list) if !list.is_empty() => {
                    send(8, Step::Ok, format!("{} 个设备", list.len()))
                }
                _ => send(8, Step::Skip, "枚举不到".to_string()),
            }

            // 9 ATRAC 编码器（可选）
            send(9, Step::Running, String::new());
            match fc_core::find_atracdenc() {
                Some(_) => send(9, Step::Ok, "已安装".to_string()),
                None => send(9, Step::Skip, "未安装（可选）".to_string()),
            }

            // 10 Speex 编码器
            send(10, Step::Running, String::new());
            match fc_core::find_speexenc() {
                Some(_) => send(10, Step::Ok, "已找到".to_string()),
                None => send(10, Step::Skip, "未找到".to_string()),
            }

            // 11 配置与音乐库缓存
            send(11, Step::Running, String::new());
            let cfg = crate::settings::Settings::load();
            send(
                11,
                Step::Ok,
                if cfg.library_cache.is_empty() {
                    "配置就绪，音乐库还没扫".to_string()
                } else {
                    format!("音乐库 {} 首", cfg.library_cache.len())
                },
            );

            let _ = tx.send(Msg::Done(BootData { tools, encoders, version: ver }));
        });

        Self {
            rx,
            items: LABELS
                .iter()
                .map(|l| Item {
                    label: l,
                    state: Step::Todo,
                    detail: String::new(),
                })
                .collect(),
            data: None,
            started: Instant::now(),
            data_at: None,
            logo: None,
            shot: None,
        }
    }

    /// 每帧调一次；`Some` 表示可以进主界面了。
    pub fn poll(&mut self) -> Option<BootData> {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Step(i, s, d) => {
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = s;
                        if !d.is_empty() {
                            it.detail = d;
                        }
                    }
                }
                Msg::Done(d) => {
                    self.data = Some(d);
                    self.data_at = Some(Instant::now());
                }
            }
        }
        // 检查完还要在「正在加载项目」上停 2 秒
        if let Some(t) = self.data_at {
            if t.elapsed() >= PROJECT_HOLD {
                return self.data.take();
            }
        }
        if self.data.is_none() && self.started.elapsed() >= TIMEOUT {
            return Some(BootData {
                tools: None,
                encoders: HashSet::new(),
                version: None,
            });
        }
        None
    }

    fn done_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.state, Step::Ok | Step::Skip | Step::Warn))
            .count()
    }

    /// 当前这一行。
    ///
    /// 显示的是「最近完成的那一项 + 它的结果」，而不是正在跑的那项：
    /// 结果只闪一下根本看不清。这一行会一直挂着，等下一项完成时被顶掉，
    /// 所以看起来像清单在逐行往下走。
    fn current(&self) -> (String, String, Step) {
        if self.data.is_some() {
            return ("正在加载项目…".to_string(), String::new(), Step::Running);
        }
        if let Some(it) = self
            .items
            .iter()
            .rev()
            .find(|i| matches!(i.state, Step::Ok | Step::Skip | Step::Warn))
        {
            return (it.label.to_string(), it.detail.clone(), it.state);
        }
        if let Some(it) = self.items.iter().find(|i| i.state == Step::Running) {
            return (it.label.to_string(), String::new(), Step::Running);
        }
        ("准备中…".to_string(), String::new(), Step::Running)
    }

    /// 进度：检查阶段走到 70%，「加载项目」那 2 秒走完剩下的 30%
    fn progress(&self) -> f32 {
        let by_step = self.done_count() as f32 / self.items.len() as f32 * 0.70;
        let by_hold = match self.data_at {
            Some(t) => (t.elapsed().as_secs_f32() / PROJECT_HOLD.as_secs_f32()).min(1.0) * 0.30,
            None => 0.0,
        };
        (by_step + by_hold).min(1.0)
    }

    /// 画过场。调用方负责每帧 `request_repaint`。
    pub fn paint(&mut self, ctx: &egui::Context) {
        let area = ctx.screen_rect();
        let p = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("boot-splash"),
        ));

        // 底图：小窗口里也铺一张（裁切后正好落在双屏那一段）
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
        // 小窗口里字更密，压得比全屏版狠一点；但留点余地让底图看得出来
        p.rect_filled(area, egui::Rounding::ZERO, tint(BG, 202));

        let m = 24.0;

        // 头部：logo + 名字 + 版本
        let logo = self.logo.get_or_insert_with(|| load_logo(ctx));
        let logo_rect = Rect::from_min_size(egui::pos2(area.left() + m, area.top() + 20.0), Vec2::splat(26.0));
        p.image(
            logo.id(),
            logo_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        let nx = logo_rect.right() + 10.0;
        p.text(
            egui::pos2(nx, logo_rect.center().y - 6.0),
            Align2::LEFT_CENTER,
            APP_NAME,
            FontId::proportional(15.0),
            TEXT,
        );
        p.text(
            egui::pos2(nx, logo_rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            format!("v{VERSION} · 正在检查运行环境"),
            FontId::proportional(9.5),
            tint(TEXT, 120),
        );

        // 一次只显示一行（达芬奇那种「检查一项、显示一项」）
        let (label, detail, state) = self.current();
        let amber = Color32::from_rgb(232, 176, 84);
        let ry = area.bottom() - m - 46.0;
        let cx = area.left() + m + 6.0;
        let color = match state {
            Step::Ok | Step::Running => accent(),
            Step::Warn => amber,
            Step::Skip | Step::Todo => tint(TEXT, 70),
        };
        match state {
            Step::Ok => {
                p.circle_filled(egui::pos2(cx, ry), 5.0, color);
                // 手画的勾：两段线，不依赖字体里有没有 ✓
                p.line_segment(
                    [egui::pos2(cx - 2.6, ry), egui::pos2(cx - 0.6, ry + 2.2)],
                    Stroke::new(1.4f32, Color32::from_rgb(12, 26, 24)),
                );
                p.line_segment(
                    [egui::pos2(cx - 0.6, ry + 2.2), egui::pos2(cx + 2.8, ry - 2.4)],
                    Stroke::new(1.4f32, Color32::from_rgb(12, 26, 24)),
                );
            }
            Step::Skip => {
                p.line_segment(
                    [egui::pos2(cx - 4.0, ry), egui::pos2(cx + 4.0, ry)],
                    Stroke::new(1.4f32, color),
                );
            }
            Step::Todo => {
                p.circle_stroke(egui::pos2(cx, ry), 4.5, Stroke::new(1.0f32, color));
            }
            Step::Running | Step::Warn => {
                // 正在跑的时候画空心环 + 中心点，看着像在转
                p.circle_stroke(egui::pos2(cx, ry), 4.5, Stroke::new(1.4f32, color));
                p.circle_filled(egui::pos2(cx, ry), 1.8, color);
            }
        }
        p.text(
            egui::pos2(area.left() + m + 22.0, ry),
            Align2::LEFT_CENTER,
            &label,
            FontId::proportional(12.0),
            tint(TEXT, 215),
        );
        if !detail.is_empty() {
            p.text(
                egui::pos2(area.right() - m, ry),
                Align2::RIGHT_CENTER,
                &detail,
                FontId::proportional(10.5),
                tint(TEXT, 130),
            );
        }

        // 底部：状态 + 进度条
        let bar = Rect::from_min_size(
            egui::pos2(area.left() + m, area.bottom() - m - 2.0),
            Vec2::new(area.width() - m * 2.0, 2.0),
        );
        p.rect_filled(bar, egui::Rounding::same(1.0), tint(Color32::WHITE, 26));
        let w = bar.width() * self.progress();
        if w > 0.5 {
            p.rect_filled(
                Rect::from_min_size(bar.min, Vec2::new(w, 2.0)),
                egui::Rounding::same(1.0),
                accent(),
            );
        }
        p.text(
            egui::pos2(area.right() - m, bar.top() - 10.0),
            Align2::RIGHT_BOTTOM,
            "建议所有操作备份 · 慎防设备损坏，请自己负责",
            FontId::proportional(9.5),
            tint(TEXT, 75),
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

/// 过场底图。要换照片就替换 `assets/splash.jpg`，代码一个字都不用动。
fn load_shot(ctx: &egui::Context) -> egui::TextureHandle {
    const JPG: &[u8] = include_bytes!("../assets/splash.jpg");
    let img = image::load_from_memory(JPG)
        .expect("内置 splash.jpg 读不出来")
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    ctx.load_texture("boot-shot", ci, egui::TextureOptions::LINEAR)
}
