# Builds both offline signer executables for Windows x64:
#   target\x86_64-pc-windows-msvc\release\hedera-offline-signer.exe
#   target\x86_64-pc-windows-msvc\release\examples\sign_request.exe
# The app build does not rebuild them; run this after any change to
# offline-signer, signer-core or hedera-tx. Close any running copy first
# or the link fails (LNK1104).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$vsBase = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build"
$vcvars = @('vcvarsarm64_amd64.bat', 'vcvars64.bat') |
    ForEach-Object { Join-Path $vsBase $_ } |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1
if (-not $vcvars) {
    throw 'Visual Studio Build Tools with an x64 MSVC toolchain were not found.'
}
$build = 'call "{0}" >nul && cd /d "{1}" && set "RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc"' -f $vcvars, $root
$build += ' && cargo build --release -p offline-signer --target x86_64-pc-windows-msvc'
$build += ' && cargo build --release -p hedera-tx --example sign_request --target x86_64-pc-windows-msvc'
& $env:ComSpec /c $build
if ($LASTEXITCODE -ne 0) {
    throw 'Signer build failed.'
}
Write-Host 'Built hedera-offline-signer.exe and examples\sign_request.exe in target\x86_64-pc-windows-msvc\release\'
