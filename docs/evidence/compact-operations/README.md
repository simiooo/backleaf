# Compact operation feedback / SIM-1012

Source: `aaeef882558363e3cba279b700a0455fde113f73`, preview24. Editable OpenPencil v44 retains 256 prior pages and adds five states; all cold reopen comparisons are exact.

New-folder completion preserves rename intent across late directory notifications. Destination matching uses the resolved snapshot parent, and rename uses that parent without blocking the UI on canonicalization. Real isolated workers test the equivalent display-path case, Unicode naming, focused input and blur commit.

Successful tasks no longer populate an operation-history menu. Short tasks stay quiet; copy/move/recycle/archive tasks reveal a 320px summary only after 600ms. Escape dismisses without reopening. Failures remain available on demand, pagination only appears when needed, and unknown byte totals are omitted. Internal Windows path prefixes are hidden in captions and tooltips.

Local strict Clippy, 101 non-headless tests (one benchmark ignored), and 36 GPUI headless tests passed. The same 36 cases passed [GitHub CI](https://github.com/simiooo/backleaf/actions/runs/42057465475). Source was pushed to staging and actually deployed at `2026-11-05T15:43:04.7273612Z` to the isolated directory recorded in staging.json. Packaged runtime and CLI smoke passed; local executable SHA256 is `30d07ef52bede260f426429ba439559bba91c134f208e450b5ed07933472abab`.

Native e413 evidence is explicitly for the earlier executable: actual blank-area right click, focused new-folder editor, Unicode typing and Tab blur persistence passed. It is not evidence for the final executable. The final native check was stopped by physical Escape before any new automated input. No further desktop automation was attempted. Hosted packaging run 42057467980 is separate and its release/install verification will be recorded in final delivery. GPU pixels, native IME/OLE gestures and screen-reader behavior are outside these headless claims.

Reports retain original bytes; manifest.json lists their hashes. Search-row spacing (SIM-1014) and independent properties/capacity (SIM-1013) are separate engineering units.
