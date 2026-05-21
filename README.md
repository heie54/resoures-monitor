# Resource Monitor

Resource Monitor is a Windows-first desktop resource monitor built with Tauri 2 and Vue 3.
It runs as a small always-on-top floating window and stays available from the native Windows
system tray.

The app is not Electron. It uses Tauri native window, command, event, and tray APIs.

## Features

- Floating frameless monitor window with a transparent Tauri shell and rounded in-app panel.
- CPU, GPU, and memory usage display.
- Top process list sorted by CPU, GPU, or memory.
- Click CPU, GPU, or Memory to switch the process sort metric.
- Docking and collapse behavior for the monitor window.
- Native Windows system tray menu.
- Settings window for opacity control.
- Close buttons hide windows to the tray; only the tray `Quit` action exits the app.

## Tech Stack

| Area | Technology |
| --- | --- |
| Frontend | Vue 3 Composition API, Vite 8, `@tauri-apps/api` 2.x |
| Desktop shell | Rust 2021, Tauri 2.x |
| System data | `sysinfo` for CPU, memory, and processes |
| GPU data | Windows PDH GPU Engine counters |
| Async runtime | Tokio |

## Requirements

| Software | Version | Purpose |
| --- | --- | --- |
| Node.js | 18+ | Frontend tooling |
| npm | 9+ | JavaScript package manager |
| Rust | Stable MSVC toolchain | Tauri backend build |
| Microsoft Edge WebView2 Runtime | Current | Required by Tauri WebView on Windows |

## Install Dependencies

```powershell
npm install
```

## Development

Run the Tauri development app from the repository root:

```powershell
npm run tauri dev
```

The Tauri config starts the Vite dev server automatically through `beforeDevCommand`.

You can also run only the frontend during UI work:

```powershell
npm run dev
```

## Build

Build the frontend:

```powershell
npm run build
```

Check the Rust/Tauri backend:

```powershell
cd src-tauri
cargo check
cd ..
```

Build the release executable:

```powershell
npm run tauri build
```

The optimized executable is created at:

```text
src-tauri/target/release/resource-monitor.exe
```

Current `src-tauri/tauri.conf.json` has `bundle.active` set to `false`, so `npm run tauri build`
creates a portable executable instead of an installer bundle.

## Release Package

Release-ready files are collected in the root `release/` folder:

```text
release/
|-- resource-monitor-v0.1.0-windows-x64/
|   |-- resource-monitor.exe
|   |-- WebView2Loader.dll
|   |-- README.md
|   |-- LICENSE
|   `-- RELEASE_NOTES.md
|-- resource-monitor-v0.1.0-windows-x64.zip
|-- RELEASE_NOTES_v0.1.0.md
`-- SHA256SUMS.txt
```

For GitHub Releases, upload:

- `release/resource-monitor-v0.1.0-windows-x64.zip`
- `release/SHA256SUMS.txt`
- `release/RELEASE_NOTES_v0.1.0.md`

Users can extract the zip and run `resource-monitor.exe`.

## Runtime Views

The app uses one Vue bundle and switches views through the URL query:

- default route: monitor window
- `index.html?mode=settings`: settings window

The settings window is created or reused by the Rust `show_settings` command.

## System Tray

The tray menu is implemented with Tauri 2 native `TrayIconBuilder` and `Menu` APIs.
It is intentionally not rendered as a Vue/WebView popup.

Current tray actions:

- Show main window
- Settings
- Auto start checkbox
- Notifications checkbox
- Quit

Only `Quit` exits the process. Closing or hiding app windows keeps the tray process alive.

## Project Structure

```text
resource-monitor/
|-- src/
|   |-- App.vue
|   |-- main.js
|   |-- style.css
|   |-- appearance.js
|   |-- percentFormat.js
|   |-- processMenuState.js
|   `-- components/
|-- src-tauri/
|   |-- src/main.rs
|   |-- capabilities/default.json
|   |-- icons/
|   |-- Cargo.toml
|   `-- tauri.conf.json
|-- public/
|-- release/
|-- package.json
|-- vite.config.js
`-- README.md
```

## Troubleshooting

If `npm run tauri build` fails because `resource-monitor.exe` is locked, stop the running app
process and rebuild:

```powershell
Get-Process resource-monitor -ErrorAction SilentlyContinue
Stop-Process -Name resource-monitor -Force
npm run tauri build
```

If the portable release cannot start on another Windows machine, install or repair the Microsoft
Edge WebView2 Runtime.

## References

- [Tauri 2 Documentation](https://v2.tauri.app/)
- [Vue 3 Documentation](https://vuejs.org/guide/)
- [Vite Documentation](https://vite.dev/guide/)
