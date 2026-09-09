use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Need to wait 5 mins b4 cancellation")]
    TimeLocked,
}
