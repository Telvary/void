# Install

`void` ships as a single static binary. Install the latest release from [GitHub Releases](https://github.com/MaximeGaudin/void/releases/latest) — commands below install to directories that are already on `PATH` by default on each platform.

## Homebrew (macOS, recommended)

```bash
brew install MaximeGaudin/tap/void
```

Homebrew handles the download, `PATH`, upgrades (`brew upgrade void`), and removes the macOS quarantine attribute for you — no Gatekeeper prompt.

## macOS (direct download)

```bash
# Apple Silicon
curl -fsSL https://github.com/MaximeGaudin/void/releases/latest/download/void-darwin-arm64.tar.gz | sudo tar xz -C /usr/local/bin

# Intel
curl -fsSL https://github.com/MaximeGaudin/void/releases/latest/download/void-darwin-amd64.tar.gz | sudo tar xz -C /usr/local/bin
```

> **Gatekeeper note.** The `curl` command above does **not** quarantine the binary, so it runs immediately. If you instead download the `.tar.gz` from the Releases page in a browser, macOS may block it ("cannot be opened because the developer cannot be verified"). Clear the quarantine attribute once:
>
> ```bash
> sudo xattr -d com.apple.quarantine /usr/local/bin/void
> ```
>
> (The right-click → Open trick is for `.app` bundles and doesn't apply to a CLI binary.)

## Linux

```bash
# x86_64
curl -fsSL https://github.com/MaximeGaudin/void/releases/latest/download/void-linux-amd64.tar.gz | sudo tar xz -C /usr/local/bin

# ARM64
curl -fsSL https://github.com/MaximeGaudin/void/releases/latest/download/void-linux-arm64.tar.gz | sudo tar xz -C /usr/local/bin
```

## Windows (PowerShell)

```powershell
$dir = "$env:LOCALAPPDATA\Programs\void"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
curl.exe -fsSL -o "$env:TEMP\void.zip" https://github.com/MaximeGaudin/void/releases/latest/download/void-windows-amd64.zip
Expand-Archive -Path "$env:TEMP\void.zip" -DestinationPath $dir -Force

# Add to PATH (current user, persists across sessions)
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$dir*") {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')
}
$env:Path += ";$dir"
```

## Verify

```bash
void --version
void doctor
```

## Build from source

Requires a recent Rust toolchain (edition 2021).

```bash
git clone https://github.com/MaximeGaudin/void.git
cd void

# Build and install to ~/bin
./scripts/build-install.sh

# Or specify a custom directory
./scripts/build-install.sh /usr/local/bin
```

```powershell
# Windows (PowerShell)
.\scripts\build-install.ps1

# Or specify a custom directory
.\scripts\build-install.ps1 -InstallDir "$HOME\bin"
```

Or plain cargo:

```bash
cargo build --release
# binary at target/release/void
```

## Next steps

Run the interactive setup wizard, then start the sync daemon:

```bash
void setup
void sync --daemon
```

See [Connector setup](connectors.md) for per-service credentials and [Configuration](configuration.md) for the full `config.toml` reference.
