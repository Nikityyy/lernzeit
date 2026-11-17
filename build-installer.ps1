param([string]$Iscc)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    if (-not $Iscc) {
        $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($command) { $Iscc = $command.Source }
        else {
            $Iscc = @(
                "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
                "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
                "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe",
                "$env:ProgramFiles\Inno Setup 7\ISCC.exe"
            ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        }
    }
    if (-not $Iscc) { throw 'Install Inno Setup 6.7.4 or newer, or pass -Iscc with the path to ISCC.exe.' }
    $manifest = Get-Content Cargo.toml -Raw
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"').Groups[1].Value
    if (-not $version) { throw 'Could not read the version from Cargo.toml.' }
    & cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    & $Iscc "/DAppVersion=$version" installer\lernzeit.iss
    if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
    $output = Join-Path $PSScriptRoot "target\installer\Lernzeit-Setup-$version.exe"
    $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($output))" | Set-Content -Encoding ascii "$output.sha256"
    Write-Output $output
} finally {
    Pop-Location
}
