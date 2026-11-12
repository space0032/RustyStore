use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

use storage_engine::{ChunkStore, FsChunkStore};

async fn bench_store_chunk(store: &FsChunkStore, data: &[u8], expected_hash: &str) {
    store
        .store_chunk(black_box("bench-chunk"), data, expected_hash)
        .await
        .unwrap();
}

async fn bench_retrieve_chunk(store: &FsChunkStore) {
    let entry = store
        .retrieve_chunk(black_box("bench-chunk"))
        .await
        .unwrap();
    black_box(entry);
}

fn storage_write_4kb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024];
    let hash = Sha256::digest(&data).to_string();

    c.bench_function("store_write_4kb", |b| {
        b.to_async(&rt)
            .iter(|| bench_store_chunk(&store, &data, &hash));
    });

    drop(dir);
}

fn storage_write_1mb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 1_048_576];
    let hash = Sha256::digest(&data).to_string();

    c.bench_function("store_write_1mb", |b| {
        b.to_async(&rt)
            .iter(|| bench_store_chunk(&store, &data, &hash));
    });

    drop(dir);
}

fn storage_write_4mb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024 * 1024];
    let hash = Sha256::digest(&data).to_string();

    c.bench_function("store_write_4mb", |b| {
        b.to_async(&rt)
            .iter(|| bench_store_chunk(&store, &data, &hash));
    });

    drop(dir);
}

fn storage_read_4mb(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024 * 1024];
    let hash = Sha256::digest(&data).to_string();
    rt.block_on(async {
        store
            .store_chunk("bench-read-chunk", &data, &hash)
            .await
            .unwrap()
    });

    c.bench_function("store_read_4mb", |b| {
        b.to_async(&rt).iter(|| bench_retrieve_chunk(&store));
    });

    drop(dir);
}

fn storage_delete(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let store = FsChunkStore::new(dir.path());
    rt.block_on(async { store.init().await.unwrap() });

    let data = vec![0x42u8; 4 * 1024 * 1024];
    let hash = Sha256::digest(&data).to_string();
    rt.block_on(async {
        store
            .store_chunk("bench-delete-chunk", &data, &hash)
            .await
            .unwrap()
    });

    c.bench_function("store_delete_4mb", |b| {
        b.to_async(&rt).iter(|| {
            let store = store.clone();
            async move {
                let _ = store.delete_chunk(black_box("bench-delete-chunk")).await;
                let _ = store.store_chunk("bench-delete-chunk", &data, &hash).await;
            }
        });
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
