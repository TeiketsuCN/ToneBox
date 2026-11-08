//! 详情栏：标签、技术参数、质量评估

#![allow(unused_imports)]

use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use egui::{Align, Align2, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};
use std::path::{Path, PathBuf};
use crate::*;
use crate::util::*;
use crate::badges::*;
use crate::titlebar::*;
use crate::rail::*;
use crate::queue::*;
use crate::player::*;
use crate::spectrum::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

impl App {

    pub(crate) fn detail(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let mut animating = false;

        // 切换选中项时，内容从下方滑入
        if self.detail_for != self.selected {
            self.detail_for = self.selected;
            self.detail_t = 0.0;
        }
        animating |= approach(&mut self.detail_t, 1.0, self.dt, 12.0);
        let t = self.detail_t;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(ui.available_height() - BAR_RESERVE)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                animating |= self.detail_body(ui, t);
            });

        animating
    }

    pub(crate) fn detail_body(&mut self, ui: &mut egui::Ui, t: f32) -> bool {
        let dt = self.dt;
        let mut animating = false;
        ui.set_width(ui.available_width());

        // 切换选中项时，内容从下方滑入 + 淡入
        if self.detail_for != self.selected {
            self.detail_for = self.selected;
            self.detail_t = 0.0;
        }
        animating |= approach(&mut self.detail_t, 1.0, dt, 12.0);
        let t = self.detail_t;
        ui.add_space(16.0 * (1.0 - t));

        let Some(it) = self.items.get(self.selected) else {
            ui.label(RichText::new("详情").size(10.5).color(DIM));
            ui.add_space(10.0);
            para(ui, "拖入文件后，这里显示源文件的真实参数、质量判断和解释。", 10.5, DIM);
            return animating;
        };

        let fade = |c: Color32| c.gamma_multiply(t);

        ui.label(RichText::new("详情").size(10.5).color(fade(DIM)));
        ui.add_space(8.0);
        let disp = it.tags.display_title(&it.name());
        ui.add(
            egui::Label::new(RichText::new(&disp).size(12.5).color(fade(TEXT)).strong())
                .wrap_mode(egui::TextWrapMode::Truncate),
        );
        let sub = it.tags.display_sub();
        if !sub.is_empty() {
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(RichText::new(&sub).size(9.5).color(fade(MUTED)))
                    .wrap_mode(egui::TextWrapMode::Truncate),
            );
        }
        // 文件名单独一行（老 .oma 的文件名本身可能就是线索）
        if disp != it.name() {
            ui.add_space(1.0);
            ui.add(
                egui::Label::new(
                    RichText::new(it.name()).size(9.0).color(fade(DIM)),
                )
                .wrap_mode(egui::TextWrapMode::Truncate),
            );
        }
        ui.add_space(10.0);

        ui.add_space(10.0);

        if let Some(m) = &it.info {
            ui.horizontal(|ui| {
                chip(ui, m.codec_label(), ACCENT);
                if let Some(q) = quality_of(m) {
                    ui.add_space(6.0);
                    animating |= quality_badge(ui, q, m.bitrate_kbps);
                }
            });
            ui.add_space(10.0);

            egui::Grid::new("info_grid")
                .num_columns(2)
                .spacing([12.0, 7.0])
                .show(ui, |ui| {
                    if m.bitrate_kbps > 0 {
                        row(ui, "码率", &format!("{} kbps", m.bitrate_kbps));
                    }
                    if m.sample_rate > 0 {
                        let text = if is_dsd(&m.codec) {
                            let tier = dsd_tier_label(&m.codec, m.sample_rate, m.bitrate_kbps);
                            if tier.is_empty() {
                                fmt_dsd_mhz(m.sample_rate)
                            } else {
                                format!("{tier} · {}", fmt_dsd_mhz(m.sample_rate))
                            }
                        } else {
                            format!("{} Hz", m.sample_rate)
                        };
                        row(ui, "采样率", &text);
                    }
                    if is_dsd(&m.codec) {
                        // DSD 是 1-bit 脉冲密度，ffprobe 报的 8 bit 是打包字节数
                        row(ui, "位深", "1 bit");
                    } else if let Some(b) = m.bits {
                        row(ui, "位深", &format!("{b} bit"));
                    }
                    if m.channels > 0 {
                        row(ui, "声道", &format!("{}", m.channels));
                    }
                    if m.duration_s > 0.0 {
                        row(ui, "时长", &fmt_duration(m.duration_s));
                    }
                    if m.size_bytes > 0 {
                        row(ui, "体积", &fmt_size(m.size_bytes));
                    }
                    let spec = it.spec(self.target);
                    let est = estimate_size(m, spec, &self.cfg.opts);
                    if est > 0 {
                        row(ui, "输出", &format!("{} ≈ {}", spec.name, fmt_size(est)));
                    }
                });

            // ── 质量评估（频谱分析）──
            ui.add_space(14.0);
            match &it.analysis {
                Analysis::Idle | Analysis::Running => {
                    para(ui, "正在分析频谱…", 10.0, DIM);
                }
                Analysis::Failed(e) => {
                    ui.vertical(|ui| {
                        para(ui, "频谱分析失败", 11.0, DANGER);
                        ui.add_space(4.0);
                        para(ui, e, 9.5, MUTED);
                    });
                }
                Analysis::Done(r) => {
                    if let Some(d) = &r.dsd {
                        // DSD 不走"截止频率推码率"那套：只报格式事实，别给一个假的置信度
                        para(ui, "质量评估", 10.0, DIM);
                        ui.add_space(6.0);
                        let tier = if d.tier > 0 {
                            format!("DSD{}", d.tier)
                        } else {
                            "DSD".to_string()
                        };
                        egui::Grid::new("dsd_quality_grid")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(
                                    ui,
                                    "编码",
                                    &format!("{tier} · {}", fmt_mhz(d.bit_rate_hz)),
                                );
                                row(ui, "位深", "1 bit");
                                row(ui, "判定", "无损源码");
                            });
                    } else {
                        para(ui, "质量评估", 10.0, DIM);
                        ui.add_space(6.0);
                        egui::Grid::new("quality_grid")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                row(
                                    ui,
                                    "截止频率",
                                    &format!("{:.1} kHz", r.cutoff_hz / 1000.0),
                                );
                                let est = if !r.estimate.is_empty()
                                    && r.estimate
                                        .chars()
                                        .all(|c| c.is_ascii_digit() || c == '~')
                                {
                                    format!("{} kbps", r.estimate)
                                } else {
                                    r.estimate.clone()
                                };
                                row(ui, "推测源质量", &est);

                                ui.label(RichText::new("置信度").size(9.5).color(DIM));
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(118.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                let p = ui.painter();
                                p.rect_filled(
                                    rect,
                                    Rounding::same(5.0),
                                    tint(Color32::WHITE, 14),
                                );
                                let mut f = rect;
                                f.set_width(
                                    rect.width() * (r.confidence as f32).clamp(0.0, 1.0),
                                );
                                p.rect_filled(
                                    f,
                                    Rounding::same(5.0),
                                    if r.confidence > 0.6 { ACCENT } else { WARN },
                                );
                                ui.label(
                                    RichText::new(format!("{:.0}%", r.confidence * 100.0))
                                        .size(9.0)
                                        .color(DIM),
                                );
                                ui.end_row();
                                if let Some(l) = it.lufs {
                                    row(ui, "响度", &format!("{:.1} LUFS", l.lufs));
                                }
                            });

                        // 置信度是怎么算出来的：一条条列出来，别只给个数字
                        if !r.confidence_notes.is_empty() {
                            ui.add_space(6.0);
                            for n in &r.confidence_notes {
                                ui.horizontal(|ui| {
                                    if let Some((pct, good)) = n.delta {
                                        let (sym, col) = if good {
                                            ("+", Color32::from_rgb(110, 220, 140))
                                        } else {
                                            ("−", Color32::from_rgb(230, 120, 110))
                                        };
                                        ui.label(
                                            RichText::new(format!("{sym}{pct:.0}%"))
                                                .size(9.5)
                                                .color(col)
                                                .strong(),
                                        );
                                        ui.add_space(6.0);
                                    } else {
                                        ui.add_space(30.0);
                                    }
                                    para(ui, &n.reason, 9.0, tint(TEXT, 195));
                                });
                                ui.add_space(1.0);
                            }
                        }
                    }

                    if !r.caveats.is_empty() {
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            for c in &r.caveats {
                                para(ui, &format!("· {c}"), 9.5, tint(TEXT, 200));
                                ui.add_space(2.0);
                            }
                        });
                    }
                }
            }

            // 源 → 目标 的质量提示不再堆在详情页底部：改成挂在输出列对应控件的下方
            // （容器/格式提示挂在输出标题下，码率提示挂在码率旋钮下，升采样挂在采样率下）。
        }

        if let Some(enc) = &it.encrypted {
            // 加密格式：别只丢一句"读不出来"，给出平台、置信度、判断依据和合法出路
            ui.add_space(14.0);
            para(ui, "加密格式", 10.0, DANGER);
            ui.add_space(6.0);
            egui::Grid::new("enc_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                row(ui, "平台", enc.platform);
                row(ui, "格式", enc.format);
                ui.label(RichText::new("置信度").size(10.0).color(DIM));
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(150.0, 12.0), egui::Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, Rounding::same(6.0), tint(Color32::WHITE, 14));
                let mut f = rect;
                f.set_width(rect.width() * enc.confidence.clamp(0.0, 1.0));
                p.rect_filled(
                    f,
                    Rounding::same(6.0),
                    if enc.confidence > 0.8 { ACCENT } else { WARN },
                );
                ui.label(
                    RichText::new(format!("{:.0}%", enc.confidence * 100.0))
                        .size(9.0)
                        .color(DIM),
                );
                ui.end_row();
            });
            ui.add_space(6.0);
            for ev in &enc.evidence {
                para(ui, &format!("· {ev}"), 9.0, tint(TEXT, 190));
                ui.add_space(1.0);
            }
            ui.add_space(4.0);
            para(ui, enc.hint, 9.5, MUTED);
            ui.add_space(4.0);
            para(ui, enc.advice, 9.5, MUTED);
        } else if let Some(e) = &it.error {
            ui.add_space(14.0);
            ui.vertical(|ui| {
                para(ui, "读不出来", 11.5, DANGER);
                ui.add_space(5.0);
                para(ui, e, 10.0, MUTED);
            });
        }

        // 完成/失败不再在详情底部堆卡片：状态在队列行（✓ / ✗），通知进标题栏铃铛

        // 选中即分析（结果进缓存，切回来不重跑）
        if self.cfg.auto_analyze
            && self
                .items
                .get(self.selected)
                .map(|i| matches!(i.analysis, Analysis::Idle) && i.info.is_some())
                .unwrap_or(false)
        {
            self.start_analyze(self.selected, ui.ctx());
        }

        animating
    }

    /// 当前参数的一句话摘要（"8 · 跟源" 这种），显示在输出列标题下
    pub(crate) fn knob_summary(&self) -> String {        use fc_core::Knob;
        let o = &self.cfg.opts;
        let mut parts: Vec<String> = Vec::new();
        for k in fc_core::knobs_for(self.target) {
            match k {
                Knob::FlacLevel => parts.push(format!("{}", o.flac_level)),
                Knob::BitDepth => parts.push(o.bit_depth.label().to_string()),
                Knob::Mp3Bitrate => parts.push(format!("{}k", o.mp3_bitrate)),
                Knob::OpusBitrate => parts.push(format!("{}k", o.opus_bitrate)),
                Knob::VorbisQuality => parts.push(format!("q{}", o.vorbis_q)),
                Knob::AacBitrate => parts.push(format!("{}k", o.aac_bitrate)),
                Knob::Ac3Bitrate => parts.push(format!("{}k", o.ac3_bitrate)),
                Knob::Eac3Bitrate => parts.push(format!("{}k", o.eac3_bitrate)),
                Knob::WmaBitrate => parts.push(format!("{}k", o.wma_bitrate)),
                Knob::Atrac3Bitrate => parts.push(format!("{}k", o.atrac3_bitrate)),
                Knob::SampleRate => match fc_core::output_rate(self.target, o) {
                    Some(r) => parts.push(fmt_rate(r)),
                    None => parts.push("跟源".into()),
                },
            }
        }
        if parts.is_empty() {
            // 没有旋钮的格式（ATRAC1 / ATRAC3+）也把固定码率写出来，
            // 否则标题只有"默认"，用户不知道会转成多少
            match fc_core::bitrate_of(self.target, o) {
                Some(k) => format!("{k}k"),
                None => "默认".into(),
            }
        } else {
            parts.join(" · ")
        }
    }

}
