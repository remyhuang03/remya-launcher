# Agent Instructions & Development Guidelines

## Project scope

- Remya Launcher is a Linux desktop application for Hyprland, built with Flutter and Rust. Its application ID is `top.remya.launcher` and its executable is `remya_launcher`.
- Keep general shell command execution out of the Launcher search field.
- Clipboard history and its settings pages belong to Remya Launcher. Clipboard policy is registered with and read/written only through Remya Core app preferences. Keep example data separate from reusable controls. Add system integrations only when requested. Never present examples as detected system state.
- Keep application composition in `lib/app.dart`, startup in `lib/main.dart`, and feature code under `lib/features/<feature>/`. Extract shared widgets only when multiple call sites need them.
- Keep the Flutter project and Cargo workspace at the repository root. Place Rust crates in `lib/src/rust/<crate>/` and keep the core crate empty until it has a concrete responsibility. Keep any future generated bridge code separate from handwritten code.
- Use the public remya_client and remya_ui packages from Remya Core for shared integrations. Respect XDG paths and preserve user configuration. Do not modify Hyprland configuration.
- Write rule documents in English.

## UI and toolchains

- Use Material Design 3 Expressive and import `package:material_ui/material_ui.dart` for Material widgets. Preserve keyboard navigation, visible focus, readable contrast, and scrolling.
- Use dot shorthand where the expected type is unambiguous. Split complex widgets by responsibility and keep small single-use widgets private.
- Run Flutter and Dart via FVM. Use the pinned `.fvmrc` and the root `rust-toolchain.toml`. Keep dependency lockfiles tracked and ignore build artifacts and machine-specific settings.
- Reuse existing packages and helpers. Preserve third-party notices and check dependency compatibility with the PolyForm Noncommercial License.

## Validation and delivery

- Run `python tool/check_source_length.py` as part of local validation. It warns for Rust and Dart source files over 800 physical lines, including comments and blank lines. Review every reported file against the single-responsibility principle and refactor files that mix responsibilities.
- When a reviewed file should stay together, run `python tool/check_source_length.py add <path> --reason "<why it should stay together>"` and include the updated `tool/source_length_exceptions.json` in the change. The exception applies only to that file's recorded SHA-256. Changed files still over 800 lines require another review.
- Run Dart format, Flutter analyze and test, the Linux debug and release builds, Cargo format, Clippy and tests, and `git diff --check` as relevant. Run debug and release builds sequentially.
- After application changes, rebuild and install the release bundle with `python tool/install.py`. Verify the installed application on the desktop and close verification instances. The GUI runs on demand and the clipboard service runs with the graphical session. Use the [desktop testing skill](.agents/skills/launcher-desktop-testing/SKILL.md) for installed-bundle smoke checks and input or Wayland debugging.
- Develop on a separate `<type>/<short-description>` branch and use Conventional Commits. Stage only intended files. Show completed changes and validation before requesting approval to commit unless the user has already authorized it. Commit approval also authorizes merging to `main`, cleaning up the merged development branch, and pushing `main` to the private origin.
