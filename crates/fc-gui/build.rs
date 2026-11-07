//! 把图标和版本信息嵌进 exe。
//!
//! 失败不致命：`compile()` 出错只打警告，构建继续（只是没有图标和版本资源）。
//! 这样机器 B 那边的环境即使找不到 windres 也不会卡住。

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("FileDescription", "格式转换器");
    res.set("ProductName", "格式转换器");
    res.set("FileVersion", "0.1.0.0");
    res.set("ProductVersion", "0.1.0.0");
    res.set("CompanyName", "Format Converter (open source)");
    res.set("LegalCopyright", "MIT License");
    res.set_language(0x0804); // 简体中文

    if let Err(e) = res.compile() {
        println!("cargo:warning=图标/版本资源没编进去（不影响运行）: {e}");
    }
}
