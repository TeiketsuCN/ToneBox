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

/// 左侧两个大项：音乐库 / 输出（转换）
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Convert,
    Library,
}

/// 音乐库里的分类
#[derive(Clone, Copy, PartialEq, Eq)]
enum LibTab {
    Songs,
    Albums,
    Genres,
    Artists,
    Folders,
}

/// 扫描出来的一首歌（只放界面要用的字段）
/// 编码名 -> 是否有损（老缓存没有 lossy 字段时用）
fn label_is_lossy(label: &str) -> bool {
    matches!(
        label,
        "MP3" | "AAC" | "Vorbis" | "Opus" | "WMA" | "AC-3" | "E-AC-3" | "MP2" | "ATRAC1"
            | "ATRAC3" | "ATRAC3+" | "Musepack" | "Speex"
    )
}

#[derive(Clone)]
struct LibEntry {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    genre: String,
    duration_s: f64,
    codec: String,
    bits: Option<u32>,
    sample_rate: u32,
    bitrate_kbps: u64,
    lossy: bool,
}

enum LibMsg {
    /// 先告诉界面一共多少个文件，进度条才有分母
    Count(usize),
    Entry(LibEntry),
    Done,
}

// ── 配色 ────────────────────────────────────────────────
const TEXT: Color32 = Color32::from_rgb(240, 244, 250);
const MUTED: Color32 = Color32::from_rgb(158, 168, 186);
const DIM: Color32 = Color32::from_rgb(112, 122, 140);
const ACCENT: Color32 = Color32::from_rgb(122, 226, 214);
const WARN: Color32 = Color32::from_rgb(255, 198, 122);
const DANGER: Color32 = Color32::from_rgb(255, 138, 138);
const BG: Color32 = Color32::from_rgb(14, 16, 22);

// ── 品牌 ────────────────────────────────────────────────
/// 应用名：窗口标题 / 任务栏 / 标题栏都用它。改名只改这一行
/// （配置目录还叫 FormatConverter，改了会丢用户数据，等真换名再一起迁）。
pub const APP_NAME: &str = "ToneBox";

/// 界面基准缩放：旧版 120% 现在就是 100%。所有倍率都乘在它上面
pub const BASE_ZOOM: f32 = 1.2;

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

/// 一个转换任务。ATRAC 走外部工具，参数和 ffmpeg 完全不一样，所以分成两种。
enum Job {
    /// ffmpeg 一把梭（绝大多数格式）
    Ffmpeg {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        args: Vec<String>,
        dur: f64,
    },
    /// 外部 atracdenc：先 ffmpeg 解码成 WAV，再交给它编码
    Atrac {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        codec: &'static str,
        dur: f64,
    },
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
    /// 播放音量（没有播放器时也记住，下次 open 直接套上）
    player_volume: f32,
    /// 正在看大图的队列下标（None = 没开）
    cover_view: Option<usize>,
    /// 大图开/关的动画进度 0~1
    cover_t: f32,
    /// 正在关闭：等 cover_t 归零再真正清掉
    cover_closing: bool,
    /// 点击后从文件重新抽的大图封面（96px 的缩略图放大看太糊）
    cover_big: Option<(usize, egui::TextureHandle)>,
    cover_big_tx: std::sync::mpsc::Sender<(usize, Option<(Vec<u8>, u32, u32)>)>,
    cover_big_rx: Receiver<(usize, Option<(Vec<u8>, u32, u32)>)>,
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
    /// 捐赠收款码（放在程序目录，runtime 读取，改名即可换）
    donate_wechat: Option<egui::TextureHandle>,
    donate_alipay: Option<egui::TextureHandle>,
    /// 当前 ffmpeg 实际带哪些编码器（开机问一次）
    encoders: std::collections::HashSet<String>,
    /// 外部 ATRAC 编码器（atracdenc）。用户自己装，我们不随包分发。
    atracdenc: Option<PathBuf>,
    /// 格式百科
    wiki_open: bool,
    wiki_sel: &'static str,
    wiki_filter: String,
    /// 当前页：转换 / 音乐库
    page: Page,
    lib_tab: LibTab,
    lib_entries: Vec<LibEntry>,
    lib_scanning: bool,
    lib_total: usize,
    lib_status: String,
    lib_filter: String,
    lib_selected: Option<PathBuf>,
    /// 上一次点击（路径, 时刻），自己判双击
    lib_last_click: (Option<PathBuf>, f64),
    lib_tx: std::sync::mpsc::Sender<LibMsg>,
    lib_rx: Receiver<LibMsg>,
    /// 音乐库封面缓存（懒加载：滚到哪加载哪）
    lib_covers: std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>,
    lib_cover_pending: std::collections::HashSet<PathBuf>,
    lib_cover_tx: std::sync::mpsc::Sender<(PathBuf, Option<(Vec<u8>, u32, u32)>)>,
    lib_cover_rx: Receiver<(PathBuf, Option<(Vec<u8>, u32, u32)>)>,
    lib_cover_seq: u64,
    /// 频谱视图：开/关、算没算完、曲线、错误
    spec_open: bool,
    spec_busy: bool,
    spec_curve: Vec<(f64, f64)>,
    spec_for: Option<usize>,
    spec_err: Option<String>,
    spec_tx: std::sync::mpsc::Sender<Result<Vec<(f64, f64)>, String>>,
    spec_rx: Receiver<Result<Vec<(f64, f64)>, String>>,
    /// 正在重命名的预设下标 + 输入缓冲
    preset_rename: Option<usize>,
    preset_rename_buf: String,
    preset_rename_focus: bool,
    /// 正在重命名的配置下标 + 输入缓冲
    rename_idx: Option<usize>,
    rename_buf: String,
    rename_focus: bool,
    /// 导入/导出的结果提示
    profile_msg: String,
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
        // 输入框：默认的深底 + 亮边框在玻璃背景上很脏，这里统一成"一块浅玻璃"。
        // 内边距也在 TextEdit 自己的 margin 里设，别指望 spacing。
        style.visuals.extreme_bg_color = tint(Color32::WHITE, 16);
        style.visuals.selection.bg_fill = tint(ACCENT, 90);
        style.visuals.selection.stroke = Stroke::new(1.0, TEXT);
        for w in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            w.rounding = Rounding::same(8.0);
        }
        style.visuals.widgets.inactive.bg_stroke = line(Color32::WHITE, 26);
        style.visuals.widgets.hovered.bg_stroke = line(ACCENT, 90);
        style.visuals.widgets.active.bg_stroke = line(ACCENT, 130);
        cc.egui_ctx.set_style(style);

        let target = fc_core::default_target();
        let (atx, arx) = channel();
        let (mtx, mrx) = channel();
        let (btx, brx) = channel();
        let (ltx, lrx) = channel();
        let (cvt, cvr) = channel();
        let (spt, spr) = channel();
        let tools = fc_core::locate();
        let encoders = tools
            .as_ref()
            .map(|t| fc_core::list_encoders(&t.ffmpeg))
            .unwrap_or_default();
        let cfg = settings::Settings::load();
        // 界面缩放在建窗口时就要生效，否则第一帧会闪一下原尺寸
        cc.egui_ctx.set_zoom_factor(BASE_ZOOM * cfg.ui_scale);
        set_hdr(cfg.hdr);
        // 音乐库上次扫描的结果直接从配置里铺出来（不再每次启动都扫一遍）
        let cached_lib: Vec<LibEntry> = cfg
            .library_cache
            .iter()
            .map(|c| LibEntry {
                path: PathBuf::from(&c.path),
                title: c.title.clone(),
                artist: c.artist.clone(),
                album: c.album.clone(),
                genre: c.genre.clone(),
                duration_s: c.duration_s,
                codec: c.codec.clone(),
                bits: c.bits,
                sample_rate: c.sample_rate,
                bitrate_kbps: c.bitrate_kbps,
                // 老缓存没有 lossy 字段：按编码名兜底，别把 MP3 当成无损
                lossy: c.lossy || label_is_lossy(&c.codec),
            })
            .collect();
        let cached_status = if cached_lib.is_empty() {
            "还没有扫描".to_string()
        } else {
            format!("上次扫描：{} 首", cached_lib.len())
        };
        App {
            tools,
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
            player_volume: 0.75,
            cover_view: None,
            cover_t: 0.0,
            cover_closing: false,
            cover_big: None,
            cover_big_tx: btx,
            cover_big_rx: brx,
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
            cfg,
            settings_open: false,
            settings_t: 0.0,
            meta_open: true,
            donate_wechat: load_qr(&cc.egui_ctx, "donate-wechat.png"),
            donate_alipay: load_qr(&cc.egui_ctx, "donate-alipay.png"),
            encoders,
            atracdenc: fc_core::find_atracdenc(),
            wiki_open: false,
            wiki_sel: "flac",
            wiki_filter: String::new(),
            page: Page::Convert,
            lib_tab: LibTab::Songs,
            lib_entries: cached_lib,
            lib_scanning: false,
            lib_total: 0,
            lib_status: cached_status,
            lib_filter: String::new(),
            lib_selected: None,
            lib_last_click: (None, -10.0),
            lib_tx: ltx,
            lib_rx: lrx,
            lib_covers: std::collections::HashMap::new(),
            lib_cover_pending: std::collections::HashSet::new(),
            lib_cover_tx: cvt,
            lib_cover_rx: cvr,
            lib_cover_seq: 0,
            spec_open: false,
            spec_busy: false,
            spec_curve: Vec::new(),
            spec_for: None,
            spec_err: None,
            spec_tx: spt,
            spec_rx: spr,
            preset_rename: None,
            preset_rename_buf: String::new(),
            preset_rename_focus: false,
            rename_idx: None,
            rename_buf: String::new(),
            rename_focus: false,
            profile_msg: String::new(),
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
        let atracdenc = self.atracdenc.clone();
        let atrac3_kbps = self.cfg.opts.atrac3_bitrate;
        let mut jobs: Vec<Job> = Vec::new();
        for (i, it) in self.items.iter_mut().enumerate() {
            if it.info.is_none() || !matches!(it.state, State::Pending) {
                continue;
            }
            let spec = it.spec(self.target);
            let out = unique_output_in(&it.path, spec.ext, &self.cfg);
            let dur = it.info.as_ref().map(|m| m.duration_s).unwrap_or(0.0);

            // ATRAC 走外部编码器：ffmpeg 只负责解码，标签/封面写不进去，直接分派
            if let Some(codec) = fc_core::external::codec_for(spec.id, atrac3_kbps) {
                jobs.push(Job::Atrac {
                    idx: i,
                    input: it.path.clone(),
                    out,
                    codec,
                    dur,
                });
                it.state = State::Running(0.0);
                continue;
            }

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

            jobs.push(Job::Ffmpeg {
                idx: i,
                input: it.path.clone(),
                out,
                args,
                dur,
            });
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
            for job in jobs {
                let (idx, out) = match &job {
                    Job::Ffmpeg { idx, out, .. } => (*idx, out.clone()),
                    Job::Atrac { idx, out, .. } => (*idx, out.clone()),
                };
                let res = match job {
                    Job::Ffmpeg {
                        idx,
                        input,
                        out,
                        args,
                        dur,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        let arg_refs: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
                        fc_core::ffmpeg::convert_with_inputs(
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
                        )
                    }
                    Job::Atrac {
                        idx,
                        input,
                        out,
                        codec,
                        dur,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match atracdenc.as_ref() {
                            Some(exe) => fc_core::external::convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                codec,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(
                                "找不到 atracdenc。ATRAC 输出需要你自己装它（设置页有说明）。"
                            )),
                        }
                    }
                };
                match res {
                    Ok(size) => {
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
                // 接着上次的音量，不要每次开新文件都跳回默认
                p.set_volume(self.player_volume);
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
            frame::install(APP_NAME);
        }
        self.dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.05);
        // 命中区要知道当前缩放（120% 时按钮在物理像素上更大）
        #[cfg(windows)]
        frame::set_ui_scale(BASE_ZOOM * self.cfg.ui_scale);
        self.poll();
        self.poll_analysis();
        self.poll_media(ctx);
        self.poll_big_cover(ctx);
        self.poll_library();
        self.poll_lib_covers(ctx);
        self.poll_spectrum();
        // F2 = 看频谱（按钮之外的快捷方式）
        if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.open_spectrum();
        }

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

        // 设置页打开时把主界面藏起来：设置页是半透明玻璃，
        // 底下只该透出壁纸，不能透出队列和格式列表（会很脏）。
        let main_visible = !self.settings_open;
        let mut do_start = false;

        if main_visible {
            egui::SidePanel::left("rail").show_separator_line(false)
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
                .show(ctx, |ui| animating |= self.left_rail(ui));

            // 输出列只在"输出"页出现；"音乐库"页把整块宽度留给列表
            if self.page == Page::Convert {
                egui::SidePanel::right("output").show_separator_line(false)
                    .exact_width(OUT_W)
                    .resizable(false)
                    .frame(
                        egui::Frame::none()
                            .fill(tint(BG, 128))
                            .inner_margin(egui::Margin::symmetric(14.0, 12.0)),
                    )
                    .show(ctx, |ui| {
                        // 开始转换钉在最下面——这一列的终点就是它
                        egui::TopBottomPanel::bottom("out_start").resizable(false).show_separator_line(false)
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
                                    && self.items.iter().any(|i| {
                                        i.info.is_some() && matches!(i.state, State::Pending)
                                    });
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
            }

            // 播放栏：常驻。做成"浮起来的一块玻璃"，不要贴着窗口底边
            egui::TopBottomPanel::bottom("player").resizable(false).show_separator_line(false)
                .exact_height(80.0)
                .frame(egui::Frame::none().inner_margin(egui::Margin {
                    left: 14.0,
                    right: 14.0,
                    top: 2.0,
                    bottom: 10.0,
                }))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(tint(BG, 176))
                        .rounding(Rounding::same(14.0))
                        .shadow(egui::epaint::Shadow {
                            offset: egui::vec2(0.0, 3.0),
                            blur: 14.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(70),
                        })
                        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| self.player_bar(ui));
                });

            if self.page == Page::Convert {
                egui::SidePanel::right("detail").show_separator_line(false)
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
            }

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
                .show(ctx, |ui| {
                    if self.page == Page::Convert {
                        animating |= self.queue(ui, ctx);
                    } else {
                        animating |= self.library_view(ui, ctx);
                    }
                });
        }
        if do_start {
            self.start_batch(ctx);
        }

        // 大图封面：开/关都带缩放动画（用户要的"放大变小"）
        let cover_target = if self.cover_view.is_some() && !self.cover_closing {
            1.0
        } else {
            0.0
        };
        animating |= approach(&mut self.cover_t, cover_target, self.dt, 16.0);
        if self.cover_closing && self.cover_t <= 0.002 {
            self.cover_view = None;
            self.cover_closing = false;
        }
        if self.cover_view.is_some() {
            self.cover_overlay(ctx);
        }
        // 频谱：在封面之上、设置/百科之下
        if self.spec_open {
            self.spectrum_overlay(ctx);
        }

        // 设置覆盖层（最后画，盖住一切）
        animating |= approach(
            &mut self.settings_t,
            if self.settings_open { 1.0 } else { 0.0 },
            self.dt,
            16.0,
        );
        if self.wiki_open {
            self.wiki_page(ctx);
        }
        if self.settings_t > 0.001 {
            self.settings_panel(ctx);
        }

        // 扫描 / 封面还在后台跑时，界面得自己醒过来取结果，
        // 否则没鼠标事件就一直停在占位图上
        if self.spec_busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.lib_scanning || !self.lib_cover_pending.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
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
    if can {
        let r = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(ui.available_width(), 36.0),
        );
        accent_glow(ui.painter(), r, Rounding::same(10.0), 70);
    }
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

/// 44100 → "44.1k"，48000 → "48k"，192000 → "192k"
fn fmt_rate(hz: u32) -> String {
    if hz % 1000 == 0 {
        format!("{}k", hz / 1000)
    } else {
        format!("{:.1}k", hz as f64 / 1000.0)
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

        // 左边：logo + 名字（原来的两行文字去掉）
        let logo = egui::Rect::from_min_size(
            egui::pos2(full.left() + 9.0, full.center().y - 12.0),
            egui::vec2(24.0, 24.0),
        );
        accent_glow(ui.painter(), logo, Rounding::same(7.0), 80);
        ui.painter()
            .rect_filled(logo, Rounding::same(7.0), ACCENT);
        // 里面三条"电平条"：一眼能看出是音频工具
        for (fx, fh) in [(0.30, 0.40), (0.50, 0.64), (0.70, 0.30)] {
            let bx = logo.left() + logo.width() * fx;
            let h = logo.height() * fh;
            ui.painter().rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(bx, logo.center().y),
                    egui::vec2(2.6, h),
                ),
                Rounding::same(1.3),
                BG,
            );
        }
        ui.painter().text(
            egui::pos2(logo.right() + 10.0, full.center().y),
            egui::Align2::LEFT_CENTER,
            APP_NAME,
            egui::FontId::proportional(13.5),
            TEXT,
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
        // 设置：放在图钉左边（用户要求"放置顶按钮旁边"）。再点一次就收起来。
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, TitleBtn::Settings, self.settings_open) {
            self.settings_open = !self.settings_open;
        }
    }

    /// 左侧导航：两个大项（音乐库 / 输出），点开哪个就显示哪一块
    fn left_rail(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let mut animating = false;

        if rail_big(ui, "音乐库", self.page == Page::Library) {
            self.page = Page::Library;
            // 不自动扫描：结果在配置里，用户点「重新扫描」才更新
            if self.lib_entries.is_empty()
                && !self.lib_scanning
                && (!self.cfg.library_dirs.is_empty() || !self.cfg.library_files.is_empty())
            {
                self.lib_status = "点「重新扫描」扫一遍音乐库".into();
            }
        }
        if rail_big(ui, "输出", self.page == Page::Convert) {
            self.page = Page::Convert;
        }
        ui.add_space(10.0);

        // 用户配置：钉在最下面，两页都在
        egui::TopBottomPanel::bottom("rail_profiles").resizable(false).show_separator_line(false)
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(0.0, 4.0)))
            .show_inside(ui, |ui| {
                ui.set_width(ui.available_width());
                self.profile_panel(ui);
            });

        match self.page {
            Page::Convert => animating |= self.format_list(ui),
            Page::Library => self.library_nav(ui),
        }
        animating
    }

    /// 左下的用户配置面板：列表 + 重命名 + 导出/导入
    fn profile_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("配置").size(9.0).color(DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if tiny_text_button(ui, "+") {
                    self.cfg.add_profile();
                    self.profile_msg = "已新建一套配置".into();
                }
            });
        });
        ui.add_space(2.0);

        let mut switch_to: Option<usize> = None;
        let mut start_rename: Option<usize> = None;
        let mut remove: Option<usize> = None;
        let mut apply_rename: Option<usize> = None;
        let mut cancel_rename = false;

        for i in 0..self.cfg.profiles.len() {
            if self.rename_idx == Some(i) {
                ui.horizontal(|ui| {
                    // 缓冲从空开始、把原名当提示：直接打字就是新名字，
                    // 不改直接回车则保持原名（否则会像之前那样接在原名后面）
                    let hint = self.cfg.profiles[i].name.clone();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.rename_buf)
                            .hint_text(hint)
                            .desired_width((ui.available_width() - 6.0).max(60.0))
                            .margin(egui::vec2(6.0, 3.0)),
                    );
                    if self.rename_focus {
                        resp.request_focus();
                        self.rename_focus = false;
                    }
                    if resp.lost_focus() || ui.input(|x| x.key_pressed(egui::Key::Enter)) {
                        apply_rename = Some(i);
                    }
                    if ui.input(|x| x.key_pressed(egui::Key::Escape)) {
                        cancel_rename = true;
                    }
                });
                continue;
            }

            let on = i == self.cfg.active_profile;
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 24.0),
                egui::Sense::click(),
            );
            let p = ui.painter();
            let fill = if on {
                tint(ACCENT, 26)
            } else if resp.hovered() {
                tint(Color32::WHITE, 10)
            } else {
                Color32::TRANSPARENT
            };
            if fill != Color32::TRANSPARENT {
                p.rect_filled(rect, Rounding::same(7.0), fill);
            }
            let name_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(8.0, 0.0),
                egui::pos2(rect.right() - 26.0, rect.bottom()),
            );
            paint_clipped_v(
                ui,
                name_rect,
                10.5,
                if on { ACCENT } else { MUTED },
                &self.cfg.profiles[i].name,
            );
            // 改名按钮（✎）
            let er = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(12.0, 0.0),
                egui::vec2(20.0, 20.0),
            );
            let edit = ui.interact(er, ui.id().with(("prof_more", i)), egui::Sense::click());
            // 三个点自己画：字体里没有 "⋯" 的字形，文字会变豆腐块
            let dot_c = if edit.hovered() { TEXT } else { DIM };
            for dx in [-4.0f32, 0.0, 4.0] {
                p.circle_filled(er.center() + egui::vec2(dx, 0.0), 1.3, dot_c);
            }
            let popup_id = egui::Id::new(("prof_popup", i));
            if edit.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            } else if resp.clicked() {
                switch_to = Some(i);
            }
            egui::popup::popup_below_widget(
                ui,
                popup_id,
                &edit,
                egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(104.0);
                    if ui.button("重命名").clicked() {
                        start_rename = Some(i);
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if ui.button("删除这套").clicked() {
                        remove = Some(i);
                        ui.memory_mut(|m| m.close_popup());
                    }
                },
            );
            resp.context_menu(|ui| {
                if ui.button("重命名").clicked() {
                    start_rename = Some(i);
                    ui.close_menu();
                }
                if ui.button("删除这套配置").clicked() {
                    remove = Some(i);
                    ui.close_menu();
                }
            });
        }

        if let Some(i) = switch_to {
            self.cfg.switch_profile(i);
        }
        if let Some(i) = start_rename {
            self.rename_idx = Some(i);
            self.rename_buf.clear();
            self.rename_focus = true;
        }
        if let Some(i) = apply_rename {
            // 空输入 = 不改名
            if !self.rename_buf.trim().is_empty() {
                let name = self.rename_buf.clone();
                self.cfg.rename_profile(i, name);
            }
            self.rename_idx = None;
            self.rename_buf.clear();
        }
        if cancel_rename {
            self.rename_idx = None;
        }
        if let Some(i) = remove {
            self.cfg.remove_profile(i);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if tiny_text_button(ui, "导出") {
                self.export_profiles();
            }
            if tiny_text_button(ui, "导入") {
                self.import_profiles();
            }
        });
        if !self.profile_msg.is_empty() {
            ui.add_space(2.0);
            para(ui, &self.profile_msg, 8.5, DIM);
        }
    }

    /// 把全部配置导出成一个 JSON（换机器/备份用）
    fn export_profiles(&mut self) {
        self.cfg.snapshot_active();
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("format-converter-profiles.json")
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };
        let payload = serde_json::json!({
            "app": "format-converter",
            "version": 1,
            "active": self.cfg.active_profile,
            "profiles": self.cfg.profiles,
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => match std::fs::write(&path, s) {
                Ok(_) => self.profile_msg = format!("已导出 {}", path.display()),
                Err(e) => self.profile_msg = format!("导出失败：{e}"),
            },
            Err(e) => self.profile_msg = format!("导出失败：{e}"),
        }
    }

    /// 导入一份导出的 JSON，追加成新配置（重名自动加后缀）
    fn import_profiles(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        #[derive(serde::Deserialize)]
        struct Pkg {
            #[serde(default)]
            profiles: Vec<settings::Profile>,
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.profile_msg = format!("导入失败：{e}");
                return;
            }
        };
        let pkg: Pkg = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => {
                self.profile_msg = "导入失败：这不是配置 JSON".into();
                return;
            }
        };
        if pkg.profiles.is_empty() {
            self.profile_msg = "导入失败：文件里没有配置".into();
            return;
        }
        self.cfg.snapshot_active();
        let first = self.cfg.profiles.len();
        let mut added = 0;
        for mut p in pkg.profiles {
            let base = if p.name.trim().is_empty() {
                "导入的配置".to_string()
            } else {
                p.name.trim().to_string()
            };
            let mut name = base.clone();
            let mut k = 2;
            while self.cfg.profiles.iter().any(|q| q.name == name) {
                name = format!("{base} ({k})");
                k += 1;
            }
            p.name = name;
            self.cfg.profiles.push(p);
            added += 1;
        }
        self.cfg.active_profile = first.min(self.cfg.profiles.len() - 1);
        self.cfg.apply_active();
        self.cfg.save();
        self.profile_msg = format!("已导入 {added} 套配置");
    }

    /// 转换页的格式列表（就是原来的 sidebar，底部提示已经提到 left_rail）
    fn format_list(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let dt = self.dt;
        let mut animating = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new("输出格式").size(11.0).color(DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("百科").size(9.0).color(MUTED))
                            .fill(tint(Color32::WHITE, 8))
                            .stroke(line(Color32::WHITE, 16))
                            .rounding(Rounding::same(6.0)),
                    )
                    .on_hover_text("看看每个格式到底是什么")
                    .clicked()
                {
                    // 打开时对准当前输出格式：用户多半是"想知道我正在转的这个是什么"
                    self.wiki_sel = self.target.id;
                    self.wiki_open = true;
                }
            });
        });
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

    /// 音乐库页左侧：歌曲 / 专辑 / 风格 / 艺术家 / 库
    fn library_nav(&mut self, ui: &mut egui::Ui) {
        let songs = self.lib_entries.len();
        let albums = uniq(&self.lib_entries, |e| &e.album);
        let genres = uniq(&self.lib_entries, |e| &e.genre);
        let artists = uniq(&self.lib_entries, |e| &e.artist);
        let rows: [(LibTab, &str, usize); 5] = [
            // 「库」放最前：加音乐/加文件夹都在这里，别让人找
            (LibTab::Folders, "库", self.cfg.library_dirs.len() + self.cfg.library_files.len()),
            (LibTab::Songs, "歌曲", songs),
            (LibTab::Albums, "专辑", albums),
            (LibTab::Genres, "风格", genres),
            (LibTab::Artists, "艺术家", artists),
        ];
        for (tab, label, n) in rows {
            if rail_sub(ui, label, n, self.lib_tab == tab) {
                self.lib_tab = tab;
            }
        }

        ui.add_space(12.0);
        if self.lib_scanning {
            let total = self.lib_total.max(self.lib_entries.len());
            para(
                ui,
                &format!("扫描中… {}/{}", self.lib_entries.len(), total),
                9.0,
                ACCENT,
            );
        } else if !self.lib_status.is_empty() {
            para(ui, &self.lib_status, 9.0, DIM);
        }
    }

    /// 画一行输出格式（含展开后的小字）
    fn format_row(
        &mut self,
        ui: &mut egui::Ui,
        spec: &'static FormatSpec,
        dt: f32,
    ) -> bool {
        let mut animating = false;
        // 编码器分两类：ffmpeg 自带的，和用户自己装的外部编码器（atracdenc）
        let external = fc_core::external_encoder(spec.id);
        let enc = fc_core::encoder_for(spec.id);
        let enc_ok = match external {
            Some(_) => self.atracdenc.is_some(),
            None => enc.map(|e| self.encoders.contains(e)).unwrap_or(true),
        };
        let selectable = spec.can_encode;
        // 编码器缺失时不能选为输出，否则转换时才炸
        let usable = selectable && enc_ok;
        let selected = usable && spec.id == self.target.id;

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
        let base = if !enc_ok {
            DIM
        } else if selectable {
            TEXT
        } else {
            MUTED
        };
        let text_c = blend(base, ACCENT, cur);

        let mut label = RichText::new(spec.name).size(11.5).color(text_c);
        if !enc_ok {
            label = label.strikethrough();
        }
        let mut row = egui::Button::new(label)
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke_c))
            .rounding(Rounding::same(8.0))
            .min_size(egui::vec2(ui.available_width(), 22.0));
        if !usable {
            row = row.fill(Color32::TRANSPARENT).stroke(Stroke::NONE);
        }

        let mut resp = ui.add(row);
        if !enc_ok {
            let msg = match external {
                Some(name) => format!(
                    "你的系统里还没装 {name}（ATRAC 外部编码器）\n装好后放到程序目录或 PATH，再到设置里点「重新检测」。\n它不随本程序分发，装不装由你决定。"
                ),
                None => format!(
                    "你的机器上似乎没有这个编码器（{}）\n换个 ffmpeg 构建可能就有，或者别选这个格式。",
                    enc.unwrap_or("?")
                ),
            };
            resp = resp.on_hover_text(msg);
        } else if !selectable {
            resp = resp.on_hover_text("只读格式：能解码，不能作为输出目标");
        }
        if resp.clicked() {
            // 只读/缺编码器的格式不改输出目标，只展开解释
            if usable {
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

    /// 这个目标格式现在能不能真的转出来（含外部工具的判断）。
    ///
    /// 侧栏、百科页、"设为输出格式"按钮都用它，免得三处各写一套条件。
    fn target_available(&self, spec: &FormatSpec) -> bool {
        if !spec.can_encode {
            return false;
        }
        match fc_core::external_encoder(spec.id) {
            Some(_) => self.atracdenc.is_some(),
            None => fc_core::encoder_for(spec.id)
                .map(|e| self.encoders.contains(e))
                .unwrap_or(true),
        }
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
                                // 右边要放：状态 + ··· + ×，有音质标识还要再宽一点
                                let q_w = if info.as_ref().and_then(quality_of).is_some() { 58.0 } else { 0.0 };
                                ui.set_max_width((row_w - 42.0 - 186.0 - q_w).max(100.0));
                                let title = tags.display_title(&name);
                                ui.add(
                                    egui::Label::new(RichText::new(title).size(12.5).color(TEXT))
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                                ui.add_space(2.0);
                                let sub0 = match &info {
                                    Some(m) => {
                                        // 数值音质：编码 · 采样率/位深 · 码率 · 时长 → 目标
                                        // 用 44.1k/16bit 这种紧凑写法，一行放得下
                                        let mut parts: Vec<String> =
                                            vec![m.codec_label().to_string()];
                                        if m.sample_rate > 0 {
                                            match m.bits {
                                                Some(b) => parts.push(format!(
                                                    "{}/{}bit",
                                                    fmt_k(m.sample_rate),
                                                    b
                                                )),
                                                None => parts.push(fmt_khz(m.sample_rate)),
                                            }
                                        }
                                        if m.bitrate_kbps > 0 {
                                            parts.push(format!("{}kbps", m.bitrate_kbps));
                                        }
                                        parts.push(fmt_duration(m.duration_s));
                                        format!("{}  →  {}", parts.join(" · "), spec.name)
                                    }
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
                                // 顺序按 right_to_left 的摆放来写：先写的在最右边
                                ui.set_min_width(ui.available_width());
                                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                                // 行里这两个小按钮不要底色：默认的浅色方块在深色行上很扎眼，
                                // 只留字符本身，悬停时给一点点提亮。
                                let vis = ui.visuals_mut();
                                vis.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                                vis.widgets.hovered.weak_bg_fill = tint(Color32::WHITE, 16);
                                vis.widgets.active.weak_bg_fill = tint(Color32::WHITE, 26);
                                vis.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                                vis.widgets.hovered.bg_fill = tint(Color32::WHITE, 16);
                                vis.widgets.active.bg_fill = tint(Color32::WHITE, 26);
                                vis.widgets.inactive.bg_stroke = Stroke::NONE;
                                vis.widgets.hovered.bg_stroke = Stroke::NONE;
                                vis.widgets.active.bg_stroke = Stroke::NONE;

                                // ① 删除 ×（最右）
                                let del_resp =
                                    ui.small_button(RichText::new("×").size(11.0).color(DIM));
                                if del_resp.clicked() {
                                    remove = Some(i);
                                }

                                // ② 单曲格式菜单 ···（选过格式就显示格式名）
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

                                // ③ 状态
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

                                // ④ 音质标识（Hi-Res 会呼吸、CD 会转）
                                if let Some(m) = &info {
                                    if let Some(q) = quality_of(m) {
                                        ui.add_space(4.0);
                                        animating |= quality_badge(ui, q);
                                    }
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

    /// 格式百科：左边列表，右边长文。B 写的 29 条长文案就展示在这里。
    fn wiki_page(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let area = egui::Rect::from_min_max(
            egui::pos2(screen.left(), screen.top() + TITLEBAR_H),
            screen.max,
        );

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.wiki_open = false;
        }

        egui::Area::new(egui::Id::new("wiki_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(tint(BG, 255))
                    .inner_margin(egui::Margin::symmetric(28.0, 18.0))
                    .show(ui, |ui| {
                        // 内容尺寸要扣掉 Frame 的边距（28*2 / 18*2），否则 Area 会比区域大，
                        // egui 会把它往上顶到 y=0、盖住标题栏，关闭/最小化按钮就点不到了。
                        ui.set_min_size(egui::vec2(area.width() - 56.0, area.height() - 36.0));
                        ui.set_max_width(area.width() - 56.0);
                        ui.add_space(TITLEBAR_H);

                        // 顶栏
                        let back_rect =
                            egui::Rect::from_min_size(ui.cursor().min, egui::vec2(78.0, 30.0));
                        let back = ui.interact(
                            back_rect,
                            ui.id().with("wiki_back"),
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
                            self.wiki_open = false;
                        }

                        ui.horizontal(|ui| {
                            ui.add_space(88.0);
                            ui.label(RichText::new("格式百科").size(15.0).color(TEXT).strong());
                            ui.add_space(16.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.wiki_filter)
                                    .hint_text("搜格式名…")
                                    .desired_width(180.0)
                                    .margin(egui::vec2(8.0, 5.0)),
                            );
                        });

                        ui.add_space(14.0);

                        let keyword = self.wiki_filter.trim().to_lowercase();
                        let list: Vec<&'static FormatSpec> = fc_core::all()
                            .filter(|f| {
                                keyword.is_empty()
                                    || f.name.to_lowercase().contains(&keyword)
                                    || f.id.contains(&keyword)
                                    || f.ext.contains(&keyword)
                            })
                            .collect();

                        let body_h = (ui.available_height() - 8.0).max(120.0);
                        let right_w = (ui.available_width() - 210.0 - 10.0).max(220.0);
                        ui.horizontal_top(|ui| {
                            // 左：列表
                            ui.allocate_ui_with_layout(
                                egui::vec2(210.0, body_h),
                                Layout::top_down(Align::LEFT),
                                |ui| {
                                ui.set_min_height(body_h);
                                egui::ScrollArea::vertical()
                                    .id_salt("wiki_list")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for f in &list {
                                            let on = f.id == self.wiki_sel;
                                            let b = egui::Button::new(
                                                RichText::new(f.name).size(11.0).color(if on {
                                                    ACCENT
                                                } else {
                                                    MUTED
                                                }),
                                            )
                                            .fill(if on {
                                                tint(ACCENT, 20)
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .stroke(Stroke::NONE)
                                            .rounding(Rounding::same(7.0))
                                            .min_size(egui::vec2(ui.available_width(), 24.0));
                                            if ui.add(b).clicked() {
                                                self.wiki_sel = f.id;
                                            }
                                        }
                                    });
                                },
                            );

                            ui.add_space(10.0);

                            // 右：详情
                            ui.allocate_ui_with_layout(
                                egui::vec2(right_w, body_h),
                                Layout::top_down(Align::LEFT),
                                |ui| {
                                ui.set_min_height(body_h);
                                egui::ScrollArea::vertical()
                                    .id_salt("wiki_body")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        // 一行太长不好读，限制在 760 左右
                                        ui.set_max_width((right_w - 18.0).min(760.0));
                                        let Some(spec) = fc_core::by_id(self.wiki_sel) else {
                                            return;
                                        };
                                        ui.label(
                                            RichText::new(spec.name).size(20.0).color(TEXT).strong(),
                                        );
                                        ui.add_space(6.0);
                                        ui.horizontal(|ui| {
                                            let kind = match spec.kind {
                                                fc_core::Kind::Lossless => "无损",
                                                fc_core::Kind::Lossy => "有损",
                                                _ => "设备专用",
                                            };
                                            ui.label(
                                                RichText::new(kind).size(9.5).color(ACCENT),
                                            );
                                            ui.label(RichText::new("·").size(9.5).color(DIM));
                                            ui.label(
                                                RichText::new(spec.patent.label())
                                                    .size(9.5)
                                                    .color(MUTED),
                                            );
                                            ui.label(RichText::new("·").size(9.5).color(DIM));
                                            ui.label(
                                                RichText::new(format!(".{}", spec.ext))
                                                    .size(9.5)
                                                    .color(MUTED),
                                            );
                                            ui.label(RichText::new("·").size(9.5).color(DIM));
                                            let (w, wc) = if !spec.can_encode {
                                                ("只能读，不能转出", WARN)
                                            } else if self.target_available(spec) {
                                                ("可以转成这个", ACCENT)
                                            } else {
                                                ("要装外部编码器才能转出", WARN)
                                            };
                                            ui.label(RichText::new(w).size(9.5).color(wc));
                                        });
                                        ui.add_space(14.0);

                                        // 百科是"挑格式"的最后一站：看完解释直接换输出目标，
                                        // 不用再回侧栏找一遍。
                                        if spec.can_encode && self.target_available(spec) {
                                            let is_target = spec.id == self.target.id;
                                            let btn = egui::Button::new(
                                                RichText::new(if is_target {
                                                    format!("{} 就是当前输出格式", spec.name)
                                                } else {
                                                    format!("把输出格式设为 {}", spec.name)
                                                })
                                                .size(10.5)
                                                .color(Color32::from_rgb(10, 26, 24)),
                                            )
                                            .fill(if is_target {
                                                tint(ACCENT, 110)
                                            } else {
                                                ACCENT
                                            })
                                            .stroke(Stroke::NONE)
                                            .rounding(Rounding::same(9.0))
                                            .min_size(egui::vec2(0.0, 30.0));
                                            if ui
                                                .add(btn)
                                                .on_hover_text("回到主界面，接下来转的文件都用这个格式")
                                                .clicked()
                                                && !is_target
                                            {
                                                self.target = spec;
                                                self.wiki_open = false;
                                            }
                                            ui.add_space(4.0);
                                            para(
                                                ui,
                                                "选中后的码率等参数在右侧「输出」列调整。",
                                                8.5,
                                                DIM,
                                            );
                                        } else if spec.can_encode {
                                            ui.add_space(2.0);
                                            let why = if fc_core::external_encoder(spec.id).is_some() {
                                                "你的系统里还没装 atracdenc（外部编码器），暂时不能转成它。装好后到设置里点「重新检测」。"
                                            } else {
                                                "你的 ffmpeg 里没有这个编码器，暂时不能转成它。"
                                            };
                                            card(ui, DANGER, |ui| {
                                                para(ui, why, 10.0, MUTED);
                                            });
                                        }

                                        ui.add_space(6.0);

                                        let text = fc_core::long_blurb(spec.id)
                                            .unwrap_or(spec.blurb);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(text).size(11.0).color(MUTED),
                                            )
                                            .wrap_mode(egui::TextWrapMode::Wrap),
                                        );
                                    });
                                },
                            );
                        });
                    });
            });
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
                    // 和主界面同一套"玻璃"：半透明底 + 壁纸透过来。
                    // 之前是 tint(BG, 255) 的纯黑，用户明确说不要。
                    .fill(tint(BG, 148))
                    .inner_margin(egui::Margin::symmetric(28.0, 18.0))
                    .show(ui, |ui| {
                        // 内容尺寸要扣掉 Frame 的边距（28*2 / 18*2），否则 Area 会比区域大，
                        // egui 会把它往上顶到 y=0、盖住标题栏，关闭/最小化按钮就点不到了。
                        ui.set_min_size(egui::vec2(area.width() - 56.0, area.height() - 36.0));
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

                        egui::ScrollArea::vertical()
                            .id_salt("settings_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
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
                                    if bar_slider(ui, &mut dim, 0, 210, false) {
                                        self.cfg.dim = dim as u8;
                                        changed = true;
                                    }
                                });

                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("HDR 模式").size(10.5).color(MUTED));
                                    ui.add_space(12.0);
                                    for (val, label) in [(false, "关"), (true, "开")] {
                                        let on = self.cfg.hdr == val;
                                        if seg_button(ui, label, on) {
                                            self.cfg.hdr = val;
                                            set_hdr(val);
                                            changed = true;
                                        }
                                    }
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("更亮的强调色 + 发光，只影响界面观感，不改音频参数")
                                        .size(9.0)
                                        .color(DIM),
                                );

                                // 界面缩放：字号是写死在各个控件里的，所以这里用 egui 的
                                // 全局 zoom（文字 + 控件一起放大），而不是只改字体
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("界面缩放").size(10.5).color(MUTED));
                                    ui.add_space(12.0);
                                    for s in settings::SCALES {
                                        let on = (self.cfg.ui_scale - s).abs() < 0.001;
                                        let label = format!("{}%", (s * 100.0).round() as i32);
                                        if seg_button(ui, &label, on) {
                                            self.cfg.ui_scale = s;
                                            ui.ctx().set_zoom_factor(BASE_ZOOM * s);
                                            changed = true;
                                        }
                                    }
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("放大的是整个界面（文字和控件一起）；窗口小的时候建议 100%")
                                        .size(9.0)
                                        .color(DIM),
                                );

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
                                        row(ui, "程序", APP_NAME);
                                        row(ui, "版本", "0.1.0");
                                        row(ui, "字体", "Noto Sans SC（SIL OFL 1.1）");
                                        row(ui, "音频解码", "ffmpeg 子进程");
                                        row(ui, "ATRAC 编码", "atracdenc（LGPL-2.1+，需自行安装）");
                                        row(ui, "频谱分析", "fc-analyze（自研）");
                                        row(ui, "许可", "开源，详见仓库");
                                    });

                                ui.add_space(8.0);
                                match &self.atracdenc {
                                    Some(p) => para(
                                        ui,
                                        &format!("atracdenc：已找到  {}", p.display()),
                                        9.0,
                                        ACCENT,
                                    ),
                                    None => para(
                                        ui,
                                        "atracdenc：未安装 —— ATRAC1 / ATRAC3 / ATRAC3+ 暂时只能解码，不能转出。",
                                        9.0,
                                        WARN,
                                    ),
                                }
                                ui.horizontal(|ui| {
                                    let b = egui::Button::new(
                                        RichText::new("重新检测外部工具").size(9.0).color(MUTED),
                                    )
                                    .fill(tint(Color32::WHITE, 10))
                                    .stroke(line(Color32::WHITE, 18))
                                    .rounding(Rounding::same(7.0));
                                    if ui.add(b).clicked() {
                                        self.atracdenc = fc_core::find_atracdenc();
                                        changed = true;
                                    }
                                    if self.atracdenc.is_some() {
                                        ui.label(
                                            RichText::new("ATRAC 输出已可用")
                                                .size(9.0)
                                                .color(ACCENT),
                                        );
                                    }
                                });
                                para(
                                    ui,
                                    "atracdenc 是社区逆向实现的 ATRAC 编码器，LGPL-2.1+。它不随本程序分发：装不装、装哪个版本由你决定，程序只在你装了的时候调用它。",
                                    8.5,
                                    DIM,
                                );

                                ui.add_space(18.0);
                                section(ui, "支持开发");
                                para(
                                    ui,
                                    "这个工具免费、开源、不卖授权。如果它帮你把旧 MD 里的东西救出来了，欢迎请我喝杯咖啡。",
                                    9.5,
                                    MUTED,
                                );
                                ui.add_space(10.0);
                                let has_qr = self.donate_wechat.is_some() || self.donate_alipay.is_some();
                                if has_qr {
                                    ui.horizontal(|ui| {
                                        if let Some(t) = &self.donate_wechat {
                                            ui.vertical(|ui| {
                                                ui.add(egui::Image::new(t).fit_to_exact_size(egui::vec2(118.0, 118.0)));
                                                ui.add_space(3.0);
                                                ui.label(RichText::new("微信").size(9.0).color(DIM));
                                            });
                                        }
                                        ui.add_space(16.0);
                                        if let Some(t) = &self.donate_alipay {
                                            ui.vertical(|ui| {
                                                ui.add(egui::Image::new(t).fit_to_exact_size(egui::vec2(118.0, 118.0)));
                                                ui.add_space(3.0);
                                                ui.label(RichText::new("支付宝").size(9.0).color(DIM));
                                            });
                                        }
                                    });
                                } else {
                                    para(
                                        ui,
                                        "（把收款码放到程序目录，命名 donate-wechat.png / donate-alipay.png 即可显示）",
                                        8.5,
                                        DIM,
                                    );
                                }
                            });
                    });
            });

        if changed {
            self.cfg.save();
        }
    }

    /// 底部全局播放条
    /// 音乐库主视图（右侧整块）
    fn library_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        ui.set_width(ui.available_width());
        let mut animating = false;

        let title = match self.lib_tab {
            LibTab::Songs => "歌曲",
            LibTab::Albums => "专辑",
            LibTab::Genres => "风格",
            LibTab::Artists => "艺术家",
            LibTab::Folders => "库",
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(14.0).color(TEXT).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.lib_scanning {
                    ui.label(
                        RichText::new(format!(
                            "扫描中 {} / {}",
                            self.lib_entries.len(),
                            self.lib_total.max(self.lib_entries.len())
                        ))
                        .size(9.0)
                        .color(ACCENT),
                    );
                } else {
                    ui.label(
                        RichText::new(format!("{} 首", self.lib_entries.len()))
                            .size(9.0)
                            .color(DIM),
                    );
                }
            });
        });
        ui.add_space(6.0);

        // 加音乐 / 加文件夹永远在第一行，不藏在「库」页里
        let mut add_files = false;
        let mut add_dir = false;
        let mut rescan = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            add_files = lib_button(ui, "添加音乐", true);
            add_dir = lib_button(ui, "添加文件夹", false);
            rescan = lib_button(ui, "重新扫描", false);
            if self.lib_tab != LibTab::Folders {
                ui.add_space(6.0);
                let w = (ui.available_width() - 8.0).max(120.0);
                if search_box(ui, &mut self.lib_filter, "搜歌名 / 艺术家 / 专辑 / 风格…", w) {
                    animating = true;
                }
            }
        });
        if add_files {
            self.add_library_files();
            animating = true;
        }
        if add_dir {
            self.add_library_folder();
            animating = true;
        }
        if rescan {
            self.scan_library();
            animating = true;
        }
        ui.add_space(10.0);

        if self.cfg.library_dirs.is_empty() && self.cfg.library_files.is_empty() {
            card(ui, WARN, |ui| {
                para(ui, "还没有音乐库", 11.5, WARN);
                ui.add_space(4.0);
                para(
                    ui,
                    "点上面「添加文件夹」扫一个目录，或者「添加音乐」挑几个文件。只读扫描，不搬文件、不改文件。",
                    9.5,
                    MUTED,
                );
            });
            return animating;
        }

        // 有文件夹但还没扫过：给一个明确的入口，别让列表空着让人以为坏了
        if self.lib_entries.is_empty() && !self.lib_scanning && self.lib_tab != LibTab::Folders {
            let mut go = false;
            card(ui, WARN, |ui| {
                para(ui, "还没扫描过音乐库", 11.5, WARN);
                ui.add_space(4.0);
                para(
                    ui,
                    "扫描结果会记在配置里，以后打开直接显示；加了新歌再扫一遍就行。",
                    9.5,
                    MUTED,
                );
                ui.add_space(8.0);
                go = lib_button(ui, "开始扫描", true);
            });
            if go {
                self.scan_library();
            }
            return animating;
        }

        if self.lib_tab == LibTab::Folders {
            animating |= self.library_folders(ui);
            return animating;
        }

        let needle = self.lib_filter.trim().to_lowercase();
        let ids: Vec<usize> = (0..self.lib_entries.len())
            .filter(|&i| {
                if needle.is_empty() {
                    return true;
                }
                let e = &self.lib_entries[i];
                e.title.to_lowercase().contains(&needle)
                    || e.artist.to_lowercase().contains(&needle)
                    || e.album.to_lowercase().contains(&needle)
                    || e.genre.to_lowercase().contains(&needle)
            })
            .collect();
        let playing_path = self
            .player_for
            .and_then(|i| self.items.get(i))
            .map(|it| it.path.clone());

        let mut clicked: Option<usize> = None;
        let mut requests: Vec<PathBuf> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("lib_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if ids.is_empty() {
                    para(ui, "没找到匹配的歌。", 10.0, DIM);
                    return;
                }
                match self.lib_tab {
                    LibTab::Songs => {
                        let mut rows = ids.clone();
                        rows.sort_by(|&a, &b| {
                            self.lib_entries[a]
                                .title
                                .to_lowercase()
                                .cmp(&self.lib_entries[b].title.to_lowercase())
                        });
                        for i in rows {
                            let e = &self.lib_entries[i];
                            let sel = self.lib_selected.as_deref() == Some(e.path.as_path());
                            let pl = playing_path.as_deref() == Some(e.path.as_path());
                            // 滚到哪加载哪：这一行在视口里才去抽封面
                            let row_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::vec2(ui.available_width(), 48.0),
                            );
                            if ui.clip_rect().intersects(row_rect)
                                && !self.lib_covers.contains_key(&e.path)
                                && !self.lib_cover_pending.contains(&e.path)
                            {
                                requests.push(e.path.clone());
                            }
                            let tex = self.lib_covers.get(&e.path).and_then(|o| o.as_ref());
                            if lib_row(ui, e, pl, sel, tex).clicked() {
                                clicked = Some(i);
                            }
                        }
                    }
                    LibTab::Albums | LibTab::Artists | LibTab::Genres => {
                        let tab = self.lib_tab;
                        let mut rows = ids.clone();
                        rows.sort_by(|&a, &b| {
                            let ka = lib_key(tab, &self.lib_entries[a]).to_lowercase();
                            let kb = lib_key(tab, &self.lib_entries[b]).to_lowercase();
                            ka.cmp(&kb).then_with(|| {
                                self.lib_entries[a]
                                    .title
                                    .to_lowercase()
                                    .cmp(&self.lib_entries[b].title.to_lowercase())
                            })
                        });
                        let mut last = String::from("\u{0}");
                        for i in rows {
                            let key = lib_key(tab, &self.lib_entries[i]);
                            if key != last {
                                last = key.clone();
                                let n = ids
                                    .iter()
                                    .filter(|&&j| lib_key(tab, &self.lib_entries[j]) == key)
                                    .count();
                                let shown = key.trim();
                                lib_group(ui, if shown.is_empty() { "未知" } else { shown }, n);
                            }
                            let e = &self.lib_entries[i];
                            let sel = self.lib_selected.as_deref() == Some(e.path.as_path());
                            let pl = playing_path.as_deref() == Some(e.path.as_path());
                            let row_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::vec2(ui.available_width(), 48.0),
                            );
                            if ui.clip_rect().intersects(row_rect)
                                && !self.lib_covers.contains_key(&e.path)
                                && !self.lib_cover_pending.contains(&e.path)
                            {
                                requests.push(e.path.clone());
                            }
                            let tex = self.lib_covers.get(&e.path).and_then(|o| o.as_ref());
                            if lib_row(ui, e, pl, sel, tex).clicked() {
                                clicked = Some(i);
                            }
                        }
                    }
                    LibTab::Folders => {}
                }
            });

        if !requests.is_empty() {
            if let Some(tools) = self.tools.clone() {
                for p in requests.into_iter().take(64) {
                    if self.lib_cover_pending.insert(p.clone()) {
                        spawn_lib_cover(tools.ffmpeg.clone(), p, self.lib_cover_tx.clone());
                    }
                }
            }
        }

        if let Some(i) = clicked {
            let path = self.lib_entries[i].path.clone();
            let now = ui.input(|i| i.time);
            let dbl = self.lib_last_click.0.as_deref() == Some(path.as_path())
                && now - self.lib_last_click.1 < 0.45;
            self.lib_selected = Some(path.clone());
            self.lib_last_click = (Some(path.clone()), now);
            if dbl {
                self.play_path(path, ctx);
            }
        }
        animating
    }

    /// 「库」页：管理文件夹 + 扫描状态
    /// 挑一个文件夹加进库
    fn add_library_folder(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            let s = p.to_string_lossy().to_string();
            if !self.cfg.library_dirs.contains(&s) {
                self.cfg.library_dirs.push(s);
                self.cfg.save();
                self.scan_library();
            }
        }
    }

    /// 挑几个文件单独加进库（不挂在文件夹下）
    fn add_library_files(&mut self) {
        const EXT: &[&str] = &[
            "mp3", "mp2", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff",
            "wma", "ac3", "eac3", "oma", "aa3", "at3", "at9", "ape", "wv", "tta", "caf", "w64",
        ];
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("音频文件", EXT)
            .pick_files()
        else {
            return;
        };
        let mut added = false;
        for p in paths {
            if !is_audio(&p) {
                continue;
            }
            let s = p.to_string_lossy().to_string();
            if !self.cfg.library_files.contains(&s) {
                self.cfg.library_files.push(s);
                added = true;
            }
        }
        if added {
            self.cfg.save();
            self.scan_library();
        }
    }

    fn library_folders(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        if self.cfg.library_dirs.is_empty() && self.cfg.library_files.is_empty() {
            para(
                ui,
                "还没有添加任何东西。用上面的「添加文件夹」扫一个目录，或「添加音乐」挑几个文件。",
                10.0,
                DIM,
            );
        } else {
            ui.label(RichText::new("文件夹").size(9.5).color(DIM));
            ui.add_space(4.0);
        }

        let mut remove: Option<usize> = None;
        for (i, d) in self.cfg.library_dirs.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 30.0),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(rect.shrink2(egui::vec2(0.0, 2.0)), Rounding::same(8.0), tint(Color32::WHITE, 6));
            let label_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(10.0, 0.0),
                egui::pos2(rect.right() - 40.0, rect.bottom()),
            );
            paint_clipped_v(ui, label_rect, 10.0, MUTED, d);
            let xr = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(14.0, 0.0),
                egui::vec2(24.0, 24.0),
            );
            let x = ui.interact(xr, ui.id().with(("lib_dir_del", i)), egui::Sense::click());
            let xc = if x.hovered() { DANGER } else { DIM };
            p.text(xr.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(13.0), xc);
            if x.clicked() {
                remove = Some(i);
            }
            ui.add_space(2.0);
        }
        if let Some(i) = remove {
            self.cfg.library_dirs.remove(i);
            self.cfg.save();
            changed = true;
            self.scan_library();
        }

        // 单独加进来的文件（不挂在任何文件夹下）
        if !self.cfg.library_files.is_empty() {
            ui.add_space(10.0);
            ui.label(RichText::new("单独添加的文件").size(9.5).color(DIM));
            ui.add_space(4.0);
        }
        let mut remove_file: Option<usize> = None;
        for (i, f) in self.cfg.library_files.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 30.0),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(
                rect.shrink2(egui::vec2(0.0, 2.0)),
                Rounding::same(8.0),
                tint(Color32::WHITE, 6),
            );
            let label_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(10.0, 0.0),
                egui::pos2(rect.right() - 40.0, rect.bottom()),
            );
            paint_clipped_v(ui, label_rect, 10.0, MUTED, f);
            let xr = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(14.0, 0.0),
                egui::vec2(24.0, 24.0),
            );
            let x = ui.interact(xr, ui.id().with(("lib_file_del", i)), egui::Sense::click());
            let xc = if x.hovered() { DANGER } else { DIM };
            p.text(
                xr.center(),
                egui::Align2::CENTER_CENTER,
                "×",
                egui::FontId::proportional(13.0),
                xc,
            );
            if x.clicked() {
                remove_file = Some(i);
            }
            ui.add_space(2.0);
        }
        if let Some(i) = remove_file {
            self.cfg.library_files.remove(i);
            self.cfg.save();
            changed = true;
            self.scan_library();
        }

        ui.add_space(12.0);
        if self.lib_scanning {
            let total = self.lib_total.max(1);
            let frac = (self.lib_entries.len() as f32 / total as f32).clamp(0.0, 1.0);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 6.0),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(rect, Rounding::same(3.0), tint(Color32::WHITE, 14));
            let mut f = rect;
            f.set_width(rect.width() * frac);
            p.rect_filled(f, Rounding::same(3.0), ACCENT);
            ui.add_space(4.0);
        }
        if !self.lib_status.is_empty() {
            para(ui, &self.lib_status, 9.0, DIM);
        }
        ui.add_space(4.0);
        para(
            ui,
            "扫描只读：读标签和时长，结果只存在内存里，关掉就没了，下次打开重扫。",
            8.5,
            DIM,
        );
        changed
    }

    /// 扫一遍所有库目录（后台线程，边扫边回传）
    fn scan_library(&mut self) {
        if self.lib_scanning {
            return;
        }
        let Some(tools) = self.tools.clone() else {
            self.lib_status = "找不到 ffmpeg，扫不了".into();
            return;
        };
        let dirs: Vec<PathBuf> = self
            .cfg
            .library_dirs
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .collect();
        let seed: Vec<PathBuf> = self
            .cfg
            .library_files
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect();
        if dirs.is_empty() && seed.is_empty() {
            self.lib_status = "没有可用的文件夹或文件".into();
            return;
        }
        self.lib_scanning = true;
        self.lib_total = 0;
        self.lib_entries.clear();
        // 重扫就丢掉旧封面缓存（文件可能换了封面）
        self.lib_covers.clear();
        self.lib_cover_pending.clear();
        self.lib_status = "正在扫描…".into();
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || {
            let mut files = seed;
            for d in &dirs {
                collect_audio(d, &mut files, 0);
            }
            files.sort();
            files.dedup();
            let _ = tx.send(LibMsg::Count(files.len()));
            for f in files {
                let Ok(info) = fc_core::probe(&tools, &f) else {
                    continue;
                };
                let tags = fc_core::read_tags(&tools.ffprobe, &f);
                let title = tags.display_title(&file_stem(&f));
                let entry = LibEntry {
                    path: f.clone(),
                    title,
                    artist: tags.artist.unwrap_or_default(),
                    album: tags.album.unwrap_or_default(),
                    genre: tags.genre.unwrap_or_default(),
                    duration_s: info.duration_s,
                    codec: info.codec_label().to_string(),
                    bits: info.bits,
                    sample_rate: info.sample_rate,
                    bitrate_kbps: info.bitrate_kbps,
                    lossy: info.is_lossy(),
                };
                let _ = tx.send(LibMsg::Entry(entry));
            }
            let _ = tx.send(LibMsg::Done);
        });
    }

    fn poll_library(&mut self) {
        while let Ok(m) = self.lib_rx.try_recv() {
            match m {
                LibMsg::Count(n) => self.lib_total = n,
                LibMsg::Entry(e) => self.lib_entries.push(e),
                LibMsg::Done => {
                    self.lib_scanning = false;
                    self.lib_status = format!("扫描完成：{} 首", self.lib_entries.len());
                    // 结果写进配置：下次启动直接铺出来，不用再扫
                    self.cfg.library_cache = self
                        .lib_entries
                        .iter()
                        .map(|e| settings::CachedTrack {
                            path: e.path.to_string_lossy().to_string(),
                            title: e.title.clone(),
                            artist: e.artist.clone(),
                            album: e.album.clone(),
                            genre: e.genre.clone(),
                            duration_s: e.duration_s,
                            codec: e.codec.clone(),
                            bits: e.bits,
                            sample_rate: e.sample_rate,
                            bitrate_kbps: e.bitrate_kbps,
                            lossy: e.lossy,
                        })
                        .collect();
                    self.cfg.save();
                }
            }
        }
    }

    /// 懒加载封面的回传：解码好的 RGBA 在这里变成 GPU 纹理
    fn poll_lib_covers(&mut self, ctx: &egui::Context) {
        while let Ok((path, payload)) = self.lib_cover_rx.try_recv() {
            self.lib_cover_pending.remove(&path);
            let tex = payload.map(|(rgba, w, h)| {
                let pixels: Vec<Color32> = rgba
                    .chunks_exact(4)
                    .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                    .collect();
                let img = egui::ColorImage {
                    size: [w as usize, h as usize],
                    pixels,
                };
                self.lib_cover_seq += 1;
                ctx.load_texture(
                    format!("libcover_{}", self.lib_cover_seq),
                    img,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.lib_covers.insert(path, tex);
        }
    }

    /// 音乐库里双击一首歌：不在队列里就先加进去（顺便读标签/封面），然后播
    fn play_path(&mut self, path: PathBuf, _ctx: &egui::Context) {
        if !path.exists() {
            // 缓存里记着的文件可能已经被移走/删掉，明确告诉用户，别静默失败
            self.lib_status = format!("文件不在了：{}", path.display());
            return;
        }
        if !self.items.iter().any(|i| i.path == path) {
            self.add_files(vec![path.clone()]);
        }
        if let Some(idx) = self.items.iter().position(|i| i.path == path) {
            self.selected = idx;
            self.start_play(idx);
        }
    }

    /// 底部播放栏：常驻在"队列 + 详情"两栏下面。
    ///
    /// 没在播的时候不消失——显示当前选中项的封面和时长，按钮置灰。
    /// 点封面看大图（队列里那张只有 96px，点开时后台重新抽一次原图）。
    fn player_bar(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());

        let display = self
            .player_for
            .or_else(|| (!self.items.is_empty()).then_some(self.selected))
            .filter(|i| *i < self.items.len());
        let (playing, pos, dur) = match self.player.as_ref() {
            Some(p) => (p.is_playing(), p.position_s(), p.duration_s().max(0.1)),
            None => {
                let d = display
                    .and_then(|i| self.items[i].info.as_ref())
                    .map(|m| m.duration_s)
                    .unwrap_or(0.0);
                (false, 0.0, d.max(0.1))
            }
        };
        let has_player = self.player.is_some();
        let name = display
            .map(|i| {
                let it = &self.items[i];
                it.tags.display_title(&it.name())
            })
            .unwrap_or_else(|| "未在播放".to_string());
        let cover = display.and_then(|i| self.items[i].cover.clone());
        let has_cover = cover.is_some();

        let mut toggle = false;
        let mut stop = false;
        let mut seek: Option<f64> = None;
        let mut vol_change: Option<f32> = None;
        let mut open_cover = false;
        let mut open_spec = false;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);

            // 封面（点开大图）
            let resp = bar_cover(ui, cover.as_ref(), 48.0);
            if resp.clicked() && has_cover {
                open_cover = true;
            }

            ui.add_space(2.0);

            // 给右边的音量留固定宽度：否则标题那行会吃掉整条横向空间，
            // 音量条被挤成一个小点（第一次就是这样）。
            // 右边要放音量条 + 频谱按钮，宽度不够的话频谱按钮会画到布局外面、点不到
            let vol_w = 122.0;
            let content_w = (ui.available_width() - vol_w).max(160.0);
            ui.allocate_ui_with_layout(
                egui::vec2(content_w, 48.0),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.horizontal(|ui| {
                        if icon_button(
                            ui,
                            if playing { Icon::Pause } else { Icon::Play },
                            true,
                            has_player,
                        ) {
                            toggle = true;
                        }
                        if icon_button(ui, Icon::Stop, false, has_player) {
                            stop = true;
                        }
                        ui.add_space(6.0);
                        // 左对齐 + 截断（add_sized 会居中，标题会跑到中间）
                        ui.add(
                            egui::Label::new(
                                RichText::new(&name)
                                    .size(11.0)
                                    .color(if has_player { TEXT } else { MUTED }),
                            )
                            .truncate(),
                        );
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let time_w = 92.0;
                        let bar_w = (ui.available_width() - time_w).max(60.0);
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(bar_w, 16.0),
                            egui::Sense::click_and_drag(),
                        );
                        let frac = ((pos / dur) as f32).clamp(0.0, 1.0);
                        let p = ui.painter();
                        // 细轨道 + 圆点：比之前一整条 10px 的胶囊干净
                        let track = egui::Rect::from_center_size(
                            rect.center(),
                            egui::vec2(rect.width(), 4.0),
                        );
                        p.rect_filled(track, Rounding::same(2.0), tint(Color32::WHITE, 16));
                        if frac > 0.0 {
                            let mut f = track;
                            f.set_width(track.width() * frac);
                            let c = if hdr() {
                                Color32::from_rgb(150, 240, 228)
                            } else {
                                ACCENT
                            };
                            if hdr() {
                                accent_glow(p, f, Rounding::same(2.0), 40);
                            }
                            p.rect_filled(f, Rounding::same(2.0), c);
                        }
                        let knob_r = if resp.hovered() || resp.dragged() {
                            6.0
                        } else {
                            4.5
                        };
                        p.circle_filled(
                            egui::pos2(track.left() + track.width() * frac, track.center().y),
                            knob_r,
                            if has_player { TEXT } else { DIM },
                        );
                        if has_player && (resp.clicked() || resp.dragged()) && rect.width() > 0.0 {
                            if let Some(pp) = resp.interact_pointer_pos() {
                                seek = Some(
                                    ((pp.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64
                                        * dur,
                                );
                            }
                        }
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                fmt_duration(pos),
                                fmt_duration(dur)
                            ))
                            .size(9.0)
                            .color(DIM),
                        );
                    });
                },
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut v = self
                    .player
                    .as_ref()
                    .map(|p| p.volume())
                    .unwrap_or(self.player_volume);
                // 和进度条同一套画法，不用 egui 默认 Slider（风格对不上）
                if volume_bar(ui, &mut v) {
                    vol_change = Some(v);
                }
                ui.add_space(6.0);
                if spec_button(ui) {
                    open_spec = true;
                }
            });
        });

        if open_spec {
            self.open_spectrum();
        }

        if open_cover {
            if let Some(i) = display {
                self.cover_view = Some(i);
                self.cover_closing = false;
                self.cover_t = 0.0; // 从缩小状态开始放大
                self.start_big_cover(i);
            }
        }
        if let Some(v) = vol_change {
            self.player_volume = v;
            if let Some(p) = self.player.as_ref() {
                p.set_volume(v);
            }
        }
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
        }
    }

    /// 点开大图时后台重新抽一次封面（队列里那张只有 96px）
    fn start_big_cover(&mut self, idx: usize) {
        if self
            .cover_big
            .as_ref()
            .map(|(i, _)| *i == idx)
            .unwrap_or(false)
        {
            return;
        }
        let Some(tools) = self.tools.clone() else { return };
        let Some(it) = self.items.get(idx) else { return };
        let path = it.path.clone();
        let tx = self.cover_big_tx.clone();
        std::thread::spawn(move || {
            let payload = fc_core::extract_cover(&tools.ffmpeg, &path).and_then(|bytes| {
                let img = image::load_from_memory(&bytes).ok()?;
                // 太大的图缩到 1000px 就够看了，别把显存吃光
                let img = if img.width() > 1000 || img.height() > 1000 {
                    img.resize(1000, 1000, image::imageops::FilterType::Triangle)
                } else {
                    img
                };
                let rgba = img.to_rgba8();
                Some((rgba.as_raw().to_vec(), rgba.width(), rgba.height()))
            });
            let _ = tx.send((idx, payload));
        });
    }

    fn poll_big_cover(&mut self, ctx: &egui::Context) {
        while let Ok((idx, payload)) = self.cover_big_rx.try_recv() {
            if let Some((rgba, w, h)) = payload {
                let img = egui::ColorImage {
                    size: [w as usize, h as usize],
                    pixels: rgba
                        .chunks_exact(4)
                        .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                        .collect(),
                };
                self.cover_big = Some((
                    idx,
                    ctx.load_texture(format!("cover_big_{idx}"), img, egui::TextureOptions::LINEAR),
                ));
            }
        }
    }

    /// 大图封面：全屏暗底 + 居中大图，点任意处或 Esc 关闭
    fn cover_overlay(&mut self, ctx: &egui::Context) {
        let Some(idx) = self.cover_view else { return };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // 不直接清掉：让 cover_t 缩回 0，动画走完再关
            self.cover_closing = true;
            return;
        }
        let screen = ctx.screen_rect();
        let t = self.cover_t.clamp(0.0, 1.0);
        let tex = self
            .cover_big
            .as_ref()
            .filter(|(i, _)| *i == idx)
            .map(|(_, t)| t.clone())
            .or_else(|| self.items.get(idx).and_then(|it| it.cover.clone()));
        let title = self
            .items
            .get(idx)
            .map(|it| it.tags.display_title(&it.name()))
            .unwrap_or_default();

        let mut close = false;
        egui::Area::new(egui::Id::new("cover_view"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let resp = ui.allocate_response(screen.size(), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(screen, 0.0, tint(BG, (236.0 * t) as u8));
                // 0.86 → 1.0 的放大，配着暗底一起淡入
                let pop = 0.86 + 0.14 * t;
                match tex.as_ref() {
                    Some(tx) => {
                        let max = egui::vec2(screen.width() * 0.8, screen.height() * 0.72);
                        let ts = tx.size_vec2().max(egui::vec2(1.0, 1.0));
                        let scale = (max.x / ts.x).min(max.y / ts.y).clamp(0.05, 4.0);
                        let size = ts * scale * pop;
                        let rect = egui::Rect::from_center_size(
                            screen.center() - egui::vec2(0.0, 10.0),
                            size,
                        );
                        p.image(
                            tx.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE.gamma_multiply(t),
                        );
                        p.text(
                            egui::pos2(screen.center().x, rect.bottom() + 24.0),
                            egui::Align2::CENTER_CENTER,
                            &title,
                            egui::FontId::proportional(11.5),
                            MUTED.gamma_multiply(t),
                        );
                    }
                    None => {
                        p.text(
                            screen.center(),
                            egui::Align2::CENTER_CENTER,
                            "这个文件没有内嵌封面",
                            egui::FontId::proportional(13.0),
                            MUTED.gamma_multiply(t),
                        );
                    }
                }
                p.text(
                    egui::pos2(screen.center().x, screen.bottom() - 24.0),
                    egui::Align2::CENTER_CENTER,
                    "点击任意处或按 Esc 关闭",
                    egui::FontId::proportional(9.5),
                    DIM.gamma_multiply(t),
                );
                if resp.clicked() {
                    close = true;
                }
            });
        if close {
            self.cover_closing = true;
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
                if let Some(q) = quality_of(m) {
                    ui.add_space(6.0);
                    animating |= quality_badge(ui, q);
                }
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
                    if let Some(b) = m.bits {
                        row(ui, "位深", &format!("{b} bit"));
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
                    let est = estimate_size(m, spec, &self.cfg.opts);
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

            // 源 → 目标 的质量提示：降码率、无损转有损、升采样都在 core 里判断，
            // 界面只负责画卡片，不自己写格式判断。
            let spec = it.spec(self.target);
            for w in fc_core::quality_warnings(m, spec, &self.cfg.opts) {
                ui.add_space(14.0);
                card(ui, WARN, |ui| {
                    para(ui, w.title, 11.5, WARN);
                    ui.add_space(5.0);
                    para(ui, &w.body, 10.0, MUTED);
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


    /// 当前参数的一句话摘要（"8 · 跟源" 这种），显示在输出列标题下
    fn knob_summary(&self) -> String {        use fc_core::Knob;
        let o = &self.cfg.opts;
        let mut parts: Vec<String> = Vec::new();
        for k in fc_core::knobs_for(self.target) {
            match k {
                Knob::FlacLevel => parts.push(format!("{}", o.flac_level)),
                Knob::BitDepth => parts.push(o.bit_depth.label().to_string()),
                Knob::Mp3Bitrate => parts.push(format!("{}k", o.mp3_bitrate)),
                Knob::OpusBitrate => parts.push(format!("{}k", o.opus_bitrate)),
                Knob::VorbisQuality => parts.push(format!("q{}", o.vorbis_q)),
                Knob::AacBitrate => parts.push(format!("{}k", o.aac_bitrate)),
                Knob::Ac3Bitrate => parts.push(format!("{}k", o.ac3_bitrate)),
                Knob::Eac3Bitrate => parts.push(format!("{}k", o.eac3_bitrate)),
                Knob::WmaBitrate => parts.push(format!("{}k", o.wma_bitrate)),
                Knob::Atrac3Bitrate => parts.push(format!("{}k", o.atrac3_bitrate)),
                Knob::SampleRate => match fc_core::output_rate(self.target, o) {
                    Some(r) => parts.push(fmt_rate(r)),
                    None => parts.push("跟源".into()),
                },
            }
        }
        if parts.is_empty() {
            // 没有旋钮的格式（ATRAC1 / ATRAC3+）也把固定码率写出来，
            // 否则标题只有"默认"，用户不知道会转成多少
            match fc_core::bitrate_of(self.target, o) {
                Some(k) => format!("{k}k"),
                None => "默认".into(),
            }
        } else {
            parts.join(" · ")
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

            // ── 用户自设预设（跟着配置 1/2/3 走）──
            let mut apply_preset: Option<usize> = None;
            let mut rename_preset: Option<usize> = None;
            let mut delete_preset: Option<usize> = None;
            for (i, p) in self.cfg.presets.iter().enumerate() {
                let on = self.target.id == p.format && self.cfg.opts == p.opts;
                let b = egui::Button::new(
                    RichText::new(&p.name).size(9.5).color(if on {
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
                let resp = ui
                    .add(b)
                    .on_hover_text("点一下套用这套参数；右键改名 / 删除");
                if resp.clicked() {
                    apply_preset = Some(i);
                }
                resp.context_menu(|ui| {
                    if ui.button("重命名").clicked() {
                        rename_preset = Some(i);
                        ui.close_menu();
                    }
                    if ui.button("删除").clicked() {
                        delete_preset = Some(i);
                        ui.close_menu();
                    }
                });
            }
            // 正在改名：就地换成输入框
            if let Some(i) = self.preset_rename {
                if i < self.cfg.presets.len() {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.preset_rename_buf)
                            .hint_text(self.cfg.presets[i].name.clone())
                            .desired_width(96.0)
                            .margin(egui::vec2(6.0, 3.0)),
                    );
                    if self.preset_rename_focus {
                        resp.request_focus();
                        self.preset_rename_focus = false;
                    }
                    if resp.lost_focus() || ui.input(|x| x.key_pressed(egui::Key::Enter)) {
                        let name = self.preset_rename_buf.trim().to_string();
                        if !name.is_empty() {
                            self.cfg.presets[i].name = name;
                        }
                        self.preset_rename = None;
                        self.preset_rename_buf.clear();
                        changed = true;
                    }
                } else {
                    self.preset_rename = None;
                }
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("＋ 存为预设").size(9.5).color(ACCENT))
                        .fill(tint(ACCENT, 14))
                        .stroke(line(ACCENT, 45))
                        .rounding(Rounding::same(7.0)),
                )
                .on_hover_text("把当前格式和参数存成一套，可改名，跟着「配置」一起保存")
                .clicked()
            {
                let n = self.cfg.presets.len() + 1;
                self.cfg.presets.push(settings::UserPreset {
                    name: format!("预设 {n}"),
                    format: self.target.id.to_string(),
                    opts: self.cfg.opts.clone(),
                });
                changed = true;
            }
            if let Some(i) = apply_preset {
                if let Some(p) = self.cfg.presets.get(i).cloned() {
                    if let Some(f) = fc_core::by_id(&p.format) {
                        self.target = f;
                    }
                    self.cfg.opts = p.opts;
                    changed = true;
                }
            }
            if let Some(i) = rename_preset {
                self.preset_rename = Some(i);
                self.preset_rename_buf.clear();
                self.preset_rename_focus = true;
            }
            if let Some(i) = delete_preset {
                if i < self.cfg.presets.len() {
                    self.cfg.presets.remove(i);
                    self.preset_rename = None;
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
        // 外部编码器：让用户知道这一步不是 ffmpeg 干的
        if let Some(name) = fc_core::external_encoder(self.target.id) {
            ui.add_space(4.0);
            para(
                ui,
                &format!("由外部编码器 {name} 完成（不随包，需你自己安装）。"),
                8.5,
                DIM,
            );
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

        if self.meta_open {
            let sel = self.selected;
            let has_file = self.items.get(sel).is_some();
            // 写入标签关着时字段照样显示（用户要求"元数据全展开"），
            // 只是提醒一句：这些值不写进文件。
            if !self.cfg.write_tags {
                ui.add_space(2.0);
                para(
                    ui,
                    "写入标签没开：下面的字段只会存在程序里，不会写进输出文件。",
                    8.5,
                    WARN,
                );
            }

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
        // 单个文件可能用 `···` 覆盖过输出格式，所以逐个按它自己的目标算，
        // 不能一律乘全局格式（否则队列里有覆盖时这个数就是错的）。
        let est = self
            .items
            .iter()
            .filter_map(|i| {
                i.info
                    .as_ref()
                    .map(|m| fc_core::estimate_size(m, i.spec(self.target), &self.cfg.opts))
            })
            .sum::<u64>();
        if est > 0 {
            para(ui, &format!("预计输出 {}", fmt_size(est)), 9.0, DIM);
        }

        // 这一列改了任何设置就落盘（以前只把 changed 当"要不要重绘"用，
        // 结果预设、码率这些改动重启就没了）
        if changed {
            self.cfg.save();
        }

        changed
    }

    /// 一个参数控件（紧凑版，300px 宽的面板里能用）
    fn knob_row(&mut self, ui: &mut egui::Ui, k: fc_core::Knob) -> bool {
        use fc_core::{BitDepth, Knob};
        let mut changed = false;
        match k {
            Knob::FlacLevel => {
                let mut v = self.cfg.opts.flac_level as i32;
                if level_slider(ui, "压缩等级", &mut v, 0, 12) {
                    self.cfg.opts.flac_level = v as u8;
                    changed = true;
                }
            }
            Knob::VorbisQuality => {
                let mut v = self.cfg.opts.vorbis_q as i32;
                if level_slider(ui, "质量", &mut v, 0, 10) {
                    self.cfg.opts.vorbis_q = v as u8;
                    changed = true;
                }
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
            Knob::Mp3Bitrate => changed |= bitrate_row(ui, "mp3", &mut self.cfg.opts.mp3_bitrate),
            Knob::OpusBitrate => {
                changed |= bitrate_row(ui, "opus", &mut self.cfg.opts.opus_bitrate)
            }
            Knob::AacBitrate => changed |= bitrate_row(ui, "aac", &mut self.cfg.opts.aac_bitrate),
            Knob::Ac3Bitrate => changed |= bitrate_row(ui, "ac3", &mut self.cfg.opts.ac3_bitrate),
            Knob::Eac3Bitrate => {
                changed |= bitrate_row(ui, "eac3", &mut self.cfg.opts.eac3_bitrate)
            }
            Knob::WmaBitrate => changed |= bitrate_row(ui, "wma", &mut self.cfg.opts.wma_bitrate),
            Knob::Atrac3Bitrate => {
                changed |= bitrate_row(ui, "atrac3", &mut self.cfg.opts.atrac3_bitrate)
            }
            Knob::SampleRate => changed |= self.sample_rate_row(ui),
        }
        changed
    }

    /// 采样率旋钮。档位表来自 core（`rate_choices`），界面不判断哪个编码器支持什么。
    fn sample_rate_row(&mut self, ui: &mut egui::Ui) -> bool {
        let choices = fc_core::rate_choices(self.target.id);
        let mut changed = false;
        let auto_on = self.cfg.opts.sample_rate.is_none();
        let effective = fc_core::output_rate(self.target, &self.cfg.opts);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("采样率").size(9.5).color(MUTED));
            // 「自动」永远在第一个位置，不按状态显示/隐藏——
            // 藏起来的话，点完它自己消失，看起来就像 bug（AC-3 上踩过）。
            if seg_button(ui, "自动", auto_on) {
                self.cfg.opts.sample_rate = None;
                changed = true;
            }
            for &v in choices {
                // 只有显式选过档位才高亮；自动状态下高亮留给「自动」。
                // 配置里可能是 96k、但当前格式只支持到 48k，所以看的是实际输出值。
                let on = !auto_on && effective == Some(v);
                if seg_button(ui, &fmt_rate(v), on) {
                    self.cfg.opts.sample_rate = Some(v);
                    changed = true;
                }
            }
        });
        para(ui, fc_core::rate_note(self.target), 8.5, DIM);
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
    Settings,
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
        TitleBtn::Settings => {
            // 三条滑杆（设置图标）。画成齿轮也行，但滑杆在小尺寸下更清楚。
            for (i, dy) in [-4.6_f32, 0.0, 4.6].into_iter().enumerate() {
                p.line_segment(
                    [c + egui::vec2(-6.5, dy), c + egui::vec2(6.5, dy)],
                    Stroke::new(1.2, col),
                );
                let kx = [-2.5_f32, 2.0, -1.0][i];
                p.circle_filled(c + egui::vec2(kx, dy), 1.9, col);
            }
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
/// `enabled = false` 时只画不接点击（没在播的时候按钮要灰着但别消失）。
fn icon_button(ui: &mut egui::Ui, icon: Icon, accent: bool, enabled: bool) -> bool {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), sense);
    let p = ui.painter();
    let bg = if !enabled {
        tint(Color32::WHITE, 4)
    } else if resp.hovered() {
        tint(Color32::WHITE, 22)
    } else {
        tint(Color32::WHITE, 8)
    };
    p.rect_filled(rect, Rounding::same(8.0), bg);

    let c = rect.center();
    let col = if !enabled {
        DIM
    } else if accent {
        ACCENT
    } else {
        TEXT
    };
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
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// 左侧两个大项之一。整行 34 高，选中时青色底
fn rail_big(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 34.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let fill = if on {
        tint(ACCENT, 30)
    } else if resp.hovered() {
        tint(Color32::WHITE, 12)
    } else {
        Color32::TRANSPARENT
    };
    if on {
        accent_glow(p, rect, Rounding::same(9.0), 55);
    }
    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect, Rounding::same(9.0), fill);
    }
    p.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.5),
        if on { ACCENT } else { TEXT },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// 音乐库分类的一行（右边带数量）
fn rail_sub(ui: &mut egui::Ui, label: &str, count: usize, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 26.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let fill = if on {
        tint(ACCENT, 26)
    } else if resp.hovered() {
        tint(Color32::WHITE, 10)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect, Rounding::same(8.0), fill);
    }
    p.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.0),
        if on { ACCENT } else { MUTED },
    );
    if count > 0 {
        p.text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            egui::FontId::proportional(9.0),
            DIM,
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// 音乐库的一首歌：封面 + 标题 + 艺术家·专辑 + 编码 + 音质标识 + 时长
fn lib_row(
    ui: &mut egui::Ui,
    e: &LibEntry,
    playing: bool,
    selected: bool,
    cover: Option<&egui::TextureHandle>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 50.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let hovered = resp.hovered();
    let fill = if selected {
        tint(ACCENT, 26)
    } else if hovered {
        tint(Color32::WHITE, 8)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect.shrink2(egui::vec2(0.0, 1.0)), Rounding::same(8.0), fill);
    }
    let left = rect.left() + 10.0;
    if playing {
        p.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(left - 6.0, rect.center().y - 9.0),
                egui::vec2(3.0, 18.0),
            ),
            Rounding::same(1.5),
            ACCENT,
        );
    }
    // 封面：37×37，圆角，没图给一块浅底
    let cover_rect = egui::Rect::from_min_size(
        egui::pos2(left, rect.center().y - 18.5),
        egui::vec2(37.0, 37.0),
    );
    p.rect_filled(cover_rect, Rounding::same(7.0), tint(Color32::WHITE, 10));
    if let Some(t) = cover {
        p.image(
            t.id(),
            cover_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        p.text(
            cover_rect.center(),
            egui::Align2::CENTER_CENTER,
            "♪",
            egui::FontId::proportional(13.0),
            DIM,
        );
    }

    // 右边的音质标识（上方）+ 时长（下方）
    let q = if e.codec == "DSD" {
        Some(Quality::Dsd)
    } else {
        quality_from(e.lossy, e.sample_rate, e.bits, e.bitrate_kbps)
    };
    let badge_w = q.map(quality_size).unwrap_or(0.0);
    let text_right = rect.right() - 12.0 - badge_w.max(46.0) - 8.0;
    let text_x = cover_rect.right() + 10.0;
    let title_rect = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top() + 7.0),
        egui::pos2(text_right, rect.top() + 27.0),
    );
    let sub_rect = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top() + 26.0),
        egui::pos2(text_right, rect.bottom() - 5.0),
    );
    paint_clipped(
        ui,
        title_rect,
        12.0,
        if playing { ACCENT } else { TEXT },
        &e.title,
    );
    // 副标题：编码 · 艺术家 · 专辑（编码放前面，用户一眼知道是什么格式）
    let mut parts: Vec<String> = vec![e.codec.clone()];
    if !e.artist.is_empty() {
        parts.push(e.artist.clone());
    }
    if !e.album.is_empty() {
        parts.push(e.album.clone());
    }
    if !e.genre.is_empty() {
        parts.push(e.genre.clone());
    }
    paint_clipped(ui, sub_rect, 9.5, MUTED, &parts.join(" · "));

    if let Some(q) = q {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 12.0 - badge_w, rect.top() + 6.0),
            egui::vec2(badge_w, 17.0),
        );
        let t = ui.input(|i| i.time);
        if paint_quality(ui.painter(), r, q, t) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(45));
        }
    }
    p.text(
        egui::pos2(rect.right() - 12.0, rect.top() + 33.0),
        egui::Align2::RIGHT_CENTER,
        fmt_duration(e.duration_s),
        egui::FontId::proportional(9.5),
        DIM,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 分组标题（专辑 / 艺术家 / 风格）+ 一条分隔线
fn lib_group(ui: &mut egui::Ui, title: &str, count: usize) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(11.5).color(TEXT).strong());
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{count} 首")).size(9.0).color(DIM));
    });
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 1.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, tint(Color32::WHITE, 10));
    ui.add_space(3.0);
}

/// 分组用的键
fn lib_key(tab: LibTab, e: &LibEntry) -> String {
    match tab {
        LibTab::Albums => e.album.clone(),
        LibTab::Artists => e.artist.clone(),
        LibTab::Genres => e.genre.clone(),
        _ => String::new(),
    }
}

/// 非空唯一值个数（专辑 / 艺术家 / 风格）
fn uniq(entries: &[LibEntry], key: impl Fn(&LibEntry) -> &String) -> usize {
    let mut set = std::collections::HashSet::new();
    for e in entries {
        let k = key(e).trim();
        if !k.is_empty() {
            set.insert(k);
        }
    }
    set.len()
}

/// 单行裁剪绘制（不会换行把行高撑破）
fn paint_clipped(ui: &egui::Ui, rect: egui::Rect, size: f32, color: Color32, text: &str) {
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

/// 统一风格的搜索框（输入框视觉在 App::new 里全局调过）
fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str, width: f32) -> bool {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(width)
            .margin(egui::vec2(10.0, 5.0)),
    )
    .changed()
}

fn file_stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string_lossy().to_string())
}

fn is_audio(p: &Path) -> bool {
    const EXT: &[&str] = &[
        "mp3", "mp2", "flac", "m4a", "m4b", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff",
        "wma", "ac3", "eac3", "oma", "aa3", "at3", "at9", "ape", "wv", "tta", "caf", "w64",
        "dsf", "dff", "mka", "wavpack",
    ];
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            EXT.contains(&e.as_str())
        })
        .unwrap_or(false)
}

/// 递归收集音频文件。深度和数量都设上限，避免误加 C:\ 这种目录卡死。
fn collect_audio(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 8 || out.len() > 20000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for ent in rd.flatten() {
        let p = ent.path();
        let Ok(ft) = ent.file_type() else { continue };
        if ft.is_dir() {
            if p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            collect_audio(&p, out, depth + 1);
        } else if ft.is_file() && is_audio(&p) {
            out.push(p);
        }
        if out.len() > 20000 {
            return;
        }
    }
}

/// 音乐库顶部的小按钮（主按钮是青色实心）
fn lib_button(ui: &mut egui::Ui, label: &str, primary: bool) -> bool {
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

/// 很小的文字按钮（导出/导入/＋）
fn tiny_text_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(label).size(9.0).color(MUTED))
            .fill(tint(Color32::WHITE, 8))
            .stroke(line(Color32::WHITE, 14))
            .rounding(Rounding::same(6.0)),
    )
    .clicked()
}

/// 单行裁剪绘制 + 垂直居中（路径那类文字要居中对齐才不"歪"）
fn paint_clipped_v(ui: &egui::Ui, rect: egui::Rect, size: f32, color: Color32, text: &str) {
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

/// 后台抽音乐库封面：48px 左右就够列表用，别把整张图塞进显存
fn spawn_lib_cover(
    ffmpeg: PathBuf,
    path: PathBuf,
    tx: std::sync::mpsc::Sender<(PathBuf, Option<(Vec<u8>, u32, u32)>)>,
) {
    std::thread::spawn(move || {
        let payload = fc_core::extract_cover(&ffmpeg, &path).and_then(|bytes| {
            let img = image::load_from_memory(&bytes).ok()?;
            let small = img.resize_to_fill(40, 40, image::imageops::FilterType::Triangle);
            let rgba = small.to_rgba8();
            Some((rgba.as_raw().to_vec(), rgba.width(), rgba.height()))
        });
        let _ = tx.send((path, payload));
    });
}

// ── 频谱视图 ──────────────────────────────────────────

/// 播放栏里的"看频谱"按钮：三根小电平条
fn spec_button(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    let p = ui.painter();
    let hot = resp.hovered();
    p.rect_filled(
        rect,
        Rounding::same(8.0),
        if hot {
            tint(Color32::WHITE, 16)
        } else {
            Color32::TRANSPARENT
        },
    );
    let c = if hot { ACCENT } else { MUTED };
    let heights = [6.0, 11.0, 8.0, 13.0];
    for (i, h) in heights.iter().enumerate() {
        let x = rect.center().x - 7.5 + i as f32 * 5.0;
        let r = egui::Rect::from_center_size(
            egui::pos2(x, rect.center().y + 1.0),
            egui::vec2(2.4, *h),
        );
        p.rect_filled(r, Rounding::same(1.2), c);
    }
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

impl App {
    /// 打开频谱视图：用当前播放（没在播就用选中项）的文件
    fn open_spectrum(&mut self) {
        let idx = self
            .player_for
            .or(if self.items.is_empty() {
                None
            } else {
                Some(self.selected)
            });
        let Some(i) = idx else { return };
        let Some(item) = self.items.get(i) else { return };
        let Some(tools) = self.tools.clone() else {
            self.spec_err = Some("找不到 ffmpeg，读不了频谱".into());
            self.spec_open = true;
            return;
        };
        let path = item.path.clone();
        let sr = item
            .info
            .as_ref()
            .map(|m| m.sample_rate)
            .filter(|v| *v > 0)
            .unwrap_or(44100);
        self.spec_open = true;
        self.spec_busy = true;
        self.spec_curve.clear();
        self.spec_for = Some(i);
        self.spec_err = None;
        let tx = self.spec_tx.clone();
        std::thread::spawn(move || {
            let r = fc_analyze::spectrum(&tools.ffmpeg, &path, sr, 260)
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
    }

    fn poll_spectrum(&mut self) {
        while let Ok(r) = self.spec_rx.try_recv() {
            self.spec_busy = false;
            match r {
                Ok(c) => self.spec_curve = c,
                Err(e) => self.spec_err = Some(e),
            }
        }
    }

    /// 全屏频谱：对数频率轴 + dB 网格 + 曲线
    fn spectrum_overlay(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.spec_open = false;
            return;
        }
        let screen = ctx.screen_rect();
        let title = self
            .spec_for
            .and_then(|i| self.items.get(i))
            .map(|it| it.tags.display_title(&it.name()))
            .unwrap_or_default();
        let spec = self
            .spec_for
            .and_then(|i| self.items.get(i))
            .and_then(|it| it.info.clone())
            .map(|m| {
                let mut s = m.codec_label().to_string();
                if m.sample_rate > 0 {
                    s.push_str(&format!(" · {}", fmt_khz(m.sample_rate)));
                }
                if let Some(b) = m.bits {
                    s.push_str(&format!(" · {b} bit"));
                }
                if m.bitrate_kbps > 0 {
                    s.push_str(&format!(" · {} kbps", m.bitrate_kbps));
                }
                s
            })
            .unwrap_or_default();
        let curve = self.spec_curve.clone();
        let busy = self.spec_busy;
        let err = self.spec_err.clone();
        let mut close = false;
        egui::Area::new(egui::Id::new("spectrum_view"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let resp = ui.allocate_response(screen.size(), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(screen, 0.0, tint(BG, 236));
                if resp.clicked() {
                    close = true;
                }

                let panel = egui::Rect::from_center_size(
                    screen.center() - egui::vec2(0.0, 6.0),
                    egui::vec2(screen.width() * 0.74, 380.0),
                );
                p.rect_filled(panel, Rounding::same(18.0), tint(BG, 228));
                p.rect_stroke(
                    panel,
                    Rounding::same(18.0),
                    Stroke::new(1.0, tint(Color32::WHITE, 26)),
                );

                p.text(
                    panel.min + egui::vec2(24.0, 22.0),
                    egui::Align2::LEFT_CENTER,
                    "频谱",
                    egui::FontId::proportional(13.0),
                    TEXT,
                );
                p.text(
                    panel.min + egui::vec2(66.0, 22.0),
                    egui::Align2::LEFT_CENTER,
                    &title,
                    egui::FontId::proportional(10.5),
                    MUTED,
                );
                p.text(
                    egui::pos2(panel.right() - 24.0, panel.min.y + 22.0),
                    egui::Align2::RIGHT_CENTER,
                    &spec,
                    egui::FontId::proportional(9.0),
                    DIM,
                );

                // 绘图区（留左边给 dB 刻度、下面给频率刻度）
                let plot = egui::Rect::from_min_max(
                    panel.min + egui::vec2(52.0, 46.0),
                    panel.max - egui::vec2(24.0, 40.0),
                );
                p.rect_filled(plot, Rounding::same(8.0), tint(Color32::BLACK, 40));

                // dB 网格：0 / -30 / -60 / -90
                for db in [0.0f32, -30.0, -60.0, -90.0] {
                    let y = plot.bottom() - ((db + 90.0) / 90.0) * plot.height();
                    p.line_segment(
                        [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                        Stroke::new(1.0, tint(Color32::WHITE, 12)),
                    );
                    p.text(
                        egui::pos2(plot.left() - 6.0, y),
                        egui::Align2::RIGHT_CENTER,
                        format!("{db:.0}"),
                        egui::FontId::proportional(8.0),
                        DIM,
                    );
                }
                // 频率网格：100 / 1k / 10k
                let f_lo = 20.0f64;
                let f_hi = 20000.0f64;
                for (f, label) in [
                    (100.0, "100"),
                    (1000.0, "1k"),
                    (10000.0, "10k"),
                ] {
                    let x = plot.left()
                        + ((f / f_lo).log10() / (f_hi / f_lo).log10()) as f32 * plot.width();
                    p.line_segment(
                        [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
                        Stroke::new(1.0, tint(Color32::WHITE, 10)),
                    );
                    p.text(
                        egui::pos2(x, plot.bottom() + 10.0),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(8.0),
                        DIM,
                    );
                }

                if !curve.is_empty() {
                    let pts: Vec<egui::Pos2> = curve
                        .iter()
                        .map(|(f, db)| {
                            let t = ((f / f_lo).log10() / (f_hi / f_lo).log10())
                                .clamp(0.0, 1.0) as f32;
                            let y =
                                plot.bottom() - (((db + 90.0) / 90.0).clamp(0.0, 1.0) as f32) * plot.height();
                            egui::pos2(plot.left() + t * plot.width(), y)
                        })
                        .collect();
                    // 一段一段填（每段是凸四边形），整条曲线不是凸的，不能一次画
                    for w in pts.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        p.add(egui::Shape::convex_polygon(
                            vec![
                                a,
                                b,
                                egui::pos2(b.x, plot.bottom()),
                                egui::pos2(a.x, plot.bottom()),
                            ],
                            tint(ACCENT, 26),
                            Stroke::NONE,
                        ));
                    }
                    p.add(egui::Shape::line(
                        pts,
                        Stroke::new(1.4, ACCENT),
                    ));
                }

                if busy {
                    p.text(
                        plot.center(),
                        egui::Align2::CENTER_CENTER,
                        "正在算频谱…",
                        egui::FontId::proportional(11.0),
                        MUTED,
                    );
                } else if let Some(e) = &err {
                    p.text(
                        plot.center(),
                        egui::Align2::CENTER_CENTER,
                        e,
                        egui::FontId::proportional(10.0),
                        DANGER,
                    );
                } else if curve.is_empty() {
                    p.text(
                        plot.center(),
                        egui::Align2::CENTER_CENTER,
                        "没有可显示的数据",
                        egui::FontId::proportional(10.0),
                        DIM,
                    );
                }

                p.text(
                    egui::pos2(panel.center().x, panel.bottom() - 16.0),
                    egui::Align2::CENTER_CENTER,
                    "点击任意处或按 Esc 关闭",
                    egui::FontId::proportional(8.5),
                    DIM,
                );
            });
        if close {
            self.spec_open = false;
        }
    }
}

/// 音质档位：列表/详情里的小标识
#[derive(Clone, Copy, PartialEq, Eq)]
enum Quality {
    Dsd,
    HiRes,
    Cd,
    Hq,
    Std,
    Lq,
}

/// 从"有没有损 + 采样率 + 位深 + 码率"判档（音乐库的缓存里只有这些字段）
fn quality_from(lossy: bool, sr: u32, bits: Option<u32>, kbps: u64) -> Option<Quality> {
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
    Some(if kbps >= 192 {
        Quality::Hq
    } else if kbps >= 96 {
        Quality::Std
    } else {
        Quality::Lq
    })
}

fn quality_of(m: &fc_core::MediaInfo) -> Option<Quality> {
    if m.codec.is_empty() || m.sample_rate == 0 {
        return None;
    }
    if m.codec.starts_with("dsd") {
        return Some(Quality::Dsd);
    }
    quality_from(m.is_lossy(), m.sample_rate, m.bits, m.bitrate_kbps)
}

fn quality_size(q: Quality) -> f32 {
    match q {
        Quality::Dsd => 48.0,
        Quality::HiRes => 56.0,
        Quality::Cd => 38.0,
        Quality::Hq => 28.0,
        Quality::Std => 32.0,
        Quality::Lq => 28.0,
    }
}

/// 在给定矩形里画标识。返回 true = 有动画（调用方负责请求重绘）。
fn paint_quality(p: &egui::Painter, rect: egui::Rect, q: Quality, t: f64) -> bool {
    let r = Rounding::same(8.5);
    let label = match q {
        Quality::Dsd => "DSD",
        Quality::HiRes => "Hi-Res",
        Quality::Cd => "CD",
        Quality::Hq => "HQ",
        Quality::Std => "STD",
        Quality::Lq => "LQ",
    };
    match q {
        Quality::HiRes => {
            // 金属金：上深下亮的渐变 + 呼吸外发光 + 斜向扫光
            let pulse = 0.5 + 0.5 * (t * 1.7).sin() as f32;
            let gold = Color32::from_rgb(247, 199, 108);
            let gold_hi = Color32::from_rgb(255, 238, 186);
            let deep = Color32::from_rgb(92, 62, 18);
            p.rect_filled(rect, r, deep);
            let clip = p.with_clip_rect(rect);
            let steps = 5;
            for i in 0..steps {
                let f = i as f32 / steps as f32;
                let band = egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.top() + f * rect.height()),
                    egui::pos2(
                        rect.right(),
                        rect.top() + (i + 1) as f32 / steps as f32 * rect.height() + 1.0,
                    ),
                );
                clip.rect_filled(band, Rounding::ZERO, blend(deep, gold, 0.25 + f * 0.85));
            }
            let phase = ((t * 0.4) % 1.0) as f32;
            // 只在中间平直段扫，而且用三层不同宽度叠出软边，
            // 一层硬边会看着像多出来一个小方块
            let span = (rect.width() - 34.0).max(8.0);
            let bx = rect.left() + 17.0 + phase * span;
            let slant = 7.0;
            for (i, w) in [16.0f32, 10.0, 5.0].iter().enumerate() {
                let a = (16.0 + 10.0 * pulse) as u8 / (i as u8 + 1);
                clip.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(bx, rect.top()),
                        egui::pos2(bx + w, rect.top()),
                        egui::pos2(bx + w - slant, rect.bottom()),
                        egui::pos2(bx - slant, rect.bottom()),
                    ],
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
        Quality::Dsd => {
            // 深紫底 + 一条流动的"1-bit 码流"（点串），右边写 DSD
            let violet = Color32::from_rgb(186, 160, 255);
            p.rect_filled(rect, r, Color32::from_rgb(30, 22, 54));
            p.rect_stroke(rect, r, Stroke::new(1.0, tint(violet, 140)));
            let clip = p.with_clip_rect(rect.shrink(1.5));
            let n = 12;
            for i in 0..n {
                let f = i as f32 / (n - 1) as f32;
                let phase = t as f32 * 3.2 + f * 9.0;
                let y = rect.center().y + phase.sin() * 4.0;
                let x = rect.left() + 7.0 + f * (rect.width() - 30.0);
                let a = (110.0 + 120.0 * phase.sin().abs()) as u8;
                clip.circle_filled(egui::pos2(x, y), 1.15, tint(violet, a));
            }
            p.text(
                egui::pos2(rect.right() - 6.5, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                label,
                egui::FontId::proportional(8.5),
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
fn quality_badge(ui: &mut egui::Ui, q: Quality) -> bool {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(quality_size(q), 17.0),
        egui::Sense::hover(),
    );
    let t = ui.input(|i| i.time);
    let anim = paint_quality(ui.painter(), rect, q, t);
    if anim {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(45));
    }
    anim
}

/// 采样率写法：44100 -> "44.1 kHz"，48000 -> "48 kHz"
fn fmt_khz(sr: u32) -> String {
    if sr % 1000 == 0 {
        format!("{} kHz", sr / 1000)
    } else {
        format!("{:.1} kHz", sr as f64 / 1000.0)
    }
}

/// 更短的采样率写法：44100 -> "44.1k"，给列表里省地方
fn fmt_k(sr: u32) -> String {
    if sr % 1000 == 0 {
        format!("{}k", sr / 1000)
    } else {
        format!("{:.1}k", sr as f64 / 1000.0)
    }
}

use std::sync::atomic::{AtomicBool, Ordering};

/// HDR 模式（界面观感）：更亮的强调色 + 发光。只影响渲染，不改任何音频参数。
static HDR_ON: AtomicBool = AtomicBool::new(false);

pub fn hdr() -> bool {
    HDR_ON.load(Ordering::Relaxed)
}

fn set_hdr(on: bool) {
    HDR_ON.store(on, Ordering::Relaxed);
}

/// 在矩形外面套几层很淡的圆角光晕（HDR 模式下才画）
fn accent_glow(p: &egui::Painter, rect: egui::Rect, rounding: Rounding, strength: u8) {
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
fn bar_slider(ui: &mut egui::Ui, v: &mut i32, min: i32, max: i32, ticks: bool) -> bool {
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
fn level_slider(ui: &mut egui::Ui, label: &str, v: &mut i32, min: i32, max: i32) -> bool {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(9.5).color(MUTED));
        bar_slider(ui, v, min, max, true)
    })
    .inner
}

/// 音量条：和进度条同一套画法（细轨道 + 圆点），拖动调音量。
fn volume_bar(ui: &mut egui::Ui, value: &mut f32) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(64.0, 16.0), egui::Sense::click_and_drag());
    let p = ui.painter();
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
    p.rect_filled(track, Rounding::same(2.0), tint(Color32::WHITE, 16));
    let frac = value.clamp(0.0, 1.0);
    if frac > 0.0 {
        let mut f = track;
        f.set_width(track.width() * frac);
        p.rect_filled(f, Rounding::same(2.0), tint(MUTED, 200));
    }
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(
        egui::pos2(track.left() + track.width() * frac, track.center().y),
        if hot { 6.0 } else { 4.5 },
        if hot { TEXT } else { MUTED },
    );
    if resp.clicked() || resp.dragged() {
        if let Some(pp) = resp.interact_pointer_pos() {
            *value = ((pp.x - track.left()) / track.width()).clamp(0.0, 1.0);
            return true;
        }
    }
    false
}

/// 播放栏的封面按钮：48px 方块，有图显示图，没图给占位；返回 response 供点击。
fn bar_cover(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, size: f32) -> egui::Response {    // 没封面就别装成能点的样子（不然光标变成手型、点了没反应）
    let sense = if tex.is_some() {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), sense);
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(9.0), tint(Color32::WHITE, 10));
    match tex {
        Some(t) => {
            p.image(
                t.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            p.rect_filled(
                egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)),
                Rounding::same(3.0),
                tint(Color32::WHITE, 18),
            );
        }
    }
    if resp.hovered() && tex.is_some() {
        p.rect_stroke(
            rect,
            Rounding::same(9.0),
            Stroke::new(1.0, tint(Color32::WHITE, 80)),
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 读一张收款码图片（放程序目录，用户自己换）。没有就返回 None。
fn load_qr(ctx: &egui::Context, name: &str) -> Option<egui::TextureHandle> {
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

/// 码率旋钮：档位表来自 core，界面不自己维护格式知识。
///
/// 顺手把「选这个值要注意什么」画在下面——AC-3 的 48kHz、WMA 的老播放器上限
/// 这类信息比一个数字有用得多。
fn bitrate_row(ui: &mut egui::Ui, id: &str, val: &mut u32) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("码率").size(9.5).color(MUTED));
        for &v in fc_core::bitrate_choices(id) {
            let on = *val == v;
            if seg_button(ui, &format!("{v}"), on) {
                *val = v;
                changed = true;
            }
        }
    });
    if let Some(note) = fc_core::by_id(id).and_then(fc_core::knob_note) {
        para(ui, note, 8.5, DIM);
    }
    changed
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
    let title: Vec<u16> = format!("{APP_NAME}\0").encode_utf16().collect();
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

    // 命令行：打印当前用到的 ffmpeg/ffprobe 路径
    if args.first().map(|s| s.as_str()) == Some("--tools") {
        match fc_core::locate() {
            Some(t) => {
                println!("ffmpeg : {}", t.ffmpeg.display());
                println!("ffprobe: {}", t.ffprobe.display());
            }
            None => println!("没找到 ffmpeg（需要 ffmpeg.exe + ffprobe.exe）"),
        }
        match fc_core::find_atracdenc() {
            Some(p) => println!("atracdenc: {}", p.display()),
            None => println!("atracdenc: 未安装（ATRAC 输出需要它，见设置页说明）"),
        }
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
            .with_inner_size([1302.0 * BASE_ZOOM, 700.0 * BASE_ZOOM])
            .with_min_inner_size([1212.0 * BASE_ZOOM, 600.0 * BASE_ZOOM])
            .with_decorations(false)
            .with_transparent(true)
            .with_title(APP_NAME);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
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
