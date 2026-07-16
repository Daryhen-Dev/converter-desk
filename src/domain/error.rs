use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("unsupported URL scheme: {0}")]
    UnsupportedScheme(String),
}

#[cfg(test)]
mod tests {
    use super::DomainError;

    // 2.3 RED → GREEN — DomainError variants are constructible and display non-empty strings
    #[test]
    fn domain_error_invalid_url_is_constructible() {
        let err = DomainError::InvalidUrl("bad input".to_string());
        let display = format!("{err}");
        assert!(!display.is_empty(), "Display string must be non-empty");
        assert!(display.contains("bad input"));
    }

    #[test]
    fn domain_error_unsupported_scheme_is_constructible() {
        let err = DomainError::UnsupportedScheme("ftp".to_string());
        let display = format!("{err}");
        assert!(!display.is_empty(), "Display string must be non-empty");
        assert!(display.contains("ftp"));
    }

    #[test]
    fn domain_error_variants_are_distinct() {
        let a = DomainError::InvalidUrl("x".into());
        let b = DomainError::UnsupportedScheme("y".into());
        assert_ne!(a, b);
    }
}
