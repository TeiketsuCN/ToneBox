//! 歌词管理：内置 .lrc 编辑器。
//!
//! 左边是音乐库里的歌（可搜索），右边编辑同名 `.lrc`；
//! 保存后同步刷新音乐库里的 LRC 标记。

use std::path::PathBuf;

use eframe::egui;

use crate::App;

impl App {
    pub(crate) fn lyrics_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        let mut animating = false;
        ui.horizontal_top(|ui| {
            let list_w = (ui.available_width() * 0.34).clamp(260.0, 380.0);
            let h = ui.available_height();
            ui.allocate_ui_with_layout(
                egui::vec2(list_w, h),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_max_width(list_w);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(crate::tr!("歌词管理"))
                                .size(12.5)
                                .color(crate::TEXT)
                                .strong(),
                        );
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new(crate::tr!("刷新")).size(8.5).color(crate::MUTED))
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, crate::tint(egui::Color32::WHITE, 18)))
                                    .rounding(egui::Rounding::same(6.0)),
                            )
                            .clicked()
                        {
                            for e in &mut self.lib_entries {
                                e.has_lyrics = e.path.with_extension("lrc").exists();
                            }
                            self.lyr_msg = crate::tr!("已刷新歌词配对状态").to_string();
                        }
                    });
                    ui.add_space(6.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.lyr_search)
                            .hint_text(crate::tr!("搜索歌曲 / 艺术家"))
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);

                    let needle = self.lyr_search.trim().to_lowercase();
                    let rows: Vec<(PathBuf, String, String, bool)> = self
                        .lib_entries
                        .iter()
                        .filter(|e| {
                            needle.is_empty()
                                || e.title.to_lowercase().contains(&needle)
                                || e.artist.to_lowercase().contains(&needle)
                        })
                        .map(|e| {
                            (
                                e.path.clone(),
                                e.title.clone(),
                                e.artist.clone(),
                                e.has_lyrics,
                            )
                        })
                        .collect();
                    if let Some(tools) = self.tools.clone() {
                        for (p, _, _, _) in &rows {
                            if !self.lib_covers.contains_key(p)
                                && self.lib_cover_pending.insert(p.clone())
                            {
                                crate::library::spawn_lib_cover(
                                    tools.ffmpeg.clone(),
                                    p.clone(),
                                    self.lib_cover_tx.clone(),
                                );
                            }
                        }
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("lyrics_list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if rows.is_empty() {
                                crate::para(ui, crate::tr!("没有匹配的歌曲。"), 9.0, crate::DIM);
                            }
                            for (path, title, artist, has) in rows {
                                let selected = self.lyr_sel.as_ref() == Some(&path);
                                let label = if has {
                                    format!("{title}  · LRC")
                                } else {
                                    title.clone()
                                };
                                let mut clicked_row = false;
                                ui.horizontal(|ui| {
                                    if let Some(tex) = self.lib_covers.get(&path).and_then(|o| o.as_ref()) {
                                        ui.add(egui::Image::new((tex.id(), egui::vec2(26.0, 26.0))));
                                    } else {
                                        let (r, _) = ui.allocate_exact_size(
                                            egui::vec2(26.0, 26.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            r,
                                            egui::Rounding::same(4.0),
                                            crate::tint(egui::Color32::WHITE, 10),
                                        );
                                    }
                                    let resp = ui.add_sized(
                                        [ui.available_width(), 26.0],
                                        egui::Button::new(
                                            egui::RichText::new(label)
                                                .size(9.5)
                                                .color(if selected { crate::on_accent() } else { crate::TEXT }),
                                        )
                                        .fill(if selected {
                                            crate::accent()
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .stroke(egui::Stroke::new(1.0, crate::tint(egui::Color32::WHITE, 12)))
                                        .rounding(egui::Rounding::same(6.0)),
                                    );
                                    clicked_row = resp.clicked();
                                    resp.on_hover_text(if artist.is_empty() {
                                        path.display().to_string()
                                    } else {
                                        format!("{artist}\n{}", path.display())
                                    });
                                });
                                if clicked_row {
                                    if self.lyr_dirty {
                                        self.lyr_msg = crate::tr!("未保存的修改已丢弃").to_string();
                                    }
                                    self.lyr_sel = Some(path.clone());
                                    self.lyr_dirty = false;
                                    let lrc = path.with_extension("lrc");
                                    self.lyr_text = std::fs::read_to_string(&lrc)
                                        .unwrap_or_else(|_| "[00:00.00]\n".to_string());
                                }
                            }
                        });
                },
            );
            ui.separator();
            ui.vertical(|ui| {
                animating |= self.lyrics_editor(ui, ctx);
            });
        });
        animating
    }

    fn lyrics_editor(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> bool {
        let Some(song) = self.lyr_sel.clone() else {
            crate::para(
                ui,
                crate::tr!("左边选一首歌，右边编辑它的 .lrc；没有就自动新建。"),
                9.5,
                crate::DIM,
            );
            return false;
        };
        let lrc = song.with_extension("lrc");
        let exists = lrc.exists();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(
                    song.file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default(),
                )
                .size(11.0)
                .color(crate::TEXT)
                .strong(),
            );
            ui.label(
                egui::RichText::new(if exists { "LRC" } else { crate::tr!("未创建") })
                    .size(8.5)
                    .color(if exists { crate::accent() } else { crate::DIM }),
            );
        });
        ui.label(
            egui::RichText::new(lrc.display().to_string())
                .size(8.0)
                .color(crate::DIM),
        );
        ui.add_space(6.0);

        let mut save = false;
        ui.horizontal(|ui| {
            let ghost = |label: &str| {
                egui::Button::new(egui::RichText::new(label).size(9.0).color(crate::MUTED))
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.0, crate::tint(egui::Color32::WHITE, 18)))
                    .rounding(egui::Rounding::same(7.0))
            };
            if ui.add(ghost(crate::tr!("保存"))).clicked() {
                save = true;
            }
            if ui.add(ghost(crate::tr!("新建模板"))).clicked() {
                self.lyr_text = "[00:00.00]\n[00:10.00] ".to_string();
                self.lyr_dirty = true;
            }
            let playing = self.player_for.as_deref() == Some(song.as_path());
            if ui.add(ghost(if playing { crate::tr!("停止") } else { crate::tr!("试听") })).clicked() {
                if playing {
                    self.player = None;
                    self.player_for = None;
                } else {
                    self.add_files(vec![song.clone()]);
                    if let Some(idx) = self.items.iter().position(|i| i.path == song) {
                        self.start_play(idx);
                    }
                }
            }
            if ui.add(ghost(crate::tr!("插入时间戳"))).clicked() {
                if let Some(p) = &self.player {
                    let t = p.position_s();
                    let m = (t / 60.0).floor() as u32;
                    let s = t - m as f64 * 60.0;
                    self.lyr_text.push_str(&format!("\\n[{m:02}:{s:05.2}] "));
                    self.lyr_dirty = true;
                }
            }
            if ui.add(ghost(crate::tr!("打开所在文件夹"))).clicked() {
                if let Some(dir) = song.parent() {
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
                    }
                }
            }
            if self.lyr_dirty {
                ui.label(
                    egui::RichText::new(crate::tr!("未保存"))
                        .size(8.5)
                        .color(crate::WARN),
                );
            }
        });
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            save = true;
        }
        if !self.lyr_msg.is_empty() {
            ui.label(
                egui::RichText::new(self.lyr_msg.clone())
                    .size(8.5)
                    .color(crate::DIM),
            );
        }
        ui.add_space(4.0);

        let mut edited = false;
        egui::ScrollArea::vertical()
            .id_salt("lyrics_edit_scroll")
            .max_height((ui.available_height() - 8.0).max(140.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let editor = ui.add(
                    egui::TextEdit::multiline(&mut self.lyr_text)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(40)
                        .hint_text("[00:12.34] 歌词文本"),
                );
                edited = editor.changed();
            });
        if edited {
            self.lyr_dirty = true;
        }

        if save {
            match std::fs::write(&lrc, self.lyr_text.as_bytes()) {
                Ok(()) => {
                    self.lyr_dirty = false;
                    self.lyr_msg = crate::fmt2!("已保存 {}", "Saved {}", lrc.display().to_string());
                    if let Some(e) = self.lib_entries.iter_mut().find(|e| e.path == song) {
                        e.has_lyrics = lrc.exists();
                    }
                }
                Err(err) => {
                    self.lyr_msg = crate::fmt2!("保存失败：{err}", "Save failed: {err}");
                }
            }
        }
        true
    }
}
/// 解析 LRC：返回按时间排序的 (秒, 文本)。
pub(crate) fn parse_lrc(text: &str) -> Vec<(f64, String)> {
    let mut out: Vec<(f64, String)> = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim();
        let mut times: Vec<f64> = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some((m, s)) = tag.split_once(':') {
                if let (Ok(mm), Ok(ss)) = (m.trim().parse::<f64>(), s.trim().parse::<f64>()) {
                    times.push(mm * 60.0 + ss);
                }
            }
            rest = rest[end + 1..].trim_start();
        }
        if times.is_empty() || rest.is_empty() {
            continue;
        }
        for t in times {
            out.push((t, rest.to_string()));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}