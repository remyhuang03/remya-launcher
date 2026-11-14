<p align="center">
  <img src="assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">A Material Design 3 Expressive launcher and clipboard manager for Hyprland</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.47.8-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.47.8">
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

Remya Launcher (`top.remya.launcher`) is a keyboard-first application launcher, quick switcher, and Wayland clipboard history manager designed for Hyprland using Flutter, Rust, and Material Design 3 Expressive.

- **Keyboard-first workflow**: Designed so your hands stay on the main key zone, with natural prefix scopes, `Alt+1–9` quick pick, and contextual footer key hints.
- **Built-in Wayland clipboard history**: A background Rust watcher persists copied text, code, links, colors, files, and image thumbnails with sensitive-data masking and Remya Core policy storage.
- **Material Design 3 Expressive**: Smooth shape morphing, Inter and JetBrains Mono typography, and a `gtk-layer-shell` overlay anchored at 22% screen height.

> [!CAUTION]
> **Early Development & Preview Only — Not Ready for Daily Use**
>
> Remya Launcher is in early active development. Currently, **the calculator**, **live clipboard history** (the Rust clipboard daemon and live Clipboard view) and **interactive UI design previews** (`--preview`) are implemented. Most other launcher scopes, search backends, and Hyprland window/action integrations described below reflect the planned [Remya Launcher Figma specification](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) and are not yet implemented.

## 🚀 Installation

### AUR (`dev` channel)

On Arch Linux, install the development package once available in the AUR:

```bash
paru -S remya-launcher-git || yay -S remya-launcher-git
```

### Release Archive

Prebuilt release archives are not yet published. Once releases are available on [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases), verify the checksum, extract the archive, and run the local installer:

```bash
sha256sum -c SHA256SUMS --ignore-missing
tar --zstd -xf remya_launcher-0.1.0+1-linux-x86_64.tar.zst
cd remya_launcher-0.1.0+1-linux-x86_64
python tool/install.py
```

Install **Remya Core** before Remya Launcher. Clipboard capture and retention policy belongs to Launcher and is stored by Core through the `remya-launcher` app-preferences registration. `remya_launcher --settings [clipboard]` opens the settings route; its UI is pending.

## ✨ Features

### Currently Available

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
  - Optional FP8 and general calendar arithmetic remain deferred.
- **Interactive Design Previews (`--preview`)**:
  - Inspect all planned UI states from a built bundle using `--preview=<name>`:
    - Launcher states: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Clipboard type previews: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

### Planned Scopes & Capabilities (Design Spec)

The following capabilities are planned according to the [Remya Launcher design specification](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher):

- **Natural Prefix Scopes**:
  - **All (`Ctrl+1`)**: Mixed results across apps, files, web history, and commands (up to 3 items per group, ordered dynamically by best score) and direct URL detection.
  - **Apps (`Ctrl+2`)**: XDG `.desktop` fuzzy matching across name, `GenericName`, `Keywords`, and executable with frecency weighting and secondary desktop actions (e.g., new private window).
  - **Files (`Ctrl+3`, `/` or `~` prefix)**: Native path completion (`Tab` segment-by-segment), fuzzy filename search (`fd` + recent files), and a right-hand preview pane (directory contents, text excerpts, image thumbnails) expanding the panel to 880 px.
  - **Web (`Ctrl+4`, `?` prefix)**: Read-only local browser history (Firefox `places.sqlite` / Chromium `History`) ranked by frecency, with a fallback web search action (`Ctrl+Enter`).
  - **Clipboard (`Ctrl+6`, `;` prefix)**: Full 960 px split view with type filters (`All · Text · Images · Links · Files · Colors` via `Ctrl+F`), `Pinned → Today → Yesterday → Earlier` grouping, duplicate merging with copy counts, type-aware previews (syntax-highlighted code with line numbers, color swatches in HEX/RGB/HSL/OKLCH, checkerboard image previews), and direct paste-back to the previous focused window (`Enter`, or `Ctrl+Enter` for type-specific secondary actions).
  - **Windows (`Ctrl+7`, `#` prefix)**: `hyprctl clients` matching by title and class with workspace badges; `Enter` focuses the window and switches workspace, `Ctrl+Enter` moves it to the current workspace.
  - **Emoji (`:` prefix)** & **Run (`$` / `>` prefix)**: Emoji/symbol search by name or alias and direct shell/terminal command execution.
- **Keyboard-First Navigation**:
  - `↑`/`↓` or `Ctrl+N`/`Ctrl+P` to navigate across groups; `Ctrl+↑`/`Ctrl+↓` to jump between group heads.
  - Hold `Alt` to reveal row numbers and press `Alt+1–9` to execute the primary action directly (`Alt+1–6` for Pinned items in the empty state), or `Ctrl+Alt+1–9` for the secondary action.
  - `→` or `Ctrl+K` to open the filterable Actions menu; `Ctrl+C` to copy the selected value; `Ctrl+D` to pin/unpin; `Shift+Del` to forget from Recent or delete from history.
  - `Ctrl+W` / `Ctrl+Backspace` to delete a word or path segment; `Ctrl+U` to clear the query; `Esc` to step back through menu → query → scope → close; `?` on empty input for the full shortcut cheat sheet.
- **Hyprland Overlay & Visual System**:
  - `gtk-layer-shell` overlay with exclusive keyboard interactivity, 720 px default width, downward-only height transitions (250 ms), and dismiss on click-outside or focus loss.
  - Material Design 3 Expressive shape morphing (`20 px → 12 px` on selected rows and pinned tiles, `16 px → 10 px` on scope chips) and dynamic theme tokens synced with the Remya desktop seed color.

## ⌨️ Development

### Tech Stack & Architecture

- **Frontend**: Flutter & Dart (pinned via `.fvmrc` to `3.47.8`) with `material_ui` (Material Design 3 Expressive) and `flutter_svg`.
  - `lib/app.dart` & `lib/main.dart`: Application composition, CLI flag handling (`--clipboard`, `--preview=...`), and license registration.
  - `lib/features/launcher/`: Launcher panel, search field, scope chips, result/pinned rows, keycap hints, calculator card, file preview, and action menu.
  - `lib/features/clipboard/`: Live clipboard page, Unix socket client, type previews, and clipboard actions.
  - `lib/features/preview/`: Interactive design examples assembled from reusable controls.
- **Backend**: Rust (`edition = "2024"`, `resolver = "3"`) workspace at the repository root:
  - `lib/src/rust/clipboard/`: Wayland clipboard watcher daemon (`remya-launcher-clipboard`), SQLite persistence, and IPC server.
  - `lib/src/rust/core/`: Core Rust crate (reserved for shared launcher backend logic).

### Prerequisites & Setup

Ensure GTK 3, `gtk-layer-shell`, Python 3, `inter-font` (`sudo pacman -S inter-font` on Arch), FVM, and the Rust toolchain defined in `rust-toolchain.toml` are installed.

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

To inspect a design preview from a debug build:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Validation & Local Installation

Run formatting, static analysis, tests, and sequential debug/release builds:

```bash
fvm dart format --output=none --set-exit-if-changed .
fvm flutter analyze
fvm flutter test
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
fvm flutter build linux --debug
fvm flutter build linux --release
```

Install the release bundle, desktop entry, and `top.remya.launcher.clipboard.service` systemd user unit under `$XDG_DATA_HOME`:

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Use `~/.local/bin/remya_launcher --clipboard` for a Hyprland `Super+V` keybinding.

## 📜 License

Remya Launcher is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE). Material Symbols and third-party notices are included in `assets/licenses/`.
