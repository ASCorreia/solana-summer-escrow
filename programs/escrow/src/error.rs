use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Time Lock is Activated")]
    TimeLockActive,
    #[msg("Arithmetic overflow")]
    Overflow,
}
