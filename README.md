# UpdateGit

**Auto-updater from GitHub Releases for Windows apps.**

Binary compiled in Rust, zero dependencies, standalone executable.

---

## Usage

```
updategit <app_name> [repo] [--self-update]
```

| Parameter | Description | Default |
|-----------|-------------|---------|
| `app_name` | Application name to download and run | `s50info` |
| `repo` | GitHub repository name (owner is hardcoded) | Same as `app_name` |
| `--self-update` | Update the updater itself, then exit | — |

### Examples

```powershell
# Download and run latest release of wertyMSD/centralita_teamleader
.\updategit centralita_teamleader

# Download from a different repo
.\updategit centralita_teamleader centralita-releases

# Update the updater itself
.\updategit --self-update
```

---

## What it does

1. Kills the target process if it's running
2. Downloads the latest ZIP from GitHub Releases
3. Extracts files to the current directory
4. Hides `_internal` folder
5. Launches the application

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `GITHUB_TOKEN` | PAT for private repos (optional) |
| `GITHUB_REPO` | Override default repo name |
| `ZIP_PASSWORD` | Password for encrypted ZIPs (default: `123`) |
| `SELF_UPDATE` | Set to `true` to enable self-update |

---

## Release Asset Naming

The updater looks for a ZIP asset matching the app name. Search order:

1. **Exact match**: `appname.zip`, `appname_1.0.0.zip`, `appname-1.0.0.zip`
2. **Partial match**: any `.zip` containing the app name
3. **Fallback**: first component only (e.g. `centralita` from `centralita_teamleader`)

---

## Build

Requires [Rust](https://rustup.rs/) and Windows.

```powershell
cargo build --release
# Output: target/release/updategit.exe
```

---

## Publish

```powershell
.\publish.ps1
```

Compiles, deletes all existing releases, creates a new one with the binary.

---

**© 2025 ALCA TIC, S.L.**
