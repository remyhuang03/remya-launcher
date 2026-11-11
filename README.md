# Remya Launcher

Linux launcher for Hyprland, built with Flutter, Rust and Material Design 3 Expressive. Clipboard history is available in the live launcher. Other scopes are design previews.

## Run

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

The default view opens the launcher. Select Clipboard, type `;`, or press `Ctrl+6` to open history. Run `remya_launcher --clipboard` to go directly to history. Use that command for a Hyprland `Super+V` binding. To open a design example from a built bundle, pass `--preview`.

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

Available examples are `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, and `clipboard`. Clipboard detail variants are `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, and `clipboard-hidden`.

The design uses the Inter and JetBrains Mono font families. The preview accepts the JetBrainsMono Nerd Font Mono family and falls back to system fonts where needed.

## Layout

- `lib/features/launcher/` contains the panel, search field, scope chips, result and pin rows, key hints, calculator card, file preview, and action menu.
- `lib/features/clipboard/` contains the live clipboard page, its service client, and the clipboard preview controls.
- `lib/features/preview/` assembles the design examples using the reusable controls.
- `lib/src/rust/clipboard/` builds the clipboard service.

The Material Symbols and example app icons come from the [Remya Launcher design](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher?node-id=2005-24). App logos belong to their respective owners and identify the sample apps.

## Install locally

The Linux build needs GTK 3, gtk-layer-shell, Python 3, the pinned Flutter and Rust toolchains, and the Inter font. The clipboard service runs under the systemd user manager in a Hyprland graphical session.

```bash
fvm flutter build linux --release
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

The installer puts the bundle, desktop entry and `top.remya.launcher.clipboard.service` user unit under the user's XDG data directory. The clipboard service runs with the graphical session. The Launcher window runs on demand.

## Clipboard history

Manage capture, maximum history count, retention days, and clearing in **Remya Settings → System → Clipboard**. History defaults to 500 unpinned items for up to 30 days. Pinned items are exempt from automatic pruning. The service stores ordinary items and settings in `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (default `~/.local/state/remya/launcher/clipboard.sqlite3`).

Wayland data-control does not identify which application provided a clipboard selection. Source labels and per-application exclusions in the design therefore cannot be implemented reliably. Link previews show the copied URL. For data marked sensitive by the source application, Launcher keeps a masked entry in memory for up to 45 seconds. These controls apply only to Launcher history.

History search uses titles, previews and local paths. The watcher caps each selection at 64 MiB and each decoded image at 24 megapixels. Images above 20 MB within those limits retain a thumbnail. Captured image formats are PNG, JPEG, WebP, GIF and BMP.

The same-user service interface lives at `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock`. Settings sends one UTF-8 JSON object terminated by a newline and receives one JSON response, with either `{"ok":true,"value":...}` or `{"ok":false,"error":"..."}`.

| Request | Successful value |
| --- | --- |
| `{"op":"preferences"}` | An object with `enabled`, `maxItems`, and `retentionDays` |
| `{"op":"setPreference","key":"maxItems","value":500}` | The effective preferences. Also accepts `enabled` and `retentionDays` as keys |
| `{"op":"clear"}` | `null` after clearing Launcher history |
