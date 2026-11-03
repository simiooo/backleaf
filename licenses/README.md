# Native dependency notices

`scripts/native/collect-licenses.py` follows the locked Windows normal/build graph and bundles registry/source notice files. For dual-licensed dependencies whose published crate omits the root license files, it selects the declared Apache-2.0 option and includes the canonical license with Cargo attribution. MIT-only omissions have versioned notice overrides and source URLs in `overrides/sources.json`.

GPUI and the internal library crates used by this executable declare Apache-2.0. The Zed application is not linked. The collected manifest lists each actual dependency, version, license declaration and retained source. SeaHash's upstream host returned HTTP 403 during collection; its published MIT declaration and Debian's version-matched copyright/permission notice are retained instead.
