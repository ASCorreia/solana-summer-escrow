use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("The escrow cannot be cancelled before the time lock expires")]
    TimeLockActive,
}
