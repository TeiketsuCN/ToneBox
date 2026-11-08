//! 打印本机音频设备（调试 / 验证用）：
//! `cargo run -p fc-player --example devices`

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        for d in fc_player::devices::list()? {
            println!(
                "[{}] {:<8} {:<10} {:<5} {} | {} | {}",
                if d.is_default { "默认" } else { "    " },
                match d.flow {
                    fc_player::devices::Flow::Output => "输出",
                    fc_player::devices::Flow::Input => "输入",
                },
                d.kind.label(),
                if d.is_bluetooth { "蓝牙" } else { "" },
                d.name,
                d.description,
                d.format_label,
            );
        }
    }
    Ok(())
}
