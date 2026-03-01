use rand::Rng;

/// Hash a password using bcrypt.
pub fn hash_password(password: &str) -> Result<String, bcrypt::BcryptError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
}

/// Verify a password against a bcrypt hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Generate a random session token.
pub fn generate_session_token() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..64)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect()
}

/// Extract a Bearer token from an Authorization header value.
/// Returns None if the token part is empty.
pub fn extract_bearer_token(auth_header: &str) -> Option<&str> {
    let token = auth_header.strip_prefix("Bearer ")?;
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Extract session token from a cookie header value.
pub fn extract_session_from_cookies(cookie_header: &str) -> Option<&str> {
    cookie_header
        .split(';')
        .find_map(|c| c.trim().strip_prefix("myground_session="))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_password() {
        let hash = hash_password("testpass123").unwrap();
        assert!(verify_password("testpass123", &hash));
        assert!(!verify_password("wrongpass", &hash));
    }

    #[test]
    fn session_token_is_64_chars() {
        let token = generate_session_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn session_tokens_are_unique() {
        let a = generate_session_token();
        let b = generate_session_token();
        assert_ne!(a, b);
    }

    #[test]
    fn extract_bearer_token_valid() {
        assert_eq!(extract_bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(
            extract_bearer_token("Bearer myground_ak_longkey"),
            Some("myground_ak_longkey")
        );
    }

    #[test]
    fn extract_bearer_token_invalid() {
        assert_eq!(extract_bearer_token("Basic abc123"), None);
        assert_eq!(extract_bearer_token("bearer abc123"), None);
        assert_eq!(extract_bearer_token(""), None);
        assert_eq!(extract_bearer_token("BearerNoSpace"), None);
        assert_eq!(extract_bearer_token("Bearer "), None); // empty token
    }

    #[test]
    fn extract_session_from_cookie_string() {
        assert_eq!(
            extract_session_from_cookies("myground_session=abc123; other=val"),
            Some("abc123")
        );
        assert_eq!(
            extract_session_from_cookies("other=val; myground_session=xyz"),
            Some("xyz")
        );
        assert_eq!(
            extract_session_from_cookies("other=val"),
            None
        );
    }
}
