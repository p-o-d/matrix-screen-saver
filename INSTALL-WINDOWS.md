# Installing on Windows

## Requirements
- Windows 10 or later
- DirectX 12 capable GPU (DX12 preferred; falls back to Vulkan)

## Install

1. Copy `matrix-screensaver.exe` to `C:\Windows\System32\matrix-screensaver.scr`
   (rename from `.exe` to `.scr`)

2. Right-click the `.scr` file → **Install**
   — or —
   Open **Screen Saver Settings** (right-click desktop → Personalize → Lock screen → Screen saver),
   select **Matrix Screensaver** from the dropdown.

3. Click **Settings** to configure speed, density, charset, color, and glow.

## Config file

Settings are saved to `%APPDATA%\matrix-screensaver\config.toml`.
Same format as the Linux config — you can copy settings between platforms.

## Build from source (cross-compile from Linux)

```bash
rustup target add x86_64-pc-windows-gnu
cargo build -p matrix-windows --target x86_64-pc-windows-gnu --release
# Rename output:
cp target/x86_64-pc-windows-gnu/release/matrix-screensaver.exe matrix-screensaver.scr
```
