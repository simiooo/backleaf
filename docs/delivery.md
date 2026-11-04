# Delivery index

Scope: SIM-919. Constraints: SIM-934. One engineering unit is active at a time.

| Queue | Issue | Deliverable / gate |
| --- | --- | --- |
| accepted | SIM-921 | User accepted v6 as the implementation baseline on 2026-11-04 |
| accepted | SIM-948 | v6 feedback incorporated and accepted as baseline |
| focus_issue | SIM-965 | Subtle toolbar elevation, spacing and reversible search motion; preview.7 |
| ready_resume | SIM-963 | Icon implementation and validation bundled with 965; product acceptance pending |
| review_queue | SIM-953 | v9 selection/state redesign, rubber-band and keyboard selection; installed-window smoke and 13 native runs passed; preview.2 published |
| ready_resume | SIM-954 | v13 keyboard / focus / rename-blur and navigation feedback, preview.6; broader Explorer and retry gates remain |
| review_queue | SIM-951 | Native GPUI preview deployed and published; unlocked performance and installed-window smoke passed |
| review_queue | SIM-935 | Technical plan only; native rendering now progresses through SIM-951, broader Windows validation remains open |
| ready_resume | SIM-920 | v12 local recursive filename search committed at 2055d260; keyboard fix proceeds through 954; saved views remain |
| ready_queue after SIM-965 | SIM-967 | External directory notifications, batched changes and activation reconciliation; current preview requires F5 |
| candidate after dependencies | SIM-922 | SQLite index, refresh, short Chinese queries, benchmark and coverage |
| candidate after dependencies | SIM-937 | Directory/search restore and failure recovery |
| cross-cutting | SIM-938 | Independent windows, stale-result rejection, coordinated shared state |

`in_review` means engineering evidence is ready for product acceptance. It does not imply that real users have validated usability or that untested hardware, IMEs or filesystems have passed.

SIM-953 engineering delivery is in product review and its claim is released. SIM-965 is claimed again for the sidebar focus feedback and final preview.7 delivery; the toolbar/search-motion work and SIM-963 dependency remain uncommitted. SIM-967 (external directory changes and activation reconciliation) is the next independent unit after 965 delivery. SIM-954 has a released safe checkpoint for the delivered v13 increment. SIM-920 has a safe source checkpoint and released claim. Saved views, persistent indexing and restore remain incomplete under SIM-920 / SIM-922 / SIM-937. SIM-951 is in the product review queue. The [file operation contract](file-operations.md) records current scope; v9 selection rules remain in visual-system-v9.md.

2026-11-03: SIM-921 v5.1 adds eight scale states to the compact baseline, for a 38-page editable master. The design covers million-item selections, a bounded 24-task queue and ten thousand failures. The isolated metadata/queue probe verifies 100,000 and 1,000,000 records, UTF-16 long paths, retained results, bounded payloads and paged reads. See [performance contract and evidence](bulk-performance.md).

Historical design/model evidence is retained. The native GPUI binary uses a pinned source submodule and Cargo.lock. Source checks pass. Final unlocked installed-executable measurements cover generated metadata up to ten million items and an actual 100,001-entry NTFS fixture. Earlier reports without WTS state remain diagnostic only. See [native evidence](evidence/native/README.md).

SIM-951 implements virtual grid rows, a bounded directory/label cache, Unicode ellipsis/full details, one window animation coordinator, native DWM chrome and WTS lock gating. The [acceptance checkpoint](native-resume-checklist.md) and immutable binary/report hashes identify the delivered unit. Search, Shell writes, thumbnails, full IME/drag/drop/NVDA coverage and restore remain separate work.

Preview `0.1.0-preview.1`, source `edfcfc674d8a309f5e2e780c870f0269530208ef`, was extracted into isolated Windows staging at 2026-11-04 10:51:39 +08:00 and published at 11:12:48 +08:00. Installed hash, CLI startup, UI smoke, final measurements and uploaded/downloaded ZIP bytes passed. The [private staging prerelease](https://github.com/simiooo/backleaf/releases/tag/native-v0.1.0-preview.1-20261104) is the acceptance entry. [Deployment evidence](evidence/native/staging-candidate.json) records exact hashes and times. Windows was unlocked by the user before the final visible checks; the prior locked-state launch deferral no longer blocks this unit.

Preview `0.1.0-preview.2`, source `d28e5af81f787fb8257cdce1d484caac667e30c9`, was extracted into isolated Windows staging at 2026-11-04 13:40:42 +08:00 and published at 14:19:58 +08:00. The [private staging prerelease](https://github.com/simiooo/backleaf/releases/tag/native-v0.1.0-preview.2-20261104) includes the executable, editable 52-page OpenPencil v9 master and SHA256 sums. Installed-window smoke, 13 final native runs, release checks and GitHub asset digests passed. [Evidence](evidence/native-selection/README.md) preserves exact report bytes, including committed-blob verification. Remote ZIP bytes were not downloaded or executed for this version. Context menus, Shell writes, search and restore are pending; v9 aesthetic acceptance remains with the product owner.

2026-11-04: v13 input feedback is delivered as [preview.6](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.6-ea34992b), source ea34992b, published at 20:21:43 +08:00. Local staging deployed at 20:14 +08:00 with visible keyboard/IME checks; CI 41827367814 completed 30 tests, lint/build and unpacked CLI smoke. Downloaded artifact bytes, embedded design and Logo match metadata and GitHub digests. [Evidence](evidence/input-feedback/README.md) separates local interactive and CI executable hashes. Unconfirmed rename compositions may require explicit candidate confirmation; the original value is preserved. Broader SIM-954 and saved-search/index/restore gates remain open.
