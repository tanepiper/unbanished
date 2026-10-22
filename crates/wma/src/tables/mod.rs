//! Codec constant tables for WMA v1/v2.
//!
//! These are bitstream-defining constants (VLC codewords, run/level maps, band layouts) that any
//! compatible decoder must reproduce exactly. They were taken from the MIT-licensed
//! `oxideav-wma` 0.0.4 crate (Copyright (c) 2026 Karpelès Lab Inc., MIT License), whose
//! provenance notes state they were extracted from the vendor WMA decoder's data sections.
//! See THIRD_PARTY.md. Regenerate with
//! `tools/gen-wma-tables.py <oxideav-wma-0.0.4 dir> crates/wma/src/tables`; do not edit by hand.
//!
//! VLC entries are `(length, codeword)`, codewords read MSB first. Run/level maps give
//! `(run, |level|)` for coefficient symbols `2..`; symbol 0 is escape, symbol 1 end-of-block.

mod coef0;
mod coef1;
mod coef2;
mod coef3;
mod coef4;
mod coef5;
mod exponents;

pub use coef0::*;
pub use coef1::*;
pub use coef2::*;
pub use coef3::*;
pub use coef4::*;
pub use coef5::*;
pub use exponents::*;
