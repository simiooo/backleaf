# Native validation and acceptance

Current regression policy (SIM-992, 2026-11-05): routine interaction checks run on GPUI's TestPlatform/HeadlessAppContext, using the production Backleaf root, TextInput, menus, focus subscriptions, directory/search workers and isolated file-operation service. The current editable baseline is v43/256 pages; v38 remains historical. No desktop input or native GPUI window is used. `./scripts/native/test-headless.ps1` enforces 31 named scenarios and emits exact test names, exit status, source/worktree state and log hash. Preview23 adds indexed-root case migration/recovery and ongoing nonrecursive search updates; Preview19 adds atomic directory/search refresh, selection identity/scroll preservation, repeated-letter cycling, nonrecursive selected-file bytes, and stable asynchronous volume-row geometry. Native capacity and PnP registration/callback checks remain separate from headless and from physical media insertion/removal.

| Headless scope | Assertions |
| --- | --- |
| Rename | F2/input/blur, real changed file and retained bytes; invalid-name draft/focus recovery and painted toast; existing-target failure without overwrite; case-only change; Escape; rename followed by another file's context menu |
| Path | Ctrl+L, blur resolution failure, retained draft and painted toast, focus restoration, corrected relative-path navigation |
| Search | Focused Unicode input through GPUI event dispatch, real matching results, Enter/Escape, 250ms virtual-clock debounce and latest query |
| Independent menu | Separate TestPlatform window; Escape and owner activation; command runs back in owner and focuses editor; owner closure removes popup without reading owner entity |
| Selection | 100k generated items, held-pointer edge scrolling, async selected-ID commit, release stops scrolling, additive Escape restores original selection; bounded caches and one pending clock |

The default TestPlatform text system uses synthetic metrics and has no pixel renderer or screen-reader adapter. Error tests assert retained error text plus the actually painted toast surface, not an invented accessibility snapshot. Unicode text injection tests GPUI routing, not native IME candidate windows. Headless popup placement replaces only the HWND/monitor boundary; production MenuPopup rendering, lease and subscriptions remain shared.

PRs and relevant pushes to staging/main run the read-only `GPUI headless regressions` workflow. Test-package publishing runs the same gate again and refuses missing, failing or dirty-source reports. Hosted CI stores logs/JSON as artifacts; packaged builds include headless.json. Native checking remains for HWND ownership/monitor placement/DWM, actual IME, Explorer/OLE gestures, third-party Shell behavior, UIA/NVDA, GPU pixels and real performance. These are verified only when changed/relevant, not by repeating all headless scenarios on the user's desktop.

The SIM-951 notes below are historical version-specific evidence.

The locked-desktop gate was resolved on 2026-11-04 after the user unlocked Windows and authorized continued automated checks. The final installed source is `edfcfc674d8a309f5e2e780c870f0269530208ef`. See [deployment](evidence/native/staging-candidate.json), [final raw evidence](evidence/native/README.md) and [method/results](native-render-validation.md).

- Source format, Clippy, four behavior tests and Release build passed.
- Eleven unlocked native performance cases and the real 100,001-entry NTFS directory completed.
- Long Unicode/unbroken ASCII text, full details and clipboard values, external UIA text, ten-million-item navigation/selection, layout, native chrome/Snap Layout and actual directory navigation were checked in the installed window.
- Active flowing labels stop requesting animation frames while minimized and resume when restored. Locked-state evidence remains separately mapped to its earlier build; Windows lock/unlock was not automated.
- Portable ZIP is deployed to isolated local staging, uploaded bytes have been downloaded and verified, and the prerelease is the product acceptance entry.

SIM-951 is engineering review, not product `done`. Remaining product work follows SIM-919's separate units: search/quick access, persistent indexing, restore, Shell writes, thumbnails, full Windows IME/accessibility/drag-and-drop and additional hardware/DPI coverage. Do not reopen SIM-951 merely to infer these features shipped.
