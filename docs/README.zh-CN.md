<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">适用于 Hyprland 的 Material Design 3 Expressive 启动器与剪贴板管理器</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.47.8-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.47.8">
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

Remya Launcher（`top.remya.launcher`）是一款专为 Hyprland 设计的键盘优先应用启动器、快速切换器与 Wayland 剪贴板历史管理器，基于 Flutter、Rust 与 Material Design 3 Expressive 构建。

- **键盘优先的工作流**：手不离主键区即可完成全部操作，支持自然前缀范围、`Alt+1–9` 直达选择与上下文底部按键提示。
- **内置 Wayland 剪贴板历史**：后台 Rust 监听守护进程持久化保存文本、代码、链接、颜色、文件与图片缩略图，支持敏感内容遮罩与 Remya Settings 联动。
- **Material Design 3 Expressive**：流畅的形状形变动效、Inter 与 JetBrains Mono 字体排版，以及固定在屏幕高度 22% 处的 `gtk-layer-shell` 浮层。

> [!CAUTION]
> **Early Development & Preview Only — Not Ready for Daily Use（早期开发与预览阶段 — 尚不适合日常使用）**
>
> Remya Launcher 目前处于早期积极开发阶段。当前已实现**计算器**、**实时剪贴板历史**（Rust 剪贴板守护进程与实时 Clipboard 视图）以及**交互式 UI 设计预览**（`--preview`）。下文描述的其余大多数启动器范围、搜索后端以及 Hyprland 窗口/操作集成均对应已规划的 [Remya Launcher Figma 设计规范](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher)，尚未完成开发。

## 🚀 安装

### AUR（`dev` 通道）

在 Arch Linux 上，待 AUR 软件包发布后可通过以下命令安装开发版：

```bash
paru -S remya-launcher-git || yay -S remya-launcher-git
```

### 发布归档包

预编译发布归档包尚未发布。未来在 [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) 提供归档包后，可校验哈希、解压并运行本地安装脚本：

```bash
sha256sum -c SHA256SUMS --ignore-missing
tar --zstd -xf remya_launcher-0.1.0+1-linux-x86_64.tar.zst
cd remya_launcher-0.1.0+1-linux-x86_64
python tool/install.py
```

在成套安装 Remya 桌面组件时，请先安装 **Remya Settings**，再安装 **Remya Bar**。剪贴板保留策略与捕获设置可在 **Remya Settings → System → Clipboard** 中管理。

## ✨ 功能特性

### 当前已可用

- **实时剪贴板历史（`remya_launcher --clipboard` / `Ctrl+6` / `;`）**：
  - 后台 Rust 监听服务（`remya-launcher-clipboard`）作为 systemd 用户单元 `top.remya.launcher.clipboard.service` 随图形会话启动。
  - 默认在 `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3`（默认 `~/.local/state/remya/launcher/clipboard.sqlite3`）中保存最多 500 条未置顶记录（最长 30 天）；已置顶项目不受自动清理限制。
  - 单条剪贴内容上限 64 MiB，解码图片上限 2400 万像素（支持 PNG、JPEG、WebP、GIF、BMP）；超过 20 MB 但在限制内的图片将保留缩略图。
  - 由源应用标记为敏感的数据仅在内存中保留最多 45 秒并自动遮罩，不会写入磁盘。
  - 在 `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` 提供同用户 Unix 域套接字 JSON IPC 接口，供 Remya Settings 调用（`preferences`、针对 `enabled`/`maxItems`/`retentionDays` 的 `setPreference` 以及 `clear`）。
  - *已知 Wayland 限制*：Wayland data-control 协议无法识别剪贴板内容由哪个应用提供，因此设计中的来源应用标签与按应用排除黑名单目前无法可靠实现。
- **计算器（`Ctrl+5` / `=`）**：
  - 在 `All` 中内联求值基本表达式并在 `Calc`（`Ctrl+5` 或 `=`）中处理完整查询，基于本地 `fend-core` 数学运算、带类型单位、带位宽与符号性的进制、FP32/FP16/BF16、时区，以及仅在显式刷新时获取的 ECB 参考汇率快照，并提供目录补全与历史记录。
  - 可选的 FP8 与通用日历算术仍处于延后状态。
- **交互式设计预览（`--preview`）**：
  - 从构建产物中传入 `--preview=<name>` 查看所有已设计的界面状态：
    - 启动器状态：`empty`、`results`、`calculator`、`files`、`web`、`actions`、`quick-pick`、`clipboard`
    - 剪贴板类型预览：`clipboard-text`、`clipboard-code`、`clipboard-image`、`clipboard-link`、`clipboard-color`、`clipboard-files`、`clipboard-hidden`

### 规划中的范围与能力（设计规范）

以下能力依据 [Remya Launcher 设计规范](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) 规划：

- **自然前缀范围（Scopes）**：
  - **All（`Ctrl+1`）**：混合展示应用、文件、Web 历史与命令（每组最多 3 条，按各组最高分动态排序），输入类 URL 内容时直接提供打开链接操作。
  - **Apps（`Ctrl+2`）**：基于 XDG `.desktop` 条目的模糊匹配（名称、`GenericName`、`Keywords`、可执行文件名），结合 frecency 排序，并将 desktop actions（如新建隐私窗口）作为次级结果。
  - **Files（`Ctrl+3`，`/` 或 `~` 前缀）**：原生路径补全（`Tab` 逐段补全）、文件名模糊搜索（`fd` + 最近使用），以及将面板扩展至 880 px 的右侧预览窗格（文件夹内容、文本摘要、图片缩略图）。
  - **Web（`Ctrl+4`，`?` 前缀）**：只读本地浏览器历史（Firefox `places.sqlite` / Chromium `History`）按 frecency 排序，并在末尾提供“网页搜索”兜底项（`Ctrl+Enter`）。
  - **Clipboard（`Ctrl+6`，`;` 前缀）**：960 px 左右分栏视图，支持类型过滤（通过 `Ctrl+F` 在 `All · Text · Images · Links · Files · Colors` 间切换）、`Pinned → Today → Yesterday → Earlier` 分组、重复项合并与复制次数统计、按类型定制的预览（带行号的语法高亮代码、HEX/RGB/HSL/OKLCH 色块、棋盘格透明图片预览），以及直接写回并粘贴到上一个聚焦窗口（`Enter`，或 `Ctrl+Enter` 执行按类型定制的次操作）。
  - **Windows（`Ctrl+7`，`#` 前缀）**：通过 `hyprctl clients` 按标题和类名匹配窗口并显示工作区徽标；`Enter` 聚焦并切换工作区，`Ctrl+Enter` 将窗口移至当前工作区。
  - **Emoji（`:` 前缀）** 与 **Run（`$` / `>` 前缀）**：按名称与别名搜索 Emoji/符号，以及直接执行 Shell / 终端命令。
- **键盘优先导航**：
  - `↑`/`↓` 或 `Ctrl+N`/`Ctrl+P` 跨分组连续移动；`Ctrl+↑`/`Ctrl+↓` 跳至上一/下一分组首项。
  - 按住 `Alt` 显示行号并按 `Alt+1–9` 直接执行主操作（空状态下对应 `Alt+1–6` Pinned 快捷项），`Ctrl+Alt+1–9` 执行次操作。
  - `→` 或 `Ctrl+K` 打开可输入过滤的 Actions 菜单；`Ctrl+C` 复制选中值；`Ctrl+D` 置顶/取消置顶；`Shift+Del` 从 Recent 遗忘或从历史中删除。
  - `Ctrl+W` / `Ctrl+Backspace` 删除前一个词或路径段；`Ctrl+U` 清空查询；`Esc` 逐级退出（菜单 → 查询 → 范围 → 关闭）；空输入按 `?` 查看完整快捷键速查表。
- **Hyprland 浮层与视觉系统**：
  - 采用 `gtk-layer-shell` 独占键盘交互浮层，默认宽度 720 px，高度变化时仅向下伸缩（250 ms），点击外部或失焦即关闭。
  - Material Design 3 Expressive 形状形变（选中行与置顶卡片圆角 `20 px → 12 px`，范围 chip `16 px → 10 px`），并与 Remya 桌面主题主色调动态同步。

## ⌨️ 开发

### 技术栈与架构

- **前端**：Flutter & Dart（通过 `.fvmrc` 固定为 `3.47.8`），使用 `material_ui`（Material Design 3 Expressive）与 `flutter_svg`。
  - `lib/app.dart` 与 `lib/main.dart`：应用装配、CLI 参数解析（`--clipboard`、`--preview=...`）及许可证注册。
  - `lib/features/launcher/`：启动器面板、搜索框、范围 chip、结果/置顶行、键帽提示、计算卡片、文件预览与操作菜单。
  - `lib/features/clipboard/`：实时剪贴板页面、Unix 套接字客户端、类型预览与剪贴板操作。
  - `lib/features/preview/`：基于可复用控件组装的交互式设计示例。
- **后端**：位于仓库根目录的 Rust（`edition = "2024"`，`resolver = "3"`）工作区：
  - `lib/src/rust/clipboard/`：Wayland 剪贴板监听守护进程（`remya-launcher-clipboard`）、SQLite 持久化与 IPC 服务。
  - `lib/src/rust/core/`：核心 Rust crate（预留用于共享启动器后端逻辑）。

### 前置依赖与环境配置

请确保系统已安装 GTK 3、`gtk-layer-shell`、Python 3、`inter-font`（Arch 下可执行 `sudo pacman -S inter-font`）、FVM 以及 `rust-toolchain.toml` 指定的 Rust 工具链。

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

从 Debug 构建产物启动设计预览：

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### 验证与本地安装

依次运行格式化检查、静态分析、测试以及 Debug/Release 构建：

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

将 Release 包、桌面入口文件以及 `top.remya.launcher.clipboard.service` systemd 用户单元安装至 `$XDG_DATA_HOME`：

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

可将 `~/.local/bin/remya_launcher --clipboard` 绑定到 Hyprland 的 `Super+V` 快捷键。

## 📜 许可证

Remya Launcher 采用 [PolyForm Noncommercial License 1.0.0](../LICENSE) 许可协议。Material Symbols 与第三方声明位于 `assets/licenses/`。
