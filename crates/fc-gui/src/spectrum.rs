//! 频谱视图

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
use crate::detail::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

impl App {

    /// 打开频谱视图：用当前播放（没在播就用选中项）的文件
    pub(crate) fn open_spectrum(&mut self) {
        let idx = self
            .player_for
            .or(if self.items.is_empty() {
                None
            } else {
                Some(self.selected)
            });
        let Some(i) = idx else { return };
        let Some(item) = self.items.get(i) else { return };
        let Some(tools) = self.tools.clone() else {
            self.spec_err = Some("找不到 ffmpeg，读不了频谱".into());
            self.spec_open = true;
            return;
        };
        let path = item.path.clone();
        let sr = item
            .info
            .as_ref()
            .map(|m| m.sample_rate)
            .filter(|v| *v > 0)
            .unwrap_or(44100);
        self.spec_open = true;
        self.spec_busy = true;
        self.spec_curve.clear();
        self.spec_for = Some(i);
        self.spec_err = None;
        let tx = self.spec_tx.clone();
        std::thread::spawn(move || {
            let r = fc_analyze::spectrum(&tools.ffmpeg, &path, sr, 260)
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
    }

    pub(crate) fn poll_spectrum(&mut self) {
        while let Ok(r) = self.spec_rx.try_recv() {
            self.spec_busy = false;
            match r {
                Ok(c) => self.spec_curve = c,
                Err(e) => self.spec_err = Some(e),
            }
        }
    }

    /// 全屏频谱：对数频率轴 + dB 网格 + 曲线
    pub(crate) fn spectrum_overlay(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.spec_open = false;
            return;
        }
        let screen = ctx.screen_rect();
        let title = self
            .spec_for
            .and_then(|i| self.items.get(i))
            .map(|it| it.tags.display_title(&it.name()))
            .unwrap_or_default();
        let spec = self
            .spec_for
            .and_then(|i| self.items.get(i))
            .and_then(|it| it.info.clone())
            .map(|m| {
                let mut s = m.codec_label().to_string();
                if m.sample_rate > 0 {
                    if is_dsd(&m.codec) {
                        let tier = dsd_tier_label(&m.codec, m.sample_rate, m.bitrate_kbps);
                        if !tier.is_empty() {
                            s.push_str(&format!(" · {tier}"));
                        }
                        s.push_str(&format!(" · {} · 1 bit", fmt_dsd_mhz(m.sample_rate)));
                    } else {
                        s.push_str(&format!(" · {}", fmt_khz(m.sample_rate)));
                        if let Some(b) = m.bits {
                            s.push_str(&format!(" · {b} bit"));
                        }
                    }
                }
                if m.bitrate_kbps > 0 {
                    s.push_str(&format!(" · {} kbps", m.bitrate_kbps));
                }
                s
            })
            .unwrap_or_default();
        let curve = self.spec_curve.clone();
        let busy = self.spec_busy;
        let err = self.spec_err.clone();
        let mut close = false;
        egui::Area::new(egui::Id::new("spectrum_view"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let resp = ui.allocate_response(screen.size(), egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(screen, 0.0, tint(BG, 236));
                if resp.clicked() {
                    close = true;
                }

                let panel = egui::Rect::from_center_size(
                    screen.center() - egui::vec2(0.0, 6.0),
                    egui::vec2(screen.width() * 0.74, 380.0),
                );
                p.rect_filled(panel, Rounding::same(18.0), tint(BG, 228));
                p.rect_stroke(
                    panel,
                    Rounding::same(18.0),
                    Stroke::new(1.0, tint(Color32::WHITE, 26)),
                );

                p.text(
                    panel.min + egui::vec2(24.0, 22.0),
                    egui::Align2::LEFT_CENTER,
                    "频谱",
                    egui::FontId::proportional(13.0),
                    TEXT,
                );
                p.text(
                    panel.min + egui::vec2(66.0, 22.0),
                    egui::Align2::LEFT_CENTER,
                    &title,
                    egui::FontId::proportional(10.5),
                    MUTED,
                );
                p.text(
                    egui::pos2(panel.right() - 24.0, panel.min.y + 22.0),
                    egui::Align2::RIGHT_CENTER,
                    &spec,
                    egui::FontId::proportional(9.0),
                    DIM,
                );

                // 绘图区（留左边给 dB 刻度、下面给频率刻度）
                let plot = egui::Rect::from_min_max(
                    panel.min + egui::vec2(52.0, 46.0),
                    panel.max - egui::vec2(24.0, 40.0),
                );
                p.rect_filled(plot, Rounding::same(8.0), tint(Color32::BLACK, 40));

                // dB 网格：0 / -30 / -60 / -90
                for db in [0.0f32, -30.0, -60.0, -90.0] {
                    let y = plot.bottom() - ((db + 90.0) / 90.0) * plot.height();
                    p.line_segment(
                        [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                        Stroke::new(1.0, tint(Color32::WHITE, 12)),
                    );
                    p.text(
                        egui::pos2(plot.left() - 6.0, y),
                        egui::Align2::RIGHT_CENTER,
                        format!("{db:.0}"),
                        egui::FontId::proportional(8.0),
                        DIM,
                    );
                }
                // 频率网格：100 / 1k / 10k
                let f_lo = 20.0f64;
                let f_hi = 20000.0f64;
                for (f, label) in [
                    (100.0, "100"),
                    (1000.0, "1k"),
                    (10000.0, "10k"),
                ] {
                    let x = plot.left()
                        + ((f / f_lo).log10() / (f_hi / f_lo).log10()) as f32 * plot.width();
                    p.line_segment(
                        [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
                        Stroke::new(1.0, tint(Color32::WHITE, 10)),
                    );
                    p.text(
                        egui::pos2(x, plot.bottom() + 10.0),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(8.0),
                        DIM,
                    );
                }

                if !curve.is_empty() {
                    let pts: Vec<egui::Pos2> = curve
                        .iter()
                        .map(|(f, db)| {
                            let t = ((f / f_lo).log10() / (f_hi / f_lo).log10())
                                .clamp(0.0, 1.0) as f32;
                            let y =
                                plot.bottom() - (((db + 90.0) / 90.0).clamp(0.0, 1.0) as f32) * plot.height();
                            egui::pos2(plot.left() + t * plot.width(), y)
                        })
                        .collect();
                    // 一段一段填（每段是凸四边形），整条曲线不是凸的，不能一次画
                    for w in pts.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        p.add(egui::Shape::convex_polygon(
                            vec![
                                a,
                                b,
                                egui::pos2(b.x, plot.bottom()),
                                egui::pos2(a.x, plot.bottom()),
                            ],
                            tint(ACCENT, 26),
                            Stroke::NONE,
                        ));
                    }
                    p.add(egui::Shape::line(
                        pts,
                        Stroke::new(1.4, ACCENT),
                    ));
                }

                if busy {
                    p.text(
                        plot.center(),
                        egui::Align2::CENTER_CENTER,
                        "正在算频谱…",
                        egui::FontId::proportional(11.0),
                        MUTED,
                    );
                } else if let Some(e) = &err {
                    p.text(
                        plot.center(),
                        egui::Align2::CENTER_CENTER,
                        e,
                        egui::FontId::proportional(10.0),
                        DANGER,
                    );
                } else if curve.is_empty() {
                    p.text(
                        plot.center(),
                        egui::Align2::CENTER_CENTER,
                        "没有可显示的数据",
                        egui::FontId::proportional(10.0),
                        DIM,
                    );
                }

                p.text(
                    egui::pos2(panel.center().x, panel.bottom() - 16.0),
                    egui::Align2::CENTER_CENTER,
                    "点击任意处或按 Esc 关闭",
                    egui::FontId::proportional(8.5),
                    DIM,
                );
            });
        if close {
            self.spec_open = false;
        }
    }

}
