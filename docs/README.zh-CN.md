<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">适用于 Hyprland 的 Material Design 3 Expressive 启动器与剪贴板管理器</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.50.0-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.50.0">
  <img src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-4C1?style=flat-square" alt="License: PolyForm Noncommercial 1.0.0">
</p>

<p align="center">
  <a href="../README.md">English</a> ·
  <strong>简体中文</strong> ·
  <a href="README.zh-TW.md">繁體中文</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.ja.md">日本語</a>
</p>

> [!CAUTION]
> **早期开发阶段（dev）。** 可能存在缺陷（bug），不提供兼容性保证，可能出现性能与稳定性问题，交互也仍在持续打磨中。

Remya Launcher（`top.remya.launcher`）是一款专为 Hyprland 设计的键盘优先应用启动器、快速切换器与 Wayland 剪贴板历史管理器，基于 Flutter、Rust 与 Material Design 3 Expressive 构建。

- **键盘优先的工作流**：手不离主键区即可完成全部操作，支持自然前缀范围、`Alt+1–9` 直达选择与上下文底部按键提示。
- **应用搜索**：在 All 中搜索已安装的应用，或按 `Ctrl+2` 切换至 Apps。按 Enter 或 `Ctrl+Enter` 打开新的应用窗口。按 `Ctrl+Right` 展开应用的已打开窗口与桌面操作，然后选中窗口按 Enter 即可聚焦。安装候选项来自本地软件包元数据。
- **内置 Wayland 剪贴板历史**：后台 Rust 监听守护进程持久化保存复制的文本、代码、链接、颜色、文件与图片缩略图，支持敏感数据遮罩与 Remya Core 策略存储。
- **本地 Web 搜索**：从已启用的浏览器配置中搜索浏览历史与书签、补全 URL，或使用 `gh ` 等关键词发起搜索。

## 🚀 安装

### 源码构建

请先安装 **Remya Core**。在 Arch Linux 上，请按照[前置依赖与环境配置](#前置依赖与环境配置)和[验证与本地安装](#验证与本地安装)构建并安装 Launcher。

### 发布归档包

对于由仓库 Linux CI 工作流构建的 Linux x86_64 候选版本，请下载并解压候选构建产物。其中包含 `remya-launcher-linux-x86_64-<commit>.tar.zst` 归档包与 `SHA256SUMS`。在包含这些文件的目录中运行：

```bash
sha256sum -c SHA256SUMS
mkdir -p remya-launcher-candidate
tar --zstd -xf remya-launcher-linux-x86_64-*.tar.zst -C remya-launcher-candidate
cd remya-launcher-candidate
python tool/install.py
```

运行 `remya_launcher --settings` 打开 Launcher 设置。使用 `remya_launcher --settings web`、`clipboard`、`apps`、`hidden-apps`、`files-search`、`files-history`、`files-preview`、`files-actions` 或 `material-theme` 可直接打开对应页面。

## ✨ 功能特性

### 当前已可用

- **All 与 Apps（`Ctrl+2`）**：搜索已安装的应用并按 `Enter` 打开。在 Apps 中，按 `Ctrl+Right` 可展开应用的已打开窗口与桌面操作。若无匹配应用，安装候选项将来自本地软件包元数据。All 还会显示支持的内联计算结果、最多三条匹配文件、最多三条本地 Web 结果，以及针对 URL 输入的直接操作。
- **Files（`Ctrl+3` / `/` / `~`）**：
  - 按文件名搜索、按 `Tab` 补全路径，或按 `Ctrl+G` 搜索文件内容。可使用 `ext:md` 或 `in:~/Documents` 等筛选条件缩小结果范围。
  - 预览选中的文件，并按 `Ctrl+K` 打开文件操作。已置顶的文件也会显示在 All 中。
  - 在 **Launcher 设置 → Files → History** 中按需启用最近文件或接入外部历史提供方，各项历史来源均需开启对应选项。
- **Web（`Ctrl+4` / `?`）**：
  - 在 **Launcher Settings → Web** 中启用 Firefox 或 Chromium 系列浏览器配置。即使未启用任何配置，仍可直接打开输入的 URL 或使用搜索引擎进行搜索。Launcher 会从已启用的配置中读取近期历史与书签，并维护私有本地索引；它会从本地浏览器数据读取网站图标，若无可用图标则显示首字母徽标。
  - 可在 **Launcher 设置 → Web** 中选择 DuckDuckGo 或 Google 作为默认搜索引擎。关键词仅切换当前查询的引擎。
  - 空输入时显示常用站点（`Alt+1–6`）与近期页面。搜索时显示按相关性与使用频率排序的书签和历史记录，随后是搜索引擎建议。`Tab` 接受 URL 补全；`Enter` 打开输入的 URL 或选中行；`Ctrl+Enter` 则改为执行搜索。
  - 输入搜索引擎关键词后接空格即可指定引擎，例如 `gh ` 对应 GitHub、`aw ` 对应 ArchWiki、`yt ` 对应 YouTube。选中搜索引擎后，`Tab` 可循环切换引擎；查询为空时按 `Backspace` 可移除引擎词元。
  - `Ctrl+K` 可打开针对页面的操作菜单：在受支持的浏览器或隐私窗口中打开、复制 URL 或 Markdown 链接、站内搜索、置顶或在 Launcher 中隐藏。Launcher 绝不会修改浏览器历史或书签；关闭某项配置会从 Launcher 中移除其索引。
- **Windows（`Ctrl+7` / `#`）**：按标题或类名筛选 Hyprland 窗口。按 `Enter` 聚焦窗口，或按 `Ctrl+Enter` 将其移至当前工作区。
- **实时剪贴板历史（`remya_launcher --clipboard` / `Ctrl+6` / `;`）**：
  - 后台 Rust 监听服务（`remya-launcher-clipboard`）作为 systemd 用户单元 `top.remya.launcher.clipboard.service` 随图形会话启动。
  - 默认在 `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3`（默认 `~/.local/state/remya/launcher/clipboard.sqlite3`）中保存最多 500 条未置顶记录（最长 30 天）；已置顶项目不受自动清理限制。
  - 单条剪贴内容上限 64 MiB，解码图片上限 2400 万像素（PNG、JPEG、WebP、GIF、BMP）；超过 20 MB 但在限制内的图片将保留缩略图。
  - 由源应用标记为敏感的数据仅在内存中保留最多 45 秒并自动遮罩，不会写入磁盘。
  - 在 `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` 面向同一用户提供历史记录操作与 `clear` 接口。策略读取自 `top.remya.Core1` 并在收到 `Changed` 时刷新；当策略不可用或无效时，捕获与自动历史清理将暂停。
  - 安装 `remya/apps/remya-launcher.json` 与桌面 `settings` 操作。剪贴板单元会在 `top.remya.core.service` 之后启动并依赖该服务。
  - *已知 Wayland 限制*：Wayland data-control 协议无法识别剪贴板内容由哪个应用提供，因此设计中的来源应用标签与按应用排除黑名单无法可靠实现。
- **计算器（`Ctrl+5` / `=`）**：
  - 在 `All` 中内联求值基本表达式并在 `Calc`（`Ctrl+5` 或 `=`）中处理完整查询，基于本地 `fend-core` 数学运算、带类型单位、带位宽与符号性的进制、FP32/FP16/BF16、时区，以及仅在显式刷新时获取的 ECB 参考汇率快照，并提供目录补全与历史记录。
  - 可选的 FP8 与通用日历算术仍处于延后状态。
- **Emoji（`:`）**：离线搜索 Emoji 18.0 的七语名称、关键词、短码或 Unicode 序列。Enter 复制，Ctrl+Right 选择变体。
  - 切换到 **Symbols** 搜索独立符号库。在 **设置 → Emoji** 中可启用 Ctrl+Enter 粘贴和本地最近使用。
- **交互式设计预览（`--preview`）**：
  - 从构建产物中传入 `--preview=<name>`，结合示例数据浏览 [Figma 设计稿](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) 中的示例界面状态：
    - 启动器状态：`empty`、`results`、`calculator`、`files`、`web`、`actions`、`quick-pick`、`clipboard`
    - 剪贴板类型预览：`clipboard-text`、`clipboard-code`、`clipboard-image`、`clipboard-link`、`clipboard-color`、`clipboard-files`、`clipboard-hidden`

## ⌨️ 开发

### 技术栈与架构

- **前端**：Flutter & Dart（通过 `.fvmrc` 固定为 `3.50.0`），使用 `material_ui`（Material Design 3 Expressive）与 `flutter_svg`。
  - `lib/app.dart` 与 `lib/main.dart`：应用装配、CLI 参数处理（`--clipboard`、`--settings`、`--preview=...`）及许可证注册。
  - `lib/features/launcher/`：启动器面板、搜索框、范围 chip、结果/置顶行、键帽提示、计算卡片、文件预览与操作菜单。
  - `lib/features/clipboard/`：实时剪贴板页面、Unix 套接字客户端、类型预览与剪贴板操作。
  - `lib/features/files/`：文件名、路径与文件内容搜索、文件操作、预览与设置。
  - `lib/features/web/`：Web 搜索结果、键盘操作、配置设置与 Web 伴生进程客户端。
  - `lib/features/preview/`：基于可复用控件组装的交互式设计示例。
- **后端**：位于仓库根目录的 Rust（`edition = "2024"`，`resolver = "3"`）工作区：
  - `lib/src/rust/clipboard/`：Wayland 剪贴板监听守护进程（`remya-launcher-clipboard`）、SQLite 持久化与 IPC 服务。
  - `lib/src/rust/apps/`：常驻应用索引、软件包查询与 Hyprland 窗口操作。
  - `lib/src/rust/calculator/`：本地求值、补全与计算器历史记录。
  - `lib/src/rust/files/`：按需启动的 Files 伴生进程，提供搜索、预览、操作与按需启用的历史记录。
  - `lib/src/rust/web/`：按需启动的 Web 伴生进程，包含浏览器读取器、私有 SQLite 索引与浏览器分发。
  - `lib/src/rust/core/`：核心 Rust crate（预留用于共享启动器后端逻辑）。

### 前置依赖与环境配置

请将 Remya Core 仓库检出并放置在本仓库同级目录 `../remya-core`，以便 Flutter 解析 `remya_client` 与 `remya_ui`。请确保系统已安装 GTK 3、`gtk-layer-shell`、CMake、Ninja、Clang、Python 3、`inter-font`、FVM 以及 `rust-toolchain.toml` 指定的 Rust 工具链。Files 内容搜索依赖 `ripgrep`，Wayland 下的复制文件功能依赖 `wl-clipboard` 中的 `wl-copy`。

```bash
fvm install
fvm flutter pub get --enforce-lockfile
fvm flutter run -d linux
```

从 Debug 构建产物启动设计预览：

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### 验证与本地安装

依次运行格式化检查、静态分析、测试以及 Debug/Release 构建：

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

将 Release 包、桌面入口文件以及 `top.remya.launcher.clipboard.service` 和 `top.remya.launcher.apps.service` systemd 用户单元安装至 `$XDG_DATA_HOME`。Calculator、Files 与 Web 按需运行。

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

参阅 [Hyprland 快捷键配置](../README.md#hyprland-shortcuts)。

## 📜 许可证

Remya Launcher 采用 [PolyForm Noncommercial License 1.0.0](../LICENSE) 许可协议。Material Symbols 与第三方声明位于 `assets/licenses/`。
