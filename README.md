# recode

Native Omarchy Linux development environment built with Tauri 2 + Rust.

## Dev with live reload

```bash
npm run tauri:dev
```

This starts the Vite dev server with HMR and launches the Tauri app directly. Frontend changes are reflected immediately. Rust changes require a rebuild.

If no window appears:
```bash
GDK_BACKEND=x11 npm run tauri:dev
```

Logs:
- Frontend: `/tmp/recode-vite.log`
- Tauri: `/tmp/recode-tauri.log`

> Troubleshooting: if you need the old static `dist/` path instead, use `npm run tauri:dev` with the environment variable `RECODE_DEV_MODE=static` or run `bash scripts/tauri-dev.sh` directly.

## Install on Omarchy

```bash
bash scripts/install-omarchy.sh
```

This builds the release binary and installs it to:
- `~/.local/bin/recode` for normal users
- `/usr/local/bin/recode` when run with sudo

## Run installed app

```bash
recode
```

## Test

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

## Build

```bash
npm run tauri:build
# or
cargo build --release --bin recode --manifest-path src-tauri/Cargo.toml
```

## Updates and signing

```bash
bash scripts/setup-updater-keys.sh
```

This generates the updater signing keys and stores them under `~/.config/recode/updater/`.

## Release artifact signing

```bash
bash scripts/sign-release-artifact.sh target/release/recode
```

This creates a detached SHA-256 signature using the updater keys.
