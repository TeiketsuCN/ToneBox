import io

p = r"D:\FormatConverter\app\src\main.rs"
s = io.open(p, encoding="utf-8").read()
anchor = "/// Win11 亚克力背景"
i = s.index(anchor)

tail = r'''/// 只设置深色标题栏。
///
/// 踩过的坑，别再加回去：
/// - DWMWA_SYSTEMBACKDROP_TYPE（亚克力 / Mica）会让窗口变成完全不透明，
///   覆盖掉 winit 的 per-pixel alpha，半透明窗口直接失效。
/// - SetWindowCompositionAttribute 的 ACCENT_ENABLE_ACRYLICBLURBEHIND 同样如此。
/// - DwmExtendFrameIntoClientArea(-1) 在本环境（glow + winit）下也不生效。
/// 结论：要半透明就没有毛玻璃，要毛玻璃就得放弃半透明。本项目选半透明。
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

    // 命令行入口：不走 GUI，直接转换。用来在界面之外验证同一条管线。
    if args.first().map(|s| s.as_str()) == Some("--convert") {
        let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
            eprintln!("用法: format-converter --convert <输入> <输出.flac>");
            std::process::exit(2);
        };
        let tools = ffmpeg::locate().expect("找不到 ffmpeg / ffprobe");
        let info = match ffmpeg::probe(&tools, Path::new(input)) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("无法读取: {e:#}");
                std::process::exit(1);
            }
        };
        println!(
            "输入: {} / {} kbps / {} Hz / {:.1}s{}",
            info.codec_label(),
            info.bitrate_kbps,
            info.sample_rate,
            info.duration_s,
            if info.is_lossy() {
                "  [有损源：转 FLAC 不会增加信息量]"
            } else {
                ""
            }
        );
        let mut step = 0i32;
        if let Err(e) = ffmpeg::convert_flac(
            &tools,
            Path::new(input),
            Path::new(output),
            info.duration_s,
            Box::new(move |p| {
                let s = (p * 10.0) as i32;
                if s != step {
                    step = s;
                    println!("  {:>3}%", s * 10);
                }
            }),
        ) {
            eprintln!("转换失败: {e:#}");
            std::process::exit(1);
        }
        let sz = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
        println!("完成: {output}  ({} KB)", sz / 1024);
        return Ok(());
    }

    let preload = args.first().map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([780.0, 600.0])
            .with_min_inner_size([640.0, 480.0])
            .with_transparent(true)
            .with_title("格式转换器"),
        ..Default::default()
    };
    eframe::run_native(
        "格式转换器",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc);
            if let Some(p) = preload {
                app.load(p);
            }
            Ok(Box::new(app))
        }),
    )
}
'''

io.open(p, "w", encoding="utf-8", newline="\n").write(s[:i] + tail)
print("rewritten, length =", len(s[:i] + tail))
