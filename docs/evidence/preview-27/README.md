# preview27: confirmed-failure retry candidate

Source `7b1ae2916c1f19197d74de3e6d2902ca4373f55d` (0.1.0-preview.27), published as a trial candidate on top of preview.26. Design master remains editable OpenPencil v46 (271 pages); no new visual states were needed because the retry action uses the existing task-panel design.

## What changed in this candidate

- File-operation tasks that end in `Failed` or `Partial` with confirmed failed top-level roots now show one “重试” action in the task detail. Retry creates a new task with the original operation, target and captured identities, seeded only with roots whose recorded state is `Failed`. Successful, unknown, canceled, not-processed and partial-directory items are never replayed, and a task can be retried once; failures of the retry task get their own action. Copy, move and recycle are accepted; other operations and canceled/unknown tasks are rejected.
- Explorer file-clipboard interop was verified on the preview.26 ancestry with 16 real-window checks (Backleaf copy/cut to Explorer paste, Explorer copy to Backleaf paste, byte-identical contents, cut source removal, external clipboard change clearing the cut marker). The clipboard implementation is unchanged in preview.27; that evidence is recorded separately in [../sim-954/README.md](../sim-954/README.md).
- Test gate grows to 40 production GPUI headless scenarios plus 112 non-headless logic/file/native tests (previously 109); release notes and README were updated accordingly.

## Published package

- Release: [test-v0.1.0-preview.27-7b1ae291](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.27-7b1ae291), published 2026-11-06 13:11:28 +08:00 (05:11:28Z), pre-release, not draft, tag target `7b1ae29`.
- [Windows test package CI 42166140371](https://github.com/simiooo/backleaf/actions/runs/42166140371) and [headless CI 42166129690](https://github.com/simiooo/backleaf/actions/runs/42166129690) succeeded for the same source.
- Assets: ZIP `88f7a023…d1909` (16,479,932 bytes), design `216c6ffd…ab179`, build record `8a166019…f764324`, sums file `462616d9…f56e70`.
- The ZIP, build record and sums file were downloaded anonymously over HTTPS without a GitHub token; downloaded bytes match the release asset digests. Inside the ZIP, `Backleaf.exe` is `a6e0c95f…640a2a7` and matches `build.json` (`source_commit=7b1ae29`, `ci_run_url=.../42166140371`), the packaged `headless.json` matches `headless_report_sha256`, and `lib/7z.dll` matches `archive_library_sha256`. See [public-assets.json](public-assets.json) and [ci-package-verified.json](ci-package-verified.json).

## Anonymous isolated installation

`scripts/native/test-installer-online.ps1` downloaded the public release anonymously, verified assets/inner hashes and the Microsoft-signed runtime manifest, installed to isolated directories with `-NoShortcut`, checked three app-local runtime loads and ran the hidden CLI diagnostic. The default user installation was not touched.

- Windows PowerShell 5.1.26100.9711: `passed=true`, EXE `a6e0c95f…`, installed under `.local/installer-staging/5-69db163b…`. [online-ps5.json](online-ps5.json)
- PowerShell 7.6.9 (official portable ZIP, SHA256 `ce748480…a94d`): `passed=true`, same EXE hash, installed under `.local/installer-staging/7-baaf3c6f…`. [online-ps7.json](online-ps7.json)

## Visible retry smoke on the published executable

The published CI executable (`a6e0c95f…`, extracted from the anonymously downloaded ZIP) repeated the locked-file scenario on an unlocked 2048×1152 / 125% desktop with two isolated windows: copy `keep.txt` and locked `locked.txt` into an empty destination, observe the partial task, release the lock, click the real “重试” action, then verify the result. All 12 named checks passed ([retry-visible-report.json](retry-visible-report.json), [retry-visible.log](retry-visible.log)):

- task panel showed “1 项未完成”, the failure reason “文件正在使用” and the “重试” button ([task-panel-uia.json](task-panel-uia.json), [03](screenshots/03-task-panel-with-retry.png));
- after retry, only `locked.txt` was copied into the destination, which ended with exactly `keep.txt` and `locked.txt` (no duplicate of the earlier success); both source files remained intact; both instances exited with code 0 ([04](screenshots/04-after-retry.png));
- session preflight reports the desktop unlocked and the test used isolated state directories ([session-preflight.json](session-preflight.json)).

## Limits

- This candidate is for trial; SIM-954 remains in progress because failed children inside a partially copied directory are not retried, move/recycle retry has no dedicated real-machine test, and selection-count last-row avoidance, IME candidate withdrawal and operation-pressure acceptance are still open.
- The visible check ran on the public ZIP-extracted CI executable, not on a default-user installation; no default installation or upgrade is claimed. Preview.27 has not gone through product acceptance.
- Single monitor and 125% scaling; no mixed-DPI, monitor removal, NVDA, native IME or GPU-pixel claim. Clipboard interop evidence belongs to the unchanged preview.26 implementation and is not re-run here.
- Reports and screenshots retain their bytes; [manifest.json](manifest.json) lists SHA256 for every file in this directory except the manifest itself.

## Files

[public-build.json](public-build.json) and [public-SHA256SUMS.txt](public-SHA256SUMS.txt) are the anonymously downloaded package records; [environment.json](environment.json) captures OS, display, PowerShell and executable hashes; [ci-package-status.json](ci-package-status.json) / [ci-headless-status.json](ci-headless-status.json) keep the raw CI conclusions.
