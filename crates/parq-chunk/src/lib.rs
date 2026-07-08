//! `parq-chunk` — Fixed-size 64 MiB chunk splitter with zero thread contention.
//!
//! See module-level docs in the workspace README for full architecture notes.
//! The key invariant: all returned `&'a [u8]` slices are sub-slices of the
//! caller's `data: &'a [u8]` — zero bytes are copied.

/// Default raw chunk size: 64 MiB.
/// Each rayon worker takes one window, snaps forward to the next `\n`,
/// and parses independently — no synchronisation between threads.
pub const CHUNK_SIZE_BYTES: usize = 64 * 1024 * 1024;

/// Partition `data` into ~64 MiB sub-slices, each cut exactly on a `\n`.
///
/// # Zero-copy contract
/// All returned `&'a [u8]` are sub-slices of `data`.  No byte is copied.
/// The borrow checker enforces that `data` outlives every returned slice.
pub fn fixed_chunks<'a>(data: &'a [u8], chunk_size: Option<usize>) -> Vec<&'a [u8]> {
    if data.is_empty() {
        return Vec::new();
    }
    let window = chunk_size.unwrap_or(CHUNK_SIZE_BYTES).max(1);
    let mut chunks = Vec::with_capacity(data.len() / window + 2);
    let mut start = 0usize;

    while start < data.len() {
        let tentative = (start + window).min(data.len());
        if tentative == data.len() {
            chunks.push(&data[start..]);
            break;
        }
        let end = snap_to_newline(data, tentative);
        chunks.push(&data[start..end]);
        start = end;
    }
    chunks
}

/// Scan forward from `pos` to the byte after the next `\n`.
/// Returns `data.len()` if no newline follows.
/// In practice this scans at most a few hundred bytes (one JSON record).
#[inline]
pub fn snap_to_newline(data: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < data.len() {
        if data[i] == b'\n' {
            return i + 1;
        }
        i += 1;
    }
    data.len()
}

/// Cheap record-count estimate: count `\n` bytes in a chunk.
pub fn count_lines(data: &[u8]) -> usize {
    data.iter().filter(|&&b| b == b'\n').count()
}

/// Offset just after the first `n` newlines — used for file head sampling.
pub fn offset_after_n_lines(data: &[u8], n: usize) -> usize {
    let mut count = 0usize;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            count += 1;
            if count >= n {
                return i + 1;
            }
        }
    }
    data.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ndjson(rows: usize) -> Vec<u8> {
        (0..rows)
            .flat_map(|i| format!("{{\"id\":{i}}}\n").into_bytes())
            .collect()
    }

    #[test]
    fn lossless_for_multiple_chunk_sizes() {
        let data = ndjson(50_000);
        for cs in [64, 512, 4096, 1 << 20] {
            let chunks = fixed_chunks(&data, Some(cs));
            let rebuilt: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
            assert_eq!(rebuilt, data, "chunk_size={cs}");
        }
    }

    #[test]
    fn no_chunk_splits_mid_line() {
        let data = ndjson(1_000);
        let end_ptr = data.as_ptr_range().end;
        for chunk in fixed_chunks(&data, Some(128)) {
            if chunk.as_ptr_range().end != end_ptr {
                assert_eq!(*chunk.last().unwrap(), b'\n');
            }
        }
    }

    #[test]
    fn empty_input() {
        assert!(fixed_chunks(b"", None).is_empty());
    }

    #[test]
    fn no_trailing_newline() {
        let data = b"a\nb\nno_newline";
        let chunks = fixed_chunks(data, Some(4));
        let rebuilt: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
        assert_eq!(&rebuilt, data as &[u8]);
    }
}
