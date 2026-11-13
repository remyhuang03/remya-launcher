<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Hyprland 向けの Material Design 3 Expressive ランチャー＆クリップボードマネージャー</h3>

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
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <strong>日本語</strong>
</p>

Remya Launcher（`top.remya.launcher`）は、Flutter、Rust、Material Design 3 Expressive を用いて構築された、Hyprland 向けのキーボードファーストなアプリケーションランチャー、クイックスイッチャー、Wayland クリップボード履歴マネージャーです。

- **キーボードファーストのワークフロー**：メインキー領域から手を離さずに操作できるよう設計されており、自然なプレフィックススコープ、`Alt+1–9` クイックピック、コンテキストに応じたフッターキーヒントを備えています。
- **内蔵 Wayland クリップボード履歴**：バックグラウンドの Rust ウォッチャーがテキスト、コード、リンク、カラー、ファイル、画像サムネイルを永続化し、機密データのマスク処理や Remya Settings との連携に対応します。
- **Material Design 3 Expressive**：滑らかなシェイプモーフィング、Inter および JetBrains Mono のタイポグラフィ、画面高さ 22% に配置される `gtk-layer-shell` オーバーレイを採用しています。

> [!CAUTION]
> **Early Development & Preview Only — Not Ready for Daily Use（初期開発・プレビュー版 — 日常利用には未対応）**
>
> Remya Launcher は現在初期の活発な開発段階にあります。現時点で実装されているのは、**ライブクリップボード履歴**（Rust クリップボードデーモンとライブ Clipboard ビュー）および **インタラクティブ UI デザインプレビュー**（`--preview`）のみです。以下に記載されているその他のランチャースコープ、検索バックエンド、Hyprland ウィンドウ／アクション連携の大部分は、計画中の [Remya Launcher Figma 仕様](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) に基づくものであり、まだ実装されていません。

## 🚀 インストール

### AUR（`dev` チャンネル）

Arch Linux では、AUR パッケージが公開され次第、開発版をインストールできます：

```bash
paru -S remya-launcher-git || yay -S remya-launcher-git
```

### リリースアーカイブ

ビルド済みのリリースアーカイブはまだ公開されていません。[GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) でリリースが公開され次第、チェックサムを検証してアーカイブを展開し、ローカルインストーラーを実行してください：

```bash
sha256sum -c SHA256SUMS --ignore-missing
tar --zstd -xf remya_launcher-0.1.0+1-linux-x86_64.tar.zst
cd remya_launcher-0.1.0+1-linux-x86_64
python tool/install.py
```

Remya デスクトップスイートをペアでインストールする場合は、**Remya Bar** より先に **Remya Settings** をインストールしてください。クリップボードの保持設定やキャプチャ設定は **Remya Settings → System → Clipboard** で管理できます。

## ✨ 機能

### 現在利用可能

- **ライブクリップボード履歴（`remya_launcher --clipboard` / `Ctrl+6` / `;`）**：
  - バックグラウンドの Rust ウォッチャー（`remya-launcher-clipboard`）が systemd ユーザーユニット `top.remya.launcher.clipboard.service` としてグラフィカルセッションとともに稼働します。
  - デフォルトで最大 500 件のピン留めされていない項目を 30 日間、`$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3`（デフォルト `~/.local/state/remya/launcher/clipboard.sqlite3`）に保存します。ピン留めされた項目は自動削除の対象外です。
  - 最大 64 MiB の選択データと最大 24 メガピクセルのデコード画像（PNG、JPEG、WebP、GIF、BMP）をキャプチャし、制限内で 20 MB を超える画像はサムネイルを保持します。
  - ソースアプリによって機密指定されたデータはディスクに書き込まず、メモリ上で最大 45 秒間マスクして保持します。
  - Remya Settings 向けに同一ユーザー用の Unix ソケット JSON IPC インターフェースを `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` に公開しています（`preferences`、`enabled`/`maxItems`/`retentionDays` 用の `setPreference`、`clear`）。
  - *既知の Wayland 制限*：Wayland data-control はクリップボードの提供元アプリケーションを識別できないため、設計にあるソースアプリラベルやアプリごとの除外ブラックリストは確実に実装できません。
- **インタラクティブデザインプレビュー（`--preview`）**：
  - ビルド済みバンドルから `--preview=<name>` を指定して、計画中の全 UI 状態を確認できます：
    - ランチャー状態：`empty`、`results`、`calculator`、`files`、`web`、`actions`、`quick-pick`、`clipboard`
    - クリップボード種別プレビュー：`clipboard-text`、`clipboard-code`、`clipboard-image`、`clipboard-link`、`clipboard-color`、`clipboard-files`、`clipboard-hidden`

### 計画中のスコープと機能（デザイン仕様）

以下の機能は [Remya Launcher デザイン仕様](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) に基づいて計画されています：

- **自然なプレフィックススコープ**：
  - **All（`Ctrl+1`）**：アプリ、ファイル、Web 履歴、コマンドを混在表示（各グループ最大 3 件、最高スコア順で動的配置）。数値や括弧式には計算カードが自動表示され、URL らしい入力にはリンクを開くアクションが提示されます。
  - **Apps（`Ctrl+2`）**：XDG `.desktop` エントリのあいまい検索（名前、`GenericName`、`Keywords`、実行ファイル）と frecency 重み付け、セカンダリデスクトップアクション（新しいプライベートウィンドウ等）に対応。
  - **Files（`Ctrl+3`、`/` または `~` プレフィックス）**：ネイティブパス補完（`Tab` でセグメント単位）、ファイル名あいまい検索（`fd` + 最近使用）、880 px に拡張される右側のプレビューペイン（ディレクトリ内容、テキスト抜粋、画像サムネイル）。
  - **Web（`Ctrl+4`、`?` プレフィックス）**：読み取り専用のローカルブラウザ履歴（Firefox `places.sqlite` / Chromium `History`）を frecency 順に表示し、末尾に Web 検索アクション（`Ctrl+Enter`）を配置。
  - **Calc（`Ctrl+5`、`=` プレフィックス）**：`libqalculate` による計算、単位・通貨換算、進数変換、パーセンテージ、日付演算（`Tab` で候補単位を切り替え、`↑` で履歴式を呼び出し）。
  - **Clipboard（`Ctrl+6`、`;` プレフィックス）**：960 px の分割ビュー、種別フィルター（`Ctrl+F` で `All · Text · Images · Links · Files · Colors` を切り替え）、`Pinned → Today → Yesterday → Earlier` グループ化、重複統合とコピー回数カウント、種別ごとのプレビュー（行番号付きシンタックスハイライトコード、HEX/RGB/HSL/OKLCH カラースウォッチ、市松模様背景の画像プレビュー）、直前にフォーカスしていたウィンドウへの直接ペースト（`Enter`、種別ごとのセカンダリアクションは `Ctrl+Enter`）。
  - **Windows（`Ctrl+7`、`#` プレフィックス）**：`hyprctl clients` によるタイトル／クラス一致とワークスペースバッジ表示。`Enter` でフォーカス＆ワークスペース切替、`Ctrl+Enter` で現在のワークスペースへ移動。
  - **Emoji（`:` プレフィックス）** ＆ **Run（`$` / `>` プレフィックス）**：名前やエイリアスによる絵文字・記号検索と、シェル／ターミナルコマンドの直接実行。
- **キーボードファーストナビゲーション**：
  - `↑`/`↓` または `Ctrl+N`/`Ctrl+P` でグループ間を連続移動、`Ctrl+↑`/`Ctrl+↓` で前後のグループ先頭へジャンプ。
  - `Alt` を押し続けて行番号を表示し、`Alt+1–9` でプライマリアクションを直接実行（空状態では `Alt+1–6` が Pinned に対応）、`Ctrl+Alt+1–9` でセカンダリアクションを実行。
  - `→` または `Ctrl+K` で絞り込み可能なアクションメニューを開き、`Ctrl+C` で選択値をコピー、`Ctrl+D` でピン留め切替、`Shift+Del` で Recent から忘却または履歴削除。
  - `Ctrl+W` / `Ctrl+Backspace` で単語・パスセグメント削除、`Ctrl+U` でクエリ消去、`Esc` でメニュー → クエリ → スコープ → 終了へと段階的に戻り、空入力時の `?` でショートカット一覧を表示。
- **Hyprland オーバーレイとビジュアルシステム**：
  - `gtk-layer-shell` による排他的キーボード入力オーバーレイ（デフォルト幅 720 px、高さ変化時は下方向のみ 250 ms で伸縮、外部クリックまたはフォーカス喪失で閉じる）。
  - Material Design 3 Expressive のシェイプモーフィング（選択行・ピン留めタイル `20 px → 12 px`、スコープチップ `16 px → 10 px`）と Remya デスクトップのシードカラーに連動する動的テーマトークン。

## ⌨️ 開発

### 技術スタックとアーキテクチャ

- **フロントエンド**：Flutter & Dart（`.fvmrc` で `3.47.8` に固定）、`material_ui`（Material Design 3 Expressive）、`flutter_svg`。
  - `lib/app.dart` ＆ `lib/main.dart`：アプリケーション構成、CLI フラグ処理（`--clipboard`、`--preview=...`）、ライセンス登録。
  - `lib/features/launcher/`：ランチャーパネル、検索フィールド、スコープチップ、結果／ピン留め行、キーキャップヒント、計算カード、ファイルプレビュー、アクションメニュー。
  - `lib/features/clipboard/`：ライブクリップボードページ、Unix ソケットクライアント、種別プレビュー、クリップボードアクション。
  - `lib/features/preview/`：再利用可能なコントロールから組み立てられたインタラクティブデザイン例。
- **バックエンド**：リポジトリルートの Rust（`edition = "2024"`、`resolver = "3"`）ワークスペース：
  - `lib/src/rust/clipboard/`：Wayland クリップボードウォッチャーデーモン（`remya-launcher-clipboard`）、SQLite 永続化、IPC サーバー。
  - `lib/src/rust/core/`：コア Rust クレート（共有ランチャーバックエンドロジック用に予約）。

### 前提条件とセットアップ

GTK 3、`gtk-layer-shell`、Python 3、`inter-font`（Arch では `sudo pacman -S inter-font`）、FVM、および `rust-toolchain.toml` で指定された Rust ツールチェーンがインストールされていることを確認してください。

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

デバッグビルドからデザインプレビューを確認するには：

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### 検証とローカルインストール

フォーマット、静的解析、テスト、および Debug/Release ビルドを順次実行します：

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

リリースバンドル、デスクトップエントリ、`top.remya.launcher.clipboard.service` systemd ユーザーユニットを `$XDG_DATA_HOME` 配下にインストールします：

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Hyprland の `Super+V` キーバインドには `~/.local/bin/remya_launcher --clipboard` を指定してください。

## 📜 ライセンス

Remya Launcher は [PolyForm Noncommercial License 1.0.0](../LICENSE) の下でライセンスされています。Material Symbols およびサードパーティのライセンス表記は `assets/licenses/` に含まれています。
