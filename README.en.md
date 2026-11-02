# LeafShield · 一叶

[简体中文](README.md) | English · **Author: Leigg**

A **Rust + Win32** break reminder for Windows, with screen temperature and brightness controls, a floating countdown, and full-screen breaks that require a manually typed phrase to end early.

Current native preview: **0.7.0-dev.4**. The source is merged into **`main`**, with `dev.win` retained as the Windows development branch. Targets **Windows 11 x64**; Windows 10 compatibility has not been fully verified. **macOS and Linux are not currently supported.**

The application uses native controls and GDI drawing. Running it requires no WebView2, browser, or Node.js, and starts no HTTP server.

The current native Windows build uses **under 3 MiB of private memory** while resident, and its portable EXE is **under 2 MiB**. These figures are measured from `0.7.0-dev.4`; see Resource usage for the accounting details.

## Screenshots

### Screen temperature and brightness

![Screen temperature and brightness](.github/images/Snipaste_2026-11-02_19-37-17.png)

### Work and break timer

![Work and break timer](.github/images/Snipaste_2026-11-02_19-37-31.png)

### Unlock phrases and backgrounds

![Unlock phrases and emergency instructions](.github/images/Snipaste_2026-11-02_19-37-37.png)

### Options

![Floating timer, notifications, startup, and language options](.github/images/Snipaste_2026-11-02_19-37-55.png)

## Features

| Feature | Current behavior |
| --- | --- |
| Work and break timer | Defaults to 45 minutes of work and a 2-minute break; presets `25/2`, `45/2`, `60/3`, and `90/5`, plus custom durations, pause, resume, and immediate breaks |
| Screen adjustment | 1500–6500 K and 20–100% brightness; defaults to 5000 K / 80%; moving left makes the screen warmer; applies to all displays |
| Multi-monitor breaks | An always-on-top break window on each display, ending automatically; the cursor and unlock area start hidden, with input revealed on the active screen |
| Mindful unlock | Five fixed phrases; type the complete phrase to end a break early. Prompts cannot be copied, pasting is blocked, and mismatches receive a red underline |
| Emergency unlock | Optional, with instructions: 50 manual keystrokes within 20 seconds. A pause over 2 seconds or a timeout resets the count; holding a key does not repeat-count; successful unlocks are logged |
| Floating countdown | Dragging, hide at any screen edge, hover to reveal, click-through mode, and position reset; shares its context menu with the system tray |
| Foreground whitelist | Up to five process filenames; choose EXE files or desktop `.lnk` shortcuts to defer automatic breaks while a listed app is in the foreground |
| Break backgrounds | Three bundled offline photographs: forest, mountain, and night sky; custom JPEG / PNG / WebP images supported |
| Local data | Settings, daily statistics, and the latest 200 log entries in local JSON; supports startup for the current Windows user |
| Chinese and English | Switch in Options → Language; settings, break windows, floating timer, tray, and five phrases switch together |

Custom images may be up to 5 MB, 16 megapixels, and 8192 pixels on either edge. Imported images are normalized to JPEG within 1920×1200 and 500 KB.

## Download and run

Download the [Windows native preview 0.7.0-dev.4](https://github.com/chaseSpace/timetorest-rs/releases/tag/win-v0.7.0-dev.4):

| File | Purpose |
| --- | --- |
| `leafshield-native-0.7.0-dev.4.exe` | Windows x64 portable preview |
| `SHA256SUMS.txt` | SHA-256 checksums for the EXE and license |
| `LICENSE` | MIT license |

Run the EXE directly; installation is not required. This preview is unsigned, so Windows may display a security prompt. Verify it with the checksum file from the same release. **The 0.6.x packages on GitHub Releases are historical Tauri builds.**

1. Exit the older version from its tray menu before running the new EXE.
2. Choose durations in Timer. Use Preview to view the break screen.
3. Closing settings keeps the tray and timer running; reopen settings or exit from the tray.
4. Configure startup in Options. Keep an enabled EXE in a stable location; after moving it, toggle startup again to update the path. Starting this development build does not automatically rewrite existing startup entries.

Data continues to use `%APPDATA%\com.leafshield.desktop`. The first native launch backs up existing settings. Open the data folder from Options to inspect local files.

## Build from source

Windows builds require Rust stable, MSVC C++ Build Tools, and the Windows SDK. Node.js 22+ is also required for the project wrapper commands.

From a checkout of `main`, run:

```powershell
npm run build:native
```

Locally generated files:

| File | Purpose |
| --- | --- |
| `artifacts/native-dev/leafshield-native-0.7.0-dev.4.exe` | Windows x64 portable preview |
| `artifacts/native-dev/SHA256SUMS.txt` | SHA-256 checksums for the EXE and license |
| `artifacts/native-dev/LICENSE` | MIT license |

You can also build directly with the Rust toolchain. The executable goes to Cargo's `release` output directory:

```powershell
cargo build --release -p leafshield-native
```

## Resource usage

One resident snapshot of `0.7.0-dev.4` on the development machine measured:

| Metric | Measured value |
| --- | ---: |
| Private working set | **2.19 MiB** |
| Private commit | **2.84 MiB** |
| Native Windows portable EXE | **1.65 MiB** (1,733,632-byte GitHub build) |
| Total working set | 20.93 MiB |

This supports the compact description: **under 3 MiB of private memory on Windows, with an executable under 2 MiB**. Total working set includes shared Windows DLL pages and does not represent memory owned exclusively by the application. Usage varies with windows, photographs, displays, and system state; loading break backgrounds increases memory use.

## Technology

| Area | Technology |
| --- | --- |
| Application and core | Rust 2024 edition, `leafshield-core` |
| Windows UI | Win32, Common Controls v6, GDI, DWM, `windows` / `windows-sys` |
| System integration | Windows color matrices, keyboard hooks, system tray, COM file dialogs and shortcut resolution, current-user startup entries |
| Data and images | `serde` / `serde_json`, `chrono`, `image`, `sha2` |
| Validation and builds | Rust tests, rustfmt, Clippy, PowerShell / C# native UI automation, GitHub Actions |

## Validation

```powershell
cargo test -p leafshield-core -p leafshield-native
cargo fmt --all -- --check
cargo clippy -p leafshield-core -p leafshield-native --all-targets -- -D warnings
npm run test:native-ui
```

The current build passed **39 Rust tests and 18 automated UI checks**. One test that changes desktop colors is ignored by default. UI checks cover slider rails, click placement, dragging and stale pixels, bilingual emergency instructions, tray menus, and settings windows.

UI automation uses a temporary profile with global keyboard hooks and screen adjustment disabled, and does not rewrite normal startup entries. These checks do not replace real multi-monitor break, IME, color restoration, or login-startup acceptance. See the [native Windows development notes](docs/native-windows.md).

## License and acknowledgments

Project code is available under the [MIT License](LICENSE). Bundled photographs retain their own licenses; see [image sources and photographer credits](public/wallpapers/SOURCES.md).

Thanks to **CareUEyes**. Its UI design and selected features inspired this project.

This is a break reminder; it cannot block Ctrl+Alt+Del, secure desktops, or forced process termination.
