//! Mozilla's LZ4 files (`.jsonlz4`): the magic `mozLz40\0`, the decompressed
//! size as a little-endian u32, then one raw LZ4 block.

/// Largest file, compressed or not, an import reads. Session files of heavy
/// users reach tens of MiB.
pub const MAX_BYTES: usize = 64 << 20;

const MAGIC: &[u8] = b"mozLz40\0";

/// `data` decompressed if it is a mozlz4 file, else unchanged (plain JSON).
pub fn decode(data: Vec<u8>) -> Result<Vec<u8>, String> {
    let Some(rest) = data.strip_prefix(MAGIC) else {
        return Ok(data);
    };
    let (size, block) = rest
        .split_first_chunk::<4>()
        .ok_or("truncated mozlz4 header")?;
    let size = usize::try_from(u32::from_le_bytes(*size)).unwrap_or(usize::MAX);
    if size > MAX_BYTES {
        return Err(format!("decompresses to more than {} MiB", MAX_BYTES >> 20));
    }
    let json = lz4_flex::block::decompress(block, size).map_err(|e| e.to_string())?;
    if json.len() != size {
        return Err(format!("{} bytes instead of {size}", json.len()));
    }
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mozlz4(size: u32, block: &[u8]) -> Vec<u8> {
        [MAGIC, &size.to_le_bytes(), block].concat()
    }

    #[test]
    fn round_trip_and_plain_json() {
        let json = br#"{"windows":[{"tabs":[]},{"tabs":[]},{"tabs":[]}]}"#;
        let packed = mozlz4(
            json.len().try_into().unwrap(),
            &lz4_flex::block::compress(json),
        );
        assert_eq!(decode(packed).unwrap(), json);
        assert_eq!(decode(json.to_vec()).unwrap(), json);
    }

    #[test]
    fn rejects_broken_files() {
        assert!(decode(MAGIC.to_vec()).is_err(), "no size");
        assert!(
            decode(mozlz4(u32::MAX, &[0x10, b'x'])).is_err(),
            "too large"
        );
        // A match pointing before the start of the output.
        assert!(decode(mozlz4(64, &[0x1f, b'x', 0xff, 0xff, 0xff])).is_err());
        let json = br#"{"a": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        let block = lz4_flex::block::compress(json);
        assert!(decode(mozlz4(8, &block)).is_err(), "size too small");
        assert!(
            decode(mozlz4(64, &block[..block.len() / 2])).is_err(),
            "truncated"
        );
    }
}
