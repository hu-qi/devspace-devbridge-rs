param(
    [string]$Version = $env:DEVBRIDGE_VERSION,
    [string]$InstallDir = $env:DEVBRIDGE_INSTALL_DIR,
    [string]$Repository = $env:DEVBRIDGE_REPOSITORY
)

$ErrorActionPreference = "Stop"
if (-not $Version) { $Version = "latest" }
if (-not $InstallDir) { $InstallDir = Join-Path $HOME ".huawei\bin" }
if (-not $Repository) { $Repository = "hu-qi/devspace-devbridge-rs" }

$arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    "X64" { "x86_64" }
    "Arm64" { "aarch64" }
    default { throw "Unsupported architecture: $_" }
}

$target = "$arch-pc-windows-msvc"
$archive = "devbridge-$target.zip"
$base = "https://github.com/$Repository/releases"
if ($Version -eq "latest") {
    $url = "$base/latest/download/$archive"
} else {
    $tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
    $url = "$base/download/$tag/$archive"
}
$checksumUrl = "$url.sha256"

$temp = Join-Path ([IO.Path]::GetTempPath()) ("devbridge-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    $archivePath = Join-Path $temp $archive
    $checksumPath = "$archivePath.sha256"
    Invoke-WebRequest -Uri $url -OutFile $archivePath
    Invoke-WebRequest -Uri $checksumUrl -OutFile $checksumPath
    $expected = ((Get-Content $checksumPath -Raw).Trim() -split "\s+")[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { throw "SHA-256 verification failed" }

    Expand-Archive -Path $archivePath -DestinationPath $temp -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item (Join-Path $temp "devbridge.exe") (Join-Path $InstallDir "devbridge.exe") -Force
    Write-Host "Installed devbridge to $InstallDir\devbridge.exe"
    Write-Host "Add $InstallDir to PATH if it is not already present."
}
finally {
    Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
}
