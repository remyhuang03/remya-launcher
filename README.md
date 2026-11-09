# Remya Launcher

Linux Launcher UI for Hyprland, built with Flutter and Material Design 3 Expressive. The example views use fixed design data. The search field accepts text locally, and reusable controls expose callbacks for future integrations.

## Run

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

The default view shows the launcher before a query. To open another design state from a built bundle, pass `--preview`.

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

Available examples are `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, and `clipboard`. Clipboard detail variants are `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, and `clipboard-hidden`.

The design uses the Inter and JetBrains Mono font families. The preview accepts the JetBrainsMono Nerd Font Mono family and falls back to system fonts where needed.

## Layout

- `lib/features/launcher/` contains the panel, search field, scope chips, result and pin rows, key hints, calculator card, file preview, and action menu.
- `lib/features/clipboard/` contains the clipboard filter and preview controls.
- `lib/features/preview/` assembles the design examples using the reusable controls.
- `lib/src/rust/core/` reserves the Rust workspace's core crate for future APIs.

The Material Symbols and example app icons come from the [Remya Launcher design](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher?node-id=2005-24). App logos belong to their respective owners and identify the sample apps.

## Install locally

```bash
fvm flutter build linux --release
python tool/install.py
~/.local/bin/remya_launcher --preview=clipboard
```

The installer puts the bundle and desktop entry under the user’s XDG data directory. Remya Launcher runs on demand.
