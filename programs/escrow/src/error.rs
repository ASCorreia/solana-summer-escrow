use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Errir with TimeLockActive")]
    CustomError,
    #[msg("Timestamp overflow computing the cancel unlock time")]
    Overflow,
}
