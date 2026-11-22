import re
import pathlib
import json

ROOT = pathlib.Path(r"C:\Users\23501\Desktop\FormatConverter-source-20261114-1922\crates\fc-gui\src")
CJK = re.compile(r'[\u4e00-\u9fff]')
LIT = re.compile(r'"((?:[^"\\]|\\.)*)"')

by_file = {}
for f in sorted(ROOT.glob("*.rs")):
    s = f.read_text(encoding="utf-8")
    if f.name in ("i18n.rs",):
        continue
    items = []
    for m in LIT.finditer(s):
        lit = m.group(1)
        if not CJK.search(lit):
            continue
        # already translated at source?
        start = max(0, m.start() - 20)
        ctx = s[start:m.start()]
        if "tr!(" in ctx or "i18n::tr(" in ctx:
            continue
        # 跳过注释行
        line_start = s.rfind("\n", 0, m.start()) + 1
        if s[line_start:m.start()].lstrip().startswith("//"):
            continue
        items.append(lit)
    if items:
        by_file[f.name] = items

out = pathlib.Path(r"C:\Users\23501\Desktop\FormatConverter-source-20261114-1922\out\untranslated.json")
out.write_text(json.dumps(by_file, ensure_ascii=False, indent=1), encoding="utf-8")
for k, v in by_file.items():
    print(k, len(v))
print("TOTAL", sum(len(v) for v in by_file.values()))
