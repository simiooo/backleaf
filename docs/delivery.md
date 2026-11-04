# Delivery index

Scope: SIM-919. Constraints: SIM-934. One engineering unit is active at a time.

| Queue | Issue | Deliverable / gate |
| --- | --- | --- |
| accepted | SIM-921 | User accepted v6 as the implementation baseline on 2026-11-04 |
| accepted | SIM-948 | v6 feedback incorporated and accepted as baseline |
| review_queue | SIM-951 | Native GPUI preview deployed and published; unlocked performance and installed-window smoke passed |
| review_queue | SIM-935 | Technical plan only; native rendering now progresses through SIM-951, broader Windows validation remains open |
| candidate after dependencies | SIM-920 | Search semantics and one saved-search/folder quick access list |
| candidate after dependencies | SIM-922 | SQLite index, refresh, short Chinese queries, benchmark and coverage |
| candidate after dependencies | SIM-937 | Directory/search restore and failure recovery |
| cross-cutting | SIM-938 | Independent windows, stale-result rejection, coordinated shared state |

`in_review` means engineering evidence is ready for product acceptance. It does not imply that real users have validated usability or that untested hardware, IMEs or filesystems have passed.

At this handoff, focus_issue and focus_operation are empty and SIM-951's claim is released for product acceptance. The desktop gate is resolved. ready_queue is empty until the next independent unit is explicitly qualified against its dependencies; the candidate sequence alone is not a readiness decision.

2026-11-03: SIM-921 v5.1 adds eight scale states to the compact baseline, for a 38-page editable master. The design covers million-item selections, a bounded 24-task queue and ten thousand failures. The isolated metadata/queue probe verifies 100,000 and 1,000,000 records, UTF-16 long paths, retained results, bounded payloads and paged reads. See [performance contract and evidence](bulk-performance.md).

Historical design/model evidence is retained. The native GPUI binary uses a pinned source submodule and Cargo.lock. Source checks pass. Final unlocked installed-executable measurements cover generated metadata up to ten million items and an actual 100,001-entry NTFS fixture. Earlier reports without WTS state remain diagnostic only. See [native evidence](evidence/native/README.md).

SIM-951 implements virtual grid rows, a bounded directory/label cache, Unicode ellipsis/full details, one window animation coordinator, native DWM chrome and WTS lock gating. The [acceptance checkpoint](native-resume-checklist.md) and immutable binary/report hashes identify the delivered unit. Search, Shell writes, thumbnails, full IME/drag/drop/NVDA coverage and restore remain separate work.

Preview `0.1.0-preview.1`, source `edfcfc674d8a309f5e2e780c870f0269530208ef`, was extracted into isolated Windows staging at 2026-11-04 10:51:39 +08:00 and published at 11:12:48 +08:00. Installed hash, CLI startup, UI smoke, final measurements and uploaded/downloaded ZIP bytes passed. The [private staging prerelease](https://github.com/simiooo/backleaf/releases/tag/native-v0.1.0-preview.1-20261104) is the acceptance entry. [Deployment evidence](evidence/native/staging-candidate.json) records exact hashes and times. Windows was unlocked by the user before the final visible checks; the prior locked-state launch deferral no longer blocks this unit.
