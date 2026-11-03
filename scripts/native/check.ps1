$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
Push-Location $projectRoot
try {
    cargo fmt -p backleaf --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed' }
    cargo clippy -p backleaf --locked --no-deps -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    cargo test --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Native behavior tests failed' }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Native release build failed' }
} finally { Pop-Location }
