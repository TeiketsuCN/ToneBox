//! 右侧「任务」列：已加入的转换任务列表 + 开始按钮。

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
use crate::overlays::*;

/// 「添加任务」按钮：和开始转换同样大小，但用次要配色
pub(crate) fn add_task_button(ui: &mut egui::Ui, can: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(w, 40.0),
        if can { egui::Sense::click() } else { egui::Sense::hover() },
    );
    let p = ui.painter();
    let bg = if !can {
        tint(Color32::WHITE, 14)
    } else if resp.hovered() {
        blend(accent(), Color32::WHITE, 0.12)
    } else {
        accent()
    };
    p.rect_filled(rect, Rounding::same(10.0), bg);
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "添加任务",
        FontId::proportional(12.5),
        if can { on_accent() } else { DIM },
    );
    if can && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    can && resp.clicked()
}

impl App {
    pub(crate) fn task_list(&mut self, ui: &mut egui::Ui) -> bool {
        let mut animating = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("任务").size(10.0).color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.tasks.is_empty() {
                    ui.label(
                        RichText::new(format!("{}", self.tasks.len()))
                            .size(9.0)
                            .color(DIM),
                    );
                }
            });
        });
        ui.add_space(8.0);

        if self.tasks.is_empty() {
            para(ui, "还没有任务。", 9.5, DIM);
            return false;
        }

        let mut remove: Option<usize> = None;
        let mut clear_all = false;
        for (i, t) in self.tasks.iter().enumerate() {
            let spec = fc_core::by_id(t.spec_id);
            let fmt = spec.map(|s| s.name).unwrap_or("?");
            let (label, color) = match &t.state {
                State::Pending => ("待转换".to_string(), DIM),
                State::Running(p) => (format!("{:.0}%", p * 100.0), accent()),
                State::Done(_, _) => ("完成".to_string(), accent()),
                State::Failed(_) => ("失败".to_string(), DANGER),
            };
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
            let p = ui.painter();
            if resp.hovered() {
                p.rect_filled(rect, Rounding::same(8.0), tint(Color32::WHITE, 8));
            }
            p.text(
                rect.left_top() + egui::vec2(4.0, 6.0),
                Align2::LEFT_TOP,
                clip(&t.name, 14),
                FontId::proportional(10.5),
                TEXT,
            );
            p.text(
                rect.left_top() + egui::vec2(4.0, 24.0),
                Align2::LEFT_TOP,
                format!("{fmt} · {label}"),
                FontId::proportional(9.0),
                color,
            );
            // 进度条
            if let State::Running(pr) = &t.state {
                let bar = egui::Rect::from_min_size(
                    rect.left_bottom() + egui::vec2(4.0, -6.0),
                    egui::vec2(rect.width() - 8.0, 3.0),
                );
                p.rect_filled(bar, Rounding::same(1.5), tint(Color32::WHITE, 18));
                let mut f = bar;
                f.set_width(bar.width() * pr.clamp(0.0, 1.0));
                p.rect_filled(f, Rounding::same(1.5), accent());
                animating = true;
            }
            // 删除按钮
            let x_rect = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(14.0, 0.0),
                egui::vec2(20.0, 20.0),
            );
            let x_resp = ui.interact(x_rect, ui.id().with(("task_x", i)), egui::Sense::click());
            let x_col = if x_resp.hovered() { TEXT } else { DIM };
            p.line_segment(
                [
                    x_rect.center() + egui::vec2(-4.0, -4.0),
                    x_rect.center() + egui::vec2(4.0, 4.0),
                ],
                Stroke::new(1.2, x_col),
            );
            p.line_segment(
                [
                    x_rect.center() + egui::vec2(4.0, -4.0),
                    x_rect.center() + egui::vec2(-4.0, 4.0),
                ],
                Stroke::new(1.2, x_col),
            );
            if x_resp.clicked() {
                remove = Some(i);
            }
            if resp.clicked() && !matches!(t.state, State::Running(_)) {
                if let Some(item) = self.items.iter().position(|x| x.path == t.path) {
                    self.selected = item;
                }
            }
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if tiny_text_button(ui, "全部清除") {
                clear_all = true;
            }
        });

        if let Some(i) = remove {
            if !matches!(self.tasks[i].state, State::Running(_)) && !self.running {
                self.tasks.remove(i);
            }
        }
        if clear_all && !self.running {
            self.tasks.retain(|t| matches!(t.state, State::Running(_)));
        }
        animating
    }
}

/// 长名字截断（按字符数，中文按 2 个宽度算）
fn clip(s: &str, max_width: usize) -> String {
    let mut w = 0;
    let mut out = String::new();
    for ch in s.chars() {
        let cw = if ch.is_ascii() { 1 } else { 2 };
        if w + cw > max_width {
            out.push('…');
            break;
        }
        w += cw;
        out.push(ch);
    }
    out
}
