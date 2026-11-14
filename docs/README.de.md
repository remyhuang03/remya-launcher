<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Ein Launcher und Zwischenablage-Manager im Material Design 3 Expressive für Hyprland</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.47.8-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.47.8">
  <img src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-4C1?style=flat-square" alt="License: PolyForm Noncommercial 1.0.0">
</p>

<p align="center">
  <a href="../README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.zh-TW.md">繁體中文</a> ·
  <a href="README.ru.md">Русский</a> ·
  <strong>Deutsch</strong> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.ja.md">日本語</a>
</p>

Remya Launcher (`top.remya.launcher`) ist ein tastaturorientierter Anwendungsstarter, Schnellwechsler und Wayland-Zwischenablage-Manager für Hyprland, entwickelt mit Flutter, Rust und Material Design 3 Expressive.

- **Tastaturzentrierter Workflow**: So gestaltet, dass Ihre Hände im Haupttastenfeld bleiben – mit natürlichen Präfix-Bereichen, `Alt+1–9`-Direktauswahl und kontextbezogenen Tastenhinweisen in der Fußzeile.
- **Integrierte Wayland-Zwischenablage**: Ein Rust-Hintergrunddienst speichert kopierten Text, Code, Links, Farben, Dateien und Bild-Miniaturansichten dauerhaft – inklusive Maskierung sensibler Daten und Integration in Remya Settings.
- **Material Design 3 Expressive**: Flüssiges Shape-Morphing, Inter- und JetBrains-Mono-Typografie sowie ein `gtk-layer-shell`-Overlay bei 22 % Bildschirmhöhe.

> [!CAUTION]
> **Early Development & Preview Only — Not Ready for Daily Use (Frühe Entwicklung & nur Vorschau — noch nicht alltagstauglich)**
>
> Remya Launcher befindet sich in einem frühen Stadium aktiver Entwicklung. Derzeit sind der **Taschenrechner**, der **Live-Zwischenablageverlauf** (Rust-Clipboard-Daemon und Live-Clipboard-Ansicht) sowie **interaktive UI-Design-Vorschauen** (`--preview`) implementiert. Die meisten weiteren unten beschriebenen Suchbereiche, Such-Backends sowie Hyprland-Fenster- und Aktionsintegrationen stammen aus der geplanten [Remya Launcher Figma-Spezifikation](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) und sind noch nicht umgesetzt.

## 🚀 Installation

### AUR (`dev`-Kanal)

Unter Arch Linux können Sie das Entwicklungspaket installieren, sobald es im AUR verfügbar ist:

```bash
paru -S remya-launcher-git || yay -S remya-launcher-git
```

### Release-Archiv

Vorkompilierte Release-Archive sind noch nicht veröffentlicht. Sobald Releases auf [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) bereitstehen, prüfen Sie die Prüfsumme, entpacken Sie das Archiv und führen Sie das lokale Installationsskript aus:

```bash
sha256sum -c SHA256SUMS --ignore-missing
tar --zstd -xf remya_launcher-0.1.0+1-linux-x86_64.tar.zst
cd remya_launcher-0.1.0+1-linux-x86_64
python tool/install.py
```

Bei kombinierten Remya-Desktop-Installationen installieren Sie bitte **Remya Settings** vor **Remya Bar**. Aufbewahrungs- und Erfassungseinstellungen der Zwischenablage werden unter **Remya Settings → System → Clipboard** verwaltet.

## ✨ Funktionen

### Derzeit verfügbar

- **Live-Zwischenablageverlauf (`remya_launcher --clipboard` / `Ctrl+6` / `;`)**:
  - Rust-Hintergrunddienst (`remya-launcher-clipboard`), der als systemd-Benutzereinheit `top.remya.launcher.clipboard.service` mit der grafischen Sitzung läuft.
  - Speichert standardmäßig bis zu 500 nicht angeheftete Einträge für 30 Tage in `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (Standard `~/.local/state/remya/launcher/clipboard.sqlite3`); angeheftete Einträge sind von der automatischen Bereinigung ausgenommen.
  - Erfasst Auswahlen bis 64 MiB und dekodierte Bilder bis 24 Megapixel (PNG, JPEG, WebP, GIF, BMP); Bilder über 20 MB innerhalb dieser Grenzen behalten eine Miniaturansicht.
  - Als sensibel markierte Einträge werden höchstens 45 Sekunden lang maskiert im Arbeitsspeicher gehalten und nicht auf den Datenträger geschrieben.
  - Stellt eine JSON-IPC-Schnittstelle über Unix-Socket unter `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` für denselben Benutzer und Remya Settings bereit (`preferences`, `setPreference` für `enabled`/`maxItems`/`retentionDays` sowie `clear`).
  - *Bekannte Wayland-Einschränkung*: Wayland data-control identifiziert nicht, welche Anwendung eine Auswahl bereitgestellt hat; Quell-App-Labels und App-Ausschlusslisten aus dem Design lassen sich daher nicht zuverlässig umsetzen.
- **Rechner (`Ctrl+5` / `=`)**:
  - Wertet grundlegende Ausdrücke inline in `All` sowie vollständige Abfragen in `Calc` (`Ctrl+5` oder `=`) lokal aus — mit `fend-core`-Mathematik, typisierten Einheiten, Zahlensystemen mit Bitbreite und Vorzeichen, FP32/FP16/BF16, Zeitzonen, nur auf explizite Aktualisierung abgerufenen EZB-Referenzkurs-Snapshots, Katalogvervollständigungen und Verlauf.
  - Optionales FP8 und allgemeine Kalenderarithmetik bleiben zurückgestellt.
- **Interaktive Design-Vorschauen (`--preview`)**:
  - Prüfen Sie alle geplanten UI-Zustände aus einem gebauten Bundle mit `--preview=<name>`:
    - Launcher-Zustände: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Zwischenablage-Typvorschauen: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

### Geplante Bereiche & Funktionen (Design-Spezifikation)

Folgende Funktionen sind gemäß der [Remya Launcher Design-Spezifikation](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) geplant:

- **Natürliche Präfix-Bereiche (Scopes)**:
  - **All (`Ctrl+1`)**: Gemischte Ergebnisse aus Apps, Dateien, Webverlauf und Befehlen (max. 3 pro Gruppe, dynamisch nach bestem Treffer sortiert) sowie direkte URL-Erkennung.
  - **Apps (`Ctrl+2`)**: Unscharfe Suche über XDG-`.desktop`-Einträge (Name, `GenericName`, `Keywords`, ausführbare Datei) mit Frecency-Gewichtung und sekundären Desktop-Aktionen (z. B. neues privates Fenster).
  - **Files (`Ctrl+3`, Präfix `/` oder `~`)**: Native Pfadvervollständigung (`Tab` segmentweise), unscharfe Dateinamensuche (`fd` + zuletzt verwendet) und rechte Vorschauspalte (Ordnerinhalt, Textauszüge, Bild-Miniaturansichten) bei Erweiterung auf 880 px.
  - **Web (`Ctrl+4`, Präfix `?`)**: Schreibgeschützter lokaler Browserverlauf (Firefox `places.sqlite` / Chromium `History`) nach Frecency sortiert, mit Fallback-Websuche (`Ctrl+Enter`).
  - **Clipboard (`Ctrl+6`, Präfix `;`)**: 960 px geteilte Ansicht mit Typfiltern (`All · Text · Images · Links · Files · Colors` per `Ctrl+F`), Gruppierung `Pinned → Today → Yesterday → Earlier`, Duplikat-Zusammenführung mit Kopierzähler, typgerechten Vorschauen (Syntax-Highlighting mit Zeilennummern, Farbfelder in HEX/RGB/HSL/OKLCH, Schachbrett-Vorschau für Bilder) und direktem Einfügen in das zuletzt fokussierte Fenster (`Enter` oder `Ctrl+Enter` für typspezifische Sekundäraktionen).
  - **Windows (`Ctrl+7`, Präfix `#`)**: Abgleich über `hyprctl clients` nach Titel und Klasse mit Arbeitsbereichs-Badge; `Enter` fokussiert und wechselt den Arbeitsbereich, `Ctrl+Enter` verschiebt das Fenster auf den aktuellen Arbeitsbereich.
  - **Emoji (Präfix `:`)** & **Run (Präfix `$` / `>`)**: Suche nach Emojis und Symbolen per Name/Alias sowie direkte Shell-/Terminal-Befehlsausführung.
- **Tastaturzentrierte Navigation**:
  - `↑`/`↓` oder `Ctrl+N`/`Ctrl+P` zur Navigation über Gruppen hinweg; `Ctrl+↑`/`Ctrl+↓` springt zum Anfang der vorherigen/nächsten Gruppe.
  - `Alt` gedrückt halten für Zeilennummern und `Alt+1–9` für die primäre Aktion drücken (`Alt+1–6` für angeheftete Einträge im leeren Zustand) oder `Ctrl+Alt+1–9` für die sekundäre Aktion.
  - `→` oder `Ctrl+K` öffnet das filterbare Aktionsmenü; `Ctrl+C` kopiert den ausgewählten Wert; `Ctrl+D` heftet an/löst; `Shift+Del` entfernt aus Recent oder dem Verlauf.
  - `Ctrl+W` / `Ctrl+Backspace` löscht ein Wort oder Pfadsegment; `Ctrl+U` leert die Eingabe; `Esc` geht schrittweise zurück (Menü → Eingabe → Bereich → Schließen); `?` bei leerer Eingabe zeigt die vollständige Tastenkürzel-Übersicht.
- **Hyprland-Overlay & Visuelles System**:
  - `gtk-layer-shell`-Overlay mit exklusiver Tastaturinteraktion, 720 px Standardbreite, ausschließlich nach unten wachsenden Höhenübergängen (250 ms) und Schließen bei Klick außerhalb oder Fokusverlust.
  - Material Design 3 Expressive Shape-Morphing (`20 px → 12 px` bei ausgewählten Zeilen und angehefteten Kacheln, `16 px → 10 px` bei Bereichs-Chips) sowie dynamische Theme-Tokens synchronisiert mit dem Remya-Desktop.

## ⌨️ Entwicklung

### Tech-Stack & Architektur

- **Frontend**: Flutter & Dart (über `.fvmrc` auf `3.47.8` fixiert) mit `material_ui` (Material Design 3 Expressive) und `flutter_svg`.
  - `lib/app.dart` & `lib/main.dart`: Anwendungsaufbau, CLI-Flag-Verarbeitung (`--clipboard`, `--preview=...`) und Lizenzregistrierung.
  - `lib/features/launcher/`: Launcher-Panel, Suchfeld, Bereichs-Chips, Ergebnis-/Pin-Zeilen, Tastenhinweise, Rechnerkarte, Dateivorschau und Aktionsmenü.
  - `lib/features/clipboard/`: Live-Zwischenablage-Seite, Unix-Socket-Client, Typvorschauen und Zwischenablage-Aktionen.
  - `lib/features/preview/`: Interaktive Design-Beispiele aus wiederverwendbaren Steuerelementen.
- **Backend**: Rust-Workspace (`edition = "2024"`, `resolver = "3"`) im Repository-Stammverzeichnis:
  - `lib/src/rust/clipboard/`: Wayland-Zwischenablage-Daemon (`remya-launcher-clipboard`), SQLite-Persistenz und IPC-Server.
  - `lib/src/rust/core/`: Kern-Rust-Crate (reserviert für gemeinsame Launcher-Backend-Logik).

### Voraussetzungen & Einrichtung

Stellen Sie sicher, dass GTK 3, `gtk-layer-shell`, Python 3, `inter-font` (`sudo pacman -S inter-font` unter Arch), FVM und die in `rust-toolchain.toml` definierte Rust-Toolchain installiert sind.

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

Design-Vorschau aus einem Debug-Build starten:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Validierung & lokale Installation

Formatierung, statische Analyse, Tests sowie sequenzielle Debug- und Release-Builds ausführen:

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

Release-Bundle, Desktop-Eintrag und `top.remya.launcher.clipboard.service` unter `$XDG_DATA_HOME` installieren:

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Nutzen Sie `~/.local/bin/remya_launcher --clipboard` für ein Hyprland-`Super+V`-Tastenkürzel.

## 📜 Lizenz

Remya Launcher steht unter der [PolyForm Noncommercial License 1.0.0](../LICENSE). Material Symbols und Hinweise zu Drittanbietern befinden sich in `assets/licenses/`.
