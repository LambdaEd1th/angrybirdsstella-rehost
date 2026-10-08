# Clip、Timeline 与事件目标的独立身份

2026-10-08。完整 goal 保持 ACTIVE，尚未完成。新增逆向仅使用官方 IDA
MCP，并复用已有 IDA 证据。记录在
`target/audits/native-animation-timeline-identity-20261008/`。

## 原生依据与实现理由

| 原生路径 | 已核实行为 | 对应实现 |
| --- | --- | --- |
| `100414BD4`、`1004148B4`、`10040C930` | 按字符串 map 的顺序加载 Clip、目标、usage；Clip 追加独立 Timeline | `AnimationAction.clips` 与各 Clip 的 `targets`，不合并重复轨道 |
| `10040E5E4` | Control 的 +64 向量保留每个 Clip，结束时间取各 Clip 最大值 | 保留 Clip 的顺序与身份；既有 float32 时间/结束语义保持 |
| `10041D918`、`10040E7B0` | 启动逐个访问 Clip 的 Timeline；State 按 EntityTarget 和 Timeline 指针查找，不按 usage 合并 | group 内 State 由 action、Clip 索引、所在目标和 usage 区分 |
| `10041D9D8` | usage 共享 ApplyCallback；每个独立 State 先稳定移除再追加，最后 State 取得优先级 | 重启逐轨道重新附加，不压缩成同一个 action 的单个 State |
| `10041E0C0` | 按 Control 保留的 State 创建顺序逐个移除；只在最后 State 离开后 swap-remove 空组 | 停止逐 Clip/目标/usage 移除，保留余下组的 setter 顺序 |
| `100410A18`、`1004111A4` | Control 先重置到零再 force 2，逐目标附加并 mode 0 应用；Animation::apply 访问 targets 后复制 Control previous time | 保留既有 start、hidden tick、force 4 与延后派发规则 |

本轮七个新捕获指令范围与原始 ARM64 文件逐字节相同。原文件 SHA-256 为
`ba45c91db09807fefa8207df0925b84250933c1c036c4846a8a71c0978749bbb`。
六份此前证据及原生字节索引复制后保存 SHA；新增 caller 查询作为调用定位
材料，未把没有字节捕获的范围算入七个新核验范围。自建 IDA 租约已释放，
保留用户的 GUI 数据库，未补丁、重命名或保存数据库。

事件轨道现归属于各 Clip 的各个目标，保留空字符串重置键。普通更新仅比较
被选中的 Timeline 自身键索引，不再把其他目标或早期 Clip 的键拼入一条
全局事件轨道。直接 Sprite 的加载快照遍历所有 Clip，避免后期同名目标
覆盖早期轨道的资源种类与所有权。

结构上，action/Clip/目标负责不可变轨道数据，`target_groups` 负责独立
State 与优先级，`apply` 负责采样和组件写入。借用迭代器代替克隆 usage
map，统一 JSON 和 typed fixture 的轨道遍历。既有 fixture 仅调整数据构造
和字段访问，保留原断言；没有新增 allow、ignore 或跳过必要检查。
此项没有性能测量，也不声明帧、CPU、GPU 或内存改善。

## 行为差异与验证

五个安装 JSON/Lua 回归检查不同目标事件、后期 Clip 的屏蔽、空键仅屏蔽
自己的目标、停止/重启的恢复，以及重复 State 移除后的矩阵 setter 顺序。
最后一项独立验证：Alpha 的第一 State 离开时组仍存活；Translation 移除
先换入 Rotation，第二 Alpha 移除再换入 Scale，所以最后应用 Scale 后
Rotation，缩放恢复为单位值。旧合并模型得到相反次序与 `[2,3]`。

在独立 RED target 中，修复前生产源码与最终相同的五个 fixture 产生
5 个真实行为失败，无编译/JSON 错误。第一次 RED 构建与缓存克隆重叠，
该结果明确排除；克隆终态后仅清理自建 RED target 的 stella-script crate
并重编，保存实际程序、清单、源码和失败日志。早期遗漏的 19 个 typed
fixture 字段访问导致的编译错误保留；新 restart 断言曾错误期待旧时间
事件，依据 `100410A18` 改为验证重置后全部六个初始回调。

最终编译输入为 1,055 文件，SHA-256
`725e25b3aaa4deea629f6cde10c3e37b391a377a3cf23cd9b2f4621347c2ecac`。
此后的修改仅为证据文档和本地 Git 记录。当前输入完成：

- macOS 工作区 all-targets/all-features：1,968 passed、0 failed、2 原有默认
  ignored；11 个保存程序独立列出 1,970 项，包含既有 82 项实际 Metal
  文件图片检查与 1 项 CPU 参考绘制。
- macOS、Web/Emscripten、Linux ARM64/x86_64、Windows MSVC ARM64/x86_64
  六目标严格 Clippy `-D warnings` 全部重新执行通过。
- Linux ARM64 实际通过 5 Timeline、10 矩阵、2 附件和 6 Poppy CPU 回归。
  真实 Helvetica 仅安装在私有无网络容器；不证明物理平台或一般字体可用性。
- 新正式 WebAssembly 的 Node 引擎、帧、隔离存档、11 语言及资源/附件/
  矩阵检查通过；五项新 Timeline 安装 Lua 回归通过。私有 Chrome profile
  WebGL2/ANGLE Metal 读取 22 项像素检查通过。词法断言的必定失败对照
  确认探针实际执行，未改变 shipped release 的全局 assert。
- 两项默认忽略长审计显式通过：六次 BirdRun 静置约 71.2 秒，完整 131
  章节入口、180 帧静置与两章重启约 298.1 秒；没有自动求解或筛掉关卡。
- 保存的当前 desktop release SHA-256 为
  `c7458e8a34283c1be9041103e18afaa0e74cadfb4cef6dfdaa0a38a5142225ba`。
  2,364 个资源文件的私有逐字节副本完成 1,000 帧 L18/Pink Shades Poppy
  瞄准场景；保留 84 个未定义全局查询，实际回退为零。PNG 和谓词失败对照
  仅证明自身回归，不证明原版完整视觉或物理窗口输入、音频。

## 保留的未完成范围

本项关闭合并 Clip/事件目标导致的上述差异，不能据此宣布完整动画或完整
目标完成。严格 JSON type/非法数据错误、多 AnimationComponent 与完整
EntityTarget 注册/遍历及回调身份、所有 State/资源寿命、完整矩阵/Z/cache
边界仍需逐项核实。随附 216 文件/429 actions 的单 Clip 盘点不能删除
这些原生能力。原先报告的 Poppy 鼓面技能与冻结表情仍未稳定复现。

全部游戏流程、原版画面/交互、物理平台、服务、性能优化与结构整理要求
继续保留。原有未提交内容先按已验证的原字节整理为本地基线提交，本项
作为独立本地提交；19 个归档自动通关文件原样保留且不参与验收。
不推送、不上传 GitHub、不发布，未使用正常玩家存档、真实凭据或真实购买。
