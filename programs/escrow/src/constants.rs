use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

/// How long an escrow must live before its maker may cancel it.
///
/// A maker who can cancel at any instant is holding a free option: they keep the
/// upside and withdraw the moment the market moves against them, and a taker who
/// has already paid a fee eats the failure. A minimum lifetime makes the offer mean
/// what it says for as long as it stands.
#[constant]
pub const CANCEL_DELAY_SECONDS: i64 = 300; // 5 minutes
