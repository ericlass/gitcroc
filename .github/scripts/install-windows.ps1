# D6, D13, PLAT-3/4: current-user NSIS install/start/uninstall and portable start.
param([string]$Artifacts, [string]$Version)
$ErrorActionPreference = 'Stop'
$install = Join-Path $env:LOCALAPPDATA 'gitcroc-install-test'
$installer = @(Get-ChildItem "$Artifacts/*-setup.exe")
if ($installer.Count -ne 1) { throw 'Expected one NSIS installer' }
$result = Start-Process $installer[0].FullName -ArgumentList @('/S', "/D=$install") -Wait -PassThru
if ($result.ExitCode -ne 0) { throw "Installer exited $($result.ExitCode)" }
$app = Join-Path $install 'gitcroc.exe'
try {
    python "$PSScriptRoot/install_common.py" $Version windows x86_64 installer $app
    if ($LASTEXITCODE -ne 0) { throw 'Installed app startup failed' }
} finally {
    # _?= keeps NSIS in this process so -Wait waits for the actual removal.
    $result = Start-Process "$install/uninstall.exe" -ArgumentList @('/S', "_?=$install") -Wait -PassThru
    if ($result.ExitCode -ne 0) { throw "Uninstaller exited $($result.ExitCode)" }
    if (Test-Path $app) { throw 'Uninstaller left gitcroc.exe installed' }
}
$portable = Join-Path $env:RUNNER_TEMP 'gitcroc-portable'
$zip = @(Get-ChildItem "$Artifacts/*-portable.zip")
if ($zip.Count -ne 1) { throw 'Expected one portable zip' }
Expand-Archive $zip[0].FullName $portable
$apps = @(Get-ChildItem $portable -Filter gitcroc.exe -Recurse)
if ($apps.Count -ne 1) { throw 'Portable zip must contain exactly one gitcroc.exe' }
python "$PSScriptRoot/install_common.py" $Version windows x86_64 portable $apps[0].FullName
if ($LASTEXITCODE -ne 0) { throw 'Portable app startup failed' }
