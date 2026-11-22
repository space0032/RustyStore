use criterion::{Criterion, black_box, criterion_group, criterion_main};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use storage_engine::{ChunkStore, FsChunkStore};

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn storage_write_4kb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024];
    let hash = sha256_hex(&data);

    c.bench_function("store_write_4kb", |b| {
        b.iter(|| {
            rt.block_on(async {
                store
                    .store_chunk(black_box("bench-chunk"), &data, &hash)
                    .await
                    .unwrap()
            })
        })
    });

    drop(dir);
}

fn storage_write_1mb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 1_048_576];
    let hash = sha256_hex(&data);

    c.bench_function("store_write_1mb", |b| {
        b.iter(|| {
            rt.block_on(async {
                store
                    .store_chunk(black_box("bench-chunk"), &data, &hash)
                    .await
                    .unwrap()
            })
        })
    });

    drop(dir);
}

fn storage_write_4mb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024 * 1024];
    let hash = sha256_hex(&data);

    c.bench_function("store_write_4mb", |b| {
        b.iter(|| {
            rt.block_on(async {
                store
                    .store_chunk(black_box("bench-chunk"), &data, &hash)
                    .await
                    .unwrap()
            })
        })
    });

    drop(dir);
}

fn storage_read_4mb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024 * 1024];
    let hash = sha256_hex(&data);
    rt.block_on(async {
        store
            .store_chunk("bench-read-chunk", &data, &hash)
            .await
            .unwrap()
    });

    c.bench_function("store_read_4mb", |b| {
        b.iter(|| {
            rt.block_on(async {
                let entry = store.retrieve_chunk("bench-read-chunk").await.unwrap();
                black_box(entry);
            })
        })
    });

    drop(dir);
}

fn storage_delete(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024 * 1024];
    let hash = sha256_hex(&data);
    rt.block_on(async {
        store
            .store_chunk("bench-delete-chunk", &data, &hash)
            .await
            .unwrap()
    });

    c.bench_function("store_delete_4mb", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = store.delete_chunk(black_box("bench-delete-chunk")).await;
                let _ = store.store_chunk("bench-delete-chunk", &data, &hash).await;
            })
        })
    });

    drop(dir);
}

criterion_group!(
    storage_benches,
    storage_write_4kb,
    storage_write_1mb,
    storage_write_4mb,
    storage_read_4mb,
    storage_delete
);

criterion_main!(storage_benches);
