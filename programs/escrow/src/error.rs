use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Escrow cannot be cancelled yet: the time lock has not elapsed")]
    TimeLockActive,
}
