//! 格式转换器 —— 三栏形态
//!
//! 布局：左（输出格式 + 该格式的小字解释） / 中（队列） / 右（选中文件详情）
//!
//! 三条已经定下来的产品决定：
//!   1. 输出格式默认全局一个，单个文件可以在队列里单独覆盖
//!   2. 支持一次拖多个文件
//!   3. 格式百科不单独开页面，挂在格式自己身上——点它，下面出小字
//!
//! 渲染注意：eframe 0.29 的 glow 后端对 `from_rgba_unmultiplied` 会二次 gamma
//! （alpha=10 实测渲染成 55% 白）。所有半透明色必须走 premultiplied，见 `tint()`。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

use eframe::egui;
use egui::{Align, Color32, Layout, RichText, Rounding, Stroke};

use fc_core::{estimate_size, FormatSpec, Group, MediaInfo, Tools};

// ── 配色 ────────────────────────────────────────────────
const TEXT: Color32 = Color32::from_rgb(240, 244, 250);
const MUTED: Color32 = Color32::from_rgb(148, 159, 178);
const DIM: Color32 = Color32::from_rgb(104, 114, 132);
const ACCENT: Color32 = Color32::from_rgb(120, 226, 212);
const WARN: Color32 = Color32::from_rgb(255, 198, 122);
const DANGER: Color32 = Color32::from_rgb(255, 138, 138);
const BG: Color32 = Color32::from_rgb(17, 19, 26);

// ── 布局规范 ────────────────────────────────────────────
// 面板宽度必须是常量，且内容宽度在面板内锁死（ui.set_width）。
// 所有长文本一律显式 wrap，任何元素都不允许横向溢出把窗口边缘切掉。
const LEFT_W: f32 = 216.0;
const RIGHT_W: f32 = 300.0;
const PANEL_PAD_X: f32 = 14.0;

fn tint(c: Color32, a: u8) -> Color32 {
    let k = a as u16;
    Color32::from_rgba_premultiplied(
        (c.r() as u16 * k / 255) as u8,
        (c.g() as u16 * k / 255) as u8,
        (c.b() as u16 * k / 255) as u8,
        a,
    )
}

fn line(c: Color32, a: u8) -> Stroke {
    Stroke::new(1.0, tint(c, a))
}

// ── 队列条目 ────────────────────────────────────────────
#[derive(Clone)]
enum State {
    Pending,
    Running(f32),
    Done(PathBuf, u64),
    Failed(String),
}

struct Item {
    path: PathBuf,
    info: Option<MediaInfo>,
    error: Option<String>,
    state: State,
    /// 单文件覆盖的目标格式 id；None 表示跟随全局
    override_id: Option<&'static str>,
}

impl Item {
    fn spec(&self, global: &'static FormatSpec) -> &'static FormatSpec {
        self.override_id
            .and_then(fc_core::by_id)
            .unwrap_or(global)
    }
    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

enum Msg {
    Progress(usize, f32),
    Done(usize, PathBuf, u64),
    Failed(usize, String),
    Finished,
}

struct App {
    tools: Option<Tools>,
    items: Vec<Item>,
    selected: usize,
    target: &'static FormatSpec,
    expanded: Option<&'static str>,
    rx: Option<Receiver<Msg>>,
    running: bool,
    batch_total: usize,
    batch_done: usize,
    frames: u64,
    backdrop_done: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::TRANSPARENT;
        style.visuals.window_fill = Color32::TRANSPARENT;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        cc.egui_ctx.set_style(style);

        let target = fc_core::default_target();
        App {
            tools: fc_core::locate(),
            items: Vec::new(),
            selected: 0,
            target,
            expanded: Some(target.id),
            rx: None,
            running: false,
            batch_total: 0,
            batch_done: 0,
            frames: 0,
            backdrop_done: false,
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let Some(tools) = self.tools.clone() else { return };
        for p in paths {
            if self.items.iter().any(|i| i.path == p) {
                continue;
            }
            let (info, error) = match fc_core::probe(&tools, &p) {
                Ok(i) => (Some(i), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            };
            self.items.push(Item {
                path: p,
                info,
                error,
                state: State::Pending,
                override_id: None,
            });
        }
        if !self.items.is_empty() && self.selected >= self.items.len() {
            self.selected = 0;
        }
    }

    fn start_batch(&mut self, ctx: &egui::Context) {
        let Some(tools) = self.tools.clone() else { return };
        let mut jobs = Vec::new();
        for (i, it) in self.items.iter_mut().enumerate() {
            if it.info.is_none() || !matches!(it.state, State::Pending) {
                continue;
            }
            let spec = it.spec(self.target);
            let out = unique_output(&it.path, spec.ext);
            let dur = it.info.as_ref().map(|m| m.duration_s).unwrap_or(0.0);
            jobs.push((i, it.path.clone(), out, spec.args, dur));
            it.state = State::Running(0.0);
        }
        if jobs.is_empty() {
            return;
        }
        self.running = true;
        self.batch_total = jobs.len();
        self.batch_done = 0;

        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx2 = ctx.clone();

        std::thread::spawn(move || {
            for (idx, input, out, args, dur) in jobs {
                let tx_p = tx.clone();
                let ctx_p = ctx2.clone();
                let res = fc_core::convert(
                    &tools,
                    &input,
                    &out,
                    args,
                    dur,
                    Box::new(move |p| {
                        let _ = tx_p.send(Msg::Progress(idx, p));
                        ctx_p.request_repaint();
                    }),
                );
                match res {
                    Ok(()) => {
                        let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                        let _ = tx.send(Msg::Done(idx, out, size));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::Failed(idx, format!("{e:#}")));
                    }
                }
                ctx2.request_repaint();
            }
            let _ = tx.send(Msg::Finished);
        });
    }

    fn poll(&mut self) {
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            match m {
                Msg::Progress(i, p) => {
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = State::Running(p);
                    }
                }
                Msg::Done(i, path, size) => {
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = State::Done(path, size);
                    }
                    self.batch_done += 1;
                }
                Msg::Failed(i, e) => {
                    if let Some(it) = self.items.get_mut(i) {
                        it.state = State::Failed(e);
                    }
                    self.batch_done += 1;
                }
                Msg::Finished => {
                    self.running = false;
                    self.rx = None;
                }
            }
        }
    }

    fn overall(&self) -> f32 {
        if self.batch_total == 0 {
            return 0.0;
        }
        let cur = self
            .items
            .iter()
            .filter_map(|i| match &i.state {
                State::Running(p) => Some(*p as f64),
                _ => None,
            })
            .sum::<f64>() as f32;
        ((self.batch_done as f32 + cur) / self.batch_total as f32).clamp(0.0, 1.0)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if !self.backdrop_done && self.frames > 3 {
            self.backdrop_done = apply_backdrop();
        }
        self.poll();

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|d| d.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }

        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 246))
                    .inner_margin(egui::Margin::symmetric(20.0, 14.0)),
            )
            .show(ctx, |ui| self.header(ui));

        egui::SidePanel::left("formats")
            .exact_width(LEFT_W)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 242))
                    .inner_margin(egui::Margin::symmetric(12.0, 14.0)),
            )
            .show(ctx, |ui| self.sidebar(ui));

        egui::SidePanel::right("detail")
            .exact_width(RIGHT_W)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 242))
                    .inner_margin(egui::Margin::symmetric(16.0, 14.0)),
            )
            .show(ctx, |ui| self.detail(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(tint(BG, 240))
                    .inner_margin(egui::Margin::symmetric(16.0, 14.0)),
            )
            .show(ctx, |ui| self.queue(ui, ctx));
    }
}

fn unique_output(input: &Path, ext: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let dir = input.parent().unwrap_or(Path::new("."));
    let mut cand = dir.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while cand.exists() {
        cand = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    cand
}

fn fmt_duration(s: f64) -> String {
    let total = s.round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn fmt_size(b: u64) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / 1048576.0)
    } else {
        format!("{} KB", b / 1024)
    }
}

impl App {
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("格式转换器").size(19.0).color(TEXT).strong());
            ui.add_space(10.0);
            ui.label(RichText::new("转换 · 质量判断 · 格式解释").size(11.0).color(DIM));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (label, color) = if self.tools.is_some() {
                    ("ffmpeg 就绪", ACCENT)
                } else {
                    ("缺 ffmpeg", DANGER)
                };
                egui::Frame::none()
                    .fill(tint(color, 26))
                    .stroke(line(color, 70))
                    .rounding(Rounding::same(20.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 3.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(label).size(10.5).color(color));
                    });
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("输出格式").size(11.0).color(DIM));
        ui.add_space(6.0);

        for group in [Group::Lossless, Group::Lossy, Group::Device] {
            let items: Vec<&'static FormatSpec> =
                fc_core::browsable().filter(|f| f.group == group).collect();
            if items.is_empty() {
                continue;
            }
            ui.add_space(8.0);
            ui.label(RichText::new(group.title()).size(10.0).color(DIM));
            ui.add_space(2.0);

            for spec in items {
                let selected = spec.id == self.target.id;
                let row = egui::Button::new(
                    RichText::new(spec.name)
                        .size(13.0)
                        .color(if selected { ACCENT } else { TEXT }),
                )
                .fill(if selected {
                    tint(ACCENT, 22)
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(if selected {
                    line(ACCENT, 60)
                } else {
                    Stroke::NONE
                })
                .rounding(Rounding::same(9.0))
                .min_size(egui::vec2(ui.available_width(), 30.0));

                if ui.add(row).clicked() {
                    self.target = spec;
                    self.expanded = if self.expanded == Some(spec.id) {
                        None
                    } else {
                        Some(spec.id)
                    };
                }

                if self.expanded == Some(spec.id) {
                    ui.add_space(3.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(spec.blurb)
                                .size(10.0)
                                .color(MUTED)
                                .line_height(Some(15.0)),
                        );
                    });
                    ui.add_space(4.0);
                }
            }
        }

        // 底部固定区
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new("只处理未加密文件\n不支持任何 DRM")
                    .size(9.5)
                    .color(DIM),
            );
        });
    }

    fn queue(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.set_width(ui.available_width());
        if self.items.is_empty() {
            self.drop_zone(ui, ctx);
            return;
        }

        let mut remove: Option<usize> = None;
        let count = self.items.len();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 62.0)
            .show(ui, |ui| {
                for i in 0..count {
                    let is_sel = i == self.selected;
                    let name = self.items[i].name();
                    let info = self.items[i].info.clone();
                    let error = self.items[i].error.clone();
                    let spec = self.items[i].spec(self.target);
                    let state = self.items[i].state.clone();
                    let override_id = self.items[i].override_id;

                    let frame = egui::Frame::none()
                        .fill(if is_sel {
                            tint(ACCENT, 16)
                        } else {
                            tint(Color32::WHITE, 8)
                        })
                        .stroke(if is_sel {
                            line(ACCENT, 55)
                        } else {
                            line(Color32::WHITE, 18)
                        })
                        .rounding(Rounding::same(12.0))
                        .inner_margin(egui::Margin::symmetric(14.0, 11.0));

                    let inner_w = ui.available_width();
                    let resp = frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let row_w = ui.available_width();
                            ui.vertical(|ui| {
                                ui.set_max_width((row_w - 168.0).max(140.0));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&name).size(12.5).color(TEXT),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                                ui.add_space(2.0);
                                let sub = match &info {
                                    Some(m) => format!(
                                        "{} · {} kbps · {}  →  {}",
                                        m.codec_label(),
                                        m.bitrate_kbps,
                                        fmt_duration(m.duration_s),
                                        spec.name
                                    ),
                                    None => {
                                        let e = error.clone().unwrap_or_else(|| "读取失败".into());
                                        let first: String = e.lines().next().unwrap_or("").chars().take(46).collect();
                                        format!("{first}…")
                                    }
                                };
                                ui.add(
                                    egui::Label::new(RichText::new(sub).size(10.0).color(DIM))
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                match &state {
                                    State::Pending => {
                                        ui.label(
                                            RichText::new("待转换").size(10.0).color(MUTED),
                                        );
                                    }
                                    State::Running(p) => {
                                        ui.add(
                                            egui::ProgressBar::new(*p)
                                                .desired_width(78.0)
                                                .fill(ACCENT),
                                        );
                                    }
                                    State::Done(_, sz) => {
                                        ui.label(
                                            RichText::new(format!("✓ {}", fmt_size(*sz)))
                                                .size(10.0)
                                                .color(ACCENT),
                                        );
                                    }
                                    State::Failed(_) => {
                                        ui.label(RichText::new("✗").size(11.0).color(DANGER));
                                    }
                                }

                                // 单文件覆盖目标格式
                                ui.menu_button(RichText::new("···").size(13.0).color(DIM), |ui| {
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
                                });

                                if ui
                                    .small_button(RichText::new("×").size(11.0).color(DIM))
                                    .clicked()
                                {
                                    remove = Some(i);
                                }
                            });
                        });
                    });

                    if resp
                        .response
                        .interact(egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.selected = i;
                    }

                    ui.add_space(6.0);
                }
            });

        if let Some(i) = remove {
            self.items.remove(i);
            self.selected = self.selected.min(self.items.len().saturating_sub(1));
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let can = !self.running
                && self.tools.is_some()
                && self
                    .items
                    .iter()
                    .any(|i| i.info.is_some() && matches!(i.state, State::Pending));
            let btn = egui::Button::new(
                RichText::new(if self.running {
                    "转换中…"
                } else {
                    "开始转换"
                })
                .size(13.0)
                .color(if can {
                    Color32::from_rgb(10, 26, 24)
                } else {
                    DIM
                }),
            )
            .fill(if can { ACCENT } else { tint(Color32::WHITE, 10) })
            .stroke(if can { Stroke::NONE } else { line(Color32::WHITE, 18) })
            .rounding(Rounding::same(9.0))
            .min_size(egui::vec2(112.0, 34.0));

            if ui.add_enabled(can, btn).clicked() {
                self.start_batch(ctx);
            }

            if ui
                .add_enabled(
                    !self.running,
                    egui::Button::new(RichText::new("添加文件").size(12.0).color(MUTED))
                        .fill(Color32::TRANSPARENT)
                        .stroke(line(Color32::WHITE, 20))
                        .rounding(Rounding::same(9.0))
                        .min_size(egui::vec2(86.0, 34.0)),
                )
                .clicked()
            {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter(
                        "音频",
                        &[
                            "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma", "ape",
                            "oma", "at3", "aa3", "aiff", "aif",
                        ],
                    )
                    .pick_files()
                {
                    self.add_files(paths);
                }
            }

            if self.running {
                ui.add(
                    egui::ProgressBar::new(self.overall())
                        .desired_width(ui.available_width() - 8.0)
                        .fill(ACCENT),
                );
            } else if ui
                .add(
                    egui::Button::new(RichText::new("清空").size(12.0).color(DIM))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(9.0)),
                )
                .clicked()
            {
                self.items.clear();
                self.selected = 0;
            }
        });
    }

    fn drop_zone(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let mut do_pick = false;
        let avail = ui.available_height();
        egui::Frame::none()
            .fill(tint(Color32::WHITE, 6))
            .stroke(line(Color32::WHITE, 20))
            .rounding(Rounding::same(16.0))
            .inner_margin(egui::Margin::symmetric(20.0, 16.0))
            .show(ui, |ui| {
                ui.set_min_height(avail - 40.0);
                let pad = ((avail - 190.0) * 0.5).max(10.0);
                ui.add_space(pad);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("把音频文件拖到这里").size(15.0).color(TEXT));
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new("可以一次拖多个 · MP3 / FLAC / WAV / M4A / OGG / Opus / ATRAC")
                            .size(10.5)
                            .color(MUTED),
                    );
                    ui.add_space(16.0);
                    let btn = egui::Button::new(
                        RichText::new("选择文件").size(12.5).color(TEXT),
                    )
                    .fill(tint(Color32::WHITE, 16))
                    .stroke(line(Color32::WHITE, 32))
                    .rounding(Rounding::same(9.0));
                    if ui.add(btn).clicked() {
                        do_pick = true;
                    }
                });
            });
        if do_pick {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter(
                    "音频",
                    &[
                        "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma", "ape", "oma",
                        "at3", "aa3", "aiff", "aif",
                    ],
                )
                .pick_files()
            {
                self.add_files(paths);
            }
        }
    }

    fn detail(&mut self, ui: &mut egui::Ui) {
        // 规范：内容宽度锁死，任何子元素不许横向溢出
        ui.set_width(ui.available_width());

        let Some(it) = self.items.get(self.selected) else {
            ui.label(RichText::new("详情").size(10.5).color(DIM));
            ui.add_space(10.0);
            para(ui, "拖入文件后，这里显示源文件的真实参数、质量判断和解释。", 10.5, DIM);
            return;
        };

        ui.label(RichText::new("详情").size(10.5).color(DIM));
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(RichText::new(it.name()).size(12.5).color(TEXT).strong())
                .wrap_mode(egui::TextWrapMode::Truncate),
        );
        ui.add_space(10.0);

        if let Some(m) = &it.info {
            ui.horizontal(|ui| {
                chip(ui, m.codec_label(), ACCENT);
            });
            ui.add_space(10.0);

            egui::Grid::new("info_grid")
                .num_columns(2)
                .spacing([12.0, 7.0])
                .show(ui, |ui| {
                    if m.bitrate_kbps > 0 {
                        row(ui, "码率", &format!("{} kbps", m.bitrate_kbps));
                    }
                    if m.sample_rate > 0 {
                        row(ui, "采样率", &format!("{} Hz", m.sample_rate));
                    }
                    if m.channels > 0 {
                        row(ui, "声道", &format!("{}", m.channels));
                    }
                    if m.duration_s > 0.0 {
                        row(ui, "时长", &fmt_duration(m.duration_s));
                    }
                    if m.size_bytes > 0 {
                        row(ui, "体积", &fmt_size(m.size_bytes));
                    }
                    let spec = it.spec(self.target);
                    let est = estimate_size(m, spec);
                    if est > 0 {
                        row(ui, "输出", &format!("{} ≈ {}", spec.name, fmt_size(est)));
                    }
                });

            let spec = it.spec(self.target);
            if m.is_lossy()
                && matches!(
                    spec.kind,
                    fc_core::Kind::Lossless | fc_core::Kind::Uncompressed
                )
            {
                ui.add_space(14.0);
                card(ui, WARN, |ui| {
                    para(ui, "不会增加任何信息量", 11.5, WARN);
                    ui.add_space(5.0);
                    para(
                        ui,
                        &format!(
                            "源文件是 {} 有损压缩。转成 {} 只是换了个更大的容器，文件会变大，声音不会变好。",
                            m.codec_label(),
                            spec.name
                        ),
                        10.0,
                        MUTED,
                    );
                });
            }
        }

        if let Some(e) = &it.error {
            ui.add_space(14.0);
            card(ui, DANGER, |ui| {
                para(ui, "读不出来", 11.5, DANGER);
                ui.add_space(5.0);
                para(ui, e, 9.5, MUTED);
            });
        }

        match &it.state {
            State::Done(path, size) => {
                ui.add_space(14.0);
                card(ui, ACCENT, |ui| {
                    para(ui, "转换完成", 11.5, ACCENT);
                    ui.add_space(5.0);
                    para(
                        ui,
                        &format!(
                            "{}　{}",
                            path.file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            fmt_size(*size)
                        ),
                        9.5,
                        MUTED,
                    );
                });
            }
            State::Failed(e) => {
                ui.add_space(14.0);
                card(ui, DANGER, |ui| {
                    para(ui, "转换失败", 11.5, DANGER);
                    ui.add_space(5.0);
                    para(ui, e, 9.5, MUTED);
                });
            }
            _ => {}
        }
    }
}

/// 段落文字：永远是 wrap 的，杜绝溢出
fn para(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    ui.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
}

/// 键值行
fn row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.label(RichText::new(key).size(9.5).color(DIM));
    ui.add(
        egui::Label::new(RichText::new(value).size(10.0).color(TEXT))
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
    ui.end_row();
}

fn card(ui: &mut egui::Ui, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(tint(color, 16))
        .stroke(line(color, 50))
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(13.0))
        .show(ui, add);
}

fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(tint(color, 20))
        .stroke(line(color, 50))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).size(9.5).color(color)).wrap_mode(egui::TextWrapMode::Extend));
        });
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(fam)
                    .or_default()
                    .insert(0, "cjk".to_owned());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

/// 只设置深色标题栏。
///
/// 踩过的坑，别再加回去：DWMWA_SYSTEMBACKDROP_TYPE、ACCENT_ENABLE_ACRYLICBLURBEHIND、
/// DwmExtendFrameIntoClientArea(-1) 三者任意一个都会覆盖 per-pixel alpha，
/// 让半透明窗口变成全不透明。
#[cfg(windows)]
fn apply_backdrop() -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    let title: Vec<u16> = "格式转换器\0".encode_utf16().collect();
    unsafe {
        let hwnd: HWND = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let dark: i32 = 1;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const _,
            4,
        ) == 0
    }
}

#[cfg(not(windows))]
fn apply_backdrop() -> bool {
    true
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.first().map(|s| s.as_str()) == Some("--convert") {
        let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
            eprintln!("用法: format-converter --convert <输入> <输出.flac>");
            std::process::exit(2);
        };
        let tools = fc_core::locate().expect("找不到 ffmpeg / ffprobe");
        let info = match fc_core::probe(&tools, Path::new(input)) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("无法读取: {e:#}");
                std::process::exit(1);
            }
        };
        let spec = fc_core::default_target();
        println!(
            "输入: {} / {} kbps / {} Hz / {:.1}s",
            info.codec_label(),
            info.bitrate_kbps,
            info.sample_rate,
            info.duration_s
        );
        if let Err(e) = fc_core::convert(
            &tools,
            Path::new(input),
            Path::new(output),
            spec.args,
            info.duration_s,
            Box::new(|_| {}),
        ) {
            eprintln!("转换失败: {e:#}");
            std::process::exit(1);
        }
        println!("完成: {output}");
        return Ok(());
    }

    let preload: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 660.0])
            .with_min_inner_size([960.0, 560.0])
            .with_transparent(true)
            .with_title("格式转换器"),
        ..Default::default()
    };
    eframe::run_native(
        "格式转换器",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc);
            if !preload.is_empty() {
                app.add_files(preload);
            }
            Ok(Box::new(app))
        }),
    )
}
