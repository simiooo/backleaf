# Search result rhythm / SIM-1014

Source `c96cf09b9bc1609cfbaf0e75dc6fc950040e18e2`, preview25. Search rows previously put two lines inside a fixed 32px body and did not fill the row, leaving the text against its hover surface's top. The shared layout now uses a 56px row with an 18px filename, 2px gap and 14px parent, centered with 11px above/below and 6px horizontal inset. Grouped rows are 64px and reserve heading space. Hover and selection only change colors. Grid results retain their existing arrangement; full-path tooltips and ellipsis remain.

OpenPencil v45 retains all 261 prior pages, adds five states and cold reopens with zero changed pixels in each. Reference PNGs are exported from a fresh process to isolate editor font caches. Strict Clippy, 101 logic/file/native tests and 36 existing production GPUI headless regressions pass; no test duplicating these constants was added.

Source pushed to staging. Actual isolated deployment at `2026-11-05T16:02:03.5514278Z`, exact directory/ZIP and hashes in staging.json; packaged runtime/CLI smoke passed. The desktop was locked during preflight, so no native input or pixel check was attempted. Hosted preview packaging is still separate; the final delivery will verify the published package. This report does not claim native GPU, IME, OLE or screen-reader validation.
