//! 底部播放栏 + 大图封面

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
use crate::spectrum::*;
use crate::detail::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Play,
    Pause,
    Stop,
    Prev,
    Next,
}

/// 播放栏这一帧想做点的事：播放/暂停、停止、跳转、音量、开频谱、看大图。
/// 先收集再统一执行，避免在画 UI 的闭包里改播放器状态。
#[derive(Default)]
pub(crate) struct BarAction {
    pub toggle: bool,
    pub stop: bool,
    pub seek: Option<f64>,
    pub volume: Option<f32>,
    pub open_spec: bool,
    pub open_cover: bool,
    pub prev: bool,
    pub next: bool,
}

impl BarAction {
    fn merge(&mut self, o: BarAction) {
        self.toggle |= o.toggle;
        self.stop |= o.stop;
        self.volume = o.volume.or(self.volume);
        self.open_spec |= o.open_spec;
        self.open_cover |= o.open_cover;
        self.prev |= o.prev;
        self.next |= o.next;
        if o.seek.is_some() {
            self.seek = o.seek;
        }
    }
}

impl App {

    /// 播放栏的细条：封面 / 走带 / 标题 / 进度 / 时间。
    ///
    /// 细条高度固定 46，占布局；展开出来的波形是浮在内容之上的（见
    /// `player_bar_expanded`）——这样鼠标扫过来时下面的按钮不会被顶走。
    pub(crate) fn player_bar(&mut self, ui: &mut egui::Ui) -> BarAction {
        let mut act = BarAction::default();
        let display = self.display_item();
        let (playing, pos, dur) = self.play_state();
        let has_player = self.player.is_some();
        let name = display
            .map(|i| {
                let it = &self.items[i];
                it.tags.display_title(&it.name())
            })
            .unwrap_or_else(|| crate::tr!("没有在播放").to_string());
        let sub = display
            .map(|i| self.items[i].tags.display_sub())
            .unwrap_or_default();
        let cover = display.and_then(|i| self.items[i].cover.clone());
        let has_cover = cover.is_some();
        let frac = ((pos / dur) as f32).clamp(0.0, 1.0);
        // 细条也画波形：不用悬停展开就能看到，避免“只有一条线”
        let wave = self.wave.as_ref().and_then(|(p, w)| {
            display
                .filter(|i| self.items[*i].path == *p)
                .map(|_| w.clone())
        });

        // 四个控件整体往下挪一点，别贴着栏顶
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
            let resp = bar_cover(ui, cover.as_ref(), 30.0);
            if resp.clicked() && has_cover {
                act.open_cover = true;
            }
            if icon_button(ui, Icon::Prev, false, has_player) {
                act.prev = true;
            }
            if icon_button(
                ui,
                if playing { Icon::Pause } else { Icon::Play },
                true,
                has_player,
            ) {
                act.toggle = true;
            }
            if icon_button(ui, Icon::Next, false, has_player) {
                act.next = true;
            }
            let right_w = 430.0;
            let mid_w = (ui.available_width() - right_w - 8.0).max(90.0);
            ui.allocate_ui_with_layout(
                egui::vec2(mid_w, 28.0),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(
                        egui::Label::new(
                            RichText::new(&name)
                                .size(11.5)
                                .color(if has_player { TEXT } else { MUTED }),
                        )
                        .truncate(),
                    );
                    if !sub.is_empty() {
                        ui.add(
                            egui::Label::new(RichText::new(&sub).size(9.0).color(DIM))
                                .truncate(),
                        );
                    }
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} / {}", fmt_duration(pos), fmt_duration(dur)))
                        .size(9.0)
                        .color(DIM),
                );
                ui.add_space(6.0);
                let w = (ui.available_width() - 4.0).clamp(90.0, 520.0);
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(w, 16.0), egui::Sense::click_and_drag());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let p = ui.painter();
                let track =
                    egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
                if let Some(chans) = &wave {
                    let cutoff = rect.left() + rect.width() * frac;
                    if let Some(col) = chans.first() {
                        let n = col.len().max(1);
                        for (i, (lo, hi)) in col.iter().enumerate() {
                            let x = rect.left() + rect.width() * (i as f32 / n as f32);
                            let amp = hi.abs().max(lo.abs()).clamp(0.02, 1.0);
                            let hh = amp * rect.height() * 0.45;
                            let c = if x <= cutoff {
                                accent()
                            } else {
                                tint(Color32::WHITE, 70)
                            };
                            p.line_segment(
                                [
                                    egui::pos2(x, rect.center().y - hh),
                                    egui::pos2(x, rect.center().y + hh),
                                ],
                                egui::Stroke::new(1.0, c),
                            );
                        }
                    }
                    p.line_segment(
                        [
                            egui::pos2(cutoff, rect.top()),
                            egui::pos2(cutoff, rect.bottom()),
                        ],
                        egui::Stroke::new(1.5, TEXT),
                    );
                } else {
                    p.rect_filled(track, Rounding::same(2.0), tint(Color32::WHITE, 22));
                    if frac > 0.0 {
                        let mut f = track;
                        f.set_width(track.width() * frac);
                        p.rect_filled(f, Rounding::same(2.0), accent());
                    }
                }
                let knob = if resp.hovered() || resp.dragged() { 5.5 } else { 4.0 };
                p.circle_filled(
                    egui::pos2(track.left() + track.width() * frac, track.center().y),
                    knob,
                    if has_player { TEXT } else { DIM },
                );
                if has_player
                    && (resp.clicked() || resp.dragged() || resp.drag_stopped())
                {
                    if let Some(pp) = resp.interact_pointer_pos() {
                        act.seek = Some(
                            ((pp.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64 * dur,
                        );
                    }
                }
            });
        });
        act
    }

    /// 展开出来的那一块：左边走带 + 音量，中间波形，右边频谱。
    /// 浮在内容之上（不参与布局），所以鼠标扫过来时下面的按钮不会被顶走。
    pub(crate) fn player_bar_expanded(&mut self, ui: &mut egui::Ui) -> BarAction {
        let display = self.display_item();
        let (_, pos, dur) = self.play_state();
        let has_player = self.player.is_some();
        let encrypted = display
            .map(|i| self.items[i].encrypted.is_some())
            .unwrap_or(false);
        let wave = self.wave.as_ref().and_then(|(p, w)| {
            display.filter(|i| self.items[*i].path == *p).map(|_| w)
        });
        let frac = ((pos / dur) as f32).clamp(0.0, 1.0);
        let mut act = BarAction::default();
        let h = (ui.available_height() - 4.0).max(36.0);

        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if icon_button(ui, Icon::Stop, false, has_player) {
                act.stop = true;
            }
            let mut v = self
                .player
                .as_ref()
                .map(|p| p.volume())
                .unwrap_or(self.player_volume);
            if volume_bar(ui, &mut v) {
                act.volume = Some(v);
            }
            ui.add_space(6.0);

            let wave_w = (ui.available_width() - 44.0).max(120.0);
            let (wrect, wresp) =
                ui.allocate_exact_size(egui::vec2(wave_w, h), egui::Sense::click_and_drag());
            let p = ui.painter();
            p.rect_filled(wrect, Rounding::same(8.0), tint(Color32::WHITE, 10));
            if let Some(chans) = wave {
                let lanes = chans.len().max(1);
                let lane_h = (h - 8.0) / lanes as f32;
                for (ci, data) in chans.iter().enumerate() {
                    let cy = wrect.top() + 4.0 + lane_h * (ci as f32 + 0.5);
                    let half = (lane_h * 0.42).max(2.0);
                    let n = data.len().max(1);
                    for (i, (mn, mx)) in data.iter().enumerate() {
                        let x = wrect.left() + wrect.width() * (i as f32 + 0.5) / n as f32;
                        let y0 = cy - mx.clamp(-1.0, 1.0) * half;
                        let y1 = cy - mn.clamp(-1.0, 1.0) * half;
                        let played = x <= wrect.left() + wrect.width() * frac;
                        let col = if played { accent() } else { tint(Color32::WHITE, 85) };
                        p.line_segment(
                            [egui::pos2(x, y0), egui::pos2(x, y1.max(y0 + 1.0))],
                            Stroke::new(1.2, col),
                        );
                    }
                }
                let px = wrect.left() + wrect.width() * frac;
                p.line_segment(
                    [
                        egui::pos2(px, wrect.top() + 3.0),
                        egui::pos2(px, wrect.bottom() - 3.0),
                    ],
                    Stroke::new(1.4, Color32::WHITE),
                );
            } else {
                p.text(
                    wrect.center(),
                    Align2::CENTER_CENTER,
                    if encrypted {
                        crate::tr!("加密文件：只识别，不解码也不播放")
                    } else if has_player {
                        crate::tr!("波形生成中…")
                    } else {
                        crate::tr!("双击队列里的文件就能试听")
                    },
                    FontId::proportional(9.5),
                    DIM,
                );
            }
            if has_player && (wresp.clicked() || wresp.dragged() || wresp.drag_stopped()) {
                if let Some(pp) = wresp.interact_pointer_pos() {
                    act.seek = Some(
                        ((pp.x - wrect.left()) / wrect.width()).clamp(0.0, 1.0) as f64 * dur,
                    );
                }
            }
            if spec_button(ui) {
                act.open_spec = true;
            }
        });

        act
    }
    /// 当前播放/显示的是哪一条（按路径找；删了/移了也不会指错歌）
    fn display_item(&self) -> Option<usize> {
        self.player_for
            .as_ref()
            .and_then(|p| self.items.iter().position(|it| &it.path == p))
            .or_else(|| (!self.items.is_empty()).then_some(self.selected))
            .filter(|i| *i < self.items.len())
    }

    fn play_state(&self) -> (bool, f64, f64) {
        match self.player.as_ref() {
            Some(p) => (p.is_playing(), p.position_s(), p.duration_s().max(0.1)),
            None => {
                let d = self
                    .display_item()
                    .and_then(|i| self.items[i].info.as_ref())
                    .map(|m| m.duration_s)
                    .unwrap_or(0.0);
                (false, 0.0, d.max(0.1))
            }
        }
    }

    /// 贴底播放栏：细条永远在，展开部分浮在上面。
    pub(crate) fn player_bar_panel(&mut self, ctx: &egui::Context) -> bool {
        let ptr = ctx.input(|i| i.pointer.hover_pos());
        let hot = ptr
            .map(|p| {
                self.bar_rect.contains(p)
                    || (self.bar_t > 0.06 && self.bar_expand_rect.contains(p))
            })
            .unwrap_or(false);
        self.bar_hot = hot;
        let target = if hot { 1.0 } else { 0.0 };
        let mut animating = approach(&mut self.bar_t, target, self.dt, 14.0);

        let mut act = BarAction::default();
        let rect = egui::TopBottomPanel::bottom("player_bar")
            .exact_height(BAR_SLIM)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 208))
                    .inner_margin(egui::Margin {
                        left: 12.0,
                        right: 12.0,
                        top: 4.0,
                        bottom: 6.0,
                    }),
            )
            .show(ctx, |ui| {
                act = self.player_bar(ui);
            })
            .response
            .rect;
        self.bar_rect = rect;

        let t = self.bar_t.clamp(0.0, 1.0);
        if t > 0.01 {
            let eased = t * t * (3.0 - 2.0 * t);
            let h = (BAR_FULL - BAR_SLIM) * eased;
            let expand = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() - h),
                egui::pos2(rect.right(), rect.top() + 1.0),
            );
            self.bar_expand_rect = expand;
            egui::Area::new(egui::Id::new("bar_expand"))
                .order(egui::Order::Middle)
                .fixed_pos(expand.min)
                .show(ctx, |ui| {
                    ui.set_width(expand.width());
                    ui.set_height(expand.height());
                    egui::Frame::none()
                        .fill(tint(BG, (238.0 * eased) as u8))
                        .inner_margin(egui::Margin {
                            left: 12.0,
                            right: 12.0,
                            top: 6.0,
                            bottom: 2.0,
                        })
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            act.merge(self.player_bar_expanded(ui));
                        });
                });
            animating = true;
        } else {
            self.bar_expand_rect = egui::Rect::NOTHING;
        }
        self.apply_bar_action(act);
        if animating {
            ctx.request_repaint();
        }
        animating
    }

    /// 把播放栏这一帧攒下的动作落到播放器/界面状态上
    fn apply_bar_action(&mut self, act: BarAction) {
        if act.open_spec {
            self.open_spectrum();
        }
        if act.open_cover {
            if let Some(i) = self.display_item() {
                self.cover_view = Some(i);
                self.cover_closing = false;
                self.cover_t = 0.0;
                self.start_big_cover(i);
            }
        }
        if let Some(v) = act.volume {
            self.player_volume = v;
            if let Some(p) = self.player.as_ref() {
                p.set_volume(v);
            }
        }
        if act.next || act.prev {
            let target = if act.next {
                self.play_idx + 1
            } else {
                self.play_idx.saturating_sub(1)
            };
            if let Some(path) = self.playlist.get(target).cloned() {
                if let Some(idx) = self.items.iter().position(|i| i.path == path) {
                    self.start_play(idx);
                }
            }
        }
        if let Some(p) = self.player.as_mut() {
            if act.toggle {
                p.toggle();
            }
            if act.stop {
                p.stop();
            }
            if let Some(t2) = act.seek {
                p.seek(t2);
            }
        }
    }
    /// 点开大图时后台重新抽一次封面（队列里那张只有 96px）
    pub(crate) fn start_big_cover(&mut self, idx: usize) {
        if self
            .cover_big
            .as_ref()
            .map(|(i, _)| *i == idx)
            .unwrap_or(false)
        {
            return;
        }
        let Some(tools) = self.tools.clone() else { return };
        let Some(it) = self.items.get(idx) else { return };
        let path = it.path.clone();
        let tx = self.cover_big_tx.clone();
        std::thread::spawn(move || {
            let payload = fc_core::extract_cover(&tools.ffmpeg, &path).and_then(|bytes| {
                let img = image::load_from_memory(&bytes).ok()?;
                // 太大的图缩到 1000px 就够看了，别把显存吃光
                let img = if img.width() > 1000 || img.height() > 1000 {
                    img.resize(1000, 1000, image::imageops::FilterType::Triangle)
                } else {
                    img
                };
                let rgba = img.to_rgba8();
                Some((rgba.as_raw().to_vec(), rgba.width(), rgba.height()))
            });
            let _ = tx.send((idx, payload));
        });
    }

    pub(crate) fn poll_big_cover(&mut self, ctx: &egui::Context) {
        while let Ok((idx, payload)) = self.cover_big_rx.try_recv() {
            if let Some((rgba, w, h)) = payload {
                let img = egui::ColorImage {
                    size: [w as usize, h as usize],
                    pixels: rgba
                        .chunks_exact(4)
                        .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                        .collect(),
                };
                self.cover_big = Some((
                    idx,
                    ctx.load_texture(format!("cover_big_{idx}"), img, egui::TextureOptions::LINEAR),
                ));
            }
        }
    }

    /// 大图封面：全屏暗底 + 居中大图，点任意处或 Esc 关闭
    pub(crate) fn cover_overlay(&mut self, ctx: &egui::Context) {
        let Some(idx) = self.cover_view else { return };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // 不直接清掉：让 cover_t 缩回 0，动画走完再关
            self.cover_closing = true;
            return;
        }
        let screen = ctx.screen_rect();
        let t = self.cover_t.clamp(0.0, 1.0);
        let tex = self
            .cover_big
            .as_ref()
            .filter(|(i, _)| *i == idx)
            .map(|(_, t)| t.clone())
            .or_else(|| self.items.get(idx).and_then(|it| it.cover.clone()));
        let title = self
            .items
            .get(idx)
            .map(|it| it.tags.display_title(&it.name()))
            .unwrap_or_default();

        let mut close = false;
        egui::Area::new(egui::Id::new("cover_view"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let resp = ui.allocate_response(screen.size(), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(screen, 0.0, tint(BG, (236.0 * t) as u8));
                // 0.86 → 1.0 的放大，配着暗底一起淡入
                let pop = 0.86 + 0.14 * t;
                match tex.as_ref() {
                    Some(tx) => {
                        let max = egui::vec2(screen.width() * 0.8, screen.height() * 0.72);
                        let ts = tx.size_vec2().max(egui::vec2(1.0, 1.0));
                        let scale = (max.x / ts.x).min(max.y / ts.y).clamp(0.05, 4.0);
                        let size = ts * scale * pop;
                        let rect = egui::Rect::from_center_size(
                            screen.center() - egui::vec2(0.0, 10.0),
                            size,
                        );
                        p.image(
                            tx.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE.gamma_multiply(t),
                        );
                        p.text(
                            egui::pos2(screen.center().x, rect.bottom() + 24.0),
                            egui::Align2::CENTER_CENTER,
                            &title,
                            egui::FontId::proportional(11.5),
                            MUTED.gamma_multiply(t),
                        );
                    }
                    None => {
                        p.text(
                            screen.center(),
                            egui::Align2::CENTER_CENTER,
                            crate::tr!("这个文件没有内嵌封面"),
                            egui::FontId::proportional(13.0),
                            MUTED.gamma_multiply(t),
                        );
                    }
                }
                p.text(
                    egui::pos2(screen.center().x, screen.bottom() - 24.0),
                    egui::Align2::CENTER_CENTER,
                    crate::tr!("点击任意处或按 Esc 关闭"),
                    egui::FontId::proportional(9.5),
                    DIM.gamma_multiply(t),
                );
                if resp.clicked() {
                    close = true;
                }
            });
        if close {
            self.cover_closing = true;
        }
    }

}

/// 播放栏的封面按钮：48px 方块，有图显示图，没图给占位；返回 response 供点击。
pub(crate) fn bar_cover(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, size: f32) -> egui::Response {    // 没封面就别装成能点的样子（不然光标变成手型、点了没反应）
    let sense = if tex.is_some() {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), sense);
    ui.painter()
        .rect_filled(rect, Rounding::same(9.0), tint(Color32::WHITE, 10));
    match tex {
        Some(t) => {
            // 用 Image 控件画，才能把封面裁成圆角（和悬停描边一致）
            let img = egui::Image::new(t)
                .fit_to_exact_size(egui::vec2(size, size))
                .rounding(Rounding::same(9.0));
            ui.put(rect, img);
        }
        None => {
            ui.painter().rect_filled(
                egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)),
                Rounding::same(3.0),
                tint(Color32::WHITE, 18),
            );
        }
    }
    if resp.hovered() && tex.is_some() {
        ui.painter().rect_stroke(
            rect,
            Rounding::same(9.0),
            Stroke::new(1.0, tint(Color32::WHITE, 80)),
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 音量条：和进度条同一套画法（细轨道 + 圆点），拖动调音量。
pub(crate) fn volume_bar(ui: &mut egui::Ui, value: &mut f32) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(64.0, 16.0), egui::Sense::click_and_drag());
    let p = ui.painter();
    let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
    p.rect_filled(track, Rounding::same(2.0), tint(Color32::WHITE, 16));
    let frac = value.clamp(0.0, 1.0);
    if frac > 0.0 {
        let mut f = track;
        f.set_width(track.width() * frac);
        p.rect_filled(f, Rounding::same(2.0), tint(MUTED, 200));
    }
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(
        egui::pos2(track.left() + track.width() * frac, track.center().y),
        if hot { 6.0 } else { 4.5 },
        if hot { TEXT } else { MUTED },
    );
    if resp.clicked() || resp.dragged() {
        if let Some(pp) = resp.interact_pointer_pos() {
            *value = ((pp.x - track.left()) / track.width()).clamp(0.0, 1.0);
            return true;
        }
    }
    false
}

/// 播放栏里的"看频谱"按钮：三根小电平条
pub(crate) fn spec_button(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    let p = ui.painter();
    let hot = resp.hovered();
    p.rect_filled(
        rect,
        Rounding::same(8.0),
        if hot {
            tint(Color32::WHITE, 16)
        } else {
            Color32::TRANSPARENT
        },
    );
    let c = if hot { accent() } else { MUTED };
    let heights = [6.0, 11.0, 8.0, 13.0];
    for (i, h) in heights.iter().enumerate() {
        let x = rect.center().x - 7.5 + i as f32 * 5.0;
        let r = egui::Rect::from_center_size(
            egui::pos2(x, rect.center().y + 1.0),
            egui::vec2(2.4, *h),
        );
        p.rect_filled(r, Rounding::same(1.2), c);
    }
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// 播放控制图标。不用字符（字体里没有 ⏸⏹），直接用 painter 画。
/// `enabled = false` 时只画不接点击（没在播的时候按钮要灰着但别消失）。
pub(crate) fn icon_button(ui: &mut egui::Ui, icon: Icon, hot: bool, enabled: bool) -> bool {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), sense);
    let p = ui.painter();
    let bg = if !enabled {
        tint(Color32::WHITE, 4)
    } else if resp.hovered() {
        tint(Color32::WHITE, 22)
    } else {
        tint(Color32::WHITE, 8)
    };
    p.rect_filled(rect, Rounding::same(8.0), bg);

    let c = rect.center();
    let col = if !enabled {
        DIM
    } else if hot {
        accent()
    } else {
        TEXT
    };
    match icon {
        Icon::Play => {
            let tri = vec![
                c + egui::vec2(-4.0, -6.5),
                c + egui::vec2(-4.0, 6.5),
                c + egui::vec2(7.0, 0.0),
            ];
            p.add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
        }
        Icon::Pause => {
            for dx in [-4.5_f32, 1.5] {
                p.rect_filled(
                    egui::Rect::from_min_size(c + egui::vec2(dx, -6.5), egui::vec2(3.0, 13.0)),
                    Rounding::same(1.0),
                    col,
                );
            }
        }
        Icon::Stop => {
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(11.0, 11.0)),
                Rounding::same(2.0),
                col,
            );
        }
        Icon::Prev => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(4.0, -6.0),
                    c + egui::vec2(4.0, 6.0),
                    c + egui::vec2(-5.0, 0.0),
                ],
                col,
                Stroke::NONE,
            ));
            p.rect_filled(
                egui::Rect::from_min_size(c + egui::vec2(-8.0, -6.0), egui::vec2(2.0, 12.0)),
                Rounding::same(1.0),
                col,
            );
        }
        Icon::Next => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-4.0, -6.0),
                    c + egui::vec2(-4.0, 6.0),
                    c + egui::vec2(5.0, 0.0),
                ],
                col,
                Stroke::NONE,
            ));
            p.rect_filled(
                egui::Rect::from_min_size(c + egui::vec2(6.0, -6.0), egui::vec2(2.0, 12.0)),
                Rounding::same(1.0),
                col,
            );
        }
    }
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
