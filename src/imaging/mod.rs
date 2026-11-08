//! Image-domain types and file/archive loading helpers.
//!
//! Decoding implementations live in `crate::decoder`; this module owns the
//! shared image model, archive entries, discovery, sorting, and loading glue.

pub mod archive;
pub mod core;
pub mod entry;
pub mod helpers;
pub mod loader;
pub mod sort;
