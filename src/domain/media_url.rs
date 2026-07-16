use crate::domain::error::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaUrl(url::Url);

impl MediaUrl {
    /// Parse and validate a URL string. Only http and https schemes are accepted.
    pub fn parse(input: &str) -> Result<Self, DomainError> {
        let parsed = url::Url::parse(input)
            .map_err(|e| DomainError::InvalidUrl(e.to_string()))?;

        match parsed.scheme() {
            "http" | "https" => {}
            s => return Err(DomainError::UnsupportedScheme(s.to_owned())),
        }

        match parsed.host_str() {
            None | Some("") => return Err(DomainError::InvalidUrl("missing host".into())),
            _ => {}
        }

        Ok(Self(parsed))
    }

    /// Return the URL as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::MediaUrl;

    // 2.5 RED → GREEN — MediaUrl validation
    #[test]
    fn accepts_valid_https_url() {
        let result = MediaUrl::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert!(result.is_ok(), "Expected Ok, got {result:?}");
    }

    #[test]
    fn accepts_valid_http_url() {
        let result = MediaUrl::parse("http://example.com/video");
        assert!(result.is_ok(), "Expected Ok, got {result:?}");
    }

    #[test]
    fn rejects_ftp_scheme() {
        let result = MediaUrl::parse("ftp://example.com/file");
        assert!(result.is_err(), "Expected Err for ftp://, got Ok");
    }

    #[test]
    fn rejects_empty_string() {
        let result = MediaUrl::parse("");
        assert!(result.is_err(), "Expected Err for empty string, got Ok");
    }

    #[test]
    fn rejects_non_url_string() {
        let result = MediaUrl::parse("not a url");
        assert!(result.is_err(), "Expected Err for 'not a url', got Ok");
    }

    #[test]
    fn rejects_url_without_host() {
        // "https://" has no host — url crate returns Err("empty host")
        let result = MediaUrl::parse("https://");
        assert!(result.is_err(), "Expected Err for URL without host (https://)");
    }

    #[test]
    fn accepts_non_ascii_percent_encoded_path() {
        // Non-ASCII percent-encoded path — url crate handles this fine
        let result = MediaUrl::parse("https://example.com/v%C3%ADdeo");
        assert!(result.is_ok(), "Expected Ok for percent-encoded non-ASCII path");
    }

    #[test]
    fn as_str_returns_the_url_string() {
        let url = MediaUrl::parse("https://example.com/video").unwrap();
        assert!(url.as_str().starts_with("https://example.com"));
    }
}
