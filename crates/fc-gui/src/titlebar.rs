//! 自绘标题栏：logo、名字、铃铛、窗口按钮

#![allow(unused_imports)]

use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use egui::{Align, Align2, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};
use std::path::{Path, PathBuf};
use crate::*;
use crate::util::*;
use crate::badges::*;
use crate::rail::*;
use crate::queue::*;
use crate::player::*;
use crate::spectrum::*;
use crate::detail::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TitleBtn {
    Pin,
    Settings,
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl App {

    /// 自绘标题栏。系统标题栏已经去掉，拖动/按钮都在这里。
    pub(crate) fn titlebar(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        ui.set_height(TITLEBAR_H);

        // 先注册拖动区（后注册的按钮会覆盖在上面，优先接点击）
        let resp = ui.interact(
            full,
            ui.id().with("titlebar_drag"),
            egui::Sense::click_and_drag(),
        );
        #[cfg(windows)]
        let native_frame = frame::ACTIVE.load(std::sync::atomic::Ordering::SeqCst);
        #[cfg(not(windows))]
        let native_frame = false;

        // 原生 WM_NCHITTEST 接管后，拖动由系统处理；再发 StartDrag 会两边抢
        if resp.drag_started() && !native_frame {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            let maxed = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed));
        }

        // 左边：logo + 名字（原来的两行文字去掉）
        let logo = egui::Rect::from_min_size(
            egui::pos2(full.left() + 9.0, full.center().y - 12.0),
            egui::vec2(24.0, 24.0),
        );
        accent_glow(ui.painter(), logo, Rounding::same(7.0), 80);
        ui.painter()
            .rect_filled(logo, Rounding::same(7.0), ACCENT);
        // 里面三条"电平条"：一眼能看出是音频工具
        for (fx, fh) in [(0.30, 0.40), (0.50, 0.64), (0.70, 0.30)] {
            let bx = logo.left() + logo.width() * fx;
            let h = logo.height() * fh;
            ui.painter().rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(bx, logo.center().y),
                    egui::vec2(2.6, h),
                ),
                Rounding::same(1.3),
                BG,
            );
        }
        let name_font = egui::FontId::proportional(13.5);
        ui.painter().text(
            egui::pos2(logo.right() + 10.0, full.center().y),
            egui::Align2::LEFT_CENTER,
            APP_NAME,
            name_font.clone(),
            TEXT,
        );

        // 铃铛：紧跟在名字右边。它落在原生命中区里，frame.rs 那边要把它标成 HTCLIENT，
        // 否则点它会被当成拖标题栏。
        let name_w = ui
            .painter()
            .layout_no_wrap(APP_NAME.to_string(), name_font, TEXT)
            .size()
            .x;
        let bell = egui::Rect::from_min_size(
            egui::pos2(logo.right() + 10.0 + name_w + 10.0, full.center().y - 13.0),
            egui::vec2(26.0, 26.0),
        );
        self.bell_rect = bell;
        #[cfg(windows)]
        frame::set_bell_left(bell.left());
        let bell_resp = ui.interact(bell, ui.id().with("bell"), egui::Sense::click());
        if bell_resp.hovered() {
            ui.painter()
                .circle_filled(bell.center(), 12.0, tint(Color32::WHITE, 16));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        bell_icon(
            ui.painter(),
            bell.shrink(5.0),
            if self.notices_open { TEXT } else { MUTED },
        );
        if self.unread_notices() > 0 {
            let dot = egui::pos2(bell.right() - 4.0, bell.top() + 4.0);
            ui.painter().circle_filled(dot, 4.5, tint(DANGER, 55));
            ui.painter().circle_filled(dot, 3.0, DANGER);
        }
        if bell_resp.clicked() {
            self.notices_open = !self.notices_open;
            if self.notices_open {
                for n in &mut self.notices {
                    n.read = true;
                }
            }
        }

        // 右边：图钉 / 最小化 / 最大化 / 关闭
        let mut rect = egui::Rect::from_min_size(
            egui::pos2(full.right() - TITLEBAR_H, full.top()),
            egui::vec2(TITLEBAR_H, TITLEBAR_H),
        );
        let maxed = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));

        // 从右往左排：关闭、最大化、最小化、图钉
        if title_button(ui, rect, TitleBtn::Close, false) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, if maxed { TitleBtn::Restore } else { TitleBtn::Maximize }, false) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed));
        }
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, TitleBtn::Minimize, false) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, TitleBtn::Pin, self.pinned) {
            self.pinned = !self.pinned;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                if self.pinned {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                },
            ));
        }
        // 设置：放在图钉左边（用户要求"放置顶按钮旁边"）。再点一次就收起来。
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        if title_button(ui, rect, TitleBtn::Settings, self.settings_open) {
            self.settings_open = !self.settings_open;
        }
    }

}

/// 标题栏按钮。图标全部用 painter 画，不依赖字体。
pub(crate) fn title_button(ui: &mut egui::Ui, rect: egui::Rect, kind: TitleBtn, active: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(("tbtn", kind as u8)), egui::Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        let bg = if kind == TitleBtn::Close {
            Color32::from_rgb(196, 60, 60)
        } else {
            tint(Color32::WHITE, 26)
        };
        p.rect_filled(rect, Rounding::same(6.0), bg);
    }
    let c = rect.center();
    let col = if resp.hovered() && kind == TitleBtn::Close {
        Color32::WHITE
    } else if active {
        ACCENT
    } else {
        MUTED
    };
    match kind {
        TitleBtn::Minimize => {
            p.line_segment(
                [c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)],
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Maximize => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)),
                Rounding::same(2.0),
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Restore => {
            // 前面那个窗口
            let front = egui::Rect::from_min_max(
                c + egui::vec2(-4.5, -2.5),
                c + egui::vec2(3.5, 5.5),
            );
            p.rect_stroke(front, Rounding::same(2.0), Stroke::new(1.2, col));
            // 后面那个窗口：只露出上边和右边，构成 L 形
            let back_tl = c + egui::vec2(-2.0, -5.0);
            let back_br = c + egui::vec2(5.0, 2.0);
            p.line_segment(
                [back_tl, egui::pos2(back_br.x, back_tl.y)],
                Stroke::new(1.2, col),
            );
            p.line_segment(
                [
                    egui::pos2(back_br.x - 0.5, back_tl.y),
                    egui::pos2(back_br.x - 0.5, back_br.y),
                ],
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Close => {
            // 两条对角线才是 ×。之前写成 for d in [-4, 4] 再 (-d,-d)→(d,d)，
            // 两轮画的是同一条线，看起来只有一个反斜杠。
            p.line_segment(
                [c + egui::vec2(-4.0, -4.0), c + egui::vec2(4.0, 4.0)],
                Stroke::new(1.2, col),
            );
            p.line_segment(
                [c + egui::vec2(-4.0, 4.0), c + egui::vec2(4.0, -4.0)],
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Pin => {
            // 简化图钉：一个圆 + 一根针；固定时实心
            let head = c + egui::vec2(0.0, -2.0);
            if active {
                p.circle_filled(head, 3.6, col);
            } else {
                p.circle_stroke(head, 3.6, Stroke::new(1.2, col));
            }
            p.line_segment(
                [head + egui::vec2(0.0, 3.8), head + egui::vec2(0.0, 8.0)],
                Stroke::new(1.2, col),
            );
        }
        TitleBtn::Settings => {
            // 三条滑杆（设置图标）。画成齿轮也行，但滑杆在小尺寸下更清楚。
            for (i, dy) in [-4.6_f32, 0.0, 4.6].into_iter().enumerate() {
                p.line_segment(
                    [c + egui::vec2(-6.5, dy), c + egui::vec2(6.5, dy)],
                    Stroke::new(1.2, col),
                );
                let kx = [-2.5_f32, 2.0, -1.0][i];
                p.circle_filled(c + egui::vec2(kx, dy), 1.9, col);
            }
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// 标题栏的铃铛。全部用 painter 画，不赌字体里有没有 🔔。
pub(crate) fn bell_icon(p: &egui::Painter, rect: egui::Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    // 钟身：上面两个大圆角，下面收窄
    p.rect_filled(
        egui::Rect::from_center_size(
            c + egui::vec2(0.0, 0.02 * s),
            egui::vec2(0.58 * s, 0.46 * s),
        ),
        Rounding {
            nw: 0.29 * s,
            ne: 0.29 * s,
            sw: 0.10 * s,
            se: 0.10 * s,
        },
        color,
    );
    // 顶部提手
    p.rect_filled(
        egui::Rect::from_center_size(
            c + egui::vec2(0.0, -0.28 * s),
            egui::vec2(0.16 * s, 0.10 * s),
        ),
        Rounding::same(1.0),
        color,
    );
    // 底沿
    p.rect_filled(
        egui::Rect::from_center_size(
            c + egui::vec2(0.0, 0.20 * s),
            egui::vec2(0.68 * s, 0.09 * s),
        ),
        Rounding::same(0.045 * s),
        color,
    );
    // 铃舌
    p.circle_filled(c + egui::vec2(0.0, 0.31 * s), 0.075 * s, color);
}
