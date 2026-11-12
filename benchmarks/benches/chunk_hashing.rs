use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rand::Rng;
use sha2::{Digest, Sha256};

fn sha256_1kb(c: &mut Criterion) {
    let data = vec![0x42u8; 1024];
    c.bench_function("sha256_1kb", |b| {
        b.iter(|| {
            let mut hasher = Sha256::new();
            hasher.update(black_box(&data));
            hasher.finalize()
        })
    });
}

fn sha256_4mb(c: &mut Criterion) {
    let data = vec![0x42u8; 4 * 1024 * 1024];
    c.bench_function("sha256_4mb", |b| {
        b.iter(|| {
            let mut hasher = Sha256::new();
            hasher.update(black_box(&data));
            hasher.finalize()
        })
    });
}

fn sha256_8mb(c: &mut Criterion) {
    let data = vec![0x42u8; 8 * 1024 * 1024];
    c.bench_function("sha256_8mb", |b| {
        b.iter(|| {
            let mut hasher = Sha256::new();
            hasher.update(black_box(&data));
            hasher.finalize()
        })
    });
}

fn sha256_random_1mb(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let data: Vec<u8> = (0..1_048_576).map(|_| rng.r#gen()).collect();
    c.bench_function("sha256_random_1mb", |b| {
        b.iter(|| {
            let mut hasher = Sha256::new();
            hasher.update(black_box(&data));
            hasher.finalize()
        })
    });
}

criterion_group!(
    hashing_benches,
    sha256_1kb,
    sha256_4mb,
    sha256_8mb,
    sha256_random_1mb
);

criterion_main!(hashing_benches);
