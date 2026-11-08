# Installs qsr, the Quasar command-line client, from the latest GitHub release.
#
#   irm https://raw.githubusercontent.com/AymericChaverot/quasar-cli/main/install.ps1 | iex
#
# $env:QSR_VERSION = "0.1.0"  installs that release instead of the latest.
# $env:QSR_INSTALL_DIR        is where the binary goes
#                             (default: %LOCALAPPDATA%\Programs\qsr).
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$repo = "AymericChaverot/quasar-cli"
$dir = if ($env:QSR_INSTALL_DIR) { $env:QSR_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\qsr" }

if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
    throw "There is a prebuilt qsr for Windows x86_64 only; see the README to build from source."
}
$target = "x86_64-pc-windows-msvc"

if ($env:QSR_VERSION) {
    $version = $env:QSR_VERSION.TrimStart("v")
} else {
    $release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
    $version = $release.tag_name.TrimStart("v")
}

$name = "qsr-$version-$target"
$base = "https://github.com/$repo/releases/download/v$version"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("qsr-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "Downloading qsr $version for $target..."
    Invoke-WebRequest "$base/$name.zip" -OutFile (Join-Path $tmp "$name.zip")
    Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $tmp "SHA256SUMS")

    $line = Get-Content (Join-Path $tmp "SHA256SUMS") | Where-Object { $_ -match " $([regex]::Escape("$name.zip"))$" }
    if (-not $line) { throw "$name.zip is not in the release's checksums." }
    $expected = ($line -split " ")[0]
    $actual = (Get-FileHash (Join-Path $tmp "$name.zip") -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) { throw "Checksum mismatch for $name.zip; nothing was installed." }

    Expand-Archive (Join-Path $tmp "$name.zip") -DestinationPath $tmp
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $exe = Join-Path $dir "qsr.exe"
    # A running qsr.exe cannot be overwritten, but it can be renamed aside.
    if (Test-Path $exe) {
        Move-Item -Force $exe (Join-Path $dir "qsr.old.exe")
    }
    Copy-Item (Join-Path $tmp "$name\qsr.exe") $exe
    Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $dir "qsr.old.exe")
} finally {
    Remove-Item -Recurse -Force $tmp
}

Write-Host "Installed qsr $version to $exe"
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $dir) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
    Write-Host "Added $dir to your PATH; open a new terminal to run it as qsr."
}
