use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Cannot cancel yet , the time lock hasn't expired")]
    TimeLockActive,
}
