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
    /// 这是什么
    pub hint: &'static str,
    /// 那怎么办——只说合规的路，绝不给破解指引
    pub advice: &'static str,
    /// 0.0 ~ 1.0
    pub confidence: f32,
    /// 这个置信度是怎么来的：每条带加减分，界面按顺序列出来
    pub evidence: Vec<Evidence>,
}

/// 一条识别证据：`delta` 表示这一条给的总分（正数=加分；负数=扣分）
#[derive(Debug, Clone)]
pub struct Evidence {
    pub delta: f32,
    pub reason: String,
}

/// 已知的加密扩展名表
const EXT_TABLE: &[(&str, &str, &str, &str, &str)] = &[
    // (扩展名, 平台, 格式代号, 说明, 合法出路)
    (
        "mflac",
        "QQ 音乐",
        "mflac",
        "QQ 音乐会员下载的加密 FLAC。密钥在客户端里，文件本身不含解密信息。",
        ADVICE_CLIENT,
    ),
    (
        "mflac0",
        "QQ 音乐",
        "mflac",
        "QQ 音乐会员下载的加密 FLAC。密钥在客户端里，文件本身不含解密信息。",
        ADVICE_CLIENT,
    ),
    (
        "mflac1",
        "QQ 音乐",
        "mflac",
        "QQ 音乐会员下载的加密 FLAC。密钥在客户端里，文件本身不含解密信息。",
        ADVICE_CLIENT,
    ),
    (
        "mgg",
        "QQ 音乐",
        "mgg",
        "QQ 音乐会员下载的加密 OGG。",
        ADVICE_CLIENT,
    ),
    (
        "mgg0",
        "QQ 音乐",
        "mgg",
        "QQ 音乐会员下载的加密 OGG。",
        ADVICE_CLIENT,
    ),
    (
        "mgg1",
        "QQ 音乐",
        "mgg",
        "QQ 音乐会员下载的加密 OGG。",
        ADVICE_CLIENT,
    ),
    (
        "qmc0",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密格式（QMC）。",
        ADVICE_CLIENT,
    ),
    (
        "qmc3",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密格式（QMC）。",
        ADVICE_CLIENT,
    ),
    (
        "qmcflac",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密 FLAC（QMC）。",
        ADVICE_CLIENT,
    ),
    (
        "qmcogg",
        "QQ 音乐",
        "qmc",
        "QQ 音乐早期加密 OGG（QMC）。",
        ADVICE_CLIENT,
    ),
    ("ncm", "网易云音乐", "ncm", "网易云音乐会员下载的加密格式（NCM）。", ADVICE_CLIENT),
    ("kgm", "酷狗音乐", "kgm", "酷狗音乐会员下载的加密格式。", ADVICE_CLIENT),
    ("kgma", "酷狗音乐", "kgm", "酷狗音乐会员下载的加密格式。", ADVICE_CLIENT),
    ("kgg", "酷狗音乐", "kgm", "酷狗音乐会员下载的加密格式。", ADVICE_CLIENT),
    ("kwm", "酷我音乐", "kwm", "酷我音乐会员下载的加密格式。", ADVICE_CLIENT),
    (
        "xm",
        "虾米音乐",
        "xm",
        "虾米音乐的加密格式。",
        "该平台 2021 年已停止服务，服务器端密钥随之下线。这类文件目前没有合规的打开方式。",
    ),
    (
        "m4p",
        "Apple Music",
        "fairplay",
        "Apple 的 FairPlay 加密 AAC。",
        "在 Apple Music 客户端内播放即可。想拿到不加密的文件，只能买 iTunes Store 的无 DRM 版本。",
    ),
    ("aa", "Audible", "aa", "Audible 有声书的旧加密格式。", ADVICE_APP),
    ("aax", "Audible", "aax", "Audible 有声书的加密格式。", ADVICE_APP),
];

/// 音乐平台通用出路
const ADVICE_CLIENT: &str = "合规的做法只有两条：在它的官方客户端里播放；\
或者从正规渠道重新买一份无损（数字专辑 / 实体 CD）。\
技术上没有解开它的合法途径，本工具也不会去做。";

/// 有专用 App 的平台
const ADVICE_APP: &str = "在它的官方 App 里播放。\
想把内容拿到别的地方用，只能看这个平台有没有提供导出功能。";

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
                       官方工具 SonicStage 已停止维护。",
                advice: "唯一合路是把 MD 机的光纤/模拟输出接到声卡，实时回录成 WAV。\
                         音质取决于回录链路，但这是不碰 DRM 的唯一办法——很多老玩家现在就是这么做的。",
                confidence: 0.97,
                evidence: vec![
                    Evidence {
                        delta: 80.0,
                        reason: "文件头 'EA3' 魔数匹配（索尼 OMA/ATRAC 容器）".to_string(),
                    },
                    Evidence {
                        delta: 17.0,
                        reason: "第 4 字节 = 0x21，是 OpenMG 的加密标记（0x01 才是无 DRM）"
                            .to_string(),
                    },
                ],
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
        let h = entropy(&head);
        let mut evidence = vec![Evidence {
            delta: 70.0,
            reason: "文件头 8 字节 = 'CTENFDAM'，是网易云 NCM 容器的固定魔数".to_string(),
        }];
        // 封面是明文存的：能读出来，说明这确实是一个带专辑封面的音乐文件
        match crate::ncm::cover(path) {
            Some(c) if c.width > 0 && c.height > 0 => {
                evidence.push(Evidence {
                    delta: 15.0,
                    reason: format!(
                        "容器内封面是明文图片（{}×{}），说明这确实是一首带专辑封面的音乐",
                        c.width, c.height
                    ),
                });
            }
            Some(_) => {
                evidence.push(Evidence {
                    delta: 10.0,
                    reason: "容器内有一张明文封面图，说明这确实是一首音乐".to_string(),
                });
            }
            None => {}
        }
        evidence.push(Evidence {
            delta: 10.0,
            reason: format!("头部熵 {h:.2}（NCM 的元数据段之后就是加密载荷）"),
        });
        evidence.push(Evidence {
            delta: -5.0,
            reason: "音频和标签仍然是加密的，本工具不解密：能认出是哪首，但验证不了内容和标签是否对应"
                .to_string(),
        });
        let score: f32 = evidence.iter().map(|e| e.delta).sum();
        return Some(Encrypted {
            platform: "网易云音乐",
            format: "ncm",
            hint: "网易云音乐会员下载的 NCM 加密文件。密钥在客户端，文件本身不含解密信息。",
            advice: ADVICE_CLIENT,
            confidence: (score / 100.0).clamp(0.0, 0.99),
            evidence,
        });
    }

    // 2.5 QQ 音乐的 QMC 容器：头部是加密的（没有魔数），但文件尾有 "musicex" 标记。
    if let Some(tail) = read_tail(path, 2048) {
        if tail
            .windows(7)
            .any(|w| w == b"musicex")
        {
            let h = entropy(&head);
            return Some(Encrypted {
                platform: "QQ 音乐",
                format: "QMC（mflac / mgg）",
                hint: "QQ 音乐会员下载的 QMC 加密文件。密钥在客户端，文件本身不含解密信息。",
                advice: ADVICE_CLIENT,
                confidence: 0.90,
                evidence: vec![
                    Evidence {
                        delta: 60.0,
                        reason: "文件末尾 2 KB 内有 'musicex' 标记，是 QMC 容器的结尾结构"
                            .to_string(),
                    },
                    Evidence {
                        delta: 30.0,
                        reason: format!("文件头不是已知音频魔数，头部熵 {h:.2}（加密载荷）"),
                    },
                ],
            });
        }
    }

    // 3. 文件名/扩展名命中。
    //    注意用整个文件名匹配：QQ 下载的文件常被改名成 `xxx.mflac1.flac`，
    //    只看最后一段扩展名（flac）会漏掉。
    if let Some((hit, platform, format, hint, advice)) = EXT_TABLE
        .iter()
        .find(|(e, ..)| *e == ext || name.contains(e))
    {
        // 有已知容器魔数就不算加密（比如改名过的普通 flac）
        if MAGIC.iter().any(|(m, _)| head.starts_with(m)) {
            return None;
        }
        // 只凭名字，**不给高置信度**：扩展名可以随便改，必须降级并说明
        let by_ext = *hit == ext;
        let h = entropy(&head);
        let mut evidence = vec![
            Evidence {
                delta: if by_ext { 30.0 } else { 20.0 },
                reason: format!(
                    "{}里出现了 '{}'（名字可以随便改，只算弱证据）",
                    if by_ext { "扩展名" } else { "文件名" },
                    hit
                ),
            },
            Evidence {
                delta: 15.0,
                reason: "文件头不是任何已知音频容器魔数".to_string(),
            },
        ];
        if h > 7.5 {
            evidence.push(Evidence {
                delta: 8.0,
                reason: format!("头部熵 {h:.2}，内容确实不像未压缩音频"),
            });
        } else {
            evidence.push(Evidence {
                delta: 0.0,
                reason: format!("头部熵 {h:.2}，不够高，可能只是把别的文件改了名"),
            });
        }
        let score: f32 = evidence.iter().map(|e| e.delta).sum();
        return Some(Encrypted {
            platform,
            format,
            hint,
            advice,
            confidence: (score / 100.0).clamp(0.0, 0.99),
            evidence,
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
                advice: "先看它从哪个平台下载的，再按那个平台的办法处理。\
                         如果是从网盘转来的，原文件名里通常有线索。",
                confidence: if h > 7.85 { 0.75 } else { 0.55 },
                evidence: vec![
                    Evidence {
                        delta: 40.0,
                        reason: "扩展名是音频，但文件头不是任何已知音频魔数".to_string(),
                    },
                    Evidence {
                        delta: 20.0,
                        reason: format!("头部熵 {h:.2}（越高越像加密/压缩数据）"),
                    },
                    Evidence {
                        delta: 0.0,
                        reason: "没有命中已知平台的魔数或尾部标记，只能说「疑似加密」".to_string(),
                    },
                ],
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

    /// 两段完整说明：是什么 + 怎么办
    pub fn full_text(&self) -> String {
        format!("{}\n\n{}", self.hint, self.advice)
    }
}

/// 读文件末尾 N 字节（QMC 的标记在尾部，头部是加密的）
fn read_tail(path: &Path, n: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(n as u64);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.take((len - start).min(u64::MAX)).read_to_end(&mut buf).ok()?;
    Some(buf)
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
        // 证据求和：魔数 70 + 高熵 10 - 音频仍加密 5 = 75（这个样本没有封面）
        assert!((r.confidence - 0.75).abs() < 0.01, "confidence = {}", r.confidence);
        assert_eq!(r.evidence.iter().map(|e| e.delta).sum::<f32>(), 75.0);
    }

    #[test]
    fn ncm_with_cover_scores_higher() {
        // 带一张明文封面时，多一条"确实是音乐"的加分
        let mut b = b"CTENFDAM".to_vec();
        b.extend_from_slice(&[0x01, 0x70]);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&[0u8; 8]);
        let mut jpg = vec![0xFF, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x40, 0x00, 0x40, 0x01, 0x01, 0x11, 0x00]);
        jpg.extend_from_slice(&[0u8; 600]);
        jpg.extend_from_slice(&[0xFF, 0xD9]);
        b.extend_from_slice(&(jpg.len() as u32).to_le_bytes());
        b.extend_from_slice(&jpg);
        let p = tmp("t_cover.ncm", &b);
        let r = identify(&p).expect("应该识别出带封面的 NCM");
        assert!((r.confidence - 0.90).abs() < 0.01, "confidence = {}", r.confidence);
        std::fs::remove_file(&p).ok();
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
