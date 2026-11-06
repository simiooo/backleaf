# SIM-954: Explorer clipboard interop and confirmed-failure retry

Two increments for SIM-954, kept separate from the preview.26 delivery because neither is part of that published package. The clipboard check exercises the already published preview.26 build; the retry UI is new source committed as `5ddfb5ce1ca108694908e479ecc9ee324f92d7f5` on top of `892eb0e`.

## Explorer clipboard interop

Build: locally built preview.26 candidate, source `892eb0e296f6e54ea759aafa3a7021de42c78ad8`, `Backleaf.exe` SHA256 `3dcff8ed…722a01f`; this is the same source as the published preview.26 package, with its own separately identified local executable.

The harness ([clipboard/clipboard-smoke.ps1](clipboard/clipboard-smoke.ps1)) drives two real windows of that executable plus real Explorer windows over Win32/UIA/keystrokes on an unlocked 2048×1152 desktop at 125% scaling. It captured the starting clipboard, restored the original text at the end, and never touched the user's default installation. All 16 named checks passed ([clipboard-report.json](clipboard/clipboard-report.json), [run.log](clipboard/run.log)):

- Backleaf Ctrl+C of two files, one with a non-ASCII name, published the standard `CF_HDROP` plus `Preferred DropEffect=1` (DROPEFFECT_COPY); Explorer pasted both files with byte-identical contents and the Backleaf sources stayed unchanged ([01](clipboard/screenshots/01-backleaf-after-copy.png), [02](clipboard/screenshots/02-explorer-after-paste-copy.png)).
- Backleaf Ctrl+X of one file published `Preferred DropEffect=2` (DROPEFFECT_MOVE); Explorer's paste moved it — destination content byte-identical, source file removed ([03](clipboard/screenshots/03-backleaf-after-cut.png), [04](clipboard/screenshots/04-explorer-after-paste-cut.png)).
- Explorer Ctrl+C of `beta.txt` produced a real Shell data object (Explorer reported effect 5 and exposed `Shell IDList Array`, `FileContents` and async formats); Backleaf's second instance read it through `CF_HDROP` and Ctrl+V created a byte-identical `beta.txt` ([05](clipboard/screenshots/05-explorer-after-copy-beta.png), [06](clipboard/screenshots/06-backleaf-after-paste-beta.png)).
- After Backleaf's cut publication, replacing the system clipboard with another application's text cleared Backleaf's cut styling; the icon returns from the dimmed cut state to its normal state ([07](clipboard/screenshots/07-backleaf-cut-before-external-change.png), [08](clipboard/screenshots/08-backleaf-after-external-change.png), enlarged comparison [cut-state-comparison.png](clipboard/screenshots/cut-state-comparison.png); the icon region differs in 2010 pixels).
- Both Backleaf instances exited with code 0 and the original clipboard was restored best-effort.

An initial attempt inside the same automation hit the app's intentional "directory is updating, retry later" guard when pasting immediately after a refresh; the recorded run retried the paste after the directory settled. This is an input-scheduling note, not a claim about clipboard data.

Limits: one machine and one Explorer implementation; no cloud/virtual/OneDrive placeholder sources, no third-party file managers, no clipboard ownership/late-render stress, and no claim about dragging (separately covered by OLE evidence). Window geometry was 1000×700 logical, screenshots physical at 125%.

## Confirmed-failure retry

Implementation (commit `5ddfb5c`): a task whose phase is `Failed` or `Partial` and that has confirmed failed top-level roots exposes one "重试" action in the task detail header. `Hub::retry` registers a new task with the same operation, target and captured identities and seeds its ledger only with roots whose recorded state is `Failed`. Successful, skipped, canceled, partial, not-processed and unverified items are never replayed; a partial directory root is deliberately excluded because replaying the root would repeat successful work inside it. A task can be retried once; failures of the retry task itself get their own retry action. Copy, move and recycle are accepted; other operation kinds and canceled/unknown tasks are rejected with a user-visible toast.

Tests: `ledger::tests::failed_roots_exclude_partial_directories_and_successes`, `service_tests::retry_replays_only_confirmed_failed_roots_into_the_original_target` and `service_tests::retry_rejects_tasks_without_confirmed_failed_roots`; the full Release suite passes 152 tests (1 ignored), strict Clippy `-D warnings` and rustfmt pass.

Real-machine check: build `target/release/backleaf.exe` from the committed sources, SHA256 `3f00424c…accc2e6`. Source contains `keep.txt` and `locked.txt` (the latter held by the harness with `FileShare.None`); two Backleaf windows perform a clipboard copy into an empty destination. `keep.txt` copies, `locked.txt` fails with "文件正在使用"; the task panel reports "1 项未完成" and shows the "重试" action ([03](retry/screenshots/03-task-panel-with-retry.png), UIA dump [task-panel-uia.json](retry/task-panel-uia.json)). The harness then releases the lock and clicks the real "重试" button; the retry task copies only `locked.txt`, the destination ends with exactly `keep.txt` and `locked.txt` (no duplicate of the earlier success), both source files remain intact, and both applications exit with code 0 ([04](retry/screenshots/04-after-retry.png), [retry-report.json](retry/retry-report.json), [run.log](retry/run.log)). All 12 named checks passed.

Limits: failed children inside a partially copied directory are not replayed in this increment; move and recycle share the same service path but have no dedicated test yet, and the real-machine check covers copy. The retry commit passed the repository's [GPUI headless CI 42163386147](https://github.com/simiooo/backleaf/actions/runs/42163386147); no package was published from this increment. No packaged preview or installation yet, no operation-pressure benchmark, and the remaining SIM-954 items (selection-count last-row avoidance, IME candidate withdrawal boundary) are outside this evidence.

## Files and reproducibility

- [clipboard/](clipboard): report, run log, session preflight, two-window harness, eight screenshots plus the cut-state comparison.
- [retry/](retry): report, run log, session preflight, harness, four screenshots, task-panel UIA before and after retry.
- [environment.json](environment.json): OS, display, DPI, PowerShell and exact executable hashes for both builds.
- [manifest.json](manifest.json) lists the SHA256 of every file in this directory except the manifest itself; directory-local `.gitattributes` keeps these bytes exact.
