use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum ColdError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    Overflow = 4,
    InvalidHotBps = 5,
    InvalidRebalanceThreshold = 6,
    ColdSignersEmpty = 7,
    InvalidColdThreshold = 8,
    DuplicateColdSigner = 9,
    SweepExists = 10,
    NoSweepPending = 11,
    NotColdSigner = 12,
    AlreadyApproved = 13,
    InsufficientApprovals = 14,
    AmountNotPositive = 15,
}
