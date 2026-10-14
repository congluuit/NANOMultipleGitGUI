//! Random, never-reused identifiers.
//!
//! Account ids end up in keychain entry names (`nanomgg:<account_id>:token`). Integer ids would
//! be reused after the database is reset, silently pairing an old keychain token with a new
//! account; 64 random bits make that practically impossible.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// `"<prefix>_<16 hex digits>"`, e.g. `acc_3f9c0a7d12e4b6f0`.
pub fn new_id(prefix: &str) -> String {
    let mut buf = [0u8; 8];
    let value = match getrandom::fill(&mut buf) {
        Ok(()) => u64::from_le_bytes(buf),
        // Practically unreachable; still unique within the process and across restarts.
        Err(_) => fallback(),
    };
    format!("{prefix}_{value:016x}")
}

fn fallback() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // SplitMix64 finalizer to spread the bits.
    let mut x =
        nanos ^ (u64::from(std::process::id()) << 32) ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_and_unique() {
        let a = new_id("acc");
        let b = new_id("acc");
        assert!(a.starts_with("acc_") && a.len() == 20, "{a}");
        assert_ne!(a, b);
        assert_ne!(fallback(), fallback());
    }
}
