# Launcher 冷启动调查

2026-11-20 · `perf/launcher-cold-start` · `b018893`

已安装的 Linux release 在 Hyprland 三屏会话里，12 次进程冷启动的首个 Flutter 内容提交中位数为 **747 ms**，范围 **683–877 ms**。同一会话的另一轮在外接屏量到 **875 ms** 的中位数，当时后台有编译任务。Flutter 首次尺寸同步会延迟透明窗口的映射，跳过这段同步仍未加快实际搜索界面。多屏点击捕获层有可测的成本，独立复测的收益波动较大。

## 测量口径

- 工作站使用 Intel Core Ultra 7 255H、Hyprland 0.57.2、Flutter 3.50.0（引擎 `371f77202a73`）及默认的 Impeller OpenGLES 渲染。Apps、Clipboard 和 Remya Core 服务随会话运行，系统文件缓存保留。
- 每次确认 `top.remya.launcher` 没有 D-Bus owner，再启动进程。最终基线和 A/B 使用快捷键调用的 `--toggle` 参数。计时从创建进程前开始，终点为 Flutter 子表面提交了与配置尺寸匹配的缓冲后，GTK 父表面的第一次 Wayland commit。父层能先提交透明缓冲，Flutter 的 EGL 路径也能先交换旧尺寸缓冲，两者都从终点判定排除。
- 运行 `WAYLAND_DEBUG=client ~/.local/bin/remya_launcher --toggle` 可查看协议序列。`get_layer_surface` 给出 Launcher 父表面，`get_subsurface` 给出 Flutter 子表面，`configure` 和 `set_buffer_scale` 用于核对缓冲尺寸。该口径量到应用提交帧，显示器实际呈现、实体按键派发和搜索结果就绪需要各自的测量。
- 每组先跑一轮预热，正式样本轮换 variant 顺序。表中的成对节省为每轮同一输出的基线减去实验值，再取中位数。不同实验构建和时段各用自己的对照。已安装版通过桌面 smoke，验证 Apps IPC、overlay 映射及 `--toggle` 正常关闭。

| 已安装版 | 样本 | 首内容帧中位数 | 最快–最慢 |
| --- | ---: | ---: | ---: |
| eDP-1，1600 × 1000 逻辑像素 | 12 | 747 ms | 683–877 ms |
| DP-1，1440 × 810 逻辑像素，有并行编译 | 10 | 875 ms | 754–955 ms |

## 首帧时间花在哪里

在 eDP-1 的正式基线，layer configure 出现在约 249 ms，GTK 第一次附加父层缓冲约 662 ms，首次有效内容提交约 747 ms。原生 runner 在映射全屏 layer-shell 窗口时启动 Flutter 和 EGL，并预先显示每块屏幕的点击捕获窗口。 [原生启动](../linux/runner/my_application.cc) · [点击捕获窗口](../linux/runner/launcher_window.cc)

Dart 探针的 8 次对照中，`main` 到 `runApp` 中位 120 ms，其中初始主题就绪约 100 ms，主题就绪后捕获 Clipboard 回贴目标约 20 ms。`runApp` 到第一个 post-frame callback 中位 398 ms，之后到 Wayland 提交中位 113 ms。另一轮 7 次细分探针测到 `runApp` 到 post-frame 中位 288 ms，两套 Material 色彩主题的同步创建合计约 3 ms。首帧构建、布局和提交还有明显的分析空间。Core 主题包含串行的 theme 和 accent D-Bus 请求，服务阻塞或冷激活时可能放大尾延迟。 [Dart 启动](../lib/main.dart) · [首帧主题](../lib/app.dart) · [Clipboard 目标捕获](../lib/features/clipboard/clipboard_platform.dart)

Flutter 引擎的 GTK 尺寸同步逻辑设定了 100 ms 截止时间，等待期间也会处理 Flutter 任务。日志中从 configure 到超时约 0.4 秒，覆盖了尺寸分配前的 GTK 工作和期间执行的任务。跳过首次同步的实验消除了警告，父层透明缓冲相对 configure 由约 401 ms 提前到 79 ms，首内容帧的成对中位数却慢了 90 ms。 [引擎尺寸同步](https://github.com/flutter/flutter/blob/371f77202a73e8dff57a8b3bff75bca55c090aee/engine/src/flutter/shell/platform/linux/fl_view_renderer_subsurface.cc#L74)

## A/B 结果

| 实验 | 成对节省中位数 | 加快的样本 | 备注 |
| --- | ---: | ---: | --- |
| Dart UI 使用独立线程 | 38 ms | 6/10 | 透明父层提早，首内容帧收益不稳定 |
| Impeller 改用 Skia OpenGL | 19 ms | 7/10 | 存在明显变慢的样本 |
| 跳过 Flutter 初次尺寸同步 | −90 ms | 1/10 | 通过隔离的信号探针跳过首次等待 |
| 临时省略三屏点击捕获窗口 | 119 ms | 10/10 | 失去跨屏点击关闭。独立复测中位慢 15 ms，3/7 加快 |
| 延后点击捕获窗口首次绘制 | 62 ms | 4/7 | 捕获层在内容后中位 115 ms 才附加缓冲，另一次未采到首内容帧 |
| Files 图标按需加载，并行捕获 Clipboard 目标 | 40 ms | 4/8 | 到 `runApp` 的中位耗时从 120 ms 降至 89 ms |
| 跳过首次主题等待 | −51 ms | 3/7 | 仅用于诊断，可能先显示默认主题 |
| 只创建当前亮暗模式的主题 | −65 ms | 2/7 | 色彩计算耗时低，整段无可重复收益 |
| 把 Web 引擎列表加载移到首帧之后 | −25 ms | 2/7 | 首帧无稳定收益，仍需验证输入后立即识别引擎快捷词 |
| 合并图标按需、并行捕获、单主题和推迟 Web | −16 ms | 2/7 | 仍未表现出可重复的首帧加速 |

默认 Impeller 配合 GTK 软件渲染的诊断运行在文字绘制时退出。这组没有可比较的首帧样本。

## 优化方向

当前最有证据的原生方向是降低点击捕获层的初始缓冲成本，同时保留主屏外点击关闭和拖放穿透。三块屏幕的 GTK 点击捕获层在一条 DP-1 基线中分别分配了约 25.6 MB、18.7 MB 和 47.8 MB 的首个 RGBA 缓冲。移除这些层能测到约百毫秒的收益，但复测结果不稳定。小透明缓冲配合 Wayland viewport 可以保留全屏输入范围，是可验证的下一种实现。提交前需查每屏点击关闭、跨屏拖放、热插拔及层级。

应用侧可以保留首次正确的主题，同时并行查询 Clipboard 回贴目标，利用 `FileIconTile` 已有的按需缓存。它能缩短 Dart 启动门槛，整段首帧收益相对有限，验证时需覆盖 Files 初次展示图标和真实回贴。 [按需缓存](../lib/features/files/file_icon_tile.dart)

实验使用独立二进制和临时改动。覆盖的是进程冷启动及首内容帧，重启后文件缓存全冷、首次输入、IME、热插拔和全套交互的性能仍待测。
