//! Secret strings (access/refresh tokens, device codes).

use std::fmt;

use zeroize::Zeroizing;

/// A token that is wiped from memory on drop and never printed.
///
/// Deliberately not `Serialize`/`Display`: the only way to read it is [`SecretToken::expose`],
/// so every place a token leaves the process is easy to audit.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(Zeroizing<String>);

impl SecretToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// The raw token. Only pass it to an HTTP header or the keychain.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl From<String> for SecretToken {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<Zeroizing<String>> for SecretToken {
    fn from(value: Zeroizing<String>) -> Self {
        Self(value)
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_token() {
        let t = SecretToken::new("ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(format!("{t:?}"), "SecretToken([REDACTED])");
        assert_eq!(t.expose(), "ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        assert!(SecretToken::new("  ").is_empty());
    }
}
