//! DRM 指纹识别 —— 回答"这文件为什么打不开"。
//!
//! **只识别，不解密。** 这是产品的法律红线（见 `docs/法律边界-重新评估.md`）。
//! 这个模块永远不碰密钥、不碰解密算法，只做两件事：
//!   1. 从文件头 / 扩展名 / 熵值判断它是哪家的加密
//!   2. 给一句话解释和合法出路
//!
//! 为什么单独成模块：加密格式的判定逻辑会一直长（现在 8 家，以后 15 家），
//! 放在 `ffmpeg.rs` 里会把那个文件撑爆。

use std::path::Path;

/// 识别结果
#[derive(Debug, Clone)]
pub struct Encrypted {
    /// 平台名，如 "QQ 音乐"
    pub platform: &'static str,
    /// 格式代号，如 "mflac"
    pub format: &'static str,
    /// 一句话说明 + 合法出路
    pub hint: &'static str,
    /// 0.0 ~ 1.0
    pub confidence: f32,
}

/// 已知的加密扩展名表
const EXT_TABLE: &[(&str, &str, &str, &str)] = &[
    // (扩展名, 平台, 格式代号, 说明)
    (
        "mflac",
        "QQ 音乐",
        "mflac",
        "QQ 音乐会员下载的加密 FLAC。密钥在客户端里，文件本身不含解密信息。",
    ),
    (
        "mflac0",
        "QQ 音乐",
        "mflac",
        "QQ 音乐会员下载的加密 FLAC。密钥在客户端里，文件本身不含解密信息。",
    ),
    (
        "mflac1",
        "QQ 音乐",
        "mflac",
        "QQ 音乐会员下载的加密 FLAC。密钥在客户端里，文件本身不含解密信息。",
    ),
    (
        "mgg",
        "QQ 音乐",
        "mgg",
        "QQ 音乐会员下载的加密 OGG。",
    ),
    (
        "mgg0",
        "QQ 音乐",
        "mgg",
        "QQ 音乐会员下载的加密 OGG。",
    ),
    (
        "mgg1",
        "QQ 音乐",
        "mgg",
        "QQ 音乐会员下载的加密 OGG。",
    ),
    (
        "qmc0",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密格式（QMC）。",
    ),
    (
        "qmc3",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密格式（QMC）。",
    ),
    (
        "qmcflac",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密 FLAC（QMC）。",
    ),
    (
        "qmcogg",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密 OGG（QMC）。",
    ),
    ("ncm", "网易云音乐", "ncm", "网易云音乐会员下载的加密格式（NCM）。"),
    ("kgm", "酷狗音乐", "kgm", "酷狗音乐会员下载的加密格式。"),
    ("kgma", "酷狗音乐", "kgm", "酷狗音乐会员下载的加密格式。"),
    ("kgg", "酷狗音乐", "kgm", "酷狗音乐会员下载的加密格式。"),
    ("kwm", "酷我音乐", "kwm", "酷我音乐会员下载的加密格式。"),
    ("xm", "虾米音乐", "xm", "虾米音乐的加密格式。**该平台已停止服务**，文件基本无解。"),
    ("m4p", "Apple Music", "fairplay", "Apple 的 FairPlay 加密 AAC。"),
    ("aa", "Audible", "aa", "Audible 有声书的旧加密格式。"),
    ("aax", "Audible", "aax", "Audible 有声书的加密格式。"),
];

/// 已知的正常容器魔数——命中任何一种就说明**不是**整文件加密
const MAGIC: &[(&[u8], &str)] = &[
    (b"fLaC", "flac"),
    (b"RIFF", "wav"),
    (b"FORM", "aiff"),
    (b"OggS", "ogg"),
    (b"ID3", "mp3"),
    (b"caff", "caf"),
    (b"\xFF\xFB", "mp3"),
    (b"\xFF\xF3", "mp3"),
    (b"\xFF\xF2", "mp3"),
    (b"MAC ", "ape"),
    (b"wvpk", "wavpack"),
    (b"TTA1", "tta"),
];

fn read_head(path: &Path, n: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf).ok()?;
    buf.truncate(got);
    Some(buf)
}

/// 采样估算熵（bits/byte）。加密数据接近 8.0，普通容器头低得多。
fn entropy(head: &[u8]) -> f32 {
    // 跳过容器头，取后面的原始数据
    let start = head.len().min(512) / 2;
    let sample = &head[start..];
    if sample.len() < 256 {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for &b in sample {
        freq[b as usize] += 1;
    }
    let n = sample.len() as f32;
    let mut h = 0.0f32;
    for &c in freq.iter() {
        if c > 0 {
            let p = c as f32 / n;
            h -= p * p.log2();
        }
    }
    h
}

/// 识别一个文件是不是加密的。不是加密文件返回 `None`。
pub fn identify(path: &Path) -> Option<Encrypted> {
    let head = read_head(path, 8192)?;
    if head.len() < 4 {
        return None;
    }

    // 1. `.oma` 要单独判：魔数一致，只有第 4 个字节区分加密与否
    if &head[0..3] == b"EA3" {
        // EA3 + 0x01 = 无 DRM（能转），EA3 + 0x21 = OpenMG 加密（不能转）
        if head.len() > 3 && head[3] == 0x21 {
            return Some(Encrypted {
                platform: "索尼",
                format: "OpenMG",
                hint: "Sony OpenMG 加密的 ATRAC 文件。密钥在当年的 MD/Walkman 设备里，\
                       官方工具 SonicStage 已停止维护。法律上不允许绕过。",
                confidence: 0.97,
            });
        }
        return None; // 无 DRM 的 OMA，正常文件
    }

    let name = path
        .file_name()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // 2. 网易云 NCM 有魔数，最可靠
    if head.starts_with(b"CTENFDAM") {
        return Some(Encrypted {
            platform: "网易云音乐",
            format: "ncm",
            hint: "网易云音乐会员下载的 NCM 加密文件。密钥在客户端，文件本身不含解密信息。",
            confidence: 0.99,
        });
    }

    // 3. 文件名/扩展名命中。
    //    注意用整个文件名匹配：QQ 下载的文件常被改名成 `xxx.mflac1.flac`，
    //    只看最后一段扩展名（flac）会漏掉。
    if let Some((hit, platform, format, hint)) = EXT_TABLE
        .iter()
        .find(|(e, ..)| *e == ext || name.contains(e))
    {
        // 有已知容器魔数就不算加密（比如改名过的普通 flac）
        if MAGIC.iter().any(|(m, _)| head.starts_with(m)) {
            return None;
        }
        // 扩展名直接命中比"文件名里出现过这个字"更可靠
        let by_ext = *hit == ext;
        let mut confidence: f32 = if by_ext { 0.95 } else { 0.85 };
        // 内容是高熵随机数据 → 再加一点把握
        if entropy(&head) > 7.5 {
            confidence = (confidence + 0.04).min(0.99);
        }
        return Some(Encrypted {
            platform,
            format,
            hint,
            confidence,
        });
    }

    // 4. 兜底：扩展名是音频，但魔数认不出、熵又极高 → 未知加密
    let looks_audio = matches!(
        ext.as_str(),
        "flac" | "mp3" | "ogg" | "m4a" | "aac" | "wav" | "ape"
    );
    if looks_audio && !MAGIC.iter().any(|(m, _)| head.starts_with(m)) {
        let h = entropy(&head);
        // 熵只用来调置信度，不作为门槛——真实加密文件采样到的熵可能不到 7.85
        if h > 7.0 {
            return Some(Encrypted {
                platform: "未知",
                format: "unknown",
                hint: "文件内容看起来是加密的，但不在已知格式库里。\
                       可能来自某个音乐平台的私有加密，也可能文件本身损坏。",
                confidence: if h > 7.85 { 0.75 } else { 0.55 },
            });
        }
    }

    None
}

/// 一句话简报，给命令行和日志用
impl Encrypted {
    pub fn summary(&self) -> String {
        format!(
            "{} · {}  （置信度 {:.0}%）",
            self.platform,
            self.format,
            self.confidence * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn ncm_magic() {
        let mut b = b"CTENFDAM".to_vec();
        b.extend_from_slice(&[0u8; 2048]);
        let p = tmp("t.ncm", &b);
        let r = identify(&p).expect("应该识别出 NCM");
        assert_eq!(r.platform, "网易云音乐");
        assert!(r.confidence > 0.9);
    }

    #[test]
    fn oma_openmg() {
        let mut b = b"EA3\x21".to_vec();
        b.extend_from_slice(&[0x5a; 2048]);
        let p = tmp("t.oma", &b);
        let r = identify(&p).expect("EA3+0x21 应该识别为 OpenMG");
        assert_eq!(r.format, "OpenMG");
    }

    #[test]
    fn oma_clean_is_not_encrypted() {
        let mut b = b"EA3\x01".to_vec();
        b.extend_from_slice(&[0x5a; 2048]);
        let p = tmp("t2.oma", &b);
        assert!(identify(&p).is_none(), "EA3+0x01 是无 DRM 的，不该报加密");
    }

    #[test]
    fn normal_flac_is_not_encrypted() {
        let mut b = b"fLaC".to_vec();
        b.extend_from_slice(&[0x11; 4096]);
        let p = tmp("t.flac", &b);
        assert!(identify(&p).is_none());
    }

    #[test]
    fn renamed_flac_is_not_encrypted() {
        // 扩展名叫 .mflac，但魔数是正常 FLAC —— 说明只是改了名
        let mut b = b"fLaC".to_vec();
        b.extend_from_slice(&[0x11; 4096]);
        let p = tmp("fake.mflac", &b);
        assert!(identify(&p).is_none());
    }
}
