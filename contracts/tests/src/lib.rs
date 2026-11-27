//! Cross-contract access-control matrix crate.
//!
//! The integration suite lives in `tests/auth_matrix.rs`. This crate is
//! `rlib`-only so the four contract crates can be linked together without the
//! duplicate exported-symbol errors that occur when several `cdylib`s are
//! linked into one test binary.
