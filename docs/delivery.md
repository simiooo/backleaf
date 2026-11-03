# Delivery index

Scope: SIM-919. Constraints: SIM-934. One engineering unit is active at a time.

| Queue | Issue | Deliverable / gate |
| --- | --- | --- |
| focus | SIM-921 | Editable design baseline, key states, motion, artifact publication |
| ready | SIM-935 | Native GPUI Windows application and Shell integration, actual Windows checks |
| ready | SIM-920 | Search semantics and one saved-search/folder quick access list |
| ready | SIM-922 | SQLite index, refresh, short Chinese queries, benchmark and coverage |
| ready | SIM-937 | Directory/search restore and failure recovery |
| cross-cutting | SIM-938 | Independent windows, stale-result rejection, coordinated shared state |

`in_review` means engineering evidence is ready for product acceptance. It does not imply that real users have validated usability or that untested hardware, IMEs or filesystems have passed.

2026-11-03: SIM-921 is revising the design after product feedback. The v4 target is a compact neutral 30-page editable master with concurrent file operations. This is a design artifact checkpoint; no GPUI binary, application staging installation or application smoke test exists yet. A design or source push alone does not complete application delivery.
