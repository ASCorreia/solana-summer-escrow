use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Custom error message")]
    CustomError,
    // Returned by cancel when the timelock has not elapsed yet. The #[msg] text is what clients see in logs, so it explains the rejection plainly (Implemented in cancel.rs -> handler)
    #[msg("Escrow is still timelocked; cancellation is not allowed yet")]
    EscrowStillLocked,
}
