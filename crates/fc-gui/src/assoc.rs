//! 「打开方式」注册：让 ToneLore 出现在右键「打开方式」和系统默认应用里。
//!
//! 只写 HKCU，不碰 HKLM；**不静默抢默认**——Windows 10/11 要求用户在
//! 「设置 → 默认应用」里自己确认，这里只负责把 ProgID 和 OpenWithProgids 写好。
//!
//! 注册内容：
//!   HKCU\Software\Classes\ToneLore.Audio\shell\open\command  -> "exe" "%1"
//!   HKCU\Software\Classes\.<ext>\OpenWithProgids            -> ToneLore.Audio
//!   HKCU\Software\RegisteredApplications                   -> ToneLore
//!   HKCU\Software\ToneLore\Capabilities\FileAssociations    -> .<ext> = ToneLore.Audio

use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const PROGID: &str = "ToneLore.Audio";

/// 能当输入播的扩展名（音频 + 视频 + 主机音乐）。
const EXTS: &[&str] = &[
    "mp3", "flac", "wav", "m4a", "aac", "ogg", "oga", "opus", "wma", "ape", "tak", "wv",
    "tta", "aiff", "aif", "aifc", "caf", "w64", "dsf", "dff", "mpc", "spx", "amr", "oma",
    "at3", "aa3", "adx", "vag", "xa", "kvag", "qoa", "brstm", "bcstm", "bfstm", "dsp",
    "idsp", "vadpcm", "nsf", "nsfe", "spc", "gbs", "vgm", "vgz", "gym", "psf", "psf2",
    "usf", "2sf", "gsf", "sid", "hes", "kss", "ay", "sap", "mp4", "mkv", "webm", "mov",
    "avi", "flv", "mpg", "mpeg", "ts", "m2ts", "m4v", "3gp", "3g2", "wmv", "ogv", "mxf",
    "nut", "gxf", "dv",
];

fn reg(args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("reg.exe");
    cmd.args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().map_err(|e| crate::fmt2!("启动 reg.exe 失败：{e}", "Failed to start reg.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().next().unwrap_or(crate::tr!("reg 失败")).to_string())
    }
}

fn exe_path() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .map_err(|e| crate::fmt2!("拿不到程序路径：{e}", "Cannot get program path: {e}"))
}

/// 注册到「打开方式」。可重复调用。
pub fn register() -> Result<(), String> {
    let exe = exe_path()?;
    let base = format!(r"HKCU\Software\Classes\{PROGID}");
    reg(&["add", &base, "/ve", "/t", "REG_SZ", "/d", crate::tr!("ToneLore 音频文件"), "/f"])?;
    reg(&[
        "add",
        &format!(r"{base}\DefaultIcon"),
        "/ve",
        "/t",
        "REG_SZ",
        "/d",
        &format!("\"{exe}\",0"),
        "/f",
    ])?;
    reg(&[
        "add",
        &format!(r"{base}\shell\open\command"),
        "/ve",
        "/t",
        "REG_SZ",
        "/d",
        &format!("\"{exe}\" \"%1\""),
        "/f",
    ])?;

    for ext in EXTS {
        let key = format!(r"HKCU\Software\Classes\.{ext}\OpenWithProgids");
        // REG_NONE 空值就是「列出这个 ProgID」的标准写法
        reg(&["add", &key, "/v", PROGID, "/t", "REG_NONE", "/d", "", "/f"])?;
    }

    reg(&[
        "add",
        r"HKCU\Software\ToneLore\Capabilities",
        "/v",
        "ApplicationName",
        "/t",
        "REG_SZ",
        "/d",
        "ToneLore",
        "/f",
    ])?;
    reg(&[
        "add",
        r"HKCU\Software\ToneLore\Capabilities",
        "/v",
        "ApplicationDescription",
        "/t",
        "REG_SZ",
        "/d",
        crate::tr!("声识 ToneLore — 音频格式转换与试听"),
        "/f",
    ])?;
    for ext in EXTS {
        reg(&[
            "add",
            r"HKCU\Software\ToneLore\Capabilities\FileAssociations",
            "/v",
            &format!(".{ext}"),
            "/t",
            "REG_SZ",
            "/d",
            PROGID,
            "/f",
        ])?;
    }
    reg(&[
        "add",
        r"HKCU\Software\RegisteredApplications",
        "/v",
        "ToneLore",
        "/t",
        "REG_SZ",
        "/d",
        r"Software\ToneLore\Capabilities",
        "/f",
    ])?;
    Ok(())
}

/// 撤销注册（不动用户已经选好的默认程序，Windows 会自己回退）。
pub fn unregister() -> Result<(), String> {
    for ext in EXTS {
        let key = format!(r"HKCU\Software\Classes\.{ext}\OpenWithProgids");
        let _ = reg(&["delete", &key, "/v", PROGID, "/f"]);
    }
    let _ = reg(&["delete", r"HKCU\Software\RegisteredApplications", "/v", "ToneLore", "/f"]);
    let _ = reg(&["delete", &format!(r"HKCU\Software\Classes\{PROGID}"), "/f"]);
    let _ = reg(&["delete", r"HKCU\Software\ToneLore\Capabilities", "/f"]);
    Ok(())
}

/// 打开 Windows 的「默认应用」设置页（用户自己把 ToneLore 设为默认）。
pub fn open_default_apps() {
    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/c", "start", "", "ms-settings:defaultapps"]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.spawn();
}
