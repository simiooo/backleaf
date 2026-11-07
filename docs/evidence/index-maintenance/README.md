# 索引维护基准与 profile

2026-11-06。目标：量化“构建、无变化重扫、通知增量”的成本来源，并用同一套基准验证后续优化。原始数据：[maintenance-profile-20k.json](maintenance-profile-20k.json)。

## 方法

- 20,000 个真实文件、200 个目录（`doc-XXXXXX.txt`），release 构建、OS 文件缓存已热、单机。
- 动作：
  1. 仅目录枚举：递归 `read_dir` 计数，不采集元数据；
  2. 身份采集样本：200 个文件逐个 `identity::capture`（CreateFile + FileId 查询），再估算全量；
  3. 全量构建：空状态目录上 `build_test_index`；
  4. 无变化启动重扫：用真实 `Service` 打开已有索引并等待构建完成（模拟应用启动/接手）；
  5. 无变化显式重扫：发送 `Action::Rescan` 后再等待完成；
  6. 通知式增量：500 个文件，分别测“逐条一个 IMMEDIATE 事务”和“128 条一个批事务”。
- profile 是 `#[cfg(test)]` 计数器，包住 reconcile 的枚举/身份/upsert 与增量路径的 root 查询/身份/upsert/commit（[index.rs](src/index.rs)、[index_bulk.rs](src/index_bulk.rs)）；生产构建不含这些分支。
- 复现：`./scripts/native/index-maintenance-benchmark.ps1 -Files 20000 -Dirs 200`。

## 基线（优化前）

| 阶段 | 总耗时 | 分解 |
| --- | ---: | --- |
| 仅目录枚举 | 58.8 ms | 2.9 µs/项 |
| 身份采集（200 样本） | 26.4 ms | 0.132 ms/文件；估算全量约 2.6 s |
| 全量构建 | **23,431.7 ms** | 枚举 51.9、身份 9,368.4、upsert 11,135.9、其余 2,875.6 ms；索引 29,937,664 B、WAL 5,117,072 B |
| 无变化启动重扫 | **24,917.2 ms** | 枚举 52.4、身份 7,096.7、upsert 5,676.7；其余约 **12 s** 是 writer 调度 |
| 无变化显式重扫 | **24,938.3 ms** | 枚举 52.1、身份 6,995.0、upsert 5,666.3；其余约 **12 s** |
| 通知式增量（500 文件） | 4,767.2 ms | 9.534 ms/条；WAL 约 8.3 KB/条。**该数字受基准路径缺陷影响，修正值见末节** |

## 优化一：writer 调度预算化

主循环原来每次只处理一个 frontier 目录再固定 `sleep 50 ms`，200 个目录形成约 12 s 固定调度开销。改为时间/数量预算连续处理（最多 32 目录或 500 ms）、目录间排空通知、空闲才 50 ms 退避。数据：[maintenance-profile-20k-scheduled.json](maintenance-profile-20k-scheduled.json)。

| 阶段 | 修复前 | 修复后 |
| --- | ---: | ---: |
| 无变化启动重扫 | 24,917.2 ms | **15,740.7 ms**（−9.2 s） |
| 无变化显式重扫 | 24,938.3 ms | **16,268.5 ms**（−8.7 s） |
| 构建 | 23,431.7 ms | 25,221.5 ms（同量级；构建不经该调度器） |

剩余重扫成本几乎全是实际工作：身份 7.78 s + upsert 6.05 s。

## 优化二：批量目录枚举

reconcile 改为“一次目录句柄 + `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo / FileIdBothDirectoryInfo)` 批量取身份/时间/大小/属性”，不可用时回退 `read_dir` + 逐文件 `identity::capture`。数据：[maintenance-profile-20k-bulk.json](maintenance-profile-20k-bulk.json)。

| 阶段 | 基线 | 批量枚举后 |
| --- | ---: | ---: |
| 全量构建 | 23,431.7 ms | **13,236.3 ms** |
| 无变化启动重扫 | 24,917.2 ms | **6,702.6 ms** |
| 无变化显式重扫 | 24,938.3 ms | **6,271.9 ms** |
| 构建 profile · 身份采集 | 9,368.4 ms | **0.0 ms**（并入批量枚举） |
| 构建 profile · upsert | 11,135.9 ms | 10,294.5 ms |
| 重扫 profile · upsert | 5,676.7 ms | 4,340.2 ms |

逐文件 `CreateFile` 成本被消除；剩余成本是 upsert 行写入与事务。

## 优化三：keyed 目录 diff

reconcile 批量枚举目录孩子后，与已索引行按 path/identity/大小/修改时间/显示名逐项比对；完全一致时**不写任何 entry 行**，只保留必须访问的子目录 frontier；有差异回落到 epoch 全量 upsert。超过 65,536 行/目录或存在 reparse/云属性时跳过 fast path。数据：[maintenance-profile-20k-keyed.json](maintenance-profile-20k-keyed.json)。

| 阶段 | 基线 | 批量枚举后 | keyed diff 后 |
| --- | ---: | ---: | ---: |
| 全量构建 | 23,431.7 ms | 13,236.3 ms | **13,167.1 ms** |
| 无变化启动重扫 | 24,917.2 ms | 6,702.6 ms | **1,137.3 ms** |
| 无变化显式重扫 | 24,938.3 ms | 6,271.9 ms | **1,013.0 ms** |
| 重扫 profile · upsert | 5,676.7 ms | 4,340.2 ms | **0.0 ms** |
| 重扫 profile · 身份采集 | 7,096.7 ms | 0.0 ms | 0.0 ms |
| 构建 profile · upsert | 11,135.9 ms | 10,294.5 ms | 10,430.5 ms |

相对原始基线：构建 −44%，无变化重扫 **−95%**（24.9 s → 1.1 s）。

## 短查询 2-gram 扩展的维护代价（本轮）

为支持任意 2 字符查询，`grams` 增加相邻 2 字符对（CJK 单字保留）；旧库由 writer 启动时回填并置 `grams_v2`。同一 20k 树复测：[maintenance-profile-20k-pairgrams.json](maintenance-profile-20k-pairgrams.json)。

| 指标 | 扩展前 | 扩展后 |
| --- | ---: | ---: |
| 全量构建 | 13.2 s | **16.8 s** |
| 索引库 | 29.5 MB | **40.6 MB** |
| 逐条增量 | 3.0 ms/条 | 4.3 ms/条 |
| 128 条批事务 | 2.6 ms/条 | 3.2 ms/条 |
| 无变化重扫 | 1.1 s | 1.1 s（不受影响） |

原因：每个新条目多写约 1 倍 grams 行。另外实测过“多行 VALUES 批量插入 grams”，构建反而从 16.8 s 升到 26.7 s（`INSERT OR IGNORE` 多行 + 外键探测成本），因此保留单行 prepared insert。构建/索引成本需要在“2 字符查询 2–3.7× 加速”和后续空闲调度之间权衡；短查询查询侧证据见[搜索短查询证据](../search-auto-index/README.md)。

## 步骤②：空闲调度与低优先级（本轮）

- [src/idle.rs](src/idle.rs)：用 `GetLastInputInfo`/`GetTickCount` 读取系统级输入空闲时长。空闲 ≥15 s 时 frontier 全速（32 目录 / 500 ms 预算、1 ms 让步）；有用户输入时限速为 1 目录 / 20 ms 预算 + 80 ms 让步，通知批次仍每轮先排空，保证增量不因限速积压。
- 索引 writer 线程启动时进入 Windows background priority（最低调度 + 后台 I/O 优先级），构建/重扫不与前台操作抢资源。
- 基准通过 `#[cfg(test)] force_idle` 固定为空闲状态，保持可重复；[maintenance-profile-20k-idle-policy.json](maintenance-profile-20k-idle-policy.json) 中无变化重扫 **1.17 s / 1.02 s**、构建 16.5 s、批增量 3.04 ms/条，与加空闲策略前一致。
- 活跃限速路径没有用人工输入去跑基准（结果不可重复）；只做策略单测（active 预算 < idle，阈值边界）与代码审查。下一步继续按需自动根 + LRU。

## 步骤③：按需自动根 + LRU（本轮）

- 默认搜索无覆盖时，Worker 发 `IndexCandidate`，App 转给 writer `AddOpportunistic`：目录登记为机会根（`roots.kind=1`），不显示任何 UI 状态；驱动器根、临时目录、`SystemRoot`/Program Files/ProgramData 被拒绝。
- 机会根上限 8、总根上限 64；超限按 `roots.used` 淘汰未固定的机会根。索引搜索命中根时发 `IndexUsed` 更新 `used`。
- 保存视图（`Destination::View`）的根路径写入 `pins` 表；被 pin 文件夹覆盖的根不会被 LRU 淘汰。显式 `Action::Add` 的根是永久根，且会把覆盖它的机会根升级为永久根。
- 旧库自动补 `roots.kind/used` 列与 `pins` 表，无需重建；新增测试覆盖 LRU/pin/驱动器拒绝与旧库迁移。
- 维护基准在改动后复测：[maintenance-profile-20k-lru.json](maintenance-profile-20k-lru.json)：构建 17.77 s、无变化重扫 **1.23 s**、索引 40.6 MB、重扫 upsert 0——与步骤②/短查询 grams 后的水平一致，无回归。

## 10 万文件规模验证

同一套真实文件基准扩大到 100,000 文件 / 1,000 目录（`-Files 100000 -Dirs 1000`），原始数据：[maintenance-profile-100k.json](maintenance-profile-100k.json)。

| 阶段 | 100k 结果 | 与 20k 对照 |
| --- | ---: | --- |
| 目录枚举 | 411.4 ms（4.1 µs/项） | 线性 |
| 全量构建 | **93.25 s**（0.93 ms/文件） | 0.82–0.96 s/万，近似线性 |
| 索引库 | **205.5 MB**（约 2.0 KB/文件） | 与 20k 的 40.6 MB 同量级比例 |
| 无变化启动重扫 | **3.24 s** | 1.1 s → 3.2 s，近似线性 |
| 无变化显式重扫 | **3.11 s** | 同上 |
| 逐条增量 | 5.32 ms/条 | 4.3 ms/条 |
| 128 条批事务 | **2.72 ms/条** | 3.2 ms/条 |

结论：keyed diff、批量枚举、批事务在 10 万规模保持线性/次线性，无变化重扫仍为秒级；构建 93 s 和 205 MB 索引进一步说明步骤 ② 的空闲/低优先级调度必要（本轮已加）。

## 优化四：通知批事务 + 改名配对

到 keyed diff 为止的“通知式增量”数字有基准缺陷：脚本用未 canonicalize 的临时目录拼增量路径，而生产 watcher 发来的是已 canonical 的路径；于是每条会在增量内部多触发一次 `path.canonicalize` 加一次 `erase_path`，把单条成本放大到 8.9–9.5 ms。修正为 canonical 路径后（[maintenance-profile-20k-batched.json](maintenance-profile-20k-batched.json)）：

| 增量路径 | 每条 | 500 条总耗时 | WAL 增长 | 说明 |
| --- | ---: | ---: | ---: | --- |
| 逐条事务 | 3.01 ms | 1,503.0 ms | 4,210,672 B | 每变更一个 IMMEDIATE 事务 |
| 128 条批事务 | **2.57 ms** | **1,285.0 ms** | **3,048,800 B** | 每批一次提交；WAL −28%，提交 555→330 ms |

改造：
- [src/index_watch.rs](src/index_watch.rs)：`Change` 携带 `from`，把 `RENAMED_OLD_NAME`/`RENAMED_NEW_NAME` 配成一条变更；旧行在同一事务内删除，不再单独发送 delete 通知。
- [src/index.rs](src/index.rs)：增量核心拆成 `incremental_in_txn`，Service 每轮把 mailbox 通知合并、按 `(root,path)` 去重后调用 `incremental_batch`，一批只 bump 一次 revision；单条 `incremental` 保留给测试/工具。
- 新增测试 `batched_incremental_applies_rename_and_create_in_one_transaction`。

## 结论与后续

1. 构建成本从 23.4 s 降到 13.2 s，剩余主要是新条目 upsert（10.4 s）与事务/其他（2.7 s）。
2. 无变化重扫从 24.9 s 降到 1.1 s，已接近“枚举 + 少量 frontier 写”。
3. 通知增量在真实 canonical 路径下逐条 3.0 ms，批事务 2.6 ms/条；WAL 降低 28%。
4. 搜索侧仍未使用这些能力：下一步“按父目录批量身份核验”（复用批量枚举）与短查询 n-gram 扩展/预检缓存。
5. USN/MFT 按用户要求暂不处理；仍保持普通权限、单写者、事务原子性、身份核验、通知溢出/启动全量校准。
