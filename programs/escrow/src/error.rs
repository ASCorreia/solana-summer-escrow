use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The time lock has not elapsed")]
    TimeLockActive
}
