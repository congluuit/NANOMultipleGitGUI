//! The encrypted envelope shared by `.nanomgg-accounts` and `.nanomgg-backup` files:
//!
//! ```json
//! { "format": "nanomgg-backup", "version": 1, "kind": "accounts" | "full",
//!   "createdAt": <unix ms>, "appVersion": "1.0.0",
//!   "kdf": { "algorithm": "argon2id", "memoryKib": 65536, "iterations": 3, "parallelism": 1,
//!            "salt": "<b64, 16 bytes>" },
//!   "cipher": "xchacha20poly1305", "nonce": "<b64, 24 bytes>", "data": "<b64 ciphertext>" }
//! ```
//!
//! The key is Argon2id(password, salt); everything but `data` is serialized again after parsing
//! and authenticated as AAD, so changing any header field (kind, KDF cost, version…) makes
//! decryption fail exactly like a wrong password does. The two cannot be told apart.

use std::io::Read;
use std::path::Path;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{BackupError, Result};

pub const FORMAT: &str = "nanomgg-backup";
pub const FORMAT_VERSION: u32 = 1;
pub const CIPHER: &str = "xchacha20poly1305";
const KDF_ALGORITHM: &str = "argon2id";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
/// Largest file read back (both kinds).
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub const MIN_PASSWORD_CHARS: usize = 8;

/// What the file holds. Stored in clear (and authenticated) so the UI can refuse the wrong
/// kind of file before asking for the password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BackupKind {
    /// Account configuration (Phase 6.3).
    Accounts,
    /// The whole app (Phase 6.6).
    Full,
}

/// Argon2id cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfCost {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfCost {
    /// 64 MiB, 3 passes, 1 lane: well under a second on a desktop (release build).
    pub const DESKTOP: Self = Self {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
    };
    /// Instant, for tests only.
    #[doc(hidden)]
    pub const TEST: Self = Self {
        memory_kib: 64,
        iterations: 1,
        parallelism: 1,
    };
}

/// Upper bounds accepted when reading a file, so a crafted header cannot make the app
/// allocate gigabytes or spin for minutes.
const MAX_MEMORY_KIB: u32 = 1024 * 1024;
const MAX_ITERATIONS: u32 = 16;
const MAX_PARALLELISM: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KdfParams {
    algorithm: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
}

/// Everything but `data`; its JSON is the AAD.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    format: String,
    version: u32,
    kind: BackupKind,
    created_at: i64,
    app_version: String,
    kdf: KdfParams,
    cipher: String,
    nonce: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    #[serde(flatten)]
    header: Header,
    data: String,
}

/// Facts readable without the password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub kind: BackupKind,
    /// Unix milliseconds.
    pub created_at: i64,
    pub app_version: String,
}

pub fn check_password(password: &str) -> Result<()> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(BackupError::WeakPassword(MIN_PASSWORD_CHARS));
    }
    Ok(())
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf)
        .map_err(|e| BackupError::Io(std::io::Error::other(format!("no randomness: {e}"))))?;
    Ok(buf)
}

fn derive_key(password: &str, kdf: &KdfParams) -> Result<Zeroizing<[u8; 32]>> {
    let bad = || BackupError::Corrupt("unsupported key derivation parameters".into());
    if kdf.algorithm != KDF_ALGORITHM
        || kdf.memory_kib > MAX_MEMORY_KIB
        || !(1..=MAX_ITERATIONS).contains(&kdf.iterations)
        || !(1..=MAX_PARALLELISM).contains(&kdf.parallelism)
    {
        return Err(bad());
    }
    let salt = B64.decode(&kdf.salt).map_err(|_| bad())?;
    if salt.len() != SALT_LEN {
        return Err(bad());
    }
    let params = Params::new(kdf.memory_kib, kdf.iterations, kdf.parallelism, Some(32))
        .map_err(|_| bad())?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), &salt, key.as_mut())
        .map_err(|_| bad())?;
    Ok(key)
}

fn aad(header: &Header) -> Result<Vec<u8>> {
    serde_json::to_vec(header).map_err(|e| BackupError::Corrupt(e.to_string()))
}

/// Encrypts `plaintext` into a complete file.
pub fn seal(
    kind: BackupKind,
    plaintext: &[u8],
    password: &str,
    app_version: &str,
    cost: KdfCost,
) -> Result<Vec<u8>> {
    check_password(password)?;
    let salt = random::<SALT_LEN>()?;
    let nonce = random::<NONCE_LEN>()?;
    let header = Header {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        kind,
        created_at: crate::now_ms(),
        app_version: app_version.into(),
        kdf: KdfParams {
            algorithm: KDF_ALGORITHM.into(),
            memory_kib: cost.memory_kib,
            iterations: cost.iterations,
            parallelism: cost.parallelism,
            salt: B64.encode(salt),
        },
        cipher: CIPHER.into(),
        nonce: B64.encode(nonce),
    };
    let key = derive_key(password, &header.kdf)?;
    let aad = aad(&header)?;
    let data = XChaCha20Poly1305::new(Key::from_slice(key.as_ref()))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| BackupError::Corrupt("encryption failed".into()))?;
    let envelope = Envelope {
        header,
        data: B64.encode(data),
    };
    serde_json::to_vec_pretty(&envelope).map_err(|e| BackupError::Corrupt(e.to_string()))
}

fn parse(bytes: &[u8]) -> Result<Envelope> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(BackupError::TooLarge {
            limit: MAX_FILE_BYTES,
        });
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| BackupError::NotABackup)?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(BackupError::NotABackup);
    }
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(BackupError::NotABackup)?;
    if version > u64::from(FORMAT_VERSION) {
        return Err(BackupError::NewerFormat(
            u32::try_from(version).unwrap_or(u32::MAX),
        ));
    }
    let envelope: Envelope =
        serde_json::from_value(value).map_err(|e| BackupError::Corrupt(e.to_string()))?;
    if envelope.header.cipher != CIPHER {
        return Err(BackupError::Corrupt(format!(
            "unsupported cipher `{}`",
            envelope.header.cipher
        )));
    }
    Ok(envelope)
}

fn info_of(header: &Header) -> BackupInfo {
    BackupInfo {
        kind: header.kind,
        created_at: header.created_at,
        app_version: header.app_version.clone(),
    }
}

/// Header facts without the password (the values are only trustworthy after [`open`]).
pub fn peek(bytes: &[u8]) -> Result<BackupInfo> {
    Ok(info_of(&parse(bytes)?.header))
}

/// Decrypts a file. `expected` refuses the other kind before the (slow) key derivation.
pub fn open(
    bytes: &[u8],
    password: &str,
    expected: Option<BackupKind>,
) -> Result<(BackupInfo, Zeroizing<Vec<u8>>)> {
    let envelope = parse(bytes)?;
    let header = &envelope.header;
    if let Some(expected) = expected
        && header.kind != expected
    {
        return Err(BackupError::WrongKind {
            found: header.kind,
            expected,
        });
    }
    let nonce = B64
        .decode(&header.nonce)
        .ok()
        .filter(|n| n.len() == NONCE_LEN)
        .ok_or_else(|| BackupError::Corrupt("invalid nonce".into()))?;
    let data = B64
        .decode(&envelope.data)
        .map_err(|_| BackupError::Corrupt("invalid data".into()))?;
    let key = derive_key(password, &header.kdf)?;
    let aad = aad(header)?;
    let plain = XChaCha20Poly1305::new(Key::from_slice(key.as_ref()))
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &data,
                aad: &aad,
            },
        )
        .map_err(|_| BackupError::WrongPassword)?;
    Ok((info_of(header), Zeroizing::new(plain)))
}

/// Reads a backup file, refusing anything over [`MAX_FILE_BYTES`].
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(BackupError::TooLarge {
            limit: MAX_FILE_BYTES,
        });
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(BackupError::TooLarge {
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PW: &str = "correct horse";

    fn sealed() -> Vec<u8> {
        seal(
            BackupKind::Accounts,
            b"secret payload",
            PW,
            "1.0.0",
            KdfCost::TEST,
        )
        .unwrap()
    }

    #[test]
    fn round_trip() {
        let bytes = sealed();
        let (info, plain) = open(&bytes, PW, Some(BackupKind::Accounts)).unwrap();
        assert_eq!(plain.as_slice(), b"secret payload");
        assert_eq!(info.kind, BackupKind::Accounts);
        assert_eq!(info.app_version, "1.0.0");
        assert!(info.created_at > 0);
        assert_eq!(peek(&bytes).unwrap(), info);
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("secret payload"));
        assert!(text.contains("\"format\": \"nanomgg-backup\""), "{text}");
        assert!(text.contains("\"cipher\": \"xchacha20poly1305\""));
    }

    #[test]
    fn same_input_gives_different_files() {
        assert_ne!(sealed(), sealed(), "fresh salt and nonce every time");
    }

    #[test]
    fn wrong_password_is_refused() {
        let err = open(&sealed(), "wrong password", None).unwrap_err();
        assert!(matches!(err, BackupError::WrongPassword), "{err}");
    }

    #[test]
    fn short_password_is_refused_when_sealing() {
        let err = seal(BackupKind::Full, b"x", "1234567", "1", KdfCost::TEST).unwrap_err();
        assert!(matches!(err, BackupError::WeakPassword(8)));
        // Eight characters, counted as characters (not bytes).
        seal(BackupKind::Full, b"x", "mậtkhẩu1", "1", KdfCost::TEST).unwrap();
    }

    fn edit(bytes: &[u8], f: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut v: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        f(&mut v);
        serde_json::to_vec(&v).unwrap()
    }

    #[test]
    fn any_header_change_breaks_decryption() {
        let bytes = sealed();
        type Edit = Box<dyn FnOnce(&mut serde_json::Value)>;
        let edits: Vec<Edit> = vec![
            Box::new(|v| v["appVersion"] = "1.0.1".into()),
            Box::new(|v| v["createdAt"] = 1.into()),
            Box::new(|v| v["kind"] = "full".into()),
            Box::new(|v| v["kdf"]["iterations"] = 2.into()),
        ];
        for f in edits {
            let changed = edit(&bytes, f);
            let err = open(&changed, PW, None).unwrap_err();
            assert!(matches!(err, BackupError::WrongPassword), "{err}");
        }
    }

    #[test]
    fn one_flipped_ciphertext_byte_breaks_decryption() {
        let bytes = sealed();
        let changed = edit(&bytes, |v| {
            let mut data = B64.decode(v["data"].as_str().unwrap()).unwrap();
            data[3] ^= 1;
            v["data"] = B64.encode(data).into();
        });
        assert!(matches!(
            open(&changed, PW, None),
            Err(BackupError::WrongPassword)
        ));
    }

    #[test]
    fn wrong_kind_is_refused_before_the_password() {
        let err = open(&sealed(), "whatever", Some(BackupKind::Full)).unwrap_err();
        assert!(matches!(
            err,
            BackupError::WrongKind {
                found: BackupKind::Accounts,
                expected: BackupKind::Full
            }
        ));
    }

    #[test]
    fn foreign_files_are_reported_clearly() {
        for junk in [&b"hello"[..], b"{}", b"{\"format\":\"other\"}", b"\x00\x01"] {
            assert!(matches!(peek(junk), Err(BackupError::NotABackup)));
        }
        let newer = edit(&sealed(), |v| v["version"] = 2.into());
        assert!(matches!(peek(&newer), Err(BackupError::NewerFormat(2))));
        let broken = edit(&sealed(), |v| v["nonce"] = 5.into());
        assert!(matches!(
            open(&broken, PW, None),
            Err(BackupError::Corrupt(_))
        ));
        let costly = edit(&sealed(), |v| v["kdf"]["memoryKib"] = (4u64 << 20).into());
        assert!(matches!(
            open(&costly, PW, None),
            Err(BackupError::Corrupt(_))
        ));
    }

    #[test]
    fn oversized_files_are_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.nanomgg-backup");
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MAX_FILE_BYTES + 1).unwrap();
        drop(f);
        assert!(matches!(
            read_file(&path),
            Err(BackupError::TooLarge { .. })
        ));
        std::fs::write(&path, sealed()).unwrap();
        assert!(peek(&read_file(&path).unwrap()).is_ok());
    }
}
