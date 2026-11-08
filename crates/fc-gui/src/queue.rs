//! 转换页中间栏：文件队列

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
use crate::player::*;
use crate::spectrum::*;
use crate::detail::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

impl App {

    pub(crate) fn queue(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        ui.set_width(ui.available_width());
        if self.items.is_empty() {
            self.drop_zone(ui);
            return false;
        }
        let dt = self.dt;
        let mut animating = false;
        let mut remove: Option<usize> = None;
        let mut play_now: Option<usize> = None;
        let count = self.items.len();

        // 按钮放在列表顶部：播放栏是悬浮的，底部按钮会被它盖住
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            if lib_button(ui, "添加文件", true) {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    self.add_files(paths);
                }
            }
            if lib_button(ui, "清空", false) && !self.running {
                self.items.clear();
                self.selected = 0;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} 个文件", count))
                        .size(9.0)
                        .color(DIM),
                );
            });
        });
        ui.add_space(6.0);

        // 刚点了「清空」：这一帧直接收工，别再用上面算好的旧下标去访问 items（会越界 panic）
        if self.items.is_empty() {
            self.drop_zone(ui);
            return false;
        }
        let count = self.items.len();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            // 底部给悬浮播放栏留位，列表不要钻到它下面
            .max_height((ui.available_height() - self.bar_reserve()).max(60.0))
            .show(ui, |ui| {
                for i in 0..count {
                    let is_sel = i == self.selected;
                    let name = self.items[i].name();
                    let info = self.items[i].info.clone();
                    let error = self.items[i].error.clone();
                    let spec = self.items[i].spec(self.target);
                    let state = self.items[i].state.clone();
                    let override_id = self.items[i].override_id;
                    let tags = self.items[i].tags.clone();
                    let cover = self.items[i].cover.clone();
                    let encrypted = self.items[i].encrypted.is_some();

                    // 动效值先算，再画
                    let (sel_t, hov_t) = {
                        let it = &mut self.items[i];
                        animating |= approach(&mut it.sel_t, if is_sel { 1.0 } else { 0.0 }, dt, 16.0);
                        let target_h = if it.hovered { 1.0 } else { 0.0 };
                        animating |= approach(&mut it.hover_t, target_h, dt, 14.0);
                        (it.sel_t, it.hover_t)
                    };

                    let base = blend(tint(Color32::WHITE, 6), tint(Color32::WHITE, 16), hov_t);
                    let fill = blend(base, tint(accent(), 30), sel_t);
                    let stroke_c = blend(
                        blend(Color32::TRANSPARENT, tint(Color32::WHITE, 22), hov_t),
                        tint(accent(), 90),
                        sel_t,
                    );

                    let frame = egui::Frame::none()
                        .fill(fill)
                        .stroke(Stroke::new(1.0, stroke_c))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(egui::Margin::symmetric(14.0, 10.0));

                    // 右侧按钮占据的区域。整行点击必须避开它，
                    // 否则整行 interact 会盖在按钮上面，把点击吃掉。
                    let mut btn_zone: Option<egui::Rect> = None;
                    let resp = frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let row_w = ui.available_width();
                            // 窄窗口（小分辨率）时行里放不下徽章，省掉它，别和标题叠在一起
                            let narrow = row_w < 400.0;
                            let thumb = cover_thumb(ui, cover.as_ref(), 36.0);
                        if encrypted {
                            paint_lock(ui.painter(), thumb);
                        }
                            ui.add_space(5.0);
                            ui.vertical(|ui| {
                                // 右边要放：状态 + ··· + ×，有音质标识还要再宽一点
                                let q_w = info
                                    .as_ref()
                                    .and_then(|m| {
                                        // 视频文件的音质徽章容易被当成"画面质量"，队列里不画
                                        if m.has_video() {
                                            return None;
                                        }
                                        quality_of(m)
                                            .map(|q| quality_width(ui.ctx(), q, m.bitrate_kbps))
                                    })
                                    .unwrap_or(0.0);
                                let reserve = if narrow { 130.0 } else { 186.0 + q_w };
                                ui.set_max_width((row_w - 42.0 - reserve).max(100.0));
                                let title = tags.display_title(&name);
                                ui.add(
                                    egui::Label::new(RichText::new(title).size(12.5).color(TEXT))
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                                ui.add_space(2.0);
                                let sub0 = match &info {
                                    Some(m) => {
                                        // 数值音质：编码 · 采样率/位深 · 码率 · 时长 → 目标
                                        // 用 44.1k/16bit 这种紧凑写法，一行放得下
                                        let mut parts: Vec<String> = Vec::new();
                                        if m.has_video() {
                                            // 视频文件先报画面：只写音频编码会让人以为这是个纯音频
                                            let v = m.video_label().unwrap_or("视频");
                                            if m.width > 0 && m.height > 0 {
                                                parts.push(format!("{v} {}×{}", m.width, m.height));
                                            } else {
                                                parts.push(v.to_string());
                                            }
                                        } else {
                                            parts.push(m.codec_label().to_string());
                                        }
                                        if m.sample_rate > 0 {
                                            if is_dsd(&m.codec) {
                                                // DSD 是 1-bit：kHz 和"8 bit"都是字节口径，换成 1-bit 写法
                                                parts.push(format!(
                                                    "{}/1bit",
                                                    fmt_dsd_m(m.sample_rate)
                                                ));
                                            } else {
                                                match m.bits {
                                                    Some(b) => parts.push(format!(
                                                        "{}/{}bit",
                                                        fmt_k(m.sample_rate),
                                                        b
                                                    )),
                                                    None => parts.push(fmt_khz(m.sample_rate)),
                                                }
                                            }
                                        }
                                        if m.bitrate_kbps > 0 {
                                            parts.push(format!("{}kbps", m.bitrate_kbps));
                                        }
                                        parts.push(fmt_duration(m.duration_s));
                                        format!("{}  →  {}", parts.join(" · "), spec.name)
                                    }
                                    None => {
                                        let e = error
                                            .clone()
                                            .unwrap_or_else(|| "读取失败".into());
                                        let first: String =
                                            e.lines().next().unwrap_or("").chars().take(46).collect();
                                        format!("{first}…")
                                    }
                                };
                                // 有艺术家/专辑就并到技术行前面
                                let artist = tags.display_sub();
                                let sub = if artist.is_empty() {
                                    sub0
                                } else {
                                    format!("{artist} · {sub0}")
                                };
                                let _ = &name;
                                ui.add(
                                    egui::Label::new(RichText::new(sub).size(10.0).color(DIM))
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                // 顺序按 right_to_left 的摆放来写：先写的在最右边
                                ui.set_min_width(ui.available_width());
                                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                                // 行里这两个小按钮不要底色：默认的浅色方块在深色行上很扎眼，
                                // 只留字符本身，悬停时给一点点提亮。
                                let vis = ui.visuals_mut();
                                vis.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                                vis.widgets.hovered.weak_bg_fill = tint(Color32::WHITE, 16);
                                vis.widgets.active.weak_bg_fill = tint(Color32::WHITE, 26);
                                vis.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                                vis.widgets.hovered.bg_fill = tint(Color32::WHITE, 16);
                                vis.widgets.active.bg_fill = tint(Color32::WHITE, 26);
                                vis.widgets.inactive.bg_stroke = Stroke::NONE;
                                vis.widgets.hovered.bg_stroke = Stroke::NONE;
                                vis.widgets.active.bg_stroke = Stroke::NONE;

                                // ① 删除 ×（最右）
                                let del_resp =
                                    ui.small_button(RichText::new("×").size(11.0).color(DIM));
                                if del_resp.clicked() {
                                    remove = Some(i);
                                }

                                // ② 单曲格式菜单 ···（选过格式就显示格式名）
                                let pick = override_id
                                    .and_then(fc_core::by_id)
                                    .map(|f| f.name.to_string());
                                let (label, label_col) = match &pick {
                                    Some(n) => (n.clone(), accent()),
                                    None => ("···".to_string(), DIM),
                                };
                                let menu_resp = ui.menu_button(
                                    RichText::new(label).size(if pick.is_some() { 10.5 } else { 12.0 }).color(label_col),
                                    |ui| {
                                        ui.set_min_width(120.0);
                                        if ui
                                            .selectable_label(override_id.is_none(), "跟随全局")
                                            .clicked()
                                        {
                                            self.items[i].override_id = None;
                                            ui.close_menu();
                                        }
                                        for f in fc_core::browsable() {
                                            let on = override_id == Some(f.id);
                                            if ui.selectable_label(on, f.name).clicked() {
                                                self.items[i].override_id = Some(f.id);
                                                ui.close_menu();
                                            }
                                        }
                                    },
                                );

                                // ③ 状态
                                match &state {
                                    State::Pending => {
                                        ui.label(RichText::new("待转换").size(10.0).color(MUTED));
                                    }
                                    State::Running(p) => {
                                        ui.add(
                                            egui::ProgressBar::new(*p)
                                                .desired_width(78.0)
                                                .fill(accent()),
                                        );
                                    }
                                    State::Done(_, sz) => {
                                        ui.label(
                                            RichText::new(format!("✓ {}", fmt_size(*sz)))
                                                .size(10.0)
                                                .color(accent()),
                                        );
                                    }
                                    State::Failed(_) => {
                                        ui.label(RichText::new("✗").size(11.0).color(DANGER));
                                    }
                                }

                                // ④ 音质标识（Hi-Res 会呼吸、CD 会转）；窄行里不画
                                if !narrow {
                                    if let Some(m) = &info {
                                        if m.has_video() {
                                            // 同上：画面文件不画音频音质徽章
                                        } else if let Some(q) = quality_of(m) {
                                            ui.add_space(4.0);
                                            animating |= quality_badge(ui, q, m.bitrate_kbps);
                                        }
                                    }
                                }
                                btn_zone = Some(menu_resp.response.rect.union(del_resp.rect));
                            });
                        });
                    });

                    let hovered = resp.response.hovered() || resp.response.contains_pointer();
                    self.items[i].hovered = hovered;

                    // 用稳定 id 重新注册交互，否则 Frame 每帧生成的 id 不同，
                    // egui 无法把两次点击关联成双击。
                    let row_id = ui.id().with(("queue_row", i));
                    let mut hit = resp.response.rect;
                    if let Some(b) = btn_zone {
                        // 从按钮左边界往右都不算行点击，留给按钮
                        hit.max.x = b.left().min(hit.max.x);
                    }
                    let row_resp = ui.interact(hit, row_id, egui::Sense::click());
                    if row_resp.clicked() {
                        // 自己判定双击：egui 的 double_clicked 依赖跨帧的 widget id，
                        // 在 ScrollArea + Frame 里不稳定，所以用时间差自己算。
                        let now = ui.input(|inp| inp.time);
                        let is_double = self.last_click.0 == i && now - self.last_click.1 < 0.45;
                        if is_double {
                            play_now = Some(i);
                            self.last_click = (usize::MAX, -10.0);
                        } else {
                            self.selected = i;
                            self.last_click = (i, now);
                        }
                    }
                    if row_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }

                    ui.add_space(5.0);
                }
            });

        if let Some(i) = remove {
            let removed = self.items.remove(i);
            // 删掉的正好是正在放/显示的那首：停止播放，别让播放栏指着不存在的歌
            if self.player_for.as_ref() == Some(&removed.path) {
                self.player = None;
                self.player_for = None;
                self.play_error = None;
            }
            // 后面的项整体前移，选中项跟着挪
            if i < self.selected {
                self.selected -= 1;
            }
            self.selected = self.selected.min(self.items.len().saturating_sub(1));
        }
        if let Some(i) = play_now {
            self.selected = i;
            self.start_play(i);
        }

        animating
    }

    pub(crate) fn drop_zone(&mut self, ui: &mut egui::Ui) {
        let mut do_pick = false;
        let avail = ui.available_height();
        // 不要外框：只有文字 + 按钮
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(20.0, 16.0))
            .show(ui, |ui| {
                // 窗口很矮时 avail 可能不足 40，set_min_height 传负数会 panic
                ui.set_min_height((avail - 40.0).max(0.0));
                let pad = ((avail - 190.0) * 0.5).max(0.0);
                ui.add_space(pad);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("把你认为有音乐的文件拖进来").size(15.0).color(TEXT));
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(
                            "可以一次拖多个 · 不限后缀，按文件内容识别",
                        )
                        .size(10.5)
                        .color(MUTED),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new("视频只取第一条音轨；认不出来的会告诉你为什么")
                            .size(9.5)
                            .color(DIM),
                    );
                    ui.add_space(16.0);
                    let btn =
                        egui::Button::new(RichText::new("选择文件").size(12.5).color(TEXT))
                            .fill(tint(Color32::WHITE, 16))
                            .stroke(line(Color32::WHITE, 32))
                            .rounding(Rounding::same(9.0));
                    if ui.add(btn).clicked() {
                        do_pick = true;
                    }
                });
            });
        if do_pick {
            if let Some(paths) = rfd::FileDialog::new().pick_files() {
                self.add_files(paths);
            }
        }
    }

}

/// 队列行左边的封面缩略图。没有封面就给一个占位方块。
pub(crate) fn cover_thumb(
    ui: &mut egui::Ui,
    tex: Option<&egui::TextureHandle>,
    size: f32,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter();
    match tex {
        Some(t) => {
            p.image(
                t.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            p.rect_stroke(rect, Rounding::same(0.0), line(Color32::WHITE, 26));
        }
        None => {
            p.rect_filled(rect, Rounding::same(4.0), tint(Color32::WHITE, 9));
            // 中间画一个音符形状太费事，用一个小方块占位
            p.rect_filled(
                egui::Rect::from_center_size(rect.center(), egui::vec2(10.0, 10.0)),
                Rounding::same(2.0),
                tint(Color32::WHITE, 22),
            );
        }
    }
    rect
}
