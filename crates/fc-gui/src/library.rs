//! 音乐库：列表、扫描、封面、文件夹管理

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
use crate::overlays::*;

impl App {

    /// 底部全局播放条
    /// 音乐库主视图（右侧整块）
    pub(crate) fn library_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        ui.set_width(ui.available_width());
        let mut animating = false;

        let title = match self.lib_tab {
            LibTab::Songs => "歌曲",
            LibTab::Albums => "专辑",
            LibTab::Genres => "风格",
            LibTab::Artists => "艺术家",
            LibTab::Folders => "库",
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(14.0).color(TEXT).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.lib_scanning {
                    ui.label(
                        RichText::new(format!(
                            "扫描中 {} / {}",
                            self.lib_entries.len(),
                            self.lib_total.max(self.lib_entries.len())
                        ))
                        .size(9.0)
                        .color(ACCENT),
                    );
                } else {
                    ui.label(
                        RichText::new(format!("{} 首", self.lib_entries.len()))
                            .size(9.0)
                            .color(DIM),
                    );
                }
            });
        });
        ui.add_space(6.0);

        // 加音乐 / 加文件夹永远在第一行，不藏在「库」页里
        let mut add_files = false;
        let mut add_dir = false;
        let mut rescan = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            add_files = lib_button(ui, "添加音乐", true);
            add_dir = lib_button(ui, "添加文件夹", false);
            rescan = lib_button(ui, "重新扫描", false);
            if self.lib_tab != LibTab::Folders {
                ui.add_space(6.0);
                let w = (ui.available_width() - 8.0).max(120.0);
                if search_box(ui, &mut self.lib_filter, "搜歌名 / 艺术家 / 专辑 / 风格…", w) {
                    animating = true;
                }
            }
        });
        if add_files {
            self.add_library_files();
            animating = true;
        }
        if add_dir {
            self.add_library_folder();
            animating = true;
        }
        if rescan {
            self.scan_library();
            animating = true;
        }
        ui.add_space(10.0);

        if self.cfg.library_dirs.is_empty() && self.cfg.library_files.is_empty() {
            card(ui, WARN, |ui| {
                para(ui, "还没有音乐库", 11.5, WARN);
                ui.add_space(4.0);
                para(
                    ui,
                    "点上面「添加文件夹」扫一个目录，或者「添加音乐」挑几个文件。只读扫描，不搬文件、不改文件。",
                    9.5,
                    MUTED,
                );
            });
            return animating;
        }

        // 有文件夹但还没扫过：给一个明确的入口，别让列表空着让人以为坏了
        if self.lib_entries.is_empty() && !self.lib_scanning && self.lib_tab != LibTab::Folders {
            let mut go = false;
            card(ui, WARN, |ui| {
                para(ui, "还没扫描过音乐库", 11.5, WARN);
                ui.add_space(4.0);
                para(
                    ui,
                    "扫描结果会记在配置里，以后打开直接显示；加了新歌再扫一遍就行。",
                    9.5,
                    MUTED,
                );
                ui.add_space(8.0);
                go = lib_button(ui, "开始扫描", true);
            });
            if go {
                self.scan_library();
            }
            return animating;
        }

        if self.lib_tab == LibTab::Folders {
            animating |= self.library_folders(ui);
            return animating;
        }

        let needle = self.lib_filter.trim().to_lowercase();
        let ids: Vec<usize> = (0..self.lib_entries.len())
            .filter(|&i| {
                if needle.is_empty() {
                    return true;
                }
                let e = &self.lib_entries[i];
                e.title.to_lowercase().contains(&needle)
                    || e.artist.to_lowercase().contains(&needle)
                    || e.album.to_lowercase().contains(&needle)
                    || e.genre.to_lowercase().contains(&needle)
            })
            .collect();
        let playing_path = self.player_for.clone();

        let mut clicked: Option<usize> = None;
        let mut requests: Vec<PathBuf> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("lib_list")
            .auto_shrink([false, false])
            .max_height(ui.available_height() - BAR_RESERVE)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if ids.is_empty() {
                    para(ui, "没找到匹配的歌。", 10.0, DIM);
                    return;
                }
                match self.lib_tab {
                    LibTab::Songs => {
                        let mut rows = ids.clone();
                        rows.sort_by(|&a, &b| {
                            self.lib_entries[a]
                                .title
                                .to_lowercase()
                                .cmp(&self.lib_entries[b].title.to_lowercase())
                        });
                        for i in rows {
                            let e = &self.lib_entries[i];
                            let sel = self.lib_selected.as_deref() == Some(e.path.as_path());
                            let pl = playing_path.as_deref() == Some(e.path.as_path());
                            // 滚到哪加载哪：这一行在视口里才去抽封面
                            let row_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::vec2(ui.available_width(), 48.0),
                            );
                            if ui.clip_rect().intersects(row_rect)
                                && !self.lib_covers.contains_key(&e.path)
                                && !self.lib_cover_pending.contains(&e.path)
                            {
                                requests.push(e.path.clone());
                            }
                            let tex = self.lib_covers.get(&e.path).and_then(|o| o.as_ref());
                            if lib_row(ui, e, pl, sel, tex).clicked() {
                                clicked = Some(i);
                            }
                        }
                    }
                    LibTab::Albums | LibTab::Artists | LibTab::Genres => {
                        let tab = self.lib_tab;
                        let mut rows = ids.clone();
                        rows.sort_by(|&a, &b| {
                            let ka = lib_key(tab, &self.lib_entries[a]).to_lowercase();
                            let kb = lib_key(tab, &self.lib_entries[b]).to_lowercase();
                            ka.cmp(&kb).then_with(|| {
                                self.lib_entries[a]
                                    .title
                                    .to_lowercase()
                                    .cmp(&self.lib_entries[b].title.to_lowercase())
                            })
                        });
                        let mut last = String::from("\u{0}");
                        for i in rows {
                            let key = lib_key(tab, &self.lib_entries[i]);
                            if key != last {
                                last = key.clone();
                                let n = ids
                                    .iter()
                                    .filter(|&&j| lib_key(tab, &self.lib_entries[j]) == key)
                                    .count();
                                let shown = key.trim();
                                lib_group(ui, if shown.is_empty() { "未知" } else { shown }, n);
                            }
                            let e = &self.lib_entries[i];
                            let sel = self.lib_selected.as_deref() == Some(e.path.as_path());
                            let pl = playing_path.as_deref() == Some(e.path.as_path());
                            let row_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::vec2(ui.available_width(), 48.0),
                            );
                            if ui.clip_rect().intersects(row_rect)
                                && !self.lib_covers.contains_key(&e.path)
                                && !self.lib_cover_pending.contains(&e.path)
                            {
                                requests.push(e.path.clone());
                            }
                            let tex = self.lib_covers.get(&e.path).and_then(|o| o.as_ref());
                            if lib_row(ui, e, pl, sel, tex).clicked() {
                                clicked = Some(i);
                            }
                        }
                    }
                    LibTab::Folders => {}
                }
            });

        if !requests.is_empty() {
            if let Some(tools) = self.tools.clone() {
                for p in requests.into_iter().take(64) {
                    if self.lib_cover_pending.insert(p.clone()) {
                        spawn_lib_cover(tools.ffmpeg.clone(), p, self.lib_cover_tx.clone());
                    }
                }
            }
        }

        if let Some(i) = clicked {
            let path = self.lib_entries[i].path.clone();
            let now = ui.input(|i| i.time);
            let dbl = self.lib_last_click.0.as_deref() == Some(path.as_path())
                && now - self.lib_last_click.1 < 0.45;
            self.lib_selected = Some(path.clone());
            self.lib_last_click = (Some(path.clone()), now);
            if dbl {
                self.play_path(path, ctx);
            }
        }
        animating
    }

    pub(crate) fn library_folders(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        if self.cfg.library_dirs.is_empty() && self.cfg.library_files.is_empty() {
            para(
                ui,
                "还没有添加任何东西。用上面的「添加文件夹」扫一个目录，或「添加音乐」挑几个文件。",
                10.0,
                DIM,
            );
        } else {
            ui.label(RichText::new("文件夹").size(9.5).color(DIM));
            ui.add_space(4.0);
        }

        let mut remove: Option<usize> = None;
        for (i, d) in self.cfg.library_dirs.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 30.0),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(rect.shrink2(egui::vec2(0.0, 2.0)), Rounding::same(8.0), tint(Color32::WHITE, 6));
            let label_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(10.0, 0.0),
                egui::pos2(rect.right() - 40.0, rect.bottom()),
            );
            paint_clipped_v(ui, label_rect, 10.0, MUTED, d);
            let xr = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(14.0, 0.0),
                egui::vec2(24.0, 24.0),
            );
            let x = ui.interact(xr, ui.id().with(("lib_dir_del", i)), egui::Sense::click());
            let xc = if x.hovered() { DANGER } else { DIM };
            p.text(xr.center(), egui::Align2::CENTER_CENTER, "×", egui::FontId::proportional(13.0), xc);
            if x.clicked() {
                remove = Some(i);
            }
            ui.add_space(2.0);
        }
        if let Some(i) = remove {
            self.cfg.library_dirs.remove(i);
            self.cfg.save();
            changed = true;
            self.scan_library();
        }

        // 单独加进来的文件（不挂在任何文件夹下）
        if !self.cfg.library_files.is_empty() {
            ui.add_space(10.0);
            ui.label(RichText::new("单独添加的文件").size(9.5).color(DIM));
            ui.add_space(4.0);
        }
        let mut remove_file: Option<usize> = None;
        for (i, f) in self.cfg.library_files.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 30.0),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(
                rect.shrink2(egui::vec2(0.0, 2.0)),
                Rounding::same(8.0),
                tint(Color32::WHITE, 6),
            );
            let label_rect = egui::Rect::from_min_max(
                rect.min + egui::vec2(10.0, 0.0),
                egui::pos2(rect.right() - 40.0, rect.bottom()),
            );
            paint_clipped_v(ui, label_rect, 10.0, MUTED, f);
            let xr = egui::Rect::from_center_size(
                rect.right_center() - egui::vec2(14.0, 0.0),
                egui::vec2(24.0, 24.0),
            );
            let x = ui.interact(xr, ui.id().with(("lib_file_del", i)), egui::Sense::click());
            let xc = if x.hovered() { DANGER } else { DIM };
            p.text(
                xr.center(),
                egui::Align2::CENTER_CENTER,
                "×",
                egui::FontId::proportional(13.0),
                xc,
            );
            if x.clicked() {
                remove_file = Some(i);
            }
            ui.add_space(2.0);
        }
        if let Some(i) = remove_file {
            self.cfg.library_files.remove(i);
            self.cfg.save();
            changed = true;
            self.scan_library();
        }

        ui.add_space(12.0);
        if self.lib_scanning {
            let total = self.lib_total.max(1);
            let frac = (self.lib_entries.len() as f32 / total as f32).clamp(0.0, 1.0);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 6.0),
                egui::Sense::hover(),
            );
            let p = ui.painter();
            p.rect_filled(rect, Rounding::same(3.0), tint(Color32::WHITE, 14));
            let mut f = rect;
            f.set_width(rect.width() * frac);
            p.rect_filled(f, Rounding::same(3.0), ACCENT);
            ui.add_space(4.0);
        }
        if !self.lib_status.is_empty() {
            para(ui, &self.lib_status, 9.0, DIM);
        }
        ui.add_space(4.0);
        para(
            ui,
            "扫描只读：读标签和时长，结果只存在内存里，关掉就没了，下次打开重扫。",
            8.5,
            DIM,
        );
        changed
    }

    /// 「库」页：管理文件夹 + 扫描状态
    /// 挑一个文件夹加进库
    pub(crate) fn add_library_folder(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            let s = p.to_string_lossy().to_string();
            if !self.cfg.library_dirs.contains(&s) {
                self.cfg.library_dirs.push(s);
                self.cfg.save();
                self.scan_library();
            }
        }
    }

    /// 挑几个文件单独加进库（不挂在文件夹下）
    pub(crate) fn add_library_files(&mut self) {
        const EXT: &[&str] = &[
            "mp3", "mp2", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff",
            "wma", "ac3", "eac3", "oma", "aa3", "at3", "at9", "ape", "wv", "tta", "caf", "w64",
        ];
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("音频文件", EXT)
            .pick_files()
        else {
            return;
        };
        let mut added = false;
        for p in paths {
            if !is_audio(&p) {
                continue;
            }
            let s = p.to_string_lossy().to_string();
            if !self.cfg.library_files.contains(&s) {
                self.cfg.library_files.push(s);
                added = true;
            }
        }
        if added {
            self.cfg.save();
            self.scan_library();
        }
    }

    /// 扫一遍所有库目录（后台线程，边扫边回传）
    pub(crate) fn scan_library(&mut self) {
        if self.lib_scanning {
            return;
        }
        let Some(tools) = self.tools.clone() else {
            self.lib_status = "找不到 ffmpeg，扫不了".into();
            return;
        };
        let dirs: Vec<PathBuf> = self
            .cfg
            .library_dirs
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .collect();
        let seed: Vec<PathBuf> = self
            .cfg
            .library_files
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect();
        if dirs.is_empty() && seed.is_empty() {
            self.lib_status = "没有可用的文件夹或文件".into();
            return;
        }
        self.lib_scanning = true;
        self.lib_total = 0;
        self.lib_entries.clear();
        // 重扫就丢掉旧封面缓存（文件可能换了封面）
        self.lib_covers.clear();
        self.lib_cover_pending.clear();
        self.lib_status = "正在扫描…".into();
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || {
            let mut files = seed;
            for d in &dirs {
                collect_audio(d, &mut files, 0);
            }
            files.sort();
            files.dedup();
            let _ = tx.send(LibMsg::Count(files.len()));
            for f in files {
                let Ok(info) = fc_core::probe(&tools, &f) else {
                    continue;
                };
                let tags = fc_core::read_tags(&tools.ffprobe, &f);
                let title = tags.display_title(&file_stem(&f));
                let entry = LibEntry {
                    path: f.clone(),
                    title,
                    artist: tags.artist.unwrap_or_default(),
                    album: tags.album.unwrap_or_default(),
                    genre: tags.genre.unwrap_or_default(),
                    duration_s: info.duration_s,
                    codec: info.codec_label().to_string(),
                    bits: info.bits,
                    sample_rate: info.sample_rate,
                    bitrate_kbps: info.bitrate_kbps,
                    lossy: info.is_lossy(),
                };
                let _ = tx.send(LibMsg::Entry(entry));
            }
            let _ = tx.send(LibMsg::Done);
        });
    }

    pub(crate) fn poll_library(&mut self) {
        while let Ok(m) = self.lib_rx.try_recv() {
            match m {
                LibMsg::Count(n) => self.lib_total = n,
                LibMsg::Entry(e) => self.lib_entries.push(e),
                LibMsg::Done => {
                    self.lib_scanning = false;
                    self.lib_status = format!("扫描完成：{} 首", self.lib_entries.len());
                    // 结果写进配置：下次启动直接铺出来，不用再扫
                    self.cfg.library_cache = self
                        .lib_entries
                        .iter()
                        .map(|e| settings::CachedTrack {
                            path: e.path.to_string_lossy().to_string(),
                            title: e.title.clone(),
                            artist: e.artist.clone(),
                            album: e.album.clone(),
                            genre: e.genre.clone(),
                            duration_s: e.duration_s,
                            codec: e.codec.clone(),
                            bits: e.bits,
                            sample_rate: e.sample_rate,
                            bitrate_kbps: e.bitrate_kbps,
                            lossy: e.lossy,
                        })
                        .collect();
                    self.cfg.save();
                }
            }
        }
    }

    /// 懒加载封面的回传：解码好的 RGBA 在这里变成 GPU 纹理
    pub(crate) fn poll_lib_covers(&mut self, ctx: &egui::Context) {
        while let Ok((path, payload)) = self.lib_cover_rx.try_recv() {
            self.lib_cover_pending.remove(&path);
            let tex = payload.map(|(rgba, w, h)| {
                let pixels: Vec<Color32> = rgba
                    .chunks_exact(4)
                    .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
                    .collect();
                let img = egui::ColorImage {
                    size: [w as usize, h as usize],
                    pixels,
                };
                self.lib_cover_seq += 1;
                ctx.load_texture(
                    format!("libcover_{}", self.lib_cover_seq),
                    img,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.lib_covers.insert(path, tex);
        }
    }

    /// 音乐库里双击一首歌：不在队列里就先加进去（顺便读标签/封面），然后播
    pub(crate) fn play_path(&mut self, path: PathBuf, _ctx: &egui::Context) {
        if !path.exists() {
            // 缓存里记着的文件可能已经被移走/删掉，明确告诉用户，别静默失败
            self.lib_status = format!("文件不在了：{}", path.display());
            return;
        }
        if !self.items.iter().any(|i| i.path == path) {
            self.add_files(vec![path.clone()]);
        }
        if let Some(idx) = self.items.iter().position(|i| i.path == path) {
            self.selected = idx;
            self.start_play(idx);
        }
    }

}

/// 音乐库的一首歌：封面 + 标题 + 艺术家·专辑 + 编码 + 音质标识 + 时长
pub(crate) fn lib_row(
    ui: &mut egui::Ui,
    e: &LibEntry,
    playing: bool,
    selected: bool,
    cover: Option<&egui::TextureHandle>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 50.0),
        egui::Sense::click(),
    );
    let p = ui.painter();
    let hovered = resp.hovered();
    let fill = if selected {
        tint(ACCENT, 26)
    } else if hovered {
        tint(Color32::WHITE, 8)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect.shrink2(egui::vec2(0.0, 1.0)), Rounding::same(8.0), fill);
    }
    let left = rect.left() + 10.0;
    if playing {
        p.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(left - 6.0, rect.center().y - 9.0),
                egui::vec2(3.0, 18.0),
            ),
            Rounding::same(1.5),
            ACCENT,
        );
    }
    // 封面：37×37，圆角，没图给一块浅底
    let cover_rect = egui::Rect::from_min_size(
        egui::pos2(left, rect.center().y - 18.5),
        egui::vec2(37.0, 37.0),
    );
    p.rect_filled(cover_rect, Rounding::same(7.0), tint(Color32::WHITE, 10));
    if let Some(t) = cover {
        p.image(
            t.id(),
            cover_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        p.text(
            cover_rect.center(),
            egui::Align2::CENTER_CENTER,
            "♪",
            egui::FontId::proportional(13.0),
            DIM,
        );
    }

    // 右边的音质标识（上方）+ 时长（下方）
    let q = if e.codec == "DSD" {
        Some(Quality::Dsd(fc_core::dsd_tier(
            &e.codec,
            e.sample_rate,
            e.bitrate_kbps,
        )))
    } else {
        quality_from(e.lossy, e.sample_rate, e.bits, e.bitrate_kbps)
    };
    let badge_w = q
        .map(|q| quality_width(ui.ctx(), q, e.bitrate_kbps))
        .unwrap_or(0.0);
    let text_right = rect.right() - 12.0 - badge_w.max(46.0) - 8.0;
    let text_x = cover_rect.right() + 10.0;
    let title_rect = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top() + 7.0),
        egui::pos2(text_right, rect.top() + 27.0),
    );
    let sub_rect = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.top() + 26.0),
        egui::pos2(text_right, rect.bottom() - 5.0),
    );
    paint_clipped(
        ui,
        title_rect,
        12.0,
        if playing { ACCENT } else { TEXT },
        &e.title,
    );
    // 副标题：编码 · 艺术家 · 专辑（编码放前面，用户一眼知道是什么格式）
    let mut parts: Vec<String> = vec![e.codec.clone()];
    if !e.artist.is_empty() {
        parts.push(e.artist.clone());
    }
    if !e.album.is_empty() {
        parts.push(e.album.clone());
    }
    if !e.genre.is_empty() {
        parts.push(e.genre.clone());
    }
    paint_clipped(ui, sub_rect, 9.5, MUTED, &parts.join(" · "));

    if let Some(q) = q {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 12.0 - badge_w, rect.top() + 6.0),
            egui::vec2(badge_w, 17.0),
        );
        let t = ui.input(|i| i.time);
        if paint_quality(ui.painter(), r, q, e.bitrate_kbps, t) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(45));
        }
    }
    p.text(
        egui::pos2(rect.right() - 12.0, rect.top() + 33.0),
        egui::Align2::RIGHT_CENTER,
        fmt_duration(e.duration_s),
        egui::FontId::proportional(9.5),
        DIM,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 分组标题（专辑 / 艺术家 / 风格）+ 一条分隔线
pub(crate) fn lib_group(ui: &mut egui::Ui, title: &str, count: usize) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(11.5).color(TEXT).strong());
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{count} 首")).size(9.0).color(DIM));
    });
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 1.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, tint(Color32::WHITE, 10));
    ui.add_space(3.0);
}

/// 分组用的键
pub(crate) fn lib_key(tab: LibTab, e: &LibEntry) -> String {
    match tab {
        LibTab::Albums => e.album.clone(),
        LibTab::Artists => e.artist.clone(),
        LibTab::Genres => e.genre.clone(),
        _ => String::new(),
    }
}

/// 非空唯一值个数（专辑 / 艺术家 / 风格）
pub(crate) fn uniq(entries: &[LibEntry], key: impl Fn(&LibEntry) -> &String) -> usize {
    let mut set = std::collections::HashSet::new();
    for e in entries {
        let k = key(e).trim();
        if !k.is_empty() {
            set.insert(k);
        }
    }
    set.len()
}

pub(crate) fn file_stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string_lossy().to_string())
}

pub(crate) fn is_audio(p: &Path) -> bool {
    const EXT: &[&str] = &[
        "mp3", "mp2", "flac", "m4a", "m4b", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff",
        "wma", "ac3", "eac3", "oma", "aa3", "at3", "at9", "ape", "wv", "tta", "caf", "w64",
        "dsf", "dff", "mka", "wavpack",
    ];
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            EXT.contains(&e.as_str())
        })
        .unwrap_or(false)
}

/// 递归收集音频文件。深度和数量都设上限，避免误加 C:\ 这种目录卡死。
pub(crate) fn collect_audio(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 8 || out.len() > 20000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for ent in rd.flatten() {
        let p = ent.path();
        let Ok(ft) = ent.file_type() else { continue };
        if ft.is_dir() {
            if p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            collect_audio(&p, out, depth + 1);
        } else if ft.is_file() && is_audio(&p) {
            out.push(p);
        }
        if out.len() > 20000 {
            return;
        }
    }
}

/// 后台抽音乐库封面：48px 左右就够列表用，别把整张图塞进显存
pub(crate) fn spawn_lib_cover(
    ffmpeg: PathBuf,
    path: PathBuf,
    tx: std::sync::mpsc::Sender<(PathBuf, Option<(Vec<u8>, u32, u32)>)>,
) {
    std::thread::spawn(move || {
        let payload = fc_core::extract_cover(&ffmpeg, &path).and_then(|bytes| {
            let img = image::load_from_memory(&bytes).ok()?;
            let small = img.resize_to_fill(40, 40, image::imageops::FilterType::Triangle);
            let rgba = small.to_rgba8();
            Some((rgba.as_raw().to_vec(), rgba.width(), rgba.height()))
        });
        let _ = tx.send((path, payload));
    });
}
