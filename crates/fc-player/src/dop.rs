//! DSD over PCM（DoP）打包。
//!
//! FFmpeg 对 DSF/DFF 用 `-c:a copy -f data` 输出的是**原始 1-bit 包**：
//! 立体声一个包 = 左声道半包 + 右声道半包（planar）。
//! DoP 把每 16 个 DSD 位塞进一个 24-bit PCM 样本：高 8 位是 marker
//! （0x05 / 0xFA 交替），低 16 位是 DSD 位；输出采样率 = DSD 位率 / 16，
//! 所以 DSD64 → 176400 Hz，DSD128 → 352800 Hz，DSD256 → 705600 Hz。
//!
//! 当前只做“检测 + 打包”，还没有接到播放器（下一步在独占输出里用）。

/// DSD 档位对应的 DoP PCM 采样率。认不出返回 None。
pub fn dop_rate_for_tier(tier: u32) -> Option<u32> {
    match tier {
        64 => Some(176_400),
        128 => Some(352_800),
        256 => Some(705_600),
        512 => Some(1_411_200),
        _ => None,
    }
}

/// 一个字节的位反转（DSF 是 LSB-first，DoP 要 MSB-first）。
fn reverse_bits(mut b: u8) -> u8 {
    b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
    b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
    b = (b & 0xAA) >> 1 | (b & 0x55) << 1;
    b
}

/// 把 2 个 DSD 字节变成 DoP 的 16 位字。
/// `lsb_first`：DSF 的每个字节低位在前、且字节序也是低位在前；DFF 则相反。
fn dsd_word(a: u8, b: u8, lsb_first: bool) -> u16 {
    if lsb_first {
        (reverse_bits(a) as u16) << 8 | reverse_bits(b) as u16
    } else {
        (a as u16) << 8 | b as u16
    }
}

/// 把一个 planar 立体声 DSD 包打成 DoP 的 s32le 帧（每帧 8 字节：L4 + R4）。
///
/// * `out` 追加，不清空；
/// * `marker` 在调用之间保持，保证 marker 连续交替；
/// * 返回写入了多少帧（每帧 = 左右各一个 24-bit DoP 样本）。
pub fn pack_stereo_packet(
    packet: &[u8],
    lsb_first: bool,
    marker: &mut bool,
    out: &mut Vec<u8>,
) -> usize {
    if packet.len() < 4 || packet.len() % 2 != 0 {
        return 0;
    }
    let half = packet.len() / 2;
    let (left, right) = packet.split_at(half);
    let frames = half / 2;
    out.reserve(frames * 8);
    for i in 0..frames {
        let m = if *marker { 0xFAu32 } else { 0x05u32 };
        *marker = !*marker;
        let l = dsd_word(left[i * 2], left[i * 2 + 1], lsb_first) as u32;
        let r = dsd_word(right[i * 2], right[i * 2 + 1], lsb_first) as u32;
        // 24-bit 样本左对齐进 32-bit 容器：marker<<24 | dsd16<<8
        let lw = (m << 24) | (l << 8);
        let rw = (m << 24) | (r << 8);
        out.extend_from_slice(&lw.to_le_bytes());
        out.extend_from_slice(&rw.to_le_bytes());
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_to_rate() {
        assert_eq!(dop_rate_for_tier(64), Some(176_400));
        assert_eq!(dop_rate_for_tier(128), Some(352_800));
        assert_eq!(dop_rate_for_tier(256), Some(705_600));
        assert_eq!(dop_rate_for_tier(0), None);
    }

    #[test]
    fn reverse_bits_works() {
        assert_eq!(reverse_bits(0b1000_0000), 0b0000_0001);
        assert_eq!(reverse_bits(0b1010_0101), 0b1010_0101);
    }

    #[test]
    fn pack_is_planar_and_marks() {
        // 左 00 FF，右 0F F0；LSB-first 会先反转每个字节
        let mut pkt = vec![0x00, 0xFF, 0x0F, 0xF0];
        let mut marker = false;
        let mut out = Vec::new();
        let frames = pack_stereo_packet(&pkt, false, &mut marker, &mut out);
        assert_eq!(frames, 1);
        assert_eq!(out.len(), 8);
        // 第一帧 marker=0x05；L word=0x00FF -> 0x05 00 FF 00
        assert_eq!(&out[0..4], &[0x00, 0xFF, 0x00, 0x05]);
        assert_eq!(&out[4..8], &[0x00, 0xF0, 0x0F, 0x05]);
        // 第二帧 marker 应该是 0xFA
        let mut out2 = Vec::new();
        let frames2 = pack_stereo_packet(&pkt, false, &mut marker, &mut out2);
        assert_eq!(frames2, 1);
        assert_eq!(out2[3], 0xFA);
        assert_eq!(out2[7], 0xFA);
    }

    #[test]
    fn lsb_first_reverses() {
        // DSF 0x80 -> 反转 0x01；两个字节组成 0x0102
        let pkt = vec![0x80, 0x40, 0, 0];
        let mut marker = false;
        let mut out = Vec::new();
        pack_stereo_packet(&pkt, true, &mut marker, &mut out);
        // L word = 0x0102 -> bytes 02 01 00 05
        assert_eq!(&out[0..4], &[0x00, 0x02, 0x01, 0x05]);
    }

    #[test]
    fn rejects_odd_packets() {
        let mut m = false;
        let mut o = Vec::new();
        assert_eq!(pack_stereo_packet(&[0u8; 5], true, &mut m, &mut o), 0);
    }
}
