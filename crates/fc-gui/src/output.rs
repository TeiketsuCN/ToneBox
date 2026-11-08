//! 输出栏：预设、参数旋钮、输出位置、元数据

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
use crate::detail::*;
use crate::library::*;
use crate::overlays::*;

/// 输出栏所有行的标签列宽：控件都从这条竖线开始，看起来才齐。
const LABEL_W: f32 = 58.0;

/// 输出栏的一行：固定宽度标签 + 右侧控件
pub(crate) fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_W, 22.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new(label).size(10.0).color(MUTED));
            },
        );
        add(ui);
    });
}

/// 缩进到控件列的提示/警告（和上面的控件对齐，不破坏整列的竖线）
pub(crate) fn hint_below(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W);
        out_hint(ui, text, color);
    });
}

impl App {

    /// 最右一列：输出设置。不属于某个文件，是全局的。
    pub(crate) fn output_panel(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let mut changed = false;

        ui.label(RichText::new("输出").size(11.0).color(DIM));
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{} · {}", self.target.name, self.knob_summary()))
                .size(12.0)
                .color(accent()),
        );
        // 视频容器：先把「这个容器收什么」说一句，再看下面具体旋钮
        if fc_core::is_video(self.target) {
            out_hint(ui, fc_core::container_note(self.target.id), DIM);
        }

        // 选中文件 → 当前目标的质量提示。提示挂在触发它的控件下面：
        // 容器/格式类在标题下，码率类在码率旋钮下，升采样在采样率下。
        let warnings: Vec<fc_core::QualityWarning> = self
            .items
            .get(self.selected)
            .and_then(|i| {
                i.info.as_ref().map(|m| {
                    fc_core::quality_warnings(m, i.spec(self.target), &self.cfg.opts)
                })
            })
            .unwrap_or_default();
        for w in warnings
            .iter()
            .filter(|w| w.kind == fc_core::WarningKind::Container)
        {
            out_hint(ui, &w.body, WARN);
        }
        ui.add_space(12.0);

        if false {
            return changed;
        }

        ui.add_space(6.0);

        // 预设
        ui.label(RichText::new("预设").size(9.0).color(DIM));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
            for p in fc_core::PRESETS {
                let on = self.target.id == p.format;
                let b = egui::Button::new(
                    RichText::new(p.name).size(9.5).color(if on {
                        accent()
                    } else {
                        MUTED
                    }),
                )
                .fill(if on {
                    tint(accent(), 20)
                } else {
                    tint(Color32::WHITE, 8)
                })
                .stroke(if on {
                    line(accent(), 45)
                } else {
                    line(Color32::WHITE, 16)
                })
                .rounding(Rounding::same(7.0));
                if ui.add(b).on_hover_text(p.note).clicked() {
                    if let Some(f) = fc_core::by_id(p.format) {
                        self.target = f;
                    }
                    (p.apply)(&mut self.cfg.opts);
                    changed = true;
                }
            }

            // ── 用户自设预设（跟着配置 1/2/3 走）──
            let mut apply_preset: Option<usize> = None;
            let mut rename_preset: Option<usize> = None;
            let mut delete_preset: Option<usize> = None;
            for (i, p) in self.cfg.presets.iter().enumerate() {
                let on = self.target.id == p.format && self.cfg.opts == p.opts;
                let b = egui::Button::new(
                    RichText::new(&p.name).size(9.5).color(if on {
                        accent()
                    } else {
                        MUTED
                    }),
                )
                .fill(if on {
                    tint(accent(), 20)
                } else {
                    tint(Color32::WHITE, 8)
                })
                .stroke(if on {
                    line(accent(), 45)
                } else {
                    line(Color32::WHITE, 16)
                })
                .rounding(Rounding::same(7.0));
                let resp = ui
                    .add(b)
                    .on_hover_text("点一下套用这套参数；右键改名 / 删除");
                if resp.clicked() {
                    apply_preset = Some(i);
                }
                resp.context_menu(|ui| {
                    if menu_btn(ui, "重命名") {
                        rename_preset = Some(i);
                        ui.close_menu();
                    }
                    if menu_btn(ui, "删除") {
                        delete_preset = Some(i);
                        ui.close_menu();
                    }
                });
            }
            // 正在改名：就地换成输入框
            if let Some(i) = self.preset_rename {
                if i < self.cfg.presets.len() {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.preset_rename_buf)
                            .hint_text(self.cfg.presets[i].name.clone())
                            .desired_width(96.0)
                            .margin(egui::vec2(6.0, 3.0)),
                    );
                    if self.preset_rename_focus {
                        resp.request_focus();
                        self.preset_rename_focus = false;
                    }
                    if resp.lost_focus() || ui.input(|x| x.key_pressed(egui::Key::Enter)) {
                        let name = self.preset_rename_buf.trim().to_string();
                        if !name.is_empty() {
                            self.cfg.presets[i].name = name;
                        }
                        self.preset_rename = None;
                        self.preset_rename_buf.clear();
                        changed = true;
                    }
                } else {
                    self.preset_rename = None;
                }
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("＋ 存为预设").size(9.5).color(accent()))
                        .fill(tint(accent(), 14))
                        .stroke(line(accent(), 45))
                        .rounding(Rounding::same(7.0)),
                )
                .on_hover_text("把当前格式和参数存成一套，可改名，跟着「配置」一起保存")
                .clicked()
            {
                let n = self.cfg.presets.len() + 1;
                self.cfg.presets.push(settings::UserPreset {
                    name: format!("预设 {n}"),
                    format: self.target.id.to_string(),
                    opts: self.cfg.opts.clone(),
                });
                changed = true;
            }
            if let Some(i) = apply_preset {
                if let Some(p) = self.cfg.presets.get(i).cloned() {
                    if let Some(f) = fc_core::by_id(&p.format) {
                        self.target = f;
                    }
                    self.cfg.opts = p.opts;
                    changed = true;
                }
            }
            if let Some(i) = rename_preset {
                self.preset_rename = Some(i);
                self.preset_rename_buf.clear();
                self.preset_rename_focus = true;
            }
            if let Some(i) = delete_preset {
                if i < self.cfg.presets.len() {
                    self.cfg.presets.remove(i);
                    self.preset_rename = None;
                    changed = true;
                }
            }
        });

        ui.add_space(8.0);

        // 参数
        // 整列的行距和按钮内边距统一，别让每行看起来高度不一样
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 10.0);
        ui.spacing_mut().button_padding = egui::vec2(9.0, 3.0);
        let knobs = fc_core::knobs_for(self.target);
        if knobs.is_empty() {
            para(ui, "这个格式没有可调参数。", 9.0, DIM);
        }
        let mut shown: Vec<fc_core::WarningKind> = vec![fc_core::WarningKind::Container];
        for k in knobs {
            changed |= self.knob_row(ui, *k);
            // 挂在旋钮下面的提示：码率类挂码率，采样率方向挂采样率，位深挂位深
            let here: &[fc_core::WarningKind] = match k {
                fc_core::Knob::SampleRate => &[
                    fc_core::WarningKind::Upsample,
                    fc_core::WarningKind::Downsample,
                ],
                fc_core::Knob::BitDepth => &[
                    fc_core::WarningKind::BitDepthDown,
                    fc_core::WarningKind::BitDepthUp,
                ],
                fc_core::Knob::Mp3Bitrate
                | fc_core::Knob::OpusBitrate
                | fc_core::Knob::VorbisQuality
                | fc_core::Knob::AacBitrate
                | fc_core::Knob::Ac3Bitrate
                | fc_core::Knob::Eac3Bitrate
                | fc_core::Knob::WmaBitrate
                | fc_core::Knob::Atrac3Bitrate => &[
                    fc_core::WarningKind::Lossy,
                    fc_core::WarningKind::LowerBitrate,
                    fc_core::WarningKind::HigherBitrate,
                ],
                _ => &[],
            };
            for kind in here {
                for w in warnings.iter().filter(|w| w.kind == *kind) {
                    hint_below(ui, &w.body, WARN);
                }
            }
            // 采样率方向已经有具体提示了，就别再重复一行通用说明
            if *k == fc_core::Knob::SampleRate
                && !warnings.iter().any(|w| {
                    matches!(
                        w.kind,
                        fc_core::WarningKind::Upsample | fc_core::WarningKind::Downsample
                    )
                })
            {
                // 只在真的升/降采样时由质量提示出警报；不再常驻一句说明
            }
            shown.extend_from_slice(here);
        }
        // 兜底：这个格式没有对应旋钮时，别把提示悄悄吞掉
        for w in warnings.iter().filter(|w| !shown.contains(&w.kind)) {
            hint_below(ui, &w.body, WARN);
        }
        if fc_core::is_video(self.target) {
            hint_below(ui, fc_core::video_knob_note(self.target.id), DIM);
        }
        // 外部编码器：让用户知道这一步不是 ffmpeg 干的
        if let Some(name) = fc_core::external_encoder(self.target.id) {
            ui.add_space(4.0);
            para(
                ui,
                &format!("由外部编码器 {name} 完成（不随包，需你自己安装）。"),
                8.5,
                DIM,
            );
        }

        // 视频容器的音轨方案：能复制就复制，不能就得按容器规矩重编——
        // 不写出来用户会以为"只是换了容器，声音没动"。
        if fc_core::is_video(self.target) {
            let src_audio = self
                .items
                .get(self.selected)
                .and_then(|i| i.info.as_ref())
                .map(|m| m.codec.clone())
                .unwrap_or_default();
            let plan = fc_core::audio_plan(self.target.id, &src_audio);
            field_row(ui, "音频", |ui| {
                // 纯视频文件没有音轨：写"无音轨"，别假装会转出一条 FLAC 900k
                let text = if src_audio.is_empty() {
                    "无音轨（纯视频）".to_string()
                } else {
                    plan.label()
                };
                ui.label(RichText::new(text).size(10.0).color(TEXT));
            });
            if !src_audio.is_empty() {
                if let fc_core::AudioPlan::Encode { label, .. } = plan {
                    hint_below(
                        ui,
                        &format!(
                            "源的 {} 不在这个容器支持的音频编码里，音轨会转成 {}。",
                            self.items
                                .get(self.selected)
                                .and_then(|i| i.info.as_ref())
                                .map(|m| m.codec_label().to_string())
                                .unwrap_or_else(|| src_audio.clone()),
                            label
                        ),
                        WARN,
                    );
                }
            }
        }

        // 响度归一化：和旋钮分开一行，因为它是"处理"不是"编码参数"
        field_row(ui, "响度", |ui| {
            let opts: [(&str, Option<f32>); 4] = [
                ("关", None),
                ("-14", Some(-14.0)),
                ("-16", Some(-16.0)),
                ("-23", Some(-23.0)),
            ];
            for (label, target) in opts {
                let on = self.cfg.loudness == target;
                if seg_button(ui, label, on) {
                    self.cfg.loudness = target;
                    changed = true;
                }
            }
        });
        if self.cfg.loudness.is_some() {
            hint_below(ui, "按 EBU R128 测整合响度，转换时补增益（峰值留 1 dB 余量）。", DIM);
            ui.add_space(4.0);
            self.loudness_preview(ui);
        }

        ui.add_space(14.0);
        ui.label(RichText::new("输出到").size(9.0).color(DIM));
        ui.add_space(4.0);
        if radio_dot(
            ui,
            "源文件同目录",
            self.cfg.output_mode == settings::OutputMode::Source,
        ) {
            self.cfg.output_mode = settings::OutputMode::Source;
            changed = true;
        }
        if radio_dot(
            ui,
            "指定目录",
            self.cfg.output_mode == settings::OutputMode::Custom,
        ) {
            self.cfg.output_mode = settings::OutputMode::Custom;
            changed = true;
        }
        if self.cfg.output_mode == settings::OutputMode::Custom {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 58.0).max(56.0);
                let mut dir = self.cfg.output_dir.clone();
                // 自绘一个和整体风格一致的输入框（egui 默认那个底和字色都太跳）
                let mut edited = false;
                egui::Frame::none()
                    .fill(tint(Color32::WHITE, 8))
                    .stroke(line(Color32::WHITE, 18))
                    .rounding(Rounding::same(7.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 5.0))
                    .show(ui, |ui| {
                        ui.set_width(w - 16.0);
                        edited = ui
                            .add(
                                egui::TextEdit::singleline(&mut dir)
                                    .frame(false)
                                    .text_color(MUTED)
                                    .hint_text(RichText::new("选一个目录").size(10.0).color(DIM)),
                            )
                            .changed();
                    });
                if edited {
                    self.cfg.output_dir = dir;
                    changed = true;
                }
                let browse = egui::Button::new(RichText::new("浏览").size(10.0).color(MUTED))
                    .fill(Color32::TRANSPARENT)
                    .stroke(line(Color32::WHITE, 20))
                    .rounding(Rounding::same(7.0));
                if ui.add_sized([52.0, 26.0], browse).clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.output_dir = p.to_string_lossy().to_string();
                        changed = true;
                    }
                }
            });
        }

        ui.add_space(12.0);
        ui.label(RichText::new("转换后").size(9.0).color(DIM));
        ui.add_space(4.0);
        if radio_dot(ui, "保留源文件", !self.delete_source) {
            self.delete_source = false;
            changed = true;
        }
        if self.delete_source_row(ui) {
            changed = true;
        }
        if self.delete_source {
            ui.add_space(2.0);
            para(ui, "删了就找不回来了，谨慎。", 8.5, DANGER);
        }

        ui.add_space(14.0);
        // 折叠头：整行可点，右侧画三角
        let (hrect, hresp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 24.0),
            egui::Sense::click(),
        );
        let hp = ui.painter();
        if hresp.hovered() {
            hp.rect_filled(hrect, Rounding::same(6.0), tint(Color32::WHITE, 10));
        }
        hp.text(
            hrect.left_center() + egui::vec2(2.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "元数据",
            egui::FontId::proportional(9.5),
            MUTED,
        );
        // 收起时显示已填了几项，不用展开也知道有没有设过
        let filled = [
            self.cfg.meta_artist.trim(),
            self.cfg.meta_album.trim(),
            self.cfg.meta_cover.trim(),
            self.items
                .get(self.selected)
                .and_then(|i| i.title_ov.as_deref())
                .unwrap_or(""),
        ]
        .iter()
        .filter(|v| !v.is_empty())
        .count();
        if filled > 0 {
            hp.text(
                hrect.left_center() + egui::vec2(44.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{} 项已填", filled),
                egui::FontId::proportional(9.0),
                accent(),
            );
        }
        let tc = hrect.right_center() - egui::vec2(9.0, 0.0);
        let tri = if self.meta_open {
            vec![
                tc + egui::vec2(-4.0, 2.0),
                tc + egui::vec2(4.0, 2.0),
                tc + egui::vec2(0.0, -3.0),
            ]
        } else {
            vec![
                tc + egui::vec2(-4.0, -2.0),
                tc + egui::vec2(4.0, -2.0),
                tc + egui::vec2(0.0, 3.0),
            ]
        };
        hp.add(egui::Shape::convex_polygon(tri, DIM, Stroke::NONE));
        if hresp.clicked() {
            self.meta_open = !self.meta_open;
        }
        ui.add_space(2.0);

        if self.meta_open {
            ui.horizontal(|ui| {
                if radio_dot_inline(ui, "自动", !self.cfg.write_tags) {
                    self.cfg.write_tags = false;
                    changed = true;
                }
                ui.add_space(10.0);
                if radio_dot_inline(ui, "写入标签", self.cfg.write_tags) {
                    self.cfg.write_tags = true;
                    changed = true;
                }
                ui.add_space(10.0);
                if self.clear_meta_button(ui) {
                    changed = true;
                }
            });
        }

        if self.meta_open {
            let sel = self.selected;
            let has_file = self.items.get(sel).is_some();
            // 写入标签关着时字段照样显示（用户要求"元数据全展开"），
            // 只是提醒一句：这些值不写进文件。
            if !self.cfg.write_tags {
                ui.add_space(4.0);
                para(
                    ui,
                    "自动：沿用源文件的标签和封面，不改写。要统一改名/换封面就选「写入标签」。",
                    8.5,
                    DIM,
                );
                return changed;
            }

            // ── 当前文件（只影响这一个）──
            if has_file {
                let mut title = self
                    .items
                    .get(sel)
                    .and_then(|i| i.title_ov.clone())
                    .unwrap_or_default();
                ui.add_space(6.0);
                ui.label(RichText::new("标题（当前文件）").size(9.0).color(DIM));
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut title)
                            .hint_text("留空=用源文件标签")
                            .margin(egui::vec2(6.0, 4.0)),
                    )
                    .changed()
                {
                    if let Some(it) = self.items.get_mut(sel) {
                        it.title_ov = if title.trim().is_empty() {
                            None
                        } else {
                            Some(title.clone())
                        };
                    }
                    changed = true;
                }

                let mut track = self
                    .items
                    .get(sel)
                    .and_then(|i| i.track_ov.clone())
                    .unwrap_or_default();
                ui.add_space(4.0);
                ui.label(RichText::new("音轨号").size(9.0).color(DIM));
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut track)
                            .hint_text("如 03")
                            .margin(egui::vec2(6.0, 4.0)),
                    )
                    .changed()
                {
                    if let Some(it) = self.items.get_mut(sel) {
                        it.track_ov = if track.trim().is_empty() {
                            None
                        } else {
                            Some(track.clone())
                        };
                    }
                    changed = true;
                }
            }

            // ── 全批（影响所有文件）──
            ui.add_space(8.0);
            ui.label(RichText::new("艺术家（全部）").size(9.0).color(DIM));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.cfg.meta_artist)
                        .hint_text("留空=用源文件标签")
                        .margin(egui::vec2(6.0, 4.0)),
                )
                .changed()
            {
                changed = true;
            }
            ui.add_space(4.0);
            ui.label(RichText::new("专辑（全部）").size(9.0).color(DIM));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.cfg.meta_album)
                        .hint_text("留空=用源文件标签")
                        .margin(egui::vec2(6.0, 4.0)),
                )
                .changed()
            {
                changed = true;
            }

            ui.add_space(6.0);
            ui.label(RichText::new("封面（全部）").size(9.0).color(DIM));
            ui.horizontal(|ui| {
                let name = std::path::Path::new(&self.cfg.meta_cover)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "沿用源文件".into());
                ui.label(RichText::new(name).size(9.0).color(DIM));
            });
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [(ui.available_width() - 46.0).max(50.0), 22.0],
                        egui::Button::new(RichText::new("选图片…").size(9.5)),
                    )
                    .clicked()
                {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("图片", &["jpg", "jpeg", "png", "webp"])
                        .pick_file()
                    {
                        self.cfg.meta_cover = p.to_string_lossy().to_string();
                        changed = true;
                    }
                }
                if ui
                    .add_sized([42.0, 22.0], egui::Button::new(RichText::new("清除").size(9.0)))
                    .clicked()
                {
                    self.cfg.meta_cover.clear();
                    changed = true;
                }
            });
            para(ui, "封面只写进 FLAC/MP3/M4A。", 8.5, DIM);
        }

        ui.add_space(8.0);
        // 单个文件可能用 `···` 覆盖过输出格式，所以逐个按它自己的目标算，
        // 不能一律乘全局格式（否则队列里有覆盖时这个数就是错的）。
        let est = self
            .items
            .iter()
            .filter_map(|i| {
                i.info
                    .as_ref()
                    .map(|m| fc_core::estimate_size(m, i.spec(self.target), &self.cfg.opts))
            })
            .sum::<u64>();
        if est > 0 {
            para(ui, &format!("预计输出 {}", fmt_size(est)), 9.0, DIM);
        }

        // 输出列的常驻说明：视频不是转视频，只是把音轨抽出来
        // 这一列改了任何设置就落盘（以前只把 changed 当"要不要重绘"用，
        // 结果预设、码率这些改动重启就没了）
        if changed {
            self.cfg.save();
        }

        changed
    }

    /// 「响度」下面的一条结果预览：源包络 + 归一化后的包络 + -1 dBFS 峰值顶线。
    ///
    /// 为什么要画**两条**：响度归一化是整轨统一增益，波形形状不会变；
    /// 只画一条"目标波形"就是把同一条曲线放大缩小，看不出改了什么。
    /// 灰色是源、彩色是目标，顶部虚线是转换时预留的 1 dB 峰值余量——
    /// 目标包络顶到虚线，说明增益被自动下调过（再高就削波了）。
    ///
    /// 波形和响度都来自已有的后台分析/波形缓存，这里不额外解码。
    pub(crate) fn loudness_preview(&mut self, ui: &mut egui::Ui) {
        let sel = self.selected;
        let target = self.cfg.loudness.unwrap_or(0.0);

        // 没测过响度就借一次后台分析（结果会缓存，切回来不重跑）。
        // 即使用户关掉了「自动分析」也该测：他刚打开响度归一化，
        // 不测就没法告诉他会被加/减多少 dB、会不会顶到峰值余量。
        let need = self
            .items
            .get(sel)
            .map(|it| {
                it.info.as_ref().map(|m| m.sample_rate > 0).unwrap_or(false)
                    && it.lufs.is_none()
                    && matches!(it.analysis, Analysis::Idle)
            })
            .unwrap_or(false);
        if need {
            self.start_analyze(sel, ui.ctx());
        }

        let Some(it) = self.items.get(sel) else {
            return;
        };
        let Some(m) = &it.info else { return };
        if m.sample_rate == 0 || it.encrypted.is_some() {
            return;
        }
        let wave = match &self.wave {
            Some((p, w)) if *p == it.path => Some(w),
            _ => None,
        };
        let Some(l) = it.lufs else {
            // 分析失败（比如样本太短）就别一直显示"测量中"
            let text = if matches!(it.analysis, Analysis::Failed(_)) {
                "测不出响度（样本太短或解码失败）"
            } else {
                "响度测量中…"
            };
            ui.label(RichText::new(text).size(8.5).color(DIM));
            return;
        };
        if !l.lufs.is_finite() || !l.peak_dbfs.is_finite() {
            ui.label(
                RichText::new("测不出响度（样本太短或几乎全是静音）")
                    .size(8.5)
                    .color(DIM),
            );
            return;
        }

        // 算法必须和 start_batch 里的一模一样，否则预览会和实际结果对不上
        let raw_gain = target - l.lufs;
        let ceiling = -1.0 - (l.peak_dbfs + raw_gain);
        let limited = ceiling < 0.0;
        let gain = (raw_gain + ceiling.min(0.0)).clamp(-30.0, 30.0);

        // 说明这是"当前选中文件"的预览：响度是全局设置，但每条素材要补的增益不同
        ui.label(
            RichText::new("响度预览（当前文件）")
                .size(8.0)
                .color(DIM),
        );
        ui.add_space(2.0);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 46.0),
            egui::Sense::hover(),
        );
        let p = ui.painter();
        p.rect_filled(rect, Rounding::same(8.0), tint(Color32::WHITE, 6));
        let inner = rect.shrink2(egui::vec2(6.0, 4.0));
        let cy = inner.center().y;
        // 上下各留 2px，±1.0 就是条带的上下边（0 dBFS）
        let half = (inner.height() * 0.5 - 2.0).max(2.0);

        // -1 dBFS 顶线（转换时峰值不会越过它，除非增益被 ±30 dB 夹住）
        let ceil_amp = 10f32.powf(-1.0 / 20.0);
        let ceil_y = cy - half * ceil_amp;
        let mut dx = inner.left();
        while dx < inner.right() {
            let x2 = (dx + 5.0).min(inner.right());
            p.line_segment(
                [egui::pos2(dx, ceil_y), egui::pos2(x2, ceil_y)],
                Stroke::new(1.0, tint(WARN, 85)),
            );
            dx += 9.0;
        }

        if let Some(chans) = wave {
            let n = chans.iter().map(|c| c.len()).max().unwrap_or(0);
            if n > 0 {
                let scale = 10f32.powf(gain / 20.0);
                // 260px 宽放 480 个桶纯属浪费，抽稀到"一像素一根"
                let step = ((n as f32) / inner.width().max(1.0)).ceil().max(1.0) as usize;
                for i in (0..n).step_by(step) {
                    let (mut mn, mut mx) = (0f32, 0f32);
                    for ch in chans {
                        if let Some(&(a, b)) = ch.get(i) {
                            mn = mn.min(a);
                            mx = mx.max(b);
                        }
                    }
                    let x = inner.left() + inner.width() * (i as f32 + 0.5) / n as f32;
                    // 源包络：灰色，作为"改之前"的参照
                    p.line_segment(
                        [
                            egui::pos2(x, cy - mx.clamp(-1.0, 1.0) * half),
                            egui::pos2(x, cy - mn.clamp(-1.0, 1.0) * half),
                        ],
                        Stroke::new(1.0, tint(Color32::WHITE, 40)),
                    );
                    // 目标包络：颜色跟着强调色；超出 0 dBFS 的部分压在边上
                    let omx = (mx * scale).clamp(-1.0, 1.0);
                    let omn = (mn * scale).clamp(-1.0, 1.0);
                    p.line_segment(
                        [
                            egui::pos2(x, cy - omx * half),
                            egui::pos2(x, cy - omn * half),
                        ],
                        Stroke::new(1.3, accent()),
                    );
                }
            }
        } else {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "波形生成中…",
                FontId::proportional(9.0),
                DIM,
            );
        }

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{:.1} → {:.0} LUFS", l.lufs, target))
                    .size(8.5)
                    .color(DIM),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("增益 {:+.1} dB", gain))
                        .size(8.5)
                        .color(if limited { WARN } else { TEXT }),
                );
            });
        });
        if limited {
            out_hint(ui, "峰值会顶到 -1 dBFS：增益已自动下调，不会削波", WARN);
        } else if gain.abs() >= 0.05 {
            out_hint(
                ui,
                &format!("输出峰值 {:.1} dBFS（预留 1 dB 余量）", l.peak_dbfs + gain),
                DIM,
            );
        } else {
            out_hint(ui, "已经是目标响度，不用补增益", DIM);
        }
    }


    /// 「清除所有」：长按 3 秒清掉批量元数据和当前文件的覆盖值
    pub(crate) fn clear_meta_button(&mut self, ui: &mut egui::Ui) -> bool {
        let dt = self.dt;
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(76.0, 22.0),
            egui::Sense::click_and_drag(),
        );
        let holding = resp.is_pointer_button_down_on();
        if holding {
            self.meta_clear_hold = (self.meta_clear_hold + dt).min(3.0);
        } else {
            self.meta_clear_hold = 0.0;
        }
        let p = ui.painter();
        let c = egui::pos2(rect.left() + 8.0, rect.center().y);
        // 和「删除源文件」同一套动画
        hold_ring(
            p,
            c,
            6.0,
            self.meta_clear_hold / 3.0,
            WARN,
            tint(Color32::WHITE, 70),
        );
        p.text(
            egui::pos2(c.x + 14.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "清除所有",
            egui::FontId::proportional(10.0),
            if holding { WARN } else { MUTED },
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if holding && self.meta_clear_hold >= 3.0 {
            self.meta_clear_hold = 0.0;
            self.cfg.meta_artist.clear();
            self.cfg.meta_album.clear();
            self.cfg.meta_cover.clear();
            if let Some(it) = self.items.get_mut(self.selected) {
                it.title_ov = None;
                it.track_ov = None;
            }
            return true;
        }
        false
    }

    /// 「删除源文件」不是普通单选：必须长按 3 秒才生效。
    /// 按住时右侧走倒计时、圆点转一圈把进度画出来，松手就取消。
    pub(crate) fn delete_source_row(&mut self, ui: &mut egui::Ui) -> bool {
        let dt = self.dt;
        let selected = self.delete_source;
        let mut changed = false;
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 22.0),
            egui::Sense::click_and_drag(),
        );
        let holding = resp.is_pointer_button_down_on() && !selected;

        if holding {
            self.delete_hold = (self.delete_hold + dt).min(3.0);
        } else if self.delete_hold > 0.0 {
            self.delete_hold = 0.0;
        }
        if holding && self.delete_hold >= 3.0 {
            self.delete_source = true;
            self.delete_hold = 0.0;
            changed = true;
        }

        let p = ui.painter();
        let c = egui::pos2(rect.left() + 8.0, rect.center().y);
        let r = 6.0;
        let ring_col = if selected { tint(DANGER, 210) } else { WARN };
        hold_ring(p, c, r, self.delete_hold / 3.0, ring_col, tint(Color32::WHITE, 70));
        if !holding && selected {
            p.circle_filled(c, r * 0.55, DANGER);
        }
        p.text(
            egui::pos2(c.x + r + 7.0, c.y),
            egui::Align2::LEFT_CENTER,
            "删除源文件",
            egui::FontId::proportional(10.0),
            if selected { DANGER } else { MUTED },
        );
        let hint = if selected {
            "已选择".to_string()
        } else if holding {
            format!("{:.1}s", (3.0 - self.delete_hold).max(0.0))
        } else {
            "长按 3 秒".to_string()
        };
        p.text(
            egui::pos2(rect.right() - 2.0, c.y),
            egui::Align2::RIGHT_CENTER,
            hint,
            egui::FontId::proportional(9.0),
            if selected {
                DANGER
            } else if holding {
                WARN
            } else {
                DIM
            },
        );
        if holding {
            ui.ctx().request_repaint();
        }
        changed
    }

    /// 一个参数控件（紧凑版，300px 宽的面板里能用）
    pub(crate) fn knob_row(&mut self, ui: &mut egui::Ui, k: fc_core::Knob) -> bool {
        use fc_core::{BitDepth, Knob};
        let mut changed = false;
        match k {
            Knob::FlacLevel => field_row(ui, "压缩等级", |ui| {
                let mut v = self.cfg.opts.flac_level as i32;
                if level_slider_inline(ui, &mut v, 0, 12) {
                    self.cfg.opts.flac_level = v as u8;
                    changed = true;
                }
            }),
            Knob::VorbisQuality => field_row(ui, "质量", |ui| {
                let mut v = self.cfg.opts.vorbis_q as i32;
                if level_slider_inline(ui, &mut v, 0, 10) {
                    self.cfg.opts.vorbis_q = v as u8;
                    changed = true;
                }
            }),
            Knob::BitDepth => {
                let depths = fc_core::bit_depths_for(self.target);
                // 换格式后原来的位深可能不在可选范围里：落到 24（有的话）或第一项
                if !depths.is_empty() && !depths.contains(&self.cfg.opts.bit_depth) {
                    self.cfg.opts.bit_depth = if depths.contains(&BitDepth::Int24) {
                        BitDepth::Int24
                    } else {
                        depths[0]
                    };
                    changed = true;
                }
                field_row(ui, "位深", |ui| {
                    for d in depths {
                        let label = match d {
                            BitDepth::Auto => "自动",
                            BitDepth::Int16 => "16",
                            BitDepth::Int24 => "24",
                            BitDepth::Int32f => "32f",
                        };
                        let on = self.cfg.opts.bit_depth == *d;
                        if seg_button(ui, label, on) {
                            self.cfg.opts.bit_depth = *d;
                            changed = true;
                        }
                    }
                });
            }
            Knob::Mp3Bitrate => changed |= bitrate_row(ui, "mp3", &mut self.cfg.opts.mp3_bitrate),
            Knob::OpusBitrate => {
                changed |= bitrate_row(ui, "opus", &mut self.cfg.opts.opus_bitrate)
            }
            Knob::AacBitrate => changed |= bitrate_row(ui, "aac", &mut self.cfg.opts.aac_bitrate),
            Knob::Ac3Bitrate => changed |= bitrate_row(ui, "ac3", &mut self.cfg.opts.ac3_bitrate),
            Knob::Eac3Bitrate => {
                changed |= bitrate_row(ui, "eac3", &mut self.cfg.opts.eac3_bitrate)
            }
            Knob::WmaBitrate => changed |= bitrate_row(ui, "wma", &mut self.cfg.opts.wma_bitrate),
            Knob::Atrac3Bitrate => {
                changed |= bitrate_row(ui, "atrac3", &mut self.cfg.opts.atrac3_bitrate)
            }
            Knob::SampleRate => changed |= self.sample_rate_row(ui),
            // ── 视频容器 ──
            Knob::VideoCodec => {
                let choices = fc_core::video_codecs_for(self.target.id);
                // 实际编码器名要落到配置里：随包 ffmpeg 没有 libx264（授权原因），
                // 能编 H.264 的是 libopenh264；不记下来 args 里就会是个不存在的名字。
                if let Some(e) = fc_core::resolve_video_encoder(
                    self.cfg.opts.video_codec,
                    &self.encoders,
                    &self.cfg.opts.video_encoder,
                ) {
                    if e != self.cfg.opts.video_encoder {
                        self.cfg.opts.video_encoder = e.to_string();
                        changed = true;
                    }
                }
                // 「视频编码」放不下在 field_row 的标签列里（5 个按钮），单独占一行
                ui.label(RichText::new("视频编码").size(10.0).color(MUTED));
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    for c in choices {
                        // 复制永远可用；真编码要看这个 ffmpeg 构建里有没有
                        let enc_ok = *c == fc_core::VideoCodec::Copy
                            || fc_core::resolve_video_encoder(*c, &self.encoders, "").is_some();
                        let on = self.cfg.opts.video_codec == *c;
                        if seg_button_state(ui, c.short(), on, enc_ok) {
                            self.cfg.opts.video_codec = *c;
                            if let Some(e) =
                                fc_core::resolve_video_encoder(*c, &self.encoders, "")
                            {
                                self.cfg.opts.video_encoder = e.to_string();
                            }
                            changed = true;
                        }
                    }
                });
            }
            Knob::VideoQuality => field_row(ui, "质量", |ui| {
                let mut v = self.cfg.opts.video_crf as i32;
                if level_slider_inline(ui, &mut v, 16, 32) {
                    self.cfg.opts.video_crf = v as u8;
                    changed = true;
                }
            }),
            Knob::VideoScale => field_row(ui, "分辨率", |ui| {
                // 0 = 保持；只给高度，宽按比例自动算
                for (label, h) in [("保持", 0u32), ("1080p", 1080), ("720p", 720), ("480p", 480)] {
                    let on = self.cfg.opts.video_height == h;
                    if seg_button(ui, label, on) {
                        self.cfg.opts.video_height = h;
                        changed = true;
                    }
                }
            }),
            Knob::VideoFps => field_row(ui, "帧率", |ui| {
                for (label, f) in [("保持", 0u32), ("24", 24), ("30", 30), ("60", 60)] {
                    let on = self.cfg.opts.video_fps == f;
                    if seg_button(ui, label, on) {
                        self.cfg.opts.video_fps = f;
                        changed = true;
                    }
                }
            }),
        }
        changed
    }

    /// 采样率旋钮：一个条 + 单独的「自动」按钮。档位表来自 core（`rate_choices`），
    /// 界面不判断哪个编码器支持什么。
    pub(crate) fn sample_rate_row(&mut self, ui: &mut egui::Ui) -> bool {
        let choices = fc_core::rate_choices(self.target.id);
        let mut changed = false;
        let auto_on = self.cfg.opts.sample_rate.is_none();
        let src_rate = self
            .items
            .get(self.selected)
            .and_then(|i| i.info.as_ref().map(|m| m.sample_rate))
            .unwrap_or(0);
        // 配置里可能是别的格式留下的 192k，但当前格式只支持到 48k——
        // 显示和落点都按 core 吸附后的实际输出值来，别把非法档位画在条上。
        let effective = fc_core::output_rate(self.target, &self.cfg.opts);
        let shown = if auto_on {
            effective.or_else(|| {
                if choices.contains(&src_rate) {
                    Some(src_rate)
                } else {
                    None
                }
            })
        } else {
            effective
        };
        field_row(ui, "采样率", |ui| {
            // 「自动」永远在第一个位置，不按状态显示/隐藏——
            // 藏起来的话，点完它自己消失，看起来就像 bug（AC-3 上踩过）。
            if seg_button(ui, "自动", auto_on) {
                self.cfg.opts.sample_rate = None;
                changed = true;
            }
            if let Some(v) = rate_slider(ui, choices, shown, !auto_on) {
                self.cfg.opts.sample_rate = Some(v);
                changed = true;
            }
            let text = match (auto_on, shown) {
                (true, _) => "自动".to_string(),
                (false, Some(r)) => fmt_rate(r),
                (false, None) => "自动".to_string(),
            };
            ui.label(
                RichText::new(text)
                    .size(9.0)
                    .color(if auto_on { DIM } else { accent() }),
            );
        });
        changed
    }

}

/// 码率旋钮：档位表来自 core，界面不自己维护格式知识。
///
/// 顺手把「选这个值要注意什么」画在下面——AC-3 的 48kHz、WMA 的老播放器上限
/// 这类信息比一个数字有用得多。
/// 弹窗里的文字按钮：透明底，别用默认灰块
pub(crate) fn menu_btn(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(label).size(10.5).color(TEXT))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(6.0))
            .min_size(egui::vec2(96.0, 24.0)),
    )
    .clicked()
}

pub(crate) fn bitrate_row(ui: &mut egui::Ui, id: &str, val: &mut u32) -> bool {
    let mut changed = false;
    field_row(ui, "码率", |ui| {
        for &v in fc_core::bitrate_choices(id) {
            let on = *val == v;
            if seg_button(ui, &format!("{v}"), on) {
                *val = v;
                changed = true;
            }
        }
    });
    if let Some(note) = fc_core::by_id(id).and_then(fc_core::knob_note) {
        hint_below(ui, note, DIM);
    }
    changed
}
