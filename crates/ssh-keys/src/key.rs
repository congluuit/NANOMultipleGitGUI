//! Generating, inspecting and importing OpenSSH keys.
//!
//! Only the OpenSSH private key format (`-----BEGIN OPENSSH PRIVATE KEY-----`, the default of
//! `ssh-keygen` since OpenSSH 7.8) is accepted. Legacy PEM and PuTTY keys are rejected with a
//! typed error that tells the user how to convert them.

use std::fmt;
use std::path::Path;

use serde::Serialize;
use ssh_key::public::KeyData;
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, EcdsaCurve, HashAlg, LineEnding, PrivateKey, PublicKey};
use zeroize::Zeroizing;

use crate::error::{Result, SshKeyError};

/// Private key files bigger than this are certainly not SSH keys.
const MAX_KEY_FILE_BYTES: u64 = 256 * 1024;

/// Key family, for display and policy (the app only generates [`KeyKind::Ed25519`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    Ed25519,
    Rsa,
    Ecdsa,
    Dsa,
    /// FIDO/U2F security key (`sk-ssh-ed25519@openssh.com`).
    SkEd25519,
    /// FIDO/U2F security key (`sk-ecdsa-sha2-nistp256@openssh.com`).
    SkEcdsa,
    Other,
}

/// Public facts about a key; never contains secret material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub kind: KeyKind,
    /// Algorithm name as written in the public key line, e.g. `ssh-ed25519`.
    pub algorithm: String,
    /// Key size in bits as printed by `ssh-keygen -l` (256 for Ed25519, modulus size for RSA).
    pub bits: Option<u32>,
    /// `SHA256:<base64 without padding>`, identical to `ssh-keygen -lf <key>`.
    pub fingerprint: String,
    /// OpenSSH public key line `<algorithm> <base64> [comment]` (what goes into
    /// `authorized_keys` or the provider's "SSH keys" page).
    pub public_key: String,
    pub comment: String,
    /// The private key is protected by a passphrase (always `false` for public keys).
    pub encrypted: bool,
}

/// Freshly generated key pair. `private_key` is the OpenSSH PEM text (encrypted when a
/// passphrase was given); it is zeroized on drop and redacted from `Debug`.
pub struct GeneratedKey {
    pub private_key: Zeroizing<String>,
    pub info: KeyInfo,
}

impl fmt::Debug for GeneratedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedKey")
            .field("private_key", &"<redacted>")
            .field("info", &self.info)
            .finish()
    }
}

/// How the `<key>.pub` next to an imported private key relates to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PublicFileCheck {
    /// No `.pub` file (the public key is embedded in the OpenSSH private key anyway).
    Missing,
    Matches,
    /// The `.pub` belongs to another key — the UI should warn, the private key wins.
    Mismatch,
    /// The `.pub` exists but cannot be parsed.
    Unreadable,
}

/// Result of importing an existing private key.
pub struct ImportedKey {
    /// Normalised OpenSSH PEM text (LF line endings, original encryption kept).
    pub private_key: Zeroizing<String>,
    pub info: KeyInfo,
    pub public_file: PublicFileCheck,
}

impl fmt::Debug for ImportedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportedKey")
            .field("private_key", &"<redacted>")
            .field("info", &self.info)
            .field("public_file", &self.public_file)
            .finish()
    }
}

/// Recognised private key container formats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PrivateKeyFormat {
    OpenSsh,
    /// `-----BEGIN <label>-----` with a label other than `OPENSSH PRIVATE KEY`.
    LegacyPem {
        label: String,
    },
    Putty,
    Unknown,
}

/// Detects the container format from the text, without decoding it.
pub fn detect_format(text: &str) -> PrivateKeyFormat {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with("PuTTY-User-Key-File-") {
        return PrivateKeyFormat::Putty;
    }
    for line in trimmed.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("-----BEGIN ") {
            let label = rest.trim_end_matches('-').trim();
            if label == "OPENSSH PRIVATE KEY" {
                return PrivateKeyFormat::OpenSsh;
            }
            if label.contains("PRIVATE KEY") {
                return PrivateKeyFormat::LegacyPem {
                    label: label.to_string(),
                };
            }
            return PrivateKeyFormat::Unknown;
        }
        if !line.is_empty() {
            break;
        }
    }
    PrivateKeyFormat::Unknown
}

/// Generates an Ed25519 key with the OS random generator.
///
/// `comment` is usually the account label or email. With a non-empty `passphrase` the key is
/// encrypted like `ssh-keygen` does (bcrypt-pbkdf, 16 rounds, aes256-ctr).
pub fn generate_ed25519(comment: &str, passphrase: Option<&str>) -> Result<GeneratedKey> {
    check_comment(comment)?;
    let mut key =
        PrivateKey::random(&mut OsRng, Algorithm::Ed25519).map_err(SshKeyError::invalid)?;
    key.set_comment(comment);
    let info = key_info(key.public_key(), false);
    let key = match passphrase.filter(|p| !p.is_empty()) {
        Some(pass) => key
            .encrypt(&mut OsRng, pass.as_bytes())
            .map_err(SshKeyError::invalid)?,
        None => key,
    };
    let private_key = key
        .to_openssh(LineEnding::LF)
        .map_err(SshKeyError::invalid)?;
    Ok(GeneratedKey {
        private_key,
        info: KeyInfo {
            encrypted: key.is_encrypted(),
            ..info
        },
    })
}

/// Reads the public facts of an OpenSSH private key. Works for encrypted keys too: the
/// OpenSSH format stores the public key in clear text.
pub fn inspect_private_key(text: &str) -> Result<KeyInfo> {
    let key = parse_private(text)?;
    Ok(key_info(key.public_key(), key.is_encrypted()))
}

/// Checks a passphrase. Unencrypted keys accept any passphrase (there is nothing to check).
pub fn verify_passphrase(text: &str, passphrase: &str) -> Result<()> {
    let key = parse_private(text)?;
    if key.is_encrypted() {
        decrypt(&key, passphrase)?;
    }
    Ok(())
}

/// Imports an OpenSSH private key from text.
///
/// An encrypted key requires `passphrase` (it is checked, the key stays encrypted in
/// [`ImportedKey::private_key`]); for unencrypted keys `passphrase` is ignored.
pub fn import_private_key(text: &str, passphrase: Option<&str>) -> Result<ImportedKey> {
    let key = parse_private(text)?;
    let info = if key.is_encrypted() {
        match passphrase {
            Some(pass) => {
                let decrypted = decrypt(&key, pass)?;
                validate_unencrypted(&decrypted)?;
                // The comment lives in the encrypted section.
                key_info(decrypted.public_key(), true)
            }
            None => return Err(SshKeyError::PassphraseRequired),
        }
    } else {
        // Decoding validated the structure; make sure the key pair is consistent.
        validate_unencrypted(&key)?;
        key_info(key.public_key(), false)
    };
    let private_key = key
        .to_openssh(LineEnding::LF)
        .map_err(SshKeyError::invalid)?;
    Ok(ImportedKey {
        private_key,
        info,
        public_file: PublicFileCheck::Missing,
    })
}

/// Takes the comment of a matching public key line when `info` has none (encrypted OpenSSH
/// keys keep their comment inside the encrypted section). Returns whether `public_line`
/// belongs to the same key.
pub(crate) fn merge_public_comment(info: &mut KeyInfo, public_line: &str) -> bool {
    match parse_public_key(public_line) {
        Ok(public) if public.fingerprint == info.fingerprint => {
            if info.comment.is_empty() && !public.comment.is_empty() {
                info.comment = public.comment;
                info.public_key = public.public_key;
            }
            true
        }
        _ => false,
    }
}

/// Imports a private key file and compares it with `<path>.pub` when present.
pub fn import_private_key_file(path: &Path, passphrase: Option<&str>) -> Result<ImportedKey> {
    let text = read_key_file(path)?;
    let mut imported = import_private_key(&text, passphrase)?;
    imported.public_file = check_public_file(path, &mut imported.info);
    Ok(imported)
}

/// Reads a (small) key file into zeroizing memory.
pub fn read_key_file(path: &Path) -> Result<Zeroizing<String>> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_KEY_FILE_BYTES {
        return Err(SshKeyError::NotAPrivateKey);
    }
    let bytes = Zeroizing::new(std::fs::read(path)?);
    match std::str::from_utf8(&bytes) {
        Ok(s) => Ok(Zeroizing::new(s.to_string())),
        Err(_) => Err(SshKeyError::NotAPrivateKey),
    }
}

/// Parses an OpenSSH public key line (`ssh-ed25519 AAAA… comment`).
pub fn parse_public_key(line: &str) -> Result<KeyInfo> {
    let key = PublicKey::from_openssh(line.trim()).map_err(SshKeyError::invalid)?;
    Ok(key_info(&key, false))
}

/// `SHA256:…` fingerprint of an OpenSSH public key line.
pub fn fingerprint(public_key_line: &str) -> Result<String> {
    parse_public_key(public_key_line).map(|info| info.fingerprint)
}

fn check_comment(comment: &str) -> Result<()> {
    if comment.contains(['\n', '\r', '\0']) {
        return Err(SshKeyError::InvalidComment);
    }
    Ok(())
}

pub(crate) fn parse_private(text: &str) -> Result<PrivateKey> {
    match detect_format(text) {
        PrivateKeyFormat::OpenSsh => {}
        PrivateKeyFormat::LegacyPem { label } => return Err(SshKeyError::LegacyPem { label }),
        PrivateKeyFormat::Putty => return Err(SshKeyError::PuttyFormat),
        PrivateKeyFormat::Unknown => return Err(SshKeyError::NotAPrivateKey),
    }
    // The decoder wants LF line endings and no stray whitespace around the armour.
    let normalised = Zeroizing::new(
        text.trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    );
    PrivateKey::from_openssh(normalised.as_bytes()).map_err(SshKeyError::invalid)
}

fn decrypt(key: &PrivateKey, passphrase: &str) -> Result<PrivateKey> {
    match key.decrypt(passphrase.as_bytes()) {
        Ok(k) => Ok(k),
        // A wrong passphrase yields garbage, detected through the check integers.
        Err(ssh_key::Error::Crypto) | Err(ssh_key::Error::FormatEncoding) => {
            Err(SshKeyError::WrongPassphrase)
        }
        Err(e) => Err(SshKeyError::invalid(e)),
    }
}

fn validate_unencrypted(key: &PrivateKey) -> Result<()> {
    if let ssh_key::private::KeypairData::Ed25519(pair) = key.key_data() {
        let derived = ssh_key::public::Ed25519PublicKey::from(&pair.private);
        if derived != pair.public {
            return Err(SshKeyError::InvalidKey(
                "private and public halves do not match".into(),
            ));
        }
    }
    Ok(())
}

fn check_public_file(private_path: &Path, info: &mut KeyInfo) -> PublicFileCheck {
    let mut pub_path = private_path.as_os_str().to_owned();
    pub_path.push(".pub");
    let Ok(text) = std::fs::read_to_string(&pub_path) else {
        return PublicFileCheck::Missing;
    };
    let line = text.lines().next().unwrap_or("");
    if parse_public_key(line).is_err() {
        PublicFileCheck::Unreadable
    } else if merge_public_comment(info, line) {
        PublicFileCheck::Matches
    } else {
        PublicFileCheck::Mismatch
    }
}

pub(crate) fn key_info(key: &PublicKey, encrypted: bool) -> KeyInfo {
    let algorithm = key.algorithm();
    let kind = match &algorithm {
        Algorithm::Ed25519 => KeyKind::Ed25519,
        Algorithm::Rsa { .. } => KeyKind::Rsa,
        Algorithm::Ecdsa { .. } => KeyKind::Ecdsa,
        Algorithm::Dsa => KeyKind::Dsa,
        Algorithm::SkEd25519 => KeyKind::SkEd25519,
        Algorithm::SkEcdsaSha2NistP256 => KeyKind::SkEcdsa,
        _ => KeyKind::Other,
    };
    let bits = match key.key_data() {
        KeyData::Ed25519(_) | KeyData::SkEd25519(_) | KeyData::SkEcdsaSha2NistP256(_) => Some(256),
        KeyData::Ecdsa(k) => Some(match k.curve() {
            EcdsaCurve::NistP256 => 256,
            EcdsaCurve::NistP384 => 384,
            EcdsaCurve::NistP521 => 521,
        }),
        KeyData::Rsa(k) => k.n.as_positive_bytes().map(mpint_bits),
        KeyData::Dsa(k) => k.p.as_positive_bytes().map(mpint_bits),
        _ => None,
    };
    KeyInfo {
        kind,
        algorithm: algorithm.as_str().to_string(),
        bits,
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        public_key: key
            .to_openssh()
            .unwrap_or_else(|_| String::from("<unencodable>")),
        comment: key.comment().to_string(),
        encrypted,
    }
}

fn mpint_bits(bytes: &[u8]) -> u32 {
    let bytes = match bytes.iter().position(|b| *b != 0) {
        Some(i) => &bytes[i..],
        None => return 0,
    };
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX / 8);
    len * 8 - bytes[0].leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats() {
        assert_eq!(
            detect_format("-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n"),
            PrivateKeyFormat::OpenSsh
        );
        assert_eq!(
            detect_format("\r\n-----BEGIN RSA PRIVATE KEY-----\r\nabc\r\n"),
            PrivateKeyFormat::LegacyPem {
                label: "RSA PRIVATE KEY".into()
            }
        );
        assert_eq!(
            detect_format("-----BEGIN ENCRYPTED PRIVATE KEY-----\n"),
            PrivateKeyFormat::LegacyPem {
                label: "ENCRYPTED PRIVATE KEY".into()
            }
        );
        assert_eq!(
            detect_format("PuTTY-User-Key-File-3: ssh-ed25519\n"),
            PrivateKeyFormat::Putty
        );
        assert_eq!(
            detect_format("ssh-ed25519 AAAA x"),
            PrivateKeyFormat::Unknown
        );
        assert_eq!(
            detect_format("-----BEGIN PUBLIC KEY-----\n"),
            PrivateKeyFormat::Unknown
        );
    }

    #[test]
    fn mpint_bit_count() {
        assert_eq!(mpint_bits(&[0x00, 0x80, 0x00]), 16);
        assert_eq!(mpint_bits(&[0x01, 0x00]), 9);
        assert_eq!(mpint_bits(&[]), 0);
    }

    #[test]
    fn rejects_comment_with_newline() {
        assert!(matches!(
            generate_ed25519("a\nb", None),
            Err(SshKeyError::InvalidComment)
        ));
    }
}
