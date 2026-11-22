param(
    [int]$Port = 55432,
    [string]$Image = "postgres:15-alpine"
)

# Provisions a disposable PostgreSQL, runs the metadata integration tests, and
# removes the container. Requires Docker.
$ErrorActionPreference = "Continue"

$name = "rustystore-test-pg"

docker rm -f $name *> $null
docker run -d --name $name `
    -e POSTGRES_USER=rustystore `
    -e POSTGRES_PASSWORD=rustystore `
    -e POSTGRES_DB=rustystore_test `
    -p "${Port}:5432" `
    $Image *> $null
if ($LASTEXITCODE -ne 0) {
    throw "docker run failed"
}

try {
    $ready = $false
    for ($i = 0; $i -lt 60; $i++) {
        docker exec $name pg_isready -U rustystore -d rustystore_test *> $null
        if ($LASTEXITCODE -eq 0) {
            $ready = $true
            break
        }
        Start-Sleep -Seconds 1
    }
    if (-not $ready) {
        throw "PostgreSQL did not become ready within 60s"
    }

    $env:TEST_DATABASE_URL = "postgres://rustystore:rustystore@localhost:${Port}/rustystore_test"
    cargo test -p metadata --test postgres_integration -- --test-threads=1
    if ($LASTEXITCODE -ne 0) {
        throw "metadata integration tests failed"
    }
}
finally {
    docker rm -f $name *> $null
}
