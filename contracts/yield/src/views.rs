use soroban_sdk::Env;

/// All supported features bitmask for the yield contract.
pub const ALL_CAPABILITIES: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// Returns the u64 bitmap of supported features on the yield contract
/// so clients can detect capability deltas.
///
/// # Arguments
/// * `_env` - The Soroban environment.
///
/// # Returns
/// A `u64` bitmask representing supported capabilities.
pub fn capabilities(_env: &Env) -> u64 {
    ALL_CAPABILITIES
}
