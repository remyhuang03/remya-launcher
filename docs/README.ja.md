<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Hyprland 向けの Material Design 3 Expressive ランチャー＆クリップボードマネージャー</h3>

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
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <strong>日本語</strong>
</p>

> [!CAUTION]
> **初期開発段階（dev）。** 不具合（バグ）が含まれる可能性があり、互換性の保証はありません。パフォーマンスや安定性の問題が発生する可能性があり、各種インタラクションも現在調整中です。

Remya Launcherは、Flutter、Rust、Material Design 3 Expressive を用いて構築された、Hyprland 向けのキーボードファーストなアプリケーションランチャー、クイックスイッチャー、Wayland クリップボード履歴マネージャーです。

- **キーボードファーストのワークフロー**：メインキー領域から手を離さずに操作できるよう設計されており、自然なプレフィックススコープを備えています。

## 🚀 インストール

### ソースビルド

最初に **Remya Core** をインストールしてください。Arch Linux では、[前提条件とセットアップ](#前提条件とセットアップ) と [検証とローカルインストール](#検証とローカルインストール) の手順に従って Launcher をビルドおよびインストールします。

### リリースアーカイブ

Remya Core `0.3.0` をインストールした後、[GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) から `remya_launcher-0.1.1-linux-x86_64.tar.zst` と `SHA256SUMS` をダウンロードします。両方のファイルがあるディレクトリで次のコマンドを実行します。

```bash
sha256sum -c SHA256SUMS
tar --zstd -xf remya_launcher-0.1.1-linux-x86_64.tar.zst
cd remya_launcher-0.1.1-linux-x86_64
python tool/install.py
```

Launcher の設定は `remya_launcher --settings` で開きます。特定のページを直接開くには、`remya_launcher --settings web`、`clipboard`、`apps`、`hidden-apps`、`files-search`、`files-history`、`files-preview`、`files-actions`、または `material-theme` を指定します。

## ✨ 機能

- **All（`Ctrl+1`）と Apps（`Ctrl+2`）**：インストール済みアプリケーションを検索し、`Enter` で開きます。Apps では `Ctrl+Right` でアプリの開いているウィンドウとデスクトップアクションを展開できます。一致するアプリがない場合、インストール候補はローカルのパッケージメタデータから取得されます。All では対応するインライン計算や最大 3 件の一致ファイル、最大 3 件のローカル Web 結果、URL 入力に対する直接アクションも表示されます。

- **Files（`Ctrl+3` / `/` / `~`）**：

  - ファイル名検索、`→` によるパス補完、`Ctrl+G` によるファイル内容検索に対応しています。`ext:md` や `in:~/Documents` などのフィルターで結果を絞り込めます。
  - 選択したファイルをプレビューし、`Ctrl+K` でファイルアクションを開けます。ピン留めしたファイルは All にも表示されます。
  - **Launcher の設定 → Files → History** で、最近のファイルや外部履歴プロバイダーの連携をオプトインで有効化できます（各履歴ソースは対応するオプションの有効化が必要です）。

- **Web（`Ctrl+4` / `?`）**：

  - **Launcher Settings → Web** で Firefox 系または Chromium 系のプロファイルを有効化します。プロファイルを有効化していない場合でも、入力した URL を開いたり検索エンジンで検索したりできます。Launcher は有効化されたプロファイルから最近の履歴とブックマークを読み取り、プライベートのローカルインデックスを保持します。ファビコンはローカルのブラウザデータから読み取り、利用できない場合はモノグラムを表示します。
  - **Launcher の設定 → Web** で既定の検索エンジンに DuckDuckGo または Google を選べます。キーワードで指定したエンジンは現在の検索にのみ適用されます。
  - 入力が空のときはよく使うサイト（`Alt+1–6`）と最近のページを表示します。検索時にはブックマークと履歴が関連性と使用頻度順に表示され、その後に検索エンジンの候補が続きます。`→` で URL 補完を確定し、`Enter` で入力した URL または選択中の行を開きます。`Ctrl+Enter` を押すと代わりに検索を実行します。
  - GitHub なら `gh `、ArchWiki なら `aw `、YouTube なら `yt ` のように、検索エンジンのキーワードに続けてスペースを入力します。検索エンジンが選択された状態で `Tab` を押すと検索エンジンを切り替えられ、クエリが空のときに `Backspace` を押すとトークンを削除できます。
  - `Ctrl+K` でページに対するアクションを開きます：対応ブラウザまたはプライベートウィンドウで開く、URL または Markdown リンクをコピーする、そのサイト内を検索する、ピン留めする、Launcher から非表示にするといった操作が可能です。Launcher がブラウザの履歴やブックマークを変更することはありません。プロファイルをオフにすると、そのインデックスは Launcher から削除されます。

- **Windows（`Ctrl+7` / `#`）**：Hyprland のウィンドウをタイトルまたはクラスで絞り込みます。`Enter` でウィンドウにフォーカスし、`Ctrl+Enter` で現在のワークスペースに移動します。

- **ライブクリップボード履歴（`remya_launcher --clipboard` / `Ctrl+6` / `;`）**：

  - バックグラウンドの Rust ウォッチャー（`remya-launcher-clipboard`）が systemd ユーザーユニット `top.remya.launcher.clipboard.service` としてグラフィカルセッションとともに稼働します。
  - デフォルトで最大 500 件のピン留めされていない項目を 30 日間、`$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3`（デフォルト `~/.local/state/remya/launcher/clipboard.sqlite3`）に保存します。ピン留めされた項目は自動削除の対象外です。
  - 最大 64 MiB の選択データと最大 24 メガピクセルのデコード画像（PNG、JPEG、WebP、GIF、BMP）をキャプチャし、制限内で 20 MB を超える画像はサムネイルを保持します。
  - ソースアプリケーションによって機密指定されたエントリはディスクに書き込まず、メモリ上で最大 45 秒間自動的にマスクして保持します。
  - `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` で同一ユーザー向けの履歴操作と `clear` を公開しています。ポリシーは `top.remya.Core1` から読み取られ、`Changed` で更新されます。利用不可または無効な間は、キャプチャと自動履歴クリーンアップが一時停止します。
  - `remya/apps/remya-launcher.json` とデスクトップの `settings` アクションをインストールします。クリップボードユニットは `top.remya.core.service` の後に起動し、`top.remya.core.service` を必要（wants）とします。
  - *既知の Wayland 制限*：Wayland data-control はクリップボードの提供元アプリケーションを識別できないため、設計にあるソースアプリラベルやアプリごとの除外ブラックリストは確実に実装できません。

- **電卓（`Ctrl+5` / `=`）**：

  - `All` での基本式のインライン評価および `Calc`（`Ctrl+5` または `=`）でのフルクエリを、ローカルの `fend-core` 演算、型付き単位、ビット幅・符号付き対応の進数、FP32/FP16/BF16、タイムゾーン、明示的な更新時のみ取得する ECB 参照スナップショット、カタログ補完、履歴とともに処理します。
  - オプションの FP8 および一般的なカレンダー演算は引き続き延期されています。

- **Chars（`Ctrl+8` / `:`）**：7 言語の名前やキーワード、ショートコード、Unicode シーケンスで Emoji 18.0 をオフライン検索。Enter でコピーし、Ctrl+Right でバリエーションを選択できます。
  - **All**、**Emoji**、**Symbols** で絞り込めます。**設定 → Emoji** で Ctrl+Enter による貼り付けとローカルの最近使った絵文字を有効にできます。

- **インタラクティブデザインプレビュー（`--preview`）**：

  - ビルド済みバンドルから `--preview=<name>` を指定して、サンプルデータで [Figma デザイン](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) の UI 状態例を確認できます：
    - ランチャー状態：`empty`、`results`、`calculator`、`files`、`web`、`actions`、`quick-pick`、`clipboard`
    - クリップボード種別プレビュー：`clipboard-text`、`clipboard-code`、`clipboard-image`、`clipboard-link`、`clipboard-color`、`clipboard-files`、`clipboard-hidden`

## ⌨️ 開発

### 技術スタックとアーキテクチャ

- **フロントエンド**：Flutter & Dart（`.fvmrc` で `3.50.0` に固定）、`material_ui`（Material Design 3 Expressive）、`flutter_svg`。
  - `lib/app.dart` ＆ `lib/main.dart`：アプリケーション構成、CLI フラグ処理（`--clipboard`、`--settings`、`--preview=...`）、ライセンス登録。
  - `lib/features/launcher/`：ランチャーパネル、検索フィールド、スコープチップ、結果／ピン留め行、キーキャップヒント、計算カード、ファイルプレビュー、アクションメニュー。
  - `lib/features/clipboard/`：ライブクリップボードページ、Unix ソケットクライアント、種別プレビュー、クリップボードアクション。
  - `lib/features/files/`：ファイル名・パス・内容検索、ファイルアクション、プレビュー、設定。
  - `lib/features/web/`：Web 検索結果、キーボードアクション、プロファイル設定、Web サイドカークライアント。
  - `lib/features/preview/`：再利用可能なコントロールから組み立てられたインタラクティブデザイン例。
- **バックエンド**：リポジトリルートの Rust（`edition = "2024"`、`resolver = "3"`）ワークスペース：
  - `lib/src/rust/clipboard/`：Wayland クリップボードウォッチャーデーモン（`remya-launcher-clipboard`）、SQLite 永続化、IPC サーバー。
  - `lib/src/rust/apps/`：常駐アプリケーションインデックス、パッケージ検索、Hyprland ウィンドウアクション。
  - `lib/src/rust/calculator/`：ローカル評価、補完、電卓履歴。
  - `lib/src/rust/files/`：検索、プレビュー、アクション、オプトイン履歴を担うオンデマンドの Files サイドカー。
  - `lib/src/rust/web/`：ブラウザリーダー、プライベート SQLite インデックス、ブラウザ起動ディスパッチを備えたオンデマンド Web サイドカー。
  - `lib/src/rust/core/`：コア Rust クレート（共有ランチャーバックエンドロジック用に予約）。

### 前提条件とセットアップ

Flutter が `remya_client` と `remya_ui` を解決できるよう、このリポジトリと並べて `../remya-core` として Remya Core のチェックアウトを配置してください。GTK 3、`gtk-layer-shell`、CMake、Ninja、Clang、Python 3、`inter-font`、FVM、および `rust-toolchain.toml` で指定された Rust ツールチェーンをインストールしてください。Files の内容検索には `ripgrep`、Wayland でのファイルコピーには `wl-clipboard` の `wl-copy` を使用します。

```bash
fvm install
fvm flutter pub get --enforce-lockfile
fvm flutter run -d linux
```

デバッグビルドからデザインプレビューを確認するには：

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### 検証とローカルインストール

フォーマット、静的解析、テスト、および Debug/Release ビルドを順次実行します：

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

リリースバンドル、デスクトップエントリ、および `top.remya.launcher.clipboard.service` と `top.remya.launcher.apps.service` systemd ユーザーユニットを `$XDG_DATA_HOME` 配下にインストールします。Calculator、Files、Web はオンデマンドで実行されます。

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

[Hyprland のキーバインド設定](../README.md#hyprland-shortcuts)を参照してください。

## 📜 ライセンス

Remya Launcher は [PolyForm Noncommercial License 1.0.0](../LICENSE) の下でライセンスされています。Material Symbols およびサードパーティのライセンス表記は `assets/licenses/` に含まれています。
