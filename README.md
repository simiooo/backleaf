# 拾径 / Backleaf

A Windows file manager with a real directory grid, local search and one quick access list.

Design precedes implementation. Open [design/Backleaf.fig](design/Backleaf.fig), the preserved v6 baseline. The current [v9 state/geometry revision](design/Backleaf-v9.fig) retains those 44 pages and adds eight editable pages, indexed in [design](design/README.md). Product scope follows Nuto SIM-919 and SIM-934; multi-window behavior follows SIM-938.

The user accepted v6 as the implementation baseline on 2026-11-04. A native Windows GPUI preview is implemented: viewport-based file rendering, a worker-owned SQLite directory snapshot, long text/details, and a bounded filename highlight animation. Search and Shell write operations remain separate work units.

Build on Windows with MSVC and Rust 1.99.1:

```powershell
./scripts/native/bootstrap.ps1
cargo build --release --locked
./target/release/backleaf.exe
```

`--fixture 1000000 --flow 8` opens an explicitly labelled rendering sample. `--path <directory>` uses a real directory. The [Windows staging preview](https://github.com/simiooo/backleaf/releases/tag/native-v0.1.0-preview.1-20261104) has passed unlocked installed-window smoke and native rendering measurements through ten million metadata items, plus an actual 100,001-entry directory. See [native validation and its current limits](docs/native-render-validation.md).

See [design contract](docs/design-contract.md), [research](docs/research.md) and [delivery status](docs/delivery.md).
