import io
p = r"D:\FormatConverter\docs\留言\from-B.md"
lines = io.open(p, encoding="utf-8").read().split("\n")
# 7 是 <<<<<<< (index 6)，33 是 ======= (index 32)，56 是 >>>>>>> (index 55)
head = lines[:6]
ours = lines[7:32]
theirs = lines[33:55]
rest = lines[56:]
out = head + ours + theirs + rest
io.open(p, "w", encoding="utf-8", newline="\n").write("\n".join(out))
print("resolved")