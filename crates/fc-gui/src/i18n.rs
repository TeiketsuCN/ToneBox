//! 极简 i18n：中文是源语言，英文查表。
//!
//! 为什么不用第三方库：界面文案是硬编码在 22 个文件里的，引入 Fluent/gettext
//! 需要先抽 key，改动比查表大得多。这里只做「原文即 key」：中文原样保留，
//! 英文模式下按 key 查英文；查不到就退回中文（不会 panic，也不会显示空洞）。
//!
//! 用法：
//!   tr!("设置")                    -> "Settings"
//!   tr!("已完成 {} 个文件", n)      -> format! 语义，英文模板也保留 {}
//!   crate::i18n::tr("设置")        -> 非宏场景

use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0); // 0 = zh, 1 = en

pub fn set_lang(tag: &str) {
    let en = matches!(
        tag.trim().to_ascii_lowercase().as_str(),
        "en" | "en-us" | "en_us" | "english"
    );
    LANG.store(if en { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn is_en() -> bool {
    LANG.load(Ordering::Relaxed) == 1
}

pub fn tag() -> &'static str {
    if is_en() {
        "en"
    } else {
        "zh"
    }
}

/// 原文 → 英文。未收录的 key 原样返回（中文兜底）。
pub fn tr(key: &'static str) -> &'static str {
    if !is_en() {
        return key;
    }
    match key {
        // ── 标题栏 / 导航 ──
        "版本 1.6.1 · 建议先备份你的文件" => "v1.6.1 · back up your files first",
        "建议所有操作备份 · 慎防设备损坏 请自己负责" => "Back up everything; device damage is your own risk",
        "设备" => "Devices",
        "蓝牙" => "Bluetooth",
        "引擎" => "Engine",
        "点「重新扫描」扫一遍音乐库" => "Click \"Rescan\" to scan the library",
        "声箱ToneBox" => "ToneBox",
        "设置" => "Settings",
        "任务" => "Tasks",
        "转换" => "Convert",
        "音乐库" => "Library",
        "编解码" => "Encoding",
        "引擎和设备" => "Engines & devices",
        "输出格式" => "Output format",
        "输出" => "Output",
        "详情" => "Details",
        "队列" => "Queue",
        "添加文件" => "Add files",
        "清空" => "Clear",
        "开始" => "Start",
        "暂停" => "Pause",
        "取消" => "Cancel",
        "关闭" => "Close",
        "确定" => "OK",
        "保存" => "Save",
        "删除" => "Delete",
        "重试" => "Retry",
        "退出" => "Quit",
        "最小化" => "Minimize",
        "最大化" => "Maximize",
        "还原" => "Restore",
        "置顶" => "Always on top",
        "取消置顶" => "Unpin",
        "没有在播放" => "Nothing playing",
        "播放" => "Play",
        "停止" => "Stop",
        "上一首" => "Previous",
        "下一首" => "Next",
        "音量" => "Volume",
        // ── 队列 ──
        "把文件拖进来，或点「添加文件」" => "Drop files here, or click \"Add files\"",
        "还没有文件" => "No files yet",
        "在文件夹中显示" => "Show in folder",
        "移除" => "Remove",
        "源文件同目录" => "Next to source",
        "指定目录" => "Custom folder",
        "保留源文件" => "Keep source",
        "删除源文件" => "Delete source",
        "生成" => "Create",
        "不生成" => "None",
        "写入标签" => "Write tags",
        "自动" => "Auto",
        // ── 状态 / 通用 ──
        "等待中" => "Pending",
        "进行中" => "Running",
        "已完成" => "Done",
        "失败" => "Failed",
        "完成" => "Done",
        "正在处理" => "Processing",
        "个文件" => "files",
        "1 个文件" => "1 file",
        "重新检测" => "Re-detect",
        "依赖 / 外部工具" => "Dependencies / external tools",
        "找到" => "Found",
        "未找到" => "Not found",
        "可选" => "optional",
        "已找到" => "Found",
        "语言" => "Language",
        "中文" => "Chinese",
        "英文" => "English",
        "文件关联" => "File association",
        "注册到打开方式" => "Register in \"Open with\"",
        "移除注册" => "Remove registration",
        "打开默认应用设置" => "Open default apps",
        "CUE 分轨" => "CUE splitting",
        "选择 CUE 并分轨" => "Pick a CUE and split",
        "正在分轨…" => "Splitting…",
        "分轨完成" => "Split finished",
        "分轨失败" => "Split failed",
        "设置 → 依赖 / 外部工具 → 重新检测" => "Settings → Dependencies → Re-detect",
        "支持开发" => "Support development",
        // ── 提示语 ──
        "加密文件：本工具只识别，不解码也不播放。" => {
            "Encrypted file: this tool only identifies it; no decoding or playback."
        }
        "找不到 ffmpeg，无法试听" => "ffmpeg not found; cannot preview",
        "转换前请备份原文件" => "Back up originals before converting",
        "建议先备份你的文件" => "Back up your files first",
        _ => key,
    }
}

/// `tr!("中文")` / `tr!("中文 {}", x)`。占位符顺序必须与英文模板一致。
#[macro_export]
macro_rules! tr {
    ($s:literal) => {
        $crate::i18n::tr($s)
    };
    ($s:literal, $($arg:tt)*) => {
        format!($crate::i18n::tr($s), $($arg)*)
    };
}
