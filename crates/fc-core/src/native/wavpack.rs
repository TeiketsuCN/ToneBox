//! WavPack 原生解码器（待实现）。

use std::path::Path;

use anyhow::{anyhow, Result};

use super::WavInfo;

pub fn decode(_input: &Path, _output: &Path) -> Result<WavInfo> {
    Err(anyhow!("WavPack 的原生解码器尚未实现"))
}
