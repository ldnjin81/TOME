# Builds the Windows installer (NSIS, installs for the current user) and the portable zip.
# Run on Windows from anywhere, with cargo and node on PATH:
#   powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
# Output: target\release\bundle\nsis\TOME_<version>_x64-setup.exe
#         target\release\bundle\portable\TOME_<version>_x64_portable.zip
# THIRD-PARTY-NOTICES.md must be current (python3 scripts/third_party_notices.py).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not (Test-Path THIRD-PARTY-NOTICES.md)) { throw 'THIRD-PARTY-NOTICES.md is missing: run scripts/third_party_notices.py' }
$version = (Get-Content src-tauri\tauri.conf.json -Raw | ConvertFrom-Json).version

# The first run downloads the NSIS tools into Tauri's own cache.
& ui\node_modules\.bin\tauri build --bundles nsis
if ($LASTEXITCODE) { throw 'tauri build failed' }

$stage = Join-Path $env:TEMP 'tome-portable'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null
Copy-Item target\release\tome.exe, LICENSE, THIRD-PARTY-NOTICES.md, README.md, README.ko.md $stage
$out = 'target\release\bundle\portable'
New-Item -ItemType Directory -Force $out | Out-Null
$zip = Join-Path $out "TOME_${version}_x64_portable.zip"
Compress-Archive -Path "$stage\*" -DestinationPath $zip -Force
Remove-Item -Recurse -Force $stage

Get-ChildItem "target\release\bundle\nsis\TOME_${version}_x64-setup.exe", $zip | ForEach-Object {
  '{0}  {1:N1} MB  sha256 {2}' -f $_.FullName, ($_.Length / 1MB), (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
}
