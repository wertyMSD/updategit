param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Repo = "wertyMSD/updategit"

# ── 1. Leer versión de Cargo.toml ──────────────────────────────────────
$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $Version = $matches[1]
} else {
    Write-Error "No se pudo leer la versión de Cargo.toml"
    exit 1
}

$Tag = "v$Version"
Write-Host "╔══════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║           PUBLISH - UpdateGit $Version             ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# ── 2. Compilar ───────────────────────────────────────────────────────
if (-not $SkipBuild) {
    Write-Host "[1/4] Compilando..." -ForegroundColor Yellow
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build falló"
        exit 1
    }
    Write-Host "  OK" -ForegroundColor Green
} else {
    Write-Host "[1/4] Build omitido (--SkipBuild)" -ForegroundColor DarkGray
}

# ── 3. Verificar que el exe existe ────────────────────────────────────
$ExePath = "target\release\updategit.exe"
if (-not (Test-Path $ExePath)) {
    Write-Error "No se encontró $ExePath"
    exit 1
}

# ── 4. Eliminar TODAS las releases existentes ─────────────────────────
Write-Host "[2/4] Eliminando releases anteriores..." -ForegroundColor Yellow
$releases = gh release list --repo $Repo --json tagName --limit 100 2>$null | ConvertFrom-Json
if ($releases) {
    foreach ($rel in $releases) {
        Write-Host "  Eliminando $($rel.tagName)..." -ForegroundColor DarkGray
        gh release delete $rel.tagName --repo $Repo --yes --cleanup-tag 2>$null
    }
    Write-Host "  $($releases.Count) release(s) eliminada(s)" -ForegroundColor Green
} else {
    Write-Host "  No hay releases previas" -ForegroundColor DarkGray
}

# ── 5. Crear nueva release ────────────────────────────────────────────
Write-Host "[3/4] Creando release $Tag..." -ForegroundColor Yellow
gh release create $Tag `
    $ExePath `
    --repo $Repo `
    --title "UpdateGit $Version" `
    --notes "Auto-updater from GitHub Releases for Windows apps.`n`n**Changes:** see commit history.`n`n**Download:** updategit.exe below."

if ($LASTEXITCODE -ne 0) {
    Write-Error "Falló la creación de la release"
    exit 1
}
Write-Host "  OK" -ForegroundColor Green

# ── 6. Verificar ──────────────────────────────────────────────────────
Write-Host "[4/4] Verificando..." -ForegroundColor Yellow
$latest = gh release view --repo $Repo --json tagName,assets --jq '{tag: .tagName, assets: [.assets[].name]}'
Write-Host "  $latest" -ForegroundColor DarkGray

Write-Host ""
Write-Host "Release publicada: https://github.com/$Repo/releases/tag/$Tag" -ForegroundColor Cyan
