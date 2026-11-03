# 大规模数据模型验证

```powershell
python spikes/bulk-scale/probe.py
```

仅使用 Python 标准库。10 万、100 万条目及 1,024 条约 32K 字符的 UTF-16 路径分别在新进程执行，避免峰值内存被上一个案例污染。2 个数据生产线程经有界批次队列交给 SQLite WAL/FULL 写入者；检查记录不丢、失败计数、独立取消信号和设备额度模型。数据库保存在被忽略的 `.local/bulk-probe`，不触碰用户文件。

记录字节预算是编码路径与固定字段的负载计费；Python 对象、SQLite 和进程开销另由 Windows 的峰值工作集/PrivateUsage 记录，不声称 Python 对象实际占用等于序列化大小。可见页最多 96 条，使用覆盖索引返回短显示字段，按需取完整路径。UI 唤醒模型只有一个待处理标记，常规快照最多 10 Hz。

输出 [规模证据](../../docs/evidence/bulk-scale-model.json)。`spool_seconds` 是数据生成、通道传递与数据库落盘时间，不是文件复制速度。页延迟为已建立数据库的暖读样本；不代表冷目录首屏。线程/设备额度属于调度模型，不是实际 I/O 工作者或 Windows Shell 实现。GPUI 绘制、UIA、Shell/OLE 内存、磁盘争用、取消当前系统操作及崩溃恢复必须在应用完成后另测。

同库比较短字段投影与完整路径页读取：

```powershell
python spikes/bulk-scale/probe.py --worker 1024 --long-paths --compare-full-paths
```

对照会额外读取完整长路径，所以该进程的峰值内存包含基线成本，不能当作仅使用短字段方案的内存。单独结果见 [同库对照](../../docs/evidence/long-path-page-comparison.json)。早期 `bulk-scale-model-full-path-baseline.json` 为探索记录，不同 schema 的落盘耗时不作严格比较。
