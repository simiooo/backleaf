# Delivery index

Scope: SIM-919. Constraints: SIM-934. One engineering unit is active at a time.

| Queue | Issue | Deliverable / gate |
| --- | --- | --- |
| accepted | SIM-921 | User accepted v6 as the implementation baseline on 2026-11-04 |
| accepted | SIM-948 | v6 feedback incorporated and accepted as baseline |
| focus_issue | SIM-951 | Native GPUI render candidate, packaging and staging preparation; final unlocked UI/performance smoke pending |
| review_queue | SIM-935 | Technical plan only; native rendering now progresses through SIM-951, broader Windows validation remains open |
| candidate after dependencies | SIM-920 | Search semantics and one saved-search/folder quick access list |
| candidate after dependencies | SIM-922 | SQLite index, refresh, short Chinese queries, benchmark and coverage |
| candidate after dependencies | SIM-937 | Directory/search restore and failure recovery |
| cross-cutting | SIM-938 | Independent windows, stale-result rejection, coordinated shared state |

`in_review` means engineering evidence is ready for product acceptance. It does not imply that real users have validated usability or that untested hardware, IMEs or filesystems have passed.

Current focus_operation: native candidate packaging/staging. The external gate is the locked Windows desktop: the user has been asked to unlock it. No ready_queue entry is inferred from the candidate sequence. SIM-951 cannot enter engineering review until its final executable is tested on an unlocked desktop and its installed window passes smoke checks.

2026-11-03: SIM-921 v5.1 adds eight scale states to the compact baseline, for a 38-page editable master. The design covers million-item selections, a bounded 24-task queue and ten thousand failures. The isolated metadata/queue probe verifies 100,000 and 1,000,000 records, UTF-16 long paths, retained results, bounded payloads and paged reads. See [performance contract and evidence](bulk-performance.md).

Historical design/model evidence is retained. A native GPUI binary is now built with a pinned GPUI source submodule and Cargo.lock. Source checks pass; runtime diagnostics cover generated metadata up to ten million items and an actual 100,001-entry NTFS fixture. Late discovery of a locked desktop prevents treating the untracked-session timing set as final visible acceptance. See [native evidence status](evidence/native/README.md).

SIM-951 implements virtual grid rows, a bounded directory/label cache, Unicode ellipsis/full details, one window animation coordinator, native DWM chrome and WTS lock gating. The immutable source/build evidence and [resume checklist](native-resume-checklist.md) distinguish implementation, diagnostic measurements and the remaining staging gate. This does not claim search, Shell write operations, full IME/drag/drop/NVDA coverage or restore have shipped.
