<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Ein Launcher und Zwischenablage-Manager im Material Design 3 Expressive für Hyprland</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.50.0-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.50.0">
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

> [!CAUTION]
> **Frühe Entwicklung (dev).** Rechnen Sie mit Bugs; es besteht keine Kompatibilitätsgarantie. Leistungs- und Stabilitätsprobleme können auftreten, und Interaktionen werden noch verfeinert.

Remya Launcher ist ein tastaturorientierter Anwendungsstarter, Schnellwechsler und Wayland-Zwischenablage-Manager für Hyprland, entwickelt mit Flutter, Rust und Material Design 3 Expressive.

- **Tastaturzentrierter Workflow**: So gestaltet, dass Ihre Hände im Haupttastenfeld bleiben – mit natürlichen Präfix-Bereichen.

## 🚀 Installation

### Build aus dem Quellcode

Installieren Sie zuerst **Remya Core**. Folgen Sie unter Arch Linux den Abschnitten [Voraussetzungen & Einrichtung](#voraussetzungen--einrichtung) und [Validierung & lokale Installation](#validierung--lokale-installation), um Launcher zu bauen und zu installieren.

### Release-Archiv

Laden Sie nach der Installation von Remya Core `0.2.0` die Dateien `remya_launcher-0.1.0-dev.20261118.0-linux-x86_64.tar.zst` und `SHA256SUMS` von [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) herunter. Führen Sie die folgenden Befehle im Verzeichnis mit beiden Dateien aus.

```bash
sha256sum -c SHA256SUMS
tar --zstd -xf remya_launcher-0.1.0-dev.20261118.0-linux-x86_64.tar.zst
cd remya_launcher-0.1.0-dev.20261118.0-linux-x86_64
python tool/install.py
```

Öffnen Sie die Launcher-Einstellungen mit `remya_launcher --settings`. Öffnen Sie eine Seite direkt mit `remya_launcher --settings web`, `clipboard`, `apps`, `hidden-apps`, `files-search`, `files-history`, `files-preview`, `files-actions` oder `material-theme`.

## ✨ Funktionen

- **All (`Ctrl+1`) und Apps (`Ctrl+2`)**: Durchsuchen Sie installierte Anwendungen und öffnen Sie sie mit `Enter`. In Apps klappt `Ctrl+Right` die geöffneten Fenster und Desktop-Aktionen einer App aus. Wenn keine passenden Apps vorhanden sind, stammen Installationskandidaten aus lokalen Paketmetadaten. All zeigt außerdem unterstützte Inline-Berechnungen, bis zu drei passende Dateien und bis zu drei lokale Web-Ergebnisse sowie eine direkte Aktion für URL-Eingaben.

- **Files (`Ctrl+3` / `/` / `~`)**:

  - Suchen Sie nach Dateinamen, vervollständigen Sie einen Pfad mit `→` oder drücken Sie `Ctrl+G`, um Dateiinhalte zu durchsuchen. Grenzen Sie Ergebnisse mit Filtern wie `ext:md` oder `in:~/Documents` ein.
  - Sehen Sie sich die Vorschau der ausgewählten Datei an und drücken Sie `Ctrl+K` für Dateiaktionen. Angeheftete Dateien erscheinen auch in All.
  - Aktivieren Sie optional zuletzt verwendete Dateien oder verbinden Sie externe Verlaufsquellen unter **Launcher-Einstellungen → Files → History**. Jede Verlaufsquelle erfordert die entsprechende Option.

- **Web (`Ctrl+4` / `?`)**:

  - Aktivieren Sie Firefox- oder Chromium-Familienprofile unter **Launcher-Einstellungen → Web**. Auch ohne aktivierte Profile können Sie eine eingegebene URL öffnen oder mit einer Suchmaschine suchen. Launcher liest den aktuellen Verlauf und die Lesezeichen aus aktivierten Profilen und führt einen privaten lokalen Index. Favicons werden aus lokalen Browserdaten gelesen; ist keines verfügbar, wird ein Monogramm angezeigt.
  - Unter **Launcher-Einstellungen → Web** lässt sich DuckDuckGo oder Google als Standardsuchmaschine wählen. Suchkürzel gelten für die aktuelle Anfrage.
  - Bei leerer Eingabe werden häufig besuchte Websites (`Alt+1–6`) und zuletzt besuchte Seiten angezeigt. Die Suche zeigt Lesezeichen und Verlauf nach Relevanz und Nutzung sortiert, gefolgt von Suchmaschinenvorschlägen. `→` übernimmt die URL-Vervollständigung; `Enter` öffnet die eingegebene URL oder den ausgewählten Eintrag; `Ctrl+Enter` sucht stattdessen.
  - Geben Sie ein Suchmaschinen-Schlüsselwort gefolgt von einem Leerzeichen ein, z. B. `gh ` für GitHub, `aw ` für ArchWiki oder `yt ` für YouTube. Ist eine Suchmaschine ausgewählt, wechselt `Tab` durch die Suchmaschinen und `Backspace` entfernt das Token, wenn die Suchanfrage leer ist.
  - `Ctrl+K` öffnet Aktionen für eine Seite: In einem unterstützten Browser oder privaten Fenster öffnen, URL oder Markdown-Link kopieren, die Website durchsuchen, anheften oder in Launcher ausblenden. Launcher verändert niemals den Browserverlauf oder die Lesezeichen. Das Deaktivieren eines Profils entfernt dessen Index aus Launcher.

- **Windows (`Ctrl+7` / `#`)**: Filtern Sie Hyprland-Fenster nach Titel oder Klasse. Drücken Sie `Enter`, um ein Fenster zu fokussieren, oder `Ctrl+Enter`, um es auf den aktuellen Arbeitsbereich zu verschieben.

- **Live-Zwischenablageverlauf (`remya_launcher --clipboard` / `Ctrl+6` / `;`)**:

  - Rust-Hintergrunddienst (`remya-launcher-clipboard`), der als systemd-Benutzereinheit `top.remya.launcher.clipboard.service` mit der grafischen Sitzung läuft.
  - Speichert standardmäßig bis zu 500 nicht angeheftete Einträge für 30 Tage in `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (Standard `~/.local/state/remya/launcher/clipboard.sqlite3`); angeheftete Einträge sind von der automatischen Bereinigung ausgenommen.
  - Erfasst Auswahlen bis 64 MiB und dekodierte Bilder bis 24 Megapixel (PNG, JPEG, WebP, GIF, BMP); Bilder über 20 MB innerhalb dieser Grenzen behalten eine Miniaturansicht.
  - Maskiert von der Quellanwendung als sensibel markierte Einträge automatisch für höchstens 45 Sekunden im Arbeitsspeicher, ohne sie auf den Datenträger zu schreiben.
  - Stellt Verlaufsvorgänge für denselben Benutzer sowie `clear` unter `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` bereit. Die Richtlinie wird aus `top.remya.Core1` gelesen und bei `Changed` aktualisiert; solange sie nicht verfügbar oder ungültig ist, pausieren Erfassung und automatische Verlaufsbereinigung.
  - Installiert `remya/apps/remya-launcher.json` und eine Desktop-Aktion `settings`. Die Zwischenablage-Einheit startet nach `top.remya.core.service` und benötigt diese (`Wants`).
  - *Bekannte Wayland-Einschränkung*: Wayland data-control identifiziert nicht, welche Anwendung eine Zwischenablage-Auswahl bereitgestellt hat; Quell-App-Labels und App-Ausschlusslisten aus dem Design lassen sich daher nicht zuverlässig umsetzen.

- **Rechner (`Ctrl+5` / `=`)**:

  - Wertet grundlegende Ausdrücke inline in `All` sowie vollständige Abfragen in `Calc` (`Ctrl+5` oder `=`) lokal aus — mit `fend-core`-Mathematik, typisierten Einheiten, Zahlensystemen mit Bitbreite und Vorzeichen, FP32/FP16/BF16, Zeitzonen, nur auf explizite Aktualisierung abgerufenen EZB-Referenzkurs-Snapshots, Katalogvervollständigungen und Verlauf.
  - Optionales FP8 und allgemeine Kalenderarithmetik bleiben zurückgestellt.

- **Chars (`Ctrl+8` / `:`)**: Emoji 18.0 offline in sieben Sprachen nach Namen, Schlüsselwort, Kurzcode oder Unicode-Sequenz suchen. Enter kopiert das Emoji, Strg+Rechts öffnet die Varianten.
  - Mit **All**, **Emoji** oder **Symbols** lassen sich die Ergebnisse filtern. Unter **Einstellungen → Emoji** lassen sich Einfügen mit Strg+Enter und die lokale Liste zuletzt verwendeter Emojis aktivieren.

- **Interaktive Design-Vorschauen (`--preview`)**:

  - Erkunden Sie beispielhafte UI-Zustände aus dem [Figma-Design](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) mit Beispieldaten aus einem gebauten Bundle über `--preview=<name>`:
    - Launcher-Zustände: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Zwischenablage-Typvorschauen: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

## ⌨️ Entwicklung

### Tech-Stack & Architektur

- **Frontend**: Flutter & Dart (über `.fvmrc` auf `3.50.0` fixiert) mit `material_ui` (Material Design 3 Expressive) und `flutter_svg`.
  - `lib/app.dart` & `lib/main.dart`: Anwendungsaufbau, CLI-Flag-Verarbeitung (`--clipboard`, `--settings`, `--preview=...`) und Lizenzregistrierung.
  - `lib/features/launcher/`: Launcher-Panel, Suchfeld, Bereichs-Chips, Ergebnis-/Pin-Zeilen, Tastenhinweise, Rechnerkarte, Dateivorschau und Aktionsmenü.
  - `lib/features/clipboard/`: Live-Zwischenablage-Seite, Unix-Socket-Client, Typvorschauen und Zwischenablage-Aktionen.
  - `lib/features/files/`: Dateinamen-, Pfad- und Inhaltssuche, Dateiaktionen, Vorschauen und Einstellungen.
  - `lib/features/web/`: Web-Ergebnisse, Tastaturaktionen, Profileinstellungen und der Web-Sidecar-Client.
  - `lib/features/preview/`: Interaktive Design-Beispiele aus wiederverwendbaren Steuerelementen.
- **Backend**: Rust-Workspace (`edition = "2024"`, `resolver = "3"`) im Repository-Stammverzeichnis:
  - `lib/src/rust/clipboard/`: Wayland-Zwischenablage-Daemon (`remya-launcher-clipboard`), SQLite-Persistenz und IPC-Server.
  - `lib/src/rust/apps/`: Residenter Anwendungsindex, Paketabfragen und Hyprland-Fensteraktionen.
  - `lib/src/rust/calculator/`: Lokale Auswertung, Vervollständigungen und Rechnerverlauf.
  - `lib/src/rust/files/`: On-Demand-Files-Sidecar für Suche, Vorschauen, Aktionen und optionalen Verlauf.
  - `lib/src/rust/web/`: On-Demand-Web-Sidecar mit Browser-Readern, privatem SQLite-Index und Browser-Dispatch.
  - `lib/src/rust/core/`: Kern-Rust-Crate (reserviert für gemeinsame Launcher-Backend-Logik).

### Voraussetzungen & Einrichtung

Platzieren Sie den Remya-Core-Checkout neben diesem Repository als `../remya-core`, damit Flutter `remya_client` und `remya_ui` auflösen kann. Installieren Sie GTK 3, `gtk-layer-shell`, CMake, Ninja, Clang, Python 3, `inter-font`, FVM und die in `rust-toolchain.toml` definierte Rust-Toolchain. Die Inhaltssuche in Files nutzt `ripgrep`, und „Datei kopieren“ verwendet unter Wayland `wl-copy` aus `wl-clipboard`.

```bash
fvm install
fvm flutter pub get --enforce-lockfile
fvm flutter run -d linux
```

Um eine Design-Vorschau aus einem Debug-Build zu prüfen:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Validierung & lokale Installation

Formatierung, statische Analyse, Tests sowie sequenzielle Debug- und Release-Builds ausführen:

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

Installieren Sie das Release-Bundle, den Desktop-Eintrag sowie die systemd-Benutzereinheiten `top.remya.launcher.clipboard.service` und `top.remya.launcher.apps.service` unter `$XDG_DATA_HOME`. Rechner, Files und Web laufen bei Bedarf (On-Demand).

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Siehe [Tastenkürzel für Hyprland](../README.md#hyprland-shortcuts).

## 📜 Lizenz

Remya Launcher steht unter der [PolyForm Noncommercial License 1.0.0](../LICENSE). Material Symbols und Hinweise zu Drittanbietern befinden sich in `assets/licenses/`.
