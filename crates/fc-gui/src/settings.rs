//! 设置项与持久化。
//!
//! 存到 `%APPDATA%\FormatConverter\config.json`。
//! 只放**界面侧**的设置；影响 ffmpeg 参数的（位深、压缩等级）等契约变更后再加。
//!
//! ## 用户配置（Profile）
//!
//! 一套「常用配置」= 转换参数 + 输出位置 + 元数据偏好。用户可以建多套、随时切换、
//! 改个名字、导出成 JSON 带到另一台机器。`cfg.opts` 这些字段始终是"当前这套"的展开，
//! 切换/保存时和 `profiles[active]` 互相同步。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 界面缩放档位。只影响显示，不影响转换结果。

fn default_bg_color() -> [u8; 3] {
    [20, 22, 30]
}

/// 播放栏默认高度（和 util::BAR_H 保持一致）
fn default_bar_h() -> f32 {
    164.0
}

/// 窗口大小。固定尺寸，切换时直接改窗口，不做界面缩放，
/// 这样布局永远按设计宽度排，不会拉歪。
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub enum WindowSize {
    /// 小（1800×780）—— 1080p 的机器
    Compact,
    /// 中（1874×840，默认）
    #[default]
    Normal,
    /// 大（2000×900）—— 2K 屏
    Large,
    /// 超大（2200×1100）—— 大屏
    Huge,
}

impl WindowSize {
    pub fn label(self) -> &'static str {
        match self {
            WindowSize::Compact => "小",
            WindowSize::Normal => "中",
            WindowSize::Large => "大",
            WindowSize::Huge => "超大",
        }
    }
    /// 返回 (宽, 高)，单位是物理像素。
    /// 五列固定宽度合计 1339px；歌曲列表的每一行还要放封面+标题+徽章，
    /// 低于 1800 就会挤在一起。
    pub fn size_px(self) -> (f32, f32) {
        match self {
            WindowSize::Compact => (1800.0, 780.0),
            WindowSize::Normal => (1874.0, 840.0),
            WindowSize::Large => (2000.0, 900.0),
            WindowSize::Huge => (2200.0, 1100.0),
        }
    }
    pub fn all() -> [WindowSize; 4] {
        [
            WindowSize::Compact,
            WindowSize::Normal,
            WindowSize::Large,
            WindowSize::Huge,
        ]
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum BgMode {
    /// 壁纸模糊（默认）
    Wallpaper,
    /// 纯色
    Solid,
    /// 渐变
    Gradient,
    /// 关闭（最省显卡）
    Off,
}

impl BgMode {
    pub fn label(self) -> &'static str {
        match self {
            BgMode::Wallpaper => "壁纸模糊",
            BgMode::Solid => "纯色",
            BgMode::Gradient => "渐变",
            BgMode::Off => "关闭",
        }
    }
    pub fn all() -> [BgMode; 4] {
        [BgMode::Wallpaper, BgMode::Solid, BgMode::Gradient, BgMode::Off]
    }
}

/// 转换后的文件放哪
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum OutputMode {
    /// 和源文件同目录（默认）
    #[default]
    Source,
    /// 指定目录
    Custom,
}

/// 用户自设的预设：一套格式 + 参数，可以改名。跟着「配置 1/2/3」走。
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct UserPreset {
    pub name: String,
    /// 目标格式 id（fc_core::by_id 用）
    pub format: String,
    #[serde(default)]
    pub opts: fc_core::ConvertOptions,
}

/// 一套常用配置。切换配置时整块替换。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub opts: fc_core::ConvertOptions,
    #[serde(default)]
    pub output_mode: OutputMode,
    #[serde(default)]
    pub output_dir: String,
    #[serde(default = "default_bg_color")]
    pub bg_color: [u8; 3],
    #[serde(default)]
    pub bg_gradient: u8,
    #[serde(default)]
    pub loudness: Option<f32>,
    #[serde(default)]
    pub write_tags: bool,
    #[serde(default)]
    pub meta_artist: String,
    #[serde(default)]
    pub meta_album: String,
    #[serde(default)]
    pub meta_cover: String,
    /// 这套配置里用户自设的预设
    #[serde(default)]
    pub presets: Vec<UserPreset>,
}

impl Profile {
    pub fn from_settings(s: &Settings, name: String) -> Self {
        Self {
            name,
            opts: s.opts.clone(),
            output_mode: s.output_mode,
            output_dir: s.output_dir.clone(),
            bg_color: s.bg_color,
            bg_gradient: s.bg_gradient,
            loudness: s.loudness,
            write_tags: s.write_tags,
            meta_artist: s.meta_artist.clone(),
            meta_album: s.meta_album.clone(),
            meta_cover: s.meta_cover.clone(),
            presets: s.presets.clone(),
        }
    }
}

/// 音乐库扫描结果的一条缓存。持久化到 config.json，
/// 这样每次启动不用重扫（用户自己点「重新扫描」时才更新）。
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct CachedTrack {
    pub path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub genre: String,
    #[serde(default)]
    pub duration_s: f64,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub bits: Option<u32>,
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default)]
    pub bitrate_kbps: u64,
    /// 有损压缩过（DSD/FLAC 之类为 false）
    #[serde(default)]
    pub lossy: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Settings {
    pub bg_mode: BgMode,
    /// 纯色背景的颜色
    #[serde(default = "default_bg_color")]
    pub bg_color: [u8; 3],
    /// 渐变背景用第几套配色（见 bg::GRADIENTS）
    #[serde(default)]
    pub bg_gradient: u8,
    /// 壁纸压暗程度（0~255），数值越大越暗
    pub dim: u8,
    /// 播放栏高度（逻辑点，含顶部拖拽条）。用户拖过就记住，下次启动沿用。
    #[serde(default = "default_bar_h")]
    pub bar_height: f32,
    /// 播放输出设备（WASAPI 友好名）。空 = 系统默认输出。
    #[serde(default)]
    pub audio_out_device: String,
    /// 播放时应用 ReplayGain（读文件里的 replaygain_track_gain 标签）
    #[serde(default)]
    pub replaygain: bool,
    /// 播放走 WASAPI 独占（直通；失败自动退回共享）
    #[serde(default)]
    pub exclusive_output: bool,
    /// 选中文件就自动做频谱分析
    pub auto_analyze: bool,
    /// 输出参数（压缩等级 / 位深 / 码率……）——当前配置的展开
    #[serde(default)]
    pub opts: fc_core::ConvertOptions,
    /// 输出位置模式
    #[serde(default)]
    pub output_mode: OutputMode,
    /// 指定目录（output_mode = Custom 时用）
    #[serde(default)]
    pub output_dir: String,
    /// 批量元数据：艺术家（空 = 沿用源文件标签）
    #[serde(default)]
    pub meta_artist: String,
    /// 批量元数据：专辑
    #[serde(default)]
    pub meta_album: String,
    /// 封面图路径（空 = 沿用源文件内嵌封面）
    #[serde(default)]
    pub meta_cover: String,
    /// 响度归一化目标（LUFS）；None = 不动。存在配置里，切配置会跟着走。
    #[serde(default)]
    pub loudness: Option<f32>,
    /// 是否写标签（默认关：先把文件转出来，标签需要时再开）
    #[serde(default)]
    pub write_tags: bool,
    /// 界面缩放（1.0 = 100%）。用 egui 的 zoom factor 实现，文字和控件一起放大，
    /// 否则我们那些写死的 `.size(9.5)` 根本不会变。
    /// 窗口大小：固定尺寸，靠这个切换（不再做界面缩放）
    #[serde(default)]
    pub window: WindowSize,
    /// 音乐库文件夹（绝对路径）。用户自己挑，扫出来的文件不落盘、每次启动可重扫。
    #[serde(default)]
    pub library_dirs: Vec<String>,
    /// 单独加进音乐库的文件（不在任何库文件夹里也能听）
    #[serde(default)]
    pub library_files: Vec<String>,
    /// 上次扫描的结果（用户手动扫描后才更新）
    #[serde(default)]
    pub library_cache: Vec<CachedTrack>,
    /// 用户自设的预设（当前配置的展开）
    #[serde(default)]
    pub presets: Vec<UserPreset>,
    /// 用户配置列表
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// 当前生效的配置下标
    #[serde(default)]
    pub active_profile: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bg_mode: BgMode::Wallpaper,
            bg_color: default_bg_color(),
            bg_gradient: 0,
            dim: 96,
            bar_height: default_bar_h(),
            audio_out_device: String::new(),
            replaygain: false,
            exclusive_output: false,
            auto_analyze: true,
            opts: fc_core::ConvertOptions::default(),
            output_mode: OutputMode::Source,
            output_dir: String::new(),
            meta_artist: String::new(),
            meta_album: String::new(),
            meta_cover: String::new(),
            loudness: None,
            write_tags: false,
            window: WindowSize::Large,
            library_dirs: Vec::new(),
            library_files: Vec::new(),
            library_cache: Vec::new(),
            presets: Vec::new(),
            profiles: Vec::new(),
            active_profile: 0,
        }
    }
}

impl Settings {
    pub fn dir() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("FormatConverter"))
    }

    pub fn file() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.json"))
    }

    pub fn load() -> Self {
        let mut s = Self::file()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default();
        if s.profiles.is_empty() {
            // 第一次用（或老配置）：拿当前设置生成三套，用户可以直接改名用
            let list: Vec<Profile> = (1..=3)
                .map(|n| Profile::from_settings(&s, format!("配置 {n}")))
                .collect();
            s.profiles = list;
            s.active_profile = 0;
        }
        s.active_profile = s.active_profile.min(s.profiles.len().saturating_sub(1));
        s.apply_active();
        s
    }

    /// 把当前展开的设置写回当前配置（切换/保存前调用）
    pub fn snapshot_active(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let i = self.active_profile.min(self.profiles.len() - 1);
        let p = &mut self.profiles[i];
        p.opts = self.opts.clone();
        p.output_mode = self.output_mode;
        p.output_dir = self.output_dir.clone();
        // 背景色/渐变和响度都属于"这套配置"：apply_active 会从这里读回来，
        // 只写不读（或反过来）会让用户改了设置、重启后被打回原样。
        p.bg_color = self.bg_color;
        p.bg_gradient = self.bg_gradient;
        p.loudness = self.loudness;
        p.write_tags = self.write_tags;
        p.meta_artist = self.meta_artist.clone();
        p.meta_album = self.meta_album.clone();
        p.meta_cover = self.meta_cover.clone();
        p.presets = self.presets.clone();
    }

    /// 把当前配置铺回展开的设置
    pub fn apply_active(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let i = self.active_profile.min(self.profiles.len() - 1);
        let p = self.profiles[i].clone();
        self.opts = p.opts;
        self.output_mode = p.output_mode;
        self.output_dir = p.output_dir;
        self.bg_color = p.bg_color;
        self.bg_gradient = p.bg_gradient;
        self.loudness = p.loudness;
        self.write_tags = p.write_tags;
        self.meta_artist = p.meta_artist;
        self.meta_album = p.meta_album;
        self.meta_cover = p.meta_cover;
        self.presets = p.presets;
    }

    pub fn switch_profile(&mut self, i: usize) {
        if i >= self.profiles.len() {
            return;
        }
        self.snapshot_active();
        self.active_profile = i;
        self.apply_active();
        self.save();
    }

    pub fn add_profile(&mut self) {
        self.snapshot_active();
        let n = self.profiles.len() + 1;
        let p = Profile::from_settings(self, format!("配置 {n}"));
        self.profiles.push(p);
        self.active_profile = self.profiles.len() - 1;
        self.save();
    }

    pub fn rename_profile(&mut self, i: usize, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() || i >= self.profiles.len() {
            return;
        }
        self.profiles[i].name = name;
        self.save();
    }

    pub fn remove_profile(&mut self, i: usize) {
        if self.profiles.len() <= 1 || i >= self.profiles.len() {
            return;
        }
        self.profiles.remove(i);
        if self.active_profile >= self.profiles.len() {
            self.active_profile = self.profiles.len() - 1;
        }
        self.apply_active();
        self.save();
    }

    /// 保存到配置文件。先把当前展开项写回当前配置，避免"改了参数没保存进配置"。
    pub fn save(&mut self) {
        self.snapshot_active();
        let Some(dir) = Self::dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        if let Some(p) = Self::file() {
            if let Ok(s) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 背景色/渐变/响度必须能来回走一趟：apply_active 会从 Profile 读回来，
    /// snapshot_active 漏写任何一项，重启后用户改的设置就没了。
    #[test]
    fn profile_snapshot_roundtrips_background_and_loudness() {
        let mut s = Settings::default();
        s.profiles = vec![
            Profile::from_settings(&s, "A".into()),
            Profile::from_settings(&s, "B".into()),
        ];
        s.active_profile = 0;
        s.bg_color = [1, 2, 3];
        s.bg_gradient = 7;
        s.loudness = Some(-14.0);
        s.snapshot_active();

        // 切到 B：应该是 B 自己的值（默认 None / 0）
        s.active_profile = 1;
        s.apply_active();
        assert_eq!(s.loudness, None);
        assert_eq!(s.bg_gradient, 0);

        // 切回 A：刚才改的三项都还在
        s.active_profile = 0;
        s.apply_active();
        assert_eq!(s.loudness, Some(-14.0));
        assert_eq!(s.bg_gradient, 7);
        assert_eq!(s.bg_color, [1, 2, 3]);
    }
}
