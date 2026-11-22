//! 覆盖层：格式百科、设置页、通知面板

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
use crate::output::*;
use crate::library::*;

/// 纯色背景可选的颜色（都偏暗，保证文字对比）
pub(crate) const SOLIDS: [[u8; 3]; 10] = [
    [20, 22, 30],
    [16, 18, 20],
    [18, 26, 24],
    [28, 18, 22],
    [20, 20, 34],
    [12, 12, 14],
    [24, 18, 30],
    [14, 20, 28],
    [26, 20, 16],
    [18, 22, 26],
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    Display,
    Behavior,
    Tools,
    Storage,
    About,
}

impl SettingsTab {
    fn label(self) -> &'static str {
        match self {
            SettingsTab::Display => crate::tr!("显示"),
            SettingsTab::Behavior => crate::tr!("行为"),
            SettingsTab::Tools => crate::tr!("依赖 / 工具"),
            SettingsTab::Storage => crate::tr!("产出与存储"),
            SettingsTab::About => crate::tr!("关于"),
        }
    }
}

impl App {

    /// 格式百科：左边列表，右边长文。B 写的 29 条长文案就展示在这里。
    pub(crate) fn wiki_page(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let area = egui::Rect::from_min_max(
            egui::pos2(screen.left(), screen.top() + TITLEBAR_H),
            screen.max,
        );

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.wiki_open = false;
        }

        egui::Area::new(egui::Id::new("wiki_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min)
            // 和设置页一样：不让 Area 被 egui 上顶盖住标题栏
            .constrain(false)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(tint(BG, 255))
                    .inner_margin(egui::Margin::symmetric(28.0, 18.0))
                    .show(ui, |ui| {
                        // 内容尺寸要扣掉 Frame 的边距（28*2 / 18*2），否则 Area 会比区域大，
                        // egui 会把它往上顶到 y=0、盖住标题栏，关闭/最小化按钮就点不到了。
                        let content_w = (area.width() - 56.0).max(240.0);
                        let content_h = (area.height() - 36.0).max(220.0);
                        ui.set_min_size(egui::vec2(content_w, content_h));
                        ui.set_max_width(content_w);
                        ui.set_max_height(content_h);
                        ui.add_space(TITLEBAR_H);

                        // 顶栏：返回按钮 + 标题 + 搜索
                        ui.horizontal(|ui| {
                            let back = egui::Button::new(
                                RichText::new(crate::tr!("返回")).size(10.0).color(MUTED),
                            )
                            .fill(tint(Color32::WHITE, 8))
                            .stroke(line(Color32::WHITE, 16))
                            .rounding(Rounding::same(7.0))
                            .min_size(egui::vec2(56.0, 24.0));
                            if ui.add(back).on_hover_text(crate::tr!("回到主界面（Esc 也行）")).clicked() {
                                self.wiki_open = false;
                            }
                            ui.add_space(12.0);
                            ui.label(RichText::new(crate::tr!("格式百科")).size(15.0).color(TEXT).strong());
                            ui.add_space(16.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.wiki_filter)
                                    .hint_text(crate::tr!("搜格式名…"))
                                    .desired_width(180.0)
                                    .margin(egui::vec2(8.0, 5.0)),
                            );
                        });

                        ui.add_space(14.0);

                        let keyword = self.wiki_filter.trim().to_lowercase();
                        let list: Vec<&'static FormatSpec> = fc_core::all()
                            .filter(|f| {
                                keyword.is_empty()
                                    || f.name.to_lowercase().contains(&keyword)
                                    || f.id.contains(&keyword)
                                    || f.ext.contains(&keyword)
                            })
                            .collect();

                        let body_h = (ui.available_height() - 8.0).max(120.0);
                        let right_w = (ui.available_width() - 210.0 - 10.0).max(220.0);
                        ui.horizontal_top(|ui| {
                            // 左：列表
                            ui.allocate_ui_with_layout(
                                egui::vec2(210.0, body_h),
                                Layout::top_down(Align::LEFT),
                                |ui| {
                                ui.set_min_height(body_h);
                                egui::ScrollArea::vertical()
                                    .id_salt("wiki_list")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for f in &list {
                                            let on = f.id == self.wiki_sel;
                                            let b = egui::Button::new(
                                                RichText::new(f.name).size(11.0).color(if on {
                                                    accent()
                                                } else {
                                                    MUTED
                                                }),
                                            )
                                            .fill(if on {
                                                tint(accent(), 20)
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .stroke(Stroke::NONE)
                                            .rounding(Rounding::same(7.0))
                                            .min_size(egui::vec2(ui.available_width(), 24.0));
                                            if ui.add(b).clicked() {
                                                self.wiki_sel = f.id;
                                            }
                                        }
                                    });
                                },
                            );

                            ui.add_space(10.0);

                            // 右：详情
                            ui.allocate_ui_with_layout(
                                egui::vec2(right_w, body_h),
                                Layout::top_down(Align::LEFT),
                                |ui| {
                                ui.set_min_height(body_h);
                                egui::ScrollArea::vertical()
                                    .id_salt("wiki_body")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        // 一行太长不好读，限制在 760 左右
                                        ui.set_max_width((right_w - 18.0).min(760.0));
                                        let Some(spec) = fc_core::by_id(self.wiki_sel) else {
                                            return;
                                        };
                                        ui.label(
                                            RichText::new(crate::i18n::tr(spec.name)).size(20.0).color(TEXT).strong(),
                                        );
                                        ui.add_space(6.0);
                                        ui.horizontal(|ui| {
                                            let kind = match spec.kind {
                                                fc_core::Kind::Lossless => crate::tr!("无损"),
                                                fc_core::Kind::Lossy => crate::tr!("有损"),
                                                _ => crate::tr!("设备专用"),
                                            };
                                            ui.label(
                                                RichText::new(kind).size(9.5).color(accent()),
                                            );
                                            ui.label(RichText::new("·").size(9.5).color(DIM));
                                            ui.label(
                                                RichText::new(spec.patent.label())
                                                    .size(9.5)
                                                    .color(MUTED),
                                            );
                                            ui.label(RichText::new("·").size(9.5).color(DIM));
                                            ui.label(
                                                RichText::new(format!(".{}", spec.ext))
                                                    .size(9.5)
                                                    .color(MUTED),
                                            );
                                            ui.label(RichText::new("·").size(9.5).color(DIM));
                                            let (w, wc) = if !spec.can_encode {
                                                (crate::tr!("只能读，不能转出"), WARN)
                                            } else if self.target_available(spec) {
                                                (crate::tr!("可以转成这个"), accent())
                                            } else {
                                                (crate::tr!("要装外部编码器才能转出"), WARN)
                                            };
                                            ui.label(RichText::new(w).size(9.5).color(wc));
                                        });
                                        ui.add_space(14.0);

                                        // 百科是"挑格式"的最后一站：看完解释直接换输出目标，
                                        // 不用再回侧栏找一遍。
                                        if spec.can_encode && self.target_available(spec) {
                                            let is_target = spec.id == self.target.id;
                                            let btn = egui::Button::new(
                                                RichText::new(if is_target {
                                                    crate::fmt2!("{} 就是当前输出格式", "{} is the current output format", spec.name)
                                                } else {
                                                    crate::fmt2!("把输出格式设为 {}", "Set output format to {}", spec.name)
                                                })
                                                .size(10.5)
                                                .color(on_accent()),
                                            )
                                            .fill(if is_target {
                                                tint(accent(), 110)
                                            } else {
                                                accent()
                                            })
                                            .stroke(Stroke::NONE)
                                            .rounding(Rounding::same(9.0))
                                            .min_size(egui::vec2(0.0, 30.0));
                                            if ui
                                                .add(btn)
                                                .on_hover_text(crate::tr!("回到主界面，接下来转的文件都用这个格式"))
                                                .clicked()
                                                && !is_target
                                            {
                                                self.set_target(spec);
                                                self.wiki_open = false;
                                            }
                                            ui.add_space(4.0);
                                            para(
                                                ui,
                                                crate::tr!("选中后的码率等参数在右侧「输出」列调整。"),
                                                8.5,
                                                DIM,
                                            );
                                        } else if spec.can_encode {
                                            ui.add_space(2.0);
                                            let why = if fc_core::external_encoder(spec.id).is_some() {
                                                crate::tr!("你的系统里还没装 atracdenc（外部编码器），暂时不能转成它。装好后到设置里点「重新检测」。")
                                            } else {
                                                crate::tr!("你的 ffmpeg 里没有这个编码器，暂时不能转成它。")
                                            };
                                            card(ui, DANGER, |ui| {
                                                para(ui, why, 10.0, MUTED);
                                            });
                                        }

                                        ui.add_space(6.0);

                                        let text = fc_core::long_blurb(spec.id)
                                            .unwrap_or_else(|| crate::i18n::tr(spec.blurb));
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(text).size(11.0).color(MUTED),
                                            )
                                            .wrap_mode(egui::TextWrapMode::Wrap),
                                        );
                                    });
                                },
                            );
                        });
                    });
            });
    }

    /// 设置页：从下往上滑入的覆盖层，不弹新窗口
    /// 设置页：从下往上滑入的覆盖层，不弹新窗口。
    ///
    /// 结构：顶栏 + 标签栏 + 单列页面。每个标签调用自己的渲染函数，
    /// 不再像以前那样把「外观 / 行为 / 工具」都塞进同一页的两列里。
    pub(crate) fn settings_panel(&mut self, ctx: &egui::Context) {
        let t = self.settings_t;
        let screen = ctx.screen_rect();
        let top = screen.top() + TITLEBAR_H;
        let area = egui::Rect::from_min_max(egui::pos2(screen.left(), top), screen.max);

        let mut changed = false;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }

        egui::Area::new(egui::Id::new("settings_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min)
            // 不让 egui 自动把 Area 挪进屏幕：窗口矮的时候内容比屏幕高，
            // 默认的 constrain 会把整个 Area 往上顶到 y=0，盖住标题栏
            // （表现就是标题栏按钮点不到、但窗口还能拖；按 Esc 关掉设置就好）。
            // 内容溢出交给内部的 ScrollArea，位置永远钉在标题栏下面。
            .constrain(false)
            .show(ctx, |ui| {
                egui::Frame::none()
                    // 和主界面同一套"玻璃"：半透明底 + 壁纸透过来。
                    // 滑动过程中偏实（避免主界面从底下透上来发脏），停稳后回到玻璃感
                    .fill({
                        let k = ((t - 0.9) / 0.1).clamp(0.0, 1.0);
                        let s = k * k * (3.0 - 2.0 * k);
                        tint(BG, (255.0 - 107.0 * s) as u8)
                    })
                    .inner_margin(egui::Margin::symmetric(28.0, 18.0))
                    .show(ui, |ui| {
                        // 内容尺寸要扣掉 Frame 的边距（28*2 / 18*2）。
                        // min 和 max 都要卡：只给 min 的话，设置内容比窗口高时
                        // Area 会被撑大并往上顶、盖住标题栏。
                        let content_w = (area.width() - 56.0).max(240.0);
                        let content_h = (area.height() - 36.0).max(220.0);
                        ui.set_min_size(egui::vec2(content_w, content_h));
                        ui.set_max_width(content_w);
                        ui.set_max_height(content_h);
                        // 顶部 44px 是原生标题栏命中区（HTCAPTION），
                        // 放在那里的按钮点不到，所以内容整体下移避开。
                        ui.add_space(TITLEBAR_H);

                        // 顶栏：不要返回键了，点齿轮或 Esc 退出
                        ui.label(RichText::new(crate::tr!("设置")).size(15.0).color(TEXT).strong());
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            for tab in [
                                SettingsTab::Display,
                                SettingsTab::Behavior,
                                SettingsTab::Tools,
                                SettingsTab::Storage,
                                SettingsTab::About,
                            ] {
                                let on = self.settings_tab == tab;
                                let b = egui::Button::new(
                                    RichText::new(tab.label())
                                        .size(10.0)
                                        .color(if on { on_accent() } else { MUTED }),
                                )
                                .fill(if on { accent() } else { Color32::TRANSPARENT })
                                .stroke(line(Color32::WHITE, 16))
                                .rounding(Rounding::same(8.0));
                                if ui.add(b).clicked() {
                                    self.settings_tab = tab;
                                }
                            }
                        });
                        ui.add_space(14.0);

                        // 关于 / 存储是独立整页，不参与滚动布局。
                        match self.settings_tab {
                            SettingsTab::About => {
                                self.about_tab(ui);
                                return;
                            }
                            SettingsTab::Storage => {
                                self.storage_tab(ui);
                                return;
                            }
                            _ => {}
                        }

                        egui::ScrollArea::vertical()
                            .id_salt("settings_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                // 单列：宽屏上别把一行拉得太长，读起来累
                                let w = ui.available_width().min(760.0);
                                ui.set_max_width(w);
                                match self.settings_tab {
                                    SettingsTab::Display => self.display_tab(ui, &mut changed),
                                    SettingsTab::Behavior => self.behavior_tab(ui, &mut changed),
                                    SettingsTab::Tools => self.tools_tab(ui, &mut changed),
                                    _ => {}
                                }
                            });
                    });
            });

        if changed {
            self.cfg.save();
        }
    }

    /// 设置「显示」：外观（背景 / 暗化 / 窗口大小）+ 语言。
    fn display_tab(&mut self, ui: &mut egui::Ui, changed: &mut bool) {
        // ── 外观 ──
        section(ui, crate::tr!("外观"));
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::tr!("背景")).size(10.5).color(MUTED));
            ui.add_space(12.0);
            for m in settings::BgMode::all() {
                let on = self.cfg.bg_mode == m;
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(m.label()).size(10.5).color(if on {
                                on_accent()
                            } else {
                                MUTED
                            }),
                        )
                        .fill(if on { accent() } else { tint(Color32::WHITE, 10) })
                        .stroke(if on {
                            Stroke::NONE
                        } else {
                            line(Color32::WHITE, 20)
                        })
                        .rounding(Rounding::same(8.0)),
                    )
                    .clicked()
                {
                    self.cfg.bg_mode = m;
                    *changed = true;
                }
            }
        });

        if self.cfg.bg_mode == settings::BgMode::Solid {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(64.0);
                for c in SOLIDS {
                    let on = self.cfg.bg_color == c;
                    let col = Color32::from_rgb(c[0], c[1], c[2]);
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(20.0, 20.0),
                        egui::Sense::click(),
                    );
                    let p = ui.painter();
                    p.rect_filled(rect, Rounding::same(6.0), col);
                    if on {
                        p.rect_stroke(
                            rect.expand(2.0),
                            Rounding::same(7.0),
                            Stroke::new(1.5, accent()),
                        );
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(
                            egui::CursorIcon::PointingHand,
                        );
                    }
                    if resp.clicked() {
                        self.cfg.bg_color = c;
                        *changed = true;
                    }
                }
            });
        }
        if self.cfg.bg_mode == settings::BgMode::Gradient {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(64.0);
                for (i, (name, _, _, _)) in
                    crate::bg::GRADIENTS.iter().enumerate()
                {
                    let on = self.cfg.bg_gradient as usize == i;
                    // 左边给个 12×12 的渐变小样，不然只有名字看不出颜色
                    let (pre, _) = ui.allocate_exact_size(
                        egui::vec2(12.0, 12.0),
                        egui::Sense::hover(),
                    );
                    crate::bg::paint_gradient_preview(
                        ui.painter(),
                        pre,
                        crate::bg::gradient_colors(i as u8),
                    );
                    if ui
                        .add(
                            egui::Button::new(RichText::new(crate::i18n::tr(*name)).size(
                                10.5,
                            ).color(if on {
                                on_accent()
                            } else {
                                MUTED
                            }))
                            .fill(if on {
                                accent()
                            } else {
                                tint(Color32::WHITE, 10)
                            })
                            .stroke(if on {
                                Stroke::NONE
                            } else {
                                line(Color32::WHITE, 20)
                            })
                            .rounding(Rounding::same(8.0)),
                        )
                        .clicked()
                    {
                        self.cfg.bg_gradient = i as u8;
                        *changed = true;
                    }
                }
            });
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::tr!("暗化")).size(10.5).color(MUTED));
            ui.add_space(12.0);
            let mut dim = self.cfg.dim as i32;
            if bar_slider(ui, &mut dim, 0, 210, false) {
                self.cfg.dim = dim as u8;
                *changed = true;
            }
        });

        // 窗口大小：固定尺寸，切档位直接改窗口，不做界面缩放。
        // 缩放会让内容比窗口先变大，五列必然挤坏。
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::tr!("窗口大小")).size(10.5).color(MUTED));
            ui.add_space(12.0);
            for m in settings::WindowSize::all() {
                let on = self.cfg.window == m;
                if seg_button(ui, m.label(), on) {
                    self.cfg.window = m;
                    let (w, h) = m.size_px();
                    // InnerSize 会再乘一次全局 zoom，先除掉
                    ui.ctx().send_viewport_cmd(
                        egui::ViewportCommand::InnerSize(
                            egui::vec2(w / BASE_ZOOM, h / BASE_ZOOM),
                        ),
                    );
                    *changed = true;
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if seg_button(ui, crate::tr!("全屏"), self.fullscreen) {
                crate::set_fullscreen(
                    &mut self.fullscreen,
                    ui.ctx(),
                    self.cfg.window.size_px(),
                );
            }
            ui.label(
                RichText::new(crate::tr!("F11 切换；Esc 退出全屏"))
                    .size(9.0)
                    .color(DIM),
            );
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(crate::tr!("窗口固定尺寸：小 1160×660，中 1260×700，大 1380×760，超大 1580×880（随屏幕自动缩放）"))
                .size(9.0)
                .color(DIM),
        );

        section(ui, crate::tr!("语言 / Language"));
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.cfg.lang == "zh", crate::tr!("中文"))
                .clicked()
            {
                self.cfg.lang = "zh".to_string();
                crate::i18n::set_lang("zh");
                fc_analyze::set_en(false);
                fc_core::format::set_en(false);
                fc_core::wiki::set_en(false);
                self.cfg.save();
                ui.ctx().request_repaint();
            }
            if ui
                .selectable_label(self.cfg.lang == "en", "English")
                .clicked()
            {
                self.cfg.lang = "en".to_string();
                crate::i18n::set_lang("en");
                fc_analyze::set_en(true);
                fc_core::format::set_en(true);
                fc_core::wiki::set_en(true);
                self.cfg.save();
                ui.ctx().request_repaint();
            }
        });
        para(
            ui,
            "Switch takes effect immediately. English translation is being filled in.",
            8.5,
            DIM,
        );
    }

    /// 设置「行为」：自动分析、配置方案、CUE 分轨。
    fn behavior_tab(&mut self, ui: &mut egui::Ui, changed: &mut bool) {
        // ── 行为 ──
        section(ui, crate::tr!("行为"));
        let mut auto = self.cfg.auto_analyze;
        // 复选框底色透明：只留勾和文字，别在深色面板上再压一块方块
        let r = ui
            .scope(|ui| {
                let v = &mut ui.style_mut().visuals.widgets;
                v.inactive.bg_fill = Color32::TRANSPARENT;
                v.inactive.weak_bg_fill = Color32::TRANSPARENT;
                v.inactive.bg_stroke = Stroke::NONE;
                v.hovered.bg_fill = tint(Color32::WHITE, 8);
                v.hovered.weak_bg_fill = tint(Color32::WHITE, 8);
                v.hovered.bg_stroke = Stroke::NONE;
                v.active.bg_fill = tint(Color32::WHITE, 12);
                v.active.weak_bg_fill = tint(Color32::WHITE, 12);
                v.active.bg_stroke = Stroke::NONE;
                ui.checkbox(
                    &mut auto,
                    RichText::new(crate::tr!("选中文件时自动分析频谱"))
                        .size(10.5)
                        .color(MUTED),
                )
            })
            .inner;
        if r.changed() {
            self.cfg.auto_analyze = auto;
            *changed = true;
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(crate::tr!("关掉后不自动分析，省 CPU；每个文件约 1 秒"))
                .size(9.0)
                .color(DIM),
        );

        ui.add_space(18.0);
        section(ui, crate::tr!("配置"));
        self.profile_panel(ui);

ui.add_space(18.0);
        section(ui, crate::tr!("CUE 分轨"));
        para(
            ui,
            crate::tr!("选一个 .cue：按 TRACK / INDEX 把整轨切成单曲，格式用当前选中的输出格式，文件输出到 cue 同目录。"),
            8.5,
            DIM,
        );
        ui.add_space(6.0);
        let cue_btn = egui::Button::new(
            RichText::new(crate::tr!("选择 CUE 并分轨")).size(9.0).color(MUTED),
        )
        .fill(tint(Color32::WHITE, 10))
        .stroke(line(Color32::WHITE, 18))
        .rounding(Rounding::same(7.0));
        if ui.add(cue_btn).clicked() {
            if let Some(cue) = rfd::FileDialog::new()
                .add_filter("CUE", &["cue"])
                .pick_file()
            {
                let tools = self.tools.clone();
                let target = self.target.id;
                let out = cue
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .to_path_buf();
                self.cue_msg = Some(crate::tr!("正在分轨…").to_string());
                let (tx, rx) = std::sync::mpsc::channel();
                self.cue_rx = Some(rx);
                std::thread::spawn(move || {
                    let res = match tools {
                        Some(t) => fc_core::cue::split(
                            &t, &cue, &out, target, |_, _| {},
                        )
                        .map_err(|e| format!("{e:#}")),
                        None => Err(crate::tr!("找不到 ffmpeg").to_string()),
                    };
                    let _ = tx.send(res);
                });
            }
        }
        if let Some(m) = &self.cue_msg {
            para(ui, m, 8.5, DIM);
        }
    }

    /// 设置「工具」：外部编码器探测结果 + 文件关联。
    fn tools_tab(&mut self, ui: &mut egui::Ui, changed: &mut bool) {
        section(ui, crate::tr!("依赖 / 外部工具"));

        // 一行：圆点 + 名字 + 状态；下面缩进一行路径
        let dep = |ui: &mut egui::Ui,
                       name: &str,
                       ok: bool,
                       status: &str,
                       detail: &str| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(
                    egui::vec2(10.0, 10.0),
                    egui::Sense::hover(),
                );
                ui.painter().circle_filled(
                    r.center(),
                    3.0,
                    if ok { accent() } else { WARN },
                );
                ui.label(
                    RichText::new(name)
                        .size(9.5)
                        .color(if ok { TEXT } else { MUTED }),
                );
                ui.label(
                    RichText::new(status)
                        .size(8.8)
                        .color(if ok { accent() } else { WARN }),
                );
            });
            if !detail.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(detail).size(8.0).color(DIM),
                        )
                        .truncate(),
                    );
                });
            }
        };

        // ffmpeg：核心依赖
        {
            let ok = self.tools.is_some();
            let path = self
                .tools
                .as_ref()
                .map(|t| t.ffmpeg.display().to_string())
                .unwrap_or_else(|| crate::tr!("未找到 —— 所有转换都不可用").to_string());
            let status = if ok {
                format!(
                    "{} · {} 个编码器",
                    self.ffmpeg_ver.as_deref().unwrap_or(crate::tr!("版本未知")),
                    self.encoders.len()
                )
            } else {
                crate::tr!("未找到").to_string()
            };
            dep(ui, "ffmpeg", ok, &status, &path);
        }
        // ffprobe：读文件真实参数
        {
            let ok = self.tools.is_some();
            let path = self
                .tools
                .as_ref()
                .map(|t| t.ffprobe.display().to_string())
                .unwrap_or_else(|| crate::tr!("未找到 —— 读不出采样率/位深").to_string());
            let status = if ok { crate::tr!("已找到") } else { crate::tr!("未找到") };
            dep(ui, "ffprobe", ok, status, &path);
        }
        // atracdenc：可选
        {
            let ok = self.atracdenc.is_some();
            let status = if ok {
                crate::tr!("已安装 · ATRAC1/3/3+ 可输出")
            } else {
                crate::tr!("未安装（可选）· ATRAC 只能解码")
            };
            let path = self
                .atracdenc
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| {
                    crate::tr!("装好后放到程序目录或 PATH，再点下面的「重新检测」")
                        .to_string()
                });
            dep(ui, "atracdenc", ok, status, &path);
        }
        // speexenc：随包
        {
            let ok = self.speexenc.is_some();
            let status = if ok {
                crate::tr!("已找到 · Speex 可输出")
            } else {
                crate::tr!("未找到 · Speex 不可输出")
            };
            let path = self
                .speexenc
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| {
                    crate::tr!("随包提供，通常在 vendor\\speex 或程序的 speex\\ 目录")
                        .to_string()
                });
            dep(ui, "speexenc", ok, status, &path);
        }
        // wavpack：可选，有损 hybrid 模式要靠它
        {
            let ok = self.wavpack.is_some();
            let status = if ok {
                crate::tr!("已找到 · WavPack 有损可输出")
            } else {
                crate::tr!("未找到（可选）· 只有无损 WavPack")
            };
            let path = self
                .wavpack
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| {
                    crate::tr!("ffmpeg 的 wavpack 编码器不支持 hybrid；装 wavpack.exe 才有有损")
                        .to_string()
                });
            dep(ui, "wavpack", ok, status, &path);
        }
        // ofr：OptimFROG，官方不随包
        {
            let ok = self.ofr.is_some();
            let status = if ok {
                crate::tr!("已找到 · OptimFROG 可输出")
            } else {
                crate::tr!("未安装（可选）· 只能解码")
            };
            let path = self
                .ofr
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| {
                    crate::tr!("官方许可要求分发 CLI 需授权，所以不随包；到 losslessaudio.org 下载后放到程序目录")
                        .to_string()
                });
            dep(ui, "ofr", ok, status, &path);
        }
        // mpcenc：随包，Musepack 用
        {
            let ok = self.mpcenc.is_some();
            let status = if ok {
                crate::tr!("已找到 · Musepack 可输出")
            } else {
                crate::tr!("未找到 · Musepack 不可输出")
            };
            let path = self
                .mpcenc
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| crate::tr!("随包提供，通常在 musepack 目录").to_string());
            dep(ui, "mpcenc", ok, status, &path);
        }
        // psxavenc：可选，PS1 XA / PS2 VAG 用
        {
            let ok = self.psxavenc.is_some();
            let status = if ok {
                crate::tr!("已找到 · PS1 XA / PS2 VAG 可输出")
            } else {
                crate::tr!("未找到（可选）· PS1 XA / PS2 VAG 不可输出")
            };
            let path = self
                .psxavenc
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| crate::tr!("放到程序目录或 psxavenc 子目录").to_string());
            dep(ui, "psxavenc", ok, status, &path);
        }
        // qoaconv：可选，QOA 用
        {
            let ok = self.qoaconv.is_some();
            let status = if ok {
                crate::tr!("已找到 · QOA 可输出")
            } else {
                crate::tr!("未找到（可选）· QOA 不可输出")
            };
            let path = self
                .qoaconv
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| crate::tr!("放到程序目录或 qoa 子目录").to_string());
            dep(ui, "qoaconv", ok, status, &path);
        }
        // VGAudioCli：可选，任天堂系容器用
        {
            let ok = self.vgaudio.is_some();
            let status = if ok {
                crate::tr!("已找到 · BRSTM/BCSTM/BFSTM/DSP/IDSP 可输出")
            } else {
                crate::tr!("未找到（可选）· 任天堂容器不可输出")
            };
            let path = self
                .vgaudio
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| crate::tr!("放到程序目录或 vgaudio 子目录").to_string());
            dep(ui, "vgaudio", ok, status, &path);
        }
        // vadpcm：可选，N64 VADPCM 用
        {
            let ok = self.vadpcm.is_some();
            let status = if ok {
                crate::tr!("已找到 · N64 VADPCM 可输出")
            } else {
                crate::tr!("未找到（可选）· N64 VADPCM 不可输出")
            };
            let path = self
                .vadpcm
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| crate::tr!("放到程序目录或 vadpcm 子目录").to_string());
            dep(ui, "vadpcm", ok, status, &path);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let b = egui::Button::new(
                RichText::new(crate::tr!("重新检测")).size(9.0).color(MUTED),
            )
            .fill(tint(Color32::WHITE, 10))
            .stroke(line(Color32::WHITE, 18))
            .rounding(Rounding::same(7.0));
            if ui.add(b).clicked() {
                self.tools = fc_core::locate();
                if let Some(t) = &self.tools {
                    self.ffmpeg_ver = fc_core::ffmpeg_version(&t.ffmpeg);
                    self.encoders = fc_core::list_encoders(&t.ffmpeg);
                }
                self.atracdenc = fc_core::find_atracdenc();
                self.speexenc = fc_core::find_speexenc();
                self.wavpack = fc_core::find_wavpack();
                self.ofr = fc_core::find_ofr();
                self.mpcenc = fc_core::find_mpcenc();
                self.psxavenc = fc_core::find_psxavenc();
                self.qoaconv = fc_core::find_qoaconv();
                self.vgaudio = fc_core::find_vgaudio();
                self.vadpcm = fc_core::find_vadpcm();
                *changed = true;
            }
            ui.label(
                RichText::new(crate::tr!("换过工具目录或版本后点一下"))
                    .size(8.5)
                    .color(DIM),
            );
        });
        para(
            ui,
            crate::tr!("atracdenc 是社区逆向实现的 ATRAC 编码器，LGPL-2.1+。它不随本程序分发：装不装、装哪个版本由你决定，程序只在你装了的时候调用它。"),
            8.5,
            DIM,
        );

        section(ui, crate::tr!("文件关联"));
        para(
            ui,
            crate::tr!("注册后 ToneLore 会出现在右键「打开方式」里，双击文件即可打开并试听。Windows 10/11 不允许程序静默设为默认，需要你到「默认应用」里自己确认。"),
            8.5,
            DIM,
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let mk = |label: &str| {
                egui::Button::new(RichText::new(label).size(9.0).color(MUTED))
                    .fill(tint(Color32::WHITE, 10))
                    .stroke(line(Color32::WHITE, 18))
                    .rounding(Rounding::same(7.0))
            };
            if ui.add(mk(crate::tr!("注册到打开方式"))).clicked() {
                self.assoc_msg = Some(match crate::assoc::register() {
                    Ok(()) => crate::tr!("已注册：右键「打开方式」里能看到 ToneLore。").to_string(),
                    Err(e) => crate::fmt2!("注册失败：{e}", "Registration failed: {e}"),
                });
            }
            if ui.add(mk(crate::tr!("移除注册"))).clicked() {
                self.assoc_msg = Some(match crate::assoc::unregister() {
                    Ok(()) => crate::tr!("已移除注册。").to_string(),
                    Err(e) => crate::fmt2!("移除失败：{e}", "Removal failed: {e}"),
                });
            }
            if ui.add(mk(crate::tr!("打开默认应用设置"))).clicked() {
                crate::assoc::open_default_apps();
            }
        });
        if let Some(m) = &self.assoc_msg {
            para(ui, m, 8.5, DIM);
        }
    }
    /// 标题栏那个小圆点（进度环）点开的面板：活动中心。
    ///
    /// 原来这里是"通知"面板；「任务」列被砍掉之后，任务的列表、开始按钮、
    /// 正在跑的进度都搬到这里——一处看全部"正在发生什么"。
    /// 底下保留通知列表，转换完成/失败仍然记在这。
    pub(crate) fn notices_overlay(&mut self, ctx: &egui::Context) {
        if !self.notices_open {
            return;
        }
        let screen = ctx.screen_rect();
        let w = 372.0f32;
        let x = self
            .bell_rect
            .left()
            .clamp(screen.left() + 12.0, screen.right() - w - 12.0);
        let max_h = (screen.height() - TITLEBAR_H - 40.0).max(240.0);
        let mut panel_rect = egui::Rect::NOTHING;
        egui::Area::new(egui::Id::new("notices_area"))
            // 离标题栏留 12：贴着画的话点按钮容易点进最小化
            .fixed_pos(egui::pos2(x - 8.0, screen.top() + TITLEBAR_H + 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let inner = egui::Frame::none()
                    .fill(tint(BG, 248))
                    .stroke(Stroke::new(1.0, tint(Color32::WHITE, 26)))
                    .rounding(Rounding::same(14.0))
                    .shadow(egui::epaint::Shadow {
                        offset: egui::vec2(0.0, 6.0),
                        blur: 20.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha(120),
                    })
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                    .show(ui, |ui| {
                        ui.set_width(w - 28.0);
                        ui.set_max_height(max_h);

                        // ── 头：活动 + 开始转换 ──
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(crate::tr!("活动")).size(12.5).color(TEXT).strong());
                            let pending = self
                                .tasks
                                .iter()
                                .filter(|t| matches!(t.state, State::Pending))
                                .count();
                            if pending > 0 && !self.running {
                                ui.label(
                                    RichText::new(crate::fmt2!("{pending} 个待转换", "{pending} pending"))
                                        .size(9.0)
                                        .color(DIM),
                                );
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(crate::tr2!("钉住", "Pin"))
                                                .size(8.0)
                                                .color(if self.notices_pinned { accent() } else { MUTED }),
                                        )
                                        .fill(if self.notices_pinned {
                                            tint(accent(), 24)
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .stroke(line(Color32::WHITE, 16))
                                        .rounding(Rounding::same(6.0)),
                                    )
                                    .on_hover_text(crate::tr2!(
                                        "钉住后点面板外面不会关闭",
                                        "Pinned: clicking outside will not close this panel"
                                    ))
                                    .clicked()
                                {
                                    self.notices_pinned = !self.notices_pinned;
                                }
                                let failed = self
                                    .tasks
                                    .iter()
                                    .filter(|t| matches!(t.state, State::Failed(_)))
                                    .count();
                                if failed > 0 && !self.running {
                                    let rb = egui::Button::new(
                                        RichText::new(crate::fmt2!(
                                            "重试失败 ({failed})",
                                            "Retry failed ({failed})"
                                        ))
                                        .size(9.5)
                                        .color(MUTED),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(line(Color32::WHITE, 20))
                                    .rounding(Rounding::same(8.0));
                                    if ui.add(rb).clicked() {
                                        for t in &mut self.tasks {
                                            if matches!(t.state, State::Failed(_)) {
                                                t.state = State::Pending;
                                            }
                                        }
                                        self.start_batch(ui.ctx());
                                    }
                                    ui.add_space(6.0);
                                }
                                let can = !self.running
                                    && self.tools.is_some()
                                    && pending > 0;
                                let (label, col) = if self.running {
                                    (
                                        format!("{:.0}%", self.overall() * 100.0),
                                        accent(),
                                    )
                                } else {
                                    (crate::tr!("开始转换").to_string(), if can { on_accent() } else { DIM })
                                };
                                let b = egui::Button::new(RichText::new(label).size(10.0).color(col))
                                    .fill(if can {
                                        accent()
                                    } else {
                                        tint(Color32::WHITE, 12)
                                    })
                                    .stroke(Stroke::NONE)
                                    .rounding(Rounding::same(8.0))
                                    .min_size(egui::vec2(76.0, 24.0));
                                if ui
                                    .add_enabled(can || self.running, b)
                                    .on_hover_text(crate::tr!("把任务列表里待转换的文件都转掉"))
                                    .clicked()
                                    && can
                                {
                                    self.start_batch(ui.ctx());
                                }
                            });
                        });
                        ui.add_space(8.0);

                        // ── 任务列表（原「任务」列的内容）──
                        egui::ScrollArea::vertical()
                            .id_salt("activity_tasks")
                            .max_height((max_h * 0.52).max(120.0))
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                self.task_list(ui);
                            });

                        ui.add_space(8.0);
                                                ui.add_space(10.0);
                        self.hairline(ui);
                        ui.add_space(8.0);

                        // ── 缓存：现在手上有多少东西是算好的 ──
                        let waves = if self.wave.is_some() { 1 } else { 0 };
                        let analyzed = self
                            .items
                            .iter()
                            .filter(|i| matches!(i.analysis, Analysis::Done(_)))
                            .count();
                        let covers = self.items.iter().filter(|i| i.cover.is_some()).count();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(crate::tr!("缓存")).size(9.5).color(MUTED));
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!(
                                    "波形 {waves} · 分析 {analyzed} · 封面 {covers} · 库 {} 首",
                                    self.lib_entries.len()
                                ))
                                .size(9.0)
                                .color(DIM),
                            );
                        });
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(crate::tr!("波形/分析/封面都是按需算的，关掉就没了"))
                                .size(8.5)
                                .color(DIM),
                        );

                        ui.add_space(10.0);
                        self.hairline(ui);
                        ui.add_space(8.0);

                        // ── 通知 ──
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(crate::tr!("通知")).size(10.5).color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if !self.notices.is_empty()
                                    && ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(crate::tr!("清空")).size(9.0).color(DIM),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE),
                                        )
                                        .clicked()
                                {
                                    self.notices.clear();
                                }
                            });
                        });
                        ui.add_space(4.0);
                        if self.notices.is_empty() {
                            para(ui, crate::tr!("还没有通知。转换完成或失败都会记一条。"), 9.0, DIM);
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("activity_notices")
                                .max_height(180.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    // 新的在上面
                                    for i in (0..self.notices.len()).rev() {
                                        let (ok, title, body) = {
                                            let n = &self.notices[i];
                                            (n.ok, n.title.clone(), n.body.clone())
                                        };
                                        if i + 1 < self.notices.len() {
                                            ui.add_space(6.0);
                                            self.hairline(ui);
                                            ui.add_space(6.0);
                                        }
                                        ui.horizontal(|ui| {
                                            let (dot, _) = ui.allocate_exact_size(
                                                egui::vec2(9.0, 15.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().circle_filled(
                                                dot.center(),
                                                3.0,
                                                if ok { accent() } else { DANGER },
                                            );
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(title)
                                                        .size(10.0)
                                                        .color(if ok { accent() } else { DANGER }),
                                                );
                                                ui.add_space(1.0);
                                                para(ui, &body, 8.5, MUTED);
                                            });
                                        });
                                    }
                                });
                        }

                        // ── 系统：CPU / 内存 / 编解码压力（最下面，实时）──
                        ui.add_space(10.0);
                        self.hairline(ui);
                        ui.add_space(7.0);
                        self.sys.sample(ui.ctx());
                        let s = self.sys.stats;
                        ui.label(RichText::new(crate::tr!("系统")).size(9.5).color(MUTED));
                        ui.add_space(3.0);
                        Self::stat_row(
                            ui,
                            "CPU",
                            &format!("{:.0}%", s.cpu_total * 100.0),
                            &crate::fmt2!("本程序 {:.1}%", "This app {:.1}%", s.cpu_self * 100.0),
                            s.cpu_total,
                        );
                        let used = s.mem_total.saturating_sub(s.mem_avail);
                        let mfrac = if s.mem_total > 0 {
                            used as f32 / s.mem_total as f32
                        } else {
                            0.0
                        };
                        Self::stat_row(
                            ui,
                            crate::tr!("内存"),
                            &format!(
                                "{:.1}/{:.1} GB {:.0}%",
                                used as f64 / 1_073_741_824.0,
                                s.mem_total as f64 / 1_073_741_824.0,
                                mfrac * 100.0
                            ),
                            &crate::fmt2!("本程序 {} MB", "This app {} MB", s.mem_self / 1_048_576),
                            mfrac,
                        );
                        Self::stat_row(
                            ui,
                            crate::tr!("编解码压力"),
                            &format!("{:.0}%", s.cpu_children * 100.0),
                            &if s.codec_procs > 0 {
                                crate::fmt2!("{} 个进程", "{} processes", s.codec_procs)
                            } else {
                                crate::tr!("空闲（无编解码任务）").to_string()
                            },
                            s.cpu_children,
                        );
                        ui.add_space(8.0);
                    });
                panel_rect = inner.response.rect;
            });

        // Esc / 点面板外面关掉（圆点自己不算"外面"，不然点它会把面板又开又关）
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.notices_open = false;
        } else if ctx.input(|i| i.pointer.any_click()) {
            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                if !panel_rect.contains(pos) && !self.bell_rect.contains(pos) && !self.console_rect.contains(pos) && !self.notices_pinned {
                    self.notices_open = false;
                }
            }
        }
    }

    /// 一块 1px 的分隔线（面板里到处要用）
    fn hairline(&self, ui: &mut egui::Ui) {
        let (r, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 1.0),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(r, Rounding::ZERO, tint(Color32::WHITE, 16));
    }

/// 面板底部的一行系统指标：名称 + 细条 + 数值 + 备注
fn stat_row(ui: &mut egui::Ui, name: &str, value: &str, sub: &str, frac: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_sized(
            egui::vec2(68.0, 12.0),
            egui::Label::new(RichText::new(name).size(8.5).color(MUTED)),
        );
        let (r, _) = ui.allocate_exact_size(egui::vec2(54.0, 5.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(r, Rounding::same(2.5), tint(Color32::WHITE, 18));
        let w = (r.width() * frac.clamp(0.0, 1.0)).max(1.0);
        let mut f = r;
        f.set_width(w);
        ui.painter().rect_filled(f, Rounding::same(2.5), accent());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if !sub.is_empty() {
                ui.label(RichText::new(sub).size(8.5).color(DIM));
            }
            ui.label(RichText::new(value).size(9.5).color(TEXT));
        });
    });
}
}
fn qr_texture(ctx: &egui::Context, bytes: &[u8], key: &str) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(key, pixels, egui::TextureOptions::LINEAR))
}

impl App {
    /// 设置页「关于」：版本、GitHub、两个赞赏二维码。
    fn about_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.qr_wechat.is_none() {
            self.qr_wechat = qr_texture(&ctx, include_bytes!("../assets/donate-wechat.png"), "qr_wechat");
        }
        if self.qr_alipay.is_none() {
            self.qr_alipay = qr_texture(&ctx, include_bytes!("../assets/donate-alipay.png"), "qr_alipay");
        }

        ui.label(RichText::new(crate::APP_NAME).size(20.0).color(TEXT).strong());
        ui.label(RichText::new(format!("v{}", crate::VERSION)).size(11.0).color(accent()));
        ui.add_space(8.0);
        ui.label(
            RichText::new(crate::tr!("音频格式顾问与转换器 · 本地处理，不上传音频"))
                .size(10.0)
                .color(MUTED),
        );
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let gh = egui::Button::new(RichText::new(crate::tr!("GitHub 仓库")).size(9.5).color(on_accent()))
                .fill(accent())
                .stroke(Stroke::NONE)
                .rounding(Rounding::same(8.0))
                .min_size(egui::vec2(96.0, 26.0));
            if ui.add(gh).clicked() {
                ctx.open_url(egui::OpenUrl::new_tab("https://github.com/TeiketsuCN/ToneLore"));
            }
            if ui
                .add(
                    egui::Button::new(RichText::new(crate::tr!("问题反馈")).size(9.5).color(MUTED))
                        .fill(Color32::TRANSPARENT)
                        .stroke(line(Color32::WHITE, 20))
                        .rounding(Rounding::same(8.0))
                        .min_size(egui::vec2(86.0, 26.0)),
                )
                .clicked()
            {
                ctx.open_url(egui::OpenUrl::new_tab("https://github.com/TeiketsuCN/ToneLore/issues"));
            }
        });
        ui.add_space(16.0);

        ui.label(RichText::new(crate::tr!("支持开发")).size(10.5).color(MUTED).strong());
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            for (tex, label) in [
                (&self.qr_wechat, crate::tr2!("微信赞赏", "WeChat tip")),
                (&self.qr_alipay, crate::tr2!("支付宝赞赏", "Alipay tip")),
            ] {
                ui.vertical(|ui| {
                    if let Some(t) = tex {
                        ui.add(egui::Image::new((t.id(), egui::vec2(132.0, 132.0))));
                    }
                    ui.label(RichText::new(label).size(9.0).color(DIM));
                });
                ui.add_space(18.0);
            }
        });
        ui.add_space(16.0);
        ui.label(
            RichText::new(crate::tr!("主程序 MIT；FFmpeg 按 LGPL 构建；第三方工具单独标注许可。"))
                .size(8.5)
                .color(DIM),
        );
    }
}
impl App {
    /// 独立控制台窗口：像终端一样看日志，支持 clear/help/version/logs 命令。
    pub(crate) fn console_overlay(&mut self, ctx: &egui::Context) {
        if !self.console_open {
            return;
        }
        let mut open = true;
        let mut copy = false;
        let mut clear = false;
        let mut open_dir = false;
        let mut submitted: Option<String> = None;

        let screen = ctx.screen_rect();
        let win_w = (screen.width() * 0.46).clamp(420.0, 720.0).min(screen.width() - 48.0);
        let win_h = (screen.height() * 0.45).clamp(260.0, 420.0).min(screen.height() - 120.0);
        let win_x = (screen.right() - win_w - 24.0).max(screen.left() + 24.0);
        let body_h = (win_h - 118.0).max(90.0);
        let win_y = (screen.top() + TITLEBAR_H + 24.0)
            .min(screen.bottom() - win_h - 24.0)
            .max(screen.top() + TITLEBAR_H + 12.0);
        egui::Window::new(crate::tr2!("控制台", "Console"))
            .id(egui::Id::new("console_window"))
            .open(&mut open)
            .order(egui::Order::Tooltip)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(tint(BG, 246))
                    .stroke(Stroke::new(1.0, tint(Color32::WHITE, 30)))
                    .rounding(Rounding::same(12.0)),
            )
            .default_pos(egui::pos2(win_x, win_y))
            .default_size([win_w, win_h])
            .min_size([(win_w * 0.6).max(320.0), (win_h * 0.6).max(200.0)])
            .max_width(screen.width() - 48.0)
            .max_height(screen.height() - 96.0)
            .constrain(true)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let ghost = |label: &str| {
                        egui::Button::new(RichText::new(label).size(9.0).color(MUTED))
                            .fill(Color32::TRANSPARENT)
                            .stroke(line(Color32::WHITE, 18))
                            .rounding(Rounding::same(7.0))
                    };
                    if ui.add(ghost(crate::tr2!("复制", "Copy"))).clicked() {
                        copy = true;
                    }
                    if ui.add(ghost(crate::tr2!("清空", "Clear"))).clicked() {
                        clear = true;
                    }
                    if ui.add(ghost(crate::tr2!("打开日志", "Open logs"))).clicked() {
                        open_dir = true;
                    }
                    ui.label(
                        RichText::new(self.log_path.display().to_string())
                            .size(8.0)
                            .color(DIM),
                    );
                });
                ui.separator();
                let console_body_h = {
                    let avail = ui.available_height();
                    if avail.is_finite() {
                        (avail - 44.0).clamp(80.0, screen.height() - 220.0)
                    } else {
                        (win_h - 118.0).max(90.0)
                    }
                };
                egui::ScrollArea::vertical()
                    .id_salt("console_lines")
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .max_height(console_body_h)
                    .show(ui, |ui| {
                        if self.console.is_empty() {
                            ui.label(
                                RichText::new(crate::tr2!(
                                    "ToneLore 控制台。转换一次，这里会出现日志。输入 help 看命令。",
                                    "ToneLore console. Run a conversion to see logs. Type help for commands."
                                ))
                                .monospace()
                                .size(9.0)
                                .color(DIM),
                            );
                        }
                        for line in &self.console {
                            ui.label(RichText::new(line).monospace().size(9.0).color(TEXT));
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(">").monospace().color(accent()));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.console_input)
                            .frame(false)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .hint_text(crate::tr2!("输入命令，回车执行", "Type a command, press Enter")),
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submitted = Some(std::mem::take(&mut self.console_input));
                        // 回车后把焦点还回输入框，别让用户再点一次
                        ui.memory_mut(|m| m.request_focus(r.id));
                    }
                });
            });

        if !open {
            self.console_open = false;
        }
        if copy {
            ctx.copy_text(self.console.join("\n"));
        }
        if clear {
            self.console.clear();
        }
        if open_dir {
            if let Some(dir) = self.log_path.parent() {
                #[cfg(windows)]
                {
                    let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
                }
            }
        }
        if let Some(line) = submitted {
            self.run_console_command(line);
        }
    }

    fn run_console_command(&mut self, line: String) {
        let cmd = line.trim().to_lowercase();
        self.log(format!("> {}", line.trim()));
        match cmd.as_str() {
            "" => {}
            "clear" | "cls" => self.console.clear(),
            "help" | "?" => self.log(
                "commands: help, version, logs, clear".to_string(),
            ),
            "version" => self.log(format!("ToneLore {}", crate::VERSION)),
            "logs" | "open" => {
                if let Some(dir) = self.log_path.parent() {
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
                    }
                }
            }
            other => self.log(format!("unknown command: {other}")),
        }
    }
}
impl App {
    /// 设置页「产出与存储」：默认导出位置、控制台日志、缓存清理。
    fn storage_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(crate::tr!("产出与存储")).size(13.0).color(TEXT).strong());
        ui.add_space(10.0);

        ui.label(RichText::new(crate::tr!("默认导出位置")).size(10.5).color(MUTED).strong());
        ui.add_space(4.0);
        if crate::radio_dot(ui, crate::tr!("和源文件同目录"), self.cfg.output_mode == settings::OutputMode::Source) {
            self.cfg.output_mode = settings::OutputMode::Source;
        }
        if crate::radio_dot(ui, crate::tr!("指定目录"), self.cfg.output_mode == settings::OutputMode::Custom) {
            self.cfg.output_mode = settings::OutputMode::Custom;
        }
        ui.horizontal(|ui| {
            let mut dir = self.cfg.output_dir.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut dir)
                    .hint_text(crate::tr!("选一个目录"))
                    .desired_width((ui.available_width() - 80.0).max(120.0)),
            );
            if r.changed() {
                self.cfg.output_dir = dir;
            }
            if ui.button(crate::tr!("浏览")).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.cfg.output_dir = p.to_string_lossy().to_string();
                }
            }
        });
        ui.add_space(14.0);

        ui.label(RichText::new(crate::tr!("控制台日志")).size(10.5).color(MUTED).strong());
        ui.add_space(4.0);
        ui.label(
            RichText::new(self.log_path.display().to_string())
                .size(8.5)
                .color(DIM),
        );
        ui.horizontal(|ui| {
            if ui.button(crate::tr!("打开日志目录")).clicked() {
                if let Some(dir) = self.log_path.parent() {
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
                    }
                }
            }
            if ui.button(crate::tr!("清空本次会话日志")).clicked() {
                self.console.clear();
            }
        });
        ui.add_space(14.0);

        ui.label(RichText::new(crate::tr!("缓存")).size(10.5).color(MUTED).strong());
        ui.add_space(4.0);
        ui.label(
            RichText::new(crate::fmt2!(
                "音乐库缓存 {} 首，封面缓存 {} 张",
                "Library cache: {} tracks, {} covers",
                self.cfg.library_cache.len(),
                self.lib_covers.len()
            ))
            .size(9.0)
            .color(DIM),
        );
        ui.horizontal(|ui| {
            if ui.button(crate::tr!("清空音乐库缓存")).clicked() {
                self.cfg.library_cache.clear();
                self.lib_entries.clear();
                self.lib_selected = None;
                self.cfg.save();
                self.lib_status = crate::tr!("音乐库缓存已清空").to_string();
            }
            if ui.button(crate::tr!("清空封面缓存")).clicked() {
                self.lib_covers.clear();
                self.lib_cover_pending.clear();
                self.lib_status = crate::tr!("封面缓存已清空").to_string();
            }
            if ui.button(crate::tr!("清理临时文件")).clicked() {
                let dir = std::env::temp_dir().join("ToneLore");
                let mut n = 0;
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    for e in rd.flatten() {
                        if std::fs::remove_file(e.path()).is_ok() {
                            n += 1;
                        }
                    }
                }
                self.lib_status = crate::fmt2!(
                    "已清理 {} 个临时文件",
                    "Removed {} temp file(s)",
                    n
                );
            }
        });
        ui.add_space(14.0);
        ui.label(
            RichText::new(crate::tr!("配置目录：%APPDATA%\\ToneLore；临时文件：%TEMP%\\ToneLore"))
                .size(8.5)
                .color(DIM),
        );
    }
}