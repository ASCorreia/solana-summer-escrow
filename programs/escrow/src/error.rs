use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("Threshold Time Has Not Passed Yet")]
    TimeThresholdHasNotPassed,
}
