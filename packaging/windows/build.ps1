param([switch]$PackageOnly)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '2' }
if (-not $env:RUSTFLAGS) { $env:RUSTFLAGS = '-C target-feature=+crt-static' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
$output = if ($env:KESTREL_DIST_DIR) { $env:KESTREL_DIST_DIR } else { Join-Path $root 'dist' }
$architecture = 'x86_64-pc-windows-msvc'
New-Item -ItemType Directory -Force -Path $target, $output | Out-Null
$target = (Resolve-Path $target).Path
$output = (Resolve-Path $output).Path
$staging = Join-Path $target ('kestrel-windows-' + [Guid]::NewGuid().ToString('N'))
$application = Join-Path $staging 'Kestrel'

Push-Location $root
try {
    $metadata = & cargo metadata --no-deps --locked --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the package version' }
    $version = ($metadata.packages | Where-Object name -eq 'kestrel-app').version
    if (-not $PackageOnly) {
        & cargo build --release --locked --target $architecture -p kestrel-app -p kestrel-cli -p kestrel-mcp
        if ($LASTEXITCODE -ne 0) { throw 'Windows compilation failed' }
    }
    New-Item -ItemType Directory -Path (Join-Path $application 'licenses') -Force | Out-Null
    foreach ($name in @('kestrel', 'kestrelcli', 'kestrel-mcp')) {
        Copy-Item (Join-Path $target "$architecture/release/$name.exe") $application
    }
    foreach ($name in @('kestrelcli', 'kestrel-mcp')) {
        & (Join-Path $application "$name.exe") --version
        if ($LASTEXITCODE -ne 0) { throw "$name could not start" }
    }
    Copy-Item (Join-Path $root 'LICENSE-MIT'), (Join-Path $root 'LICENSE-APACHE') (Join-Path $application 'licenses')
    Copy-Item (Join-Path $root 'crates/kestrel-app/assets/fonts/*-LICENSE.txt') (Join-Path $application 'licenses')
    Copy-Item (Join-Path $PSScriptRoot 'README.txt') $application
    $archive = Join-Path $output "Kestrel-$version-x86_64-windows.zip"
    Compress-Archive -LiteralPath $application -DestinationPath $archive -Force
    $compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
    if (-not (Test-Path $compiler)) { throw 'Install Inno Setup 6 to build the Windows installer' }
    & $compiler "/DAppVersion=$version" "/DSourceDir=$application" "/O$output" (Join-Path $PSScriptRoot 'installer.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Windows installer compilation failed' }
    $installer = Join-Path $output "Kestrel-$version-x86_64-setup.exe"
    $installed = Join-Path $staging 'installed'
    $setup = Start-Process -FilePath $installer -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$installed`"") -Wait -PassThru
    if ($setup.ExitCode -ne 0) { throw "Installer failed with exit code $($setup.ExitCode)" }
    & (Join-Path $installed 'kestrelcli.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'The installed CLI could not start' }
    $uninstall = Start-Process -FilePath (Join-Path $installed 'unins000.exe') -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) { throw "Uninstaller failed with exit code $($uninstall.ExitCode)" }
    foreach ($artifact in @($archive, $installer)) {
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifact).Hash.ToLowerInvariant()
        $filename = [IO.Path]::GetFileName($artifact)
        [IO.File]::WriteAllText("$artifact.sha256", "$hash  $filename`n", [Text.UTF8Encoding]::new($false))
    }
} finally {
    if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
    Pop-Location
}
