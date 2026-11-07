//! 设备专用组 —— 全部只能解码，不进侧栏。
//!
//! 这个文件双方都不要动。ATRAC 家族的历史信息写在这里，供百科使用。

use super::{FormatSpec, Group, Kind, Patent};

pub static DEVICE: &[FormatSpec] = &[
    FormatSpec {
        id: "atrac1",
        name: "ATRAC1",
        ext: "aea",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Free,
        can_encode: false,
        primary: false,
        blurb: "初代 MD 的编码（1992）。\n码率 292kbps，只能解码。",
        args: &[],
        size_ratio: 0.09,
    },
    FormatSpec {
        id: "atrac3",
        name: "ATRAC3",
        ext: "oma",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Free,
        can_encode: false,
        primary: false,
        blurb: "索尼 1999 年的 MDLP 编码。\n只能解码（专利已过期），无法编码。",
        args: &[],
        size_ratio: 0.06,
    },
    FormatSpec {
        id: "atrac3p",
        name: "ATRAC3+",
        ext: "oma",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Expiring,
        can_encode: false,
        primary: false,
        blurb: "索尼 2003 年的编码，Hi-MD / Walkman 用。\n只能解码。同一个 .oma 扩展名有加密和不加密两种。",
        args: &[],
        size_ratio: 0.06,
    },
    FormatSpec {
        id: "atrac9",
        name: "ATRAC9",
        ext: "at9",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Active,
        can_encode: false,
        primary: false,
        blurb: "索尼 2013 年的编码，PS4 / Vita 用。\n专利还在保护期，只做识别。",
        args: &[],
        size_ratio: 0.06,
    },
];
