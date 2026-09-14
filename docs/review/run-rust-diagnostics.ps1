param([switch]$Live)
$ErrorActionPreference = 'Stop'
$reviewRepo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$reviewManifest = Join-Path $reviewRepo 'src-tauri/Cargo.toml'
$reviewTest = Join-Path $reviewRepo 'src-tauri/tests/release_review_diagnostics.rs'
if (Test-Path -LiteralPath $reviewTest) { throw "Refusing to overwrite $reviewTest" }
New-Item -ItemType Directory -Path (Split-Path -Parent $reviewTest) -Force | Out-Null
try {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'release_repros.rs') -Destination $reviewTest
    if ($Live) {
        & cargo test --manifest-path $reviewManifest --test release_review_diagnostics live_extraction_contract_probe -- --ignored --nocapture
    } else {
        & cargo test --manifest-path $reviewManifest --test release_review_diagnostics -- --nocapture
    }
    $reviewExit = $LASTEXITCODE
} finally {
    Remove-Item -LiteralPath $reviewTest
}
exit $reviewExit
