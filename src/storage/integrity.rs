use sha2::{Digest, Sha256};

/// Calculate SHA-256 checksum for data integrity verification
pub fn calculate_checksum(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Verify that data matches the expected checksum
pub fn verify_checksum(data: &str, expected_checksum: &str) -> bool {
    let actual_checksum = calculate_checksum(data);
    actual_checksum == expected_checksum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_calculation() {
        let data = "test data";
        let checksum = calculate_checksum(data);

        // SHA-256 of "test data"
        assert_eq!(
            checksum,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
    }

    #[test]
    fn test_checksum_verification() {
        let data = "test data";
        let checksum = calculate_checksum(data);

        assert!(verify_checksum(data, &checksum));
        assert!(!verify_checksum(data, "wrong_checksum"));
    }
}
