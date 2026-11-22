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
                                RichText::new("返回").size(10.0).color(MUTED),
                            )
                            .fill(tint(Color32::WHITE, 8))
                            .stroke(line(Color32::WHITE, 16))
                            .rounding(Rounding::same(7.0))
                            .min_size(egui::vec2(56.0, 24.0));
                            if ui.add(back).on_hover_text("回到主界面（Esc 也行）").clicked() {
                                self.wiki_open = false;
                            }
                            ui.add_space(12.0);
                            ui.label(RichText::new("格式百科").size(15.0).color(TEXT).strong());
                            ui.add_space(16.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.wiki_filter)
                                    .hint_text("搜格式名…")
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
                                            RichText::new(spec.name).size(20.0).color(TEXT).strong(),
                                        );
                                        ui.add_space(6.0);
                                        ui.horizontal(|ui| {
                                            let kind = match spec.kind {
                                                fc_core::Kind::Lossless => "无损",
                                                fc_core::Kind::Lossy => "有损",
                                                _ => "设备专用",
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
                                                ("只能读，不能转出", WARN)
                                            } else if self.target_available(spec) {
                                                ("可以转成这个", accent())
                                            } else {
                                                ("要装外部编码器才能转出", WARN)
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
                                                    format!("{} 就是当前输出格式", spec.name)
                                                } else {
                                                    format!("把输出格式设为 {}", spec.name)
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
                                                .on_hover_text("回到主界面，接下来转的文件都用这个格式")
                                                .clicked()
                                                && !is_target
                                            {
                                                self.set_target(spec);
                                                self.wiki_open = false;
                                            }
                                            ui.add_space(4.0);
                                            para(
                                                ui,
                                                "选中后的码率等参数在右侧「输出」列调整。",
                                                8.5,
                                                DIM,
                                            );
                                        } else if spec.can_encode {
                                            ui.add_space(2.0);
                                            let why = if fc_core::external_encoder(spec.id).is_some() {
                                                "你的系统里还没装 atracdenc（外部编码器），暂时不能转成它。装好后到设置里点「重新检测」。"
                                            } else {
                                                "你的 ffmpeg 里没有这个编码器，暂时不能转成它。"
                                            };
                                            card(ui, DANGER, |ui| {
                                                para(ui, why, 10.0, MUTED);
                                            });
                                        }

                                        ui.add_space(6.0);

                                        let text = fc_core::long_blurb(spec.id)
                                            .unwrap_or(spec.blurb);
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
    pub(crate) fn settings_panel(&mut self, ctx: &egui::Context) {
        let t = self.settings_t;
        let screen = ctx.screen_rect();
        let top = screen.top() + TITLEBAR_H;
        let area = egui::Rect::from_min_max(
            egui::pos2(screen.left(), top),
            screen.max,
        );
        // 整页做"从下往上滑入 / 往下滑出"：滑出时主界面从上面露出来，
        // 不会像以前那样滑 22px 就突然消失
        let offset = 0.0;

        let mut changed = false;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }

        egui::Area::new(egui::Id::new("settings_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(area.min + egui::vec2(0.0, offset))
            // 不让 egui 自动把 Area 挪进屏幕：窗口矮的时候内容比屏幕高，
            // 默认的 constrain 会把整个 Area **往上顶到 y=0**，盖住标题栏
            // （表现就是标题栏按钮点不到、但窗口还能拖；按 Esc 关掉设置就好）。
            // 内容溢出交给内部的 ScrollArea，位置永远钉在标题栏下面。
            .constrain(false)
            .show(ctx, |ui| {
                egui::Frame::none()
                    // 和主界面同一套"玻璃"：半透明底 + 壁纸透过来。
                    // 之前是 tint(BG, 255) 的纯黑，用户明确说不要。
                    // 滑动过程中偏实（避免主界面从底下透上来发脏），停稳后回到玻璃感
                    .fill({
                        // 滑动过程中几乎不透明（主界面不能从底下透上来），停稳后再回到玻璃感
                        let k = ((t - 0.9) / 0.1).clamp(0.0, 1.0);
                        let s = k * k * (3.0 - 2.0 * k);
                        tint(BG, (255.0 - 107.0 * s) as u8)
                    })
                    .inner_margin(egui::Margin::symmetric(28.0, 18.0))
                    .show(ui, |ui| {
                        // 内容尺寸要扣掉 Frame 的边距（28*2 / 18*2）。
                        // min 和 max **都要卡**：只给 min 的话，窗口缩到小/中/大时
                        // 设置内容比窗口高，Area 会被撑大并往上顶、盖住标题栏
                        // ——标题栏是原生命中区，盖住后按钮点不到，但拖窗口还正常。
                        let content_w = (area.width() - 56.0).max(240.0);
                        let content_h = (area.height() - 36.0).max(220.0);
                        ui.set_min_size(egui::vec2(content_w, content_h));
                        ui.set_max_width(content_w);
                        ui.set_max_height(content_h);
                        // 顶部 44px 是原生标题栏命中区（HTCAPTION），
                        // 放在那里的按钮点不到，所以内容整体下移避开。
                                        ui.add_space(TITLEBAR_H);

                        // 顶栏：不要返回键了，点齿轮或 Esc 退出
                        ui.label(RichText::new("设置").size(15.0).color(TEXT).strong());
                        ui.add_space(18.0);

                        egui::ScrollArea::vertical()
                            .id_salt("settings_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let col_w = ((ui.available_width() - 28.0) / 2.0).max(240.0);
                                // 不再强制 620 高：窗口矮的时候那会把内容顶出可视区，
                                // 也会让 Area 比窗口还高。给可用高度，超出由 ScrollArea 滚。
                                let col_h = ui.available_height().max(220.0);
                                ui.horizontal_top(|ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_w, col_h),
                                        Layout::top_down(Align::LEFT),
                                        |ui| {
                                            ui.set_max_width(col_w);
                                // ── 外观 ──
                                section(ui, "外观");
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("背景").size(10.5).color(MUTED));
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
                                            changed = true;
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
                                                changed = true;
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
                                                    egui::Button::new(RichText::new(*name).size(
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
                                                changed = true;
                                            }
                                        }
                                    });
                                }

                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("暗化").size(10.5).color(MUTED));
                                    ui.add_space(12.0);
                                    let mut dim = self.cfg.dim as i32;
                                    if bar_slider(ui, &mut dim, 0, 210, false) {
                                        self.cfg.dim = dim as u8;
                                        changed = true;
                                    }
                                });

                                // 窗口大小：固定尺寸，切档位直接改窗口，不做界面缩放。
                                // 缩放会让内容比窗口先变大，五列必然挤坏。
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("窗口大小").size(10.5).color(MUTED));
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
                                            changed = true;
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if seg_button(ui, "全屏", self.fullscreen) {
                                        crate::set_fullscreen(
                                            &mut self.fullscreen,
                                            ui.ctx(),
                                            self.cfg.window.size_px(),
                                        );
                                    }
                                    ui.label(
                                        RichText::new("F11 切换；Esc 退出全屏")
                                            .size(9.0)
                                            .color(DIM),
                                    );
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("窗口固定尺寸：小 1160×660，中 1260×700，大 1380×760，超大 1580×880（随屏幕自动缩放）")
                                        .size(9.0)
                                        .color(DIM),
                                );

                                ui.add_space(18.0);

                                // ── 行为 ──
                                section(ui, "行为");
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
                                            RichText::new("选中文件时自动分析频谱")
                                                .size(10.5)
                                                .color(MUTED),
                                        )
                                    })
                                    .inner;
                                if r.changed() {
                                    self.cfg.auto_analyze = auto;
                                    changed = true;
                                }
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("关掉后不自动分析，省 CPU；每个文件约 1 秒")
                                        .size(9.0)
                                        .color(DIM),
                                );

                                ui.add_space(18.0);
                                section(ui, "配置");
                                self.profile_panel(ui);

                                ui.add_space(18.0);

                                        },
                                    );
                                    ui.add_space(28.0);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_w, col_h),
                                        Layout::top_down(Align::LEFT),
                                        |ui| {
                                            ui.set_max_width(col_w);
                                // ── 关于 ──
                                section(ui, "关于");
                                egui::Grid::new("about")
                                    .num_columns(2)
                                    .spacing([16.0, 6.0])
                                    .show(ui, |ui| {
                                        row(ui, "程序", APP_NAME);
                                        row(ui, "版本", crate::VERSION);
                                        // 署名用昵称，不写真名：隐私和以后转手都方便
                                        row(ui, "开发", "克洛伊Chloe");
                                        // 不写真名：昵称和收款码上的署名一致，隐私和转手都方便
                                        row(ui, "字体", "Noto Sans SC（SIL OFL 1.1）");
                                        row(ui, "音频解码", "ffmpeg 子进程");
                                        row(ui, "ATRAC 编码", "atracdenc（LGPL-2.1+，需自行安装）");
                                        row(ui, "频谱分析", "fc-analyze（自研）");
                                        row(ui, "许可", "开源，详见仓库");
                                    });

                                // GitHub：图标 + 仓库链接，点哪个都打开仓库
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if let Some(icon) = &self.github_icon {
                                        let r = ui.add(
                                            egui::ImageButton::new(
                                                egui::Image::new(icon)
                                                    .fit_to_exact_size(egui::vec2(22.0, 22.0)),
                                            )
                                            .frame(false),
                                        );
                                        if r.on_hover_text(GITHUB_URL).clicked() {
                                            ui.ctx().open_url(egui::OpenUrl::new_tab(GITHUB_URL));
                                        }
                                    }
                                    let link = ui.add(
                                        egui::Label::new(
                                            RichText::new("TeiketsuCN / ToneBox")
                                                .size(9.5)
                                                .color(accent()),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if link.clicked() {
                                        ui.ctx().open_url(egui::OpenUrl::new_tab(GITHUB_URL));
                                    }
                                });

                                ui.add_space(10.0);
                                section(ui, "依赖 / 外部工具");

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
                                        .unwrap_or_else(|| "未找到 —— 所有转换都不可用".to_string());
                                    let status = if ok {
                                        format!(
                                            "{} · {} 个编码器",
                                            self.ffmpeg_ver.as_deref().unwrap_or("版本未知"),
                                            self.encoders.len()
                                        )
                                    } else {
                                        "未找到".to_string()
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
                                        .unwrap_or_else(|| "未找到 —— 读不出采样率/位深".to_string());
                                    let status = if ok { "已找到" } else { "未找到" };
                                    dep(ui, "ffprobe", ok, status, &path);
                                }
                                // atracdenc：可选
                                {
                                    let ok = self.atracdenc.is_some();
                                    let status = if ok {
                                        "已安装 · ATRAC1/3/3+ 可输出"
                                    } else {
                                        "未安装（可选）· ATRAC 只能解码"
                                    };
                                    let path = self
                                        .atracdenc
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| {
                                            "装好后放到程序目录或 PATH，再点下面的「重新检测」"
                                                .to_string()
                                        });
                                    dep(ui, "atracdenc", ok, status, &path);
                                }
                                // speexenc：随包
                                {
                                    let ok = self.speexenc.is_some();
                                    let status = if ok {
                                        "已找到 · Speex 可输出"
                                    } else {
                                        "未找到 · Speex 不可输出"
                                    };
                                    let path = self
                                        .speexenc
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| {
                                            "随包提供，通常在 vendor\\speex 或程序的 speex\\ 目录"
                                                .to_string()
                                        });
                                    dep(ui, "speexenc", ok, status, &path);
                                }
                                // wavpack：可选，有损 hybrid 模式要靠它
                                {
                                    let ok = self.wavpack.is_some();
                                    let status = if ok {
                                        "已找到 · WavPack 有损可输出"
                                    } else {
                                        "未找到（可选）· 只有无损 WavPack"
                                    };
                                    let path = self
                                        .wavpack
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| {
                                            "ffmpeg 的 wavpack 编码器不支持 hybrid；装 wavpack.exe 才有有损"
                                                .to_string()
                                        });
                                    dep(ui, "wavpack", ok, status, &path);
                                }
                                // ofr：OptimFROG，官方不随包
                                {
                                    let ok = self.ofr.is_some();
                                    let status = if ok {
                                        "已找到 · OptimFROG 可输出"
                                    } else {
                                        "未安装（可选）· 只能解码"
                                    };
                                    let path = self
                                        .ofr
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| {
                                            "官方许可要求分发 CLI 需授权，所以不随包；到 losslessaudio.org 下载后放到程序目录"
                                                .to_string()
                                        });
                                    dep(ui, "ofr", ok, status, &path);
                                }
                                // mpcenc：随包，Musepack 用
                                {
                                    let ok = self.mpcenc.is_some();
                                    let status = if ok {
                                        "已找到 · Musepack 可输出"
                                    } else {
                                        "未找到 · Musepack 不可输出"
                                    };
                                    let path = self
                                        .mpcenc
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "随包提供，通常在 musepack 目录".to_string());
                                    dep(ui, "mpcenc", ok, status, &path);
                                }
                                // psxavenc：可选，PS1 XA / PS2 VAG 用
                                {
                                    let ok = self.psxavenc.is_some();
                                    let status = if ok {
                                        "已找到 · PS1 XA / PS2 VAG 可输出"
                                    } else {
                                        "未找到（可选）· PS1 XA / PS2 VAG 不可输出"
                                    };
                                    let path = self
                                        .psxavenc
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "放到程序目录或 psxavenc 子目录".to_string());
                                    dep(ui, "psxavenc", ok, status, &path);
                                }
                                // qoaconv：可选，QOA 用
                                {
                                    let ok = self.qoaconv.is_some();
                                    let status = if ok {
                                        "已找到 · QOA 可输出"
                                    } else {
                                        "未找到（可选）· QOA 不可输出"
                                    };
                                    let path = self
                                        .qoaconv
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "放到程序目录或 qoa 子目录".to_string());
                                    dep(ui, "qoaconv", ok, status, &path);
                                }
                                // VGAudioCli：可选，任天堂系容器用
                                {
                                    let ok = self.vgaudio.is_some();
                                    let status = if ok {
                                        "已找到 · BRSTM/BCSTM/BFSTM/DSP/IDSP 可输出"
                                    } else {
                                        "未找到（可选）· 任天堂容器不可输出"
                                    };
                                    let path = self
                                        .vgaudio
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "放到程序目录或 vgaudio 子目录".to_string());
                                    dep(ui, "vgaudio", ok, status, &path);
                                }
                                // vadpcm：可选，N64 VADPCM 用
                                {
                                    let ok = self.vadpcm.is_some();
                                    let status = if ok {
                                        "已找到 · N64 VADPCM 可输出"
                                    } else {
                                        "未找到（可选）· N64 VADPCM 不可输出"
                                    };
                                    let path = self
                                        .vadpcm
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "放到程序目录或 vadpcm 子目录".to_string());
                                    dep(ui, "vadpcm", ok, status, &path);
                                }

                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    let b = egui::Button::new(
                                        RichText::new("重新检测").size(9.0).color(MUTED),
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
                                        changed = true;
                                    }
                                    ui.label(
                                        RichText::new("换过工具目录或版本后点一下")
                                            .size(8.5)
                                            .color(DIM),
                                    );
                                });
                                para(
                                    ui,
                                    "atracdenc 是社区逆向实现的 ATRAC 编码器，LGPL-2.1+。它不随本程序分发：装不装、装哪个版本由你决定，程序只在你装了的时候调用它。",
                                    8.5,
                                    DIM,
                                );

                                ui.add_space(16.0);
                                section(ui, "文件关联");
                                para(
                                    ui,
                                    "注册后 ToneBox 会出现在右键「打开方式」里，双击文件即可打开并试听。Windows 10/11 不允许程序静默设为默认，需要你到「默认应用」里自己确认。",
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
                                    if ui.add(mk("注册到打开方式")).clicked() {
                                        self.assoc_msg = Some(match crate::assoc::register() {
                                            Ok(()) => "已注册：右键「打开方式」里能看到 ToneBox。".to_string(),
                                            Err(e) => format!("注册失败：{e}"),
                                        });
                                    }
                                    if ui.add(mk("移除注册")).clicked() {
                                        self.assoc_msg = Some(match crate::assoc::unregister() {
                                            Ok(()) => "已移除注册。".to_string(),
                                            Err(e) => format!("移除失败：{e}"),
                                        });
                                    }
                                    if ui.add(mk("打开默认应用设置")).clicked() {
                                        crate::assoc::open_default_apps();
                                    }
                                });
                                if let Some(m) = &self.assoc_msg {
                                    para(ui, m, 8.5, DIM);
                                }

                                ui.add_space(18.0);
                                section(ui, "支持开发");
                                para(
                                    ui,
                                    "免费、开源、不卖授权。觉得有用就扫下面任意一个。",
                                    9.5,
                                    MUTED,
                                );
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    for (tex, label) in [
                                        (&self.donate_wechat, "微信"),
                                        (&self.donate_alipay, "支付宝"),
                                    ] {
                                        ui.vertical(|ui| {
                                            if let Some(t) = tex {
                                                ui.add(
                                                    egui::Image::new(t)
                                                        .fit_to_exact_size(egui::vec2(96.0, 96.0)),
                                                );
                                            }
                                            ui.add_space(3.0);
                                            ui.label(RichText::new(label).size(9.0).color(DIM));
                                        });
                                        ui.add_space(14.0);
                                    }
                                });
                                        },
                                    );
                                });
                            });
                    });
            });

        if changed {
            self.cfg.save();
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
                            ui.label(RichText::new("活动").size(12.5).color(TEXT).strong());
                            let pending = self
                                .tasks
                                .iter()
                                .filter(|t| matches!(t.state, State::Pending))
                                .count();
                            if pending > 0 && !self.running {
                                ui.label(
                                    RichText::new(format!("{pending} 个待转换"))
                                        .size(9.0)
                                        .color(DIM),
                                );
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let can = !self.running
                                    && self.tools.is_some()
                                    && pending > 0;
                                let (label, col) = if self.running {
                                    (
                                        format!("{:.0}%", self.overall() * 100.0),
                                        accent(),
                                    )
                                } else {
                                    ("开始转换".to_string(), if can { on_accent() } else { DIM })
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
                                    .on_hover_text("把任务列表里待转换的文件都转掉")
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
                            ui.label(RichText::new("缓存").size(9.5).color(MUTED));
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
                            RichText::new("波形/分析/封面都是按需算的，关掉就没了")
                                .size(8.5)
                                .color(DIM),
                        );

                        ui.add_space(10.0);
                        self.hairline(ui);
                        ui.add_space(8.0);

                        // ── 通知 ──
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("通知").size(10.5).color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if !self.notices.is_empty()
                                    && ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("清空").size(9.0).color(DIM),
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
                            para(ui, "还没有通知。转换完成或失败都会记一条。", 9.0, DIM);
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
                        ui.label(RichText::new("系统").size(9.5).color(MUTED));
                        ui.add_space(3.0);
                        Self::stat_row(
                            ui,
                            "CPU",
                            &format!("{:.0}%", s.cpu_total * 100.0),
                            &format!("本程序 {:.1}%", s.cpu_self * 100.0),
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
                            "内存",
                            &format!(
                                "{:.1}/{:.1} GB {:.0}%",
                                used as f64 / 1_073_741_824.0,
                                s.mem_total as f64 / 1_073_741_824.0,
                                mfrac * 100.0
                            ),
                            &format!("本程序 {} MB", s.mem_self / 1_048_576),
                            mfrac,
                        );
                        Self::stat_row(
                            ui,
                            "编解码压力",
                            &format!("{:.0}%", s.cpu_children * 100.0),
                            &if s.codec_procs > 0 {
                                format!("{} 个进程", s.codec_procs)
                            } else {
                                "空闲（无编解码任务）".to_string()
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
                if !panel_rect.contains(pos) && !self.bell_rect.contains(pos) {
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
