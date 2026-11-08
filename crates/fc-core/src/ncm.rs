//! NCM 容器里**没有被加密**的那一小块：封面图。
//!
//! 为什么只做封面：
//! - NCM 的音频和标签块是加密的，解它们要绕过 DRM；封面是明文 JPEG/PNG，
//!   直接存在容器里，读它不涉及任何解密。
//! - 对"被人改了名字、不知道是什么歌"的文件，看到封面往往就够认出来了。
//!
//! 结构（实测）：magic "CTENFDAM" → 2 字节 → 4 字节标签长度 → 标签（加密）→
//!   CRC → 若干字节 → **4 字节封面长度（小端）→ 封面图（明文）** → 加密的音频载荷。

use std::path::Path;

/// 从 NCM 里取出来的封面
pub struct NcmCover {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

const MAGIC: &[u8; 8] = b"CTENFDAM";
/// 封面一般在前几 MB 里，没必要整文件读进来
const SCAN_LIMIT: usize = 8 << 20;

/// 取 NCM 容器里的明文封面。不是 NCM、或找不到封面就返回 None。
pub fn cover(path: &Path) -> Option<NcmCover> {
    use std::io::Read;

    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.by_ref().take(SCAN_LIMIT as u64).read_to_end(&mut buf).ok()?;
    if buf.len() < 16 || &buf[..8] != MAGIC {
        return None;
    }

    // 找图片签名；优先取"前面 4 字节长度自洽"的那个
    let candidates = [
        (buf.windows(3).position(|w| w == [0xFF, 0xD8, 0xFF]), "jpg"),
        (
            buf.windows(8)
                .position(|w| w == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
            "png",
        ),
    ];
    for (pos, kind) in candidates {
        let Some(start) = pos else { continue };
        if start < 4 {
            continue;
        }
        let len = u32::from_le_bytes([
            buf[start - 4],
            buf[start - 3],
            buf[start - 2],
            buf[start - 1],
        ]) as usize;
        // 长度字段要自洽：不能太短，也不能超出已经读到的范围
        if len < 512 || start + len > buf.len() {
            continue;
        }
        let bytes = buf[start..start + len].to_vec();
        let (width, height) = if kind == "jpg" {
            jpeg_size(&bytes).unwrap_or((0, 0))
        } else {
            png_size(&bytes).unwrap_or((0, 0))
        };
        return Some(NcmCover {
            bytes,
            width,
            height,
        });
    }
    None
}

fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 {
        return None;
    }
    let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
    let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
    Some((w, h))
}

fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        // SOF0..SOF3 / SOF5..SOF7 / SOF9..SOF11 / SOF13..SOF15
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个最小 NCM：magic + 头 + 4 字节长度 + 一段假 JPEG
    fn fake_ncm(img: &[u8]) -> Vec<u8> {
        let mut v = MAGIC.to_vec();
        v.extend_from_slice(&[0x01, 0x70]);
        v.extend_from_slice(&0u32.to_le_bytes()); // 标签长度 0
        v.extend_from_slice(&[0u8; 8]); // CRC + 间隙
        v.extend_from_slice(&(img.len() as u32).to_le_bytes());
        v.extend_from_slice(img);
        v
    }

    fn tiny_jpeg() -> Vec<u8> {
        // SOI + SOF0(宽 4 高 3) + EOI，长度字段自洽就够了，不解码
        let mut v = vec![0xFF, 0xD8];
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x03, 0x00, 0x04, 0x01, 0x01, 0x11, 0x00]);
        v.extend_from_slice(&[0u8; 600]);
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn finds_plain_cover() {
        let img = tiny_jpeg();
        let data = fake_ncm(&img);
        let p = std::env::temp_dir().join("fc_test_cover.ncm");
        std::fs::write(&p, &data).unwrap();
        let c = cover(&p).expect("应该能找到封面");
        assert_eq!(c.bytes.len(), img.len());
        assert_eq!((c.width, c.height), (4, 3));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn rejects_non_ncm() {
        let p = std::env::temp_dir().join("fc_test_not_ncm.bin");
        std::fs::write(&p, b"not an ncm file at all").unwrap();
        assert!(cover(&p).is_none());
        let _ = std::fs::remove_file(&p);
    }
}
