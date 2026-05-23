param(
    [string]$EccodesRoot = "",
    [string]$BaseUrl = "https://sites.ecmwf.int/repository/eccodes/test-data/data/bufr"
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($EccodesRoot)) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
    $EccodesRoot = Join-Path $RepoRoot "external\eccodes-2.47.0"
}
$DataDir = Join-Path $EccodesRoot "data\bufr"
$Names = @()
$Names += Get-Content (Join-Path $DataDir "bufr_data_files.txt")
$Names += @("vos308014_v3_26.bufr", "bad.bufr")
$Names += Get-Content (Join-Path $DataDir "bufr_ref_files.txt")

$Client = [System.Net.Http.HttpClient]::new()
$Downloaded = 0
$Skipped = 0
$Failed = @()

foreach ($Name in $Names) {
    if ([string]::IsNullOrWhiteSpace($Name)) {
        continue
    }

    $Out = Join-Path $DataDir $Name
    if (Test-Path $Out) {
        $Skipped += 1
        continue
    }

    try {
        $Bytes = $Client.GetByteArrayAsync("$BaseUrl/$Name").GetAwaiter().GetResult()
        [System.IO.File]::WriteAllBytes($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out), $Bytes)
        $Downloaded += 1
    }
    catch {
        $Failed += $Name
    }
}

Write-Host "downloaded=$Downloaded skipped=$Skipped failed=$($Failed.Count)"
if ($Failed.Count -gt 0) {
    $Failed
    exit 1
}
