import re
import pathlib

ROOT = pathlib.Path(r"C:\Users\23501\Desktop\FormatConverter-source-20261114-1922\crates\fc-gui\src")

PATS = [
    (r'RichText::new\(\s*"', 'RichText::new(crate::tr!("'),
    (r'Label::new\(\s*"', 'Label::new(crate::tr!("'),
    (r'Button::new\(\s*"', 'Button::new(crate::tr!("'),
    (r'ui\.label\(\s*"', 'ui.label(crate::tr!("'),
    (r'ui\.heading\(\s*"', 'ui.heading(crate::tr!("'),
    (r'ui\.button\(\s*"', 'ui.button(crate::tr!("'),
    (r'para\(ui,\s*"', 'para(ui, crate::tr!("'),
    (r'section\(ui,\s*"', 'section(ui, crate::tr!("'),
    (r'on_hover_text\(\s*"', 'on_hover_text(crate::tr!("'),
    (r'hint_text\(\s*"', 'hint_text(crate::tr!("'),
]

CJK = re.compile(r'[\u4e00-\u9fff]')
total = 0

for f in ROOT.glob("*.rs"):
    s = f.read_text(encoding="utf-8")
    orig = s
    for pat, rep in PATS:
        out = []
        pos = 0
        for m in re.finditer(pat, s):
            if m.start() < pos:
                continue
            i = m.end()
            j = i
            while j < len(s):
                if s[j] == "\\":
                    j += 2
                    continue
                if s[j] == '"':
                    break
                j += 1
            lit = s[i:j]
            out.append(s[pos:m.start()])
            if CJK.search(lit):
                out.append(rep + lit + '")')
                total += 1
            else:
                out.append(s[m.start():j + 1])
            pos = j + 1
        out.append(s[pos:])
        s = "".join(out)
    if s != orig:
        f.write_text(s, encoding="utf-8")
        print("changed", f.name)

print("wrapped", total)
