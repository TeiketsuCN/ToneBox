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
mod single;
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
mod tasks;
mod codec;
mod bg;
#[cfg(windows)]
mod frame;
mod settings;
mod sysstat;
mod boot;
mod assoc;
mod i18n;

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
use tasks::*;

/// 频谱分析的状态
pub(crate) enum Analysis {
    Idle,
    Running,
    Done(QualityReport),
    Failed(String),
}

/// 左侧三个大项：音乐库 / 编解码（转换）/ 引擎和设备
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Convert,
    Library,
    /// 「引擎和设备」：音频设备、播放输出选择、引擎信息
    Engine,
}

/// 音乐库里的分类
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibTab {
    Songs,
    Playlists,
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
/// 强调色现在跟着背景走（见 `util::accent`）：选中条 / 圆点 / 进度条 / 徽章
/// 全部在绘制时读它。这里的 `pub(crate) use` 让各子模块 `use crate::*` 直接拿到。
pub(crate) use util::{accent, on_accent, set_accent};
pub(crate) const WARN: Color32 = Color32::from_rgb(255, 198, 122);
pub(crate) const DANGER: Color32 = Color32::from_rgb(255, 138, 138);
/// "正常 / 加分"的绿：内容体检的 OK 分项、置信度里的 `+X%` 都用它
pub(crate) const GOOD: Color32 = Color32::from_rgb(110, 220, 140);
pub(crate) const BG: Color32 = Color32::from_rgb(14, 16, 22);

/// 队列能接受的输入扩展名：音频 + 视频容器。
///
/// 视频容器现在也是正式的输出目标（`Group::Video`）：可以只换容器（copy），
/// 也可以选 H.264/H.265/VP9/AV1 重编码。画面取第一条，**音轨全保留**。
pub(crate) const INPUT_EXTS: &[&str] = &[
    "mp3", "mp2", "flac", "wav", "m4a", "aac", "ogg", "oga", "opus", "wma", "ape", "wv", "tta",
    "oma", "aa3", "at3", "at9", "aiff", "aif", "caf", "w64", "ac3", "eac3",
    // 视频容器：只提取音轨
    "mkv", "mp4", "m4v", "mov", "avi", "webm", "ts", "m2ts", "wmv", "flv", "mpg", "mpeg",
    "3gp", "ogv", "vob",
];

// ── 品牌 ────────────────────────────────────────────────
/// 应用名：窗口标题 / 任务栏 / 标题栏都用它。改名只改这一行
/// （配置目录还叫 ToneLore，改了会丢用户数据，等真换名再一起迁）。
pub const APP_NAME: &str = "声识ToneLore";
/// 版本号只写这一处：关于页、启动过场都读它，别再多抄几份
pub(crate) const VERSION: &str = "2.0.0";

/// 界面基准缩放：旧版 120% 现在就是 100%。所有倍率都乘在它上面
pub const BASE_ZOOM: f32 = 1.2;

/// 这台机器该用多大缩放。
///
/// 原来是写死 1.2 的——开发机是 2K（1440 高）时正好，搬到 1080p 就"什么都大一号、
/// 四栏挤到窗口外面"。现在启动时按屏幕逻辑高度挑一次：够高才用 1.2。
static ZOOM_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(crate) fn zoom() -> f32 {
    let b = ZOOM_BITS.load(std::sync::atomic::Ordering::Relaxed);
    if b == 0 {
        BASE_ZOOM
    } else {
        f32::from_bits(b)
    }
}

fn set_zoom(z: f32) {
    ZOOM_BITS.store(f32::to_bits(z), std::sync::atomic::Ordering::Relaxed);
}

/// 屏幕逻辑高度：1080p/100% = 1080，2K/100% = 1440，1080p/150% = 720
#[cfg(windows)]
fn screen_logical_height() -> f32 {
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
        fn GetDpiForSystem() -> u32;
    }
    let h = unsafe { GetSystemMetrics(1) } as f32; // SM_CYSCREEN
    let dpi = unsafe { GetDpiForSystem() }.max(96) as f32;
    h * 96.0 / dpi
}

#[cfg(not(windows))]
fn screen_logical_height() -> f32 {
    1440.0
}

// ── 布局规范 ────────────────────────────────────────────
// 面板宽度必须是常量，内容宽度在面板内锁死。
// 所有长文本一律显式 wrap，任何元素都不允许横向溢出把窗口边缘切掉。
pub(crate) const TITLEBAR_H: f32 = 44.0;
pub(crate) const LEFT_W: f32 = 216.0;
pub(crate) const RIGHT_W: f32 = 300.0;
pub(crate) const OUT_W: f32 = 300.0;


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
    /// vgmstream 解出来的临时 WAV；没有就是 None（大多数格式直接读原文件）
    media: Option<PathBuf>,
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
    /// 整合响度（LUFS）/ 采样峰值；分析线程顺手测的，测不出来就是 None
    lufs: Option<fc_core::loudness::Loudness>,
    /// 读不出来时，如果认出是加密格式，把识别结果留着给详情页显示
    encrypted: Option<fc_core::Encrypted>,
    /// 内容体检（静音/削波/噪声/测试信号……）
    health: Option<fc_analyze::health::Health>,
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
    /// 真正拿来解码/播放的文件：vgmstream 解过的用临时 WAV，否则就是原文件
    fn media_path(&self) -> &Path {
        self.media.as_deref().unwrap_or(&self.path)
    }
    /// 删掉为这个文件生成的临时 WAV（只删 %TEMP%\ToneLore 下的，绝不碰源文件）
    pub(crate) fn forget_media(&self) {
        let Some(m) = &self.media else { return };
        let root = std::env::temp_dir().join("ToneLore");
        if m.starts_with(&root) {
            let _ = std::fs::remove_file(m);
        }
    }
}

/// 后台读完标签/封面后回传
pub(crate) struct MediaMsg {
    path: PathBuf,
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
        /// 响度归一化目标（LUFS）；None = 不动
        norm: Option<f32>,
    },
    /// 外部 atracdenc：先 ffmpeg 解码成 WAV，再交给它编码
    Atrac {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        codec: &'static str,
        dur: f64,
    },
    /// 随包 speexenc：ffmpeg 解 8k WAV，再交给 speexenc
    Speex {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        dur: f64,
    },
    /// 外部 wavpack CLI：hybrid 有损（ffmpeg 的 wavpack 编码器不支持这个模式）
    Wavpack {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        dur: f64,
        kbps: u32,
    },
    /// 外部 ofr.exe：OptimFROG 无损
    Ofr {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        dur: f64,
        preset: u8,
    },
    /// 随包 mpcenc：Musepack 有损
    Mpc {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        dur: f64,
        profile: u8,
    },
    /// 外部 psxavenc：PS1 XA / PS2 VAG
    Psx {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        dur: f64,
        kind: &'static str,
    },
    /// 外部 qoaconv：QOA 有损
    Qoa {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        dur: f64,
    },
    /// 外部 VGAudioCli：BRSTM / BCSTM / BFSTM / DSP / IDSP
    Vgaudio {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
        dur: f64,
    },
    /// 外部 vadpcm：N64 VADPCM
    Vadpcm {
        idx: usize,
        input: PathBuf,
        out: PathBuf,
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

/// 一个"已加入任务列表"的转换任务：文件 + 加进来那一刻的格式/参数快照。
/// 队列负责选文件，任务列表负责真正要转的东西。
pub(crate) struct Task {
    pub path: PathBuf,
    /// 实际解码用的文件（vgmstream 临时 WAV）；输出命名仍用 path
    pub src: PathBuf,
    pub name: String,
    pub spec_id: &'static str,
    pub opts: fc_core::ConvertOptions,
    pub dur: f64,
    pub state: State,
}

pub(crate) struct App {
    tools: Option<Tools>,
    items: Vec<Item>,
    tasks: Vec<Task>,
    selected: usize,
    target: &'static FormatSpec,
    /// 输出后缀。和目标格式一起切换，默认是 `target.ext`；
    /// AAC 这种能出 .m4a / .aac 两种容器的，可以在输出栏里改。
    ext_sel: &'static str,
    /// 当前打开的场景建议（只影响输出栏的推荐卡片，不进配置）
    scenario: Option<&'static str>,
    /// 音乐库-歌单页当前选中的歌单
    pl_sel: Option<usize>,
    /// 设置页子菜单（默认「关于」）
    settings_tab: overlays::SettingsTab,
    qr_wechat: Option<egui::TextureHandle>,
    qr_alipay: Option<egui::TextureHandle>,
    /// 诊断控制台：本次会话的转换事件，最多保留 2000 行
    pub(crate) console: Vec<String>,
    pub(crate) console_open: bool,
    pub(crate) log_path: PathBuf,
    expanded: Option<&'static str>,
    rx: Option<Receiver<Msg>>,
    running: bool,
    batch_total: usize,
    batch_done: usize,
    frames: u64,
    backdrop_done: bool,
    player: Option<fc_player::Player>,
    /// 正在播放/显示的文件（用路径记，删了/移动了也不会指错歌）
    player_for: Option<PathBuf>,
    /// 连续播放列表（路径）与当前下标；解码线程会自动接下一首
    playlist: Vec<PathBuf>,
    play_idx: usize,
    /// 已经看到播放器切到第几首，用来同步界面
    play_track_seen: u64,
    play_error: Option<String>,
    /// 播放音量（没有播放器时也记住，下次 open 直接套上）
    player_volume: f32,
    /// 是否全屏（F11 切换，不写进配置）
    fullscreen: bool,
    /// 启动过场；`None` 表示已经进主界面
    boot: Option<boot::Boot>,
    /// 命令行带上来的待载入文件，等过场把 tools 备好后才能 probe
    preload: Vec<PathBuf>,
    /// 启动时带了文件：过场结束后直接试听第一首（双击文件打开的场景）
    autoplay: bool,
    /// 文件关联注册的最近一条提示
    assoc_msg: Option<String>,
    /// CUE 分轨线程的返回值
    cue_rx: Option<std::sync::mpsc::Receiver<Result<Vec<PathBuf>, String>>>,
    cue_msg: Option<String>,
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
    /// 「编解码」页第一次进去时枚举的设备
    audio_devices: Vec<fc_player::devices::AudioDevice>,
    devices_loaded: bool,
    devices_error: Option<String>,
    /// 蓝牙：已配对/扫描结果/线程通道
    bt_paired: Vec<fc_player::bluetooth::BtDevice>,
    bt_paired_loaded: bool,
    bt_found: Vec<fc_player::bluetooth::BtDevice>,
    bt_scanning: bool,
    bt_scan_rx: Option<std::sync::mpsc::Receiver<anyhow::Result<Vec<fc_player::bluetooth::BtDevice>>>>,
    bt_action_rx: Option<std::sync::mpsc::Receiver<anyhow::Result<String>>>,
    bt_msg: Option<String>,
    /// 左侧格式分组菜单的展开状态（无损/有损/视频/设备专用）
    group_open: [bool; 7],
    /// 「引擎和设备」子页：设备 / 蓝牙 / 引擎
    codec_tab: codec::CodecTab,
    // 背景与动效
    bg: bg::WallpaperBg,
    /// 当前生效的强调色（背景/底色一变就重算，然后重设一遍 egui style 里沾色的项）
    accent_now: Color32,
    /// 播放栏展开程度 0~1：鼠标移上去展开，移开收成一条细条
    bar_t: f32,
    /// 这一帧鼠标是不是在播放栏里（含展开后的整块）
    bar_hot: bool,
    /// 细条这一帧占的矩形 / 展开块的矩形（下一帧用它判鼠标在不在里面）
    bar_rect: egui::Rect,
    /// CPU / 内存 / 编解码压力的采样器
    sys: sysstat::Sampler,
    /// ASIO 驱动的探测结果（打开过一次就缓存，别每次画界面都开驱动）
    asio_status: Option<(bool, String)>,
    /// ASIO 探测在后台子进程里跑；顺便在顶部显示“正在运行”
    asio_probing: bool,
    asio_probe_rx: Option<std::sync::mpsc::Receiver<(Vec<(String, String)>, Option<String>)>>,
    /// 音频设备枚举也放后台，避免点进引擎页时界面卡住
    devices_loading: bool,
    devices_rx: Option<std::sync::mpsc::Receiver<Result<Vec<fc_player::devices::AudioDevice>, String>>>,
    /// 顶部“正在运行…”提示
    activity: Option<String>,
    bar_expand_rect: egui::Rect,
    dt: f32,
    fmt_sel: Vec<(&'static str, f32)>,
    blurb_t: f32,
    blurb_id: Option<&'static str>,
    detail_t: f32,
    detail_for: usize,
    /// 详情页里点了“分析频谱”，等这一帧画完再起线程
    pending_analyze: Option<usize>,
    /// 上一次点击的行号和时刻，用来自己判定双击
    last_click: (usize, f64),
    /// 窗口是否置顶
    pinned: bool,
    /// 频谱分析结果通道
    analysis_tx: std::sync::mpsc::Sender<(PathBuf, Result<QualityReport, String>, Option<fc_core::loudness::Loudness>, Option<fc_analyze::health::Health>)>,
    analysis_rx: Receiver<(PathBuf, Result<QualityReport, String>, Option<fc_core::loudness::Loudness>, Option<fc_analyze::health::Health>)>,
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
    github_icon: Option<egui::TextureHandle>,
    donate_wechat: Option<egui::TextureHandle>,
    donate_alipay: Option<egui::TextureHandle>,
    /// 当前 ffmpeg 实际带哪些编码器（开机问一次）
    encoders: std::collections::HashSet<String>,
    /// 外部 ATRAC 编码器（atracdenc）。用户自己装，我们不随包分发。
    atracdenc: Option<PathBuf>,
    /// 随包的 Speex 编码器（vendor\speex\speexenc.exe）
    speexenc: Option<PathBuf>,
    /// 外部 WavPack CLI（有损 hybrid 模式必须用它）
    wavpack: Option<PathBuf>,
    /// 外部 OptimFROG CLI（ofr.exe，官方不随包，用户自装）
    ofr: Option<PathBuf>,
    /// 随包的 Musepack 编码器（vendor\musepack\mpcenc.exe，LGPL）
    mpcenc: Option<PathBuf>,
    /// PS1/PS2 编码器 psxavenc（Zlib，可选外部工具）
    psxavenc: Option<PathBuf>,
    /// QOA 参考转换器 qoaconv（MIT）
    qoaconv: Option<PathBuf>,
    /// 任天堂系编码器 VGAudioCli（MIT）
    vgaudio: Option<PathBuf>,
    /// N64 VADPCM 编解码器（MPL-2.0）
    vadpcm: Option<PathBuf>,
    /// ffmpeg 版本串，设置页显示用（启动时问一次就缓存）
    ffmpeg_ver: Option<String>,
    /// 格式百科
    wiki_open: bool,
    wiki_sel: &'static str,
    wiki_filter: String,
    /// 当前页：转换 / 音乐库
    page: Page,
    /// 切页动画进度 0~1（1 = 静止）
    page_t: f32,
    /// 转换后删除源文件。**不进配置**：破坏性操作每次启动都回到关
    delete_source: bool,
    /// 「清除所有元数据」的长按进度
    meta_clear_hold: f32,
    /// 当前缓存的波形：路径 + 每个声道的 (min,max) 桶
    wave: Option<(PathBuf, Vec<Vec<(f32, f32)>>)>,
    /// 正在生成波形的文件（避免重复派发）
    wave_pending: Option<PathBuf>,
    wave_tx: std::sync::mpsc::Sender<(PathBuf, Vec<Vec<(f32, f32)>>)>,
    wave_rx: Receiver<(PathBuf, Vec<Vec<(f32, f32)>>)>,
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
    /// 切换全局输出格式。后缀选择跟着回到新格式的默认值，
    /// 否则从 AAC(.aac) 切到 FLAC 会留下一个 `.aac` 的假后缀。
    pub(crate) fn set_target(&mut self, spec: &'static FormatSpec) {
        if self.target.id != spec.id {
            self.ext_sel = spec.ext;
        }
        self.target = spec;
    }

    /// 这次该用什么后缀：全局目标听用户选的，单文件覆盖用那个格式的默认。
    pub(crate) fn out_ext(&self, spec: &FormatSpec) -> &'static str {
        if spec.id == self.target.id {
            self.ext_sel
        } else {
            spec.ext
        }
    }

    /// 某个外部工具在不在。名字来自 `fc_core::external_encoder`。
    pub(crate) fn external_ok(&self, name: &str) -> bool {
        match name {
            "wavpack" => self.wavpack.is_some(),
            "ofr" => self.ofr.is_some(),
            "mpcenc" => self.mpcenc.is_some(),
            "psxavenc" => self.psxavenc.is_some(),
            "qoaconv" => self.qoaconv.is_some(),
            "vgaudio" => self.vgaudio.is_some(),
            "vadpcm" => self.vadpcm.is_some(),
            // atracdenc 是默认那个（external::NAME）
            _ => self.atracdenc.is_some(),
        }
    }

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
        style.visuals.selection.bg_fill = tint(accent(), 90);
        style.visuals.selection.stroke = Stroke::new(1.0, TEXT);
        for w in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            w.rounding = Rounding::same(8.0);
        }
        style.visuals.widgets.inactive.bg_stroke = line(Color32::WHITE, 26);
        style.visuals.widgets.hovered.bg_stroke = line(accent(), 90);
        style.visuals.widgets.active.bg_stroke = line(accent(), 130);
        // 滚动条：平时完全隐藏（不挡内容），鼠标进到那一栏才浮出来
        style.spacing.scroll = egui::style::ScrollStyle::floating();
        style.spacing.scroll.foreground_color = false;
        style.spacing.scroll.bar_width = 5.0;
        style.spacing.scroll.floating_width = 2.5;
        style.spacing.scroll.dormant_handle_opacity = 0.0;
        style.spacing.scroll.active_handle_opacity = 0.35;
        style.spacing.scroll.interact_handle_opacity = 0.8;
        style.spacing.scroll.dormant_background_opacity = 0.0;
        style.spacing.scroll.active_background_opacity = 0.0;
        style.spacing.scroll.interact_background_opacity = 0.0;
        style.visuals.widgets.inactive.bg_fill = Color32::from_gray(70);
        style.visuals.widgets.hovered.bg_fill = Color32::from_gray(100);
        style.visuals.widgets.active.bg_fill = Color32::from_gray(130);
        cc.egui_ctx.set_style(style);

        let target = fc_core::default_target();
        let (atx, arx) = channel();
        let (wtx, wrx) = channel();
        let (mtx, mrx) = channel();
        let (btx, brx) = channel();
        let (ltx, lrx) = channel();
        let (cvt, cvr) = channel();
        let (spt, spr) = channel();
        clean_stale_temp();
        let boot = boot::Boot::start();
        let cfg = settings::Settings::load();
        if std::env::var_os("TONELORE_LANG").is_none() {
            i18n::set_lang(&cfg.lang);
        }
        fc_analyze::set_en(i18n::is_en());
        fc_core::format::set_en(i18n::is_en());
        fc_core::wiki::set_en(i18n::is_en());
        // 界面缩放在建窗口时就要生效，否则第一帧会闪一下原尺寸
        cc.egui_ctx.set_zoom_factor(zoom());
        // 注意：窗口尺寸这里不能再动。
        // 建窗口时是过场的小尺寸（boot::SPLASH_W/H），进主界面那一刻直接铺满工作区；
        // 之前这里有一句"按配置尺寸对齐"，会把过场窗口立刻撑大，小窗口就白做了。
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
            crate::tr!("还没有扫描").to_string()
        } else {
            format!("上次扫描：{} 首", cached_lib.len())
        };
        App {
            tools: None,
            items: Vec::new(),
            tasks: Vec::new(),
            selected: 0,
            target,
            ext_sel: target.ext,
            scenario: None,
            pl_sel: None,
            settings_tab: overlays::SettingsTab::About,
            qr_wechat: None,
            qr_alipay: None,
            console: Vec::new(),
            console_open: false,
            log_path: { 
                let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
                let p = std::env::var_os("APPDATA")
                    .map(|a| PathBuf::from(a).join("ToneLore").join("logs").join(format!("session-{ms}.log")))
                    .unwrap_or_default();
                if let Some(dir) = p.parent() { let _ = std::fs::create_dir_all(dir); }
                p
            },
            expanded: Some(target.id),
            rx: None,
            running: false,
            batch_total: 0,
            batch_done: 0,
            frames: 0,
            backdrop_done: false,
            player: None,
            player_for: None,
            playlist: Vec::new(),
            play_idx: 0,
            play_track_seen: 0,
            play_error: None,
            player_volume: 0.75,
            fullscreen: false,
            boot: Some(boot),
            preload: Vec::new(),
            autoplay: false,
            assoc_msg: None,
            cue_rx: None,
            cue_msg: None,
            cover_view: None,
            cover_t: 0.0,
            cover_closing: false,
            cover_big: None,
            cover_big_tx: btx,
            cover_big_rx: brx,
            audio_devices: Vec::new(),
            devices_loaded: false,
            devices_error: None,
            bt_paired: Vec::new(),
            bt_paired_loaded: false,
            bt_found: Vec::new(),
            bt_scanning: false,
            bt_scan_rx: None,
            bt_action_rx: None,
            bt_msg: None,
            // 常用两组和视频默认展开；设备专用（ATRAC）默认收着
            group_open: [true, true, false, false, false, false, false],
            codec_tab: codec::CodecTab::Devices,
            bg: bg::WallpaperBg::new(&cc.egui_ctx),
            accent_now: bg::DEFAULT_ACCENT,
            bar_t: 0.0,
            bar_hot: false,
            bar_rect: egui::Rect::NOTHING,
            sys: sysstat::Sampler::new(),
            asio_status: None,
            asio_probing: false,
            asio_probe_rx: None,
            devices_loading: false,
            devices_rx: None,
            activity: None,
            bar_expand_rect: egui::Rect::NOTHING,
            dt: 0.016,
            fmt_sel: fc_core::browsable().map(|f| (f.id, 0.0)).collect(),
            blurb_t: 1.0,
            blurb_id: Some(target.id),
            detail_t: 1.0,
            detail_for: 0,
            pending_analyze: None,
            last_click: (usize::MAX, -10.0),
            pinned: false,
            analysis_tx: atx,
            analysis_rx: arx,
            media_tx: mtx,
            media_rx: mrx,
            cfg,
            settings_open: std::env::var("TONELORE_OPEN_SETTINGS").is_ok(),
            settings_t: 0.0,
            meta_open: true,
            notices: Vec::new(),
            notices_open: false,
            bell_rect: egui::Rect::NOTHING,
            delete_hold: 0.0,
            github_icon: load_qr(&cc.egui_ctx, "github", GITHUB_ICON),
            donate_wechat: load_qr(&cc.egui_ctx, "donate-wechat", DONATE_WECHAT),
            donate_alipay: load_qr(&cc.egui_ctx, "donate-alipay", DONATE_ALIPAY),
            encoders: std::collections::HashSet::new(),
            atracdenc: fc_core::find_atracdenc(),
            speexenc: fc_core::find_speexenc(),
            wavpack: fc_core::find_wavpack(),
            ofr: fc_core::find_ofr(),
            mpcenc: fc_core::find_mpcenc(),
            psxavenc: fc_core::find_psxavenc(),
            qoaconv: fc_core::find_qoaconv(),
            vgaudio: fc_core::find_vgaudio(),
            vadpcm: fc_core::find_vadpcm(),
            ffmpeg_ver: None,
            wiki_open: false,
            wiki_sel: "flac",
            wiki_filter: String::new(),
            page: Page::Convert,
            page_t: 1.0,
            delete_source: false,
            meta_clear_hold: 0.0,
            wave: None,
            wave_pending: None,
            wave_tx: wtx,
            wave_rx: wrx,
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

    /// 背景色/壁纸一变，强调色就跟着换。
    ///
    /// 90 多个控件在绘制时读 `accent()`，所以这里只要在画界面之前把全局值换掉；
    /// 剩下几项（输入框选区、hover/focus 描边）是 egui style 里的，要单独重设。
    fn refresh_accent(&mut self, ctx: &egui::Context) {
        let want = bg::accent_for(
            self.cfg.bg_mode,
            self.cfg.bg_gradient,
            self.cfg.bg_color,
            self.bg.average(),
        );
        if want == self.accent_now {
            return;
        }
        self.accent_now = want;
        set_accent(want);
        let mut style = (*ctx.style()).clone();
        style.visuals.selection.bg_fill = tint(accent(), 90);
        style.visuals.widgets.hovered.bg_stroke = line(accent(), 90);
        style.visuals.widgets.active.bg_stroke = line(accent(), 130);
        ctx.set_style(style);
    }

    /// 播放栏 + 底边距实际占掉的高度：队列/音乐库的滚动区要留这么多，
    /// 内容才不会钻到悬浮播放栏下面（栏高可拖，所以不能再用常量）。
    /// 播放栏现在是贴底的一条独立面板：内容区直接铺到底，不用再为它留空
    pub(crate) fn bar_reserve(&self) -> f32 {
        0.0
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let Some(tools) = self.tools.clone() else { return };
        for p in paths {
            if self.items.iter().any(|i| i.path == p) {
                continue;
            }
            let mut encrypted = None;
            // vgmstream 能读、ffmpeg 读不了的游戏音频：先解成临时 WAV 再 probe
            let mut media: Option<PathBuf> = None;
            let mut probe_path = p.clone();
            if fc_core::is_vgmstream_ext(&p) {
                if let Some(exe) = fc_core::find_vgmstream() {
                    let tmp = vgm_temp_path();
                    match fc_core::vgmstream_decode(&exe, &p, &tmp) {
                        Ok(()) => {
                            probe_path = tmp.clone();
                            media = Some(tmp);
                        }
                        Err(e) => crate::err2!("vgmstream 解码失败：{e:#}", "vgmstream decode failed: {e:#}"),
                    }
                }
            } else if fc_core::is_module_ext(&p) {
                if let Some(exe) = fc_core::find_openmpt123() {
                    let tmp = vgm_temp_path();
                    match fc_core::openmpt_decode(&exe, &p, &tmp) {
                        Ok(()) => {
                            probe_path = tmp.clone();
                            media = Some(tmp);
                        }
                        Err(e) => crate::err2!("openmpt123 渲染失败：{e:#}", "openmpt123 render failed: {e:#}"),
                    }
                }
            }
            let (info, error) = match fc_core::probe(&tools, &probe_path) {
                Ok(i) => (Some(i), None),
                Err(e) => {
                    // 读不出来时先问一句"是不是加密的"，能答就答得比报错清楚
                    match fc_core::identify(&p) {
                        Some(enc) => {
                            let msg = format!("{}\n\n{}", enc.summary(), enc.full_text());
                            encrypted = Some(enc);
                            (None, Some(msg))
                        }
                        None => (None, Some(format!("{e:#}"))),
                    }
                }
            };
            let idx = self.items.len();
            spawn_media_load(&tools, media.as_deref().unwrap_or(&p), idx, self.media_tx.clone());
            self.items.push(Item {
                path: p,
                media,
                info,
                error,
                state: State::Pending,
                override_id: None,
                hover_t: 0.0,
                sel_t: 0.0,
                hovered: false,
                progress_t: 0.0,
                analysis: Analysis::Idle,
                lufs: None,
                encrypted,
                health: None,
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

    /// 把当前选中的文件按当前格式/参数加进任务列表
    pub(crate) fn add_task(&mut self) {
        let Some(it) = self.items.get(self.selected) else {
            return;
        };
        if it.info.is_none() {
            return;
        }
        let spec = it.spec(self.target);
        let name = it.tags.display_title(&it.name());
        let path = it.path.clone();
        let src = it.media_path().to_path_buf();
        let dur = it.info.as_ref().map(|m| m.duration_s).unwrap_or(0.0);
        if self.tasks.iter().any(|t| t.path == path && t.spec_id == spec.id) {
            return;
        }
        self.tasks.push(Task {
            path,
            src,
            name,
            spec_id: spec.id,
            opts: self.cfg.opts.clone(),
            dur,
            state: State::Pending,
        });
    }

    fn start_batch(&mut self, ctx: &egui::Context) {
        let Some(tools) = self.tools.clone() else { return };
        let atracdenc = self.atracdenc.clone();
        let speexenc = self.speexenc.clone();
        let wavpack = self.wavpack.clone();
        let ofr = self.ofr.clone();
        let mpcenc = self.mpcenc.clone();
        let psxavenc = self.psxavenc.clone();
        let qoaconv = self.qoaconv.clone();
        let vgaudio = self.vgaudio.clone();
        let vadpcm = self.vadpcm.clone();
        let atrac3_kbps = self.cfg.opts.atrac3_bitrate;
        let mut jobs: Vec<Job> = Vec::new();
        let mut used_out: Vec<PathBuf> = Vec::new();
        for i in 0..self.tasks.len() {
            if !matches!(self.tasks[i].state, State::Pending) {
                continue;
            }
            let spec = match fc_core::by_id(self.tasks[i].spec_id) {
                Some(s) => s,
                None => continue,
            };
            // 同一个文件加了两条不同格式的任务时，输出名要错开
            // 后缀听用户在输出栏里的选择（多后缀格式才有得选）
            let ext = self.out_ext(spec);
            let Some(mut out) = output_path(&self.tasks[i].path, ext, &self.cfg) else {
                self.tasks[i].state = State::Failed(crate::tr2!("输出已存在（设置为跳过）", "Output exists (set to skip)").to_string());
                continue;
            };
            let mut n = 1;
            while used_out.contains(&out) {
                n += 1;
                let stem = out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let base_stem = stem.trim_end_matches(|c: char| c == ')' || c.is_ascii_digit() || c == ' ').to_string();
                out.set_file_name(format!("{base_stem} ({n}).{ext}"));
            }
            used_out.push(out.clone());
            let dur = self.tasks[i].dur;

            // ATRAC 走外部编码器：ffmpeg 只负责解码，标签/封面写不进去，直接分派
            if let Some(codec) = fc_core::external::codec_for(spec.id, atrac3_kbps) {
                jobs.push(Job::Atrac {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    codec,
                    dur,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // WavPack 有损走官方 CLI：ffmpeg 的 wavpack 编码器没有 hybrid 模式
            if spec.id == "wavpack_lossy" {
                if self.wavpack.is_none() {
                    self.tasks[i].state = State::Failed(
                        crate::tr!("找不到 wavpack.exe，WavPack 有损输出不可用（设置页有说明）").to_string(),
                    );
                    continue;
                }
                jobs.push(Job::Wavpack {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                    kbps: self.tasks[i].opts.wavpack_bitrate,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // OptimFROG 走官方 ofr.exe（不随包：官方许可要求分发 CLI 需授权）
            if spec.id == "optimfrog" {
                if self.ofr.is_none() {
                    self.tasks[i].state = State::Failed(
                        crate::tr!("找不到 ofr.exe，OptimFROG 输出不可用（losslessaudio.org 下载后放到程序目录）")
                            .to_string(),
                    );
                    continue;
                }
                jobs.push(Job::Ofr {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                    preset: self.tasks[i].opts.ofr_preset,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // Musepack 走随包 mpcenc（LGPL）
            if spec.id == "musepack" {
                if self.mpcenc.is_none() {
                    self.tasks[i].state =
                        State::Failed(crate::tr!("找不到 mpcenc.exe，Musepack 输出不可用").to_string());
                    continue;
                }
                jobs.push(Job::Mpc {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                    profile: self.tasks[i].opts.mpc_profile,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // PS1 XA / PS2 VAG 走外部 psxavenc（Zlib 许可，可选安装）
            if spec.id == "xa" || spec.id == "vag" {
                if self.psxavenc.is_none() {
                    self.tasks[i].state = State::Failed(
                        crate::tr!("找不到 psxavenc.exe，PS1 XA / PS2 VAG 输出不可用（extras 包里有）")
                            .to_string(),
                    );
                    continue;
                }
                jobs.push(Job::Psx {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                    kind: if spec.id == "xa" { "xa" } else { "vag" },
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // QOA 走参考实现 qoaconv（MIT，~50 KB）
            if spec.id == "qoa" {
                if self.qoaconv.is_none() {
                    self.tasks[i].state =
                        State::Failed(crate::tr!("找不到 qoaconv.exe，QOA 输出不可用（extras 包里有）").to_string());
                    continue;
                }
                jobs.push(Job::Qoa {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // 任天堂系（BRSTM/BCSTM/BFSTM/DSP/IDSP）走 VGAudioCli（MIT）
            if matches!(spec.id, "brstm" | "bcstm" | "bfstm" | "dsp" | "idsp") {
                if self.vgaudio.is_none() {
                    self.tasks[i].state = State::Failed(
                        crate::tr!("找不到 VGAudioCli.exe，任天堂容器输出不可用（extras 包里有）").to_string(),
                    );
                    continue;
                }
                jobs.push(Job::Vgaudio {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // N64 VADPCM 走外部 vadpcm（MPL-2.0）
            if spec.id == "vadpcm" {
                if self.vadpcm.is_none() {
                    self.tasks[i].state = State::Failed(
                        crate::tr!("找不到 vadpcm.exe，N64 VADPCM 输出不可用（extras 包里有）").to_string(),
                    );
                    continue;
                }
                jobs.push(Job::Vadpcm {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // Speex 走随包的 speexenc：ffmpeg 只负责解成 8k WAV
            if spec.id == "speex" {
                if self.speexenc.is_none() {
                    self.tasks[i].state =
                        State::Failed(crate::tr!("找不到 speexenc，Speex 输出不可用").to_string());
                    continue;
                }
                jobs.push(Job::Speex {
                    idx: i,
                    input: self.tasks[i].src.clone(),
                    out,
                    dur,
                });
                self.tasks[i].state = State::Running(0.0);
                continue;
            }

            // 参数由 core 统一生成，GUI 不拼 ffmpeg 命令行。
            // 视频容器要看见源流的编码（能不能 copy / 音轨要不要重编），所以把 info 也传进去。
            let src_item = self.items.iter().find(|x| x.path == self.tasks[i].path);
            let src_info = src_item.and_then(|x| x.info.as_ref());
            if let Some(m) = src_info {
                if m.needs_external_decoder()
                    && !fc_core::ffmpeg::has_decoder(&tools, &m.codec)
                    && !(spec.id == "ac4" && m.codec == "ac4")
                {
                    self.tasks[i].state = State::Failed(format!(
                        "{} 是空间音频，内置 ffmpeg 没有解码器，不能转码；请保留原文件。",
                        m.codec_label()
                    ));
                    continue;
                }
            }
            if spec.id == "ac4" && src_info.map(|m| m.codec.as_str() != "ac4").unwrap_or(true) {
                self.tasks[i].state = State::Failed(
                    crate::tr!("AC-4 只能从 AC-4 源直通封装；普通音频没有 AC-4 编码器。").to_string(),
                );
                continue;
            }
            // IEC 61937 是 S/PDIF 上的压缩音频直通：源不是压缩比特流就没东西可装
            if spec.id == "iec61937"
                && src_info
                    .map(|m| {
                        !matches!(
                            m.codec.as_str(),
                            "ac3" | "eac3" | "dts" | "mp2" | "mp3" | "truehd" | "mlp"
                        )
                    })
                    .unwrap_or(true)
            {
                self.tasks[i].state = State::Failed(
                    crate::tr!("IEC 61937 只能直通压缩音频（AC-3 / E-AC-3 / DTS / MPEG），PCM 源没有可直通的比特流。")
                        .to_string(),
                );
                continue;
            }
            let mut args = fc_core::args_for_with(spec, &self.tasks[i].opts, src_info);

            // 芯片音乐没有固定时长：libgme 会一直渲染，默认只取前 3 分钟
            if fc_core::is_chiptune(&self.tasks[i].path) {
                args.push("-t".to_string());
                args.push(fc_core::CHIPTUNE_SECONDS.to_string());
            }

            // 元数据：源标签 → 单文件覆盖 → 批量覆盖，后写的赢
            if self.cfg.write_tags {
                let mut tags = src_item.map(|x| x.tags.clone()).unwrap_or_default();
                if let Some(v) = src_item
                    .and_then(|x| x.title_ov.as_ref())
                    .filter(|v| !v.trim().is_empty())
                {
                    tags.title = Some(v.trim().to_string());
                }
                if let Some(v) = src_item
                    .and_then(|x| x.track_ov.as_ref())
                    .filter(|v| !v.trim().is_empty())
                {
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
                input: self.tasks[i].src.clone(),
                out,
                args,
                dur,
                norm: self.cfg.loudness,
            });
            self.tasks[i].state = State::Running(0.0);
        }
        if jobs.is_empty() {
            return;
        }
        self.running = true;
        self.batch_total = jobs.len();
        self.log(format!("[START] {} job(s)", jobs.len()));
        self.batch_done = 0;

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let lyric_mode = self.cfg.lyric_mode;
        let ctx2 = ctx.clone();

        std::thread::spawn(move || {
            for job in jobs {
                let (idx, out) = match &job {
                    Job::Ffmpeg { idx, out, .. } => (*idx, out.clone()),
                    Job::Atrac { idx, out, .. } => (*idx, out.clone()),
                    Job::Speex { idx, out, .. } => (*idx, out.clone()),
                    Job::Wavpack { idx, out, .. } => (*idx, out.clone()),
                    Job::Ofr { idx, out, .. } => (*idx, out.clone()),
                    Job::Mpc { idx, out, .. } => (*idx, out.clone()),
                    Job::Psx { idx, out, .. } => (*idx, out.clone()),
                    Job::Qoa { idx, out, .. } => (*idx, out.clone()),
                    Job::Vgaudio { idx, out, .. } => (*idx, out.clone()),
                    Job::Vadpcm { idx, out, .. } => (*idx, out.clone()),
                };
                // 所有任务都带源时长，用于转换后的自检
                let job_dur = match &job {
                    Job::Ffmpeg { dur, .. }
                    | Job::Atrac { dur, .. }
                    | Job::Speex { dur, .. }
                    | Job::Wavpack { dur, .. }
                    | Job::Ofr { dur, .. }
                    | Job::Mpc { dur, .. }
                    | Job::Psx { dur, .. }
                    | Job::Qoa { dur, .. }
                    | Job::Vgaudio { dur, .. }
                    | Job::Vadpcm { dur, .. } => *dur,
                };
                let res = match job {
                    Job::Ffmpeg {
                        idx,
                        input,
                        out,
                        mut args,
                        dur,
                        norm,
                    } => {
                        // 响度归一化：先量一遍，再用 volume 增益（峰值留 1 dB 余量）
                        if let Some(target) = norm {
                            if let Ok(m) = fc_analyze::loudness(&tools.ffmpeg, &input) {
                                let mut gain = target - m.lufs;
                                let ceiling = -1.0 - (m.peak_dbfs + gain);
                                if ceiling < 0.0 {
                                    gain += ceiling;
                                }
                                let gain = gain.clamp(-30.0, 30.0);
                                if gain.abs() >= 0.05 {
                                    args.push("-af".into());
                                    args.push(format!("volume={gain:.2}dB"));
                                }
                            }
                        }
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
                                crate::tr!("找不到 atracdenc。ATRAC 输出需要你自己装它（设置页有说明）。")
                            )),
                        }
                    }
                    Job::Speex {
                        idx,
                        input,
                        out,
                        dur,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match speexenc.as_ref() {
                            Some(exe) => fc_core::external::speex_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 speexenc"))),
                        }
                    }
                    Job::Psx {
                        idx,
                        input,
                        out,
                        dur,
                        kind,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match psxavenc.as_ref() {
                            Some(exe) => fc_core::external::psx_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                kind,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 psxavenc.exe"))),
                        }
                    }
                    Job::Qoa {
                        idx,
                        input,
                        out,
                        dur,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match qoaconv.as_ref() {
                            Some(exe) => fc_core::external::qoa_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 qoaconv.exe"))),
                        }
                    }
                    Job::Vgaudio {
                        idx,
                        input,
                        out,
                        dur,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match vgaudio.as_ref() {
                            Some(exe) => fc_core::external::vgaudio_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 VGAudioCli.exe"))),
                        }
                    }
                    Job::Vadpcm {
                        idx,
                        input,
                        out,
                        dur,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match vadpcm.as_ref() {
                            Some(exe) => fc_core::external::vadpcm_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 vadpcm.exe"))),
                        }
                    }
                    Job::Mpc {
                        idx,
                        input,
                        out,
                        dur,
                        profile,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match mpcenc.as_ref() {
                            Some(exe) => fc_core::external::mpc_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                profile,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 mpcenc.exe"))),
                        }
                    }
                    Job::Ofr {
                        idx,
                        input,
                        out,
                        dur,
                        preset,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match ofr.as_ref() {
                            Some(exe) => fc_core::external::ofr_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                preset,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 ofr.exe"))),
                        }
                    }
                    Job::Wavpack {
                        idx,
                        input,
                        out,
                        dur,
                        kbps,
                    } => {
                        let tx_p = tx.clone();
                        let ctx_p = ctx2.clone();
                        match wavpack.as_ref() {
                            Some(exe) => fc_core::external::wavpack_convert(
                                &tools,
                                exe,
                                &input,
                                &out,
                                kbps,
                                dur,
                                Box::new(move |p| {
                                    let _ = tx_p.send(Msg::Progress(idx, p));
                                    ctx_p.request_repaint();
                                }),
                            ),
                            None => Err(anyhow::anyhow!(crate::tr!("找不到 wavpack.exe"))),
                        }
                    }
                };
                // 转换后自检：回读输出，坏文件/时长离谱直接报出来，不假装成功
                let res = res.and_then(|size| {
                    match fc_core::verify_output(&tools, &out, job_dur) {
                        Ok(()) => Ok(size),
                        Err(why) => Err(anyhow::anyhow!(
                            "{}: {why}",
                            crate::i18n::tr("输出已生成，但自检没通过")
                        )),
                    }
                });
                match res {
                    Ok(size) => {
                        // 空歌词 sidecar：同名 .lyc / .txt，已有就不覆盖
                        let lyric_ext = match lyric_mode {
                            crate::settings::LyricMode::Lyc => Some("lyc"),
                            crate::settings::LyricMode::Txt => Some("txt"),
                            crate::settings::LyricMode::None => None,
                        };
                        if let Some(ext) = lyric_ext {
                            let lyric = out.with_extension(ext);
                            if !lyric.exists() {
                                let _ = std::fs::write(&lyric, b"");
                            }
                        }
                        let _ = tx.send(Msg::Done(idx, out, size));
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        let _ = tx.send(Msg::Failed(idx, format!("[{}] {msg}", classify_error(&msg))));
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
        let media = it.media_path().to_path_buf();
        let key = it.path.clone();
        it.analysis = Analysis::Running;

        let tx = self.analysis_tx.clone();
        let ctx = ctx.clone();
        let ffmpeg = self.tools.as_ref().map(|t| t.ffmpeg.clone());
        let tools = self.tools.clone();
        std::thread::spawn(move || {
            let res = fc_analyze::analyze(&media, &info).map_err(|e| format!("{e:#}"));
            // 响度要解整轨，跟着这次分析一起做，别让用户再等一次
            let loud = ffmpeg
                .as_ref()
                .and_then(|f| fc_analyze::loudness(f, &media).ok());
            // 内容体检：静音/削波/噪声/测试信号，一次整轨解码搞定
            let health = tools
                .as_ref()
                .and_then(|t| fc_analyze::health::analyze(t, &media).ok());
            let _ = tx.send((key, res, loud, health));
            ctx.request_repaint();
        });
    }

    /// 后台算波形（8 kHz 立体声，一首歌几百毫秒）
    fn request_wave(&mut self, path: &Path, ctx: &egui::Context) {
        if self.wave.as_ref().map(|(p, _)| p.as_path()) == Some(path)
            || self.wave_pending.as_deref() == Some(path)
        {
            return;
        }
        // 波形键仍用原文件路径（界面按队列项匹配），解码用 media（vgmstream 临时 WAV）
        let media = self
            .items
            .iter()
            .find(|i| i.path == path)
            .map(|i| i.media_path().to_path_buf())
            .unwrap_or_else(|| path.to_path_buf());
        let Some(tools) = self.tools.clone() else { return };
        let key = path.to_path_buf();
        let tx = self.wave_tx.clone();
        self.wave_pending = Some(key.clone());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Ok(w) = fc_analyze::waveform(&tools.ffmpeg, &media, 480) {
                let _ = tx.send((key, w));
                ctx.request_repaint();
            }
        });
    }

    fn start_play(&mut self, idx: usize) {
        // 连续播放列表：当前队列里所有能播的音频（视频取第一条音轨）
        let list: Vec<PathBuf> = self
            .items
            .iter()
            .filter(|i| i.info.is_some() && i.encrypted.is_none())
            .map(|i| i.path.clone())
            .collect();
        if let Some(pos) = list.iter().position(|p| p == &self.items[idx].path) {
            self.playlist = list;
            self.play_idx = pos;
        }
        // 加密文件只识别：不播放、不解码
        if let Some(it) = self.items.get(idx) {
            if it.encrypted.is_some() {
                self.player = None;
                self.player_for = Some(self.items[idx].path.clone());
                self.play_error = Some(crate::tr!("加密文件：本工具只识别，不解码也不播放。").into());
                return;
            }
        }
        if let Some(p) = &self.player {
            p.fade_out(12);
        }
        self.player = None;
        self.play_error = None;
        let Some(tools) = self.tools.clone() else {
            self.play_error = Some(crate::tr!("找不到 ffmpeg，无法试听").into());
            self.player_for = Some(self.items[idx].path.clone());
            return;
        };
        let Some(it) = self.items.get(idx) else { return };
        let Some(info) = it.info.clone() else {
            self.play_error = Some(crate::tr!("这个文件解不出音频，没法试听").into());
            self.player_for = Some(self.items[idx].path.clone());
            return;
        };
        let has_external_decoder = self
            .tools
            .as_ref()
            .map(|t| fc_core::ffmpeg::has_decoder(t, &info.codec))
            .unwrap_or(false);
        if info.needs_external_decoder() && !has_external_decoder {
            self.player = None;
            self.player_for = Some(it.path.clone());
            self.play_error = Some(format!(
                "{}：内置 ffmpeg 没有这个空间音频解码器，只能保留原文件，不能转成普通声道。",
                info.codec_label()
            ));
            return;
        }
        // 输出设备 / 采样率跟随 / ReplayGain 都从设置里带过去
        let dsd = info.codec.starts_with("dsd");
        let tier = fc_core::dsd_tier(&info.codec, info.sample_rate, info.bitrate_kbps);
        let dop_rate = fc_player::dop::dop_rate_for_tier(tier);
        let mut opts = fc_player::PlayOptions {
            device_name: if self.cfg.audio_out_device.is_empty() {
                None
            } else {
                Some(self.cfg.audio_out_device.clone())
            },
            source_rate: dop_rate.unwrap_or(info.sample_rate),
            replaygain_db: if self.cfg.replaygain {
                it.tags.replaygain_db
            } else {
                None
            },
            engine: if dsd && dop_rate.is_some() {
                fc_player::Engine::Exclusive
            } else {
                self.cfg.engine_enum()
            },
            dop: dsd && dop_rate.is_some(),
            dop_lsb_first: info.codec.contains("lsbf"),
        };
        let mut opening =
            fc_player::Player::open_with(&tools.ffmpeg, it.media_path(), info.duration_s, &opts);
        // DoP 失败（设备没 176.4k / 独占被占）：明确回退 PCM，不假装在播 DSD
        if opening.is_err() && opts.dop {
            let why = opening.err().map(|e| format!("{e:#}")).unwrap_or_default();
            opts.dop = false;
            opts.source_rate = 0;
            opts.engine = self.cfg.engine_enum();
            self.play_error = Some(format!("DSD DoP 不可用（{why}）· 已回退 PCM"));
            opening =
                fc_player::Player::open_with(&tools.ffmpeg, it.media_path(), info.duration_s, &opts);
        }
        match opening {
            Ok(p) => {
                // 接着上次的音量，不要每次开新文件都跳回默认
                p.set_volume(self.player_volume);
                self.play_track_seen = p.track_no();
                self.player = Some(p);
                self.player_for = Some(self.items[idx].path.clone());
                self.arm_next();
            }
            Err(e) => {
                self.play_error = Some(format!("{e:#}"));
                self.player_for = Some(self.items[idx].path.clone());
            }
        }
    }

    /// 把列表里的下一首预排给解码线程：当前首 EOF 时无缝接上
    fn arm_next(&mut self) {
        let Some(next_path) = self.playlist.get(self.play_idx + 1).cloned() else {
            return;
        };
        let Some(p) = self.player.as_ref() else { return };
        let Some(next_item) = self.items.iter().find(|i| i.path == next_path) else {
            return;
        };
        let (dur, gain) = (
            next_item.info.as_ref().map(|m| m.duration_s).unwrap_or(0.0),
            if self.cfg.replaygain {
                next_item.tags.replaygain_db
            } else {
                None
            },
        );
        p.set_next(next_item.media_path(), dur, gain);
    }

    fn poll_media(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.media_rx.try_recv() {
            if let Some(it) = self.items.iter_mut().find(|it| it.path == m.path) {
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
                        format!("cover_{}", m.path.display()),
                        img,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
        }
    }

    fn poll_analysis(&mut self) {
        while let Ok((path, res, loud, health)) = self.analysis_rx.try_recv() {
            if let Some(it) = self.items.iter_mut().find(|it| it.path == path) {
                it.analysis = match res {
                    Ok(r) => Analysis::Done(r),
                    Err(e) => Analysis::Failed(e),
                };
                if loud.is_some() {
                    it.lufs = loud;
                }
                it.health = health;
            }
        }
    }

    fn poll(&mut self) {
        while let Ok((p, w)) = self.wave_rx.try_recv() {
            self.wave = Some((p, w));
            self.wave_pending = None;
        }
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            match m {
                Msg::Progress(i, p) => {
                    if let Some(t) = self.tasks.get_mut(i) {
                        t.state = State::Running(p);
                    }
                }
                Msg::Done(i, path, size) => {
                    self.log(format!("[OK] {}", path.display()));
                    // 转换成功且用户选了「删除源文件」——在标记成功之后删，
                    // 删失败也不影响结果（比如源文件正被别的程序占用）
                    let src = self.tasks.get(i).map(|t| t.path.clone());
                    let name = self
                        .tasks
                        .get(i)
                        .map(|t| t.name.clone())
                        .unwrap_or_default();
                    let out_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if let Some(t) = self.tasks.get_mut(i) {
                        t.state = State::Done(path.clone(), size);
                        let p = t.path.clone();
                        if let Some(it) = self.items.iter_mut().find(|x| x.path == p) {
                            if matches!(it.state, State::Pending) {
                                it.state = State::Done(path, size);
                            }
                        }
                    }
                    if self.delete_source {
                        if let Some(src) = src {
                            // 删不掉就算了（源文件可能正被占用），不打断整批
                            let _ = std::fs::remove_file(&src);
                        }
                    }
                    self.batch_done += 1;
                    self.push_notice(
                        true,
                        crate::tr!("转换完成"),
                        format!("{name} → {out_name} · {}", fmt_size(size)),
                    );
                }
                Msg::Failed(i, e) => {
                    self.log(format!("[FAIL] {e}"));
                    let name = self
                        .tasks
                        .get(i)
                        .map(|t| t.name.clone())
                        .unwrap_or_default();
                    if let Some(t) = self.tasks.get_mut(i) {
                        t.state = State::Failed(e.clone());
                        let p = t.path.clone();
                        if let Some(it) = self.items.iter_mut().find(|x| x.path == p) {
                            if matches!(it.state, State::Pending) {
                                it.state = State::Failed(e.clone());
                            }
                        }
                    }
                    self.batch_done += 1;
                    self.push_notice(false, crate::tr!("转换失败"), format!("{name}\n{e}"));
                }
                Msg::Finished => {
                    self.log("batch finished");
                    self.running = false;
                    self.rx = None;
                    // 一批多个文件时再补一条汇总，单文件就别重复了
                    if self.batch_total > 1 {
                        let fail = self
                            .tasks
                            .iter()
                            .filter(|t| matches!(t.state, State::Failed(_)))
                            .count();
                        let ok = self.batch_total.saturating_sub(fail);
                        self.push_notice(
                            fail == 0,
                            crate::tr!("本批转换结束"),
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


    /// 标题栏圆环要显示什么：进度 / 打勾 / 感叹号 / 空闲
    pub(crate) fn batch_ring_state(&self) -> (f32, BatchRing) {
        if self.running {
            return (self.overall(), BatchRing::Running);
        }
        if self.tasks.iter().any(|t| matches!(t.state, State::Failed(_))) {
            return (0.0, BatchRing::Error);
        }
        if !self.tasks.is_empty() && self.tasks.iter().all(|t| matches!(t.state, State::Done(..))) {
            return (1.0, BatchRing::Done);
        }
        (0.0, BatchRing::Idle)
    }

    pub(crate) fn overall(&self) -> f32 {
        if self.batch_total == 0 {
            return 0.0;
        }
        let cur = self
            .tasks
            .iter()
            .filter_map(|t| match &t.state {
                State::Running(p) => Some(*p as f64),
                _ => None,
            })
            .sum::<f64>() as f32;
        ((self.batch_done as f32 + cur) / self.batch_total as f32).clamp(0.0, 1.0)
    }
}

/// 全屏开关：铺满显示器工作区，退出时还原原来的窗口矩形。
///
/// 不发给 winit `ViewportCommand::Fullscreen`：那会切显示模式 / 色彩空间，
/// Win11 + HDR 下退出后整个桌面会变暗（关一次 HDR 才恢复）。
/// 我们本来就是无边框自绘标题栏，铺满工作区看起来就是全屏，而且不碰 HDR。
pub(crate) fn set_fullscreen(full: &mut bool, ctx: &egui::Context, size_px: (f32, f32)) {
    *full = !*full;
    #[cfg(windows)]
    {
        if frame::set_fill_work_area(*full) {
            // 窗口大小是原生改的，让 egui 下一帧重新对齐一次布局
            ctx.request_repaint();
            return;
        }
    }
    // 兜底（非 Windows，或窗口句柄还没拿到）：退回 eframe 的真全屏
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(*full));
    if !*full {
        let (w, h) = size_px;
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            w / zoom(),
            h / zoom(),
        )));
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
        // 过场窗口要居中：eframe 建窗口时不能指定位置，这里等句柄装好后摆一次
        #[cfg(windows)]
        if self.frames == 4 {
            frame::center_window();
        }

        // 启动过场：窗口已经开出来了，ffmpeg 探测还在后台跑；这期间只画过场，
        // 主界面的面板一个都不排版。结束的那一帧顺手铺满工作区（默认全屏）。
        let mut boot_data = None;
        if let Some(b) = self.boot.as_mut() {
            boot_data = b.poll();
            if boot_data.is_none() {
                b.paint(ctx);
                ctx.request_repaint();
                return;
            }
        }
        if let Some(d) = boot_data {
            self.boot = None;
            self.tools = d.tools;
            self.encoders = d.encoders;
            self.ffmpeg_ver = d.version;
            // 默认全屏：进主界面这一刻把过场小窗口铺满工作区
            set_fullscreen(&mut self.fullscreen, ctx, self.cfg.window.size_px());
            // 小窗口当还原点会很难看：按配置尺寸重算一个居中矩形
            #[cfg(windows)]
            {
                let (w, h) = self.cfg.window.size_px();
                frame::set_restore_size(w, h);
            }
            // 主界面的最小尺寸（布局规范：1120×600）
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                1120.0 / zoom(),
                600.0 / zoom(),
            )));
            let pre = std::mem::take(&mut self.preload);
            if !pre.is_empty() {
                let first = pre[0].clone();
                self.add_files(pre);
                // 双击文件打开：入队后直接播第一首（过场已经把 tools 备好了）
                if self.autoplay {
                    self.autoplay = false;
                    if let Some(idx) = self.items.iter().position(|i| i.path == first) {
                        self.selected = idx;
                        self.start_play(idx);
                    }
                }
            }
        }
        self.dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.05);
        // 强调色必须先于任何面板计算：这一帧画出去的所有选中条/圆点都用它
        self.refresh_accent(ctx);

        // F11 / Esc 切换全屏（Esc 只在全屏时退出）
        let f11 = ctx.input(|i| i.key_pressed(egui::Key::F11));
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if f11 || (esc && self.fullscreen) {
            set_fullscreen(&mut self.fullscreen, ctx, self.cfg.window.size_px());
        }
        // 命中区要知道当前缩放（120% 时按钮在物理像素上更大）
        #[cfg(windows)]
        frame::set_ui_scale(zoom());
        self.poll();
        self.poll_analysis();

        // 播放栏波形：当前显示的那首还没算过就后台补上（加密文件没有 info，跳过）
        if let Some(i) = self
            .player_for
            .as_ref()
            .and_then(|p| self.items.iter().position(|it| &it.path == p))
            .or_else(|| {
                if self.items.is_empty() {
                    None
                } else {
                    Some(self.selected)
                }
            })
        {
            if let Some(it) = self.items.get(i) {
                if it.info.is_some() {
                    let p = it.path.clone();
                    self.request_wave(&p, ctx);
                }
            }
        }
        self.poll_media(ctx);
        // CUE 分轨线程的结果
        if let Some(rx) = self.cue_rx.take() {
            match rx.try_recv() {
                Ok(Ok(files)) => {
                    self.cue_msg = Some(format!("分轨完成：{} 个文件", files.len()));
                }
                Ok(Err(e)) => self.cue_msg = Some(format!("分轨失败：{e}")),
                Err(std::sync::mpsc::TryRecvError::Empty) => self.cue_rx = Some(rx),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.cue_msg = Some(crate::tr!("分轨线程结束").to_string());
                }
            }
        }
        // 自动播放/首次进界面时 selected 没变化，不会触发“选中项变更”的波形请求；
        // 这里补一次，别让播放条一直空着
        if self.wave.is_none() && !self.items.is_empty() {
            if let Some(p) = self.items.get(self.selected).map(|i| i.path.clone()) {
                self.request_wave(&p, ctx);
            }
        }
        self.poll_big_cover(ctx);
        self.poll_library();
        self.poll_lib_covers(ctx);
        self.poll_spectrum();
        // F2 = 看频谱（按钮之外的快捷方式）
        if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.open_spectrum();
        }

        // 解码线程已经无缝切到下一首：同步界面、波形和再下一首
        if let Some((tn, cp)) = self
            .player
            .as_ref()
            .map(|p| (p.track_no(), p.current_path()))
        {
            if tn != self.play_track_seen {
                self.play_track_seen = tn;
                // cp 是解码用的路径（vgmstream 时是临时 WAV），映射回队列项
                if let Some(item) = self.items.iter().find(|it| it.media_path() == cp) {
                    let original = item.path.clone();
                    self.player_for = Some(original.clone());
                    if let Some(i) = self.items.iter().position(|it| it.path == original) {
                        self.selected = i;
                    }
                    if let Some(pi) = self.playlist.iter().position(|x| x == &original) {
                        self.play_idx = pi;
                    }
                    self.request_wave(&original, ctx);
                }
                self.arm_next();
            }
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
        self.bg.paint_mode(
            ctx,
            self.cfg.bg_mode,
            self.cfg.dim,
            self.cfg.bg_color,
            self.cfg.bg_gradient,
        );

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

        // 顶部“正在运行…”小条：后台枚举/探测时给个明确反馈
        if let Some(msg) = self.activity.clone() {
            egui::Area::new(egui::Id::new("activity_banner"))
                .anchor(
                    egui::Align2::CENTER_TOP,
                    egui::vec2(0.0, TITLEBAR_H + 10.0 + inset),
                )
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(8, 14, 24, 225))
                        .rounding(Rounding::same(9.0))
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(12.0).color(accent()));
                                ui.label(
                                    RichText::new(&msg).size(9.5).color(TEXT),
                                );
                            });
                        });
                });
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        // 设置页打开时把主界面藏起来：设置页是半透明玻璃，
        // 底下只该透出壁纸，不能透出队列和格式列表（会很脏）。
        // 设置页动画期间主界面要画在下面（不然滑动会露出壁纸的"闪屏"）；
        // 动画快结束时（t>=0.9，设置页已经盖满）再隐起来，切换点上没有可见变化
        // 设置页是整页切换，不做滑动动画——之前那套"滑入滑出"就是闪的来源
        let main_visible = !self.settings_open;

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

            // 播放栏：贴底的一条，鼠标移上去展开波形/音量（不再悬浮、不再拖）。
            // 必须排在右栏之前，这样它横跨「队列 + 详情 + 输出」整条底边，
            // 右栏再从它上方分出去——否则播放栏只能缩在中间那一栏底下。
            animating |= self.player_bar_panel(ctx);

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
                    .show(ctx, |ui| {
                        animating |= self.detail(ui);
                    });
            if let Some(i) = self.pending_analyze.take() {
                self.start_analyze(i, ctx);
            }
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
                    match self.page {
                        Page::Convert => animating |= self.queue(ui, ctx),
                        Page::Library => animating |= self.library_view(ui, ctx),
                        Page::Engine => self.codec_view(ui),
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
                let soft = (w * 0.16).max(28.0);
                // 让遮罩完整扫出右边（而不是卡在右边缘突然消失）；
                // 最后 25% 再整体淡出，收尾不突兀
                let edge = x0 - soft + (w + 2.0 * soft) * self.page_t;
                let f = ((self.page_t - 0.75) / 0.25).clamp(0.0, 1.0);
                let fade = 1.0 - f * f * (3.0 - 2.0 * f);
                let alpha_max = (236.0 * fade) as u8;
                // 用 Mesh 做水平渐变，比一条条矩形干净（不会有竖条纹）
                let xs = [
                    x0,
                    (edge - soft).clamp(x0, scr.right()),
                    (edge + soft).clamp(x0, scr.right()),
                    scr.right(),
                ];
                let alphas = [0u8, 0, alpha_max, alpha_max];
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
        self.settings_t = if self.settings_open { 1.0 } else { 0.0 };
        if self.wiki_open {
            self.wiki_page(ctx);
        }
        if self.settings_open {
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
        let to_tex = |bytes: Vec<u8>| -> Option<(Vec<u8>, u32, u32)> {
            let img = image::load_from_memory(&bytes).ok()?;
            let small = img.resize_exact(256, 256, image::imageops::FilterType::CatmullRom);
            let rgba = small.to_rgba8();
            Some((rgba.as_raw().clone(), rgba.width(), rgba.height()))
        };
        let mut cover = fc_core::extract_cover(&ffmpeg, &p).and_then(to_tex);
        // 加密容器（NCM）里封面是明文存的：ffmpeg 打不开整文件，但能单独取这张图
        if cover.is_none() {
            if let Some(c) = fc_core::ncm::cover(&p) {
                cover = to_tex(c.bytes);
            }
        }
        let _ = tx.send(MediaMsg { path: p.clone(), tags, cover });
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

pub(crate) fn output_path(input: &Path, ext: &str, cfg: &settings::Settings) -> Option<PathBuf> {
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
    let base = dir.join(format!("{stem}.{ext}"));
    match cfg.existing {
        settings::ExistingPolicy::Overwrite => Some(base),
        settings::ExistingPolicy::Skip => (!base.exists()).then_some(base),
        settings::ExistingPolicy::Rename => {
            let mut cand = base;
            let mut n = 1;
            while cand.exists() {
                cand = dir.join(format!("{stem} ({n}).{ext}"));
                n += 1;
            }
            Some(cand)
        }
    }
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

/// 命令行帮助。加新参数时记得同步这里，以及 docs/接手说明.md 的常用命令。
fn print_usage() {
    if i18n::is_en() {
        println!("ToneLore {} — Windows audio format converter", VERSION);
        println!();
        println!("Usage:");
        println!("  ToneLore.exe                          open the GUI");
        println!("  ToneLore.exe --convert <in> <out> [--to <format-id>] [--vcodec <codec>]");
        println!("  ToneLore.exe --formats                list output formats");
        println!("  ToneLore.exe --tools                  detect ffmpeg / external encoders");
        println!("  ToneLore.exe --devices                list audio devices");
        println!("  ToneLore.exe --engines                list playback engines");
        println!("  ToneLore.exe --asio-probe             probe ASIO drivers");
        println!("  ToneLore.exe --play <file> [secs]     preview (default 4s)");
        println!("  ToneLore.exe --tags <file>            tags and cover info
  ToneLore.exe --verify-lossless ...     compare decoded PCM for lossless targets");
        println!("  ToneLore.exe --identify <file>        encryption/source detection");
        println!("  ToneLore.exe --health <file>          content health check");
        println!("  ToneLore.exe --split-cue <cue> [--to flac] [--out-dir DIR]");
        println!("  ToneLore.exe --warnings <file>        per-format quality warnings");
        println!("  ToneLore.exe --version                show version");
        println!("  ToneLore.exe --help                   show this help");
        println!("  ToneLore.exe --register               register in \"Open with\"");
        println!("  ToneLore.exe --unregister             remove the registration");
        return;
    }
    println!("ToneLore {} — Windows 音频格式转换器", VERSION);
    println!();
    println!("用法:");
    println!("  ToneLore.exe                          打开图形界面");
    println!("  ToneLore.exe --convert <输入> <输出> [--to <格式id>] [--vcodec <编码>]");
    println!("  ToneLore.exe --formats                列出可输出格式");
    println!("  ToneLore.exe --tools                  探测 ffmpeg / 外部编码器");
    println!("  ToneLore.exe --devices                列出音频设备");
    println!("  ToneLore.exe --engines                列出播放引擎");
    println!("  ToneLore.exe --asio-probe             ASIO 驱动探测");
    println!("  ToneLore.exe --play <文件> [秒数]      试听（默认 4 秒）");
    println!("  ToneLore.exe --tags <文件>            标签与封面信息");
    println!("  ToneLore.exe --identify <文件>        加密/来源识别");
    println!("  ToneLore.exe --health <文件>          内容体检");
    println!("  ToneLore.exe --split-cue <cue> [--to flac] [--out-dir 目录]");
    println!("  ToneLore.exe --warnings <文件>        各格式的质量提示");
    println!("  ToneLore.exe --version                显示版本");
    println!("  ToneLore.exe --help                   显示本帮助");
    println!("  ToneLore.exe --register               注册到「打开方式」");
    println!("  ToneLore.exe --unregister             移除注册");
}

/// vgmstream 解码用的临时 WAV 路径（每次唯一，放 %TEMP%\ToneLore）。
fn vgm_temp_path() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join("ToneLore");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "vgm_{}_{}.wav",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

/// 命令行临时文件清理：离开作用域时删掉。
struct TempCleanup(Option<PathBuf>);

impl Drop for TempCleanup {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn main() -> eframe::Result<()> {
    #[cfg(feature = "vulkan")]
    std::env::set_var("WGPU_BACKEND", "vulkan");
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // --verify-lossless：无损转换后逐样本比对 PCM（也会从 args 里移除）
    let verify_lossless_flag = if let Some(pos) = args.iter().position(|a| a == "--verify-lossless") {
        args.remove(pos);
        true
    } else {
        false
    };

    // --lang en / --lang zh：命令行临时切界面语言（也会从 args 里移除，避免被当文件路径）
    if let Some(pos) = args.iter().position(|a| a == "--lang") {
        let value = args.get(pos + 1).cloned().unwrap_or_default();
        std::env::set_var("TONELORE_LANG", &value);
        let end = (pos + 2).min(args.len());
        args.drain(pos..end);
    }
    if let Ok(lang) = std::env::var("TONELORE_LANG") {
        i18n::set_lang(&lang);
        fc_analyze::set_en(i18n::is_en());
        fc_core::format::set_en(i18n::is_en());
        fc_core::wiki::set_en(i18n::is_en());
    }

    // 先按屏幕挑缩放：后面开窗、建界面、原生命中区都读它
    set_zoom(if screen_logical_height() >= 1150.0 { 1.2 } else { 1.0 });

    // 只有 GUI 模式做单实例检查；任何 --xxx 命令行模式都允许多开
    // -h / -V 也算命令行：不然已经有一个实例在跑时会被它吞掉
    let cli_mode = args
        .first()
        .map(|s| s.starts_with("--") || s == "-h" || s == "-V")
        .unwrap_or(false);
    // 带文件路径启动（双击/「打开方式」）时不要被已有窗口吞掉：开新窗口播它
    let has_path_arg = args.iter().any(|a| !a.starts_with('-'));
    if !cli_mode && !has_path_arg && single::already_running() {
        single::activate_existing();
        return Ok(());
    }

    // 命令行：版本与帮助。以前 --version 会当 GUI 启动并挂住，脚本只能去翻文件
    if matches!(args.first().map(|s| s.as_str()), Some("--version") | Some("-V")) {
        println!("ToneLore {}", VERSION);
        return Ok(());
    }
    if matches!(args.first().map(|s| s.as_str()), Some("--help") | Some("-h")) {
        print_usage();
        return Ok(());
    }
    // 文件关联：给安装脚本/高级用户用，不弹界面
    if matches!(args.first().map(|s| s.as_str()), Some("--register") | Some("--assoc")) {
        match assoc::register() {
            Ok(()) => {
                crate::say2!("已注册到「打开方式」；到系统设置里可以设为默认。", "Registered under \"Open with\"; set it as default in Windows Settings.");
                return Ok(());
            }
            Err(e) => {
                crate::err2!("注册失败：{e}", "Registration failed: {e}");
                std::process::exit(1);
            }
        }
    }
    if args.first().map(|s| s.as_str()) == Some("--unregister") {
        match assoc::unregister() {
            Ok(()) => {
                crate::say2!("已移除 ToneLore 的打开方式注册。", "ToneLore registration removed.");
                return Ok(());
            }
            Err(e) => {
                crate::err2!("移除失败：{e}", "Removal failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // 命令行：报告依赖探测结果（排查"为什么 ATRAC 不能选"用）
    if args.first().map(|s| s.as_str()) == Some("--tools") {
        let show = |name: &str, p: Option<std::path::PathBuf>| match p {
            Some(p) => println!("{name:<10} {}", p.display()),
            None => println!("{name:<10} 未找到"),
        };
        match fc_core::locate() {
            Some(t) => {
                show("ffmpeg", Some(t.ffmpeg.clone()));
                show("ffprobe", Some(t.ffprobe.clone()));
            }
            None => println!("{:<10} 未找到", "ffmpeg"),
        }
        show("atracdenc", fc_core::find_atracdenc());
        show("speexenc", fc_core::find_speexenc());
        show("wavpack", fc_core::find_wavpack());
        show("ofr", fc_core::find_ofr());
        show("mpcenc", fc_core::find_mpcenc());
        show("psxavenc", fc_core::find_psxavenc());
        show("qoaconv", fc_core::find_qoaconv());
        show("vgaudio", fc_core::find_vgaudio());
        show("vadpcm", fc_core::find_vadpcm());
        show("vgmstream", fc_core::find_vgmstream());
        show("openmpt123", fc_core::find_openmpt123());
        return Ok(());
    }

    // 命令行：列出可以当输出的格式 id（配 --convert --to 用）
    if args.first().map(|s| s.as_str()) == Some("--formats") {
        for f in fc_core::all().filter(|f| f.can_encode) {
            println!("{:<16} {:<24} .{}", f.id, f.name, f.ext);
        }
        return Ok(());
    }

    if args.first().map(|s| s.as_str()) == Some("--convert") {
        // vgmstream 能读、ffmpeg 读不了的游戏音频：先解成临时 WAV，再走正常流程
        let mut vgm_tmp: Option<PathBuf> = None;
        if let Some(p) = args.get(1) {
            let vgm = fc_core::is_vgmstream_ext(Path::new(p));
            let module = fc_core::is_module_ext(Path::new(p));
            if vgm || module {
                let decoded = if vgm {
                    fc_core::find_vgmstream().map(|exe| {
                        let tmp = std::env::temp_dir().join(format!(
                            "tb_vgm_{}_{}.wav",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        ));
                        (exe, tmp, true)
                    })
                } else {
                    fc_core::find_openmpt123().map(|exe| {
                        let tmp = std::env::temp_dir().join(format!(
                            "tb_mod_{}_{}.wav",
                            std::process::id(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        ));
                        (exe, tmp, false)
                    })
                };
                if let Some((exe, tmp, is_vgm)) = decoded {
                    let res = if is_vgm {
                        fc_core::vgmstream_decode(&exe, Path::new(p), &tmp)
                    } else {
                        fc_core::openmpt_decode(&exe, Path::new(p), &tmp)
                    };
                    match res {
                        Ok(()) => {
                            args[1] = tmp.to_string_lossy().into_owned();
                            vgm_tmp = Some(tmp);
                        }
                        Err(e) => crate::err2!("输入解码失败，继续用 ffmpeg 试：{e:#}", "Input decode failed; falling back to ffmpeg: {e:#}"),
                    }
                }
            }
        }
        let _vgm_cleanup = TempCleanup(vgm_tmp);
        let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
            crate::say2!("用法: ToneLore --convert <输入> <输出> [--to <格式id>]", "Usage: ToneLore --convert <in> <out> [--to <format-id>]");
            crate::say2!("例:   ToneLore --convert a.flac b.aptx --to aptx", "e.g.  ToneLore --convert a.flac b.aptx --to aptx");
            std::process::exit(2);
        };
        // --to 可选：不写就是默认目标（FLAC）。id 见左侧输出格式列表。
        let target = match args.iter().position(|a| a == "--to") {
            Some(i) => {
                let id = args.get(i + 1).map(|s| s.as_str()).unwrap_or("");
                match fc_core::by_id(id) {
                    Some(f) if f.can_encode => f,
                    Some(_) => {
                        crate::err2!("{id} 只能读，不能作为输出格式", "{id} is read-only and cannot be an output target");
                        std::process::exit(2);
                    }
                    None => {
                        crate::err2!("未知格式 id: {id}（试 --formats 看列表）", "Unknown format id: {id} (see --formats)");
                        std::process::exit(2);
                    }
                }
            }
            None => fc_core::default_target(),
        };
        let tools = fc_core::locate().expect(crate::tr!("找不到 ffmpeg / ffprobe"));
        let info = match fc_core::probe(&tools, Path::new(input)) {
            Ok(i) => i,
            Err(e) => {
                crate::err2!("无法读取: {e:#}", "Cannot read: {e:#}");
                std::process::exit(1);
            }
        };
        crate::say2!(
            "输入: {} / {} kbps / {} Hz / {:.1}s",
            "Input: {} / {} kbps / {} Hz / {:.1}s",
            info.codec_label(),
            info.bitrate_kbps,
            info.sample_rate,
            info.duration_s
        );
        // 外部编码器（ATRAC / Speex / WavPack 有损 / OptimFROG）走各自的 CLI，
        // 不能拼 ffmpeg 参数——否则会像以前一样静默走错路径
        let opts = fc_core::ConvertOptions::default();
        let mut external_done = false;
        if let Some(codec) = fc_core::external::codec_for(target.id, opts.atrac3_bitrate) {
            match fc_core::find_atracdenc() {
                Some(exe) => {
                    match fc_core::external::convert(
                        &tools,
                        &exe,
                        Path::new(input),
                        Path::new(output),
                        codec,
                        info.duration_s,
                        Box::new(|_| {}),
                    ) {
                        Ok(_) => external_done = true,
                        Err(e) => {
                            crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                            std::process::exit(1);
                        }
                    }
                }
                None => {
                    crate::err2!("找不到 atracdenc（ATRAC 输出需要它）", "atracdenc not found (required for ATRAC output)");
                    std::process::exit(1);
                }
            }
        } else if target.id == "speex" {
            match fc_core::find_speexenc() {
                Some(exe) => match fc_core::external::speex_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 speexenc", "speexenc not found");
                    std::process::exit(1);
                }
            }
        } else if target.id == "wavpack_lossy" {
            match fc_core::find_wavpack() {
                Some(exe) => match fc_core::external::wavpack_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    opts.wavpack_bitrate,
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 wavpack.exe", "wavpack.exe not found");
                    std::process::exit(1);
                }
            }
        } else if target.id == "musepack" {
            match fc_core::find_mpcenc() {
                Some(exe) => match fc_core::external::mpc_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    opts.mpc_profile,
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 mpcenc.exe", "mpcenc.exe not found");
                    std::process::exit(1);
                }
            }
        } else if target.id == "optimfrog" {
            match fc_core::find_ofr() {
                Some(exe) => match fc_core::external::ofr_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    opts.ofr_preset,
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 ofr.exe（OptimFROG 输出需要它）", "ofr.exe not found (required for OptimFROG output)");
                    std::process::exit(1);
                }
            }
        } else if target.id == "xa" || target.id == "vag" {
            match fc_core::find_psxavenc() {
                Some(exe) => match fc_core::external::psx_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    if target.id == "xa" { "xa" } else { "vag" },
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 psxavenc.exe（PS1 XA / PS2 VAG 输出需要它）", "psxavenc.exe not found (required for PS1 XA / PS2 VAG output)");
                    std::process::exit(1);
                }
            }
        } else if target.id == "qoa" {
            match fc_core::find_qoaconv() {
                Some(exe) => match fc_core::external::qoa_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 qoaconv.exe（QOA 输出需要它）", "qoaconv.exe not found (required for QOA output)");
                    std::process::exit(1);
                }
            }
        } else if matches!(target.id, "brstm" | "bcstm" | "bfstm" | "dsp" | "idsp") {
            match fc_core::find_vgaudio() {
                Some(exe) => match fc_core::external::vgaudio_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 VGAudioCli.exe（BRSTM/BCSTM/BFSTM/DSP/IDSP 输出需要它）", "VGAudioCli.exe not found (required for BRSTM/BCSTM/BFSTM/DSP/IDSP output)");
                    std::process::exit(1);
                }
            }
        } else if target.id == "vadpcm" {
            match fc_core::find_vadpcm() {
                Some(exe) => match fc_core::external::vadpcm_convert(
                    &tools,
                    &exe,
                    Path::new(input),
                    Path::new(output),
                    info.duration_s,
                    Box::new(|_| {}),
                ) {
                    Ok(_) => external_done = true,
                    Err(e) => {
                        crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
                        std::process::exit(1);
                    }
                },
                None => {
                    crate::err2!("找不到 vadpcm.exe（N64 VADPCM 输出需要它）", "vadpcm.exe not found (required for N64 VADPCM output)");
                    std::process::exit(1);
                }
            }
        }
        if external_done {
            crate::say2!("完成: {output}", "Done: {output}");
            return Ok(());
        }
        // 带上源信息：视频容器要靠它决定"能不能直接 copy"，
        // 否则 CLI 一律按 copy 走，转 mpg/vob 这种会直接失败
        // 视频容器要先按"这个 ffmpeg 里真有的编码器"挑一个，
        // 否则默认会写 libx264，而随包构建只有 libopenh264，CLI 会直接报错
        let mut opts = fc_core::ConvertOptions::default();
        // 视频容器可选编码：--vcodec prores / dnxhr / ffv1 / vp8 / mjpeg ...
        if let Some(pos) = args.iter().position(|a| a == "--vcodec") {
            if let Some(name) = args.get(pos + 1) {
                opts.video_codec = match name.to_ascii_lowercase().as_str() {
                    "copy" => fc_core::VideoCodec::Copy,
                    "h264" => fc_core::VideoCodec::H264,
                    "hevc" | "h265" => fc_core::VideoCodec::Hevc,
                    "vp9" => fc_core::VideoCodec::Vp9,
                    "vp8" => fc_core::VideoCodec::Vp8,
                    "av1" => fc_core::VideoCodec::Av1,
                    "mpeg2" => fc_core::VideoCodec::Mpeg2,
                    "mpeg4" => fc_core::VideoCodec::Mpeg4,
                    "mpeg1" => fc_core::VideoCodec::Mpeg1,
                    "theora" => fc_core::VideoCodec::Theora,
                    "wmv2" => fc_core::VideoCodec::Wmv2,
                    "dv" => fc_core::VideoCodec::Dv,
                    "amv" => fc_core::VideoCodec::Amv,
                    "flv" => fc_core::VideoCodec::Flv,
                    "prores" => fc_core::VideoCodec::ProRes,
                    "dnxhr" | "dnxhd" => fc_core::VideoCodec::DnxHr,
                    "ffv1" => fc_core::VideoCodec::Ffv1,
                    "mjpeg" => fc_core::VideoCodec::Mjpeg,
                    "gif" => fc_core::VideoCodec::Gif,
                    "apng" => fc_core::VideoCodec::Apng,
                    other => {
                        crate::err2!("未知视频编码: {other}", "Unknown video codec: {other}");
                        std::process::exit(2);
                    }
                };
            }
        }
        if fc_core::is_video(target) {
            let src_v = info.video_codec.clone().unwrap_or_default();
            let vc = fc_core::effective_video_codec(target.id, &opts, &src_v);
            let encs = fc_core::list_encoders(&tools.ffmpeg);
            if let Some(e) = fc_core::resolve_video_encoder(vc, &encs, "") {
                opts.video_encoder = e.to_string();
            }
        }
        let mut args = fc_core::args_for_with(target, &opts, Some(&info));
        // 芯片音乐没有固定时长：默认只渲染前 3 分钟（和 GUI 批量一致）
        if fc_core::is_chiptune(Path::new(input)) {
            args.push("-t".to_string());
            args.push(fc_core::CHIPTUNE_SECONDS.to_string());
        }
        let arg_refs: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
        if let Err(e) = fc_core::convert(
            &tools,
            Path::new(input),
            Path::new(output),
            &arg_refs,
            info.duration_s,
            Box::new(|_| {}),
        ) {
            crate::err2!("转换失败: {e:#}", "Conversion failed: {e:#}");
            std::process::exit(1);
        }
        if let Err(why) = fc_core::verify_output(&tools, Path::new(output), info.duration_s) {
            crate::err2!("文件已生成，但自检没通过：{why}", "File written, but self-check failed: {why}");
            std::process::exit(1);
        }
        crate::say2!("完成: {output}", "Done: {output}");
        return Ok(());
    }

    // 命令行：列出音频输出/输入设备（排查"为什么没声音"很有用）
    if args.first().map(|s| s.as_str()) == Some("--devices") {
        match fc_player::devices::list() {
            Ok(list) => {
                if list.is_empty() {
                    crate::say2!("没有检测到音频设备", "No audio devices detected");
                }
                for d in list {
                    println!(
                        "{}{:<10} {:<12}{}{}",
                        if d.is_default { crate::tr!("[默认] ") } else { "       " },
                        match d.flow {
                            fc_player::devices::Flow::Output => crate::tr!("输出"),
                            fc_player::devices::Flow::Input => crate::tr!("输入"),
                        },
                        d.kind.label(),
                        if d.is_bluetooth { crate::tr!("蓝牙 ") } else { "" },
                        format!("{} | {}", d.name, d.format_label)
                    );
                }
            }
            Err(e) => crate::err2!("枚举设备失败: {e:#}", "Device enumeration failed: {e:#}"),
        }
        return Ok(());
    }

    // 命令行：ASIO 探测放在子进程里跑，驱动崩溃也不会带走主程序
    if args.first().map(|s| s.as_str()) == Some("--asio-probe") {
        #[cfg(windows)]
        for (name, status) in fc_player::asio::probe() {
            println!("{name}\t{status}");
        }
        return Ok(());
    }

    // 命令行：打印当前用到的 ffmpeg/ffprobe 路径
    if args.first().map(|s| s.as_str()) == Some("--engines") {
        // 自检：本机有哪些播放引擎可用
        #[cfg(windows)]
        print!("{}", fc_player::asio::describe());
        println!("Core Audio：{}", fc_player::core_audio::status_line());
        crate::say2!("自研引擎：可用（自己的重采样 + 环形缓冲 + WASAPI 共享）", "Custom engine: available (own resampler + ring buffer + WASAPI shared)");
        let cfg_e = settings::Settings::load();
        crate::say2!("当前配置的引擎：{}", "Configured engine: {}", cfg_e.engine_enum().label());
        return Ok(());
    }
    if args.first().map(|s| s.as_str()) == Some("--tools") {
        match fc_core::locate() {
            Some(t) => {
                println!("ffmpeg : {}", t.ffmpeg.display());
                println!("ffprobe: {}", t.ffprobe.display());
            }
            None => crate::say2!("没找到 ffmpeg（需要 ffmpeg.exe + ffprobe.exe）", "ffmpeg not found (ffmpeg.exe + ffprobe.exe required)"),
        }
        match fc_core::find_atracdenc() {
            Some(p) => println!("atracdenc: {}", p.display()),
            None => crate::say2!("atracdenc: 未安装（ATRAC 输出需要它，见设置页说明）", "atracdenc: not installed (required for ATRAC output; see Settings)"),
        }
        return Ok(());
    }

    // 命令行：打印标签与封面信息（调试用）
    if args.first().map(|s| s.as_str()) == Some("--tags") {
        let Some(input) = args.get(1) else {
            crate::say2!("用法: ToneLore --tags <文件>", "Usage: ToneLore --tags <file>");
            std::process::exit(2);
        };
        let tools = fc_core::locate().expect(crate::tr!("找不到 ffprobe"));
        let path = Path::new(input);
        let t = fc_core::read_tags(&tools.ffprobe, path);
        crate::say2!("标题: {}", "Title: {}", t.display_title(crate::tr2!("(无)", "(none)")));
        crate::say2!("副标题: {}", "Subtitle: {}", if t.display_sub().is_empty() { crate::tr2!("(无)", "(none)").to_string() } else { t.display_sub() });
        crate::say2!("封面: {}", "Cover: {}", if fc_core::has_cover(&tools.ffprobe, path) { crate::tr2!("有", "yes") } else { crate::tr2!("无", "no") });
        if let Some(bytes) = fc_core::extract_cover(&tools.ffmpeg, path) {
            crate::say2!("封面字节: {}（{}）", "Cover bytes: {} ({})", bytes.len(),
                if bytes.starts_with(&[0xFF, 0xD8]) { "JPEG" } else { "PNG" });
        }
        return Ok(());
    }

    // 命令行：只做加密识别，不开界面
    if args.first().map(|s| s.as_str()) == Some("--identify") {
        let Some(input) = args.get(1) else {
            crate::say2!("用法: ToneLore --identify <文件>", "Usage: ToneLore --identify <file>");
            std::process::exit(2);
        };
        let path = Path::new(input);
        match fc_core::identify(path) {
            Some(e) => {
                println!("{}", e.summary());
                println!();
                println!("{}", e.full_text());
            }
            None => crate::say2!("不是加密文件（或不在已知格式库里）", "Not encrypted (or not in the known-format database)"),
        }
        return Ok(());
    }

    // 命令行：内容体检（测试用，也方便脚本批跑）
    if args.first().map(|s| s.as_str()) == Some("--wave") {
        let Some(input) = args.get(1) else {
            crate::say2!("用法: ToneLore.exe --wave <文件>", "Usage: ToneLore.exe --wave <file>");
            std::process::exit(2);
        };
        let Some(tools) = fc_core::locate() else {
            crate::err2!("找不到 ffmpeg", "ffmpeg not found");
            std::process::exit(1);
        };
        match fc_analyze::waveform(&tools.ffmpeg, Path::new(input), 480) {
            Ok(w) => {
                let max = w
                    .iter()
                    .flatten()
                    .map(|(a, b)| a.abs().max(b.abs()))
                    .fold(0.0f32, f32::max);
                let peak_cols = w
                    .iter()
                    .filter(|col| col.iter().any(|(a, b)| a.abs().max(b.abs()) > 0.02))
                    .count();
                println!(
                    "cols={} channels={} max={max:.4} cols_over_0.02={peak_cols}",
                    w.len(),
                    w.first().map(|c| c.len()).unwrap_or(0)
                );
                for (i, col) in w.iter().take(3).enumerate() {
                    let (lo, hi) = col.first().copied().unwrap_or((0.0, 0.0));
                    println!("  {i}: first={lo:.3}/{hi:.3}");
                }
            }
            Err(e) => {
                crate::err2!("波形失败: {e:#}", "Waveform failed: {e:#}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    // 诊断/脚本用：在资源管理器里定位文件（和库里按钮同一条路径）
    // 诊断：英文词典覆盖率（格式说明 / 分组标题）
    if args.first().map(|s| s.as_str()) == Some("--i18n-check") {
        i18n::set_lang("en");
        let all: Vec<_> = fc_core::all().collect();
        let missing: Vec<&str> = all
            .iter()
            .filter(|f| crate::i18n::tr(f.blurb) == f.blurb)
            .map(|f| f.id)
            .collect();
        println!(
            "blurbs: {}/{} translated; missing {}",
            all.len() - missing.len(),
            all.len(),
            missing.join(", ")
        );
        let mut group_missing = Vec::new();
        for g in fc_core::Group::all() {
            if crate::i18n::tr(g.title()) == g.title() {
                group_missing.push(g.title());
            }
        }
        println!("groups missing: {:?}", group_missing);
        println!("wiki en entries: {}", fc_core::wiki_en::TABLE.len());
        return Ok(());
    }
    if args.first().map(|s| s.as_str()) == Some("--reveal") {
        if let Some(p) = args.get(1) {
            library::reveal_in_explorer(Path::new(p));
        }
        return Ok(());
    }
    if args.first().map(|s| s.as_str()) == Some("--split-cue") {
        let Some(cue) = args.get(1) else {
            crate::say2!("用法: ToneLore.exe --split-cue <cue> [--to flac] [--out-dir 目录]", "Usage: ToneLore.exe --split-cue <cue> [--to flac] [--out-dir DIR]");
            std::process::exit(2);
        };
        let mut target = "flac";
        let mut out_dir: Option<PathBuf> = None;
        if let Some(pos) = args.iter().position(|a| a == "--to") {
            if let Some(v) = args.get(pos + 1) {
                target = v.as_str();
            }
        }
        if let Some(pos) = args.iter().position(|a| a == "--out-dir") {
            if let Some(v) = args.get(pos + 1) {
                out_dir = Some(PathBuf::from(v));
            }
        }
        let out_dir = out_dir.unwrap_or_else(|| {
            Path::new(cue)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        });
        let Some(tools) = fc_core::locate() else {
            crate::err2!("找不到 ffmpeg", "ffmpeg not found");
            std::process::exit(1);
        };
        match fc_core::cue::split(&tools, Path::new(cue), &out_dir, target, |done, total| {
            crate::say2!("  分轨 {done}/{total}", "  Splitting {done}/{total}");
        }) {
            Ok(files) => {
                for f in &files {
                    println!("完成: {}", f.display());
                }
                return Ok(());
            }
            Err(e) => {
                crate::err2!("分轨失败: {e:#}", "Split failed: {e:#}");
                std::process::exit(1);
            }
        }
    }
    if args.first().map(|s| s.as_str()) == Some("--health") {
        let Some(input) = args.get(1) else {
            crate::say2!("用法: ToneLore --health <文件>", "Usage: ToneLore --health <file>");
            std::process::exit(2);
        };
        let tools = fc_core::locate().expect(crate::tr!("找不到 ffmpeg"));
        match fc_analyze::health::analyze(&tools, Path::new(input)) {
            Ok(h) => {
                for c in &h.checks {
                    let mark = match c.severity {
                        fc_analyze::health::Severity::Ok => "OK  ",
                        fc_analyze::health::Severity::Warn => "WARN",
                        fc_analyze::health::Severity::Bad => "BAD ",
                    };
                    println!("{mark} {:<10} {}", c.name, c.value);
                }
                println!();
                println!(
                    "结论: {}（时长 {:.1}s，onset {:.2}/s）",
                    h.verdict.label(),
                    h.duration_s,
                    h.onset_rate
                );
            }
            Err(e) => eprintln!("{e:#}"),
        }
        return Ok(());
    }

    // 命令行：列出所有格式在当前源文件上的质量提示（测试用）
    if args.first().map(|s| s.as_str()) == Some("--warnings") {
        let Some(input) = args.get(1) else {
            crate::say2!("用法: ToneLore --warnings <文件>", "Usage: ToneLore --warnings <file>");
            std::process::exit(2);
        };
        let tools = fc_core::locate().expect(crate::tr!("找不到 ffmpeg"));
        let info = fc_core::probe(&tools, Path::new(input)).expect(crate::tr!("读不到音频信息"));
        let mut opts = fc_core::ConvertOptions::default();
        opts.bit_depth = fc_core::format::BitDepth::Int24;
        opts.sample_rate = Some(48000);
        for f in fc_core::format::browsable() {
            let w = fc_core::format::quality_warnings(&info, f, &opts);
            let kinds: Vec<String> = w
                .iter()
                .map(|x| format!("{:?}", x.kind))
                .collect();
            println!("{:<10} {}", f.id, kinds.join(","));
        }
        return Ok(());
    }

    if args.first().map(|s| s.as_str()) == Some("--play") {
        let Some(input) = args.get(1) else {
            crate::say2!("用法: ToneLore --play <文件> [秒数]", "Usage: ToneLore --play <file> [seconds]");
            std::process::exit(2);
        };
        let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4.0);
        let tools = fc_core::locate().expect(crate::tr!("找不到 ffmpeg"));
        let info = match fc_core::probe(&tools, Path::new(input)) {
            Ok(i) => i,
            Err(e) => {
                crate::err2!("无法读取: {e:#}", "Cannot read: {e:#}");
                std::process::exit(1);
            }
        };
        println!(
            "试听: {} / {} kbps / {:.1}s",
            info.codec_label(),
            info.bitrate_kbps,
            info.duration_s
        );
        let cfg = settings::Settings::load();
        let tags = fc_core::read_tags(&tools.ffprobe, Path::new(input));
        let tier = fc_core::dsd_tier(&info.codec, info.sample_rate, info.bitrate_kbps);
        let dop_rate = fc_player::dop::dop_rate_for_tier(tier);
        let popts = fc_player::PlayOptions {
            device_name: if cfg.audio_out_device.is_empty() {
                None
            } else {
                Some(cfg.audio_out_device.clone())
            },
            source_rate: dop_rate.unwrap_or(info.sample_rate),
            replaygain_db: if cfg.replaygain {
                tags.replaygain_db
            } else {
                None
            },
            engine: if dop_rate.is_some() {
                fc_player::Engine::Exclusive
            } else {
                cfg.engine_enum()
            },
            dop: dop_rate.is_some(),
            dop_lsb_first: info.codec.contains("lsbf"),
        };
        if info.needs_external_decoder() && !fc_core::ffmpeg::has_decoder(&tools, &info.codec) {
            eprintln!(
                "{}：内置 ffmpeg 没有这个空间音频解码器，不能试听；请保留原文件。",
                info.codec_label()
            );
            std::process::exit(1);
        }
        let mut pl = match fc_player::Player::open_with(
            &tools.ffmpeg,
            Path::new(input),
            info.duration_s,
            &popts,
        ) {
            Ok(p) => p,
            Err(e) => {
                crate::err2!("打开音频输出失败: {e:#}", "Failed to open audio output: {e:#}");
                std::process::exit(1);
            }
        };
        crate::say2!("输出: {}", "Output: {}", pl.output_label());
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            std::thread::sleep(std::time::Duration::from_millis(500));
            crate::say2!("  位置 {:.2}s / {:.2}s", "  Position {:.2}s / {:.2}s", pl.position_s(), info.duration_s);
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

    // 先开一个小的过场窗口（检查完再铺满工作区）。
    // 这里不能用配置里的窗口尺寸：那会变成一个满屏空壳先晃一下。
    let viewport = egui::ViewportBuilder::default()
            .with_inner_size([boot::SPLASH_W, boot::SPLASH_H])
            // 过场阶段的最小尺寸先放宽，进主界面时再抬到 1120×600
            .with_min_inner_size([400.0, 320.0])
            .with_decorations(false)
            .with_title(APP_NAME);
    // wgpu/Vulkan 后端的透明窗口和 DWM 玻璃不兼容，会出现黑块；Vulkan 版直接用不透明表面
    let mut viewport = if cfg!(feature = "vulkan") {
        viewport.with_transparent(false)
    } else {
        viewport.with_transparent(true)
    };
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let title = if cfg!(feature = "vulkan") {
        crate::tr!("声识ToneLore Vulkan")
    } else {
        APP_NAME
    };
    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    #[cfg(feature = "vulkan")]
    {
        options.renderer = eframe::Renderer::Wgpu;
    }
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc);
            // 队列里每首都要 probe，得等过场把 tools 备好；先存着，过场结束再塞
            app.autoplay = !preload.is_empty();
            app.preload = preload;
            Ok(Box::new(app))
        }),
    )
}
/// 启动时清掉 %TEMP%\ToneLore 里超过 24 小时的文件（上次异常退出留下的）。
fn clean_stale_temp() {
    let dir = std::env::temp_dir().join("ToneLore");
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let now = std::time::SystemTime::now();
    for e in rd.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(|d| d.as_secs() > 24 * 3600)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        for it in &self.items {
            it.forget_media();
        }
    }
}

impl App {
    /// 往诊断控制台写一行，同时追加到会话日志文件。
    pub(crate) fn log(&mut self, line: impl Into<String>) {
        let entry = line.into();
        self.console.push(entry.clone());
        if self.console.len() > 2000 {
            self.console.remove(0);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            use std::io::Write;
            let _ = writeln!(f, "{entry}");
        }
    }
}
#[cfg(test)]
mod output_policy_tests {
    use super::*;

    #[test]
    fn existing_policy_controls_output_name() {
        let dir = std::env::temp_dir().join(format!("tb_policy_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("song.wav");
        std::fs::write(&input, b"x").unwrap();
        std::fs::write(dir.join("song.flac"), b"x").unwrap();

        let mut cfg = settings::Settings::default();
        cfg.output_mode = settings::OutputMode::Custom;
        cfg.output_dir = dir.display().to_string();

        cfg.existing = settings::ExistingPolicy::Rename;
        let p = output_path(&input, "flac", &cfg).unwrap();
        assert_eq!(p.file_name().unwrap(), "song (1).flac");

        cfg.existing = settings::ExistingPolicy::Skip;
        assert!(output_path(&input, "flac", &cfg).is_none());

        cfg.existing = settings::ExistingPolicy::Overwrite;
        let p = output_path(&input, "flac", &cfg).unwrap();
        assert_eq!(p.file_name().unwrap(), "song.flac");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
/// 失败原因的大类，给任务列表一个一眼能懂的前缀。
fn classify_error(msg: &str) -> &'static str {
    let m = msg.to_ascii_lowercase();
    if msg.contains("找不到") || m.contains("not found") || m.contains("no such file") {
        crate::tr2!("缺少外部工具", "Missing tool")
    } else if msg.contains("解码") || m.contains("decode") || m.contains("invalid data") {
        crate::tr2!("输入解码失败", "Input decode")
    } else if m.contains("no space") || msg.contains("磁盘") {
        crate::tr2!("磁盘空间不足", "Disk full")
    } else if msg.contains("自检") || m.contains("verify") {
        crate::tr2!("输出自检失败", "Output self-check")
    } else if m.contains("encoder") || msg.contains("编码") {
        crate::tr2!("编码失败", "Encoder")
    } else {
        crate::tr2!("转换失败", "Conversion")
    }
}

#[cfg(test)]
mod error_classify_tests {
    use super::classify_error;

    #[test]
    fn classifies_common_failures() {
        assert_eq!(classify_error("找不到 atracdenc"), "缺少外部工具");
        assert_eq!(classify_error("ffmpeg 解码失败"), "输入解码失败");
        assert_eq!(classify_error("磁盘空间不足"), "磁盘空间不足");
        assert_eq!(classify_error("输出自检没通过"), "输出自检失败");
        assert_eq!(classify_error("encoder not found"), "缺少外部工具");
        assert_eq!(classify_error("something weird"), "转换失败");
    }
}