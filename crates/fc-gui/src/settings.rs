//! 设置项与持久化。
//!
//! 存到 `%APPDATA%\FormatConverter\config.json`。
//! 只放**界面侧**的设置；影响 ffmpeg 参数的（位深、压缩等级）等契约变更后再加。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum BgMode {
    /// 壁纸模糊（默认）
    Wallpaper,
    /// 纯色
    Solid,
    /// 关闭（最省显卡）
    Off,
}

impl BgMode {
    pub fn label(self) -> &'static str {
        match self {
            BgMode::Wallpaper => "壁纸模糊",
            BgMode::Solid => "纯色",
            BgMode::Off => "关闭",
        }
    }
    pub fn all() -> [BgMode; 3] {
        [BgMode::Wallpaper, BgMode::Solid, BgMode::Off]
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Settings {
    pub bg_mode: BgMode,
    /// 壁纸压暗程度（0~255），数值越大越暗
    pub dim: u8,
    /// 选中文件就自动做频谱分析
    pub auto_analyze: bool,
    /// 输出参数（压缩等级 / 位深 / 码率……）
    #[serde(default)]
    pub opts: fc_core::ConvertOptions,
    /// 输出位置模式
    #[serde(default)]
    pub output_mode: OutputMode,
    /// 指定目录（output_mode = Custom 时用）
    #[serde(default)]
    pub output_dir: String,
    /// 转换成功后删除源文件
    #[serde(default)]
    pub delete_source: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bg_mode: BgMode::Wallpaper,
            dim: 96,
            auto_analyze: true,
            opts: fc_core::ConvertOptions::default(),
            output_mode: OutputMode::Source,
            output_dir: String::new(),
            delete_source: false,
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
        let Some(p) = Self::file() else {
            return Self::default();
        };
        std::fs::read(p)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(dir) = Self::dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        if let Some(p) = Self::file() {
            if let Ok(s) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}
