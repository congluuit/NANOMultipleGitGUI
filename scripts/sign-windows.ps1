<#
.SYNOPSIS
  Tauri `bundle.windows.signCommand` hook: signs one PE file (app exe, sidecar or installer).

.DESCRIPTION
  Tauri calls this once per file it wants signed, passing the absolute path.
  Without SignPath credentials in the environment it only logs and exits 0, so local and
  unsigned CI builds keep working.

  With SIGNPATH_API_TOKEN, SIGNPATH_ORGANIZATION_ID, SIGNPATH_PROJECT_SLUG and
  SIGNPATH_SIGNING_POLICY_SLUG set, the file is submitted to SignPath via the official
  `SignPath` PowerShell module and replaced in place by the signed result.

  NOTE: the SignPath Foundation (free OSS certificates) requires origin verification, which
  is only available through the GitHub Actions integration. The production release flow
  therefore signs in CI with `signpath/github-action-submit-signing-request` (see
  .github/workflows/release.yml, job `windows-signed`); this hook is kept for the
  paid/self-managed SignPath tiers and to exercise Tauri's signing call sites.
#>
param(
  [Parameter(Mandatory = $true, Position = 0)]
  [string]$File
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $File)) {
  Write-Error "[sign] file not found: $File"
  exit 1
}

$log = $env:NANOMGG_SIGN_LOG
if ($log) { Add-Content -LiteralPath $log -Value $File }

# Tauri also asks to sign the NSIS plugin DLLs and WiX extensions it bundles. Those are
# third-party binaries and must not carry this project's signature.
$normalized = $File -replace '/', '\'
if ($normalized -match '\\nsis\\[^\\]+\\Plugins\\' -or $normalized -match '\\wix\\[^\\]+\\wix\\') {
  Write-Host "[sign] third-party binary - skipped: $File"
  exit 0
}

if (-not $env:SIGNPATH_API_TOKEN) {
  Write-Host "[sign] SIGNPATH_API_TOKEN not set - leaving unsigned: $File"
  exit 0
}

foreach ($name in 'SIGNPATH_ORGANIZATION_ID', 'SIGNPATH_PROJECT_SLUG', 'SIGNPATH_SIGNING_POLICY_SLUG') {
  if (-not (Get-Item -Path "Env:$name" -ErrorAction SilentlyContinue)) {
    Write-Error "[sign] $name must be set when SIGNPATH_API_TOKEN is present"
    exit 1
  }
}

if (-not (Get-Module -ListAvailable -Name SignPath)) {
  Install-Module -Name SignPath -Scope CurrentUser -Force -AllowClobber
}
Import-Module SignPath

$artifactConfig = $env:SIGNPATH_ARTIFACT_CONFIGURATION_SLUG
if (-not $artifactConfig) { $artifactConfig = 'pe-file' }

$signed = "$File.signed"
Submit-SigningRequest `
  -InputArtifactPath $File `
  -ApiToken $env:SIGNPATH_API_TOKEN `
  -OrganizationId $env:SIGNPATH_ORGANIZATION_ID `
  -ProjectSlug $env:SIGNPATH_PROJECT_SLUG `
  -SigningPolicySlug $env:SIGNPATH_SIGNING_POLICY_SLUG `
  -ArtifactConfigurationSlug $artifactConfig `
  -OutputArtifactPath $signed `
  -WaitForCompletion

Move-Item -LiteralPath $signed -Destination $File -Force
Write-Host "[sign] signed: $File"
