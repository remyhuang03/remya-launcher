# Agent Instructions & Development Guidelines

## Project scope

- Remya Launcher is a Linux desktop application for Hyprland, built with Flutter and Rust. Its application ID is `top.remya.launcher` and its executable is `remya_launcher`.
- Clipboard history is implemented by Remya Launcher and managed through the Clipboard page in Remya Settings. Keep example data separate from reusable controls. Add system integrations only when requested. Never present examples as detected system state.
- Keep application composition in `lib/app.dart`, startup in `lib/main.dart`, and feature code under `lib/features/<feature>/`. Extract shared widgets only when multiple call sites need them.
- Keep the Flutter project and Cargo workspace at the repository root. Place Rust crates in `lib/src/rust/<crate>/` and keep the core crate empty until it has a concrete responsibility. Keep any future generated bridge code separate from handwritten code.
- Use the public Remya Settings client for future Settings integrations. Respect XDG paths and preserve user configuration. Do not modify Hyprland configuration.
- Write rule documents in English.

## UI and toolchains

- Use Material Design 3 Expressive and import `package:material_ui/material_ui.dart` for Material widgets. Preserve keyboard navigation, visible focus, readable contrast, and scrolling.
- Use dot shorthand where the expected type is unambiguous. Split complex widgets by responsibility and keep small single-use widgets private.
- Run Flutter and Dart via FVM. Use the pinned `.fvmrc` and the root `rust-toolchain.toml`. Keep dependency lockfiles tracked and ignore build artifacts and machine-specific settings.
- Reuse existing packages and helpers. Preserve third-party notices and check dependency compatibility with the PolyForm Noncommercial License.

## Validation and delivery

- Run Dart format, Flutter analyze and test, the Linux debug and release builds, Cargo format, Clippy and tests, and `git diff --check` as relevant. Run debug and release builds sequentially.
- After application changes, rebuild and install the release bundle with `python tool/install.py`. Verify the installed application on the desktop and close verification instances. The GUI runs on demand and the clipboard service runs with the graphical session.
- Develop on a separate `<type>/<short-description>` branch and use Conventional Commits. Stage only intended files. Show completed changes and validation before requesting approval to commit unless the user has already authorized it. Commit approval also authorizes merging to `main`, cleaning up the merged development branch, and pushing `main` to the private origin.
