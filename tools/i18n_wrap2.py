import re
import pathlib

ROOT = pathlib.Path(r"C:\Users\23501\Desktop\FormatConverter-source-20261114-1922\crates\fc-gui\src")
CJK = re.compile(r'[\u4e00-\u9fff]')
LIT = re.compile(r'"((?:[^"\\]|\\.)*)"')
SKIP_MACROS = ("println!", "eprintln!", "panic!", "write!", "assert", "format_args!", "format!")


def lit_at(s, i):
    """i 指向开引号之后；返回 (literal, end_index_after_closing_quote)"""
    j = i
    while j < len(s):
        if s[j] == "\\":
            j += 2
            continue
        if s[j] == '"':
            return s[i:j], j + 1
        j += 1
    return None, j


def wrap_file(path, stats):
    s = path.read_text(encoding="utf-8")

    # 1) 暂不处理 format!（format! 要求字面量，运行时查表做不到）
    s = s
    out = []
    pos = 0
    for m in []:
        if m.start() < pos:
            continue
        i = m.end()
        lit, end = lit_at(s, i)
        if lit is None or not CJK.search(lit):
            continue
        out.append(s[pos:m.start()])
        out.append(s[m.start():m.end()].replace("format!(", "crate::tr!(", 1))
        out.append(lit + '"')
        pos = end
        stats["format"] += 1
    out.append(s[pos:])
    s = "".join(out)

    # 2) 剩余中文 literal，按上下文包裹
    out = []
    pos = 0
    for m in LIT.finditer(s):
        if m.start() < pos:
            continue
        lit = m.group(1)
        if not CJK.search(lit):
            continue
        before = s[max(0, m.start() - 24):m.start()]
        if "tr!(" in before or "i18n::tr(" in before:
            continue
        line_start = s.rfind("\n", 0, m.start()) + 1
        line = s[line_start:m.end()]
        if line.lstrip().startswith("//"):
            continue
        if any(mac in line for mac in SKIP_MACROS):
            continue
        out.append(s[pos:m.start()])
        out.append("crate::tr!(" + m.group(0) + ")")
        pos = m.end()
        stats["plain"] += 1
    out.append(s[pos:])
    s = "".join(out)

    if s != path.read_text(encoding="utf-8"):
        path.write_text(s, encoding="utf-8")
        return True
    return False


stats = {"format": 0, "plain": 0}
for f in sorted(ROOT.glob("*.rs")):
    if f.name == "i18n.rs":
        continue
    if wrap_file(f, stats):
        print("changed", f.name)
print(stats)
