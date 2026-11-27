<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">適用於 Hyprland 的 Material Design 3 Expressive 啟動器與剪貼簿管理器</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.50.0-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.50.0">
  <img src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-4C1?style=flat-square" alt="License: PolyForm Noncommercial 1.0.0">
</p>

<p align="center">
  <a href="../README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <strong>繁體中文</strong> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.ja.md">日本語</a>
</p>

> [!CAUTION]
> **早期開發階段（dev）。** 可能存在錯誤（bug），不提供相容性保證，可能出現效能與穩定性問題，且各項互動仍在持續調整中。

Remya Launcher是一款專為 Hyprland 設計的鍵盤優先應用程式啟動器、快速切換器與 Wayland 剪貼簿歷史管理器，使用 Flutter、Rust 與 Material Design 3 Expressive 打造。

- **鍵盤優先的工作流程**：雙手無需離開主鍵區即可完成操作，提供自然前綴範圍。

## 🚀 安裝

### 原始碼建置

請先安裝 **Remya Core**。在 Arch Linux 上，請依照[先決條件與環境設定](#先決條件與環境設定)與[驗證與本機安裝](#驗證與本機安裝)的步驟建置並安裝 Launcher。

### 發行壓縮檔

安裝 Remya Core `0.3.0` 後，從 [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) 下載 `remya_launcher-0.1.1-linux-x86_64.tar.zst` 和 `SHA256SUMS`。在存放這兩個檔案的目錄中執行以下指令。

```bash
sha256sum -c SHA256SUMS
tar --zstd -xf remya_launcher-0.1.1-linux-x86_64.tar.zst
cd remya_launcher-0.1.1-linux-x86_64
python tool/install.py
```

使用 `remya_launcher --settings` 開啟 Launcher 設定。亦可透過 `remya_launcher --settings web`、`clipboard`、`apps`、`hidden-apps`、`files-search`、`files-history`、`files-preview`、`files-actions` 或 `material-theme` 直接開啟特定頁面。

## ✨ 功能特色

- **All（`Ctrl+1`）與 Apps（`Ctrl+2`）**：搜尋已安裝的應用程式並按 `Enter` 開啟。在 Apps 中，按 `Ctrl+Right` 可展開應用程式已開啟的視窗與桌面動作。若無符合的應用程式，安裝候選項目將來自本機套件中繼資料。All 亦會顯示支援的內嵌計算結果、最多三個相符檔案、最多三筆本機 Web 結果，以及針對網址輸入的直接動作。

- **Files（`Ctrl+3` / `/` / `~`）**：

  - 依檔名搜尋、按 `→` 補全路徑，或按 `Ctrl+G` 搜尋檔案內容。可使用 `ext:md` 或 `in:~/Documents` 等篩選條件縮小結果。
  - 預覽所選檔案，並按 `Ctrl+K` 開啟檔案動作。已釘選的檔案也會顯示於 All 中。
  - 在 **Launcher 設定 → Files → History** 中按需啟用近期檔案或連結外部歷程記錄提供者；各項歷程記錄來源皆須啟用對應選項。

- **Web（`Ctrl+4` / `?`）**：

  - 在 **Launcher Settings → Web** 中啟用 Firefox 或 Chromium 系列設定檔。即使未啟用任何設定檔，仍可開啟輸入的網址或使用搜尋引擎搜尋。Launcher 會從已啟用的設定檔讀取近期歷史紀錄與書籤，並維護私密的本機索引；它會從本機瀏覽器資料讀取網站圖示，若無可用圖示則顯示字首縮寫。
  - 可在 **Launcher 設定 → Web** 中選擇 DuckDuckGo 或 Google 作為預設搜尋引擎。關鍵字只會切換目前查詢的引擎。
  - 空白輸入時會顯示常用網站（`Alt+1–6`）與近期頁面。搜尋時會顯示依相關性與使用頻率排序的書籤與歷史紀錄，接著是搜尋引擎建議。按 `→` 接受網址補全；按 `Enter` 開啟輸入的網址或選取的項目；按 `Ctrl+Enter` 則改為進行搜尋。
  - 輸入搜尋引擎關鍵字並加上空格，例如 `gh ` 代表 GitHub、`aw ` 代表 ArchWiki，或 `yt ` 代表 YouTube。選取搜尋引擎後，按 `Tab` 可循環切換引擎，查詢為空時按 `Backspace` 可移除該關鍵字標記。
  - 按 `Ctrl+K` 可開啟頁面的動作選單：在支援的瀏覽器或私密視窗中開啟、複製網址或 Markdown 連結、搜尋該網站、釘選，或從 Launcher 中隱藏。Launcher 絕不會修改瀏覽器歷史紀錄或書籤；關閉設定檔將會從 Launcher 移除其索引。

- **Windows（`Ctrl+7` / `#`）**：依標題或類別篩選 Hyprland 視窗。按 `Enter` 聚焦視窗，或按 `Ctrl+Enter` 將其移至目前工作區。

- **即時剪貼簿歷史（`remya_launcher --clipboard` / `Ctrl+6` / `;`）**：

  - 背景 Rust 監聽程式（`remya-launcher-clipboard`）作為 `top.remya.launcher.clipboard.service` systemd 使用者單元隨圖形工作階段執行。
  - 預設於 `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3`（預設 `~/.local/state/remya/launcher/clipboard.sqlite3`）保存最多 500 筆未釘選項目（最長 30 天）；已釘選項目不受自動清理影響。
  - 擷取上限 64 MiB 的選取內容與上限 2,400 萬像素的解碼圖片（PNG、JPEG、WebP、GIF、BMP）；超過 20 MB 但在上述限制內的圖片會保留縮圖。
  - 由來源應用程式標記為敏感的項目僅於記憶體中保留最多 45 秒並自動遮罩，不會寫入磁碟。
  - 於 `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` 提供同使用者歷史紀錄操作與 `clear`。原則會從 `top.remya.Core1` 讀取並於 `Changed` 時重新整理；當原則無法取得或無效時，擷取與自動歷史清理將暫停。
  - 安裝 `remya/apps/remya-launcher.json` 與桌面 `settings` 動作。剪貼簿單元會在 `top.remya.core.service` 之後啟動並宣告對其的相依。
  - *已知 Wayland 限制*：Wayland data-control 無法辨識剪貼簿選取內容是由哪個應用程式提供，因此設計中的來源應用程式標籤與依應用程式排除黑名單無法可靠實作。

- **計算機（`Ctrl+5` / `=`）**：

  - 在 `All` 中內嵌求值基本算式並在 `Calc`（`Ctrl+5` 或 `=`）中處理完整查詢，透過本機 `fend-core` 數學運算、具型別單位、含位寬與正負號的進位制、FP32/FP16/BF16、時區，以及僅於明確重新整理時擷取的 ECB 參考匯率快照，並提供目錄補全與歷史紀錄。
  - 選用的 FP8 與一般行事曆算術仍處於延後狀態。

- **Chars（`Ctrl+8` / `:`）**：離線搜尋 Emoji 18.0 的七語名稱、關鍵詞、短碼或 Unicode 序列。Enter 複製，Ctrl+Right 選擇變體。
  - 使用 **All**、**Emoji** 或 **Symbols** 篩選結果。在 **設定 → Emoji** 中可啟用 Ctrl+Enter 貼上和本機最近使用。

- **互動式設計預覽（`--preview`）**：

  - 使用 `--preview=<name>` 從建置完成的 bundle 搭配範例資料檢視 [Figma 設計](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) 中的範例 UI 狀態：
    - 啟動器狀態：`empty`、`results`、`calculator`、`files`、`web`、`actions`、`quick-pick`、`clipboard`
    - 剪貼簿類型預覽：`clipboard-text`、`clipboard-code`、`clipboard-image`、`clipboard-link`、`clipboard-color`、`clipboard-files`、`clipboard-hidden`

## ⌨️ 開發

### 技術堆疊與架構

- **前端**：Flutter & Dart（透過 `.fvmrc` 鎖定為 `3.50.0`），搭配 `material_ui`（Material Design 3 Expressive）與 `flutter_svg`。
  - `lib/app.dart` 與 `lib/main.dart`：應用程式組裝、CLI 旗標處理（`--clipboard`、`--settings`、`--preview=...`）與授權條款註冊。
  - `lib/features/launcher/`：啟動器面板、搜尋欄、範圍 chip、結果與釘選列、按鍵提示、計算機卡片、檔案預覽及動作選單。
  - `lib/features/clipboard/`：即時剪貼簿頁面、Unix socket 用戶端、類型預覽與剪貼簿動作。
  - `lib/features/files/`：檔名、路徑與檔案內容搜尋、檔案動作、預覽及設定。
  - `lib/features/web/`：Web 結果、鍵盤動作、設定檔設定與 Web sidecar 用戶端。
  - `lib/features/preview/`：以可重用控制項組裝的互動式設計範例。
- **後端**：位於儲存庫根目錄的 Rust（`edition = "2024"`，`resolver = "3"`）工作區：
  - `lib/src/rust/clipboard/`：Wayland 剪貼簿監聽常駐程式（`remya-launcher-clipboard`）、SQLite 持久化與 IPC 伺服器。
  - `lib/src/rust/apps/`：常駐應用程式索引、套件查詢與 Hyprland 視窗動作。
  - `lib/src/rust/calculator/`：本機求值、補全與計算機歷史紀錄。
  - `lib/src/rust/files/`：隨選啟動的 Files sidecar，提供搜尋、預覽、動作與選擇性啟用的歷程記錄。
  - `lib/src/rust/web/`：隨選 Web sidecar，包含瀏覽器讀取器、私密 SQLite 索引與瀏覽器分派。
  - `lib/src/rust/core/`：核心 Rust crate（保留用於共用啟動器後端邏輯）。

### 先決條件與環境設定

請將 Remya Core 簽出至與本儲存庫同層級的 `../remya-core`，以便 Flutter 解析 `remya_client` 與 `remya_ui`。請安裝 GTK 3、`gtk-layer-shell`、CMake、Ninja、Clang、Python 3、`inter-font`、FVM，以及 `rust-toolchain.toml` 中指定的 Rust 工具鏈。Files 內容搜尋使用 `ripgrep`，在 Wayland 上複製檔案則使用 `wl-clipboard` 提供的 `wl-copy`。

```bash
fvm install
fvm flutter pub get --enforce-lockfile
fvm flutter run -d linux
```

若要從 Debug 建置檢視設計預覽：

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### 驗證與本機安裝

執行格式化檢查、靜態分析、測試以及循序的 Debug／Release 建置：

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

將 Release bundle、桌面進入點以及 `top.remya.launcher.clipboard.service` 與 `top.remya.launcher.apps.service` systemd 使用者單元安裝至 `$XDG_DATA_HOME`。Calculator、Files 與 Web 則採隨選執行。

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

請參閱 [Hyprland 快捷鍵設定](../README.md#hyprland-shortcuts)。

## 📜 授權條款

Remya Launcher 採用 [PolyForm Noncommercial License 1.0.0](../LICENSE) 授權。Material Symbols 與第三方聲明包含於 `assets/licenses/` 中。
