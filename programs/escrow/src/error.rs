use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("The cancel time lock has not elapsed — please wait five minutes after creation")]
    TimeLockActive,
}
