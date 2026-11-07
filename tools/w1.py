import io
p = r"D:\FormatConverter\crates\fc-core\src\lib.rs"
s = io.open(p, encoding="utf-8").read()
if "pub mod metadata;" not in s:
    s = s.replace("pub mod tags;", "pub mod tags;\npub mod metadata;")
if "pub use metadata::" not in s:
    s = s.replace("pub use tags::{extract_cover, has_cover, read_tags, Tags};",
                  "pub use metadata::{metadata_args, MetaWrite};\npub use tags::{extract_cover, has_cover, read_tags, Tags};")
io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("metadata module wired")