//! 「编解码」页：音频设备、播放输出选择、引擎信息。
//!
//! 这个页面是用户自己构思的"引擎和设备"占位页长出来的第一版：
//! 先把"机器上到底有哪些输出/输入设备、哪个是耳机/蓝牙"说清楚，
//! 再让用户指定播放用哪个输出。设备枚举在 Windows 上走 WASAPI
//! （`fc-player::devices`），不做独占，只报告事实。

#![allow(unused_imports)]

use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};
use crate::*;
use crate::util::*;
use crate::output::*;
use crate::rail::*;

/// 设备行里的小标签颜色
fn kind_color(k: fc_player::devices::Kind) -> Color32 {
    use fc_player::devices::Kind;
    match k {
        Kind::Headphones | Kind::Headset => Color32::from_rgb(150, 220, 190),
        Kind::Microphone => WARN,
        Kind::Hdmi => Color32::from_rgb(160, 180, 230),
        Kind::Digital => Color32::from_rgb(140, 200, 210),
        Kind::Usb => Color32::from_rgb(180, 200, 220),
        Kind::Speakers => MUTED,
        Kind::Other => DIM,
    }
}

const BT_COLOR: Color32 = Color32::from_rgb(130, 175, 255);

impl App {
    /// 「编解码」空页面 + 设备检测。
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

        egui::ScrollArea::vertical()
            .id_salt("codec_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add_space(4.0);
                ui.label(RichText::new("编解码").size(14.0).color(TEXT).strong());
                ui.add_space(4.0);
                para(
                    ui,
                    "这里显示系统实际的音频输出/输入设备、播放用哪个输出，以及引擎信息。",
                    9.5,
                    MUTED,
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if tiny_text_button(ui, "重新检测") {
                        self.devices_loaded = false;
                        self.audio_devices.clear();
                        self.devices_error = None;
                    }
                    if let Some(e) = &self.devices_error {
                        ui.label(RichText::new(format!("检测失败：{e}")).size(9.0).color(DANGER));
                    } else if self.audio_devices.is_empty() {
                        ui.label(
                            RichText::new("没有检测到设备（或当前平台不支持）")
                                .size(9.0)
                                .color(DIM),
                        );
                    }
                });
                ui.add_space(8.0);

                // ── 输出设备 ──
                ui.label(RichText::new("输出设备").size(10.0).color(DIM));
                ui.add_space(4.0);
                let outs: Vec<usize> = (0..self.audio_devices.len())
                    .filter(|i| self.audio_devices[*i].flow == fc_player::devices::Flow::Output)
                    .collect();
                let mut pick: Option<String> = None;
                for (n, idx) in outs.iter().enumerate() {
                    let dev = &self.audio_devices[*idx];
                    let selected = if self.cfg.audio_out_device.is_empty() {
                        dev.is_default
                    } else {
                        dev.name == self.cfg.audio_out_device
                    };
                    if self.device_row(ui, n, dev, selected, true) {
                        pick = Some(if dev.is_default {
                            // 选默认设备时存空串：换默认设备后自动跟着走
                            String::new()
                        } else {
                            dev.name.clone()
                        });
                    }
                }
                if let Some(name) = pick {
                    self.cfg.audio_out_device = name;
                    self.cfg.save();
                    // 正在播的话下次生效更简单：停掉当前播放，避免"声音还从旧设备出"
                    self.player = None;
                }
                ui.add_space(4.0);
                pose_hint(
                    ui,
                    "选「系统默认」会跟着 Windows 的默认输出走；耳机/蓝牙按设备名和驱动描述判断。",
                );

                // ── 输入设备 ──
                ui.add_space(14.0);
                ui.label(RichText::new("输入设备").size(10.0).color(DIM));
                ui.add_space(4.0);
                let ins: Vec<usize> = (0..self.audio_devices.len())
                    .filter(|i| self.audio_devices[*i].flow == fc_player::devices::Flow::Input)
                    .collect();
                for (n, idx) in ins.iter().enumerate() {
                    let dev = &self.audio_devices[*idx];
                    self.device_row(ui, 1000 + n, dev, false, false);
                }
                if ins.is_empty() {
                    para(ui, "没有检测到输入设备。", 9.0, DIM);
                }
                ui.add_space(4.0);
                pose_hint(ui, "输入设备只做检测，不录音；耳麦会同时出现在输入和输出里。");

                // ── 播放选项 ──
                ui.add_space(16.0);
                ui.label(RichText::new("播放").size(10.0).color(DIM));
                ui.add_space(6.0);
                let mut rg = self.cfg.replaygain;
                if ui
                    .checkbox(
                        &mut rg,
                        RichText::new("播放时应用 ReplayGain（读文件的 gain 标签）")
                            .size(10.0)
                            .color(MUTED),
                    )
                    .changed()
                {
                    self.cfg.replaygain = rg;
                    self.cfg.save();
                }
                pose_hint(
                    ui,
                    "只对带 REPLAYGAIN_TRACK_GAIN 标签的文件生效；没有标签就不加不减。",
                );
                ui.add_space(6.0);
                let mut ex = self.cfg.exclusive_output;
                if ui
                    .checkbox(
                        &mut ex,
                        RichText::new("独占输出（WASAPI 直通，绕过系统混音）")
                            .size(10.0)
                            .color(MUTED),
                    )
                    .changed()
                {
                    self.cfg.exclusive_output = ex;
                    self.cfg.save();
                }
                pose_hint(
                    ui,
                    "独占时按源采样率输出、队列里的 s32 字节直通声卡；音量与 ReplayGain 不生效，\
                     音量请用系统/功放。设备被占用会自动退回共享。",
                );
                if let Some(p) = &self.player {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("当前输出").size(9.0).color(DIM));
                        ui.label(RichText::new(p.output_label()).size(9.0).color(MUTED));
                    });
                }

                // ── 引擎 ──
                ui.add_space(16.0);
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
                ui.add_space(10.0);

                // ── HiFi 输出：做到哪一步、差什么，写在页面上，别让人猜 ──
                ui.add_space(4.0);
                ui.label(RichText::new("HiFi 输出").size(10.0).color(DIM));
                ui.add_space(6.0);
                egui::Grid::new("codec_hifi")
                    .num_columns(2)
                    .spacing([14.0, 5.0])
                    .show(ui, |ui| {
                        row(ui, "独占输出", if self.cfg.exclusive_output {
                            "开 · WASAPI 直通"
                        } else {
                            "关（用共享输出）"
                        });
                        row(ui, "采样率", "独占时按源采样率；共享时由 Windows 混音格式决定");
                        row(ui, "整数直通", "独占 + 32-bit int：队列字节原样写声卡，音量/ReplayGain 不介入");
                        row(ui, "无缝播放", "未做：要先有队列连播（现在是单文件试听）");
                        row(ui, "DSD", "解成 PCM 播放；DoP/原生 DSD 未做");
                        row(ui, "ASIO", "未支持（需要 ASIO SDK 和驱动）");
                    });
                ui.add_space(10.0);
            });
    }

    /// 一行设备。`selectable` 的输出设备可以点，输入设备只显示。
    fn device_row(
        &self,
        ui: &mut egui::Ui,
        slot: usize,
        dev: &fc_player::devices::AudioDevice,
        selected: bool,
        selectable: bool,
    ) -> bool {
        let h = 46.0;
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), h),
            if selectable {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let p = ui.painter();
        let fill = if selected {
            tint(accent(), 20)
        } else if resp.hovered() && selectable {
            tint(Color32::WHITE, 8)
        } else {
            tint(Color32::WHITE, 4)
        };
        p.rect_filled(rect, Rounding::same(9.0), fill);
        if selected {
            p.rect_stroke(rect, Rounding::same(9.0), Stroke::new(1.0, tint(accent(), 90)));
        }

        let dot_c = egui::pos2(rect.left() + 16.0, rect.center().y);
        let ring = if selected {
            accent()
        } else if resp.hovered() && selectable {
            tint(Color32::WHITE, 120)
        } else {
            tint(Color32::WHITE, 60)
        };
        p.circle_stroke(dot_c, 6.0, Stroke::new(1.2, ring));
        if selected {
            p.circle_filled(dot_c, 3.3, accent());
        } else if selectable {
            // 没选中的输出设备画一个空心的点，暗示"可以点"
            p.circle_filled(dot_c, 1.6, tint(Color32::WHITE, 40));
        }

        let name_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 32.0, rect.top() + 6.0),
            egui::pos2(rect.right() - 210.0, rect.top() + 24.0),
        );
        paint_clipped_v(ui, name_rect, 11.0, TEXT, &dev.name);

        // 第二行：类型 · 格式；蓝牙单独用蓝色标出来
        let mut x = rect.left() + 32.0;
        let y = rect.top() + 33.0;
        let type_text = dev.kind.label();
        p.text(
            egui::pos2(x, y),
            Align2::LEFT_CENTER,
            type_text,
            FontId::proportional(9.0),
            kind_color(dev.kind),
        );
        let tw = ui
            .ctx()
            .fonts(|f| f.layout_no_wrap(type_text.into(), FontId::proportional(9.0), TEXT).size().x);
        x += tw + 8.0;
        if dev.is_bluetooth {
            p.text(
                egui::pos2(x, y),
                Align2::LEFT_CENTER,
                "蓝牙",
                FontId::proportional(9.0),
                BT_COLOR,
            );
            let bw = ui.ctx().fonts(|f| {
                f.layout_no_wrap("蓝牙".into(), FontId::proportional(9.0), TEXT)
                    .size()
                    .x
            });
            x += bw + 8.0;
        }
        let fmt_rect = egui::Rect::from_min_max(
            egui::pos2(x, y - 8.0),
            egui::pos2(rect.right() - 12.0, y + 8.0),
        );
        paint_clipped_v(ui, fmt_rect, 8.5, DIM, &dev.detail());

        // 右侧：默认 / 使用中
        if dev.is_default {
            p.text(
                egui::pos2(rect.right() - 12.0, rect.top() + 15.0),
                Align2::RIGHT_CENTER,
                "系统默认",
                FontId::proportional(8.5),
                accent(),
            );
        }
        if selected && selectable {
            p.text(
                egui::pos2(rect.right() - 12.0, rect.top() + 31.0),
                Align2::RIGHT_CENTER,
                "播放用这个",
                FontId::proportional(8.5),
                MUTED,
            );
        }

        if resp.hovered() && selectable {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let _ = slot;
        resp.clicked() && selectable
    }
}

fn pose_hint(ui: &mut egui::Ui, text: &str) {
    para(ui, text, 8.5, DIM);
}
