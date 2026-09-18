use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The escrow has expired")]
    EscrowExpired,
    #[msg("The receive amount must be greater than zero")]
    InvalidReceiveAmount,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
}
