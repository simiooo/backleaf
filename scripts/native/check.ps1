$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
Push-Location $projectRoot
try {
    cargo fmt -p backleaf --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed' }
    cargo clippy --release -p backleaf --locked --all-targets --no-deps -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    cargo test --release --locked -p backleaf -- --skip headless_tests::
    if ($LASTEXITCODE -ne 0) { throw 'Native behavior tests failed' }
    & (Join-Path $PSScriptRoot 'test-headless.ps1')
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Native release build failed' }
} finally { Pop-Location }
