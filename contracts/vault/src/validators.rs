//! Bounded string validators for user-supplied vault metadata.
//!
//! Metadata is stored as contract state and later consumed by wallets,
//! indexers, and off-chain services. Accepting arbitrary Unicode would allow
//! invisible controls, bidi overrides, and homoglyph confusables to make two
//! different byte strings appear identical. The on-chain policy is therefore
//! intentionally narrow: metadata identifiers and values must be visible ASCII.
//! ASCII is already NFC-normalized, so stored values have one canonical byte
//! representation without pulling large Unicode tables into the WASM.
//!
//! The validator is O(n) over the input length, with `n` capped at the
//! contract's existing 256-byte metadata limit.

use soroban_sdk::String;

/// Maximum byte length accepted by [`normalize_visible_ascii`].
pub const MAX_VALIDATED_STRING_LEN: u32 = 256;

/// Normalize a bounded metadata string to its canonical on-chain form.
///
/// Accepted strings are non-empty visible ASCII with no leading or trailing
/// spaces. This rejects C0/DEL controls, zero-width and bidi controls, and
/// Unicode confusables by construction. Because ASCII has no decomposed forms,
/// the returned value is NFC-normalized and byte-stable.
pub fn normalize_visible_ascii(s: &String) -> Result<[u8; MAX_VALIDATED_STRING_LEN as usize], ()> {
    let len = s.len();
    if len == 0 || len > MAX_VALIDATED_STRING_LEN {
        return Err(());
    }

    let mut buf = [0u8; MAX_VALIDATED_STRING_LEN as usize];
    s.copy_into_slice(&mut buf[..len as usize]);
    let bytes = &buf[..len as usize];

    if bytes[0] == b' ' || bytes[len as usize - 1] == b' ' {
        return Err(());
    }

    for &b in bytes {
        if !(0x20..=0x7e).contains(&b) {
            return Err(());
        }
    }

    Ok(buf)
}

/// Return whether a bounded metadata string is accepted by the on-chain policy.
pub fn is_visible_ascii_metadata(s: &String) -> bool {
    normalize_visible_ascii(s).is_ok()
}
