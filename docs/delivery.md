# Delivery index

Scope: SIM-919. Constraints: SIM-934. One engineering unit is active at a time.

| Queue | Issue | Deliverable / gate |
| --- | --- | --- |
| review_queue | SIM-921 | Design baseline pending product acceptance; latest artifact v6 follows SIM-948 feedback |
| review_queue after handoff | SIM-948 | v6 spacing, aligned minimal task panel, long text, flow states and isolated viewport model |
| review_queue | SIM-935 | Technical-plan issue is in_review; actual native application and Windows validation remain unimplemented |
| candidate after dependencies | SIM-920 | Search semantics and one saved-search/folder quick access list |
| candidate after dependencies | SIM-922 | SQLite index, refresh, short Chinese queries, benchmark and coverage |
| candidate after dependencies | SIM-937 | Directory/search restore and failure recovery |
| cross-cutting | SIM-938 | Independent windows, stale-result rejection, coordinated shared state |

`in_review` means engineering evidence is ready for product acceptance. It does not imply that real users have validated usability or that untested hardware, IMEs or filesystems have passed.

After the v6 design/model feedback handoff, focus_issue and focus_operation are empty and its claim is released; product waiting is not active development. No ready application unit has been newly verified in this checkpoint; the candidate sequence is not a ready_queue or an active claim. No external blocked_queue condition is recorded. Design-only review is separate from application engineering delivery: GPUI implementation must still complete its build, actual staging deployment and smoke checks before that application unit enters review.

2026-11-03: SIM-921 v5.1 adds eight scale states to the compact baseline, for a 38-page editable master. The design covers million-item selections, a bounded 24-task queue and ten thousand failures. The isolated metadata/queue probe verifies 100,000 and 1,000,000 records, UTF-16 long paths, retained results, bounded payloads and paged reads. See [performance contract and evidence](bulk-performance.md).

This is a design/model artifact checkpoint; no GPUI binary, application staging installation or application smoke test exists yet. Design publication and model measurements do not complete application delivery. The remaining application sequence above depends on design acceptance and the actual Windows validation gate; it is not a claim that those issues have been implemented.

SIM-948 handles the new product feedback as one unit: relaxed vertical spacing, shared visual axis, filename-only state presentation, minimal overview, complete long-text access and bounded visible rendering. It does not claim native application implementation. See [v6 contract](viewport-and-text.md).
