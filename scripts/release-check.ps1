# Release validation: formatting, compile, lint, tests, and optional images.
#
# PostgreSQL-backed tests run when TEST_DATABASE_URL or DATABASE_URL is set.
# S3 integration tests run when TEST_S3_ENDPOINT is set.
# Set RELEASE_CHECK_DOCKER=1 to also build the three service images.
$ErrorActionPreference = "Stop"

function Invoke-Checked {
    param(
        [string]$Label,
        [string[]]$Arguments
    )

    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

Push-Location (Join-Path $PSScriptRoot "..")
try {
    Write-Host "[release] formatting"
    Invoke-Checked "formatting" @("fmt", "--all", "--", "--check")

    Write-Host "[release] compile"
    Invoke-Checked "compile" @("check", "--workspace", "--all-targets")

    Write-Host "[release] lint"
    Invoke-Checked "lint" @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings")

    Write-Host "[release] tests"
    Invoke-Checked "tests" @("test", "--workspace")

    Write-Host "[release] S3 store unit tests"
    Invoke-Checked "S3 unit tests" @("test", "-p", "storage-engine", "--features", "s3", "--lib")

    if ($env:TEST_S3_ENDPOINT) {
        Write-Host "[release] S3 integration tests"
        Invoke-Checked "S3 integration tests" @("test", "-p", "storage-engine", "--features", "s3", "--test", "s3_integration")
    }
    else {
        Write-Host "[release] skipping S3 integration tests (TEST_S3_ENDPOINT unset)"
    }

    if (-not ($env:TEST_DATABASE_URL -or $env:DATABASE_URL)) {
        Write-Host "[release] note: PostgreSQL/full-stack tests were skipped (TEST_DATABASE_URL unset)"
    }

    if ($env:RELEASE_CHECK_DOCKER -eq "1") {
        Write-Host "[release] building container images"
        & docker build -f crates/coordinator/Dockerfile -t rustystore-coordinator .
        if ($LASTEXITCODE -ne 0) { throw "coordinator image build failed" }
        & docker build -f crates/storage-node/Dockerfile -t rustystore-storage-node .
        if ($LASTEXITCODE -ne 0) { throw "storage-node image build failed" }
        & docker build -f crates/gateway/Dockerfile -t rustystore-gateway .
        if ($LASTEXITCODE -ne 0) { throw "gateway image build failed" }
    }

    Write-Host "[release] checks passed"
}
finally {
    Pop-Location
}
