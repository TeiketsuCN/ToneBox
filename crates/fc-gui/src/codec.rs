//! 「引擎和设备」页：设备 / 蓝牙 / 引擎 三个子页。
//!
//! 设备页先回答"这台机器上到底有哪些输出/输入、哪个是耳机/蓝牙/摄像头"，
//! 再让用户指定播放用哪个输出；蓝牙单独一页；引擎页放播放选项和引擎信息。
//!
//! 设备图标是手绘的（字体里没有这些符号）：耳机、麦克风、摄像头、扬声器、HDMI、
//! 数字输出、USB、蓝牙、芯片。Steam/Realtek 这些厂商特征用颜色和角标标出来，
//! USB 声卡用金色 + 呼吸光提示"可能是独立声卡"。

#![allow(unused_imports)]

use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};
use crate::*;
use crate::util::*;
use crate::output::*;
use crate::rail::*;

/// 「引擎和设备」的三个子页
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodecTab {
    Devices,
    Bluetooth,
    Engine,
}

/// 各厂商/类型的识别色
const C_STEAM: Color32 = Color32::from_rgb(58, 104, 220); // 深蓝
const C_REALTEK: Color32 = Color32::from_rgb(48, 96, 210); // 深蓝（瑞昱/瑞鼎）
const C_USB: Color32 = Color32::from_rgb(232, 181, 74); // 黄金
const C_CAMERA: Color32 = Color32::from_rgb(226, 138, 106);
const C_BT: Color32 = Color32::from_rgb(130, 175, 255);
const C_HEAD: Color32 = Color32::from_rgb(150, 220, 190);
const C_MIC: Color32 = Color32::from_rgb(255, 198, 122);
const C_HDMI: Color32 = Color32::from_rgb(160, 180, 230);
const C_DIGITAL: Color32 = Color32::from_rgb(140, 200, 210);
const C_NVIDIA: Color32 = Color32::from_rgb(118, 185, 0); // NVIDIA 绿

fn kind_color(k: fc_player::devices::Kind) -> Color32 {
    use fc_player::devices::Kind;
    match k {
        Kind::Headphones | Kind::Headset => C_HEAD,
        Kind::Microphone => C_MIC,
        Kind::Hdmi => C_HDMI,
        Kind::Digital => C_DIGITAL,
        Kind::Usb => C_USB,
        Kind::Speakers => MUTED,
        Kind::Other => DIM,
    }
}

/// 一个设备的主色：厂商/用途优先级高于类型
fn device_color(d: &fc_player::devices::AudioDevice) -> Color32 {
    if d.is_steam {
        C_STEAM
    } else if d.is_camera {
        C_CAMERA
    } else if d.is_nvidia {
        C_NVIDIA
    } else if d.is_realtek {
        C_REALTEK
    } else if d.kind == fc_player::devices::Kind::Usb {
        C_USB
    } else if d.is_bluetooth {
        C_BT
    } else {
        kind_color(d.kind)
    }
}

impl App {
    /// 「引擎和设备」总入口。
    pub(crate) fn codec_view(&mut self, ui: &mut egui::Ui) {
        if !self.devices_loaded {
            self.devices_loaded = true;
            match fc_player::devices::list() {
                Ok(v) => {
                    self.audio_devices = v;
                    self.devices_error = None;
                }
                Err(e) => self.devices_error = Some(format!("{e:#}")),
            }
        }

        // 这个页有 USB 呼吸光/蓝牙冲刺动效：不靠鼠标事件也要持续跑
        ui.ctx().request_repaint();

        egui::ScrollArea::vertical()
            .id_salt("codec_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("引擎和设备").size(14.0).color(TEXT).strong());
                    ui.add_space(12.0);
                    if tiny_text_button(ui, "重新检测") {
                        self.devices_loaded = false;
                        self.audio_devices.clear();
                        self.devices_error = None;
                    }
                });
                ui.add_space(4.0);

                if let Some(e) = &self.devices_error {
                    para(ui, &format!("设备检测失败：{e}"), 9.0, DANGER);
                    ui.add_space(8.0);
                }

                match self.codec_tab {
                    CodecTab::Devices => self.codec_devices(ui),
                    CodecTab::Bluetooth => self.codec_bluetooth(ui),
                    CodecTab::Engine => self.codec_engine(ui),
                }
                ui.add_space(12.0);
            });
    }

    /// 设备子页：输出 + 输入（不含蓝牙，蓝牙在单独一页）
    fn codec_devices(&mut self, ui: &mut egui::Ui) {
        let mut pick: Option<String> = None;
        ui.label(RichText::new("输出设备").size(10.0).color(DIM));
        ui.add_space(2.0);
        let outs: Vec<usize> = (0..self.audio_devices.len())
            .filter(|i| {
                let d = &self.audio_devices[*i];
                d.flow == fc_player::devices::Flow::Output && !d.is_bluetooth
            })
            .collect();
        for idx in outs {
            let dev = &self.audio_devices[idx];
            let selected = if self.cfg.audio_out_device.is_empty() {
                dev.is_default
            } else {
                dev.name == self.cfg.audio_out_device
            };
            if self.device_row(ui, dev, selected, true) {
                pick = Some(if dev.is_default {
                    String::new()
                } else {
                    dev.name.clone()
                });
            }
        }
        if let Some(name) = pick {
            self.cfg.audio_out_device = name;
            self.cfg.save();
            // 换输出时停掉当前播放，避免声音还从旧设备出
            self.player = None;
        }

        ui.add_space(12.0);
        ui.label(RichText::new("输入设备").size(10.0).color(DIM));
        ui.add_space(2.0);
        let ins: Vec<usize> = (0..self.audio_devices.len())
            .filter(|i| {
                let d = &self.audio_devices[*i];
                d.flow == fc_player::devices::Flow::Input && !d.is_bluetooth
            })
            .collect();
        if ins.is_empty() {
            para(ui, "没有检测到输入设备。", 9.0, DIM);
        } else {
            for idx in ins {
                let dev = &self.audio_devices[idx];
                self.device_row(ui, dev, false, false);
            }
        }
        ui.add_space(6.0);
        para(
            ui,
            "耳机/摄像头/USB 声卡按设备名和驱动描述判断；Steam 串流设备标深蓝，Realtek 板载标深蓝。",
            8.5,
            DIM,
        );
    }

    /// 蓝牙子页：按耳机分组，写明正在使用的蓝牙协议，并画一条"冲刺"数据带动效。
    fn codec_bluetooth(&mut self, ui: &mut egui::Ui) {
        let groups = fc_player::devices::bluetooth_groups(&self.audio_devices);
        if groups.is_empty() {
            para(ui, "没有检测到蓝牙音频设备。", 10.0, MUTED);
            ui.add_space(4.0);
            para(
                ui,
                "先在 Windows 设置里把耳机/音箱连上，再点右上角「重新检测」。",
                8.5,
                DIM,
            );
            return;
        }
        // 冲刺动效要持续重绘
        ui.ctx().request_repaint();
        let t = ui.input(|i| i.time) as f32;

        let mut pick: Option<String> = None;
        for g in groups {
            // 耳机标题
            ui.horizontal(|ui| {
                paint_bt_rune(ui, 18.0);
                ui.add_space(6.0);
                ui.label(RichText::new(&g.name).size(12.0).color(TEXT).strong());
                ui.add_space(8.0);
                ui.label(RichText::new("已连接").size(8.5).color(C_BT));
            });
            ui.add_space(4.0);

            // 协议清单：全部列出来，标清楚哪个正在用
            for prof in &g.profiles {
                let (state, col) = profile_state(prof);
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    let dot = if prof.active { C_BT } else { tint(Color32::WHITE, 40) };
                    ui.label(RichText::new("●").size(7.0).color(dot));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(prof.name)
                            .size(9.5)
                            .color(if prof.active { TEXT } else { MUTED })
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(state).size(8.5).color(col));
                    ui.add_space(8.0);
                    ui.label(RichText::new(prof.role).size(8.5).color(DIM));
                });
            }
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.add_space(28.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(760.0, 0.0),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        para(
                            ui,
                            "实际编解码（SBC / AAC / aptX / LDAC）由耳机和驱动协商，Windows 不公开；这里报的是蓝牙协议。",
                            8.0,
                            DIM,
                        );
                    },
                );
            });
            ui.add_space(6.0);

            // 冲刺动效：数据带沿轨道向右冲，A2DP 在用时速度更快
            let active = g.profiles.iter().any(|p| p.active && p.name == "A2DP");
            let (track, _) = ui.allocate_exact_size(
                egui::vec2((ui.available_width() - 4.0).min(1096.0), 30.0),
                egui::Sense::hover(),
            );
            paint_sprint(ui.painter(), track, t, C_BT, active);
            ui.add_space(6.0);

            // 端点（可点选为播放输出）
            for d in &g.endpoints {
                let selected = if self.cfg.audio_out_device.is_empty() {
                    d.is_default && d.flow == fc_player::devices::Flow::Output
                } else {
                    d.name == self.cfg.audio_out_device
                };
                let can_pick = d.flow == fc_player::devices::Flow::Output;
                if self.device_row(ui, d, selected, can_pick) {
                    pick = Some(d.name.clone());
                }
            }
            ui.add_space(12.0);
        }
        if let Some(name) = pick {
            self.cfg.audio_out_device = name;
            self.cfg.save();
            self.player = None;
        }
    }

    /// 引擎子页：播放选项、当前输出、引擎与输出能力
    fn codec_engine(&mut self, ui: &mut egui::Ui) {
        // 引擎页内容限宽：说明文字按 780 换行，不横着拉满整个窗口
        ui.set_max_width(780.0);
        ui.label(RichText::new("播放").size(10.0).color(DIM));
        ui.add_space(6.0);
        let mut rg = self.cfg.replaygain;
        if transparent_toggle(ui, &mut rg, "播放时应用 ReplayGain（按文件里的响度标签调整）") {
            self.cfg.replaygain = rg;
            self.cfg.save();
        }
        let mut ex = self.cfg.exclusive_output;
        if transparent_toggle(ui, &mut ex, "独占输出（WASAPI 直通，绕过系统混音）") {
            self.cfg.exclusive_output = ex;
            self.cfg.save();
        }

        // ReplayGain 是啥：用大白话讲清楚，不堆术语
        ui.add_space(6.0);
        ui.allocate_ui_with_layout(
            egui::vec2(780.0, 0.0),
            Layout::top_down(Align::LEFT),
            |ui| {
                para(ui, "ReplayGain 是什么？", 9.0, MUTED);
                para(
                    ui,
                    "同一张专辑里，歌和歌的响度经常不一样：有的母带压得响，有的轻。",
                    8.5,
                    DIM,
                );
                para(
                    ui,
                    "ReplayGain 是文件里的一条标签，写着「这首歌该降/升多少 dB，才能和别的歌一样响」。",
                    8.5,
                    DIM,
                );
                para(
                    ui,
                    "打开后，播放时会按标签把音量调平，不改文件本身；没有这条标签的歌不受影响。",
                    8.5,
                    DIM,
                );
                para(
                    ui,
                    "这里读的是单曲增益（REPLAYGAIN_TRACK_GAIN），不是整张专辑一起调的那种。",
                    8.5,
                    DIM,
                );
                pose_hint(
                    ui,
                    "独占输出为了原样把数据送给声卡，不应用 ReplayGain；需要调响度时用共享输出。",
                );
            },
        );

        if let Some(p) = &self.player {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("当前输出").size(9.0).color(DIM));
                ui.label(RichText::new(p.output_label()).size(9.0).color(MUTED));
            });
        }

        ui.add_space(14.0);
        ui.label(RichText::new("引擎").size(10.0).color(DIM));
        ui.add_space(6.0);
        egui::Grid::new("codec_engine")
            .num_columns(2)
            .spacing([14.0, 5.0])
            .show(ui, |ui| {
                row(ui, "解码/编码", "ffmpeg 子进程（崩溃隔离）");
                if let Some(t) = &self.tools {
                    row(ui, "ffmpeg", &t.ffmpeg.display().to_string());
                } else {
                    row(ui, "ffmpeg", "没找到");
                }
                match &self.atracdenc {
                    Some(p) => row(ui, "ATRAC 编码", &p.display().to_string()),
                    None => row(ui, "ATRAC 编码", "未安装 atracdenc"),
                }
                row(ui, "可用编码器", &format!("{} 个", self.encoders.len()));
                row(ui, "格式表", &format!("{} 条", fc_core::all().count()));
            });

        ui.add_space(14.0);
        ui.label(RichText::new("输出能力").size(10.0).color(DIM));
        ui.add_space(6.0);
        egui::Grid::new("codec_out_caps")
            .num_columns(2)
            .spacing([14.0, 5.0])
            .show(ui, |ui| {
                row(ui, "独占输出", if self.cfg.exclusive_output {
                    "开 · WASAPI 直通"
                } else {
                    "关（用共享输出）"
                });
                row(ui, "采样率", "独占时按源采样率；共享时由 Windows 混音格式决定");
                row(ui, "数据直通", "独占 + 32-bit int：队列字节原样写声卡，音量/ReplayGain 不介入");
                row(ui, "连续播放", "未做：要先有队列连播（现在是单文件试听）");
                row(ui, "DSD", "解成 PCM 播放；DoP/原生 DSD 未做");
                row(ui, "ASIO", "未支持（需要 ASIO SDK 和驱动）");
            });
    }

    /// 一行设备：大图标 + 名称 + 类型/角标 + 格式；输出设备可点选。
    /// 行背景保持透明（圆点选项不做底色），只有图标和圆点着色。
    fn device_row(
        &self,
        ui: &mut egui::Ui,
        dev: &fc_player::devices::AudioDevice,
        selected: bool,
        selectable: bool,
    ) -> bool {
        let h = 64.0;
        // 限制行宽：设备名在左、标记在右，别把水印甩到窗口最右边
        let w = ui.available_width().min(1100.0);
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(w, h),
            if selectable {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let p = ui.painter();
        // 透明底：只画图标/文字，不整行铺色
        let color = device_color(dev);

        let icon = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 8.0, rect.center().y - 22.0),
            egui::vec2(44.0, 44.0),
        );
        let t = ui.input(|i| i.time) as f32;
        paint_device_icon(p, icon, dev, color, t);

        // 选择圆点
        let dot_c = egui::pos2(icon.right() + 16.0, rect.center().y);
        if selectable {
            p.circle_stroke(
                dot_c,
                6.5,
                Stroke::new(
                    1.3,
                    if selected {
                        accent()
                    } else if resp.hovered() {
                        tint(Color32::WHITE, 120)
                    } else {
                        tint(Color32::WHITE, 55)
                    },
                ),
            );
            if selected {
                p.circle_filled(dot_c, 3.6, accent());
            }
        }

        let text_x = dot_c.x + 18.0;
        let name_rect = egui::Rect::from_min_max(
            egui::pos2(text_x, rect.top() + 9.0),
            egui::pos2(rect.right() - 12.0, rect.top() + 29.0),
        );
        paint_clipped_v(
            ui,
            name_rect,
            11.5,
            if selected { accent() } else { TEXT },
            &dev.name,
        );

        // 第二行：类型 + 厂商/用途角标；格式靠右
        let y = rect.top() + 42.0;
        let mut x = text_x;
        // NVIDIA 显卡音频：类型保持中文"显示器HDMI音频输出"，右边角标绿框白字「英伟达」
        let kind = if dev.is_nvidia {
            "显示器HDMI音频输出"
        } else {
            dev.kind.label()
        };
        let kind_col = if dev.is_nvidia {
            C_NVIDIA
        } else {
            kind_color(dev.kind)
        };
        p.text(
            egui::pos2(x, y),
            Align2::LEFT_CENTER,
            kind,
            FontId::proportional(9.5),
            kind_col,
        );
        x += text_w(ui, kind, 9.5) + 10.0;

        for (label, col) in badges(dev) {
            // 角标做成小药丸：深蓝/金色本身偏暗，纯文字在深底上看不清
            let tw = text_w(ui, &label, 8.5);
            let w = tw + 10.0;
            let pill = egui::Rect::from_min_size(
                egui::pos2(x, y - 7.5),
                egui::vec2(w, 15.0),
            );
            if label == "英伟达" {
                // 绿框白字：只描边，不铺色
                p.rect_filled(pill, Rounding::same(4.0), tint(C_NVIDIA, 10));
                p.rect_stroke(
                    pill,
                    Rounding::same(4.0),
                    Stroke::new(1.1, C_NVIDIA),
                );
                p.text(
                    pill.center(),
                    Align2::CENTER_CENTER,
                    &label,
                    FontId::proportional(8.5),
                    Color32::WHITE,
                );
            } else {
                p.rect_filled(pill, Rounding::same(4.0), tint(col, 40));
                p.text(
                    pill.center(),
                    Align2::CENTER_CENTER,
                    &label,
                    FontId::proportional(8.5),
                    blend(col, Color32::WHITE, 0.45),
                );
            }
            x += w + 6.0;
        }

        let fmt_rect = egui::Rect::from_min_max(
            egui::pos2(x, y - 9.0),
            egui::pos2(rect.right() - 12.0, y + 9.0),
        );
        paint_clipped_v(ui, fmt_rect, 8.5, DIM, &dev.detail());

        if resp.hovered() && selectable {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp.clicked() && selectable
    }
}

/// 设备角标：(文字, 颜色)
fn badges(d: &fc_player::devices::AudioDevice) -> Vec<(String, Color32)> {
    let mut out = Vec::new();
    if d.is_default {
        out.push(("默认设备".to_string(), accent()));
    }
    if d.is_steam {
        out.push(("Steam".to_string(), C_STEAM));
    }
    if d.is_realtek {
        out.push(("瑞鼎".to_string(), C_REALTEK));
    }
    if d.is_camera {
        out.push(("摄像头".to_string(), C_CAMERA));
    }
    if d.is_nvidia {
        out.push(("英伟达".to_string(), C_NVIDIA));
    }
    if d.kind == fc_player::devices::Kind::Usb {
        out.push(("独立声卡".to_string(), C_USB));
    }
    if d.is_bluetooth {
        out.push(("蓝牙".to_string(), C_BT));
    }
    out
}

fn text_w(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.ctx().fonts(|f| {
        f.layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE)
            .size()
            .x
    })
}

/// 手绘设备图标。`color` 已经包含厂商/类型信息；USB 会加一圈呼吸光。
fn paint_device_icon(
    p: &egui::Painter,
    rect: egui::Rect,
    d: &fc_player::devices::AudioDevice,
    color: Color32,
    t: f32,
) {
    use fc_player::devices::Kind;
    // 底：半透明圆角块，深蓝/金色会更有存在感
    let pulse = if d.kind == Kind::Usb {
        0.5 + 0.5 * (t * 2.2).sin().abs()
    } else {
        1.0
    };
    p.rect_filled(rect, Rounding::same(11.0), tint(color, (34.0 * pulse) as u8));
    p.rect_stroke(
        rect,
        Rounding::same(11.0),
        Stroke::new(1.2, tint(color, (95.0 * pulse) as u8)),
    );
    // USB 声卡：两圈向外扩散的呼吸光
    if d.kind == Kind::Usb {
        for k in 0..2 {
            let phase = ((t * 1.1 + k as f32 * 0.5) % 1.0).clamp(0.0, 1.0);
            let grow = 3.0 + phase * 9.0;
            p.rect_stroke(
                rect.expand(grow),
                Rounding::same(11.0 + grow * 0.35),
                Stroke::new(1.5, tint(color, ((1.0 - phase) * 75.0) as u8)),
            );
        }
    }

    let c = rect.center();
    let stroke = Stroke::new(1.8, color);
    if d.is_camera {
        // 摄像头：机身 + 镜头 + 上方取景凸起
        let body = egui::Rect::from_center_size(c + egui::vec2(0.0, 2.0), egui::vec2(26.0, 18.0));
        p.rect_stroke(body, Rounding::same(4.0), stroke);
        p.circle_stroke(c + egui::vec2(0.0, 2.0), 6.0, Stroke::new(1.6, color));
        p.rect_filled(
            egui::Rect::from_center_size(c + egui::vec2(-7.0, -10.0), egui::vec2(9.0, 4.0)),
            Rounding::same(1.5),
            color,
        );
        return;
    }
    if d.is_steam {
        // Steam：游戏手柄（圆角横条 + 两个摇杆）
        let body = egui::Rect::from_center_size(c, egui::vec2(30.0, 18.0));
        p.rect_stroke(body, Rounding::same(8.0), stroke);
        p.circle_filled(c + egui::vec2(-8.0, 1.0), 2.2, color);
        p.circle_filled(c + egui::vec2(8.0, 1.0), 2.2, color);
        p.line_segment(
            [c + egui::vec2(-3.0, -2.0), c + egui::vec2(3.0, 2.0)],
            Stroke::new(1.4, color),
        );
        return;
    }
    match d.kind {
        Kind::Headphones | Kind::Headset => {
            // 头梁 + 两个耳罩
            let r = 11.0;
            let mut pts = Vec::new();
            for i in 0..=16 {
                let a = std::f32::consts::PI + std::f32::consts::PI * (i as f32 / 16.0);
                pts.push(c + egui::vec2(a.cos(), a.sin()) * r + egui::vec2(0.0, 4.0));
            }
            p.add(egui::Shape::line(pts, stroke));
            p.rect_filled(
                egui::Rect::from_center_size(c + egui::vec2(-11.0, 6.0), egui::vec2(5.0, 11.0)),
                Rounding::same(2.5),
                color,
            );
            p.rect_filled(
                egui::Rect::from_center_size(c + egui::vec2(11.0, 6.0), egui::vec2(5.0, 11.0)),
                Rounding::same(2.5),
                color,
            );
        }
        Kind::Microphone => {
            // 胶囊 + 支架
            let cap = egui::Rect::from_center_size(c + egui::vec2(0.0, -4.0), egui::vec2(9.0, 15.0));
            p.rect_stroke(cap, Rounding::same(4.5), stroke);
            let mut pts = Vec::new();
            for i in 0..=12 {
                let a = std::f32::consts::PI * (i as f32 / 12.0);
                pts.push(c + egui::vec2(a.cos() * 9.0, a.sin() * 9.0 + 2.0));
            }
            p.add(egui::Shape::line(pts, Stroke::new(1.5, color)));
            p.line_segment(
                [c + egui::vec2(0.0, 11.0), c + egui::vec2(0.0, 16.0)],
                Stroke::new(1.5, color),
            );
            p.line_segment(
                [c + egui::vec2(-5.0, 16.0), c + egui::vec2(5.0, 16.0)],
                Stroke::new(1.5, color),
            );
        }
        Kind::Hdmi => {
            // 显示器 + 底座
            let body = egui::Rect::from_center_size(c + egui::vec2(0.0, -3.0), egui::vec2(26.0, 17.0));
            p.rect_stroke(body, Rounding::same(2.5), stroke);
            p.line_segment(
                [c + egui::vec2(-6.0, 12.0), c + egui::vec2(6.0, 12.0)],
                Stroke::new(1.5, color),
            );
        }
        Kind::Digital => {
            // 同轴/光纤：圆口 + 中心点
            p.circle_stroke(c, 10.0, stroke);
            p.circle_filled(c, 3.0, color);
        }
        Kind::Usb => {
            // USB 三叉
            p.line_segment(
                [c + egui::vec2(0.0, -12.0), c + egui::vec2(0.0, 12.0)],
                stroke,
            );
            p.line_segment(
                [c + egui::vec2(0.0, 2.0), c + egui::vec2(-9.0, -5.0)],
                stroke,
            );
            p.line_segment(
                [c + egui::vec2(0.0, -3.0), c + egui::vec2(9.0, -9.0)],
                stroke,
            );
            p.circle_filled(c + egui::vec2(0.0, -12.0), 2.0, color);
            p.rect_filled(
                egui::Rect::from_center_size(c + egui::vec2(-9.0, -5.0), egui::vec2(4.0, 4.0)),
                Rounding::same(1.0),
                color,
            );
            p.circle_filled(c + egui::vec2(9.0, -9.0), 2.0, color);
        }
        Kind::Speakers => {
            // 箱体 + 声波
            let body = egui::Rect::from_center_size(c + egui::vec2(-5.0, 0.0), egui::vec2(15.0, 22.0));
            p.rect_stroke(body, Rounding::same(3.0), stroke);
            p.circle_stroke(c + egui::vec2(-5.0, 4.0), 3.5, Stroke::new(1.4, color));
            for (i, r) in [8.0f32, 13.0].iter().enumerate() {
                let mut pts = Vec::new();
                for k in 0..=10 {
                    let a = -0.7 + 1.4 * (k as f32 / 10.0);
                    pts.push(
                        c + egui::vec2(6.0, 0.0)
                            + egui::vec2(a.cos() * *r, a.sin() * *r),
                    );
                }
                p.add(egui::Shape::line(
                    pts,
                    Stroke::new(1.2, tint(color, if i == 0 { 200 } else { 120 })),
                ));
            }
        }
        Kind::Other => {
            p.circle_stroke(c, 8.0, stroke);
            p.circle_filled(c, 2.5, color);
        }
    }
}

/// 协议状态文字 + 颜色
fn profile_state(p: &fc_player::devices::BtProfile) -> (&'static str, Color32) {
    if p.active {
        return match p.name {
            "AVRCP" => ("可用", MUTED),
            _ => ("正在使用", C_BT),
        };
    }
    match p.name {
        "HSP" => ("兼容备用", DIM),
        _ => ("未连接", DIM),
    }
}

/// 蓝牙符号（手绘）
fn paint_bt_rune(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(size, size),
        egui::Sense::hover(),
    );
    let p = ui.painter();
    let c = rect.center();
    let h = size * 0.38;
    let w = size * 0.24;
    let s = Stroke::new(1.8, C_BT);
    p.line_segment(
        [egui::pos2(c.x, c.y - h), egui::pos2(c.x, c.y + h)],
        s,
    );
    let up = egui::pos2(c.x + w, c.y - h * 0.5);
    let lo = egui::pos2(c.x - w, c.y + h * 0.5);
    let lup = egui::pos2(c.x - w, c.y - h * 0.5);
    let llo = egui::pos2(c.x + w, c.y + h * 0.5);
    p.line_segment([egui::pos2(c.x, c.y - h), up], s);
    p.line_segment([up, lo], s);
    p.line_segment([lo, egui::pos2(c.x, c.y + h)], s);
    p.line_segment([egui::pos2(c.x, c.y - h), lup], s);
    p.line_segment([lup, llo], s);
    p.line_segment([llo, egui::pos2(c.x, c.y + h)], s);
}

/// "冲刺"数据带：几个箭头带着尾迹向右冲；
/// A2DP 在用时速度更快，没在用时几乎不动（避免误导成正在传音频）。
fn paint_sprint(
    p: &egui::Painter,
    rect: egui::Rect,
    t: f32,
    color: Color32,
    active: bool,
) {
    p.rect_filled(rect, Rounding::same(15.0), tint(Color32::WHITE, 5));
    let clip = rect.shrink(3.0);
    let p = p.with_clip_rect(clip);
    let y = rect.center().y;

    // 底：一条很淡的虚线轨道，让"冲刺"有参照
    let mut x = clip.left();
    while x < clip.right() {
        let x2 = (x + 4.0).min(clip.right());
        p.line_segment(
            [egui::pos2(x, y), egui::pos2(x2, y)],
            Stroke::new(1.0, tint(Color32::WHITE, 18)),
        );
        x += 10.0;
    }

    let speed = if active { 0.75 } else { 0.10 };
    let n = 6;
    for i in 0..n {
        let phase = (t * speed + i as f32 / n as f32) % 1.0;
        let hx = clip.left() + clip.width() * phase;
        // 靠近右端淡出，像冲出画面
        let fade = ((1.0 - phase) / 0.12).clamp(0.0, 1.0);
        let alpha = if phase > 0.88 { (170.0 * fade) as u8 } else { 170 };
        if alpha < 8 {
            continue;
        }
        // 尾迹：越靠后越短、越淡
        for k in 0..5 {
            let back = k as f32 * 7.0;
            let x0 = hx - back - 6.0;
            let x1 = hx - back;
            let a = alpha.saturating_sub(k as u8 * 32);
            if a < 8 {
                continue;
            }
            p.line_segment(
                [egui::pos2(x0, y), egui::pos2(x1, y)],
                Stroke::new(2.6 - k as f32 * 0.35, tint(color, a)),
            );
        }
        // 箭头头
        let head = Stroke::new(2.4, tint(color, alpha.saturating_add(40)));
        p.line_segment(
            [egui::pos2(hx - 5.0, y - 4.0), egui::pos2(hx, y)],
            head,
        );
        p.line_segment(
            [egui::pos2(hx - 5.0, y + 4.0), egui::pos2(hx, y)],
            head,
        );
    }
    // 右端终点光
    let glow = 0.5 + 0.5 * (t * 3.2).sin().abs();
    p.circle_filled(
        egui::pos2(clip.right() - 5.0, y),
        if active { 3.6 } else { 2.2 },
        tint(color, (85.0 * glow) as u8),
    );
}

/// 透明勾选行：不铺底，只画一个方框 + 勾，点击整行切换
fn transparent_toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(760.0), 26.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let box_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 9.0, rect.center().y),
        egui::vec2(14.0, 14.0),
    );
    p.rect_filled(box_rect, Rounding::same(3.0), Color32::TRANSPARENT);
    p.rect_stroke(
        box_rect,
        Rounding::same(3.0),
        Stroke::new(1.2, if *on { accent() } else { tint(Color32::WHITE, 70) }),
    );
    if *on {
        p.line_segment(
            [
                egui::pos2(box_rect.left() + 3.0, box_rect.center().y),
                egui::pos2(box_rect.center().x, box_rect.bottom() - 4.0),
            ],
            Stroke::new(1.8, accent()),
        );
        p.line_segment(
            [
                egui::pos2(box_rect.center().x, box_rect.bottom() - 4.0),
                egui::pos2(box_rect.right() - 2.5, box_rect.top() + 4.0),
            ],
            Stroke::new(1.8, accent()),
        );
    }
    p.text(
        egui::pos2(rect.left() + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(9.5),
        if *on { TEXT } else { MUTED },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *on = !*on;
        return true;
    }
    false
}

fn pose_hint(ui: &mut egui::Ui, text: &str) {
    para(ui, text, 8.5, DIM);
}
