use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sha2::{Digest, Sha256};

fn hash_4mb(c: &mut Criterion) {
    let data = vec![0u8; 4 * 1024 * 1024];
    c.bench_function("sha256_4mb", |b| {
        b.iter(|| {
            let mut hasher = Sha256::new();
            hasher.update(black_box(&data));
            hasher.finalize()
        })
    });
}

criterion_group!(benches, hash_4mb);
criterion_main!(benches);
