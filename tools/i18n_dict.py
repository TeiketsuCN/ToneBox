# 生成 i18n.rs 的英文查表（原中文 key -> English）。
# 新增界面文案时，把中文 key 和英文放这里，然后运行本脚本。
import pathlib

D = {
    # 标题栏 / 导航 / 通用
    "声箱ToneBox": "ToneBox",
    "版本 1.6.1 · 建议先备份你的文件": "v1.6.1 · back up your files first",
    "建议所有操作备份 · 慎防设备损坏 请自己负责": "Back up everything; device damage is your own risk",
    "音乐库": "Library",
    "编解码": "Encoding",
    "引擎和设备": "Engines & devices",
    "设备": "Devices",
    "蓝牙": "Bluetooth",
    "引擎": "Engine",
    "点「重新扫描」扫一遍音乐库": 'Click "Rescan" to scan the library',
    "设置": "Settings",
    "任务": "Tasks",
    "转换": "Convert",
    "输出": "Output",
    "详情": "Details",
    "播放": "Play",
    "停止": "Stop",
    "上一首": "Previous",
    "下一首": "Next",
    "音量": "Volume",
    "没有在播放": "Nothing playing",
    "取消": "Cancel",
    "关闭": "Close",
    "确定": "OK",
    "保存": "Save",
    "删除": "Delete",
    "移除": "Remove",
    "重试": "Retry",
    "退出": "Quit",
    "最小化": "Minimize",
    "最大化": "Maximize",
    "还原": "Restore",
    "置顶": "Always on top",
    "取消置顶": "Unpin",
    "重新检测": "Re-detect",
    "语言 / Language": "Language",
    "中文": "Chinese",
    "英文": "English",
    # 引擎页
    "输出设备": "Output device",
    "输入设备": "Input device",
    "没有检测到输入设备。": "No input devices detected.",
    "蓝牙配对（Windows 设置）": "Bluetooth pairing (Windows Settings)",
    "已配对设备": "Paired devices",
    "取消配对": "Unpair",
    "没有检测到蓝牙音频设备。": "No Bluetooth audio devices detected.",
    "已连接": "Connected",
    "ReplayGain 是什么？": "What is ReplayGain?",
    "当前输出": "Current output",
    "播放引擎": "Playback engine",
    "选好输出设备后点右上角「重新检测」": 'Pick an output device, then click "Re-detect" at the top right',
    "引擎信息": "Engine info",
    "输出能力": "Output capabilities",
    # 详情页
    "拖入文件后，这里显示源文件的真实参数、质量判断和解释。": "Drop a file here to see its real parameters, quality estimate and explanation.",
    "纯视频文件没有音轨，不做频谱分析和内容体检。": "Video-only files have no audio; no spectrum or health analysis.",
    "准备分析频谱…": "Preparing spectrum…",
    "未分析（自动分析已关闭）": "Not analyzed (auto analysis off)",
    "正在分析频谱…": "Analyzing spectrum…",
    "频谱分析失败": "Spectrum analysis failed",
    "质量评估": "Quality assessment",
    "推测源质量": "Estimated source quality",
    "内容异常": "Content anomaly",
    "置信度": "Confidence",
    "内容体检": "Content health",
    "加密格式": "Encrypted format",
    "容器内封面（明文）": "Cover inside container (plaintext)",
    "只读这张图，不碰加密音频": "Only reads this image; does not touch encrypted audio",
    "读不出来": "Cannot read",
    "测不出响度（样本太短或几乎全是静音）": "Cannot measure loudness (too short or nearly silent)",
    "响度预览（当前文件）": "Loudness preview (current file)",
    "视频编码": "Video codec",
    # 音乐库
    "还没有音乐库": "No library yet",
    "还没扫描过音乐库": "Library has not been scanned",
    "在文件夹中显示": "Show in folder",
    "没找到匹配的歌。": "No matching tracks.",
    "文件夹": "Folder",
    "单独添加的文件": "Individually added files",
    # 输出面板
    "预设": "Presets",
    "点一下套用这套参数；右键改名 / 删除": "Click to apply; right-click to rename / delete",
    "＋ 存为预设": "+ Save as preset",
    "把当前格式和参数存成一套，可改名，跟着「配置」一起保存": "Save current format and settings as a preset; follows the active profile",
    "这个格式没有可调参数。": "This format has no adjustable parameters.",
    "输出到": "Output to",
    "选一个目录": "Choose a folder",
    "浏览": "Browse",
    "转换后": "After conversion",
    "删了就找不回来了，谨慎。": "Deletion is permanent; be careful.",
    "歌词文件": "Lyrics file",
    "标题（当前文件）": "Title (current file)",
    "留空=用源文件标签": "Empty = use source tag",
    "音轨号": "Track number",
    "如 03": "e.g. 03",
    "艺术家（全部）": "Artist (all)",
    "专辑（全部）": "Album (all)",
    "封面（全部）": "Cover (all)",
    "选图片…": "Choose image…",
    "清除": "Clear",
    "封面只写进 FLAC/MP3/M4A。": "Cover art is only written to FLAC/MP3/M4A.",
    # 百科 / 返回
    "返回": "Back",
    "回到主界面（Esc 也行）": "Back to main view (Esc works too)",
    "格式百科": "Format encyclopedia",
    "搜格式名…": "Search format name…",
    "回到主界面，接下来转的文件都用这个格式": "Back to main view; following files use this format",
    "百科": "Encyclopedia",
    "看看每个格式到底是什么": "See what each format really is",
    "输出格式": "Output format",
    "视频保留第一条画面 + 全部音轨。": "Video keeps the first video stream + all audio tracks.",
    "只读格式：能解码，不能作为输出目标": "Read-only formats: decodable, but not output targets",
    # 设置
    "外观": "Appearance",
    "背景": "Background",
    "暗化": "Dim",
    "窗口大小": "Window size",
    "F11 切换；Esc 退出全屏": "F11 toggles; Esc exits fullscreen",
    "窗口固定尺寸：小 1160×660，中 1260×700，大 1380×760，超大 1580×880（随屏幕自动缩放）": "Fixed window sizes: S 1160×660, M 1260×700, L 1380×760, XL 1580×880 (auto-scaled to the screen)",
    "行为": "Behavior",
    "选中文件时自动分析频谱": "Analyze spectrum automatically when a file is selected",
    "关掉后不自动分析，省 CPU；每个文件约 1 秒": "Off saves CPU; about 1 second per file",
    "配置": "Profiles",
    "关于": "About",
    "依赖 / 外部工具": "Dependencies / external tools",
    "换过工具目录或版本后点一下": "Click after changing a tool folder or version",
    "文件关联": "File association",
    "CUE 分轨": "CUE splitting",
    "选择 CUE 并分轨": "Pick a CUE and split",
    "支持开发": "Support development",
    # 活动 / 通知
    "活动": "Activity",
    "把任务列表里待转换的文件都转掉": "Convert all pending files in the task list",
    "缓存": "Cache",
    "波形/分析/封面都是按需算的，关掉就没了": "Waveforms/analysis/covers are computed on demand; turning it off discards them",
    "通知": "Notifications",
    "还没有通知。转换完成或失败都会记一条。": "No notifications yet. Completed and failed conversions are logged here.",
    "系统": "System",
    # 队列
    "待转换": "Pending",
    "把你认为有音乐的文件拖进来": "Drop the files you think contain music",
    "可以一次拖多个 · 不限后缀，按文件内容识别": "Multiple files at once; any extension, identified by content",
    "视频来源：全部音轨都会保留；认不出来的会告诉你为什么": "Video sources keep all audio tracks; unrecognized files explain why",
    "选择文件": "Choose files",
    "＋ 任务": "+ Task",
    "把选中的文件按当前参数加进任务": "Add selected files to tasks with current settings",
    "还没有任务。": "No tasks yet.",
    "添加文件": "Add files",
}


def rust_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


arms = "\n".join(f'        {rust_str(k)} => {rust_str(v)},' for k, v in D.items())

src = f'''//! 极简 i18n：中文是源语言，英文查表。
//!
//! 为什么不用第三方库：界面文案是硬编码在 22 个文件里的，引入 Fluent/gettext
//! 需要先抽 key，改动比查表大得多。这里只做「原文即 key」：中文原样保留，
//! 英文模式下按 key 查英文；查不到就退回中文（不会 panic，也不会显示空洞）。
//!
//! 用法：
//!   tr!("设置")                    -> "Settings"
//!   tr!("已完成 {{}} 个文件", n)      -> format! 语义，英文模板也保留 {{}}
//!   crate::i18n::tr("设置")        -> 非宏场景

use std::sync::atomic::{{AtomicU8, Ordering}};

static LANG: AtomicU8 = AtomicU8::new(0); // 0 = zh, 1 = en

pub fn set_lang(tag: &str) {{
    let en = matches!(
        tag.trim().to_ascii_lowercase().as_str(),
        "en" | "en-us" | "en_us" | "english"
    );
    LANG.store(if en {{ 1 }} else {{ 0 }}, Ordering::Relaxed);
}}

pub fn is_en() -> bool {{
    LANG.load(Ordering::Relaxed) == 1
}}

pub fn tag() -> &'static str {{
    if is_en() {{ "en" }} else {{ "zh" }}
}}

/// 原文 → 英文。未收录的 key 原样返回（中文兜底）。
pub fn tr(key: &'static str) -> &'static str {{
    if !is_en() {{
        return key;
    }}
    match key {{
{arms}
        _ => key,
    }}
}}

/// `tr!("中文")` / `tr!("中文 {{}}", x)`。占位符顺序必须与英文模板一致。
#[macro_export]
macro_rules! tr {{
    ($s:literal) => {{
        $crate::i18n::tr($s)
    }};
    ($s:literal, $($arg:tt)*) => {{
        format!($crate::i18n::tr($s), $($arg)*)
    }};
}}
'''

out = pathlib.Path(r"C:\Users\23501\Desktop\FormatConverter-source-20261114-1922\crates\fc-gui\src\i18n.rs")
out.write_text(src, encoding="utf-8")
print("wrote", len(D), "keys")
