//! 壁纸背景 —— "假毛玻璃"。
//!
//! 为什么不用系统亚克力：`DWMWA_SYSTEMBACKDROP_TYPE` / `ACCENT_ENABLE_ACRYLICBLURBEHIND` /
//! `DwmExtendFrameIntoClientArea(-1)` 都会接管合成，废掉 winit 的 per-pixel alpha。
//!
//! 所以照抄 Salt Player 的做法（已逆向确认，见 docs/逆向-SaltPlayer.md）：
//!   读系统壁纸 → 缩小 → 模糊一次 → 按窗口屏幕坐标取 UV → 当背景画。
//!
//! 关键优化：**壁纸只模糊一次**。窗口移动时只改 UV 偏移，不重算模糊，开销接近零。

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, ColorImage, Rect, TextureHandle, TextureOptions};

pub struct WallpaperBg {
    tex: Option<TextureHandle>,
    /// 模糊前的原图尺寸，用于判断是否加载成功
    src: Option<PathBuf>,
}

impl WallpaperBg {
    pub fn new(ctx: &egui::Context) -> Self {
        let src = find_wallpaper();
        let tex = src.as_ref().and_then(|p| {
            // TranscodedWallpaper 没有扩展名，image::open 靠后缀猜格式会失败，
            // 必须用 ImageReader::with_guessed_format 按内容嗅探。
            let img = image::ImageReader::open(p)
                .ok()?
                .with_guessed_format()
                .ok()?
                .decode()
                .ok()?
                .into_rgba8();
            let small = shrink_and_blur(&img, 8, 3);
            Some(ctx.load_texture("wallpaper_bg", small, TextureOptions::LINEAR))
        });
        Self { tex, src }
    }

    pub fn source(&self) -> Option<&Path> {
        self.src.as_deref()
    }

    /// 画在最底层。窗口移动时 UV 跟着走，所以背景"贴着桌面"。
    pub fn paint_mode(
        &self,
        ctx: &egui::Context,
        mode: crate::settings::BgMode,
        dim: u8,
        solid: [u8; 3],
        gradient: u8,
    ) {
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());

        use crate::settings::BgMode;

        if mode == BgMode::Off {
            painter.rect_filled(screen, 0.0, Color32::from_rgb(15, 17, 22));
            return;
        }
        if mode == BgMode::Solid {
            painter.rect_filled(
                screen,
                0.0,
                Color32::from_rgb(solid[0], solid[1], solid[2]),
            );
            return;
        }
        if mode == BgMode::Gradient {
            paint_gradient(&painter, screen, gradient_colors(gradient));
            return;
        }

        let Some(tex) = &self.tex else {
            painter.rect_filled(screen, 0.0, Color32::from_rgb(15, 17, 22));
            return;
        };

        // 窗口在屏幕上的位置（点 = 逻辑像素，比值与 DPI 无关）
        let info = ctx.input(|i| i.viewport().clone());
        let outer = info
            .outer_rect
            .or(info.inner_rect)
            .unwrap_or(Rect::from_min_size(egui::Pos2::ZERO, screen.size()));
        let mon = info
            .monitor_size
            .unwrap_or(egui::vec2(1920.0, 1080.0))
            .max(egui::vec2(1.0, 1.0));

        let uv = Rect::from_min_max(
            egui::pos2(
                (outer.min.x / mon.x).clamp(0.0, 1.0),
                (outer.min.y / mon.y).clamp(0.0, 1.0),
            ),
            egui::pos2(
                (outer.max.x / mon.x).clamp(0.0, 1.0),
                (outer.max.y / mon.y).clamp(0.0, 1.0),
            ),
        );
        painter.image(tex.id(), screen, uv, Color32::WHITE);

        // 压暗：保证任何壁纸上文字都够对比
        let a = dim as u16;
        painter.rect_filled(
            screen,
            0.0,
            Color32::from_rgba_premultiplied(
                (8 * a / 255) as u8,
                (10 * a / 255) as u8,
                (16 * a / 255) as u8,
                dim,
            ),
        );
    }
}

/// 缩小 → 多次盒式模糊（近似高斯）。缩小本身已经损失大量高频，所以代价极低。
fn shrink_and_blur(img: &image::RgbaImage, factor: u32, passes: u32) -> ColorImage {
    let (w, h) = (img.width(), img.height());
    let nw = (w / factor).max(16);
    let nh = (h / factor).max(16);
    let mut small = image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle);

    for _ in 0..passes {
        box_blur(&mut small, 4);
    }
    // 再放大回去一半，避免 egui 里线性放大过度糊
    let up = image::imageops::resize(
        &small,
        (nw * 2).min(w),
        (nh * 2).min(h),
        image::imageops::FilterType::Triangle,
    );

    let pixels = up
        .pixels()
        .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
        .collect();
    ColorImage {
        size: [up.width() as usize, up.height() as usize],
        pixels,
    }
}

/// 可分离盒式模糊
fn box_blur(img: &mut image::RgbaImage, radius: u32) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let r = radius as i32;
    if r <= 0 {
        return;
    }
    let mut buf = img.clone();

    // 横向
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 3];
            let mut n = 0u32;
            for dx in -r..=r {
                let sx = (x + dx).clamp(0, w - 1) as u32;
                let p = buf.get_pixel(sx, y as u32);
                acc[0] += p[0] as u32;
                acc[1] += p[1] as u32;
                acc[2] += p[2] as u32;
                n += 1;
            }
            img.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, 255]),
            );
        }
    }

    // 纵向
    buf = img.clone();
    for x in 0..w {
        for y in 0..h {
            let mut acc = [0u32; 3];
            let mut n = 0u32;
            for dy in -r..=r {
                let sy = (y + dy).clamp(0, h - 1) as u32;
                let p = buf.get_pixel(x as u32, sy);
                acc[0] += p[0] as u32;
                acc[1] += p[1] as u32;
                acc[2] += p[2] as u32;
                n += 1;
            }
            img.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, 255]),
            );
        }
    }
}

fn find_wallpaper() -> Option<PathBuf> {
    // 1. Windows 自己转码好的扁平化壁纸（最稳，任何壁纸类型都有）
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let p = PathBuf::from(appdata).join(r"Microsoft\Windows\Themes\TranscodedWallpaper");
        if p.is_file() {
            return Some(p);
        }
    }
    // 2. 注册表里的原图
    if let Some(p) = reg_wallpaper() {
        if p.is_file() {
            return Some(p);
        }
    }
    // 3. Windows 自带默认壁纸
    if let Some(root) = std::env::var_os("SystemRoot") {
        let p = PathBuf::from(root).join(r"Web\Wallpaper\Windows\img0.jpg");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(windows)]
fn reg_wallpaper() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    let sub: Vec<u16> = "Control Panel\\Desktop\0".encode_utf16().collect();
    let name: Vec<u16> = "WallPaper\0".encode_utf16().collect();
    let mut buf = vec![0u16; 2048];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut len,
        )
    };
    if rc != 0 {
        return None;
    }
    let n = (len as usize / 2).saturating_sub(1);
    let s = String::from_utf16_lossy(&buf[..n]);
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

#[cfg(not(windows))]
fn reg_wallpaper() -> Option<PathBuf> {
    None
}

/// 渐变壁纸的几套配色。都压得比较暗，保证字看得清。
pub const GRADIENTS: [(&str, Color32, Color32, Color32); 10] = [
    // 起点压暗一点保证文字可读，中间和终点给足颜色
    (
        "极光",
        Color32::from_rgb(6, 20, 34),
        Color32::from_rgb(18, 102, 107),
        Color32::from_rgb(55, 194, 160),
    ),
    (
        "晚霞",
        Color32::from_rgb(40, 18, 50),
        Color32::from_rgb(138, 58, 99),
        Color32::from_rgb(224, 138, 74),
    ),
    (
        "青瓷",
        Color32::from_rgb(7, 33, 30),
        Color32::from_rgb(28, 107, 95),
        Color32::from_rgb(134, 199, 166),
    ),
    (
        "深海",
        Color32::from_rgb(3, 15, 28),
        Color32::from_rgb(13, 60, 99),
        Color32::from_rgb(46, 134, 176),
    ),
    (
        "紫罗兰",
        Color32::from_rgb(25, 15, 44),
        Color32::from_rgb(74, 42, 126),
        Color32::from_rgb(165, 102, 208),
    ),
    (
        "森林",
        Color32::from_rgb(6, 21, 12),
        Color32::from_rgb(27, 78, 44),
        Color32::from_rgb(87, 160, 90),
    ),
    (
        "琥珀",
        Color32::from_rgb(28, 18, 6),
        Color32::from_rgb(110, 68, 16),
        Color32::from_rgb(208, 149, 51),
    ),
    (
        "落樱",
        Color32::from_rgb(36, 16, 33),
        Color32::from_rgb(113, 48, 79),
        Color32::from_rgb(210, 126, 151),
    ),
    (
        "钢蓝",
        Color32::from_rgb(13, 20, 27),
        Color32::from_rgb(42, 66, 86),
        Color32::from_rgb(108, 143, 163),
    ),
    (
        "石墨",
        Color32::from_rgb(11, 11, 15),
        Color32::from_rgb(38, 38, 46),
        Color32::from_rgb(85, 85, 95),
    ),
];

pub fn gradient_colors(i: u8) -> [Color32; 3] {
    let (_, a, b, c) = GRADIENTS[(i as usize).min(GRADIENTS.len() - 1)];
    [a, b, c]
}

/// 左上 → 右下的三段渐变。用网格顶点色插值，不画条带、也不用 shader。
fn paint_gradient(painter: &egui::Painter, rect: Rect, stops: [Color32; 3]) {
    const N: usize = 6;
    let mut mesh = egui::Mesh::default();
    for iy in 0..=N {
        for ix in 0..=N {
            let u = ix as f32 / N as f32;
            let v = iy as f32 / N as f32;
            // 斜着走：横向占多、纵向占少，看起来比纯水平渐变有层次
            let t = (u * 0.74 + v * 0.26).clamp(0.0, 1.0);
            let pos = egui::pos2(
                rect.left() + rect.width() * u,
                rect.top() + rect.height() * v,
            );
            mesh.colored_vertex(pos, ramp(stops, t));
        }
    }
    for iy in 0..N {
        for ix in 0..N {
            let i = (iy * (N + 1) + ix) as u32;
            let right = i + 1;
            let down = i + (N + 1) as u32;
            let dr = down + 1;
            mesh.add_triangle(i, right, down);
            mesh.add_triangle(right, dr, down);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// 设置页里的小色块预览
pub fn paint_gradient_preview(painter: &egui::Painter, rect: Rect, stops: [Color32; 3]) {
    let r = rect.shrink(0.5);
    paint_gradient(painter, r, stops);
    painter.rect_stroke(
        r,
        egui::Rounding::same(3.0),
        egui::Stroke::new(0.8, Color32::from_white_alpha(40)),
    );
}

fn ramp(stops: [Color32; 3], t: f32) -> Color32 {
    if t < 0.5 {
        lerp_color(stops[0], stops[1], t * 2.0)
    } else {
        lerp_color(stops[1], stops[2], (t - 0.5) * 2.0)
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}
