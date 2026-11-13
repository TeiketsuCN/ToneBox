fn main() -> anyhow::Result<()> {
    let tools = fc_core::locate().expect("ffmpeg");
    let exe = fc_core::find_speexenc().expect("speexenc");
    let input = std::path::Path::new(r"D:\FormatConverter\out\gapless\covered.flac");
    let output = std::path::Path::new(r"D:\FormatConverter\out\fmt_test\speex_app.spx");
    let size = fc_core::external::speex_convert(
        &tools,
        &exe,
        input,
        output,
        2.0,
        Box::new(|p| println!("progress {p:.2}")),
    )?;
    println!("size={size}");
    Ok(())
}
