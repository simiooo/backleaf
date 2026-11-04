# SIM-951 acceptance checkpoint

The locked-desktop gate was resolved on 2026-11-04 after the user unlocked Windows and authorized continued automated checks. The final installed source is `edfcfc674d8a309f5e2e780c870f0269530208ef`. See [deployment](evidence/native/staging-candidate.json), [final raw evidence](evidence/native/README.md) and [method/results](native-render-validation.md).

- Source format, Clippy, four behavior tests and Release build passed.
- Eleven unlocked native performance cases and the real 100,001-entry NTFS directory completed.
- Long Unicode/unbroken ASCII text, full details and clipboard values, external UIA text, ten-million-item navigation/selection, layout, native chrome/Snap Layout and actual directory navigation were checked in the installed window.
- Active flowing labels stop requesting animation frames while minimized and resume when restored. Locked-state evidence remains separately mapped to its earlier build; Windows lock/unlock was not automated.
- Portable ZIP is deployed to isolated local staging, uploaded bytes have been downloaded and verified, and the prerelease is the product acceptance entry.

SIM-951 is engineering review, not product `done`. Remaining product work follows SIM-919's separate units: search/quick access, persistent indexing, restore, Shell writes, thumbnails, full Windows IME/accessibility/drag-and-drop and additional hardware/DPI coverage. Do not reopen SIM-951 merely to infer these features shipped.
