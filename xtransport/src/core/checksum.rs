//! CRC32 checksum implementation for data integrity verification.
//!
//! This module provides a `no_std` compatible CRC32 implementation
//! using the IEEE 802.3 polynomial (0xEDB88320).
//!
//! # Example
//!
//! ```rust
//! use xtransport::Crc32;
//!
//! let data = b"Hello, World!";
//! let checksum = Crc32::compute(data);
//!
//! // Verify checksum
//! assert!(Crc32::verify(data, checksum));
//! ```

/// CRC32 polynomial (IEEE 802.3, reflected).
const CRC32_POLYNOMIAL: u32 = 0xEDB88320;

/// Pre-computed CRC32 lookup table for performance.
/// Generated using the IEEE 802.3 polynomial.
const CRC32_TABLE: [u32; 256] = generate_crc32_table();

/// Generates the CRC32 lookup table at compile time.
const fn generate_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;

    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;

        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC32_POLYNOMIAL;
            } else {
                crc >>= 1;
            }
            j += 1;
        }

        table[i] = crc;
        i += 1;
    }

    table
}

/// CRC32 checksum calculator.
///
/// Provides methods for computing and verifying CRC32 checksums
/// using the IEEE 802.3 polynomial.
#[derive(Debug, Clone, Copy)]
pub struct Crc32 {
    /// Current CRC state (inverted for final output).
    state: u32,
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    /// Creates a new CRC32 calculator with initial state.
    #[inline]
    pub const fn new() -> Self {
        Self { state: 0xFFFFFFFF }
    }

    /// Updates the CRC with the given data.
    ///
    /// This method can be called multiple times to process
    /// data in chunks.
    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            let index = ((self.state ^ (byte as u32)) & 0xFF) as usize;
            self.state = (self.state >> 8) ^ CRC32_TABLE[index];
        }
    }

    /// Finalizes and returns the CRC32 checksum.
    #[inline]
    pub const fn finalize(self) -> u32 {
        self.state ^ 0xFFFFFFFF
    }

    /// Computes the CRC32 checksum of the given data in one call.
    ///
    /// This is a convenience method equivalent to:
    /// ```rust,ignore
    /// let mut crc = Crc32::new();
    /// crc.update(data);
    /// crc.finalize()
    /// ```
    #[inline]
    pub fn compute(data: &[u8]) -> u32 {
        let mut crc = Self::new();
        crc.update(data);
        crc.finalize()
    }

    /// Computes CRC32 over multiple data slices without copying.
    ///
    /// Useful for computing checksum over non-contiguous data.
    #[inline]
    pub fn compute_slices(slices: &[&[u8]]) -> u32 {
        let mut crc = Self::new();
        for slice in slices {
            crc.update(slice);
        }
        crc.finalize()
    }

    /// Verifies that the data matches the expected checksum.
    #[inline]
    pub fn verify(data: &[u8], expected: u32) -> bool {
        Self::compute(data) == expected
    }

    /// Verifies checksum over multiple data slices.
    #[inline]
    pub fn verify_slices(slices: &[&[u8]], expected: u32) -> bool {
        Self::compute_slices(slices) == expected
    }

    /// Resets the CRC calculator to initial state.
    #[inline]
    pub fn reset(&mut self) {
        self.state = 0xFFFFFFFF;
    }

    /// Returns the current intermediate state.
    ///
    /// This can be used to save and restore CRC state
    /// for incremental computation.
    #[inline]
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// Creates a CRC calculator from a saved state.
    #[inline]
    pub const fn from_state(state: u32) -> Self {
        Self { state }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_data() {
        assert_eq!(Crc32::compute(&[]), 0x00000000);
    }

    #[test]
    fn test_known_values() {
        // "123456789" should produce 0xCBF43926
        let data = b"123456789";
        assert_eq!(Crc32::compute(data), 0xCBF43926);
    }

    #[test]
    fn test_incremental() {
        let data = b"Hello, World!";
        let full_crc = Crc32::compute(data);

        let mut crc = Crc32::new();
        crc.update(b"Hello, ");
        crc.update(b"World!");
        assert_eq!(crc.finalize(), full_crc);
    }

    #[test]
    fn test_verify() {
        let data = b"Test data for CRC";
        let checksum = Crc32::compute(data);
        assert!(Crc32::verify(data, checksum));
        assert!(!Crc32::verify(data, checksum ^ 1));
    }

    #[test]
    fn test_slices() {
        let full = b"Hello, World!";
        let full_crc = Crc32::compute(full);

        let slices: &[&[u8]] = &[b"Hello", b", ", b"World!"];
        assert_eq!(Crc32::compute_slices(slices), full_crc);
    }
}
