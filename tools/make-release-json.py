import json
import hashlib
from pathlib import Path

zip_path = Path(r'D:\FormatConverter\dist\ToneBox-beta1.0-win64.zip')
h = hashlib.sha256(zip_path.read_bytes()).hexdigest().upper()

body = f"""## ToneBox Beta 1.0（Windows x64）

测试版，建议所有操作备份；转码/播放/设备相关操作请自己负责。

### 下载
下载下面的 `ToneBox-beta1.0-win64.zip`，解压后双击 `format-converter.exe`。ffmpeg 已随包放在 `ffmpeg\\`，不要单独移动。

### 这一版有什么
- 格式判断 + 解释、音乐库、试听、批量转码
- 视频容器输出：MKV / MP4 / WEBM / AVI，第一条画面 + 全部音轨
- 音频设备检测：输出/输入、耳机/蓝牙/HDMI/USB、Steam/Realtek/NVIDIA/FxSound 标记
- WASAPI 独占直通、按源采样率、ReplayGain
- 可拖动/拉伸/收起的播放栏，极光默认主题

### 已知限制
- ASIO 未支持；连续播放（gapless）未做；DSD 只解 PCM
- 蓝牙只能看协议，不能配对接管，也读不到 SBC/AAC/aptX/LDAC
- 视频只保留第一条画面 + 全部音轨

### 校验
SHA256（zip）：`{h}`

MIT 许可；随包 FFmpeg 为 LGPL v3，详见包内 `THIRD_PARTY_NOTICES.md`。
"""

payload = {
    "name": "ToneBox Beta 1.0",
    "body": body,
    "prerelease": True,
    "draft": False,
}
out = Path(r'D:\FormatConverter\out\release_body.json')
out.write_text(json.dumps(payload, ensure_ascii=False), encoding='utf-8')
print(out)
print(h)
