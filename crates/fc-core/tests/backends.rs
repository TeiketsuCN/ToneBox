//! 编码后端表：这条规则值得钉死 —— **有内置就默认内置**，
//! 只有外部实现的格式（APE/TAK/DSD…）才显示具体工具名。

#[test]
fn aac_has_two_backends_and_default_is_builtin() {
    let b = fc_core::backends_for("aac");
    assert_eq!(b.len(), 2);
    assert_eq!(b[0].id, "builtin", "有内置就必须默认内置");
    assert_eq!(b[1].id, "fdkaac");
    assert_eq!(fc_core::backend_tool("aac", "fdkaac"), Some("fdkaac"));
    assert_eq!(fc_core::backend_tool("aac", "builtin"), None);
}

#[test]
fn external_only_formats_report_their_tool() {
    assert_eq!(fc_core::backends_for("ape")[0].id, "mac");
    assert_eq!(fc_core::backend_tool("ape", "mac"), Some("mac"));
    assert_eq!(fc_core::backends_for("tak")[0].id, "takc");
    assert_eq!(fc_core::backends_for("dsf")[0].id, "native");
    assert_eq!(fc_core::backend_tool("dsf", "native"), None);
    // 只有一条路的格式：内置 ffmpeg
    assert_eq!(fc_core::backends_for("flac").len(), 1);
    assert_eq!(fc_core::backends_for("flac")[0].id, "builtin");
}
