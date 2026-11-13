# Fetch the bundled Speex encoder (BSD) used for .spx output.
# Binaries come from MSYS2 packages:
#   mingw-w64-ucrt-x86_64-speex 1.2.1-1
#   mingw-w64-ucrt-x86_64-speexdsp 1.2.1-1
#   mingw-w64-ucrt-x86_64-libogg 1.3.5-1
# See THIRD_PARTY_NOTICES.md for attribution and licenses.

$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $src 'vendor\speex'
$tmp = Join-Path $env:TEMP ('tonebox_speex_' + [guid]::NewGuid().ToString('N'))
$pkgs = Join-Path $tmp 'pkgs'
$extract = Join-Path $tmp 'extract'
New-Item -ItemType Directory -Force -Path $vendor, $pkgs, $extract | Out-Null

$urls = @(
    'https://mirror.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-speex-1.2.1-1-any.pkg.tar.zst',
    'https://mirror.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-speexdsp-1.2.1-1-any.pkg.tar.zst',
    'https://mirror.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-libogg-1.3.5-1-any.pkg.tar.zst'
)

foreach ($u in $urls) {
    $name = Split-Path $u -Leaf
    $out = Join-Path $pkgs $name
    Write-Host "downloading $name"
    curl.exe -L --fail --max-time 180 -o $out $u
    if ($LASTEXITCODE -ne 0) { throw "download failed: $u" }
    tar.exe -xf $out -C $extract
    if ($LASTEXITCODE -ne 0) { throw "extract failed: $out" }
}

$bin = Join-Path $extract 'ucrt64\bin'
Copy-Item (Join-Path $bin 'speexenc.exe') $vendor -Force
Copy-Item (Join-Path $bin 'libspeex-1.dll') $vendor -Force
Copy-Item (Join-Path $bin 'libspeexdsp-1.dll') $vendor -Force
Copy-Item (Join-Path $bin 'libogg-0.dll') $vendor -Force

$lic = Join-Path $extract 'ucrt64\share\licenses'
Copy-Item (Join-Path $lic 'speex\LICENSE') (Join-Path $vendor 'LICENSE-speex.txt') -Force
Copy-Item (Join-Path $lic 'speexdsp\LICENSE') (Join-Path $vendor 'LICENSE-speexdsp.txt') -Force
Copy-Item (Join-Path $lic 'libogg\COPYING') (Join-Path $vendor 'LICENSE-libogg.txt') -Force

Remove-Item $tmp -Recurse -Force
Write-Host "Speex installed to $vendor"
