//! 左侧导航：两个大项、格式列表、音乐库分类、用户配置

#![allow(unused_imports)]

use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use egui::{Align, Align2, Color32, FontId, Layout, RichText, Rounding, Stroke, Vec2};
use std::path::{Path, PathBuf};
use crate::*;
use crate::util::*;
use crate::badges::*;
use crate::titlebar::*;
use crate::queue::*;
use crate::player::*;
use crate::spectrum::*;
use crate::detail::*;
use crate::output::*;
use crate::library::*;
use crate::overlays::*;

/// 配置列表里 ⋯ 菜单的按钮：透明底、左对齐，别用 egui 默认那个灰块
fn menu_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(label).size(10.5).color(TEXT))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(6.0))
            .min_size(egui::vec2(96.0, 24.0)),
    )
    .clicked()
}

impl App {

    /// 左侧导航：两个大项（音乐库 / 输出），点开哪个就显示哪一块
    pub(crate) fn left_rail(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let mut animating = false;

        if rail_big(ui, "音乐库", self.page == Page::Library) {
            if self.page != Page::Library {
                self.page_t = 0.0;
            }
            self.page = Page::Library;
            // 不自动扫描：结果在配置里，用户点「重新扫描」才更新
            if self.lib_entries.is_empty()
                && !self.lib_scanning
                && (!self.cfg.library_dirs.is_empty() || !self.cfg.library_files.is_empty())
            {
                self.lib_status = "点「重新扫描」扫一遍音乐库".into();
            }
        }
        if rail_big(ui, "输出", self.page == Page::Convert) {
            if self.page != Page::Convert {
                self.page_t = 0.0;
            }
            self.page = Page::Convert;
        }
        ui.add_space(10.0);

        match self.page {
            Page::Convert => animating |= self.format_list(ui),
            Page::Library => self.library_nav(ui),
        }
        animating
    }

    /// 用户配置：列表 + 重命名 + 导出/导入（现在放在设置页里）
    pub(crate) fn profile_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if tiny_text_button(ui, "＋ 新建配置") {
                self.cfg.add_profile();
                self.profile_msg = "已新建一套配置".into();
            }
            if !self.profile_msg.is_empty() {
                ui.label(RichText::new(&self.profile_msg).size(9.0).color(accent()));
            }
        });
        ui.add_space(2.0);

        let mut switch_to: Option<usize> = None;
        let mut start_rename: Option<usize> = None;
        let mut remove: Option<usize> = None;
        let mut apply_rename: Option<usize> = None;
        let mut cancel_rename = false;

        for i in 0..self.cfg.profiles.len() {
            if self.rename_idx == Some(i) {
                ui.horizontal(|ui| {
                    // 缓冲从空开始、把原名当提示：直接打字就是新名字，
                    // 不改直接回车则保持原名（否则会像之前那样接在原名后面）
                    let hint = self.cfg.profiles[i].name.clone();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.rename_buf)
                            .hint_text(hint)
                            .desired_width((ui.available_width() - 6.0).max(60.0))
                            .margin(egui::vec2(6.0, 3.0)),
                    );
                    if self.rename_focus {
                        resp.request_focus();
                        self.rename_focus = false;
                    }
                    if resp.lost_focus() || ui.input(|x| x.key_pressed(egui::Key::Enter)) {
                        apply_rename = Some(i);
                    }
                    if ui.input(|x| x.key_pressed(egui::Key::Escape)) {
                        cancel_rename = true;
                    }
                });
                continue;
            }

            let on = i == self.cfg.active_profile;
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 24.0),
                egui::Sense::click(),
            );
            let p = ui.painter();
            let fill = if on {
                tint(accent(), 26)
            } else if resp.hovered() {
                tint(Color32::WHITE, 10)
            } else {
                Color32::TRANSPARENT
            };
            if fill != Color32::TRANSPARENT {
                p.rect_filled(rect, Rounding::same(7.0), fill);
            }
            let name_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(8.0, 0.0),
                egui::pos2(rect.right() - 26.0, rect.bottom()),
            );
            paint_clipped_v(
                ui,
                name_rect,
                10.5,
                if on { accent() } else { MUTED },
                &self.cfg.profiles[i].name,
            );
            // 改名按钮（✎）
            let er = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(12.0, 0.0),
                egui::vec2(20.0, 20.0),
            );
            let edit = ui.interact(er, ui.id().with(("prof_more", i)), egui::Sense::click());
            // 三个点自己画：字体里没有 "⋯" 的字形，文字会变豆腐块
            let dot_c = if edit.hovered() { TEXT } else { DIM };
            for dx in [-4.0f32, 0.0, 4.0] {
                p.circle_filled(er.center() + egui::vec2(dx, 0.0), 1.3, dot_c);
            }
            let popup_id = egui::Id::new(("prof_popup", i));
            if edit.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            } else if resp.clicked() {
                switch_to = Some(i);
            }
            egui::popup::popup_below_widget(
                ui,
                popup_id,
                &edit,
                egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(104.0);
                    if menu_button(ui, "重命名") {
                        start_rename = Some(i);
                        ui.memory_mut(|m| m.close_popup());
                    }
                    if menu_button(ui, "删除这套") {
                        remove = Some(i);
                        ui.memory_mut(|m| m.close_popup());
                    }
                },
            );
            resp.context_menu(|ui| {
                if menu_button(ui, "重命名") {
                    start_rename = Some(i);
                    ui.close_menu();
                }
                if menu_button(ui, "删除这套配置") {
                    remove = Some(i);
                    ui.close_menu();
                }
            });
        }

        if let Some(i) = switch_to {
            self.cfg.switch_profile(i);
        }
        if let Some(i) = start_rename {
            self.rename_idx = Some(i);
            self.rename_buf.clear();
            self.rename_focus = true;
        }
        if let Some(i) = apply_rename {
            // 空输入 = 不改名
            if !self.rename_buf.trim().is_empty() {
                let name = self.rename_buf.clone();
                self.cfg.rename_profile(i, name);
            }
            self.rename_idx = None;
            self.rename_buf.clear();
        }
        if cancel_rename {
            self.rename_idx = None;
        }
        if let Some(i) = remove {
            self.cfg.remove_profile(i);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if tiny_text_button(ui, "导出") {
                self.export_profiles();
            }
            if tiny_text_button(ui, "导入") {
                self.import_profiles();
            }
        });
        if !self.profile_msg.is_empty() {
            ui.add_space(2.0);
            para(ui, &self.profile_msg, 8.5, DIM);
        }
    }

    /// 把全部配置导出成一个 JSON（换机器/备份用）
    pub(crate) fn export_profiles(&mut self) {
        self.cfg.snapshot_active();
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("format-converter-profiles.json")
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };
        let payload = serde_json::json!({
            "app": "format-converter",
            "version": 1,
            "active": self.cfg.active_profile,
            "profiles": self.cfg.profiles,
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => match std::fs::write(&path, s) {
                Ok(_) => self.profile_msg = format!("已导出 {}", path.display()),
                Err(e) => self.profile_msg = format!("导出失败：{e}"),
            },
            Err(e) => self.profile_msg = format!("导出失败：{e}"),
        }
    }

    /// 导入一份导出的 JSON，追加成新配置（重名自动加后缀）
    pub(crate) fn import_profiles(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        #[derive(serde::Deserialize)]
        struct Pkg {
            #[serde(default)]
            profiles: Vec<settings::Profile>,
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.profile_msg = format!("导入失败：{e}");
                return;
            }
        };
        let pkg: Pkg = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => {
                self.profile_msg = "导入失败：这不是配置 JSON".into();
                return;
            }
        };
        if pkg.profiles.is_empty() {
            self.profile_msg = "导入失败：文件里没有配置".into();
            return;
        }
        self.cfg.snapshot_active();
        let first = self.cfg.profiles.len();
        let mut added = 0;
        for mut p in pkg.profiles {
            let base = if p.name.trim().is_empty() {
                "导入的配置".to_string()
            } else {
                p.name.trim().to_string()
            };
            let mut name = base.clone();
            let mut k = 2;
            while self.cfg.profiles.iter().any(|q| q.name == name) {
                name = format!("{base} ({k})");
                k += 1;
            }
            p.name = name;
            self.cfg.profiles.push(p);
            added += 1;
        }
        self.cfg.active_profile = first.min(self.cfg.profiles.len() - 1);
        self.cfg.apply_active();
        self.cfg.save();
        self.profile_msg = format!("已导入 {added} 套配置");
    }

    /// 转换页的格式列表（就是原来的 sidebar，底部提示已经提到 left_rail）
    pub(crate) fn format_list(&mut self, ui: &mut egui::Ui) -> bool {
        ui.set_width(ui.available_width());
        let dt = self.dt;
        let mut animating = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new("输出格式").size(11.0).color(DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("百科").size(9.0).color(MUTED))
                            .fill(tint(Color32::WHITE, 8))
                            .stroke(line(Color32::WHITE, 16))
                            .rounding(Rounding::same(6.0)),
                    )
                    .on_hover_text("看看每个格式到底是什么")
                    .clicked()
                {
                    // 打开时对准当前输出格式：用户多半是"想知道我正在转的这个是什么"
                    self.wiki_sel = self.target.id;
                    self.wiki_open = true;
                }
            });
        });
        ui.add_space(4.0);

        // 展开状态变化时，把高度动画归零重来
        if self.blurb_id != self.expanded {
            self.blurb_id = self.expanded;
            self.blurb_t = 0.0;
        }
        // 只在这里驱动一次，不能放进每行的循环（会互相覆盖导致永远振荡）
        animating |= approach(
            &mut self.blurb_t,
            if self.expanded.is_some() { 1.0 } else { 0.0 },
            dt,
            14.0,
        );

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // 紧凑排布：全部格式一屏放下，不做折叠
                ui.spacing_mut().item_spacing.y = 1.0;
                // 按钮默认内边距 7px 会把 24px 的行撑到 28px，侧栏挤不下
                ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);

                for group in Group::all() {
                    // 列出该组全部格式：能编码的可以选为输出，
                    // 只读的也列出来——用户需要知道"手上这个是什么、为什么不能输出成它"
                    let specs: Vec<&'static FormatSpec> =
                        fc_core::all().filter(|f| f.group == group).collect();
                    if specs.is_empty() {
                        continue;
                    }

                    ui.add_space(6.0);
                    ui.label(RichText::new(group.title()).size(9.5).color(DIM));
                    ui.add_space(1.0);

                    for spec in specs {
                        animating |= self.format_row(ui, spec, dt);
                    }
                }

                // 视频容器也是正式格式行（见 Group::Video），只留一行很短的边界说明：
                // 详细的"容器收什么编码"在输出列和百科里，不在这里堆字
                ui.add_space(8.0);
                para(ui, "视频只处理第一条画面和音轨。", 8.5, DIM);
            });

        animating
    }

    /// 音乐库页左侧：歌曲 / 专辑 / 风格 / 艺术家 / 库
    pub(crate) fn library_nav(&mut self, ui: &mut egui::Ui) {
        let songs = self.lib_entries.len();
        let albums = uniq(&self.lib_entries, |e| &e.album);
        let genres = uniq(&self.lib_entries, |e| &e.genre);
        let artists = uniq(&self.lib_entries, |e| &e.artist);
        let rows: [(LibTab, &str, usize); 5] = [
            // 「库」放最前：加音乐/加文件夹都在这里，别让人找
            (LibTab::Folders, "库", self.cfg.library_dirs.len() + self.cfg.library_files.len()),
            (LibTab::Songs, "歌曲", songs),
            (LibTab::Albums, "专辑", albums),
            (LibTab::Genres, "风格", genres),
            (LibTab::Artists, "艺术家", artists),
        ];
        for (tab, label, n) in rows {
            if rail_sub(ui, label, n, self.lib_tab == tab) {
                self.lib_tab = tab;
            }
        }

        ui.add_space(12.0);
        if self.lib_scanning {
            let total = self.lib_total.max(self.lib_entries.len());
            para(
                ui,
                &format!("扫描中… {}/{}", self.lib_entries.len(), total),
                9.0,
                accent(),
            );
        } else if !self.lib_status.is_empty() {
            para(ui, &self.lib_status, 9.0, DIM);
        }
    }

    /// 画一行输出格式（含展开后的小字）
    pub(crate) fn format_row(
        &mut self,
        ui: &mut egui::Ui,
        spec: &'static FormatSpec,
        dt: f32,
    ) -> bool {
        let mut animating = false;
        // 编码器分两类：ffmpeg 自带的，和用户自己装的外部编码器（atracdenc）
        let external = fc_core::external_encoder(spec.id);
        let enc = fc_core::encoder_for(spec.id);
        let enc_ok = match external {
            Some(_) => self.atracdenc.is_some(),
            None => enc.map(|e| self.encoders.contains(e)).unwrap_or(true),
        };
        let selectable = spec.can_encode;
        // 编码器缺失时不能选为输出，否则转换时才炸
        let usable = selectable && enc_ok;
        let selected = usable && spec.id == self.target.id;

        let cur = {
            match self.fmt_sel.iter_mut().find(|(id, _)| *id == spec.id) {
                Some((_, v)) => {
                    animating |= approach(v, if selected { 1.0 } else { 0.0 }, dt, 16.0);
                    *v
                }
                None => 0.0,
            }
        };

        let fill = blend(Color32::TRANSPARENT, tint(accent(), 34), cur);
        let stroke_c = blend(Color32::TRANSPARENT, tint(accent(), 80), cur);
        let base = if !enc_ok {
            DIM
        } else if selectable {
            TEXT
        } else {
            MUTED
        };
        let text_c = blend(base, accent(), cur);

        let mut label = RichText::new(spec.name).size(11.5).color(text_c);
        if !enc_ok {
            label = label.strikethrough();
        }
        let mut row = egui::Button::new(label)
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke_c))
            .rounding(Rounding::same(8.0))
            .min_size(egui::vec2(ui.available_width(), 22.0));
        if !usable {
            row = row.fill(Color32::TRANSPARENT).stroke(Stroke::NONE);
        }

        let mut resp = ui.add(row);
        if !enc_ok {
            let msg = match external {
                Some(name) => format!(
                    "你的系统里还没装 {name}（ATRAC 外部编码器）\n装好后放到程序目录或 PATH，再到设置里点「重新检测」。\n它不随本程序分发，装不装由你决定。"
                ),
                None => format!(
                    "你的机器上似乎没有这个编码器（{}）\n换个 ffmpeg 构建可能就有，或者别选这个格式。",
                    enc.unwrap_or("?")
                ),
            };
            resp = resp.on_hover_text(msg);
        } else if !selectable {
            resp = resp.on_hover_text("只读格式：能解码，不能作为输出目标");
        }
        if resp.clicked() {
            // 只读/缺编码器的格式不改输出目标，只展开解释
            if usable {
                self.target = spec;
            }
            self.expanded = if self.expanded == Some(spec.id) {
                None
            } else {
                Some(spec.id)
            };
        }

        // 小字：高度 + 透明度一起动
        let expanded_here = self.expanded == Some(spec.id);
        if self.blurb_t > 0.01 && expanded_here {
            // 左边留 6px 缩进，所以排版宽度要比可用宽度再窄 6px，
            // 否则文字会被自己的裁剪矩形切掉右边（长文案换行后就露出来了）
            let avail_w = (ui.available_width() - 4.0).max(60.0);
            let text_w = (avail_w - 6.0).max(50.0);
            // 侧栏用小字说明（三行以内）。`wiki.rs` 的长文案留给以后的百科页，
            // 塞进侧栏会把列表撑得没法看。
            let galley = ui.painter().layout(
                spec.blurb.to_string(),
                egui::FontId::proportional(9.5),
                MUTED,
                text_w,
            );
            let full_h = galley.size().y + 3.0;
            let h = full_h * self.blurb_t;
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(avail_w, h), egui::Sense::hover());
            ui.painter()
                .with_clip_rect(rect)
                .galley(rect.min + egui::vec2(6.0, 0.0), galley, MUTED);
        }

        animating
    }

    /// 这个目标格式现在能不能真的转出来（含外部工具的判断）。
    ///
    /// 侧栏、百科页、"设为输出格式"按钮都用它，免得三处各写一套条件。
    pub(crate) fn target_available(&self, spec: &FormatSpec) -> bool {
        if !spec.can_encode {
            return false;
        }
        match fc_core::external_encoder(spec.id) {
            Some(_) => self.atracdenc.is_some(),
            None => fc_core::encoder_for(spec.id)
                .map(|e| self.encoders.contains(e))
                .unwrap_or(true),
        }
    }

}

/// 左侧两个大项之一。整行 34 高，选中时青色底
pub(crate) fn rail_big(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 34.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let fill = if on {
        tint(accent(), 30)
    } else if resp.hovered() {
        tint(Color32::WHITE, 12)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect, Rounding::same(9.0), fill);
    }
    p.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.5),
        if on { accent() } else { TEXT },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// 音乐库分类的一行（右边带数量）
pub(crate) fn rail_sub(ui: &mut egui::Ui, label: &str, count: usize, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 26.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let fill = if on {
        tint(accent(), 26)
    } else if resp.hovered() {
        tint(Color32::WHITE, 10)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect, Rounding::same(8.0), fill);
    }
    p.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.0),
        if on { accent() } else { MUTED },
    );
    if count > 0 {
        p.text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            egui::FontId::proportional(9.0),
            DIM,
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
