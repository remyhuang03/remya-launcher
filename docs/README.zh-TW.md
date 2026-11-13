<p align="center">
  <img src="../assets/icons/launcher.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">適用於 Hyprland 的 Material Design 3 Expressive 啟動器與剪貼簿管理器</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.47.8-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.47.8">
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

Remya Launcher（`top.remya.launcher`）是一款專為 Hyprland 設計的鍵盤優先應用程式啟動器、快速切換器與 Wayland 剪貼簿歷史管理器，使用 Flutter、Rust 與 Material Design 3 Expressive 打造。

- **鍵盤優先的工作流程**：雙手無需離開主鍵區即可完成操作，提供自然前綴範圍、`Alt+1–9` 快速選取與情境式底部按鍵提示。
- **內建 Wayland 剪貼簿歷史**：背景 Rust 監聽常駐程式會保存複製的文字、程式碼、連結、色彩、檔案與圖片縮圖，支援敏感內容遮罩及 Remya Settings 整合。
- **Material Design 3 Expressive**：流暢的形狀形變動畫、Inter 與 JetBrains Mono 字型排版，以及固定於螢幕高度 22% 的 `gtk-layer-shell` 浮層。

> [!CAUTION]
> **Early Development & Preview Only — Not Ready for Daily Use（早期開發與預覽階段 — 尚未適合日常使用）**
>
> Remya Launcher 目前處於早期積極開發階段。現階段僅實作了**即時剪貼簿歷史**（Rust 剪貼簿常駐程式與即時 Clipboard 檢視）以及**互動式 UI 設計預覽**（`--preview`）。以下所述的其餘多數啟動器範圍、搜尋後端與 Hyprland 視窗／動作整合皆屬於規劃中的 [Remya Launcher Figma 設計規範](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher)，尚未實作。

## 🚀 安裝

### AUR（`dev` 頻道）

在 Arch Linux 上，待 AUR 套件上架後可透過以下命令安裝開發版本：

```bash
paru -S remya-launcher-git || yay -S remya-launcher-git
```

### 發行壓縮檔

預編譯的發行壓縮檔尚未發布。未來於 [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) 提供壓縮檔後，請驗證雜湊值、解壓縮並執行本機安裝腳本：

```bash
sha256sum -c SHA256SUMS --ignore-missing
tar --zstd -xf remya_launcher-0.1.0+1-linux-x86_64.tar.zst
cd remya_launcher-0.1.0+1-linux-x86_64
python tool/install.py
```

成套安裝 Remya 桌面元件時，請先安裝 **Remya Settings**，再安裝 **Remya Bar**。剪貼簿保留天數與擷取設定可在 **Remya Settings → System → Clipboard** 中管理。

## ✨ 功能特色

### 目前已提供

- **即時剪貼簿歷史（`remya_launcher --clipboard` / `Ctrl+6` / `;`）**：
  - 背景 Rust 監聽程式（`remya-launcher-clipboard`）以 systemd 使用者服務 `top.remya.launcher.clipboard.service` 隨圖形工作階段啟動。
  - 預設於 `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3`（預設 `~/.local/state/remya/launcher/clipboard.sqlite3`）保存最多 500 筆未釘選項目（最長 30 天）；已釘選項目不受自動清理影響。
  - 單筆選取內容上限 64 MiB，解碼圖片上限 2,400 萬像素（支援 PNG、JPEG、WebP、GIF、BMP）；超過 20 MB 但在限制內的圖片會保留縮圖。
  - 由來源應用程式標記為敏感的資料僅於記憶體中保留最多 45 秒並自動遮罩，不會寫入磁碟。
  - 於 `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` 提供同使用者 Unix socket JSON IPC 介面供 Remya Settings 使用（`preferences`、針對 `enabled`/`maxItems`/`retentionDays` 的 `setPreference` 與 `clear`）。
  - *已知 Wayland 限制*：Wayland data-control 無法辨識剪貼簿內容來自哪個應用程式，因此設計中的來源應用程式標籤與依應用程式排除黑名單無法可靠實作。
- **互動式設計預覽（`--preview`）**：
  - 可從建置完成的 bundle 傳入 `--preview=<name>` 檢視所有設計狀態：
    - 啟動器狀態：`empty`、`results`、`calculator`、`files`、`web`、`actions`、`quick-pick`、`clipboard`
    - 剪貼簿類型預覽：`clipboard-text`、`clipboard-code`、`clipboard-image`、`clipboard-link`、`clipboard-color`、`clipboard-files`、`clipboard-hidden`

### 規劃中的範圍與能力（設計規範）

以下能力依據 [Remya Launcher 設計規範](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) 規劃：

- **自然前綴範圍（Scopes）**：
  - **All（`Ctrl+1`）**：混合呈現應用程式、檔案、Web 歷史與指令（每組最多 3 筆，依各組最佳分數動態排序），輸入數字或括號算式時自動顯示計算答案卡，輸入類 URL 字串時直接提供開啟連結動作。
  - **Apps（`Ctrl+2`）**：XDG `.desktop` 模糊比對（名稱、`GenericName`、`Keywords`、執行檔），結合 frecency 權重與次要 desktop actions（例如開啟無痕視窗）。
  - **Files（`Ctrl+3`，`/` 或 `~` 前綴）**：原生路徑補全（`Tab` 逐段補全）、檔名模糊搜尋（`fd` + 最近使用），以及將面板擴展至 880 px 的右側預覽窗格（資料夾內容、文字摘要、圖片縮圖）。
  - **Web（`Ctrl+4`，`?` 前綴）**：唯讀本機瀏覽器歷史（Firefox `places.sqlite` / Chromium `History`）依 frecency 排序，並於末端提供網頁搜尋動作（`Ctrl+Enter`）。
  - **Calc（`Ctrl+5`，`=` 前綴）**：由 `libqalculate` 驅動的算式計算、單位與貨幣換算、進位制轉換、百分比與日期差計算（`Tab` 輪換候選單位，`↑` 喚回歷史算式）。
  - **Clipboard（`Ctrl+6`，`;` 前綴）**：960 px 左右雙欄檢視，支援類型篩選（透過 `Ctrl+F` 於 `All · Text · Images · Links · Files · Colors` 切換）、`Pinned → Today → Yesterday → Earlier` 分組、重複項目合併與複製次數統計、依類型客製化預覽（含行號的語法高亮程式碼、HEX/RGB/HSL/OKLCH 色塊、棋盤格透明圖片預覽），以及直接寫回並貼上至上一個聚焦視窗（`Enter`，或 `Ctrl+Enter` 執行該類型的次要動作）。
  - **Windows（`Ctrl+7`，`#` 前綴）**：透過 `hyprctl clients` 依標題與類別比對視窗並顯示工作區徽標；`Enter` 聚焦並切換工作區，`Ctrl+Enter` 將視窗移至目前工作區。
  - **Emoji（`:` 前綴）** 與 **Run（`$` / `>` 前綴）**：依名稱與別名搜尋 Emoji／符號，以及直接執行 Shell／終端機指令。
- **鍵盤優先導覽**：
  - `↑`/`↓` 或 `Ctrl+N`/`Ctrl+P` 跨分組連續移動；`Ctrl+↑`/`Ctrl+↓` 跳至上／下一個分組首項。
  - 按住 `Alt` 顯示行號並按 `Alt+1–9` 直接執行主要動作（空狀態下對應 `Alt+1–6` Pinned 項目），`Ctrl+Alt+1–9` 執行次要動作。
  - `→` 或 `Ctrl+K` 開啟可輸入篩選的 Actions 選單；`Ctrl+C` 複製選取的值；`Ctrl+D` 釘選／取消釘選；`Shift+Del` 從 Recent 遺忘或從歷史中刪除。
  - `Ctrl+W` / `Ctrl+Backspace` 刪除前一個詞彙或路徑段；`Ctrl+U` 清空查詢；`Esc` 依序退回（選單 → 查詢 → 範圍 → 關閉）；空白輸入時按 `?` 顯示完整快捷鍵速查表。
- **Hyprland 浮層與視覺系統**：
  - 採用 `gtk-layer-shell` 獨佔鍵盤互動浮層，預設寬度 720 px，高度變化時僅向下伸縮（250 ms），點擊外部或失去焦點即關閉。
  - Material Design 3 Expressive 形狀形變（選取列與釘選卡片圓角 `20 px → 12 px`，範圍 chip `16 px → 10 px`），並與 Remya 桌面主題種子色彩動態同步。

## ⌨️ 開發

### 技術堆疊與架構

- **前端**：Flutter & Dart（透過 `.fvmrc` 鎖定為 `3.47.8`），搭配 `material_ui`（Material Design 3 Expressive）與 `flutter_svg`。
  - `lib/app.dart` 與 `lib/main.dart`：應用程式組裝、CLI 參數處理（`--clipboard`、`--preview=...`）與授權條款註冊。
  - `lib/features/launcher/`：啟動器面板、搜尋欄、範圍 chip、結果與釘選列、按鍵提示、計算機卡片、檔案預覽及動作選單。
  - `lib/features/clipboard/`：即時剪貼簿頁面、Unix socket 用戶端、類型預覽與剪貼簿動作。
  - `lib/features/preview/`：以可重用元件組裝的互動式設計範例。
- **後端**：位於儲存庫根目錄的 Rust（`edition = "2024"`，`resolver = "3"`）工作區：
  - `lib/src/rust/clipboard/`：Wayland 剪貼簿監聽常駐程式（`remya-launcher-clipboard`）、SQLite 持久化與 IPC 伺服器。
  - `lib/src/rust/core/`：核心 Rust crate（保留用於共用啟動器後端邏輯）。

### 先決條件與環境設定

請先確認系統已安裝 GTK 3、`gtk-layer-shell`、Python 3、`inter-font`（Arch 可執行 `sudo pacman -S inter-font`）、FVM 及 `rust-toolchain.toml` 指定的 Rust 工具鏈。

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

從 Debug 建置執行設計預覽：

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### 驗證與本機安裝

執行格式化檢查、靜態分析、測試以及循序的 Debug／Release 建置：

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

將 Release bundle、桌面進入點與 `top.remya.launcher.clipboard.service` systemd 使用者單元安裝至 `$XDG_DATA_HOME`：

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

可將 `~/.local/bin/remya_launcher --clipboard` 設定為 Hyprland 的 `Super+V` 快捷鍵綁定。

## 📜 授權條款

Remya Launcher 採用 [PolyForm Noncommercial License 1.0.0](../LICENSE) 授權。Material Symbols 與第三方聲明位於 `assets/licenses/`。
