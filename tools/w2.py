import io
p = r"D:\FormatConverter\docs\留言\from-B.md"
lines = io.open(p, encoding="utf-8").read().split("\n")
head = lines[:6]              # 标题 + 说明
ours = lines[7:41]            # HEAD 段（metadata）
theirs = lines[42:71]         # holdout 段
rest = lines[72:]             # 之后的内容
out = head + ours + theirs + rest
io.open(p, "w", encoding="utf-8", newline="\n").write("\n".join(out))
print("resolved, lines:", len(out))