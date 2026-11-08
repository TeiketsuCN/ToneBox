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
}

impl App {

    /// 底部播放栏：常驻在"队列 + 详情"两栏下面。
    ///
    /// 没在播的时候不消失——显示当前选中项的封面和时长，按钮置灰。
    /// 点封面看大图（队列里那张只有 96px，点开时后台重新抽一次原图）。
    pub(crate) fn player_bar(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());

        let display = self
            .player_for
            .as_ref()
            .and_then(|p| self.items.iter().position(|it| &it.path == p))
            .or_else(|| (!self.items.is_empty()).then_some(self.selected))
            .filter(|i| *i < self.items.len());
        let (playing, pos, dur) = match self.player.as_ref() {
            Some(p) => (p.is_playing(), p.position_s(), p.duration_s().max(0.1)),
            None => {
                let d = display
                    .and_then(|i| self.items[i].info.as_ref())
                    .map(|m| m.duration_s)
                    .unwrap_or(0.0);
                (false, 0.0, d.max(0.1))
            }
        };
        let has_player = self.player.is_some();
        let name = display
            .map(|i| {
                let it = &self.items[i];
                it.tags.display_title(&it.name())
            })
            .unwrap_or_else(|| "未在播放".to_string());
        let sub = display
            .map(|i| self.items[i].tags.display_sub())
            .unwrap_or_default();
        let cover = display.and_then(|i| self.items[i].cover.clone());
        let has_cover = cover.is_some();
        // 只有"当前显示的那首"的波形才拿来画
        let wave = self.wave.as_ref().and_then(|(p, w)| {
            display
                .filter(|i| self.items[*i].path == *p)
                .map(|_| w)
        });

        let mut toggle = false;
        let mut stop = false;
        let mut seek: Option<f64> = None;
        let mut vol_change: Option<f32> = None;
        let mut open_cover = false;
        let mut open_spec = false;

        // ── 上半：封面 + 走带 + 标题 + 音量 ──
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
            let resp = bar_cover(ui, cover.as_ref(), 64.0);
            if resp.clicked() && has_cover {
                open_cover = true;
            }
            ui.add_space(2.0);

            let vol_w = 122.0;
            let content_w = (ui.available_width() - vol_w).max(160.0);
            ui.allocate_ui_with_layout(
                egui::vec2(content_w, 64.0),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.horizontal(|ui| {
                        if icon_button(
                            ui,
                            if playing { Icon::Pause } else { Icon::Play },
                            true,
                            has_player,
                        ) {
                            toggle = true;
                        }
                        if icon_button(ui, Icon::Stop, false, has_player) {
                            stop = true;
                        }
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&name)
                                    .size(11.5)
                                    .color(if has_player { TEXT } else { MUTED }),
                            )
                            .truncate(),
                        );
                    });
                    ui.add_space(3.0);
                    ui.add(
                        egui::Label::new(RichText::new(&sub).size(9.0).color(DIM)).truncate(),
                    );
                },
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut v = self
                    .player
                    .as_ref()
                    .map(|p| p.volume())
                    .unwrap_or(self.player_volume);
                if volume_bar(ui, &mut v) {
                    vol_change = Some(v);
                }
                ui.add_space(6.0);
                if spec_button(ui) {
                    open_spec = true;
                }
            });
        });

        ui.add_space(4.0);

        // ── 中部：波形（左右声道各一条；点/拖也能跳转）──
        let wave_h = 52.0;
        let (wrect, wresp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), wave_h),
            egui::Sense::click_and_drag(),
        );
        let p = ui.painter();
        p.rect_filled(wrect, Rounding::same(8.0), tint(Color32::WHITE, 6));
        let frac = ((pos / dur) as f32).clamp(0.0, 1.0);
        if let Some(chans) = wave {
            let lanes = chans.len().max(1);
            let lane_h = (wave_h - 6.0) / lanes as f32;
            for (ci, data) in chans.iter().enumerate() {
                let cy = wrect.top() + 3.0 + lane_h * (ci as f32 + 0.5);
                let half = (lane_h * 0.42).max(2.0);
                let n = data.len().max(1);
                for (i, (mn, mx)) in data.iter().enumerate() {
                    let x = wrect.left() + wrect.width() * (i as f32 + 0.5) / n as f32;
                    let y0 = cy - mx.clamp(-1.0, 1.0) * half;
                    let y1 = cy - mn.clamp(-1.0, 1.0) * half;
                    let played = x <= wrect.left() + wrect.width() * frac;
                    let col = if played { ACCENT } else { tint(Color32::WHITE, 90) };
                    p.line_segment(
                        [egui::pos2(x, y0), egui::pos2(x, y1.max(y0 + 1.0))],
                        Stroke::new(1.2, col),
                    );
                }
            }
            let px = wrect.left() + wrect.width() * frac;
            p.line_segment(
                [egui::pos2(px, wrect.top() + 2.0), egui::pos2(px, wrect.bottom() - 2.0)],
                Stroke::new(1.4, Color32::WHITE),
            );
        } else {
            p.text(
                wrect.center(),
                Align2::CENTER_CENTER,
                if display.map(|i| self.items[i].encrypted.is_some()).unwrap_or(false) {
                    "加密文件：只识别，不解码也不播放"
                } else if has_player {
                    "波形生成中…"
                } else {
                    "双击队列里的文件即可试听"
                },
                FontId::proportional(9.5),
                DIM,
            );
        }
        if has_player && (wresp.clicked() || wresp.dragged()) {
            if let Some(pp) = wresp.interact_pointer_pos() {
                seek = Some(
                    ((pp.x - wrect.left()) / wrect.width()).clamp(0.0, 1.0) as f64 * dur,
                );
            }
        }

        ui.add_space(6.0);

        // ── 底部：进度条 ──
        ui.horizontal(|ui| {
            let time_w = 96.0;
            let bar_w = (ui.available_width() - time_w).max(60.0);
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(bar_w, 16.0),
                egui::Sense::click_and_drag(),
            );
            let p = ui.painter();
            let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
            p.rect_filled(track, Rounding::same(2.0), tint(Color32::WHITE, 16));
            if frac > 0.0 {
                let mut f = track;
                f.set_width(track.width() * frac);
                p.rect_filled(f, Rounding::same(2.0), ACCENT);
            }
            let knob_r = if resp.hovered() || resp.dragged() {
                6.0
            } else {
                4.5
            };
            p.circle_filled(
                egui::pos2(track.left() + track.width() * frac, track.center().y),
                knob_r,
                if has_player { TEXT } else { DIM },
            );
            if has_player && (resp.clicked() || resp.dragged()) && rect.width() > 0.0 {
                if let Some(pp) = resp.interact_pointer_pos() {
                    seek = Some(
                        ((pp.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * dur,
                    );
                }
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "{} / {}",
                    fmt_duration(pos),
                    fmt_duration(dur)
                ))
                .size(9.0)
                .color(DIM),
            );
        });

        if open_spec {
            self.open_spectrum();
        }
        if open_cover {
            if let Some(i) = display {
                self.cover_view = Some(i);
                self.cover_closing = false;
                self.cover_t = 0.0;
                self.start_big_cover(i);
            }
        }
        if let Some(v) = vol_change {
            self.player_volume = v;
            if let Some(p) = self.player.as_ref() {
                p.set_volume(v);
            }
        }
        if let Some(p) = self.player.as_mut() {
            if toggle {
                p.toggle();
            }
            if stop {
                p.stop();
            }
            if let Some(t2) = seek {
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
                            "这个文件没有内嵌封面",
                            egui::FontId::proportional(13.0),
                            MUTED.gamma_multiply(t),
                        );
                    }
                }
                p.text(
                    egui::pos2(screen.center().x, screen.bottom() - 24.0),
                    egui::Align2::CENTER_CENTER,
                    "点击任意处或按 Esc 关闭",
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
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(9.0), tint(Color32::WHITE, 10));
    match tex {
        Some(t) => {
            p.image(
                t.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            p.rect_filled(
                egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)),
                Rounding::same(3.0),
                tint(Color32::WHITE, 18),
            );
        }
    }
    if resp.hovered() && tex.is_some() {
        p.rect_stroke(
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
    let c = if hot { ACCENT } else { MUTED };
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
pub(crate) fn icon_button(ui: &mut egui::Ui, icon: Icon, accent: bool, enabled: bool) -> bool {
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
    } else if accent {
        ACCENT
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
    }
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
