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
    pub fn paint(&self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());

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
        painter.rect_filled(screen, 0.0, Color32::from_rgba_premultiplied(8, 10, 16, 96));
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
