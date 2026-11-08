//! Arc-specific parsing: `raw` mirrors the on-disk JSON, `convert` maps it
//! to the engine-independent model.

pub mod convert;
pub mod raw;

/// Arc stores timestamps as seconds since 2001-01-01 (Apple's reference
/// date). Convert to Unix seconds.
#[allow(clippy::cast_possible_truncation)]
pub fn apple_to_unix(seconds: f64) -> i64 {
    const APPLE_EPOCH_OFFSET: i64 = 978_307_200;
    // Truncation is intended: sub-second precision is irrelevant here; the
    // cast saturates and so does the sum, so no file makes this overflow.
    (seconds as i64).saturating_add(APPLE_EPOCH_OFFSET)
}

#[cfg(test)]
mod tests {
    #[test]
    fn apple_epoch_offset() {
        // 2026-10-13T14:25:35Z in Apple seconds.
        assert_eq!(super::apple_to_unix(813_488_735.0), 1_791_795_935);
        assert_eq!(super::apple_to_unix(0.0), 978_307_200);
        assert_eq!(super::apple_to_unix(1e300), i64::MAX);
    }
}
