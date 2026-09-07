use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Cannot cancel escrow yet!")]
    RejectedCancel,
}
