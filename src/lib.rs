//! Crate root — re-exports all public modules and the [`AnyError`] type alias.
pub mod api;
pub mod db;
pub mod error;
pub mod fetchers;
pub mod loaders;
pub mod logging;
pub mod models;
pub mod on_ice;
pub mod process;
pub mod provenance;
pub mod replay;
pub mod ui;
pub mod webhooks;

/// Convenience alias for a heap-allocated thread-safe error type.
pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Preserve the lowercase, zero-padded representation of persisted SHA-256 hashes.
pub(crate) fn lowercase_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod digest_tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn sha256_matches_known_vectors() {
        for (input, expected) in [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            assert_eq!(super::lowercase_hex(&Sha256::digest(input)), expected);
            let mut digest = Sha256::new();
            for byte in input.as_bytes() {
                digest.update([*byte]);
            }
            assert_eq!(super::lowercase_hex(&digest.finalize()), expected);
        }
    }
}
