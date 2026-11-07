//! Chunk generation utilities for the gateway.
//!
//! Splits an async byte stream into bounded fixed-size chunks, computing
//! SHA-256 for each chunk incrementally. Never buffers more than one chunk
//! in memory at a time.

use common::error::RustyStoreError;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncRead;
use tokio_util::codec::{BytesCodec, FramedRead};

/// A generated chunk with its content hash.
#[derive(Debug, Clone)]
pub struct GeneratedChunk {
    pub data: Vec<u8>,
    pub sha256_hex: String,
    pub index: u64,
}

/// Splits an async byte stream into bounded chunks of at most `chunk_size`
/// bytes each. Each chunk's SHA-256 is computed incrementally during the
/// split.
pub struct ChunkGenerator<R> {
    reader: FramedRead<R, BytesCodec>,
    chunk_size: usize,
    index: u64,
    eof: bool,
}

impl<R: AsyncRead + Unpin> ChunkGenerator<R> {
    pub fn new(reader: R, chunk_size: usize) -> Self {
        Self {
            reader: FramedRead::new(reader, BytesCodec::new()),
            chunk_size,
            index: 0,
            eof: false,
        }
    }

    /// Read the next bounded chunk, computing its SHA-256.
    /// Returns `None` when the stream is exhausted.
    pub async fn next_chunk(&mut self) -> Result<Option<GeneratedChunk>, RustyStoreError> {
        if self.eof {
            return Ok(None);
        }

        let mut hasher = Sha256::new();
        let mut buf = Vec::with_capacity(self.chunk_size);
        let mut remaining = self.chunk_size;

        while remaining > 0 {
            let bytes = match self.reader.next().await {
                Some(Ok(b)) => b,
                Some(Err(e)) => return Err(RustyStoreError::Io(e)),
                None => {
                    self.eof = true;
                    break;
                }
            };

            let take = bytes.len().min(remaining);
            buf.extend_from_slice(&bytes[..take]);
            hasher.update(&bytes[..take]);
            remaining -= take;

            if buf.len() >= self.chunk_size {
                break;
            }
        }

        if buf.is_empty() {
            self.eof = true;
            return Ok(None);
        }

        let sha256_hex = hex::encode(hasher.finalize());
        let chunk = GeneratedChunk {
            data: buf,
            sha256_hex,
            index: self.index,
        };
        self.index += 1;
        Ok(Some(chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::Cursor;

    #[tokio::test]
    async fn test_chunk_generator_splits_correctly() {
        let data = vec![0x42u8; 10_000];
        let cursor = Cursor::new(data);
        let mut r#gen = ChunkGenerator::new(cursor, 4096);

        let mut chunks = Vec::new();
        while let Some(chunk) = r#gen.next_chunk().await.unwrap() {
            chunks.push(chunk);
        }

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].data.len(), 4096);
        assert_eq!(chunks[1].data.len(), 4096);
        assert_eq!(chunks[2].data.len(), 1808);

        for (i, chunk) in chunks.iter().enumerate() {
            let expected = sha256_of(&chunk.data);
            assert_eq!(chunk.sha256_hex, expected, "chunk {i} hash mismatch");
            assert_eq!(chunk.index, i as u64);
        }
    }

    #[tokio::test]
    async fn test_chunk_generator_empty_stream() {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut r#gen = ChunkGenerator::new(cursor, 4096);
        let chunk = r#gen.next_chunk().await.unwrap();
        assert!(chunk.is_none());
    }

    #[tokio::test]
    async fn test_chunk_generator_exact_multiple() {
        let data = vec![0x00u8; 8192];
        let cursor = Cursor::new(data);
        let mut r#gen = ChunkGenerator::new(cursor, 4096);

        let mut chunks = Vec::new();
        while let Some(chunk) = r#gen.next_chunk().await.unwrap() {
            chunks.push(chunk);
        }
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data.len(), 4096);
        assert_eq!(chunks[1].data.len(), 4096);
    }

    fn sha256_of(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }
}
