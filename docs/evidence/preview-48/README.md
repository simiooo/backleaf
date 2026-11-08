# preview.48 · 性能优化 1–4

源码：`57d2cb9c96c880db8e922adece8c4649d8f460d6`（commit `57d2cb9`，2026-11-08）
候选版本：`0.1.0-preview.48`

- Release：[test-v0.1.0-preview.48-57d2cb9c](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.48-57d2cb9c)，pre-release，非 draft。
- Package CI：[42449997468](https://github.com/simiooo/backleaf/actions/runs/42449997468)，8m23s，fmt、严格 Clippy、141 项 native、52 项 headless、installer 回归、打包 smoke、发布全部成功。
- 资产 SHA-256：

| 资产 | SHA-256 |
| --- | --- |
| `Backleaf-0.1.0-preview.48-57d2cb9c-win-x64.zip` | `ec85bc505c434c143ea35a3ba3cd931ccc8a8dc32dda56cf495c1b471cf66db7` |
| `Backleaf-v46.fig` | `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179` |
| `build.json` | `576a53c0cfda8dd01d3686f75b8b47e2d66f47f4e035659e01e6917da8da83bd` |
| `SHA256SUMS.txt` | `e46527c5079652bfd13dfa862072d3b13fc2022822e07f9a24dec047f650fba2` |

- 打包记录（[build.json](build.json)）：EXE `52af68cb…`，headless 52 项，CLI smoke 通过。

## 1. 索引前台限速

窗口可见且系统空闲时：

- 每批最多 8 个目录，每处理一个目录休眠 12ms；后台/隐藏时恢复 32 个目录/批、1ms 工作休眠、无逐目录休眠。
- 用户有输入时保留原低速预算（1 目录/20ms 窗口 +80ms 休眠）。
- 通知批次仍即时处理，不等待爬取预算。
- UI 在创建、可见性与激活观察器里同步 foreground 状态。

限制：没有单独做“前台限速 vs 全速”的 CPU 基准；限速效果取决于索引是否正处于目录爬取阶段，通知/DB 工作不受该预算控制。

## 2. 排除可再生构建目录

扫描到以下目录时不再进入索引，并清掉历史子树（目录仍在文件列表中可见）：

- 有父目录 `Cargo.toml` 时的 `target`；
- 有 `package.json` / `pnpm-lock.yaml` / `yarn.lock` 时的 `node_modules`；
- 有 `pyproject.toml` / `requirements.txt` / `Pipfile` 时的 `.venv` / `venv`；
- 无条件排除 `.git`、`.svn`、`.hg`、`.vs`、`.idea`、`__pycache__`、`.pytest_cache`、`.next`、`.nuxt`。

验证：`artifact_directories_require_their_project_markers`、`index_excludes_generated_artifact_directories`、`purge_artifact_subtree_removes_legacy_descendant_rows` 三个 native 测试通过。

## 3. 后台 worker 懒加载

原生图标 worker 与图片缩略图 worker/通知 worker 改为首次真正请求图片时才创建。安装版同机各 2 次启动：

| 指标 | preview.47 | preview.48 |
| --- | ---: | ---: |
| 空闲线程 | 55 | **52** |
| 句柄 | ~1216 | **~1209** |
| 工作集 | 70.3 MB | **67.3 MB** |

现有图片缩略图/图标测试与 headless 套件通过。

## 4. 内容搜索并行扫描

- 目录遍历仍单线程有序；文件读取、文本嗅探、匹配放到 `available_parallelism` 限制的 2–4 个 worker。
- 文件/目录/字节/结果上限仍由原子计数控制；结果上限新增 `parallel_scan_honors_the_result_limit_and_finishes_once` 单元测试。
- 命中批次由发现它的 worker 直接发送，全部 worker 结束后协调线程发送唯一 done。
- 仍是会话内文本缓存，不是持久全文索引；首次查询仍需读文本。

## 验证

- native 141 项、headless 52 项、fmt、严格 Clippy、release 构建通过。
- 发布 ZIP/EXE 匿名下载并校验 SHA-256；打包 EXE `--session-status-file` 退出码 0。
- 机器测量数据见 [measurements.json](measurements.json)。

限制：前台限速缺少独立 CPU 基准；构建产物旧条目要等扫描到对应目录才清除；内容搜索并行顺序不再稳定（结果按到达顺序追加），容量上限与截断语义保持不变。

其它内容沿用 preview.47（标题栏三点菜单/设为默认）、preview.46（GPU 并发实验）、preview.45（侧栏家目录名）、preview.44（字体缓存/启动追踪）、preview.43（内容搜索浮层/查询优化）、preview.42（中英界面）及更早 preview。
