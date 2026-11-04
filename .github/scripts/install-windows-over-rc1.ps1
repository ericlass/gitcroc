# PLAT-4 (M6-28): the GPUI installer upgrades an installed 1.0.0-rc.1 in
# place. rc.1's own installer is installed first, then the new one runs with
# /P /UPDATE, as rc.1's updater starts it. Afterwards there is one install
# directory, %LOCALAPPDATA%\gitcroc, holding the new gitcroc.exe, and one
# uninstall key with the new version and MainBinaryName; the app starts as
# `installer` with the new version.
#
# rc.1's installer is the only download from GitHub after the build: one
# file of the v1.0.0-rc.1 release, checked against the release's SHA256SUMS.
param([string]$Artifacts, [string]$Version)
$ErrorActionPreference = 'Stop'
$logs = $env:INSTALL_LOGS
New-Item -ItemType Directory -Force $logs | Out-Null

$rc1 = 'gitcroc_1.0.0-rc.1_x64-setup.exe'
$install = Join-Path $env:LOCALAPPDATA 'gitcroc'
$uninstall = 'Software\Microsoft\Windows\CurrentVersion\Uninstall'
$key = "HKCU:\$uninstall\gitcroc"
$app = Join-Path $install 'gitcroc.exe'

function Write-Listing([string]$Name) {
    $out = Join-Path $logs "$Name-files.txt"
    Get-ChildItem $install -Recurse -ErrorAction SilentlyContinue |
        Format-Table FullName, Length, LastWriteTime | Out-String -Width 400 | Set-Content $out
    Get-ItemProperty $key -ErrorAction SilentlyContinue | Format-List | Out-String | Add-Content $out
}

# 1. rc.1, from its release.
$download = Join-Path $env:RUNNER_TEMP 'rc1'
gh release download v1.0.0-rc.1 --repo $env:GITHUB_REPOSITORY --dir $download --pattern $rc1 --pattern SHA256SUMS
if ($LASTEXITCODE -ne 0) { throw 'gh release download failed' }
$sums = @(Get-Content (Join-Path $download 'SHA256SUMS') | Where-Object { $_ -match "^([0-9a-f]{64})\s+\*?$([regex]::Escape($rc1))$" })
if ($sums.Count -ne 1) { throw "SHA256SUMS has no line for $rc1" }
$expected = ($sums[0] -split '\s+')[0]
$actual = (Get-FileHash (Join-Path $download $rc1) -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "$rc1 has SHA-256 $actual, SHA256SUMS says $expected" }
Write-Output "PASS: $rc1 matches SHA256SUMS ($actual)"

$result = Start-Process (Join-Path $download $rc1) -ArgumentList @('/S') -Wait -PassThru
if ($result.ExitCode -ne 0) { throw "rc.1's installer exited $($result.ExitCode)" }
Write-Listing 'rc1'
if ((Get-ItemProperty $key).DisplayVersion -ne '1.0.0-rc.1') { throw 'rc.1 is not installed' }
if (-not (Test-Path $app)) { throw "rc.1's gitcroc.exe is not in $install" }
# rc.1's installer wrote MainBinaryName too; without it, the check below
# shows that the new installer writes it.
Remove-ItemProperty $key -Name MainBinaryName

# 2. The new installer, as rc.1's updater starts it.
$installer = @(Get-ChildItem "$Artifacts/gitcroc_${Version}_x64-setup.exe")
if ($installer.Count -ne 1) { throw "Expected gitcroc_${Version}_x64-setup.exe" }
$result = Start-Process $installer[0].FullName -ArgumentList @('/P', '/UPDATE') -Wait -PassThru
if ($result.ExitCode -ne 0) { throw "The new installer exited $($result.ExitCode)" }
Write-Listing 'upgrade'

# 3. One install directory and one uninstall key.
$keys = @()
foreach ($root in @("HKCU:\$uninstall", "HKLM:\$uninstall", "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall")) {
    $keys += @(Get-ChildItem $root -ErrorAction SilentlyContinue |
        Where-Object { (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).DisplayName -eq 'gitcroc' })
}
if ($keys.Count -ne 1 -or $keys[0].PSChildName -ne 'gitcroc' -or $keys[0].Name -notlike 'HKEY_CURRENT_USER\*') {
    throw "Expected one uninstall key, HKCU\$uninstall\gitcroc: $($keys.Name -join ', ')"
}
$values = Get-ItemProperty $key
if ($values.DisplayVersion -ne $Version) { throw "DisplayVersion is $($values.DisplayVersion), not $Version" }
if ($values.MainBinaryName -ne 'gitcroc.exe') { throw "MainBinaryName is '$($values.MainBinaryName)'" }
if ($values.InstallLocation.Trim('"') -ne $install) { throw "InstallLocation is $($values.InstallLocation)" }
$previous = (Get-ItemProperty 'HKCU:\Software\gitcroc\gitcroc').'(default)'
if ($previous -ne $install) { throw "The kept install directory is $previous" }
$others = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\gitcroc'),
    (Join-Path $env:ProgramFiles 'gitcroc'),
    (Join-Path ${env:ProgramFiles(x86)} 'gitcroc')
) | Where-Object { Test-Path $_ }
if ($others.Count -ne 0) { throw "Another install directory: $($others -join ', ')" }
$exes = @(Get-ChildItem $install -Filter *.exe -Recurse | ForEach-Object Name | Sort-Object)
if (($exes -join ',') -ne 'gitcroc.exe,uninstall.exe') { throw "The install directory holds $($exes -join ', ')" }
# The version may be rc.1's own while M6 builds it, so the binary is
# compared too: the portable zip packs the installer's binary.
$zip = @(Get-ChildItem "$Artifacts/gitcroc-$Version-windows-x64-portable.zip")
if ($zip.Count -ne 1) { throw "Expected gitcroc-$Version-windows-x64-portable.zip" }
$portable = Join-Path $env:RUNNER_TEMP 'gitcroc-portable'
Expand-Archive $zip[0].FullName $portable
$new = @(Get-ChildItem $portable -Filter gitcroc.exe -Recurse)
if ($new.Count -ne 1) { throw 'The portable zip must contain exactly one gitcroc.exe' }
if ((Get-FileHash $app).Hash -ne (Get-FileHash $new[0].FullName).Hash) {
    throw "The installed gitcroc.exe is not the new build's"
}
Write-Output "PASS: one install in $install, key version $Version, MainBinaryName gitcroc.exe, the new gitcroc.exe"

# 4. It starts as the installer's, with the new version.
python "$PSScriptRoot/install_common.py" $Version windows x86_64 installer $app
if ($LASTEXITCODE -ne 0) { throw 'The upgraded app did not start' }
