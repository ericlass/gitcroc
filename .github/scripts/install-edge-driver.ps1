# M5-16: choose an installed WebView2 runtime, pin the app to that directory,
# and download the exact matching Microsoft Edge WebDriver from Microsoft.
# https://learn.microsoft.com/microsoft-edge/webview2/how-to/webdriver
$ErrorActionPreference = 'Stop'
$roots = @(
    (Join-Path ${env:ProgramFiles(x86)} 'Microsoft\EdgeWebView\Application'),
    (Join-Path $env:ProgramFiles 'Microsoft\EdgeWebView\Application'),
    (Join-Path $env:LOCALAPPDATA 'Microsoft\EdgeWebView\Application')
)
$runtime = $roots | ForEach-Object {
    Get-ChildItem $_ -Directory -ErrorAction SilentlyContinue
} | Where-Object {
    $_.Name -match '^\d+\.\d+\.\d+\.\d+$' -and
        (Test-Path (Join-Path $_.FullName 'msedgewebview2.exe'))
} | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $runtime) { throw 'No installed WebView2 runtime was found' }
$version = $runtime.Name
$destination = Join-Path $env:RUNNER_TEMP 'gitcroc-edge-driver'
New-Item -ItemType Directory -Force $destination | Out-Null
$archive = Join-Path $destination 'driver.zip'
Invoke-WebRequest "https://msedgedriver.microsoft.com/$version/edgedriver_win64.zip" -OutFile $archive
Expand-Archive $archive -DestinationPath $destination -Force
Remove-Item $archive
$driverVersion = & (Join-Path $destination 'msedgedriver.exe') --version
if ($LASTEXITCODE -ne 0 -or $driverVersion -notmatch "\b$([regex]::Escape($version))\b") {
    throw "The downloaded driver does not match WebView2 $version"
}
$destination >> $env:GITHUB_PATH
"WEBVIEW2_BROWSER_EXECUTABLE_FOLDER=$($runtime.FullName)" >> $env:GITHUB_ENV
$os = Get-CimInstance Win32_OperatingSystem
$proof = "$($os.Caption), version $($os.Version), build $($os.BuildNumber); WebView2 $version; $driverVersion"
Write-Output $proof
$proof >> $env:GITHUB_STEP_SUMMARY
