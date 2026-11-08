//! QQ 音乐 QMC 容器（mflac / mgg）的**明文尾巴**。
//!
//! 整个音频载荷都是加密的，封面也在加密区里，所以这边取不到封面；
//! 但文件尾部有一段未加密的固定结构，里面用 UTF-16LE 记着两样东西：
//!   - 一个 12 位的歌曲/密钥 ID（如 `000GjGZr32Abd`）
//!   - 服务器上的原始文件名（如 `F0M0002rWMtO1vljWE.mflac`）
//!
//! 对"被人改了名字、不知道是什么"的文件，这两条至少能对上号。

use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct QmcFooter {
    /// 12 位字母数字 ID（QQ 音乐内部编号）
    pub id: Option<String>,
    /// 原始文件名（服务器上的名字，不一定等于歌名）
    pub name: Option<String>,
}

/// 从 QMC 文件尾部解析明文信息。不是 QMC 或解析不到就返回 None。
pub fn footer(path: &Path) -> Option<QmcFooter> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len < 512 {
        return None;
    }
    let take = len.min(2048) as usize;
    f.seek(SeekFrom::Start(len - take as u64)).ok()?;
    let mut buf = Vec::new();
    f.take(take as u64).read_to_end(&mut buf).ok()?;

    // 尾部标记 "musicex" 是 QMC 容器的结尾
    if !buf.windows(7).any(|w| w == b"musicex") {
        return None;
    }

    let mut out = QmcFooter::default();
    for s in utf16_strings(&buf) {
        // 真实 ID 是 10~16 位、字母数字混合（例如 000GjGZr32Abd）；
        // 全数字/全零的填充串不算
        let is_id = (10..=16).contains(&s.len())
            && s.chars().all(|c| c.is_ascii_alphanumeric())
            && s.chars().any(|c| c.is_ascii_digit())
            && s.chars().any(|c| c.is_ascii_alphabetic());
        if is_id && out.id.is_none() {
            out.id = Some(s);
            continue;
        }
        let lower = s.to_ascii_lowercase();
        if (lower.ends_with(".mflac") || lower.ends_with(".mgg") || lower.ends_with(".mflac0")
            || lower.ends_with(".mgg0") || lower.ends_with(".mflac1") || lower.ends_with(".mgg1"))
            && out.name.is_none()
        {
            out.name = Some(s);
        }
    }
    if out.id.is_none() && out.name.is_none() {
        return None;
    }
    Some(out)
}

/// 从原始字节里捡出「ASCII + NUL」交替的 UTF-16LE 字符串（长度 ≥ 4）
fn utf16_strings(buf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i + 1 < buf.len() {
        let c = buf[i];
        let z = buf[i + 1];
        if z == 0 && (0x20..0x7f).contains(&c) {
            cur.push(c as char);
            i += 2;
        } else {
            if cur.len() >= 4 {
                out.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
            i += 1;
        }
    }
    if cur.len() >= 4 {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_qmc(id: &str, name: &str) -> Vec<u8> {
        let mut v = vec![0x5au8; 600];
        // 尾部结构：一堆填充 + UTF-16LE 字符串 + musicex
        for s in [id, "000000000000", name] {
            for ch in s.encode_utf16() {
                v.extend_from_slice(&ch.to_le_bytes());
            }
            v.extend_from_slice(&[0, 0, 0, 0]);
        }
        v.extend_from_slice(b"musicex\0");
        v
    }

    #[test]
    fn reads_footer_strings() {
        let data = fake_qmc("000GjGZr32Abd", "F0M0002rWMtO1vljWE.mflac");
        let p = std::env::temp_dir().join("fc_test_qmc.mflac");
        std::fs::write(&p, &data).unwrap();
        let f = footer(&p).expect("应该解析出尾部信息");
        assert_eq!(f.id.as_deref(), Some("000GjGZr32Abd"));
        assert_eq!(f.name.as_deref(), Some("F0M0002rWMtO1vljWE.mflac"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn rejects_without_marker() {
        let p = std::env::temp_dir().join("fc_test_not_qmc.bin");
        std::fs::write(&p, vec![0u8; 700]).unwrap();
        assert!(footer(&p).is_none());
        let _ = std::fs::remove_file(&p);
    }
}
