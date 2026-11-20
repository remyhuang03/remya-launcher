# Launcher 常驻后台实验

2026-11-20 · `perf/launcher-resident` · `b018893`

在同一块 4K 外接屏上，冷启动到首个有效内容帧的中位数为 **809 ms**，后台预热后首次打开为 **327 ms**，已经打开过的常驻进程再次响应现有 `--toggle` 为 **176 ms**。常驻的主要开销来自 GPU 驱动持有的系统内存。默认 Impeller 在 4K 屏第一次打开再隐藏后，i915 已报告约 **723 MiB** 为驻留状态。

## 启动延迟

工作站为 Intel Core Ultra 7 255H、Intel Arc 集显、Hyprland 0.57.2、Flutter 3.50.0 和三屏会话。下表使用 DP-3 的同屏样本。该屏为 3840 × 2160，GTK/Flutter 的实际子表面缓冲为 4608 × 2592。Apps、Clipboard 和 Core 服务均已运行，系统文件缓存保留。

| DP-3 场景 | 样本 | 中位数 | 最快–最慢 |
| --- | ---: | ---: | ---: |
| 安装版 `--toggle` 进程冷启动 | 10 | 809 ms | 741–843 ms |
| 常驻进程预热约 4 秒后首次用 `--toggle` 打开 | 8 | 327 ms | 290–395 ms |
| 常驻进程用 `--toggle` 重开 | 6 | 176 ms | 167–201 ms |
| 常驻进程用 D-Bus `Activate` 重开 | 8 | 159 ms | 131–253 ms |

计时从调用入口启动前开始，到 Flutter 提交匹配分辨率的子表面缓冲、GTK 再提交父表面为止。通过 Wayland 协议记录区分透明的外壳和实际内容。现有快捷键执行 `--toggle`，其样本包含临时 CLI 进程启动和 D-Bus 转发。直接 `Activate` 用于打开已经隐藏的实例，命令到服务端的中位延迟分别为 39 ms 和 7 ms。预热本身到 Flutter 隐藏首帧中位约 608 ms。

额外采到的面板屏及 1080p 外接屏样本未混入上表。一次重开过程中窗口失焦并自动隐藏，该轮从统计排除。这里测的是应用提交内容帧，物理按键到可输入、显示器实际呈现和首批结果就绪需要各自验证。 [冷启动对照与口径](launcher-cold-start.md)

## 隐藏常驻的内存

CPU 侧用 `/proc/PID/smaps_rollup` 计量 Launcher 和可读子进程的 PSS。Apps 和 Web helper 会禁用 dumpable，只能获取它们的 RSS，进程树的 PSS 用区间表达。下面的下界包含 Launcher 和 Files 的 PSS，上界再加不可读 helper 的 RSS。既有 Apps 和 Clipboard 服务始终运行，表中的增量依据 GUI 及其子进程。

| 后台状态 | 可读 PSS | Apps + Web RSS | 进程树 PSS 范围 |
| --- | ---: | ---: | ---: |
| 预热完成，尚未打开，原测试环境 | 99–103 MiB | 约 51 MiB | 约 99–154 MiB |
| 4K 重开 12 次，隐藏 10–12 秒，原测试环境 | 108–109 MiB | 约 51 MiB | 约 108–159 MiB |
| 隐藏，Apps 使用既有 daemon 的代理连接 | 约 100–110 MiB | 约 18 MiB | 约 100–128 MiB |

原测试进程的 `XDG_DATA_DIRS` 与 systemd 用户管理器不同，Apps daemon 握手拒绝，触发约 39 MiB RSS 的独立 Apps helper。补测只为实验进程采用用户管理器的 `XDG_DATA_DIRS`，握手成功，Apps helper 降到约 6 MiB RSS。Web helper 约 12 MiB RSS，两种状态均已计入。

GPU 侧读取 Intel i915 的 `drm-resident-system0`，按 DRM client ID 去重。这是驱动报告已有 backing 的系统内存缓冲，其中包含共享页，和进程 PSS 有交叠风险。首次预热约 46–53 MiB。默认 Impeller 一次 4K 打开再隐藏后就约 723 MiB，重开 12 次后隐藏到第 12 秒仍约 768 MiB。一次隐藏 2–12 秒的 CPU 采样中，GUI 及其 helper 合计约占单核的 0.6%。 [Linux DRM 统计定义](https://docs.kernel.org/gpu/drm-usage-stats.html#memory)

## 降低 GPU 驻留

为原生原型固定 DP-3，并用能连接既有 Apps daemon 的环境，比较四种配置。每种从新进程预热开始，打开一次后再重开 3 次。最后隐藏约 11 秒采内存。延迟中位数取这 3 次直接 D-Bus 重开的样本。

| 4K 常驻配置 | GPU 系统内存驻留 | 其中驱动标为共享 | 进程树 PSS 范围 | D-Bus 重开 |
| --- | ---: | ---: | ---: | ---: |
| Impeller | 718 MiB | 192 MiB | 104–122 MiB | 165 ms |
| Impeller，隐藏时释放旧子表面 | 623 MiB | 96 MiB | 106–124 MiB | 180 ms |
| Skia OpenGL | 405 MiB | 192 MiB | 97–115 MiB | 153 ms |
| Skia OpenGL，隐藏时释放旧子表面 | **213 MiB** | 0 MiB | 95–112 MiB | **160 ms** |

隐藏后主动让原有 Flutter view 重新分配 GTK 尺寸，会让 renderer 发现 Wayland 父表面已消失并清掉旧的 subsurface 和 EGL surface。把这一步和 Skia OpenGL 合用，这个 4K 对照中驱动报告的驻留比默认常驻下降了 **约 70%**。四组隐藏样本的 GPU 活跃缓冲均为零。

另起一轮以现有快捷键的 `--toggle` 入口验证 Skia 加释放子表面。首次预热打开为 270 ms，6 次重开中位数为 **190 ms**，范围 **172–282 ms**。最后隐藏约 16 秒时，GPU 仍驻留 **261 MiB**，进程树 PSS 约 **99–117 MiB**。Skia 和释放逻辑的长期内存、交互兼容性和跨 GPU 表现仍需验证。

## 原型行为

复用现有 GTK/Flutter 窗口。后台模式先 realize Flutter view，等调用时再 present。关闭时 hide 窗口，保留进程和 Dart 状态。原型的三屏交互 smoke 验证了显示时一个主图层和三个捕获层、隐藏无图层、D-Bus 持续归同一 PID 所有。

直接 hide/present 会遇到只有透明父层的重开。记录中，GTK 创建了新父表面，旧 Flutter 子表面销毁后未立即创建新的子表面。remap 后为 view 子树请求 allocation 可恢复内容提交。应用侧后续还需为每次打开建立独立搜索会话，重取 Clipboard 回贴窗口，并检查跨屏、IME、拖放和设置窗口。 [隔离原型补丁](launcher-resident-prototype.patch)

复现时在基线提交 `b018893` 的临时工作树应用补丁，构建 Linux release，使用构建产物启动 `--resident`。原型支持给该进程设置 `LAUNCHER_PERF_SKIA=1` 和 `LAUNCHER_PERF_DROP_HIDDEN_SURFACE=1`。另一次调用同一二进制的 `--toggle` 可打开或隐藏。实验复用了安装包的 Dart AOT、Flutter 引擎、资源和 sidecars，仅重新编译临时原生 runner。
