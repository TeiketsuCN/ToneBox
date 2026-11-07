//! 格式转换器 —— 三栏形态 + 壁纸毛玻璃 + 动效
//!
//! 布局：左（输出格式 + 小字解释） / 中（队列） / 右（选中文件详情）
//!
//! 三条产品决定：
//!   1. 输出格式默认全局一个，单个文件可以在队列里单独覆盖
//!   2. 支持一次拖多个文件
//!   3. 格式百科不单独开页面，挂在格式自己身上——点它，下面出小字
//!
//! 动效策略：不依赖 egui 的 animation API，自己用 `approach()` 做指数逼近。
//! 好处是行为完全可预测，不会因为版本升级挂掉。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bg;
#[cfg(windows)]
mod frame;
mod settings;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

use eframe::egui;
use egui::{Align, Color32, Layout, RichText, Rounding, Stroke};

use fc_analyze::QualityReport;
use fc_core::{estimate_size, FormatSpec, Group, MediaInfo, Tools};

/// 频谱分析的状态
enum Analysis {
    Idle,
    Running,
    Done(QualityReport),
    Failed(String),
}

// ── 配色 ────────────────────────────────────────────────
const TEXT: Color32 = Color32::from_rgb(240, 244, 250);
const MUTED: Color32 = Color32::from_rgb(158, 168, 186);
const DIM: Color32 = Color32::from_rgb(112, 122, 140);
const ACCENT: Color32 = Color32::from_rgb(122, 226, 214);
const WARN: Color32 = Color32::from_rgb(255, 198, 122);
const DANGER: Color32 = Color32::from_rgb(255, 138, 138);
const BG: Color32 = Color32::from_rgb(14, 16, 22);

// ── 布局规范 ────────────────────────────────────────────
// 面板宽度必须是常量，内容宽度在面板内锁死。
// 所有长文本一律显式 wrap，任何元素都不允许横向溢出把窗口边缘切掉。
const TITLEBAR_H: f32 = 44.0;
const LEFT_W: f32 = 216.0;
const RIGHT_W: f32 = 300.0;
const OUT_W: f32 = 272.0;

fn tint(c: Color32, a: u8) -> Color32 {
    let k = a as u16;
    Color32::from_rgba_premultiplied(
        (c.r() as u16 * k / 255) as u8,
        (c.g() as u16 * k / 255) as u8,
        (c.b() as u16 * k / 255) as u8,
        a,
    )
}

fn line(c: Color32, a: u8) -> Stroke {
    Stroke::new(1.0, tint(c, a))
}

/// 两个预乘色按 t 混合
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
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
fn approach(cur: &mut f32, target: f32, dt: f32, speed: f32) -> bool {
    let k = (dt * speed).clamp(0.0, 1.0);
    *cur += (target - *cur) * k;
    if (target - *cur).abs() < 0.002 {
        *cur = target;
        false
    } else {
        true
    }
}

// ── 队列条目 ────────────────────────────────────────────
#[derive(Clone)]
enum State {
    Pending,
    Running(f32),
    Done(PathBuf, u64),
    Failed(String),
}

struct Item {
    path: PathBuf,
    info: Option<MediaInfo>,
    error: Option<String>,
    state: State,
    override_id: Option<&'static str>,
    // 动效状态
    hover_t: f32,
    sel_t: f32,
    hovered: bool,
    progress_t: f32,
    analysis: Analysis,
    tags: fc_core::Tags,
    cover: Option<egui::TextureHandle>,
    /// 单文件标题覆盖（空 = 沿用源标签）
    title_ov: Option<String>,
    /// 单文件音轨号覆盖
    track_ov: Option<String>,
}

impl Item {
    fn spec(&self, global: &'static FormatSpec) -> &'static FormatSpec {
        self.override_id.and_then(fc_core::by_id).unwrap_or(global)
    }
    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// 后台读完标签/封面后回传
struct MediaMsg {
    idx: usize,
    tags: fc_core::Tags,
    cover: Option<(Vec<u8>, u32, u32)>,
}

enum Msg {
    Progress(usize, f32),
    Done(usize, PathBuf, u64),
    Failed(usize, String),
    Finished,
}

struct App {
    tools: Option<Tools>,
    items: Vec<Item>,
    selected: usize,
    target: &'static FormatSpec,
    expanded: Option<&'static str>,
    rx: Option<Receiver<Msg>>,
    running: bool,
    batch_total: usize,
    batch_done: usize,
    frames: u64,
    backdrop_done: bool,
    player: Option<fc_player::Player>,
    player_for: Option<usize>,
    play_error: Option<String>,
    // 背景与动效
    bg: bg::WallpaperBg,
    dt: f32,
    fmt_sel: Vec<(&'static str, f32)>,
    blurb_t: f32,
    blurb_id: Option<&'static str>,
    detail_t: f32,
    detail_for: usize,
    /// 上一次点击的行号和时刻，用来自己判定双击
    last_click: (usize, f64),
    /// 窗口是否置顶
    pinned: bool,
    /// 频谱分析结果通道
    analysis_tx: std::sync::mpsc::Sender<(usize, Result<QualityReport, String>)>,
    analysis_rx: Receiver<(usize, Result<QualityReport, String>)>,
    /// 标签/封面回传（后台线程读，主线程建纹理）
    media_tx: std::sync::mpsc::Sender<MediaMsg>,
    media_rx: Receiver<MediaMsg>,
    /// 设置项
    cfg: settings::Settings,
    settings_open: bool,
    settings_t: f32,
    /// 元数据区是否展开（默认收起，第四列放不下那么多控件）
    meta_open: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::TRANSPARENT;
        style.visuals.window_fill = Color32::TRANSPARENT;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        cc.egui_ctx.set_style(style);

        let target = fc_core::default_target();
        let (atx, arx) = channel();
        let (mtx, mrx) = channel();
        App {
            tools: fc_core::locate(),
            items: Vec::new(),
            selected: 0,
            target,
            expanded: Some(target.id),
            rx: None,
            running: false,
            batch_total: 0,
            batch_done: 0,
            frames: 0,
            backdrop_done: false,
            player: None,
            player_for: None,
            play_error: None,
            bg: bg::WallpaperBg::new(&cc.egui_ctx),
            dt: 0.016,
            fmt_sel: fc_core::browsable().map(|f| (f.id, 0.0)).collect(),
            blurb_t: 1.0,
            blurb_id: Some(target.id),
            detail_t: 1.0,
            detail_for: 0,
            last_click: (usize::MAX, -10.0),
            pinned: false,
            analysis_tx: atx,
            analysis_rx: arx,
            media_tx: mtx,
            media_rx: mrx,
            cfg: settings::Settings::load(),
            settings_open: false,
            settings_t: 0.0,
            meta_open: false,
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let Some(tools) = self.tools.clone() else { return };
        for p in paths {
            if self.items.iter().any(|i| i.path == p) {
                continue;
            }
            let (info, error) = match fc_core::probe(&tools, &p) {
                Ok(i) => (Some(i), None),
                Err(e) => {
                    // 读不出来时先问一句"是不是加密的"，能答就答得比报错清楚
                    let msg = match fc_core::identify(&p) {
                        Some(enc) => format!("{}\n\n{}", enc.summary(), enc.full_text()),
                        None => format!("{e:#}"),
                    };
                    (None, Some(msg))
                }
            };
            let idx = self.items.len();
            spawn_media_load(&tools, &p, idx, self.media_tx.clone());
            self.items.push(Item {
                path: p,
                info,
                error,
                state: State::Pending,
                override_id: None,
                hover_t: 0.0,
                sel_t: 0.0,
                hovered: false,
                progress_t: 0.0,
                analysis: Analysis::Idle,
                tags: fc_core::Tags::default(),
                cover: None,
                title_ov: None,
                track_ov: None,
            });
        }
        if !self.items.is_empty() && self.selected >= self.items.len() {
            self.selected = 0;
        }
    }

    fn start_batch(&mut self, ctx: &egui::Context) {
        let Some(tools) = self.tools.clone() else { return };
        let mut jobs: Vec<(usize, PathBuf, PathBuf, Vec<String>, f64)> = Vec::new();
        for (i, it) in self.items.iter_mut().enumerate() {
            if it.info.is_none() || !matches!(it.state, State::Pending) {
                continue;
            }
            let spec = it.spec(self.target);
            let out = unique_output_in(&it.path, spec.ext, &self.cfg);
            let dur = it.info.as_ref().map(|m| m.duration_s).unwrap_or(0.0);
            // 参数由 core 统一生成，GUI 不拼 ffmpeg 命令行
            let mut args = fc_core::args_for(spec, &self.cfg.opts);

            // 元数据：源标签 → 单文件覆盖 → 批量覆盖，后写的赢
            if self.cfg.write_tags {
                let mut tags = it.tags.clone();
                if let Some(v) = it.title_ov.as_ref().filter(|v| !v.trim().is_empty()) {
                    tags.title = Some(v.trim().to_string());
                }
                if let Some(v) = it.track_ov.as_ref().filter(|v| !v.trim().is_empty()) {
                    tags.track = Some(v.trim().to_string());
                }
                if !self.cfg.meta_artist.trim().is_empty() {
                    tags.artist = Some(self.cfg.meta_artist.trim().to_string());
                }
                if !self.cfg.meta_album.trim().is_empty() {
                    tags.album = Some(self.cfg.meta_album.trim().to_string());
                }

                // 封面只有支持 attached_pic 的容器才传（B 的提醒：WAV/AIFF 不吃）
                let cover_ok = matches!(spec.id, "flac" | "mp3" | "m4a" | "alac");
                let cover_path = self.cfg.meta_cover.trim();
                let cover = if cover_ok && !cover_path.is_empty() {
                    Some(std::path::PathBuf::from(cover_path))
                } else {
                    None
                };

                let mw = fc_core::MetaWrite {
                    tags: &tags,
                    cover: cover.as_deref(),
                };
                args.extend(fc_core::metadata_args(&mw));
            }

            jobs.push((i, it.path.clone(), out, args, dur));
            it.state = State::Running(0.0);
        }
        if jobs.is_empty() {
            return;
        }
        self.running = true;
        self.batch_total = jobs.len();
        self.batch_done = 0;

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx2 = ctx.clone();

        std::thread::spawn(move || {
            for (idx, input, out, args, dur) in jobs {
                let tx_p = tx.clone();
                let ctx_p = ctx2.clone();
                let arg_refs: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
                let res = fc_core::ffmpeg::convert_with_inputs(
                    &tools,
                    &input,
                    &out,
                    &arg_refs,
                    &[],
                    dur,
                    Box::new(move |p| {
                        let _ = tx_p.send(Msg::Progress(idx, p));
                        ctx_p.request_repaint();
                    }),
                );
                match res {
                    Ok(size) => {
                        // convert_with_inputs 直接返回输出字节数
                        let _ = tx.send(Msg::Done(idx, out, size));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::Failed(idx, format!("{e:#}")));
                    }
                }
                ctx2.request_repaint();
            }
            let _ = tx.send(Msg::Finished);
        });
    }

    /// 起一个后台线程做频谱分析（每个文件约 1 秒，不能卡界面）
    fn start_analyze(&mut self, idx: usize, ctx: &egui::Context) {
        let Some(it) = self.items.get_mut(idx) else {
            return;
        };
        if !matches!(it.analysis, Analysis::Idle) {
            return;
        }
        let Some(info) = it.info.clone() else {
            return;
        };
        let path = it.path.clone();
        it.analysis = Analysis::Running;

        let tx = self.analysis_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = fc_analyze::analyze(&path, &info).map_err(|e| format!("{e:#}"));
            let _ = tx.send((idx, res));
            ctx.request_repaint();
        });
    }

    fn start_play(&mut self, idx: usize) {
        self.player = None;
        self.play_error = None;
        let Some(tools) = self.tools.clone() else {
            self.play_error = Some("找不到 ffmpeg，无法试听".into());
            self.player_for = Some(idx);
            return;
        };
        let Some(it) = self.items.get(idx) else { return };
        let Some(info) = it.info.clone() else {
            self.play_error = Some("这个文件解不出音频，没法试听".into());
            self.player_for = Some(idx);
            return;
        };
        match fc_player::Player::open(&tools.ffmpeg, &it.path, info.duration_s) {
            Ok(p) => {
                self.player = Some(p);
                self.player_for = Some(idx);
            }
            Err(e) => {
                self.play_error = Some(format!("{e:#}"));
                self.player_for = Some(idx);
            }
        }
    }

    fn poll_media(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.media_rx.try_recv() {
            if let Some(it) = self.items.get_mut(m.idx) {
                it.tags = m.tags;
                if let Some((rgba, w, h)) = m.cover {
                    let img = egui::ColorImage {
                        size: [w as usize, h as usize],
                        pixels: rgba
                            .chunks_exact(4)
                            .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                            .collect(),
                    };
                    it.cover = Some(ctx.load_texture(
                        format!("cover_{}", m.idx),
                        img,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
        }
    }

    fn poll_analysis(&mut self) {
        while let Ok((idx, res)) = self.analysis_rx.try_recv() {
            if let Some(it) = self.items.get_mut(idx) {
                it.analysis = match res {
                    Ok(r) => Analysis::Done(r),
                    Err(e) => Analysis::Failed(e),
                };
            }
        }
    }

    fn poll(&mut self) {
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            match m {
                Msg::Progress(i, p) => {
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = State::Running(p);
                    }
                }
                Msg::Done(i, path, size) => {
                    // 转换成功且用户选了「删除源文件」——在标记成功之后删，
                    // 删失败也不影响结果（比如源文件正被别的程序占用）
                    let src = self.items.get(i).map(|it| it.path.clone());
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = State::Done(path, size);
                    }
                    if self.cfg.delete_source {
                        if let Some(src) = src {
                            // 删不掉就算了（源文件可能正被占用），不打断整批
                            let _ = std::fs::remove_file(&src);
                        }
                    }
                    self.batch_done += 1;
                }
                Msg::Failed(i, e) => {
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = State::Failed(e);
                    }
                    self.batch_done += 1;
                }
                Msg::Finished => {
                    self.running = false;
                    self.rx = None;
                }
            }
        }
    }

    fn overall(&self) -> f32 {
        if self.batch_total == 0 {
            return 0.0;
        }
        let cur = self
            .items
            .iter()
            .filter_map(|i| match &i.state {
                State::Running(p) => Some(*p as f64),
                _ => None,
            })
            .sum::<f64>() as f32;
        ((self.batch_done as f32 + cur) / self.batch_total as f32).clamp(0.0, 1.0)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if !self.backdrop_done && self.frames > 3 {
            self.backdrop_done = apply_backdrop();
            #[cfg(windows)]
            frame::install("格式转换器");
        }
        self.dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.05);
        self.poll();
        self.poll_analysis();
        self.poll_media(ctx);

        if let Some(p) = self.player.as_mut() {
            if p.finished() {
                p.pause();
            } else if p.is_playing() {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|d| d.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }

        // 背景画在最底层
        self.bg.paint_mode(ctx, self.cfg.bg_mode, self.cfg.dim);

        // 最大化时系统会把窗口撑到工作区外 8px，UI 这边补回来
        #[cfg(windows)]
        let inset = if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            frame::MAXIMIZE_INSET
        } else {
            0.0
        };
        #[cfg(not(windows))]
        let inset: f32 = 0.0;

        let mut animating = false;

        egui::TopBottomPanel::top("titlebar")
            .exact_height(TITLEBAR_H)
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 132))
                    .inner_margin(egui::Margin {
                        left: 10.0 + inset,
                        right: 10.0 + inset,
                        top: inset,
                        bottom: 0.0,
                    }),
            )
            .show(ctx, |ui| self.titlebar(ui));

        if self.player.is_some() {
            let name = self
                .player_for
                .and_then(|i| self.items.get(i))
                .map(|it| it.name())
                .unwrap_or_else(|| "—".into());
            egui::TopBottomPanel::bottom("player")
                .frame(
                    egui::Frame::none()
                        .fill(tint(BG, 142))
                        .inner_margin(egui::Margin {
                            left: 16.0 + inset,
                            right: 16.0 + inset,
                            top: 7.0,
                            bottom: 7.0 + inset,
                        }),
                )
                .show(ctx, |ui| self.player_bar(ui, &name));
        }

        egui::SidePanel::left("formats")
            .exact_width(LEFT_W)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 104))
                    .inner_margin(egui::Margin {
                        left: 12.0 + inset,
                        right: 12.0,
                        top: 12.0,
                        bottom: 12.0 + inset,
                    }),
            )
            .show(ctx, |ui| animating |= self.sidebar(ui));

        let mut do_start = false;
        egui::SidePanel::right("output")
            .exact_width(OUT_W)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 128))
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0)),
            )
            .show(ctx, |ui| {
                // 开始转换钉在最下面——这一列的终点就是它
                egui::TopBottomPanel::bottom("out_start")
                    .frame(
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(0.0, 8.0)),
                    )
                    .show_inside(ui, |ui| {
                        if self.running {
                            ui.add(
                                egui::ProgressBar::new(self.overall())
                                    .desired_width(ui.available_width())
                                    .fill(ACCENT),
                            );
                            ui.add_space(6.0);
                        }
                        let can = !self.running
                            && self.tools.is_some()
                            && self
                                .items
                                .iter()
                                .any(|i| i.info.is_some() && matches!(i.state, State::Pending));
                        if start_button(ui, can, self.running) {
                            do_start = true;
                        }
                    });

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        animating |= self.output_panel(ui);
                    });
            });
        if do_start {
            self.start_batch(ctx);
        }

        egui::SidePanel::right("detail")
            .exact_width(RIGHT_W)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 104))
                    .inner_margin(egui::Margin {
                        left: 16.0,
                        right: 16.0 + inset,
                        top: 12.0,
                        bottom: 12.0 + inset,
                    }),
            )
            .show(ctx, |ui| animating |= self.detail(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 92))
                    .inner_margin(egui::Margin {
                        left: 16.0,
                        right: 16.0,
                        top: 12.0,
                        bottom: 12.0 + inset,
                    }),
            )
            .show(ctx, |ui| animating |= self.queue(ui, ctx));

        // 设置覆盖层（最后画，盖住一切）
        animating |= approach(
            &mut self.settings_t,
            if self.settings_open { 1.0 } else { 0.0 },
            self.dt,
            16.0,
        );
        if self.settings_t > 0.001 {
            self.settings_panel(ctx);
        }

        if animating {
            ctx.request_repaint();
        }
    }
}

/// 后台读标签 + 抽封面。封面统一缩到 96x96，够列表用，省内存。
fn spawn_media_load(
    tools: &Tools,
    path: &Path,
    idx: usize,
    tx: std::sync::mpsc::Sender<MediaMsg>,
) {
    let ffprobe = tools.ffprobe.clone();
    let ffmpeg = tools.ffmpeg.clone();
    let p = path.to_path_buf();
    std::thread::spawn(move || {
        let tags = fc_core::read_tags(&ffprobe, &p);
        let cover = fc_core::extract_cover(&ffmpeg, &p).and_then(|bytes| {
            let img = image::load_from_memory(&bytes).ok()?;
            let small = img.resize_exact(96, 96, image::imageops::FilterType::Triangle);
            let rgba = small.to_rgba8();
            Some((rgba.as_raw().clone(), rgba.width(), rgba.height()))
        });
        let _ = tx.send(MediaMsg { idx, tags, cover });
    });
}

/// 目标目录：源文件同目录，或用户指定的目录
fn output_dir_for(input: &Path, cfg: &settings::Settings) -> PathBuf {
    match cfg.output_mode {
        settings::OutputMode::Custom => {
            let d = cfg.output_dir.trim();
            if d.is_empty() {
                input.parent().map(|p| p.to_path_buf()).unwrap_or_default()
            } else {
                PathBuf::from(d)
            }
        }
        settings::OutputMode::Source => input
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default(),
    }
}

fn unique_output_in(input: &Path, ext: &str, cfg: &settings::Settings) -> PathBuf {
    let stem = input
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let dir_buf = output_dir_for(input, cfg);
    let dir = if dir_buf.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir_buf.as_path()
    };
    let mut cand = dir.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while cand.exists() {
        cand = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    cand
}

/// 开始转换按钮（放在第四列最下面——它是这一列的终点）
fn start_button(ui: &mut egui::Ui, can: bool, running: bool) -> bool {
    let b = egui::Button::new(
        RichText::new(if running { "转换中…" } else { "开始转换" })
            .size(13.0)
            .color(if can {
                Color32::from_rgb(10, 26, 24)
            } else {
                DIM
            }),
    )
    .fill(if can { ACCENT } else { tint(Color32::WHITE, 10) })
    .stroke(Stroke::NONE)
    .rounding(Rounding::same(10.0))
    .min_size(egui::vec2(ui.available_width(), 36.0));
    ui.add_enabled(can, b).clicked()
}

fn fmt_duration(s: f64) -> String {
    let total = s.round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn fmt_size(b: u64) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / 1048576.0)
    } else {
        format!("{} KB", b / 1024)
    }
}

impl App {
    /// 自绘标题栏。系统标题栏已经去掉，拖动/按钮都在这里。
    fn titlebar(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        ui.set_height(TITLEBAR_H);

        // 先注册拖动区（后注册的按钮会覆盖在上面，优先接点击）
        let resp = ui.interact(
            full,
            ui.id().with("titlebar_drag"),
            egui::Sense::click_and_drag(),
        );
        #[cfg(windows)]
        let native_frame = frame::ACTIVE.load(std::sync::atomic::Ordering::SeqCst);
        #[cfg(not(windows))]
        let native_frame = false;

        // 原生 WM_NCHITTEST 接管后，拖动由系统处理；再发 StartDrag 会两边抢
        if resp.drag_started() && !native_frame {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            let maxed = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed));
        }

        // 左边：标题 + 副标题
        ui.painter().text(
            full.left_center() + egui::vec2(8.0, -6.0),
            egui::Align2::LEFT_CENTER,
            "格式转换器",
            egui::FontId::proportional(13.5),
            TEXT,
        );
        ui.painter().text(
            full.left_center() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_CENTER,
            "转换 · 质量判断 · 格式解释",
            egui::FontId::proportional(9.0),
            DIM,
        );

        // 右边：图钉 / 最小化 / 最大化 / 关闭
        let mut rect = egui::Rect::from_min_size(
            egui::pos2(full.right() - TITLEBAR_H, full.top()),
            egui::vec2(TITLEBAR_H, TITLEBAR_H),
        );
        let maxed = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));

        // 从右往左排：关闭、最大化、最小化、图钉
        if title_button(ui, rect, TitleBtn::Close, false) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, if maxed { TitleBtn::Restore } else { TitleBtn::Maximize }, false) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed));
        }
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, TitleBtn::Minimize, false) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, TitleBtn::Pin, self.pinned) {
            self.pinned = !self.pinned;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                if self.pinned {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                },
            ));
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let dt = self.dt;
        let mut animating = false;

        ui.label(RichText::new("输出格式").size(11.0).color(DIM));
        ui.add_space(4.0);

        // 展开状态变化时，把高度动画归零重来
        if self.blurb_id != self.expanded {
            self.blurb_id = self.expanded;
            self.blurb_t = 0.0;
        }
        // 只在这里驱动一次，不能放进每行的循环（会互相覆盖导致永远振荡）
        animating |= approach(
            &mut self.blurb_t,
            if self.expanded.is_some() { 1.0 } else { 0.0 },
            dt,
            14.0,
        );

        // 底部提示固定（用内嵌面板占位，避免被滚动内容盖住），其余区域可滚
        egui::TopBottomPanel::bottom("fmt_footer")
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(0.0, 4.0)))
            .show_inside(ui, |ui| {
                ui.set_width(ui.available_width());
                let b = egui::Button::new(RichText::new("设置").size(10.5).color(MUTED))
                    .fill(tint(Color32::WHITE, 10))
                    .stroke(line(Color32::WHITE, 18))
                    .rounding(Rounding::same(8.0))
                    .min_size(egui::vec2(ui.available_width(), 26.0));
                if ui.add(b).clicked() {
                    self.settings_open = true;
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new("只处理未加密文件 · 不支持任何 DRM")
                        .size(9.0)
                        .color(DIM),
                );
            });

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // 紧凑排布：全部格式一屏放下，不做折叠
                ui.spacing_mut().item_spacing.y = 1.0;
                // 按钮默认内边距 7px 会把 24px 的行撑到 28px，侧栏挤不下
                ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);

                for group in Group::all() {
                    // 列出该组全部格式：能编码的可以选为输出，
                    // 只读的也列出来——用户需要知道"手上这个是什么、为什么不能输出成它"
                    let specs: Vec<&'static FormatSpec> =
                        fc_core::all().filter(|f| f.group == group).collect();
                    if specs.is_empty() {
                        continue;
                    }

                    ui.add_space(6.0);
                    ui.label(RichText::new(group.title()).size(9.5).color(DIM));
                    ui.add_space(1.0);

                    for spec in specs {
                        animating |= self.format_row(ui, spec, dt);
                    }
                }
            });

        animating
    }

    /// 画一行输出格式（含展开后的小字）
    fn format_row(
        &mut self,
        ui: &mut egui::Ui,
        spec: &'static FormatSpec,
        dt: f32,
    ) -> bool {
        let mut animating = false;
        let selectable = spec.can_encode;
        let selected = selectable && spec.id == self.target.id;

        let cur = {
            match self.fmt_sel.iter_mut().find(|(id, _)| *id == spec.id) {
                Some((_, v)) => {
                    animating |= approach(v, if selected { 1.0 } else { 0.0 }, dt, 16.0);
                    *v
                }
                None => 0.0,
            }
        };

        let fill = blend(Color32::TRANSPARENT, tint(ACCENT, 34), cur);
        let stroke_c = blend(Color32::TRANSPARENT, tint(ACCENT, 80), cur);
        let base = if selectable { TEXT } else { MUTED };
        let text_c = blend(base, ACCENT, cur);

        let mut row = egui::Button::new(RichText::new(spec.name).size(11.5).color(text_c))
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke_c))
            .rounding(Rounding::same(8.0))
            .min_size(egui::vec2(ui.available_width(), 22.0));
        if !selectable {
            row = row
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);
        }

        let mut resp = ui.add(row);
        if !selectable {
            resp = resp.on_hover_text("只读格式：能解码，不能作为输出目标");
        }
        if resp.clicked() {
            // 只读格式不改输出目标，只展开解释
            if selectable {
                self.target = spec;
            }
            self.expanded = if self.expanded == Some(spec.id) {
                None
            } else {
                Some(spec.id)
            };
        }

        // 小字：高度 + 透明度一起动
        let expanded_here = self.expanded == Some(spec.id);
        if self.blurb_t > 0.01 && expanded_here {
            // 左边留 6px 缩进，所以排版宽度要比可用宽度再窄 6px，
            // 否则文字会被自己的裁剪矩形切掉右边（长文案换行后就露出来了）
            let avail_w = (ui.available_width() - 4.0).max(60.0);
            let text_w = (avail_w - 6.0).max(50.0);
            // 侧栏用小字说明（三行以内）。`wiki.rs` 的长文案留给以后的百科页，
            // 塞进侧栏会把列表撑得没法看。
            let galley = ui.painter().layout(
                spec.blurb.to_string(),
                egui::FontId::proportional(9.5),
                MUTED,
                text_w,
            );
            let full_h = galley.size().y + 3.0;
            let h = full_h * self.blurb_t;
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(avail_w, h), egui::Sense::hover());
            ui.painter()
                .with_clip_rect(rect)
                .galley(rect.min + egui::vec2(6.0, 0.0), galley, MUTED);
        }

        animating
    }

    fn queue(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        ui.set_width(ui.available_width());
        if self.items.is_empty() {
            self.drop_zone(ui);
            return false;
        }
        let dt = self.dt;
        let mut animating = false;
        let mut remove: Option<usize> = None;
        let mut play_now: Option<usize> = None;
        let count = self.items.len();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 58.0)
            .show(ui, |ui| {
                for i in 0..count {
                    let is_sel = i == self.selected;
                    let name = self.items[i].name();
                    let info = self.items[i].info.clone();
                    let error = self.items[i].error.clone();
                    let spec = self.items[i].spec(self.target);
                    let state = self.items[i].state.clone();
                    let override_id = self.items[i].override_id;
                    let tags = self.items[i].tags.clone();
                    let cover = self.items[i].cover.clone();

                    // 动效值先算，再画
                    let (sel_t, hov_t) = {
                        let it = &mut self.items[i];
                        animating |= approach(&mut it.sel_t, if is_sel { 1.0 } else { 0.0 }, dt, 16.0);
                        let target_h = if it.hovered { 1.0 } else { 0.0 };
                        animating |= approach(&mut it.hover_t, target_h, dt, 14.0);
                        (it.sel_t, it.hover_t)
                    };

                    let base = blend(tint(Color32::WHITE, 6), tint(Color32::WHITE, 16), hov_t);
                    let fill = blend(base, tint(ACCENT, 30), sel_t);
                    let stroke_c = blend(
                        blend(Color32::TRANSPARENT, tint(Color32::WHITE, 22), hov_t),
                        tint(ACCENT, 90),
                        sel_t,
                    );

                    let frame = egui::Frame::none()
                        .fill(fill)
                        .stroke(Stroke::new(1.0, stroke_c))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(egui::Margin::symmetric(14.0, 10.0));

                    // 右侧按钮占据的区域。整行点击必须避开它，
                    // 否则整行 interact 会盖在按钮上面，把点击吃掉。
                    let mut btn_zone: Option<egui::Rect> = None;
                    let resp = frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let row_w = ui.available_width();
                            cover_thumb(ui, cover.as_ref(), 36.0);
                            ui.add_space(5.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((row_w - 42.0 - 186.0).max(100.0));
                                let title = tags.display_title(&name);
                                ui.add(
                                    egui::Label::new(RichText::new(title).size(12.5).color(TEXT))
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                                ui.add_space(2.0);
                                let sub0 = match &info {
                                    Some(m) => format!(
                                        "{} · {} kbps · {}  →  {}",
                                        m.codec_label(),
                                        m.bitrate_kbps,
                                        fmt_duration(m.duration_s),
                                        spec.name
                                    ),
                                    None => {
                                        let e = error
                                            .clone()
                                            .unwrap_or_else(|| "读取失败".into());
                                        let first: String =
                                            e.lines().next().unwrap_or("").chars().take(46).collect();
                                        format!("{first}…")
                                    }
                                };
                                // 有艺术家/专辑就并到技术行前面
                                let artist = tags.display_sub();
                                let sub = if artist.is_empty() {
                                    sub0
                                } else {
                                    format!("{artist} · {sub0}")
                                };
                                let _ = &name;
                                ui.add(
                                    egui::Label::new(RichText::new(sub).size(10.0).color(DIM))
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                                match &state {
                                    State::Pending => {
                                        ui.label(RichText::new("待转换").size(10.0).color(MUTED));
                                    }
                                    State::Running(p) => {
                                        ui.add(
                                            egui::ProgressBar::new(*p)
                                                .desired_width(78.0)
                                                .fill(ACCENT),
                                        );
                                    }
                                    State::Done(_, sz) => {
                                        ui.label(
                                            RichText::new(format!("✓ {}", fmt_size(*sz)))
                                                .size(10.0)
                                                .color(ACCENT),
                                        );
                                    }
                                    State::Failed(_) => {
                                        ui.label(RichText::new("✗").size(11.0).color(DANGER));
                                    }
                                }

                                // 选过格式就把「···」换成格式名，用户一眼能看到这行要转成什么
                                let pick = override_id
                                    .and_then(fc_core::by_id)
                                    .map(|f| f.name.to_string());
                                let (label, label_col) = match &pick {
                                    Some(n) => (n.clone(), ACCENT),
                                    None => ("···".to_string(), DIM),
                                };
                                let menu_resp = ui.menu_button(
                                    RichText::new(label).size(if pick.is_some() { 10.5 } else { 12.0 }).color(label_col),
                                    |ui| {
                                        ui.set_min_width(120.0);
                                        if ui
                                            .selectable_label(override_id.is_none(), "跟随全局")
                                            .clicked()
                                        {
                                            self.items[i].override_id = None;
                                            ui.close_menu();
                                        }
                                        for f in fc_core::browsable() {
                                            let on = override_id == Some(f.id);
                                            if ui.selectable_label(on, f.name).clicked() {
                                                self.items[i].override_id = Some(f.id);
                                                ui.close_menu();
                                            }
                                        }
                                    },
                                );

                                let del_resp =
                                    ui.small_button(RichText::new("×").size(11.0).color(DIM));
                                if del_resp.clicked() {
                                    remove = Some(i);
                                }
                                btn_zone = Some(menu_resp.response.rect.union(del_resp.rect));
                            });
                        });
                    });

                    let hovered = resp.response.hovered() || resp.response.contains_pointer();
                    self.items[i].hovered = hovered;

                    // 用稳定 id 重新注册交互，否则 Frame 每帧生成的 id 不同，
                    // egui 无法把两次点击关联成双击。
                    let row_id = ui.id().with(("queue_row", i));
                    let mut hit = resp.response.rect;
                    if let Some(b) = btn_zone {
                        // 从按钮左边界往右都不算行点击，留给按钮
                        hit.max.x = b.left().min(hit.max.x);
                    }
                    let row_resp = ui.interact(hit, row_id, egui::Sense::click());
                    if row_resp.clicked() {
                        // 自己判定双击：egui 的 double_clicked 依赖跨帧的 widget id，
                        // 在 ScrollArea + Frame 里不稳定，所以用时间差自己算。
                        let now = ui.input(|inp| inp.time);
                        let is_double = self.last_click.0 == i && now - self.last_click.1 < 0.45;
                        if is_double {
                            play_now = Some(i);
                            self.last_click = (usize::MAX, -10.0);
                        } else {
                            self.selected = i;
                            self.last_click = (i, now);
                        }
                    }
                    if row_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }

                    ui.add_space(5.0);
                }
            });

        if let Some(i) = remove {
            self.items.remove(i);
            self.selected = self.selected.min(self.items.len().saturating_sub(1));
        }
        if let Some(i) = play_now {
            self.selected = i;
            self.start_play(i);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {

            if ui
                .add_enabled(
                    !self.running,
                    egui::Button::new(RichText::new("添加文件").size(12.0).color(MUTED))
                        .fill(Color32::TRANSPARENT)
                        .stroke(line(Color32::WHITE, 20))
                        .rounding(Rounding::same(9.0))
                        .min_size(egui::vec2(86.0, 34.0)),
                )
                .clicked()
            {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter(
                        "音频",
                        &[
                            "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma", "ape",
                            "oma", "at3", "aa3", "aiff", "aif",
                        ],
                    )
                    .pick_files()
                {
                    self.add_files(paths);
                }
            }

            if self.running {
                ui.add(
                    egui::ProgressBar::new(self.overall())
                        .desired_width((ui.available_width() - 8.0).max(60.0))
                        .fill(ACCENT),
                );
            } else if ui
                .add(
                    egui::Button::new(RichText::new("清空").size(12.0).color(DIM))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(9.0)),
                )
                .clicked()
            {
                self.items.clear();
                self.selected = 0;
            }
        });

        animating
    }

    fn drop_zone(&mut self, ui: &mut egui::Ui) {
        let mut do_pick = false;
        let avail = ui.available_height();
        egui::Frame::none()
            .fill(tint(Color32::WHITE, 6))
            .stroke(line(Color32::WHITE, 22))
            .rounding(Rounding::same(16.0))
            .inner_margin(egui::Margin::symmetric(20.0, 16.0))
            .show(ui, |ui| {
                ui.set_min_height(avail - 40.0);
                let pad = ((avail - 190.0) * 0.5).max(10.0);
                ui.add_space(pad);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("把音频文件拖到这里").size(15.0).color(TEXT));
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(
                            "可以一次拖多个 · MP3 / FLAC / WAV / M4A / OGG / Opus / ATRAC",
                        )
                        .size(10.5)
                        .color(MUTED),
                    );
                    ui.add_space(16.0);
                    let btn =
                        egui::Button::new(RichText::new("选择文件").size(12.5).color(TEXT))
                            .fill(tint(Color32::WHITE, 16))
                            .stroke(line(Color32::WHITE, 32))
                            .rounding(Rounding::same(9.0));
                    if ui.add(btn).clicked() {
                        do_pick = true;
                    }
                });
            });
        if do_pick {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter(
                    "音频",
                    &[
                        "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma", "ape", "oma",
                        "at3", "aa3", "aiff", "aif",
                    ],
                )
                .pick_files()
            {
                self.add_files(paths);
            }
        }
    }

    /// 设置页：从下往上滑入的覆盖层，不弹新窗口
    fn settings_panel(&mut self, ctx: &egui::Context) {
        let t = self.settings_t;
        let screen = ctx.screen_rect();
        let top = screen.top() + TITLEBAR_H;
        let area = egui::Rect::from_min_max(
            egui::pos2(screen.left(), top),
            screen.max,
        );
        // 整页不透明，只做"从下往上滑"的位移，不做淡入——否则底下的界面会透出来
        let offset = 0.0;

        let mut changed = false;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }

        egui::Area::new(egui::Id::new("settings_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min + egui::vec2(0.0, offset))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(tint(BG, 255))
                    .inner_margin(egui::Margin::symmetric(28.0, 18.0))
                    .show(ui, |ui| {
                        ui.set_min_size(area.size());
                        ui.set_max_width(area.width() - 56.0);
                        // 顶部 44px 是原生标题栏命中区（HTCAPTION），
                        // 放在那里的按钮点不到，所以内容整体下移避开。
                        ui.add_space(TITLEBAR_H + (1.0 - t) * 22.0);

                        // 顶栏
                        // 返回：用显式矩形做命中区，避免按钮布局和视觉位置对不上
                        let back_rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(78.0, 30.0),
                        );
                        let back = ui.interact(
                            back_rect,
                            ui.id().with("settings_back"),
                            egui::Sense::click(),
                        );
                        ui.painter().rect_filled(
                            back_rect,
                            Rounding::same(8.0),
                            if back.hovered() {
                                tint(Color32::WHITE, 22)
                            } else {
                                tint(Color32::WHITE, 12)
                            },
                        );
                        ui.painter().text(
                            back_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "← 返回",
                            egui::FontId::proportional(11.0),
                            TEXT,
                        );
                        if back.clicked() {
                            self.settings_open = false;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(88.0);
                            ui.label(RichText::new("设置").size(15.0).color(TEXT).strong());
                        });
                        ui.add_space(18.0);

                        // ── 外观 ──
                        section(ui, "外观");
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("背景").size(10.5).color(MUTED));
                            ui.add_space(12.0);
                            for m in settings::BgMode::all() {
                                let on = self.cfg.bg_mode == m;
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(m.label()).size(10.5).color(if on {
                                                Color32::from_rgb(10, 26, 24)
                                            } else {
                                                MUTED
                                            }),
                                        )
                                        .fill(if on { ACCENT } else { tint(Color32::WHITE, 10) })
                                        .stroke(if on {
                                            Stroke::NONE
                                        } else {
                                            line(Color32::WHITE, 20)
                                        })
                                        .rounding(Rounding::same(8.0)),
                                    )
                                    .clicked()
                                {
                                    self.cfg.bg_mode = m;
                                    changed = true;
                                }
                            }
                        });

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("暗化").size(10.5).color(MUTED));
                            ui.add_space(12.0);
                            let mut dim = self.cfg.dim as i32;
                            ui.spacing_mut().slider_width = 220.0;
                            if ui
                                .add(egui::Slider::new(&mut dim, 0..=210).show_value(false))
                                .changed()
                            {
                                self.cfg.dim = dim as u8;
                                changed = true;
                            }
                            ui.label(
                                RichText::new(format!("{}", self.cfg.dim)).size(9.0).color(DIM),
                            );
                        });

                        ui.add_space(18.0);

                        // ── 行为 ──
                        section(ui, "行为");
                        let mut auto = self.cfg.auto_analyze;
                        if ui
                            .checkbox(&mut auto, RichText::new("选中文件时自动分析频谱").size(10.5).color(MUTED))
                            .changed()
                        {
                            self.cfg.auto_analyze = auto;
                            changed = true;
                        }
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("关掉后不自动分析，省 CPU；每个文件约 1 秒")
                                .size(9.0)
                                .color(DIM),
                        );

                        ui.add_space(18.0);

                        // ── 关于 ──
                        section(ui, "关于");
                        egui::Grid::new("about")
                            .num_columns(2)
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                row(ui, "版本", "0.1.0");
                                row(ui, "字体", "Noto Sans SC（SIL OFL 1.1）");
                                row(ui, "音频解码", "ffmpeg 子进程");
                                row(ui, "频谱分析", "fc-analyze（自研）");
                                row(ui, "许可", "开源，详见仓库");
                            });
                    });
            });

        if changed {
            self.cfg.save();
        }
    }

    /// 底部全局播放条
    fn player_bar(&mut self, ui: &mut egui::Ui, name: &str) {
        ui.set_width(ui.available_width());

        let (playing, pos, dur, vol) = match self.player.as_ref() {
            Some(p) => (p.is_playing(), p.position_s(), p.duration_s().max(0.1), p.volume()),
            None => return,
        };

        let mut toggle = false;
        let mut stop = false;
        let mut seek: Option<f64> = None;
        let mut vol_change: Option<f32> = None;

        ui.horizontal(|ui| {
            if icon_button(ui, if playing { Icon::Pause } else { Icon::Play }, true) {
                toggle = true;
            }
            ui.add_space(2.0);
            if icon_button(ui, Icon::Stop, false) {
                stop = true;
            }
            ui.add_space(10.0);
            ui.add_sized(
                egui::vec2(190.0, 18.0),
                egui::Label::new(RichText::new(name).size(10.5).color(MUTED))
                    .wrap_mode(egui::TextWrapMode::Truncate),
            );
            ui.add_space(8.0);

            // 进度条：可点、可拖
            let bar_w = (ui.available_width() - 200.0).max(80.0);
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(bar_w, 12.0), egui::Sense::click_and_drag());
            let frac = ((pos / dur) as f32).clamp(0.0, 1.0);
            let p = ui.painter();
            p.rect_filled(rect, Rounding::same(6.0), tint(Color32::WHITE, 14));
            if frac > 0.0 {
                let mut f = rect;
                f.set_width(rect.width() * frac);
                p.rect_filled(f, Rounding::same(6.0), ACCENT);
            }
            if (resp.clicked() || resp.dragged()) && rect.width() > 0.0 {
                if let Some(pp) = resp.interact_pointer_pos() {
                    seek = Some(((pp.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * dur);
                }
            }

            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("{} / {}", fmt_duration(pos), fmt_duration(dur)))
                    .size(9.5)
                    .color(DIM),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut v = vol;
                ui.spacing_mut().slider_width = 70.0;
                if ui
                    .add(
                        egui::Slider::new(&mut v, 0.0..=1.0)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .changed()
                {
                    vol_change = Some(v);
                }
            });
        });

        if let Some(p) = self.player.as_mut() {
            if toggle {
                p.toggle();
            }
            if stop {
                p.stop();
            }
            if let Some(t2) = seek {
                p.seek(t2);
            }
            if let Some(v) = vol_change {
                p.set_volume(v);
            }
        }
    }

    fn detail(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let mut animating = false;

        // 切换选中项时，内容从下方滑入
        if self.detail_for != self.selected {
            self.detail_for = self.selected;
            self.detail_t = 0.0;
        }
        animating |= approach(&mut self.detail_t, 1.0, self.dt, 12.0);
        let t = self.detail_t;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                animating |= self.detail_body(ui, t);
            });

        animating
    }

    fn detail_body(&mut self, ui: &mut egui::Ui, t: f32) -> bool {
        let dt = self.dt;
        let mut animating = false;
        ui.set_width(ui.available_width());

        // 切换选中项时，内容从下方滑入 + 淡入
        if self.detail_for != self.selected {
            self.detail_for = self.selected;
            self.detail_t = 0.0;
        }
        animating |= approach(&mut self.detail_t, 1.0, dt, 12.0);
        let t = self.detail_t;
        ui.add_space(16.0 * (1.0 - t));

        let Some(it) = self.items.get(self.selected) else {
            ui.label(RichText::new("详情").size(10.5).color(DIM));
            ui.add_space(10.0);
            para(ui, "拖入文件后，这里显示源文件的真实参数、质量判断和解释。", 10.5, DIM);
            return animating;
        };

        let fade = |c: Color32| c.gamma_multiply(t);

        ui.label(RichText::new("详情").size(10.5).color(fade(DIM)));
        ui.add_space(8.0);
        let disp = it.tags.display_title(&it.name());
        ui.add(
            egui::Label::new(RichText::new(&disp).size(12.5).color(fade(TEXT)).strong())
                .wrap_mode(egui::TextWrapMode::Truncate),
        );
        let sub = it.tags.display_sub();
        if !sub.is_empty() {
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(RichText::new(&sub).size(9.5).color(fade(MUTED)))
                    .wrap_mode(egui::TextWrapMode::Truncate),
            );
        }
        // 文件名单独一行（老 .oma 的文件名本身可能就是线索）
        if disp != it.name() {
            ui.add_space(1.0);
            ui.add(
                egui::Label::new(
                    RichText::new(it.name()).size(9.0).color(fade(DIM)),
                )
                .wrap_mode(egui::TextWrapMode::Truncate),
            );
        }
        ui.add_space(10.0);

        if it.info.is_some() {
            para(ui, "双击队列里的文件即可试听", 9.5, DIM);
        }

        ui.add_space(10.0);

        if let Some(m) = &it.info {
            ui.horizontal(|ui| {
                chip(ui, m.codec_label(), ACCENT);
            });
            ui.add_space(10.0);

            egui::Grid::new("info_grid")
                .num_columns(2)
                .spacing([12.0, 7.0])
                .show(ui, |ui| {
                    if m.bitrate_kbps > 0 {
                        row(ui, "码率", &format!("{} kbps", m.bitrate_kbps));
                    }
                    if m.sample_rate > 0 {
                        row(ui, "采样率", &format!("{} Hz", m.sample_rate));
                    }
                    if m.channels > 0 {
                        row(ui, "声道", &format!("{}", m.channels));
                    }
                    if m.duration_s > 0.0 {
                        row(ui, "时长", &fmt_duration(m.duration_s));
                    }
                    if m.size_bytes > 0 {
                        row(ui, "体积", &fmt_size(m.size_bytes));
                    }
                    let spec = it.spec(self.target);
                    let est = estimate_size(m, spec);
                    if est > 0 {
                        row(ui, "输出", &format!("{} ≈ {}", spec.name, fmt_size(est)));
                    }
                });

            // ── 质量评估（频谱分析）──
            ui.add_space(14.0);
            match &it.analysis {
                Analysis::Idle | Analysis::Running => {
                    para(ui, "正在分析频谱…", 9.5, DIM);
                }
                Analysis::Failed(e) => {
                    card(ui, DANGER, |ui| {
                        para(ui, "频谱分析失败", 11.0, DANGER);
                        ui.add_space(4.0);
                        para(ui, e, 9.0, MUTED);
                    });
                }
                Analysis::Done(r) => {
                    para(ui, "质量评估", 9.5, DIM);
                    ui.add_space(6.0);
                    egui::Grid::new("quality_grid")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            row(
                                ui,
                                "截止频率",
                                &format!("{:.1} kHz", r.cutoff_hz / 1000.0),
                            );
                            let est = if !r.estimate.is_empty()
                                && r.estimate
                                    .chars()
                                    .all(|c| c.is_ascii_digit() || c == '~')
                            {
                                format!("{} kbps", r.estimate)
                            } else {
                                r.estimate.clone()
                            };
                            row(ui, "推测源质量", &est);

                            ui.label(RichText::new("置信度").size(9.5).color(DIM));
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(118.0, 10.0),
                                egui::Sense::hover(),
                            );
                            let p = ui.painter();
                            p.rect_filled(rect, Rounding::same(5.0), tint(Color32::WHITE, 14));
                            let mut f = rect;
                            f.set_width(rect.width() * (r.confidence as f32).clamp(0.0, 1.0));
                            p.rect_filled(
                                f,
                                Rounding::same(5.0),
                                if r.confidence > 0.6 { ACCENT } else { WARN },
                            );
                            ui.label(
                                RichText::new(format!("{:.0}%", r.confidence * 100.0))
                                    .size(9.0)
                                    .color(DIM),
                            );
                            ui.end_row();
                        });

                    if !r.caveats.is_empty() {
                        ui.add_space(8.0);
                        card(ui, WARN, |ui| {
                            for c in &r.caveats {
                                para(ui, &format!("· {c}"), 9.0, MUTED);
                                ui.add_space(2.0);
                            }
                        });
                    }
                }
            }

            let spec = it.spec(self.target);
            if m.is_lossy()
                && matches!(
                    spec.kind,
                    fc_core::Kind::Lossless | fc_core::Kind::Uncompressed
                )
            {
                ui.add_space(14.0);
                card(ui, WARN, |ui| {
                    para(ui, "不会增加任何信息量", 11.5, WARN);
                    ui.add_space(5.0);
                    para(
                        ui,
                        &format!(
                            "源文件是 {} 有损压缩。转成 {} 只是换了个更大的容器，文件会变大，声音不会变好。",
                            m.codec_label(),
                            spec.name
                        ),
                        10.0,
                        MUTED,
                    );
                });
            }
        }

        if let Some(e) = &it.error {
            ui.add_space(14.0);
            card(ui, DANGER, |ui| {
                para(ui, "读不出来", 11.5, DANGER);
                ui.add_space(5.0);
                para(ui, e, 9.5, MUTED);
            });
        }

        match &it.state {
            State::Done(path, size) => {
                ui.add_space(14.0);
                card(ui, ACCENT, |ui| {
                    para(ui, "转换完成", 11.5, ACCENT);
                    ui.add_space(5.0);
                    para(
                        ui,
                        &format!(
                            "{}　{}",
                            path.file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            fmt_size(*size)
                        ),
                        9.5,
                        MUTED,
                    );
                });
            }
            State::Failed(e) => {
                ui.add_space(14.0);
                card(ui, DANGER, |ui| {
                    para(ui, "转换失败", 11.5, DANGER);
                    ui.add_space(5.0);
                    para(ui, e, 9.5, MUTED);
                });
            }
            _ => {}
        }

        // 选中即分析（结果进缓存，切回来不重跑）
        if self.cfg.auto_analyze
            && self
                .items
                .get(self.selected)
                .map(|i| matches!(i.analysis, Analysis::Idle) && i.info.is_some())
                .unwrap_or(false)
        {
            self.start_analyze(self.selected, ui.ctx());
        }

        animating
    }


    /// 当前参数的一句话摘要，折叠时显示
    fn knob_summary(&self) -> String {
        use fc_core::Knob;
        match fc_core::knobs_for(self.target).first() {
            Some(Knob::FlacLevel) => format!("{}", self.cfg.opts.flac_level),
            Some(Knob::BitDepth) => self.cfg.opts.bit_depth.label().to_string(),
            Some(Knob::Mp3Bitrate) => format!("{}k", self.cfg.opts.mp3_bitrate),
            Some(Knob::OpusBitrate) => format!("{}k", self.cfg.opts.opus_bitrate),
            Some(Knob::VorbisQuality) => format!("q{}", self.cfg.opts.vorbis_q),
            _ => "默认".into(),
        }
    }

    /// 最右一列：输出设置。不属于某个文件，是全局的。
    fn output_panel(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let mut changed = false;

        ui.label(RichText::new("输出").size(11.0).color(DIM));
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{} · {}", self.target.name, self.knob_summary()))
                .size(12.0)
                .color(ACCENT),
        );
        ui.add_space(12.0);

        if false {
            return changed;
        }

        ui.add_space(6.0);

        // 预设
        ui.label(RichText::new("预设").size(9.0).color(DIM));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
            for p in fc_core::PRESETS {
                let on = self.target.id == p.format;
                let b = egui::Button::new(
                    RichText::new(p.name).size(9.5).color(if on {
                        ACCENT
                    } else {
                        MUTED
                    }),
                )
                .fill(if on {
                    tint(ACCENT, 20)
                } else {
                    tint(Color32::WHITE, 8)
                })
                .stroke(if on {
                    line(ACCENT, 45)
                } else {
                    line(Color32::WHITE, 16)
                })
                .rounding(Rounding::same(7.0));
                if ui.add(b).on_hover_text(p.note).clicked() {
                    if let Some(f) = fc_core::by_id(p.format) {
                        self.target = f;
                    }
                    (p.apply)(&mut self.cfg.opts);
                    changed = true;
                }
            }
        });

        ui.add_space(8.0);

        // 参数
        let knobs = fc_core::knobs_for(self.target);
        if knobs.is_empty() {
            para(ui, "这个格式没有可调参数。", 9.0, DIM);
        }
        for k in knobs {
            changed |= self.knob_row(ui, *k);
        }

        ui.add_space(14.0);
        ui.label(RichText::new("输出到").size(9.0).color(DIM));
        ui.add_space(4.0);
        if ui
            .radio_value(
                &mut self.cfg.output_mode,
                settings::OutputMode::Source,
                RichText::new("源文件同目录").size(10.0).color(MUTED),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .radio_value(
                &mut self.cfg.output_mode,
                settings::OutputMode::Custom,
                RichText::new("指定目录").size(10.0).color(MUTED),
            )
            .changed()
        {
            changed = true;
        }
        if self.cfg.output_mode == settings::OutputMode::Custom {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 56.0).max(56.0);
                let mut dir = self.cfg.output_dir.clone();
                if ui
                    .add_sized(
                        [w, 22.0],
                        egui::TextEdit::singleline(&mut dir).hint_text("选一个目录"),
                    )
                    .changed()
                {
                    self.cfg.output_dir = dir;
                    changed = true;
                }
                if ui
                    .add_sized([50.0, 22.0], egui::Button::new(RichText::new("选…").size(10.0)))
                    .clicked()
                {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.output_dir = p.to_string_lossy().to_string();
                        changed = true;
                    }
                }
            });
        }

        ui.add_space(12.0);
        ui.label(RichText::new("转换后").size(9.0).color(DIM));
        ui.add_space(4.0);
        if ui
            .radio_value(
                &mut self.cfg.delete_source,
                false,
                RichText::new("保留源文件").size(10.0).color(MUTED),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .radio_value(
                &mut self.cfg.delete_source,
                true,
                RichText::new("删除源文件").size(10.0).color(MUTED),
            )
            .changed()
        {
            changed = true;
        }
        if self.cfg.delete_source {
            ui.add_space(2.0);
            para(ui, "删了就找不回来了，谨慎。", 8.5, DANGER);
        }

        ui.add_space(14.0);
        // 折叠头：整行可点，右侧画三角
        let (hrect, hresp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 24.0),
            egui::Sense::click(),
        );
        let hp = ui.painter();
        if hresp.hovered() {
            hp.rect_filled(hrect, Rounding::same(6.0), tint(Color32::WHITE, 10));
        }
        hp.text(
            hrect.left_center() + egui::vec2(2.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "元数据",
            egui::FontId::proportional(9.5),
            DIM,
        );
        // 收起时显示已填了几项，不用展开也知道有没有设过
        let filled = [
            self.cfg.meta_artist.trim(),
            self.cfg.meta_album.trim(),
            self.cfg.meta_cover.trim(),
            self.items
                .get(self.selected)
                .and_then(|i| i.title_ov.as_deref())
                .unwrap_or(""),
        ]
        .iter()
        .filter(|v| !v.is_empty())
        .count();
        if filled > 0 {
            hp.text(
                hrect.left_center() + egui::vec2(44.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{} 项已填", filled),
                egui::FontId::proportional(9.0),
                ACCENT,
            );
        }
        let tc = hrect.right_center() - egui::vec2(9.0, 0.0);
        let tri = if self.meta_open {
            vec![
                tc + egui::vec2(-4.0, 2.0),
                tc + egui::vec2(4.0, 2.0),
                tc + egui::vec2(0.0, -3.0),
            ]
        } else {
            vec![
                tc + egui::vec2(-4.0, -2.0),
                tc + egui::vec2(4.0, -2.0),
                tc + egui::vec2(0.0, 3.0),
            ]
        };
        hp.add(egui::Shape::convex_polygon(tri, DIM, Stroke::NONE));
        if hresp.clicked() {
            self.meta_open = !self.meta_open;
        }
        ui.add_space(2.0);

        if self.meta_open {
            if ui
                .checkbox(
                    &mut self.cfg.write_tags,
                    RichText::new("写入标签").size(10.0).color(MUTED),
                )
                .changed()
            {
                changed = true;
            }
        }

        if self.meta_open && self.cfg.write_tags {
            let sel = self.selected;
            let has_file = self.items.get(sel).is_some();

            // ── 当前文件（只影响这一个）──
            if has_file {
                let mut title = self
                    .items
                    .get(sel)
                    .and_then(|i| i.title_ov.clone())
                    .unwrap_or_default();
                ui.add_space(6.0);
                ui.label(RichText::new("标题（当前文件）").size(9.0).color(DIM));
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut title)
                            .hint_text("留空=用源文件标签")
                            .margin(egui::vec2(6.0, 4.0)),
                    )
                    .changed()
                {
                    if let Some(it) = self.items.get_mut(sel) {
                        it.title_ov = if title.trim().is_empty() {
                            None
                        } else {
                            Some(title.clone())
                        };
                    }
                    changed = true;
                }

                let mut track = self
                    .items
                    .get(sel)
                    .and_then(|i| i.track_ov.clone())
                    .unwrap_or_default();
                ui.add_space(4.0);
                ui.label(RichText::new("音轨号").size(9.0).color(DIM));
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut track)
                            .hint_text("如 03")
                            .margin(egui::vec2(6.0, 4.0)),
                    )
                    .changed()
                {
                    if let Some(it) = self.items.get_mut(sel) {
                        it.track_ov = if track.trim().is_empty() {
                            None
                        } else {
                            Some(track.clone())
                        };
                    }
                    changed = true;
                }
            }

            // ── 全批（影响所有文件）──
            ui.add_space(8.0);
            ui.label(RichText::new("艺术家（全部）").size(9.0).color(DIM));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.cfg.meta_artist)
                        .hint_text("留空=用源文件标签")
                        .margin(egui::vec2(6.0, 4.0)),
                )
                .changed()
            {
                changed = true;
            }
            ui.add_space(4.0);
            ui.label(RichText::new("专辑（全部）").size(9.0).color(DIM));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.cfg.meta_album)
                        .hint_text("留空=用源文件标签")
                        .margin(egui::vec2(6.0, 4.0)),
                )
                .changed()
            {
                changed = true;
            }

            ui.add_space(6.0);
            ui.label(RichText::new("封面（全部）").size(9.0).color(DIM));
            ui.horizontal(|ui| {
                let name = std::path::Path::new(&self.cfg.meta_cover)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "沿用源文件".into());
                ui.label(RichText::new(name).size(9.0).color(DIM));
            });
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [(ui.available_width() - 46.0).max(50.0), 22.0],
                        egui::Button::new(RichText::new("选图片…").size(9.5)),
                    )
                    .clicked()
                {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("图片", &["jpg", "jpeg", "png", "webp"])
                        .pick_file()
                    {
                        self.cfg.meta_cover = p.to_string_lossy().to_string();
                        changed = true;
                    }
                }
                if ui
                    .add_sized([42.0, 22.0], egui::Button::new(RichText::new("清除").size(9.0)))
                    .clicked()
                {
                    self.cfg.meta_cover.clear();
                    changed = true;
                }
            });
            para(ui, "封面只写进 FLAC/MP3/M4A。", 8.5, DIM);
        }

        ui.add_space(8.0);
        let est = self
            .items
            .iter()
            .filter_map(|i| i.info.as_ref())
            .map(|m| fc_core::estimate_size(m, self.target))
            .sum::<u64>();
        if est > 0 {
            para(ui, &format!("预计输出 {}", fmt_size(est)), 9.0, DIM);
        }

        changed
    }

    /// 一个参数控件（紧凑版，300px 宽的面板里能用）
    fn knob_row(&mut self, ui: &mut egui::Ui, k: fc_core::Knob) -> bool {
        use fc_core::{BitDepth, Knob};
        let mut changed = false;
        match k {
            Knob::FlacLevel => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("压缩等级").size(9.5).color(MUTED));
                    let mut v = self.cfg.opts.flac_level as i32;
                    ui.spacing_mut().slider_width = 110.0;
                    if ui
                        .add(egui::Slider::new(&mut v, 0..=12).show_value(false))
                        .changed()
                    {
                        self.cfg.opts.flac_level = v as u8;
                        changed = true;
                    }
                    ui.label(RichText::new(format!("{v}")).size(9.0).color(DIM));
                });
            }
            Knob::VorbisQuality => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("质量").size(9.5).color(MUTED));
                    let mut v = self.cfg.opts.vorbis_q as i32;
                    ui.spacing_mut().slider_width = 140.0;
                    if ui
                        .add(egui::Slider::new(&mut v, 0..=10).show_value(false))
                        .changed()
                    {
                        self.cfg.opts.vorbis_q = v as u8;
                        changed = true;
                    }
                    ui.label(RichText::new(format!("{v}")).size(9.0).color(DIM));
                });
            }
            Knob::BitDepth => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("位深").size(9.5).color(MUTED));
                    for (d, label) in [
                        (BitDepth::Int16, "16"),
                        (BitDepth::Int24, "24"),
                        (BitDepth::Int32f, "32f"),
                    ] {
                        let on = self.cfg.opts.bit_depth == d;
                        if seg_button(ui, label, on) {
                            self.cfg.opts.bit_depth = d;
                            changed = true;
                        }
                    }
                });
            }
            Knob::Mp3Bitrate => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("码率").size(9.5).color(MUTED));
                    for v in [128u32, 192, 256, 320] {
                        let on = self.cfg.opts.mp3_bitrate == v;
                        if seg_button(ui, &format!("{v}"), on) {
                            self.cfg.opts.mp3_bitrate = v;
                            changed = true;
                        }
                    }
                });
            }
            Knob::OpusBitrate => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("码率").size(9.5).color(MUTED));
                    for v in [64u32, 96, 128, 192] {
                        let on = self.cfg.opts.opus_bitrate == v;
                        if seg_button(ui, &format!("{v}"), on) {
                            self.cfg.opts.opus_bitrate = v;
                            changed = true;
                        }
                    }
                });
            }
        }
        changed
    }
}

/// 可点击跳转的进度条
fn seek_bar(ui: &mut egui::Ui, pos: f64, dur: f64) -> Option<f64> {
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

#[derive(Clone, Copy, PartialEq)]
enum TitleBtn {
    Pin,
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// 标题栏按钮。图标全部用 painter 画，不依赖字体。
fn title_button(ui: &mut egui::Ui, rect: egui::Rect, kind: TitleBtn, active: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(("tbtn", kind as u8)), egui::Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        let bg = if kind == TitleBtn::Close {
            Color32::from_rgb(196, 60, 60)
        } else {
            tint(Color32::WHITE, 26)
        };
        p.rect_filled(rect, Rounding::same(6.0), bg);
    }
    let c = rect.center();
    let col = if resp.hovered() && kind == TitleBtn::Close {
        Color32::WHITE
    } else if active {
        ACCENT
    } else {
        MUTED
    };
    match kind {
        TitleBtn::Minimize => {
            p.line_segment(
                [c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)],
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Maximize => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)),
                Rounding::same(2.0),
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Restore => {
            // 前面那个窗口
            let front = egui::Rect::from_min_max(
                c + egui::vec2(-4.5, -2.5),
                c + egui::vec2(3.5, 5.5),
            );
            p.rect_stroke(front, Rounding::same(2.0), Stroke::new(1.2, col));
            // 后面那个窗口：只露出上边和右边，构成 L 形
            let back_tl = c + egui::vec2(-2.0, -5.0);
            let back_br = c + egui::vec2(5.0, 2.0);
            p.line_segment(
                [back_tl, egui::pos2(back_br.x, back_tl.y)],
                Stroke::new(1.2, col),
            );
            p.line_segment(
                [
                    egui::pos2(back_br.x - 0.5, back_tl.y),
                    egui::pos2(back_br.x - 0.5, back_br.y),
                ],
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Close => {
            // 两条对角线才是 ×。之前写成 for d in [-4, 4] 再 (-d,-d)→(d,d)，
            // 两轮画的是同一条线，看起来只有一个反斜杠。
            p.line_segment(
                [c + egui::vec2(-4.0, -4.0), c + egui::vec2(4.0, 4.0)],
                Stroke::new(1.2, col),
            );
            p.line_segment(
                [c + egui::vec2(-4.0, 4.0), c + egui::vec2(4.0, -4.0)],
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Pin => {
            // 简化图钉：一个圆 + 一根针；固定时实心
            let head = c + egui::vec2(0.0, -2.0);
            if active {
                p.circle_filled(head, 3.6, col);
            } else {
                p.circle_stroke(head, 3.6, Stroke::new(1.2, col));
            }
            p.line_segment(
                [head + egui::vec2(0.0, 3.8), head + egui::vec2(0.0, 8.0)],
                Stroke::new(1.2, col),
            );
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

#[derive(Clone, Copy)]
enum Icon {
    Play,
    Pause,
    Stop,
}

/// 播放控制图标。不用字符（字体里没有 ⏸⏹），直接用 painter 画。
fn icon_button(ui: &mut egui::Ui, icon: Icon, accent: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
    let p = ui.painter();
    let bg = if resp.hovered() {
        tint(Color32::WHITE, 22)
    } else {
        tint(Color32::WHITE, 8)
    };
    p.rect_filled(rect, Rounding::same(8.0), bg);

    let c = rect.center();
    let col = if accent { ACCENT } else { TEXT };
    match icon {
        Icon::Play => {
            let tri = vec![
                c + egui::vec2(-4.0, -6.5),
                c + egui::vec2(-4.0, 6.5),
                c + egui::vec2(7.0, 0.0),
            ];
            p.add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
        }
        Icon::Pause => {
            for dx in [-4.5_f32, 1.5] {
                p.rect_filled(
                    egui::Rect::from_min_size(c + egui::vec2(dx, -6.5), egui::vec2(3.0, 13.0)),
                    Rounding::same(1.0),
                    col,
                );
            }
        }
        Icon::Stop => {
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(11.0, 11.0)),
                Rounding::same(2.0),
                col,
            );
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// 队列行左边的封面缩略图。没有封面就给一个占位方块。
fn cover_thumb(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    match tex {
        Some(t) => {
            p.image(
                t.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            p.rect_stroke(rect, Rounding::same(0.0), line(Color32::WHITE, 26));
        }
        None => {
            p.rect_filled(rect, Rounding::same(4.0), tint(Color32::WHITE, 9));
            // 中间画一个音符形状太费事，用一个小方块占位
            p.rect_filled(
                egui::Rect::from_center_size(rect.center(), egui::vec2(10.0, 10.0)),
                Rounding::same(2.0),
                tint(Color32::WHITE, 22),
            );
        }
    }
}

/// 分段选择按钮（位深 / 码率这类互斥选项）
fn seg_button(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
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
fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(10.0).color(DIM));
    ui.add_space(6.0);
}

fn card(ui: &mut egui::Ui, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(tint(color, 16))
        .stroke(line(color, 50))
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(13.0))
        .show(ui, add);
}

fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
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
fn para(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    ui.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
}

fn row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.label(RichText::new(key).size(9.5).color(DIM));
    ui.add(
        egui::Label::new(RichText::new(value).size(10.0).color(TEXT))
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
    ui.end_row();
}

fn install_fonts(ctx: &egui::Context) {
    // 内嵌的 Noto Sans SC 子集（SIL OFL）。
    // 只含界面用到的字，8.1 MB 压到 169 KB，由 tools/subset-font.py 生成。
    // 改完 UI 文案记得重跑一次，否则新字会落到系统字体上，字形不一致。
    const UI_FONT: &[u8] = include_bytes!("../assets/NotoSansSC-Subset.otf");

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("ui".to_owned(), egui::FontData::from_static(UI_FONT));

    // 系统字体做兜底：用户的文件名里可能有子集里没有的罕见字
    let mut fallback = false;
    for path in [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("fallback".to_owned(), egui::FontData::from_owned(bytes));
            fallback = true;
            break;
        }
    }

    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(fam).or_default();
        list.insert(0, "ui".to_owned());
        if fallback {
            list.push("fallback".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// 只设深色标题栏。毛玻璃走的是壁纸方案（见 bg.rs），不碰 DWM。
#[cfg(windows)]
fn apply_backdrop() -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    let title: Vec<u16> = "格式转换器\0".encode_utf16().collect();
    unsafe {
        let hwnd: HWND = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let dark: i32 = 1;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const _,
            4,
        ) == 0
    }
}

#[cfg(not(windows))]
fn apply_backdrop() -> bool {
    true
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.first().map(|s| s.as_str()) == Some("--convert") {
        let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
            eprintln!("用法: format-converter --convert <输入> <输出.flac>");
            std::process::exit(2);
        };
        let tools = fc_core::locate().expect("找不到 ffmpeg / ffprobe");
        let info = match fc_core::probe(&tools, Path::new(input)) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("无法读取: {e:#}");
                std::process::exit(1);
            }
        };
        println!(
            "输入: {} / {} kbps / {} Hz / {:.1}s",
            info.codec_label(),
            info.bitrate_kbps,
            info.sample_rate,
            info.duration_s
        );
        let args = fc_core::args_for(
            fc_core::default_target(),
            &fc_core::ConvertOptions::default(),
        );
        let arg_refs: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
        if let Err(e) = fc_core::convert(
            &tools,
            Path::new(input),
            Path::new(output),
            &arg_refs,
            info.duration_s,
            Box::new(|_| {}),
        ) {
            eprintln!("转换失败: {e:#}");
            std::process::exit(1);
        }
        println!("完成: {output}");
        return Ok(());
    }

    // 命令行：打印标签与封面信息（调试用）
    if args.first().map(|s| s.as_str()) == Some("--tags") {
        let Some(input) = args.get(1) else {
            eprintln!("用法: format-converter --tags <文件>");
            std::process::exit(2);
        };
        let tools = fc_core::locate().expect("找不到 ffprobe");
        let path = Path::new(input);
        let t = fc_core::read_tags(&tools.ffprobe, path);
        println!("标题: {}", t.display_title("(无)"));
        println!("副标题: {}", if t.display_sub().is_empty() { "(无)".into() } else { t.display_sub() });
        println!("封面: {}", if fc_core::has_cover(&tools.ffprobe, path) { "有" } else { "无" });
        if let Some(bytes) = fc_core::extract_cover(&tools.ffmpeg, path) {
            println!("封面字节: {}（{}）", bytes.len(),
                if bytes.starts_with(&[0xFF, 0xD8]) { "JPEG" } else { "PNG" });
        }
        return Ok(());
    }

    // 命令行：只做加密识别，不开界面
    if args.first().map(|s| s.as_str()) == Some("--identify") {
        let Some(input) = args.get(1) else {
            eprintln!("用法: format-converter --identify <文件>");
            std::process::exit(2);
        };
        let path = Path::new(input);
        match fc_core::identify(path) {
            Some(e) => {
                println!("{}", e.summary());
                println!();
                println!("{}", e.full_text());
            }
            None => println!("不是加密文件（或不在已知格式库里）"),
        }
        return Ok(());
    }

    if args.first().map(|s| s.as_str()) == Some("--play") {
        let Some(input) = args.get(1) else {
            eprintln!("用法: format-converter --play <文件> [秒数]");
            std::process::exit(2);
        };
        let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4.0);
        let tools = fc_core::locate().expect("找不到 ffmpeg");
        let info = match fc_core::probe(&tools, Path::new(input)) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("无法读取: {e:#}");
                std::process::exit(1);
            }
        };
        println!(
            "试听: {} / {} kbps / {:.1}s",
            info.codec_label(),
            info.bitrate_kbps,
            info.duration_s
        );
        let mut pl = match fc_player::Player::open(&tools.ffmpeg, Path::new(input), info.duration_s)
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("打开音频输出失败: {e:#}");
                std::process::exit(1);
            }
        };
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            std::thread::sleep(std::time::Duration::from_millis(500));
            println!("  位置 {:.2}s / {:.2}s", pl.position_s(), info.duration_s);
        }
        pl.pause();
        println!("ok");
        return Ok(());
    }

    let preload: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();

    // 窗口图标（任务栏/Alt-Tab 用的），和 exe 里嵌的是同一张图
    let icon = {
        const PNG: &[u8] = include_bytes!("../assets/icon.png");
        image::load_from_memory(PNG).ok().map(|img| {
            let rgba = img.to_rgba8();
            egui::IconData {
                rgba: rgba.as_raw().clone(),
                width: rgba.width(),
                height: rgba.height(),
            }
        })
    };

    let mut viewport = egui::ViewportBuilder::default()
            .with_inner_size([1302.0, 700.0])
            .with_min_inner_size([1212.0, 600.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_title("格式转换器");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "格式转换器",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc);
            if !preload.is_empty() {
                app.add_files(preload);
            }
            Ok(Box::new(app))
        }),
    )
}
