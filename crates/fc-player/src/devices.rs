//! 音频设备枚举（Windows / WASAPI）。
//!
//! 只做"看见设备"，不做独占输出——输出走 `player.rs`。
//! 为什么要自己枚举而不是用 cpal 的 device 列表：
//! cpal 只给名字，判断"这是耳机还是蓝牙"需要更多信息；
//! WASAPI 能拿到设备格式、默认设备、描述，耳机/蓝牙再按名字和描述归类。
//!
//! 蓝牙设备的判定说明：Windows 的端点 ID 里没有传输类型，
//! 这里按设备名/描述里的关键字（Bluetooth / 蓝牙 / 常见 TWS 型号）判断。
//! 判不出来时宁可不标"蓝牙"，也不乱标。

use anyhow::Result;

#[cfg(windows)]
use std::sync::Once;
#[cfg(windows)]
use wasapi::{DeviceEnumerator, DeviceState, Direction};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    Output,
    Input,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Speakers,
    Headphones,
    Headset,
    Microphone,
    Hdmi,
    /// 主板/声卡的数字输出（S/PDIF、光纤、同轴）
    Digital,
    Usb,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Speakers => "扬声器",
            Kind::Headphones => "耳机",
            Kind::Headset => "耳麦",
            Kind::Microphone => "麦克风",
            Kind::Hdmi => "HDMI/显示音频",
            Kind::Digital => "数字输出",
            Kind::Usb => "USB 音频",
            Kind::Other => "其他",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioDevice {
    /// WASAPI 端点 ID（`{0.0.0.00000000}.{guid}`），选中设备靠它
    pub id: String,
    /// 给用户看的名字（Windows 的"友好名"）
    pub name: String,
    /// 驱动描述，用来辅助判断耳机/蓝牙
    pub description: String,
    pub flow: Flow,
    pub kind: Kind,
    /// 名字/描述里看出是蓝牙（没有可靠传输类型时的近似判断）
    pub is_bluetooth: bool,
    /// 名字/描述里看出是 Steam 串流设备
    pub is_steam: bool,
    /// 名字/描述里看出是 Realtek（瑞鼎）板载声卡
    pub is_realtek: bool,
    /// 名字/描述里看出是 NVIDIA 显卡音频（HDMI/DP）
    pub is_nvidia: bool,
    /// 名字/描述里看出是摄像头（带麦的 webcam 也会标）
    pub is_camera: bool,
    pub is_default: bool,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
    /// "32-bit float · 48 kHz · 2ch" 这种
    pub format_label: String,
}

impl AudioDevice {
    /// 队列/页面里的一行摘要
    pub fn detail(&self) -> String {
        self.format_label.clone()
    }
}

/// 一只蓝牙耳机/音箱（可能同时有 Stereo 和 Hands-Free 两个端点）
#[derive(Clone, Debug)]
pub struct BtHeadset {
    /// 去掉 "Stereo" / "Hands-Free AG Audio" 后缀后的名字
    pub name: String,
    pub endpoints: Vec<AudioDevice>,
    pub profiles: Vec<BtProfile>,
}

/// 蓝牙协议/用途。`active` = 当前有对应端点，正在使用。
#[derive(Clone, Copy, Debug)]
pub struct BtProfile {
    pub name: &'static str,
    pub role: &'static str,
    pub active: bool,
}

/// 把蓝牙端点按"同一只耳机"分组，并推断正在使用的协议。
///
/// Windows 不公开 A2DP 实际编解码（SBC/AAC/aptX/LDAC），这里只能从端点名推断
/// 蓝牙**协议**：立体声输出 = A2DP（+ AVRCP 遥控），免提端点 = HFP/HSP。
pub fn bluetooth_groups(devices: &[AudioDevice]) -> Vec<BtHeadset> {
    let mut groups: Vec<BtHeadset> = Vec::new();
    for d in devices.iter().filter(|d| d.is_bluetooth) {
        let base = bt_base_name(&d.name);
        if let Some(g) = groups.iter_mut().find(|g| g.name == base) {
            g.endpoints.push(d.clone());
        } else {
            groups.push(BtHeadset {
                name: base,
                endpoints: vec![d.clone()],
                profiles: Vec::new(),
            });
        }
    }
    for g in &mut groups {
        g.profiles = bt_profiles(&g.endpoints);
    }
    groups
}

fn bt_base_name(name: &str) -> String {
    let mut n = name.to_string();
    for suffix in [
        " Hands-Free AG Audio",
        " Hands-Free",
        " AG Audio",
        " Stereo",
    ] {
        n = n.replace(suffix, "");
    }
    // 去掉尾部空括号/空格：「耳机 (WH-1000XM4 )」→「耳机 (WH-1000XM4)」
    while n.contains(" )") {
        n = n.replace(" )", ")");
    }
    n.trim().to_string()
}

fn bt_profiles(endpoints: &[AudioDevice]) -> Vec<BtProfile> {
    let names = endpoints
        .iter()
        .map(|d| d.name.to_lowercase())
        .collect::<Vec<_>>()
        .join(" | ");
    let a2dp = endpoints.iter().any(|d| {
        let n = d.name.to_lowercase();
        d.flow == Flow::Output
            && (n.contains("stereo")
                || n.contains("headphone")
                || n.contains("a2dp")
                || (n.contains("耳机")
                    && !n.contains("handsfree")
                    && !n.contains("hands-free")))
    });
    let hfp = names.contains("hands-free")
        || names.contains("handsfree")
        || names.contains("headset")
        || names.contains("ag audio")
        || endpoints.iter().any(|d| d.flow == Flow::Input);
    let le = names.contains("le audio") || names.contains("le-audio");
    vec![
        BtProfile {
            name: "A2DP",
            role: "立体声音乐（高带宽）",
            active: a2dp,
        },
        BtProfile {
            name: "AVRCP",
            role: "播放/暂停/音量遥控",
            active: a2dp,
        },
        BtProfile {
            name: "HFP",
            role: "免提 + 麦克风（会降音质）",
            active: hfp,
        },
        BtProfile {
            name: "HSP",
            role: "老式免提（兼容备用）",
            active: false,
        },
        BtProfile {
            name: "LE Audio",
            role: "低功耗新协议",
            active: le,
        },
    ]
}

/// 枚举所有**活动**的输出和输入设备（默认设备排在最前）。
pub fn list() -> Result<Vec<AudioDevice>> {
    #[cfg(windows)]
    {
        list_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[cfg(windows)]
fn list_windows() -> Result<Vec<AudioDevice>> {
    static INIT: Once = Once::new();
    // COM 只初始化一次；GUI 线程可能已经初始化过（模式不同会返回错误，忽略即可）
    INIT.call_once(|| {
        let _ = wasapi::initialize_mta();
    });
    let enumerator = DeviceEnumerator::new()?;
    let mut out = Vec::new();

    for (flow, dir) in [(Flow::Output, Direction::Render), (Flow::Input, Direction::Capture)] {
        // 默认设备：拿 ID 来比，比名字可靠
        let default_id = enumerator
            .get_default_device(dir)
            .and_then(|d| d.get_id())
            .unwrap_or_default();

        let coll = enumerator.get_device_collection(dir)?;
        for dev in &coll {
            let Ok(dev) = dev else { continue };
            // 只列活动的：拔掉的/禁用的设备列出来只会让人困惑
            if !matches!(dev.get_state(), Ok(DeviceState::Active)) {
                continue;
            }
            let id = dev.get_id().unwrap_or_default();
            let name = dev
                .get_friendlyname()
                .unwrap_or_else(|_| "未命名设备".to_string());
            let description = dev.get_description().unwrap_or_default();
            let (kind, is_bluetooth) = classify(&name, &description, flow);
            let keys = format!("{name} {description}").to_lowercase();
            let is_steam = keys.contains("steam");
            let is_realtek = keys.contains("realtek") || keys.contains("瑞昱") || keys.contains("瑞鼎");
            let is_nvidia = keys.contains("nvidia");
            let is_camera = keys.contains("webcam")
                || keys.contains("camera")
                || keys.contains("摄像头")
                || keys.contains("摄像");
            let format = dev.get_device_format().ok();
            let (channels, sample_rate, bits, format_label) = match format {
                Some(f) => {
                    let bits = f.get_validbitspersample().max(f.get_bitspersample().min(32));
                    let sub = match f.get_subformat() {
                        Ok(wasapi::SampleType::Float) => "float",
                        _ => "int",
                    };
                    (
                        f.get_nchannels(),
                        f.get_samplespersec(),
                        bits,
                        format!(
                            "{bits}-bit {sub} · {} kHz · {}ch",
                            f.get_samplespersec() as f32 / 1000.0,
                            f.get_nchannels()
                        ),
                    )
                }
                None => (0, 0, 0, "格式未知".to_string()),
            };
            out.push(AudioDevice {
                is_default: !id.is_empty() && id == default_id,
                id,
                name,
                description,
                flow,
                kind,
                is_bluetooth,
                is_steam,
                is_realtek,
                is_nvidia,
                is_camera,
                channels,
                sample_rate,
                bits,
                format_label,
            });
        }
    }

    // 开发用：没有蓝牙硬件的机器上也能预览蓝牙页（仅 debug 构建）
    #[cfg(debug_assertions)]
    if std::env::var("FC_FAKE_BT").ok().as_deref() == Some("1") {
        out.extend(fake_bluetooth());
    }

    // 默认设备排前面，其余按名字排，列表稳定
    out.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// 开发用假蓝牙耳机：`FC_FAKE_BT=1` 时混进设备列表，
/// 方便在没有蓝牙硬件的机器上验收"协议清单 + 冲刺动效"。
#[cfg(debug_assertions)]
fn fake_bluetooth() -> Vec<AudioDevice> {
    let mk = |name: &str, flow: Flow, default: bool| AudioDevice {
        id: format!("fake:{name}"),
        name: name.to_string(),
        description: "Bluetooth Audio".into(),
        flow,
        kind: if flow == Flow::Output {
            Kind::Headphones
        } else {
            Kind::Microphone
        },
        is_bluetooth: true,
        is_steam: false,
        is_realtek: false,
        is_nvidia: false,
        is_camera: false,
        is_default: default,
        channels: 2,
        sample_rate: 48_000,
        bits: 16,
        format_label: "16-bit int · 48 kHz · 2ch".into(),
    };
    vec![
        mk("耳机 (WH-1000XM4 Stereo)", Flow::Output, true),
        mk("耳机 (WH-1000XM4 Hands-Free AG Audio)", Flow::Input, false),
    ]
}

/// 按设备名 / 驱动描述归类。
///
/// 耳机优先于"扬声器"：Windows 插上耳机后名字通常会变成「耳机 (...)」，
/// 但描述仍可能是 "Speakers"，所以两个都看。
fn classify(name: &str, desc: &str, flow: Flow) -> (Kind, bool) {
    let s = format!("{name} {desc}").to_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| s.contains(k));

    let is_bluetooth = has(&[
        "bluetooth",
        "蓝牙",
        "bthhfenum",
        "bthenum",
        "hands-free",
        "airpods",
        "freebuds",
        "buds",
        "wh-1000",
        "wf-1000",
        "quietcomfort",
        "soundcore",
        "tws",
        "wireless",
    ]);

    if flow == Flow::Input {
        let kind = if has(&["麦克风", "microphone", "mic ", "mic(", "话筒", "阵列"]) {
            Kind::Microphone
        } else if has(&["headset", "耳麦"]) {
            Kind::Headset
        } else {
            Kind::Other
        };
        return (kind, is_bluetooth);
    }

    let kind = if has(&["headset", "耳麦"]) {
        Kind::Headset
    } else if has(&[
        "耳机",
        "headphone",
        "earphone",
        "airpods",
        "wh-1000",
        "wf-1000",
        "buds",
    ]) {
        Kind::Headphones
    } else if has(&["hdmi", "display audio", "displayport"]) {
        Kind::Hdmi
    } else if has(&["digital output", "s/pdif", "spdif", "光纤", "同轴"]) {
        Kind::Digital
    } else if has(&["usb", "声卡", "dac"]) {
        Kind::Usb
    } else if has(&["speaker", "扬声器", "音箱"]) {
        Kind::Speakers
    } else {
        Kind::Other
    };
    (kind, is_bluetooth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_headphone_is_recognised() {
        let (k, bt) = classify(
            "Headphones (WH-1000XM4 Stereo)",
            "Bluetooth Audio",
            Flow::Output,
        );
        assert_eq!(k, Kind::Headphones);
        assert!(bt, "蓝牙耳机应该被认出来");
    }

    #[test]
    fn wired_headphone_is_not_bluetooth() {
        let (k, bt) = classify("耳机 (Realtek(R) Audio)", "Speakers", Flow::Output);
        assert_eq!(k, Kind::Headphones);
        assert!(!bt);
    }

    #[test]
    fn microphone_is_input() {
        let (k, _) = classify("麦克风阵列 (Intel Smart Sound)", "Microphone", Flow::Input);
        assert_eq!(k, Kind::Microphone);
    }

    #[test]
    fn hdmi_and_usb_are_separate() {
        assert_eq!(
            classify("DELL U2720Q (NVIDIA High Definition Audio)", "HDMI", Flow::Output).0,
            Kind::Hdmi
        );
        assert_eq!(
            classify("USB Audio DAC", "Speakers", Flow::Output).0,
            Kind::Usb
        );
    }

    fn bt_dev(name: &str, flow: Flow) -> AudioDevice {
        AudioDevice {
            id: name.to_string(),
            name: name.to_string(),
            description: "Bluetooth Audio".into(),
            flow,
            kind: if flow == Flow::Output {
                Kind::Headphones
            } else {
                Kind::Microphone
            },
            is_bluetooth: true,
            is_steam: false,
            is_realtek: false,
            is_nvidia: false,
            is_camera: false,
            is_default: false,
            channels: 2,
            sample_rate: 48_000,
            bits: 16,
            format_label: "16-bit int · 48 kHz · 2ch".into(),
        }
    }

    #[test]
    fn bluetooth_stereo_and_handsfree_form_one_headset() {
        let devs = vec![
            bt_dev("耳机 (WH-1000XM4 Stereo)", Flow::Output),
            bt_dev("耳机 (WH-1000XM4 Hands-Free AG Audio)", Flow::Input),
        ];
        let groups = bluetooth_groups(&devs);
        assert_eq!(groups.len(), 1, "两个端点应归成一只耳机");
        assert_eq!(groups[0].name, "耳机 (WH-1000XM4)");
        let p = &groups[0].profiles;
        assert!(p.iter().any(|x| x.name == "A2DP" && x.active));
        assert!(p.iter().any(|x| x.name == "HFP" && x.active));
        assert!(p.iter().any(|x| x.name == "AVRCP" && x.active));
        assert!(p.iter().any(|x| x.name == "HSP" && !x.active));
    }

    #[test]
    fn bluetooth_stereo_only_has_no_hfp() {
        let groups = bluetooth_groups(&[bt_dev("Headphones (Buds Stereo)", Flow::Output)]);
        let p = &groups[0].profiles;
        assert!(p.iter().any(|x| x.name == "A2DP" && x.active));
        assert!(p.iter().any(|x| x.name == "HFP" && !x.active));
    }
}
