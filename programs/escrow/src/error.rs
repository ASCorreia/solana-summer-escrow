use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    #[msg("The Time Lock has not elapsed")]
    EscrowCancelDelay,
}
