[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repositoryRoot
try {
    if ((git branch --show-current) -ne 'main') {
        throw 'Release preparation must run from the main branch.'
    }

    $allowedVersionFiles = @(
        'package.json',
        'package-lock.json',
        'src-tauri/Cargo.toml',
        'src-tauri/Cargo.lock',
        'src-tauri/tauri.conf.json'
    )
    $unexpectedChanges = @(git status --porcelain | ForEach-Object {
        $changedPath = $_.Substring(3).Replace('\', '/')
        if ($changedPath -notin $allowedVersionFiles) { $changedPath }
    })
    if ($unexpectedChanges.Count -gt 0) {
        throw "The working tree contains unrelated changes: $($unexpectedChanges -join ', '). Commit, stash, or discard them first."
    }

    if (git tag --list "v$Version") {
        throw "Tag v$Version already exists locally."
    }

    & node scripts/release-version.mjs set $Version
    if ($LASTEXITCODE -ne 0) { throw 'Version update failed.' }

    & node scripts/release-version.mjs check $Version
    if ($LASTEXITCODE -ne 0) { throw 'Version verification failed.' }

    & git diff --check
    if ($LASTEXITCODE -ne 0) { throw 'Whitespace validation failed.' }

    Write-Host ''
    Write-Host "Prepared tz-chatter $Version." -ForegroundColor Green
    Write-Host 'Review these version-only changes:'
    & git diff --stat
    Write-Host ''
    Write-Host 'Next:' -ForegroundColor Cyan
    Write-Host "  1. Commit and push these changes as Release v$Version."
    Write-Host '  2. Wait for CI and CodeQL to pass on main.'
    Write-Host "  3. In GitHub: Actions -> Release -> Run workflow -> version $Version."
    Write-Host '     Use dry-run to test, prerelease for a preview, or stable after the release gate.'
    Write-Host 'Running this command never creates a tag or publishes a release.'
}
finally {
    Pop-Location
}
