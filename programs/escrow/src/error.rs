use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("The offer cannot be cancelled until five minutes after it was created")]
    TimeLockActive,
}
