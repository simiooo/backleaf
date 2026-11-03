# Native evidence status

`diagnostic/` contains real GPUI/Direct3D11 scene-build and process measurements from development, including a 100,001-entry NTFS directory. The desktop was later found locked and that build did not record session state. These are retained diagnostics, **not final visible-desktop acceptance results**.

The later candidate adds WTS session notifications, a read-only session preflight, and benchmark rejection on locked/disconnected sessions. `locked-session-guard.json` tests the intentional locked branch: no decorative frame callbacks, no post-warmup redraw samples. Zero samples do not mean a zero-ms frame.

Final unlocked performance, UIA, long-text/layout checks and installed-window smoke remain pending until the user unlocks Windows. Hardware inventory is not evidence that every enumerated adapter was used for rendering. All measurements must be interpreted with their recorded viewport, scale, warmup and backend limits.
