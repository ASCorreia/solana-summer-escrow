use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

// How long an escrow stays locked before the maker can cancel it
#[constant]
pub const CANCEL_TIMELOCK_SECONDS: i64 = 5 * 60;
