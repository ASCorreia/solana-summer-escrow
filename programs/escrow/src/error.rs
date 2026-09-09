use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("The escrow cannot be cancelled until the time lock has elapsed")]
    TimeLockActive,
}
