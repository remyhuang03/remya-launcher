<p align="center">
  <img src="assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

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

Remya Launcher (`top.remya.launcher`) is a keyboard-first application launcher, quick switcher, and Wayland clipboard history manager designed for Hyprland using Flutter, Rust, and Material Design 3 Expressive.

- **Keyboard-first workflow**: Designed so your hands stay on the main key zone, with natural prefix scopes, `Alt+1–9` quick pick, and contextual footer key hints.
- **Apps search**: Search installed applications in All or press `Ctrl+2` for Apps. Open a new app window with Enter or `Ctrl+Enter`. Expand its windows and desktop actions with `Ctrl+Right`, then press Enter on a window to focus it. Install candidates come from local package metadata.
- **Built-in Wayland clipboard history**: A background Rust watcher persists copied text, code, links, colors, files, and image thumbnails with sensitive-data masking and Remya Core policy storage.
- **Local Web search**: Search browser history and bookmarks from profiles you enable, complete URLs, or start a search with a keyword like `gh `.

> [!CAUTION]
> **Early development (dev).** Performance issues remain.
>
> Apps search, Windows, Web search, Calculator, Clipboard, and Emoji are available. The Files scope and general command execution are still planned. Use `--preview` to explore the [Figma design](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) with sample data.

## 🚀 Installation

### Source Build

Install **Remya Core** first. On Arch Linux, follow [Prerequisites & Setup](#prerequisites--setup) and [Validation & Local Installation](#validation--local-installation) to build and install Launcher.

### Release Archive

For a Linux x86_64 candidate built by the repository’s Linux CI workflow, download and unzip the candidate artifact. It contains a `remya-launcher-linux-x86_64-<commit>.tar.zst` archive and `SHA256SUMS`. From the directory with those files:

```bash
sha256sum -c SHA256SUMS
mkdir -p remya-launcher-candidate
tar --zstd -xf remya-launcher-linux-x86_64-*.tar.zst -C remya-launcher-candidate
cd remya-launcher-candidate
python tool/install.py
```

Open Launcher settings with `remya_launcher --settings`. Open a page directly with `remya_launcher --settings web`, `clipboard`, `apps`, `hidden-apps`, or `material-theme`.

## ✨ Features

### Currently Available

- **All and Apps (`Ctrl+2`)**: Search installed applications and open them with `Enter`. In Apps, `Ctrl+Right` expands an app’s open windows and desktop actions. If there are no matching apps, install candidates come from local package metadata. All also shows supported inline calculations and up to three local Web results, plus a direct action for URL input.
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
  - Optional FP8 and general calendar arithmetic remain deferred.
- **Emoji and Symbols (`:`)**:
  - Search Emoji 18.0 offline across seven languages. Select **Symbols** for a separate catalog of arrows, math, currency, and other text characters.
  - Press Enter to copy or Ctrl+Right to choose an emoji variant. In **Settings → Emoji**, enable Ctrl+Enter to paste into the previous Hyprland window, or enable and clear the local recent emoji list.
- **Interactive Design Previews (`--preview`)**:
  - Inspect all planned UI states from a built bundle using `--preview=<name>`:
    - Launcher states: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Clipboard type previews: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

### Planned Scopes & Capabilities (Design Spec)

- **Files (`Ctrl+3` / `/` / `~`)**: Path completion, fuzzy filename search, and previews for directories, text, and images.
- **Run (`>`)**: Clear local clipboard history.

## ⌨️ Development

### Tech Stack & Architecture

- **Frontend**: Flutter & Dart (pinned via `.fvmrc` to `3.50.0`) with `material_ui` (Material Design 3 Expressive) and `flutter_svg`.
  - `lib/app.dart` & `lib/main.dart`: Application composition, CLI flag handling (`--clipboard`, `--settings`, `--preview=...`), and license registration.
  - `lib/features/launcher/`: Launcher panel, search field, scope chips, result/pinned rows, keycap hints, calculator card, file preview, and action menu.
  - `lib/features/clipboard/`: Live clipboard page, Unix socket client, type previews, and clipboard actions.
  - `lib/features/web/`: Web results, keyboard actions, profile settings, and the Web sidecar client.
  - `lib/features/preview/`: Interactive design examples assembled from reusable controls.
- **Backend**: Rust (`edition = "2024"`, `resolver = "3"`) workspace at the repository root:
  - `lib/src/rust/clipboard/`: Wayland clipboard watcher daemon (`remya-launcher-clipboard`), SQLite persistence, and IPC server.
  - `lib/src/rust/apps/`: Resident application index, package lookups, and Hyprland window actions.
  - `lib/src/rust/calculator/`: Local evaluation, completions, and calculator history.
  - `lib/src/rust/web/`: On-demand Web sidecar with browser readers, a private SQLite index, and browser dispatch.
  - `lib/src/rust/core/`: Core Rust crate (reserved for shared launcher backend logic).

### Prerequisites & Setup

Place the Remya Core checkout alongside this repository as `../remya-core` so Flutter can resolve `remya_client` and `remya_ui`. Install GTK 3, `gtk-layer-shell`, CMake, Ninja, Clang, Python 3, `inter-font`, FVM, and the Rust toolchain in `rust-toolchain.toml`.

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

Install the release bundle, desktop entry, and the `top.remya.launcher.clipboard.service` and `top.remya.launcher.apps.service` systemd user units under `$XDG_DATA_HOME`. Calculator and Web run on demand.

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

### Hyprland Shortcuts

Use this Hyprland Lua snippet to replace any existing Launcher bindings. Tap either Super key alone for under 250 ms to toggle Launcher. Super+V opens clipboard history. Adjust the executable path for your installation.

```lua
local superKeys = { [133] = "SUPER_L", [134] = "SUPER_R" }
local down, tap
local launcher = '"$HOME/.local/bin/remya_launcher"'
local toggle = hl.dsp.exec_cmd(launcher .. " --toggle")

hl.on("input.keyboard.key", function(code, timeMs, state)
    if state == 1 then
        tap = nil
        local superHeld = hl.is_key_down("Super_L") or hl.is_key_down("Super_R")
        down = superKeys[code] and not superHeld and { code, timeMs } or nil
    elseif state == 0 and superKeys[code] then
        if down and code == down[1] and (timeMs - down[2]) % 4294967296 < 250 then
            tap = code
        end
        down = nil
    end
end)

for code, key in pairs(superKeys) do
    hl.unbind("SUPER + " .. key)
    hl.bind(key, function()
        if tap == code then
            tap = nil
            hl.dispatch(toggle)
        end
    end, { release = true, description = "Remya: Toggle launcher" })
end

for _, key in ipairs({ "mouse:272", "mouse:273", "mouse:274", "mouse:275", "mouse:276",
    "mouse:277", "mouse:278", "mouse:279", "mouse_up", "mouse_down", "mouse_left", "mouse_right" }) do
    hl.bind(key, function() down, tap = nil, nil end,
        { ignore_mods = true, transparent = true, non_consuming = true })
end

hl.bind("SUPER + V", hl.dsp.exec_cmd(launcher .. " --clipboard"),
    { description = "Remya: Open clipboard history" })
```

## 📜 License

Remya Launcher is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE). Material Symbols and third-party notices are included in `assets/licenses/`.
