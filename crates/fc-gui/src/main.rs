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

mod util;
mod badges;
mod titlebar;
mod rail;
mod queue;
mod player;
mod spectrum;
mod detail;
mod output;
mod library;
mod overlays;
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
use util::*;
use badges::*;
use titlebar::*;
use rail::*;
use queue::*;
use player::*;
use spectrum::*;
use detail::*;
use output::*;
use library::*;
use overlays::*;

/// 频谱分析的状态
pub(crate) enum Analysis {
    Idle,
    Running,
    Done(QualityReport),
    Failed(String),
}

/// 左侧两个大项：音乐库 / 输出（转换）
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Convert,
    Library,
}

/// 音乐库里的分类
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibTab {
    Songs,
    Albums,
    Genres,
    Artists,
    Folders,
}


#[derive(Clone)]
pub(crate) struct LibEntry {
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

pub(crate) enum LibMsg {
    /// 先告诉界面一共多少个文件，进度条才有分母
    Count(usize),
    Entry(LibEntry),
    Done,
}

// ── 配色 ────────────────────────────────────────────────
pub(crate) const TEXT: Color32 = Color32::from_rgb(240, 244, 250);
pub(crate) const MUTED: Color32 = Color32::from_rgb(158, 168, 186);
pub(crate) const DIM: Color32 = Color32::from_rgb(112, 122, 140);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(122, 226, 214);
pub(crate) const WARN: Color32 = Color32::from_rgb(255, 198, 122);
pub(crate) const DANGER: Color32 = Color32::from_rgb(255, 138, 138);
pub(crate) const BG: Color32 = Color32::from_rgb(14, 16, 22);

/// 队列能接受的输入扩展名：音频 + 视频容器。
///
/// 视频不转视频——ffmpeg 那边固定 `-map 0:a:0`，只取第一条音轨，
/// 导出的还是上面选的那个音频格式。
pub(crate) const INPUT_EXTS: &[&str] = &[
    "mp3", "mp2", "flac", "wav", "m4a", "aac", "ogg", "oga", "opus", "wma", "ape", "wv", "tta",
    "oma", "aa3", "at3", "at9", "aiff", "aif", "caf", "w64", "ac3", "eac3",
    // 视频容器：只提取音轨
    "mkv", "mp4", "m4v", "mov", "avi", "webm", "ts", "m2ts", "wmv", "flv", "mpg", "mpeg",
    "3gp", "ogv", "vob",
];

// ── 品牌 ────────────────────────────────────────────────
/// 应用名：窗口标题 / 任务栏 / 标题栏都用它。改名只改这一行
/// （配置目录还叫 FormatConverter，改了会丢用户数据，等真换名再一起迁）。
pub const APP_NAME: &str = "ToneBox";

/// 界面基准缩放：旧版 120% 现在就是 100%。所有倍率都乘在它上面
pub const BASE_ZOOM: f32 = 1.2;

// ── 布局规范 ────────────────────────────────────────────
// 面板宽度必须是常量，内容宽度在面板内锁死。
// 所有长文本一律显式 wrap，任何元素都不允许横向溢出把窗口边缘切掉。
pub(crate) const TITLEBAR_H: f32 = 44.0;
pub(crate) const LEFT_W: f32 = 216.0;
pub(crate) const RIGHT_W: f32 = 300.0;
pub(crate) const OUT_W: f32 = 272.0;


// ── 队列条目 ────────────────────────────────────────────
#[derive(Clone)]
pub(crate) enum State {
    Pending,
    Running(f32),
    Done(PathBuf, u64),
    Failed(String),
}

pub(crate) struct Item {
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
pub(crate) struct MediaMsg {
    idx: usize,
    tags: fc_core::Tags,
    cover: Option<(Vec<u8>, u32, u32)>,
}

pub(crate) enum Msg {
    Progress(usize, f32),
    Done(usize, PathBuf, u64),
    Failed(usize, String),
    Finished,
}

/// 一个转换任务。ATRAC 走外部工具，参数和 ffmpeg 完全不一样，所以分成两种。
pub(crate) enum Job {
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

/// 一条完成/失败通知（标题栏铃铛里的那条）
#[derive(Clone)]
pub(crate) struct Notice {
    ok: bool,
    title: String,
    body: String,
    read: bool,
}

pub(crate) struct App {
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
    /// 完成/失败通知；标题栏铃铛上的红点看的是未读数量
    notices: Vec<Notice>,
    notices_open: bool,
    /// 铃铛这一帧的矩形（通知面板要判断"点外面关闭"时避开它）
    bell_rect: egui::Rect,
    /// 「删除源文件」已经按住了多少秒（0..3），松手清零
    delete_hold: f32,
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
    /// 切页动画进度 0~1（1 = 静止）
    page_t: f32,
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
            meta_open: false,
            notices: Vec::new(),
            notices_open: false,
            bell_rect: egui::Rect::NOTHING,
            delete_hold: 0.0,
            donate_wechat: load_qr(&cc.egui_ctx, "donate-wechat.png"),
            donate_alipay: load_qr(&cc.egui_ctx, "donate-alipay.png"),
            encoders,
            atracdenc: fc_core::find_atracdenc(),
            wiki_open: false,
            wiki_sel: "flac",
            wiki_filter: String::new(),
            page: Page::Convert,
            page_t: 1.0,
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
                    let name = self
                        .items
                        .get(i)
                        .map(|it| it.tags.display_title(&it.name()))
                        .unwrap_or_default();
                    let out_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
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
                    self.push_notice(
                        true,
                        "转换完成",
                        format!("{name} → {out_name} · {}", fmt_size(size)),
                    );
                }
                Msg::Failed(i, e) => {
                    let name = self
                        .items
                        .get(i)
                        .map(|it| it.tags.display_title(&it.name()))
                        .unwrap_or_default();
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = State::Failed(e.clone());
                    }
                    self.batch_done += 1;
                    self.push_notice(false, "转换失败", format!("{name}\n{e}"));
                }
                Msg::Finished => {
                    self.running = false;
                    self.rx = None;
                    // 一批多个文件时再补一条汇总，单文件就别重复了
                    if self.batch_total > 1 {
                        let fail = self
                            .items
                            .iter()
                            .filter(|i| matches!(i.state, State::Failed(_)))
                            .count();
                        let ok = self.batch_total.saturating_sub(fail);
                        self.push_notice(
                            fail == 0,
                            "本批转换结束",
                            format!("{ok} 个成功，{fail} 个失败"),
                        );
                    }
                }
            }
        }
    }

    /// 往铃铛里塞一条通知（最多留 50 条）
    fn push_notice(&mut self, ok: bool, title: &str, body: String) {
        self.notices.push(Notice {
            ok,
            title: title.to_string(),
            body,
            read: false,
        });
        if self.notices.len() > 50 {
            self.notices.remove(0);
        }
    }

    fn unread_notices(&self) -> usize {
        self.notices.iter().filter(|n| !n.read).count()
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
                                let can = !self.running
                                    && self.tools.is_some()
                                    && self.items.iter().any(|i| {
                                        i.info.is_some() && matches!(i.state, State::Pending)
                                    });
                                if start_button(ui, can, self.running, self.overall()) {
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

            // 播放栏不再占布局：放到队列/详情之上，让两栏一直滚到窗口底（见下面的悬浮 Area）

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

            // 切页：从左往右揭开新页面（一个渐变的遮罩扫过去）
            animating |= approach(&mut self.page_t, 1.0, self.dt, 9.0);
            if self.page_t < 0.999 {
                animating = true;
                let p = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("page_wipe"),
                ));
                let scr = ctx.screen_rect();
                let x0 = LEFT_W + inset;
                let y0 = TITLEBAR_H;
                let w = (scr.right() - x0).max(1.0);
                let edge = x0 + w * self.page_t;
                let soft = (w * 0.16).max(28.0);
                // 用 Mesh 做水平渐变，比一条条矩形干净（不会有竖条纹）
                let xs = [
                    x0,
                    (edge - soft).clamp(x0, scr.right()),
                    (edge + soft).clamp(x0, scr.right()),
                    scr.right(),
                ];
                let alphas = [0u8, 0, 236, 236];
                let mut mesh = egui::Mesh::default();
                for (i, x) in xs.iter().enumerate() {
                    let c = tint(BG, alphas[i]);
                    mesh.colored_vertex(egui::pos2(*x, y0), c);
                    mesh.colored_vertex(egui::pos2(*x, scr.bottom()), c);
                }
                for i in 0..3u32 {
                    let b = i * 2;
                    mesh.add_triangle(b, b + 1, b + 2);
                    mesh.add_triangle(b + 1, b + 3, b + 2);
                }
                p.add(egui::Shape::mesh(mesh));
            }

            // 悬浮播放栏：不占布局，盖在「队列 + 详情」上面——两栏可以一直滚到窗口底。
            // 用 Order::Middle：面板内容在 Background，封面/频谱/设置那些覆盖层在 Foreground。
            let screen = ctx.screen_rect();
            let bar_h = 80.0;
            // 左右各缩进一点，不贴满两栏（用户要"短一点"）
            let bar_pad = 26.0;
            let bar_w = if self.page == Page::Convert {
                (screen.width() - LEFT_W - OUT_W - inset * 2.0 - bar_pad * 2.0).max(240.0)
            } else {
                (screen.width() - LEFT_W - inset * 2.0 - bar_pad * 2.0).max(240.0)
            };
            let bar_pos = egui::pos2(
                LEFT_W + inset + bar_pad,
                screen.bottom() - bar_h - 10.0 - inset,
            );
            egui::Area::new(egui::Id::new("player_float"))
                .order(egui::Order::Middle)
                .fixed_pos(bar_pos)
                .show(ctx, |ui| {
                    ui.set_width(bar_w);
                    egui::Frame::none()
                        // 半透明玻璃（列表已经在上面收住，不会有文字透过来）
                        .fill(tint(BG, 150))
                        .rounding(Rounding::same(14.0))
                        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.player_bar(ui);
                        });
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
        // 通知面板最后画：它在铃铛下面，得盖住内容
        self.notices_overlay(ctx);

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
pub(crate) fn spawn_media_load(
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
pub(crate) fn output_dir_for(input: &Path, cfg: &settings::Settings) -> PathBuf {
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

pub(crate) fn unique_output_in(input: &Path, ext: &str, cfg: &settings::Settings) -> PathBuf {
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


// ── 频谱视图 ──────────────────────────────────────────


use std::sync::atomic::{AtomicBool, Ordering};


pub(crate) fn install_fonts(ctx: &egui::Context) {
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
pub(crate) fn apply_backdrop() -> bool {
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
pub(crate) fn apply_backdrop() -> bool {
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
