# preview26: independent properties windows, anonymous installation and visible-window smoke

Source `892eb0e296f6e54ea759aafa3a7021de42c78ad8` (0.1.0-preview.26). Editable OpenPencil v46 retains 266 prior pages and adds five properties states (271 pages); its independent cold reopen and pixel comparison are committed in `design/v46/`. File and folder details now open in independent windows with the complete selectable name and path, logical size/time and on-demand folder statistics: 200 ms snapshot merging and 120 ms numeric settling, final values under reduced motion or inactive windows, detected reparse and offline entries skipped, F5 rechecks the selected identity and cancels, and Esc/Ctrl+W closes while cancelling the background reader. The earlier `014faa` candidate was not delivered: its visible-scrollbar audit failed, its CI was cancelled, and no evidence from it is reused here. Every claim below concerns the final `892eb0e` source and its published CI executable only.

## Published package

- Release: [preview.26](https://github.com/simiooo/backleaf/releases/tag/test-v0.1.0-preview.26-892eb0e2), published 2026-11-06 00:44:33 +08:00 (2026-11-05T16:44:33Z), pre-release, not draft. The tag points at `892eb0e296f6e54ea759aafa3a7021de42c78ad8`.
- [Package CI 42066259842](https://github.com/simiooo/backleaf/actions/runs/42066259842) and [headless CI 42066257802](https://github.com/simiooo/backleaf/actions/runs/42066257802) both completed successfully for `892eb0e`.
- Assets: ZIP `646e6686c634479e34ca6e317e66cd4a50a3f4c35a7c509c9b4540ccab4abb21` (16,462,051 bytes), design `216c6ffd8f632f908406d4765a9be010b59c24e70d59f7b8a63bc1e5055ab179`, build record `1e48d148a0b8bd6f69656070273d3feb94f6063b1cd78cb1cb0488c489cc94fe`, sums file `12bdc3a471291835e0ce662c6a470601018cc944ef2993dd2a66cd16817744d4`.
- The ZIP, build record and sums file were downloaded anonymously over HTTPS without a GitHub token (`curl.exe`, direct `browser_download_url`); downloaded bytes match the release asset digests. Inside the ZIP, `Backleaf.exe` is `cdbd88b1fb7880881dd30ed4663d9b3f5b9dbdfd3e2c422269c585fe8cc0cfcc` and matches `build.json`, which also records `source_commit=892eb0e`, `ci_run_url=.../42066259842` and `built_at_utc=2026-11-05T16:44:02Z`. The packaged `headless.json` (`f17aa183...`) matches `build.json.headless_report_sha256`, `lib/7z.dll` (`78bf6b66...`) matches `archive_library_sha256`, and the design asset matches `design_sha256`. Hashes and digests are in [public-assets.json](public-assets.json).
- The public one-line command fetches `https://raw.githubusercontent.com/simiooo/backleaf/staging/install.ps1`; those public bytes are `2a140b01ea9063f45c0db0b6e0d348af6afe908ab84b9bd0c68c26e6e04eaf93`, identical to the repository file used by the test below ([public-installer-script.json](public-installer-script.json)).

## Anonymous isolated installation

`scripts/native/test-installer-online.ps1` dot-sources the public `install.ps1`, asks only anonymously accessible GitHub release endpoints and HTTPS asset URLs, downloads the application ZIP, `build.json`, `SHA256SUMS.txt` and `runtime-v2`, verifies sizes, digests, inner package hashes and the Microsoft-signed DLL manifest, then installs to an isolated directory with `-NoShortcut`. It never installs to the default user directory, creates no Start-menu shortcut and changes no PATH or global setting. The installed executable is then launched hidden with `--session-status-file` as a CLI diagnostic.

- Windows PowerShell 5.1.26100.9711 passed: installed at 2026-11-06T03:48:43Z under `.local/installer-staging/5-4112665c34434abe82fde181df353067`, EXE hash `cdbd88b1...`, all ten runtime DLL hashes and three app-local runtime loads verified, CLI diagnostic exit 0. See [online-ps5.json](online-ps5.json) and [online-ps5.log](online-ps5.log).
- PowerShell 7.6.9 (official portable ZIP, SHA256 `ce7484804c0f0ad4dda10c958e71acdaf7eebfb22505c1b480f89e0e9f1da94d`) passed the same flow at 2026-11-06T03:48:50Z under `.local/installer-staging/7-823595ac77914603b7d1f08a363a86e2`, same EXE/runtime hashes, three app-local loads. See [online-ps7.json](online-ps7.json) and [online-ps7.log](online-ps7.log).
- Both reports record `passed=true`, `cli_smoke_passed=true`, `anonymous_download=true`. A first attempt at 03:46Z failed before any report with a transient host TLS/transport error and was discarded; only the retried runs above are recorded. The reports correctly note `session.locked=true` at install time because the desktop was locked during those CLI-only runs; installation does not require an interactive desktop.

## Visible-window smoke on the published executable

The visible check ran on the executable extracted from the anonymously downloaded public ZIP (`cdbd88b1...`), the same SHA256 installed by both runs above, so the installed bytes and visibly tested bytes are identical. The desktop was unlocked (`locked=false`) for this run, the virtual screen was 2048x1152 at 125% scaling, and the user's default installed Backleaf instance was left running and untouched. `--state-dir` and the fixture were isolated under `.local/verify/preview26/`.

[gui-smoke.ps1](gui-smoke.ps1) drove the production executable through Win32/UIA/keystrokes; [gui-report.json](gui-report.json) records 28 named checks, all passing, plus exact window rectangles and timestamps. The observed facts:

- The main window was a single visible window (1402x901 at 579,236), its UIA tree exposed the file grid and the long fixture file as a selectable list item. A UIA-targeted mouse click selected it (`SelectionItemPattern.IsSelected=true`).
- Alt+Enter opened a second, independent `<name> · 详情` window while the main window's DWM rectangle stayed byte-for-byte identical before and after (579,236,1981,1137).
- The properties window exposed the complete 173-character name and complete path, `TXT 文件`, `188 B` and the creation/modification times as UIA text; the painted `188 字节` summary is confirmed in [visual-review.json](visual-review.json).
- Appending 100 bytes to the file and pressing F5 in the properties window changed the UIA value to `288 B` (painted `288 字节`), proving the refresh path re-reads the selected file without reopening the window.
- Resizing the window to its 340x360 minimum exposed a visible vertical scrollbar (`ControlType.ScrollBar`, name `详情滚动条`, range 0-100). Clicking its track moved the range value to 100 and scrolled the content to the bottom; enlarging the window to 620x900 removed the scrollbar entirely and showed all content.
- The folder `data` opened its own properties window and settled at `8 B` with painted `2 个文件 · 2 个文件夹`; after adding `c.txt` and pressing F5 it re-read as `12 B` with painted `3 个文件 · 2 个文件夹` and an updated modification time.
- Escape closed the properties window, the main window remained the only window and kept its rectangle, the application then exited with code 0 and no test process was left behind.

Screenshots are in [screenshots/](screenshots) (main window, long-name file details, F5 update, minimum-size scrollbar, scrolled state, enlarged state, folder details before and after F5, close and final states), raw UIA trees are in [uia/](uia), and the captured run output is [gui-run.log](gui-run.log).

## Limits

- This is an isolated anonymous installation, not a default-user installation or upgrade; no Start-menu shortcut, default directory or global setting was touched.
- The visible run launched the public ZIP-extracted executable rather than the installer's `current` directory; the SHA256 equality above is the transfer of evidence between those two copies.
- Single monitor, single 125% scale, no mixed-DPI, monitor removal, NVDA, native IME candidate, GPU-pixel or input-to-photon claim. Keyboard input was sent only after confirming the intended window was foreground.
- Folder file/folder counts and the `N 字节` summary are painted but not exposed as separate UIA text nodes (AccessKit exposes the large `N B` value and the selectable texts); those lines are confirmed from the listed screenshots in [visual-review.json](visual-review.json), not asserted as machine-readable text.
- The separately built local candidate (`local-staging.json`) is not the published package and no visible check is claimed for it.
- Reports and screenshots retain their recorded bytes; [manifest.json](manifest.json) lists the SHA256 of every file in this directory except the manifest itself.
