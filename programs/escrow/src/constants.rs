use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

/// How long a maker must wait before they are allowed to cancel their own offer.
///
/// Exported to the IDL so clients show the same countdown the program enforces.
#[constant]
pub const CANCEL_DELAY_SECONDS: i64 = 300; // 5 minutes
