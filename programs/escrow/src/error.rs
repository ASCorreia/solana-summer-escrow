use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Escrow is time-locked and cannot be cancelled yet")]
    TimeLockActive,
}
