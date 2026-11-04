# D6, D13, PLAT-3, PLAT-4 (M6-28): the NSIS installer installs for the
# current user into %LOCALAPPDATA%\gitcroc, the installed app starts as
# `installer`, and the uninstaller removes the program and its uninstall
# key. The portable zip's gitcroc.exe starts as `portable`.
param([string]$Artifacts, [string]$Version)
$ErrorActionPreference = 'Stop'
$logs = $env:INSTALL_LOGS
New-Item -ItemType Directory -Force $logs | Out-Null

$install = Join-Path $env:LOCALAPPDATA 'gitcroc'
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\gitcroc'
$app = Join-Path $install 'gitcroc.exe'

$installer = @(Get-ChildItem "$Artifacts/gitcroc_${Version}_x64-setup.exe")
if ($installer.Count -ne 1) { throw "Expected gitcroc_${Version}_x64-setup.exe" }
$result = Start-Process $installer[0].FullName -ArgumentList @('/S') -Wait -PassThru
if ($result.ExitCode -ne 0) { throw "Installer exited $($result.ExitCode)" }
Get-ChildItem $install -Recurse | Format-Table FullName, Length | Out-String -Width 400 |
    Set-Content (Join-Path $logs 'installer-files.txt')
Get-ItemProperty $key | Format-List | Out-String | Add-Content (Join-Path $logs 'installer-files.txt')
if (-not (Test-Path $app)) { throw "The installer did not put gitcroc.exe in $install" }
if ((Get-ItemProperty $key).DisplayVersion -ne $Version) { throw "The uninstall key is not version $Version" }

try {
    python "$PSScriptRoot/install_common.py" $Version windows x86_64 installer $app
    if ($LASTEXITCODE -ne 0) { throw 'Installed app startup failed' }
} finally {
    # _?= keeps NSIS in this process so -Wait waits for the actual removal;
    # it also keeps uninstall.exe, which is removed below.
    $result = Start-Process (Join-Path $install 'uninstall.exe') -ArgumentList @('/S', "_?=$install") -Wait -PassThru
    if ($result.ExitCode -ne 0) { throw "Uninstaller exited $($result.ExitCode)" }
}
if (Test-Path $app) { throw 'The uninstaller left gitcroc.exe installed' }
if (Test-Path $key) { throw 'The uninstaller left the uninstall key' }
Remove-Item (Join-Path $install 'uninstall.exe')
Write-Output "PASS: the uninstaller removed gitcroc.exe and $key"

$portable = Join-Path $env:RUNNER_TEMP 'gitcroc-portable'
$zip = @(Get-ChildItem "$Artifacts/gitcroc-$Version-windows-x64-portable.zip")
if ($zip.Count -ne 1) { throw "Expected gitcroc-$Version-windows-x64-portable.zip" }
Expand-Archive $zip[0].FullName $portable
Get-ChildItem $portable -Recurse | Format-Table FullName, Length | Out-String -Width 400 |
    Set-Content (Join-Path $logs 'portable-files.txt')
$apps = @(Get-ChildItem $portable -Filter gitcroc.exe -Recurse)
if ($apps.Count -ne 1) { throw 'The portable zip must contain exactly one gitcroc.exe' }
python "$PSScriptRoot/install_common.py" $Version windows x86_64 portable $apps[0].FullName
if ($LASTEXITCODE -ne 0) { throw 'Portable app startup failed' }
