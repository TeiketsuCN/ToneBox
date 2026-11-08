//! 自绘标题栏的原生行为。
//!
//! winit 的 `with_decorations(false)` 把整个非客户区去掉了，窗口就失去了：
//! 拖动、贴边分屏、边缘缩放、双击最大化。
//!
//! 这里用 `SetWindowSubclass` 挂一个 `WM_NCHITTEST` 处理器，把关键区域
//! 重新标记成 `HTCAPTION` / `HTLEFT` 这些，Windows 就会按原生方式处理——
//! 这是 Chromium / VS Code 自绘标题栏用的同一套办法。
//!
//! 注意：一旦这个处理器生效，egui 那边的 `ViewportCommand::StartDrag` 就必须停用，
//! 否则两边同时拖，窗口会跳。

#![cfg(windows)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::MINMAXINFO;
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, ScreenToClient, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, FindWindowW, GetClientRect, GetWindowLongPtrW, IsZoomed, SetWindowLongPtrW,
    GWLP_WNDPROC,
};

/// 原生 `WM_NCHITTEST` 是否已经接管
pub static ACTIVE: AtomicBool = AtomicBool::new(false);

/// 界面缩放（和 egui 的 zoom_factor 同步），0 = 还没设置
static UI_SCALE: AtomicU32 = AtomicU32::new(0);

/// 每次改缩放时同步给命中区计算用。
pub fn set_ui_scale(s: f32) {
    UI_SCALE.store(s.to_bits(), Ordering::Relaxed);
}

/// 最大化时窗口会被系统放大到工作区 + 2×8px 并偏移 (-8,-8)，
/// 这是带 WS_THICKFRAME 的无边框窗口的固有行为（也是 Chromium 处理的那套）。
/// 我们在 UI 里补这 8px。
pub const MAXIMIZE_INSET: f32 = 8.0;

const WM_NCHITTEST: u32 = 0x0084;
const WM_GETMINMAXINFO: u32 = 0x0024;

const HTCLIENT: isize = 1;
const HTCAPTION: isize = 2;
const HTLEFT: isize = 10;
const HTRIGHT: isize = 11;
const HTTOP: isize = 12;
const HTTOPLEFT: isize = 13;
const HTTOPRIGHT: isize = 14;
const HTBOTTOM: isize = 15;
const HTBOTTOMLEFT: isize = 16;
const HTBOTTOMRIGHT: isize = 17;

/// 和 main.rs 里的 TITLEBAR_H 保持一致
const TITLEBAR_H: f32 = 44.0;
const RESIZE_BORDER: f32 = 6.0;
/// 右上角按钮个数（设置 / 图钉 / 最小化 / 最大化 / 关闭）。
///
/// ⚠️ 这个数必须和 main.rs 里 `titlebar()` 实际画的按钮数一致：
/// 少了的话，多出来的按钮会落在 HTCAPTION 区域，点击被系统当成"拖标题栏"，
/// 按钮看着能点、实际没反应（加设置按钮时踩过）。
const TITLE_BUTTONS: f32 = 5.0;
/// main.rs 里标题栏右边距，命中区要把它一起算进去
const TITLEBAR_RIGHT_MARGIN: f32 = 10.0;

static OLD_PROC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    _data: usize,
) -> isize {
    // 无边框窗口最大化时，Windows 会按"有边框"来算，导致窗口比屏幕大 8px、偏移到 (-8,-8)。
    // 这里把最大化尺寸改成显示器工作区，正好贴合。
    if msg == WM_GETMINMAXINFO && lparam != 0 {
        let mmi = &mut *(lparam as *mut MINMAXINFO);
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if !mon.is_null() {
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(mon, &mut info) != 0 {
                let work = info.rcWork;
                let mon_rect = info.rcMonitor;
                mmi.ptMaxPosition.x = work.left - mon_rect.left;
                mmi.ptMaxPosition.y = work.top - mon_rect.top;
                mmi.ptMaxSize.x = work.right - work.left;
                mmi.ptMaxSize.y = work.bottom - work.top;
                return 0;
            }
        }
    }

    if msg == WM_NCHITTEST {
        // 命中区必须跟着"DPI × 界面缩放"一起放大，否则 120% 时按钮边缘会被当成标题栏
        let dpi = GetDpiForWindow(hwnd) as f32 / 96.0;
        let zoom = {
            let bits = UI_SCALE.load(Ordering::Relaxed);
            if bits == 0 {
                1.0
            } else {
                f32::from_bits(bits)
            }
        };
        let scale = dpi * zoom;
        // lparam 是屏幕坐标
        let x = (lparam & 0xFFFF) as i16 as i32;
        let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
        let mut pt = POINT { x, y };
        ScreenToClient(hwnd, &mut pt);

        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetClientRect(hwnd, &mut rc);

        let border = (RESIZE_BORDER * scale) as i32;
        let maxed = IsZoomed(hwnd) != 0;

        // 边缘缩放（最大化时关掉，否则会误触）
        if !maxed && border > 0 {
            let l = pt.x < border;
            let r = pt.x >= rc.right - border;
            let t = pt.y < border;
            let b = pt.y >= rc.bottom - border;
            let hit = match (l, r, t, b) {
                (true, _, true, _) => Some(HTTOPLEFT),
                (_, true, true, _) => Some(HTTOPRIGHT),
                (true, _, _, true) => Some(HTBOTTOMLEFT),
                (_, true, _, true) => Some(HTBOTTOMRIGHT),
                (true, _, _, _) => Some(HTLEFT),
                (_, true, _, _) => Some(HTRIGHT),
                (_, _, true, _) => Some(HTTOP),
                (_, _, _, true) => Some(HTBOTTOM),
                _ => None,
            };
            if let Some(h) = hit {
                return h;
            }
        }

        // 标题栏：右边按钮区留给 egui，其余当原生标题栏
        let title_h = (TITLEBAR_H * scale) as i32;
        let buttons_w = ((TITLEBAR_H + TITLEBAR_RIGHT_MARGIN) * scale * TITLE_BUTTONS) as i32;
        if pt.y < title_h && pt.x < rc.right - buttons_w {
            return HTCAPTION;
        }

        return HTCLIENT;
    }

    let old = OLD_PROC.load(Ordering::SeqCst);
    if old == 0 {
        return 0;
    }
    type WndProc = unsafe extern "system" fn(HWND, u32, usize, isize) -> isize;
    let f: WndProc = std::mem::transmute(old);
    CallWindowProcW(Some(f), hwnd, msg, wparam, lparam)
}

/// 安装子类化 + Win11 圆角。返回是否成功。
pub fn install(title: &str) -> bool {
    unsafe {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() {
            return false;
        }

        // Win11 圆角：32 = DWMWCP_ROUND
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        let round: i32 = 2;
        let _ = windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const i32 as *const _,
            4,
        );

        // 手动子类化：保存原窗口过程，再把自己的挂上去
        let old = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        if old == 0 {
            return false;
        }
        OLD_PROC.store(old, Ordering::SeqCst);
        let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, subclass_proc as usize as isize);
        let ok = prev != 0;
        ACTIVE.store(ok, Ordering::SeqCst);
        ok
    }
}
