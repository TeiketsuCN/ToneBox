import io
p = r"D:\FormatConverter\crates\fc-gui\src\main.rs"
s = io.open(p, encoding="utf-8").read()

# 1) chip 里的标签禁止换行
s = s.replace(
"""        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(9.5).color(color));
        });""",
"""        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).size(9.5).color(color)).wrap_mode(egui::TextWrapMode::Extend));
        });""")

# 2) 菜单按钮字形：⋯ 在雅黑里没有，换成 ASCII
s = s.replace('ui.menu_button(RichText::new("⋯")', 'ui.menu_button(RichText::new("···")')

# 3) 右侧详情面板：锁死最大宽度，避免卡片溢出
s = s.replace(
"""        ui.label(RichText::new("详情").size(11.0).color(DIM));
        ui.add_space(8.0);
        ui.label(RichText::new(it.name()).size(13.0).color(TEXT).strong());""",
"""        let w = ui.available_width();
        ui.set_max_width(w);
        ui.label(RichText::new("详情").size(11.0).color(DIM));
        ui.add_space(8.0);
        ui.add(egui::Label::new(RichText::new(it.name()).size(13.0).color(TEXT).strong()).truncate());""")

# 4) 队列行里的错误文字截断
s = s.replace(
"""                                    None => error.clone().unwrap_or_else(|| "读取失败".into()),""",
"""                                    None => {
                                        let e = error.clone().unwrap_or_else(|| "读取失败".into());
                                        let first: String = e.lines().next().unwrap_or("").chars().take(46).collect();
                                        format!("{first}…")
                                    }""")

io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("patched")