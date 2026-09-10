use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Escrow cancellation is locked until five minutes after creation")]
    TimeLockActive,
}
