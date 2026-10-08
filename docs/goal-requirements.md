# 完整 goal 验收清单

依据当前实际 goal：优化与拆分/整合代码均须基于官方 IDA MCP 的原生证据。2026-10-08 按用户最新要求更新 Git 策略，相关实现与必要验证完成后提交到本地 Git，不推送、不上传 GitHub、不发布；其余验收标准保留。应用内 get_goal 已确认与 docs/goal-objective.txt 完全相同的 811 字符正文，状态仍为 PAUSED；分段正文见 docs/goal-objective.md，修改前后状态见 target/audits/goal-local-git-20261008。当前仅调整目标规则与文档，未恢复新增实现或分析。此前 ACTIVE、PAUSED 和待应用说明均属对应阶段的历史状态。所有要求均须充分实现与验证，下面的部分证据不能替代完整验收；目标未完成。历史检查与最新生产源码的验证范围必须分开读取。

2026-10-08 最新 continuation 的 get_goal 已确认相同 811 字符目标为 ACTIVE。Clip/Timeline 身份修正保留每个 Clip 与目标的轨道、独立 State 和逐状态移除次序；七个新官方 IDA 指令范围与原文件字节匹配，五项最终相同 fixture 的修复前行为失败与修复后成功均已保存。当前编译输入 725e25b3 下完整工作区 1968 passed、0 failed、2 原有默认 ignored，六目标严格 Clippy、23 项 Linux CPU、五项新 WebAssembly Timeline、22 项浏览器像素及两项显式长审计通过。详情见 docs/native-animation-timeline-identity.md 与 target/audits/native-animation-timeline-identity-20261008。完整目标未完成；本项无新增性能改善结论，Poppy 原症状及严格错误、所有寿命/回调和完整平台/服务/画面要求继续保留。相关已验证改动现按最新策略提交到本地 Git，不推送或上传 GitHub；下文 PAUSED 和无提交说明属于对应历史阶段。

最新实体矩阵与状态核验根据官方 IDA 的实体矩阵、属性 setter、usage/State 所有权、加载及 start/stop 调用路径修正矩阵与优先级差异，并集中 target_groups 职责。57 个捕获/56 个唯一指令及实际虚表范围与原始 ARM64 字节核对；两处后续 RTTI loader 解析差异单独记录。修复前相同新 fixture 在隔离 target 中产生 10 脚本/2 Metal 行为失败。当前编译输入 6819d9bf 下完整工作区 1963 项成功、0 失败、2 项原有默认忽略；六目标严格 Clippy、82 项实际 Metal/1 项 CPU 参考绘制、18 项有条件 Linux CPU、新正式 WebAssembly 和 22 项浏览器像素通过。并发私有目录冲突、旧断言、fixture/编译错误及取消的旧审计保留，修复隔离 helper 后完整重验；两项长审计显式通过六次 BirdRun 静置和完整 131 入口/两章重启。隔离 release 的 L18/Poppy 瞄准帧只作为自身回归，不证明原版视觉。依据、结构理由、身份与终态见 docs/native-animation-entity-matrices.md 和 target/audits/native-animation-attachment-transform-20261008/verification.json。本项无新增性能改善结论；完整多 Clip/event target、严格 timeline 错误、缓存/矩阵边界、原 Poppy 症状及全部原生/平台/服务/性能/结构要求继续保留。

此前 Poppy 服装与附件核验根据五个官方 IDA 函数及原始字节修正多余 basename 回退，保留所选/默认完整键查找和缺失清空次序。当前完整工作区 1951 项成功、0 失败、2 项原有默认忽略，六目标严格 Clippy 全部新执行通过；80 项实际 Metal/1 项 CPU 参考绘制、有条件 Linux 8 项 CPU、6 种 Poppy 固定输入和正式 WebAssembly/实际浏览器 20 项像素回归通过。两项长审计以当前保存程序显式通过：六次 BirdRun 静置及 131 个入口/两章重启，无自动求解。新桌面 release 在隔离资源下绘制 Poppy 服装瞄准页，84 个未定义全局查询保留，实际回退为零；自身截图不能作为原版视觉依据。随附附件盘点产生零个匹配差异，所以该修正不证明 Poppy 原报告原因。原生依据、原失败与构建缓存污染保留、终态日志和精确身份见 docs/native-poppy-costumes.md 和 target/audits/native-poppy-costumes-20261008/verification.json。本项没有新增性能改善结论；原症状与完整目标仍开放。

此前图片输入路径核验修正描述目录回退、独立 AppData 双流及符号链接目标后缀替换；FilePath 纯函数从语言加载模块独立，接口和正文原字节保留。其编译输入 c9a6c7e1 下完整工作区 1942 项及原生/平台结果属于历史证据。三类暖缓存资源构造的交错耗时/CPU 中位数下降约 30%，整进程峰值 RSS 中位数下降 96/96/120 KiB，不代表帧耗时、一般 FPS、GPU 或正常关卡驻留内存改善。原生依据、性能场景及身份见 docs/native-resource-file-lookup.md 和 target/audits/native-resource-file-lookup-20261008/verification.json。以下段落保留此前源码的历史结果。

2026-10-08 资源构造扩查修正图片加载错误被吞、失败重载提前覆盖旧资源及下载 Assets 的加载顺序差异。完整工作区 1766 项通过、2 项原有忽略，包含 17 项新脚本检查和 63 项文件图片 GPU 回归；当时严格检查、源码与产物身份见 target/audits/native-image-construction-20261008/verification.json。详情见 docs/native-resource-image-construction.md。Poppy 原报告仍未稳定复现；该项不关闭全平台、全部资源/错误语义或完整视觉与流程验收。

后续重复 FONT 核验修复了旧字形跟随最后图片继续绘制/测宽的寿命差异，保留缓存指标、有效绘制前缀及 UI/三维错误状态，集中共用入口；原生悬空指针的未定义内存行为转换为明确宿主错误，不声称分配器结果或错误文本等价。当前 24 个随附字体均无重复 FONT，本项不能解释 Poppy 原报告。当前完整工作区 1781 项通过、2 项原有忽略，67 项文件图片 Metal 回归和 macOS/Web 严格 Clippy 通过；新增检查、源码与产物身份见 target/audits/native-font-atlas-ownership-20261008/verification.json 和 docs/native-font-atlas-ownership.md。

最新文件/截图构造边界修复 `<capture:` 字面文件名被误认成宿主截图句柄的问题，整理文件路径与截图的模块依赖。macOS 完整工作区 1793 项通过、2 项原有忽略，71 项文件图片 Metal 检查及 macOS/Web/Linux ARM64 严格 Clippy 通过。Linux 资源/字体寿命回归使用实际 Helvetica 字节作为隔离输入，原断言保留；不把容器字体条件或交叉检查当作物理平台/原生字体完整验收。身份、终态日志与未关闭范围见 target/audits/native-image-paths-20261008/verification.json 和 docs/native-image-paths.md。

当前空资源扩查修复无 FONT/SPRT 记录、异根提前返回与无字形绘制提前访问图集的差异。未初始化字体间距以明确宿主错误处理，保留原生单字符乘加系数为零的已知结果；不声称原生分配器或未定义结果等价。完整 macOS 工作区 1812 项通过、2 项原有忽略，75 项文件图片检查包含 74 项实际 Metal 和 1 项 CPU 参考绘制。当前 macOS/Web/Linux ARM64 严格 Clippy 与有条件 Linux 140 项聚焦 CPU 回归通过。十九项新增回归、旧失败输入修正、异步 fixture 边界修正及终态身份见 target/audits/native-empty-resources-20261008/verification.json 和 docs/native-empty-resources.md；完整目标与 Poppy 原症状保持开放。

本次补齐同一生产源码的 Linux x86_64、Windows x86_64/ARM64 MSVC 严格 Clippy；前轮三目标终态日志经全部非文档输入及产物字节核对后复用，六目标检查范围全部通过。新编译正式 WebAssembly，八项 Node 帧边界、11 语言/输入/尺寸/音频数据及隔离存档恢复、三项空资源探针通过；真实 Chromium WebGL2/ANGLE Metal 11 项逐像素检查通过。初始脚本失败保留，词法严格断言的必定失败对照确认检查实际执行。本次不改生产代码，不关闭物理平台、原版视觉、完整输入/音频或 Poppy 原报告。详情与终态身份见 docs/current-platform-matrix.md 和 target/audits/current-platform-matrix-20261008/verification.json。

| 要求 | 当前证据 | 尚需关闭的范围 |
| --- | --- | --- |
| 跨平台纯 Rust/wgpu 运行 | 当前六目标严格 Clippy 全部新检查；新正式 WebAssembly 与 Chromium ANGLE Metal 22 项像素回归；当前 macOS Metal 与历史 Linux 软件 Vulkan 有界运行 | Windows/DX12 与 Linux x86_64 实际运行、物理 GPU、输入/音频及设备/窗口恢复；严格检查不代表这些运行要求通过 |
| 完整功能与游戏流程 | 用户手动通关；原始 Lua、关卡构造、固定输入、重启、失败重试、存档等脚本回归 | 按原生证据关闭全部流程差异；自动通关已排除，不能为验收重新启用 |
| 画面与交互原生对齐 | 多项数值/生命周期、渲染测试和原生函数证据 | 尚未闭合的字体/光栅、浮点、平台合成及资源别名差异；遵循 visual-reference-policy，自身截图只证明回归 |
| 调用/回调、错误与资源生命周期 | 针对性脚本测试与已有 IDA 证据；本轮保留物理运算和调用次序 | 完整 Texture 别名、错误/无效指针与内存行为，以及 Lua 5.1 数字/字节字符串边界 |
| 平台与服务 | 本地持久化及明确的兼容服务边界；回调与协议测试 | 完整 SDK、平台 UI/输入、真实服务边界的原生验收；本地替代不证明真实平台对齐 |
| 基于官方 IDA MCP 的性能优化 | 已有 IDA FilePath/输入流/解码文件名证据、宿主采样和 48 个交错资源构造样本闭环，原编译输入的暖缓存构造耗时/CPU 中位数约降 30%；历史接触缓存与帧准备测量另有记录；本轮不新增性能结论 | 继续逐项映射原生调用、运算、数据结构与缓存/分配策略并保留正确性实测；窗口呈现/物理音频、多平台及正常关卡帧/GPU/驻留内存测量、L16 首次使用差异与完整纹理寿命；资源构造结果不能替代这些范围 |
| 基于官方 IDA MCP 的拆分/整合代码结构 | 已有原生 FilePath 职责对应纯函数模块，接口、函数/测试正文原字节保留，生命周期与错误回归通过；新增 EntityTarget 状态/矩阵职责对应 target_groups、apply 与 transform 边界，依据、回调/寿命回归及限制见 docs/native-animation-entity-matrices.md；此前共享图元/操作类型和各 PreparedFrame 析构顺序记录保留 | 基于官方 IDA MCP 继续核实原生职责、调用、接口、数据结构和所有权/寿命并审查服务等模块；逐项记录理由与回归；此前调整的原生依据及 PreparedFrame 所有权差异仍需补齐，单项拆分不代表完成 |
| 必要回归与集成验证 | 当前完整 1968 项工作区、六目标严格检查、82 项 Metal/1 项 CPU 参考绘制、23 项有条件 Linux CPU、正式 WebAssembly 五项新 Timeline 与浏览器 22 项像素，长审计结果及精确身份见 target/audits/native-animation-timeline-identity-20261008/verification.json | 完整所需平台/集成验收；严格核对测试清单、编译输入和范围，拒绝过期产物；有界像素/接口回归不能替代原版及物理平台验收 |
| 工作约束 | 新增逆向仅使用官方 IDA MCP，自建租约均已关闭；使用隔离数据，既有修改和归档保留 | 后续新增逆向/原生核验仅用官方 IDA MCP；相关实现与必要验证完成后提交到本地 Git，不推送、不上传 GitHub、不发布；保留既有修改，不覆盖正常玩家存档，不使用真实凭据或真实购买 |

每次关闭一项均应记录对应原生证据、实现、必要测试、实际运行条件和精确源码/产物身份。所有行都得到足够证明且没有必要未完成项，才允许将完整 goal 标为 complete。
