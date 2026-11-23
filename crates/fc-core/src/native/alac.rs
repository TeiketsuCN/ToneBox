//! ALAC（MP4 容器） 原生解码器（待实现）。

use std::path::Path;

use anyhow::{anyhow, Result};

use super::WavInfo;

pub fn decode(_input: &Path, _output: &Path) -> Result<WavInfo> {
    Err(anyhow!("ALAC（MP4 容器） 的原生解码器尚未实现"))
}
