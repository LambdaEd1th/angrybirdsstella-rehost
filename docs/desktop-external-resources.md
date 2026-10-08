# 桌面外置资源恢复

按用户 2026-10-08 的最新要求，撤销资源内置、首次启动自动解包和用户目录资源缓存。
调试构建仍有 CLI；`diagnostic` 保留优化后的调试工具；发布构建直接启动游戏，无 CLI。

## 当前交付与启动约定

- 发布包保留一个游戏可执行文件和外置 `runtime/data`，不附带开发工具。
  Unix 为 `.tar.gz`，Windows 为 `.zip`；用户解开发布包后启动游戏。
- 发布程序优先读取可执行文件旁的 `runtime/data`；旁边没有 `runtime` 时，
  可使用工作目录的 `runtime/data`，兼容仓库根目录的 `cargo run --release`。
  如果旁边已有不完整的 `runtime`，明确报错，避免转入别的安装和存档。
- 保留 `data_root.parent()/appdata`：存档、设置、下载状态和本地服务状态继续
  使用对应外置资源旁的 `runtime/appdata`。不自动迁移或覆盖已有存档。
- 删除 `build.rs`、资源压缩格式、哈希安装缓存及其专用依赖。
  游戏构建不读取资源；`STELLA_RUNTIME_DATA` 和 `STELLA_USER_DATA_DIR` 已撤销。
- 发布流程仅在组装外置资源包时读取固定 runtime archive，仍验证原有 SHA-256、
  原版账户字体、目录安全和 Finder 元数据。打包脚本中的归档处理不在游戏内执行。
  最终包检查架构、Windows GUI 子系统、必要外置资源和单一游戏程序布局。

## 原生依据与范围

本次恢复交付方式，不改变 Lua、资源加载器、游戏逻辑、服务回调或 GPU 渲染行为。
复用已有官方 IDA MCP 证据：`AppController applicationWillTerminate` 的
`0x1004047A8 -> stopUpdate 0x100404E24` 终止持久化边界，以及
`Configuration` 构造函数 `0x100401398` 的默认帧率。
相关既有记录见 `ida-findings.md` 的 “Native BGM restart and forced termination persistence”。
保留已实现的窗口错误汇总与终止回调顺序，以及 Windows 发布错误对话框。
没有新增 Hopper 分析，也没有实现或启用自动通关。

## 本次验证记录

审计目录：`target/audits/restore-external-resources-20261008`。
冻结的实现、Cargo、脚本、工作流及 README 清单为 `source.json`，SHA-256：
`241a3550601c886f8a50383b263efc83258032715f88d8c3bc9383ce657b6b32`。
检查过程在开始、结束时核实该清单不变；本说明单独记录验证结果。

- macOS ARM64、Windows x86_64/ARM64、Linux x86_64/ARM64：各自 debug
  全工作区所有目标/功能及 release 桌面目标严格 Clippy 共 10 项通过；
  Emscripten 浏览器目标严格 Clippy 通过。工具链 Rust 1.98.1，均使用 `-D warnings`。
- `cargo test --workspace --all-targets --all-features --locked --offline`：
  1975 项通过、0 失败，原有 2 项默认忽略保持；release 桌面测试 344 项通过、
  0 失败、0 忽略。原资源安装/缓存的 10 项专项测试随移除功能退役，新增外置资源
  定位 5 项，其余回归保留，包括代表性关卡、重启、失败重试及存档。
- Python 发布资源与打包 11 项通过，0 跳过；格式、YAML 解析和 shell 语法检查通过。
- 五个真实 release 程序均编译并链接成功，最终生成 3 个 tar.gz 和 2 个 zip。
  每包只含一个游戏程序、说明/许可证/构建信息、完整外置资源及空的 `runtime/appdata`。
  所有 2362 个外置资源、175191779 字节逐文件 SHA-256 对照通过；账户字体、
  Windows GUI 子系统、架构、无 Finder 元数据的检查通过。
- 真实 macOS release 从与程序不同的工作目录读取程序旁的外置 Lua；隔离脚本
  错误原样返回非零退出码。缺资源时非零退出，不创建目录或文件，不自动解包；
  `--help` 不进入 CLI。`diagnostic --help` 正常提供 CLI。
- macOS 程序为 16208672 字节，约 15.46 MiB；不含原先资源 bundle 标记。
  资源探针后隔离资源全部还原；正常玩家 `runtime/appdata` 的 14 个文件内容不变。

完整检查和产物哈希见审计目录 `verification.json`，每项检查有命令、终止状态、
日志哈希和冻结源清单。不使用实际凭据或发起实际购买。

跨平台 Clippy、链接和包检查不代表 Windows/Linux 实机图形交互验收。
Windows 交叉链接保留与前一轮相同的 CRT SDK PDB 缺失 `LNK4099` 提示，未屏蔽；
链接退出 0。此次不把启动错误探针作为真实窗口关闭、玩家交互或完整视觉对齐证据。
PowerShell 包装层未在本机执行，已执行其调用的共享 Python 打包逻辑和 Unix 包装层。
固定远端 runtime archive 在本次未重新下载；未来发布仍须通过 SHA-256 和原版字体检查。

长期 goal 保持暂停。本次外置资源恢复不表示完整复刻目标完成。
仅在本地提交，不推送、不上传 GitHub、不创建 tag 或发布。
