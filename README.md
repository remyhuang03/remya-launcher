<p align="center">
  <img src="assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remy'a Launcher</h1>

<h3 align="center">A Material Design 3 Expressive launcher and clipboard manager for Hyprland</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.50.0-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.50.0">
  <img src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-4C1?style=flat-square" alt="License: PolyForm Noncommercial 1.0.0">
</p>

<p align="center">
  <strong>English</strong> ·
  <a href="docs/README.zh-CN.md">简体中文</a> ·
  <a href="docs/README.zh-TW.md">繁體中文</a> ·
  <a href="docs/README.ru.md">Русский</a> ·
  <a href="docs/README.de.md">Deutsch</a> ·
  <a href="docs/README.es.md">Español</a> ·
  <a href="docs/README.ja.md">日本語</a>
</p>

> [!CAUTION]
> **Early development (dev).** Bugs may occur and compatibility is not guaranteed. You may encounter performance and stability issues. User interaction is still being refined.

Remya Launcher is a keyboard-first application launcher, quick switcher, and Wayland clipboard history manager designed for Hyprland using Flutter, Rust, and Material Design 3 Expressive.

- **Keyboard-first workflow**: Designed so your hands stay on the main key zone, with natural prefix scopes.

## 🚀 Installation

### Source Build

Install **Remya Core** first. On Arch Linux, follow [Prerequisites & Setup](#prerequisites--setup) and [Validation & Local Installation](#validation--local-installation) to build and install Launcher.

### Release Archive

With Remya Core `0.2.0` installed, download `remya_launcher-0.1.0-dev.20261118.0-linux-x86_64.tar.zst` and `SHA256SUMS` from [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases). Run these commands from the directory containing both files.

```bash
sha256sum -c SHA256SUMS
tar --zstd -xf remya_launcher-0.1.0-dev.20261118.0-linux-x86_64.tar.zst
cd remya_launcher-0.1.0-dev.20261118.0-linux-x86_64
python tool/install.py
```

Open Launcher settings with `remya_launcher --settings`. Open a page directly with `remya_launcher --settings web`, `clipboard`, `apps`, `hidden-apps`, `files-search`, `files-history`, `files-preview`, `files-actions`, or `material-theme`.

## ✨ Features

- **All and Apps (`Ctrl+2`)**: Search installed applications and open them with `Enter`. In Apps, `Ctrl+Right` expands an app’s open windows and desktop actions. If there are no matching apps, install candidates come from local package metadata. All also shows supported inline calculations, up to three matching local files, up to three local Web results, and a direct action for URL input.

- **Files (`Ctrl+3` / `/` / `~`)**:

  - Search by name, complete a path with `Tab`, or press `Ctrl+G` to search file contents. Refine results with filters such as `ext:md` or `in:~/Documents`.
  - Preview the selected file and press `Ctrl+K` for file actions. Pinned files also appear in All.
  - Enable recent files or connect external history providers in **Launcher Settings → Files → History**. Each history source requires its corresponding option.

- **Web (`Ctrl+4` / `?`)**:

  - Enable Firefox or Chromium family profiles in **Launcher Settings → Web**. With no profiles enabled, you can still open a typed URL or search with an engine. Launcher reads recent history and bookmarks from enabled profiles and keeps a private local index. It reads favicons from local browser data and shows a monogram when none is available.
  - Choose DuckDuckGo or Google as the default in **Launcher Settings → Web**. Engine keywords apply to the current query.
  - Empty input shows frequent sites (`Alt+1–6`) and recent pages. Search shows bookmarks and history ranked by relevance and use, followed by engine suggestions. `Tab` accepts URL completion; `Enter` opens the typed URL or selected row; `Ctrl+Enter` searches instead.
  - Type an engine keyword followed by a space, such as `gh ` for GitHub, `aw ` for ArchWiki, or `yt ` for YouTube. With an engine selected, `Tab` cycles engines and `Backspace` removes the token when the query is empty.
  - `Ctrl+K` opens actions for a page: open in a supported browser or private window, copy the URL or Markdown link, search the site, pin it, or hide it from Launcher. Launcher never changes browser history or bookmarks. Turning off a profile removes its index from Launcher.

- **Windows (`Ctrl+7` / `#`)**: Filter Hyprland windows by title or class. Press `Enter` to focus a window or `Ctrl+Enter` to move it to the current workspace.

- **Live Clipboard History (`remya_launcher --clipboard` / `Ctrl+6` / `;`)**:

  - Background Rust watcher (`remya-launcher-clipboard`) running as the `top.remya.launcher.clipboard.service` systemd user unit with the graphical session.
  - Persists up to 500 unpinned items for 30 days by default in `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (default `~/.local/state/remya/launcher/clipboard.sqlite3`); pinned items are exempt from automatic pruning.
  - Captures selections up to 64 MiB and decoded images up to 24 megapixels (PNG, JPEG, WebP, GIF, BMP); images larger than 20 MB within those limits retain a thumbnail.
  - Automatically masks entries marked sensitive by the source application in memory for up to 45 seconds without writing them to disk.
  - Exposes same-user history operations and `clear` at `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock`. Policy is read from `top.remya.Core1` and refreshed on `Changed`; while unavailable or invalid, capture and automatic history cleanup pause.
  - Installs `remya/apps/remya-launcher.json` and a desktop `settings` action. The clipboard unit starts after and wants `top.remya.core.service`.
  - *Known Wayland limitation*: Wayland data-control does not identify which application provided a clipboard selection, so source app labels and per-application exclusion blacklists in the design cannot be implemented reliably.

- **Calculator (`Ctrl+5` / `=`)**:

  - Evaluates inline basic expressions in `All` and full queries in `Calc` (`Ctrl+5` or `=`) locally via `fend-core` math, typed units, bases with width and signedness, FP32/FP16/BF16, time zones, and ECB reference snapshots fetched only on explicit refresh, with catalog completions and history.
  - In `Calc`, try `molar mass H₂O` or `2 mmol to mg using NaCl`. For concentration conversions, provide a chemical formula or a molar mass, as in `372.45 pmol/L to pg/mL using 272.38 g/mol`.
  - Optional FP8 and general calendar arithmetic remain deferred.

- **Emoji and Symbols (`:`)**:

  - Search Emoji 18.0 offline across seven languages. Select **Symbols** for a separate catalog of arrows, math, currency, and other text characters.
  - Press Enter to copy or Ctrl+Right to choose an emoji variant. In **Settings → Emoji**, enable Ctrl+Enter to paste into the previous Hyprland window, or enable and clear the local recent emoji list.

- **Interactive Design Previews (`--preview`)**:

  - Explore example UI states from the [Figma design](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) with sample data using `--preview=<name>`.
    - Launcher states: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Clipboard type previews: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

## ⌨️ Development

### Tech Stack & Architecture

- **Frontend**: Flutter & Dart (pinned via `.fvmrc` to `3.50.0`) with `material_ui` (Material Design 3 Expressive) and `flutter_svg`.
  - `lib/app.dart` & `lib/main.dart`: Application composition, CLI flag handling (`--clipboard`, `--settings`, `--preview=...`), and license registration.
  - `lib/features/launcher/`: Launcher panel, search field, scope chips, result/pinned rows, keycap hints, calculator card, file preview, and action menu.
  - `lib/features/clipboard/`: Live clipboard page, Unix socket client, type previews, and clipboard actions.
  - `lib/features/files/`: Filename, path, and content search, file actions, previews, and settings.
  - `lib/features/web/`: Web results, keyboard actions, profile settings, and the Web sidecar client.
  - `lib/features/preview/`: Interactive design examples assembled from reusable controls.
- **Backend**: Rust (`edition = "2024"`, `resolver = "3"`) workspace at the repository root:
  - `lib/src/rust/clipboard/`: Wayland clipboard watcher daemon (`remya-launcher-clipboard`), SQLite persistence, and IPC server.
  - `lib/src/rust/apps/`: Resident application index, package lookups, and Hyprland window actions.
  - `lib/src/rust/calculator/`: Local evaluation, completions, and calculator history.
  - `lib/src/rust/files/`: On-demand Files sidecar for search, previews, actions, and opted-in history.
  - `lib/src/rust/web/`: On-demand Web sidecar with browser readers, a private SQLite index, and browser dispatch.
  - `lib/src/rust/core/`: Core Rust crate (reserved for shared launcher backend logic).

### Prerequisites & Setup

Place the Remya Core checkout alongside this repository as `../remya-core` so Flutter can resolve `remya_client` and `remya_ui`. Install GTK 3, `gtk-layer-shell`, CMake, Ninja, Clang, Python 3, `inter-font`, FVM, and the Rust toolchain in `rust-toolchain.toml`. Files content search uses `ripgrep`, and Copy file uses `wl-copy` from `wl-clipboard` on Wayland.

```bash
fvm install
fvm flutter pub get --enforce-lockfile
fvm flutter run -d linux
```

To inspect a design preview from a debug build:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Validation & Local Installation

Run formatting, static analysis, tests, and sequential debug/release builds:

```bash
fvm dart format --output=none --set-exit-if-changed lib test
fvm flutter analyze
fvm flutter test
python -m unittest tool.test_install
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
fvm flutter build linux --debug
fvm flutter build linux --release
```

Install the release bundle, desktop entry, and the `top.remya.launcher.clipboard.service` and `top.remya.launcher.apps.service` systemd user units under `$XDG_DATA_HOME`. Calculator, Files, and Web run on demand.

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

### Hyprland Shortcuts

Use this Hyprland Lua snippet to replace existing Launcher bindings. Press and release either Super key on its own to toggle Launcher. Super+V opens clipboard history. Adjust the executable path for your installation.

```lua
local launcher = '"$HOME/.local/bin/remya_launcher"'
local toggle = hl.dsp.exec_cmd(launcher .. " --toggle")

for _, key in ipairs({ "SUPER_L", "SUPER_R" }) do
    hl.unbind("SUPER + " .. key)
    hl.bind(key, toggle, { release = true, description = "Remya: Toggle launcher" })
end

hl.bind("SUPER + V", hl.dsp.exec_cmd(launcher .. " --clipboard"),
    { description = "Remya: Open clipboard history" })
```

## 📜 License

Remya Launcher is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE). Material Symbols and third-party notices are included in `assets/licenses/`.
