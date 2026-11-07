//! 格式百科长文案。
//!
//! 为什么不塞进 `FormatSpec`：那要给 29 个条目全部加字段，一次改动横跨
//! `lossless.rs` / `lossy.rs` / `device.rs` 三个文件、两条人的分工线。
//! 单独一张 `id -> 长文` 的表，谁写内容谁就只改这个文件。
//!
//! 文案要求：80~150 字，写清「是什么 / 什么场合遇到 / 坑在哪 / 和邻居怎么选」。

/// 查一个格式的长文案。没有就返回 `None`，界面回退到 `FormatSpec.blurb`。
pub fn long_blurb(id: &str) -> Option<&'static str> {
    TABLE.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

/// 已经写了长文案的格式数量（界面用来显示进度）
pub fn covered() -> usize {
    TABLE.len()
}

static TABLE: &[(&str, &str)] = &[
    (
        "flac",
        "2001 年由 Josh Cooley 发布的自由无损压缩格式，至今仍是开源无损的事实标准。\
         编码后约为 WAV 的 55~65%，解码还原与原始 PCM 逐比特一致。\
         没有专利、没有授权费，几乎所有软件和硬件都支持。\
         需要无损归档、又不想被 Apple 生态绑住时，FLAC 是首选；\
         要极致兼容老播放器才考虑 WAV，要 Apple 生态就用 ALAC。",
    ),
    (
        "speex",
        "2002 年 Xiph.Org 的开源语音编码，专利免费。专为 8/16/32 kHz 的人声设计，\
         码率 2~44 kbps。坑在于它根本不是音乐编码器——频率上限只有 8 kHz（16 kHz 采样），\
         编出来会带明显金属感和空洞感。和 AMR 比它开源跨平台，和 Opus 比它已被完全取代。\
         只有在需要极小体积存人声、且目标设备明确支持 Speex 时才用它。",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_hits_and_misses() {
        assert!(long_blurb("flac").is_some());
        assert!(long_blurb("不存在的格式").is_none());
    }

    #[test]
    fn no_duplicate_ids() {
        let mut seen = std::collections::HashSet::new();
        for (id, _) in TABLE {
            assert!(seen.insert(*id), "wiki 表里 id 重复: {id}");
        }
    }

    #[test]
    fn every_entry_points_to_real_format() {
        for (id, _) in TABLE {
            assert!(
                crate::format::by_id(id).is_some(),
                "wiki 里写了不存在的格式: {id}"
            );
        }
    }
}
