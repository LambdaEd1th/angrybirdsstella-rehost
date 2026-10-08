# 桌面调试 CLI 与单文件发布

2026-10-08，完成用户明确授权的桌面构建/打包调整。长期完整复刻 goal
保持 PAUSED，不据此宣告完整复刻、全部平台运行或原版视觉对齐完成。

## 实现与边界

- debug 及继承 release 的 `diagnostic` profile 保留 CLI；交付的 release
  游戏入口不编译 CLI，不解析参数，直接沿用原有 `StellaApp`/winit 启动流程。
  测试程序保留诊断入口，测试程序不作为交付游戏。
- `build.rs` 在关闭 debug assertions 时验证原版字体及必要资源目录，排序
  全部文件，将无时间戳的压缩资源和逐文件 SHA-256 清单编译进程序。开发
  构建不要求资源存在；没有完整资源的发布构建明确失败。
- 启动时使用私有 staging 目录，逐文件验证后原子安装；同一内容共用目录，
  并发首次启动只采用完整安装。缓存文件被修改、丢失或替换为符号链接时
  报错，不以空资源继续运行。Finder 元数据不进入资源包。
- 资源使用 `<用户数据>/runtime/<清单哈希>`；保持现有加载器的
  `data_root.parent()/appdata` 规则，因此资源版本变化仍沿用稳定存档目录。
  `STELLA_USER_DATA_DIR` 提供绝对路径隔离测试/便携数据选择，正常启动不迁移
  开发工作区存档。原有本地账户、云存档、成就及商店替代服务默认继续启用。
- Windows 交付程序使用 GUI 子系统与静态 CRT。窗口错误在原有最终暂停、
  音频停止和脚本保存回调完成后向启动器返回；Windows 用系统对话框显示
  错误。本次仅增加宿主错误保留，不改变回调次序。
- 五个桌面目标各生成一个游戏可执行文件，不另附调试工具、资源目录或
  解压工具。Web 部署保留独立流程。macOS 是裸 Mach-O，Linux 是 ELF，
  Windows 是 `.exe`；操作系统执行权限、图形驱动及系统桌面/音频库仍适用。

这属于新增宿主打包能力，不声称 Purple 使用该压缩包或桌面缓存布局，
不计作完整 goal 的原生性能优化/结构调整验收。没有新增原版逆向或核验，
没有调用 Hopper。复用已有 IDA 生命周期证据中的
`applicationWillTerminate: 0x1004047A8` 与 `stopUpdate 0x100404E24`：
终止仍触发最终脚本持久化，错误仅在此后返回。历史证据见
[ida-findings.md](ida-findings.md) 的 “Native BGM restart and forced
termination persistence” 段；该历史记录的 Hopper 交叉结果不作为新增依据。

## 验证结果

原始记录位于 `target/audits/single-executable-release-20261008/`：

- macOS ARM64、Linux ARM64/x86_64、Windows ARM64/x86_64：调试工作区
  全 targets/features 和 release 桌面全部 targets/features 严格 Clippy 均通过；
  Web/Emscripten 严格 Clippy 通过。五个实际 release 游戏均成功链接/打包。
- 完整工作区 1980 项通过，0 失败；两项既有默认忽略的长审计未修改。
  release 桌面另有 349 项通过、0 忽略，含实际 Metal 绘制、生命周期和
  资源安装回归。Python 发布输入/打包 9 项通过。新测试没有忽略项。
- debug 和 `diagnostic` 实际构建的 `--help` 正常；五个交付文件均不含
  `script-drag-eval`、`identity-client-key-file`、`list-missing` CLI 字符串。
  两个 Windows PE 的子系统均为 2（GUI）。Linux release `--help` 到达
  GUI 启动，在无显示服务器的隔离容器中明确报显示环境错误，不输出 CLI。
- 裸 macOS 程序从仅含一个交付文件的目录启动；首次资源安装约 3.108 秒。
  2362 个资源文件、175191779 字节全部与清单 SHA-256 一致。macOS 程序
  53641712 字节，SHA-256 为
  `b9482dc922d981d9f9b3975457d596c6023623268e44decaa1af68d83f9510f8`。
- 使用实际解包资源与独立 appdata 的诊断 Metal 渲染进入主菜单，随后进入
  `Chapter01_L01` 并重启。6500 帧后显式 `if ... then error(...) end`
  检查 GameScene、关卡名及已结束的过渡。没有使用原脚本会禁用的 Lua
  `assert` 作验收；必定抛错的对照确认检查会执行、错误会传播。
- 直接 GUI 输入/关闭尚未证明：CUA 对裸程序无法绑定，对临时同字节程序
  容器的自绘窗口读取报 `-10005 timeoutReached`。测试进程受控终止，
  不将 SIGTERM 当作原生窗口关闭证据。临时容器不属于发布产物。

编译/运行检查使用冻结输入 `source-v2.json`，SHA-256
`c0dbf38f5f6638b88490c0b8d19c1720a0804fcc0f6b3f548d602857255d74bf`。
它包含一项 Python 自动生成缓存；最终源码清单剔除此缓存，另记录仅
`.gitignore` 和本说明的收尾差异。所有 Rust、Cargo、发布脚本与工作流
均与完成检查时一致。最初缺少一个测试 fixture 新字段、离线镜像缺少
新增 crate 索引，以及一次误用截图参数的失败记录保留，修复/重试结果
与之分开，不覆盖失败证据。

## 后续发布约束

本次只准备本地交付文件与修改发布流程，不执行 GitHub 下载、上传、
推送、tag 或发布。工作流的原有 runtime archive pin 与 SHA-256 未改，
未在本次下载核验远端 archive；未来发布前该 archive 必须通过原版字体
及完整资源检查，不允许跳过。已有多文件 release 的历史资产不自动删除，
需使用新版本 tag。只有所有完整 goal 要求都得到充分验证后，才能宣布
完整复刻完成。
