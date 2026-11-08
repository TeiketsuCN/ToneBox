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

        // 左边：logo + 名字（原来的两行文字去掉）
        let logo = egui::Rect::from_min_size(
            egui::pos2(full.left() + 9.0, full.center().y - 12.0),
            egui::vec2(24.0, 24.0),
        );
        ui.painter()
            .rect_filled(logo, Rounding::same(7.0), accent());
        // 声音盒子：深色箱体 + 喇叭 + 两道声波（和 exe/快捷方式图标同一套图形）
        let c = logo.center();
        let boxr = egui::Rect::from_center_size(c + egui::vec2(-3.0, 0.0), egui::vec2(12.0, 12.0));
        ui.painter().rect_filled(boxr, Rounding::same(3.0), BG);
        ui.painter()
            .circle_filled(boxr.center() + egui::vec2(-2.0, 0.0), 1.9, accent());
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                boxr.center() + egui::vec2(0.0, -3.2),
                boxr.center() + egui::vec2(4.6, 0.0),
                boxr.center() + egui::vec2(0.0, 3.2),
            ],
            accent(),
            Stroke::NONE,
        ));
        for (dx, h) in [(6.0f32, 4.0f32), (8.5, 7.0)] {
            let tip = c + egui::vec2(dx + 1.5, 0.0);
            ui.painter().line_segment(
                [c + egui::vec2(dx, -h * 0.5), tip],
                Stroke::new(1.4, BG),
            );
            ui.painter().line_segment(
                [tip, c + egui::vec2(dx, h * 0.5)],
                Stroke::new(1.4, BG),
            );
        }

        let name_font = egui::FontId::proportional(13.5);
        let name_w = ui
            .painter()
            .layout_no_wrap(APP_NAME.to_string(), name_font.clone(), TEXT)
            .size()
            .x;
        ui.painter().text(
            egui::pos2(logo.right() + 10.0, full.center().y),
            egui::Align2::LEFT_CENTER,
            APP_NAME,
            name_font.clone(),
            TEXT,
        );
        // 版本与风险提示（测试版，所有操作请自行备份）
        let beta_text = "测试版 beta 1.0";
        let beta_font = egui::FontId::proportional(9.0);
        let beta_x = logo.right() + 10.0 + name_w + 16.0;
        ui.painter().text(
            egui::pos2(beta_x, full.center().y),
            egui::Align2::LEFT_CENTER,
            beta_text,
            beta_font.clone(),
            accent(),
        );
        let beta_w = ui
            .painter()
            .layout_no_wrap(beta_text.to_string(), beta_font, accent())
            .size()
            .x;
        ui.painter().text(
            egui::pos2(beta_x + beta_w + 10.0, full.center().y),
            egui::Align2::LEFT_CENTER,
            "建议所有操作备份 · 慎防设备损坏 请自己负责",
            egui::FontId::proportional(8.5),
            DIM,
        );
        // 点左上角的 logo + 名字 = 回首页（输出页），顺手关掉设置
        let home_rect = egui::Rect::from_min_max(
            egui::pos2(logo.left() - 4.0, full.top()),
            egui::pos2(logo.right() + 10.0 + name_w + 6.0, full.bottom()),
        );
        #[cfg(windows)]
        frame::set_home_zone(home_rect.left(), home_rect.right());
        let home = ui.interact(home_rect, ui.id().with("home"), egui::Sense::click());
        if home.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if home.clicked() {
            if self.page != Page::Convert {
                self.page_t = 0.0;
            }
            self.page = Page::Convert;
            self.settings_open = false;
        }

        // 右边：图钉 / 最小化 / 最大化 / 关闭
        let mut rect = egui::Rect::from_min_size(
            egui::pos2(full.right() - TITLEBAR_H, full.top()),
            egui::vec2(TITLEBAR_H, TITLEBAR_H),
        );

        // 从右往左排：关闭、最大化、最小化、图钉
        if title_button(ui, rect, TitleBtn::Close, false) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
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

        // 转换进度小圆环：在设置左边，尺寸小一点
        rect = rect.translate(egui::vec2(-TITLEBAR_H, 0.0));
        self.bell_rect = rect;
        #[cfg(windows)]
        frame::set_bell_left(rect.left());
        let ring_resp = ui.interact(rect, ui.id().with("bell"), egui::Sense::click());
        if ring_resp.hovered() {
            ui.painter()
                .circle_filled(rect.center(), 11.0, tint(Color32::WHITE, 16));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let (prog, ring_state) = self.batch_ring_state();
        progress_ring(
            ui.painter(),
            rect.shrink(3.0),
            prog,
            ring_state,
            if self.notices_open { TEXT } else { MUTED },
        );
        if ring_resp.clicked() {
            self.notices_open = !self.notices_open;
            if self.notices_open {
                for n in &mut self.notices {
                    n.read = true;
                }
            }
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
        // 高亮只占按钮中间一小块，圆角，不是整格方块
        let hl = egui::Rect::from_center_size(rect.center(), egui::vec2(26.0, 24.0));
        p.rect_filled(hl, Rounding::same(8.0), bg);
    }
    let c = rect.center();
    let col = if resp.hovered() && kind == TitleBtn::Close {
        Color32::WHITE
    } else if active {
        accent()
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

/// 标题栏上的圆环：批处理进度 / 完成打勾 / 出错感叹号。圆里始终是透明的。
pub(crate) fn progress_ring(
    p: &egui::Painter,
    rect: egui::Rect,
    progress: f32,
    state: BatchRing,
    idle_color: Color32,
) {
    let c = rect.center();
    let r = 7.0;
    let w = 2.6;
    // 底环
    p.circle_stroke(c, r, Stroke::new(w, tint(Color32::WHITE, 30)));
    match state {
        BatchRing::Running => {
            let frac = progress.clamp(0.0, 1.0);
            if frac > 0.002 {
                let n = (48.0 * frac) as usize + 2;
                let pts: Vec<egui::Pos2> = (0..=n)
                    .map(|i| {
                        let a = -std::f32::consts::FRAC_PI_2
                            + std::f32::consts::TAU * frac * (i as f32 / n as f32);
                        c + egui::vec2(a.cos(), a.sin()) * r
                    })
                    .collect();
                p.add(egui::Shape::line(pts, Stroke::new(w, accent())));
            }
        }
        BatchRing::Done => {
            p.circle_stroke(c, r, Stroke::new(w, accent()));
            let a = c + egui::vec2(-4.2, 0.2);
            let b = c + egui::vec2(-1.2, 3.2);
            let d = c + egui::vec2(4.4, -3.4);
            p.add(egui::Shape::line(vec![a, b, d], Stroke::new(2.2, accent())));
        }
        BatchRing::Error => {
            p.circle_stroke(c, r, Stroke::new(w, DANGER));
            p.line_segment(
                [c + egui::vec2(0.0, -4.4), c + egui::vec2(0.0, 1.2)],
                Stroke::new(2.2, DANGER),
            );
            p.circle_filled(c + egui::vec2(0.0, 4.2), 1.2, DANGER);
        }
        BatchRing::Idle => {
            p.circle_stroke(c, r, Stroke::new(w, idle_color));
        }
    }
}

/// 圆环的四种状态
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchRing {
    Idle,
    Running,
    Done,
    Error,
}
