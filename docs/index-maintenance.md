# 索引维护：基准、瓶颈与优化路线

2026-11-06。维护侧基准（构建、无变化重扫、通知增量）与 profile 细节、原始数据见[证据](evidence/index-maintenance/README.md)。本节记录结论和已确认的实施顺序，暂不涉及 USN/MFT。

## 当前维护路径的成本（20,000 文件 / 200 目录）

| 阶段 | 耗时 | 主要构成 |
| --- | ---: | --- |
| 全量构建（基线） | 23.4 s | 目录枚举 0.05 s、逐文件身份采集 9.4 s、upsert/SQL+触发器 11.1 s、事务/其他 2.9 s |
| 全量构建（批量枚举 + keyed diff 后，扩展 grams 前） | **13.2 s / 29.5 MB** | 身份采集并入批量枚举（profile 0 ms）、upsert 10.4 s、事务/其他 2.7 s |
| 全量构建（短查询 2-gram 扩展后） | **16.8 s / 40.6 MB** | 新条目多写约 1 倍 grams 行，构建增加约 3.6 s、索引库 +38%；无变化重扫仍 1.1 s |
| 无变化重扫（基线） | 24.9 s | 身份 7.1 s、upsert 5.7 s、writer 调度约 12 s |
| 无变化重扫（全部维护优化后） | **1.1 s** | 目录级 keyed diff 跳过全部 entry 写入，只剩枚举 + frontier/status |
| 通知式增量（扩展 grams 后） | 逐条 4.3 ms/文件；批事务 3.2 ms/文件 | 新条目写入更多 grams，成本上升；批事务仍减少提交次数与 revision 写放大 |
| 仅目录枚举 | 58.8 ms（2.9 µs/项） | 说明枚举本身不是瓶颈 |

## 结论

1. **批量目录枚举**（`GetFileInformationByHandleEx` 的 `FileIdBothDirectoryInfo`/`FileIdExtdDirectoryInfo`，一次目录句柄取一批身份、时间、大小、属性）消掉构建/重扫中最大的一块逐文件 `CreateFile` 成本。**已修复（本轮）**：构建 23.4→13.2 s、无变化重扫 15.7→6.7 s；不可用时回退 `read_dir` + 逐文件身份采集。
2. **keyed 目录 diff + 目录级代次**：无变化重扫现在不再 UPDATE 每个 entry（epoch）。**已修复（本轮）**：目录级 keyed diff 比对 path/identity/大小/修改时间/显示名，完全一致时不写 entry 行；无变化重扫 6.7→1.1 s，重扫 upsert 4.3 s→0。超过 65,536 行/目录或存在 reparse/云属性时回落原 epoch 全量 upsert，保证正确性。
3. **writer 调度**：主循环一次只处理一个 frontier 目录、随后固定 50 ms 退避（[index.rs](../src/index.rs#L760-L771)），目录多时形成约 10 s 的固定开销。**已修复（上一轮）**：改为时间/数量预算连续处理、目录之间排空通知、空闲才 50 ms 退避；无变化重扫从 24.9 s 降到 15.7–16.3 s。证据见[维护证据](evidence/index-maintenance/README.md)。
4. **通知合并/批事务/改名配对**：**已实现（本轮）**：mailbox 按批处理并按 `(root,path)` 去重，128 条一个事务，每条只 bump 一次 revision；watcher 把 RENAMED_OLD/NEW 配对，旧行同事务删除。真实 canonical 路径下逐条 3.0 ms→批事务 2.6 ms/条，WAL −28%。
5. **搜索端批量核验 + 短查询扩展**：搜索候选/可见页已按父目录批量身份核验；`grams` 从“1–2 个 CJK 字”扩展为“任意 2 字符对 + CJK 单字”，旧库由 writer 启动时一次性回填并置 `grams_v2` 标记，读端在回填完成前仍走 `instr` 回退。代价是构建 13.2→16.8 s、索引 29.5→40.6 MB；收益是 2 字符查询相对旧 `instr` 路径首屏 2.19×、完成 2.78×，相对实时扫描 2.8–3.7×（见[搜索短查询证据](evidence/search-auto-index/search-short-grams-20k.json)）。同时结果缓存命中且索引 revision 未变时跳过选择性预检。

## 步骤②：空闲调度与低优先级（已实施）

- 新增 [src/idle.rs](../src/idle.rs)：`GetLastInputInfo` 检测系统输入空闲；writer 线程设为 Windows background priority。
- 空闲 ≥15 s：全速处理 frontier（32 目录/500 ms）；有用户输入：限速为 1 目录/20 ms + 80 ms 让步。通知批事务仍每轮先排空、始终即时应用，不会因为限速积压。
- 2 万项基准在固定空闲状态下复测：无变化重扫 1.17/1.02 s、构建 16.5 s、批增量 3.04 ms/条，与加策略前一致；10 万项构建 93 s、无变化重扫 3.2 s、索引 205 MB，进一步说明空闲/低优先级必要。证据见[维护证据](evidence/index-maintenance/README.md)。
- 活跃限速路径没有用人工输入跑基准（不可重复），由策略单测覆盖；下一步实现按需自动根与 LRU 淘汰。

## 步骤③：按需自动根 + LRU（已实施）

- 默认搜索在未覆盖目录执行时，Worker 发送 `IndexCandidate`；App 转成 `Action::AddOpportunistic`，writer 把该目录登记为**机会根**（`roots.kind=1`，不进入任何 UI 状态）。驱动器根（`C:\` 等）和临时目录被拒绝。
- 上限：机会根最多 8 个、总根最多 64 个；超出时按 `roots.used` LRU 淘汰**未固定**的机会根。索引搜索实际使用某个根时发送 `IndexUsed`，writer 更新 `roots.used`。
- 固定规则：保存视图（`Destination::View`）的 `view.root` 作为 pin 持久化到 `pins` 表；被 pin 文件夹覆盖的根不会被 LRU 淘汰。用户显式添加的根是永久根（`kind=0`），永不淘汰；显式添加若命中已有的机会根，会把它升级为永久根。
- 旧库启动时自动补 `roots.kind/used` 列和 `pins` 表；不需要重建索引。
- 测试：`opportunistic_roots_evict_lru_but_keep_pinned_roots`（LRU/pin/驱动器拒绝）、`old_database_gains_lru_columns_and_pin_table`（迁移）。维护基准在改动后复测无回归（见[维护证据](evidence/index-maintenance/README.md)）。

## 实施顺序（已确认）

1. ~~writer 调度预算化~~（已完成：重扫 −9 s；见证据）；
2. ~~批量目录枚举~~（已完成：构建 −43%、重扫 −57%；见证据）；
3. ~~keyed 目录 diff~~（已完成：无变化重扫 −83%（6.7→1.1 s）；见证据）；
4. ~~通知合并、批事务、改名配对~~（已完成：3.0→2.6 ms/条，WAL −28%；见证据）；
5. ~~搜索端按父目录批量身份核验~~（已完成：同目录 60 命中首屏 1.78×、完成 2.01×；见搜索证据）；
6. ~~短查询 n-gram 扩展、预检缓存~~（已完成：2 字符查询相对 instr 完成 2.78×；构建 +3.6 s、索引 +38%；见证据）；
7. （按用户要求暂缓）USN/MFT 或提升权限的辅助进程。
8. ~~步骤② 空闲调度与低优先级~~（已完成：空闲全速、活跃 1 目录/80 ms 限速，增量始终即时；证据见上）；
9. ~~步骤③ 按需自动根 + LRU~~（已完成：机会根 ≤8、总根 ≤64、保存视图 pin、驱动器/临时/系统目录拒绝；见证据）。

每一项都要保留现有正确性边界：单写者、事务原子性、身份核验、通知溢出/启动全量校准；并用维护侧基准复测构建、无变化重扫、批量更新与 WAL 增长。
