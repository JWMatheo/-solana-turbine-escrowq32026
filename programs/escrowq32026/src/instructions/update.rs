use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{error::ErrorCode, state::Escrow, ESCROW_SEED};

#[derive(Accounts)]
pub struct Update<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    #[account(mint::token_program = token_program)]
    pub mint_a: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_a: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        has_one = maker,
        has_one = mint_a,
        seeds = [ESCROW_SEED, maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Box<Account<'info, Escrow>>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> Update<'info> {
    pub fn update(&mut self, deposit: u64, receive: u64) -> Result<()> {
        require!(receive > 0, ErrorCode::InvalidReceiveAmount);

        let current_deposit = self.vault.amount;

        if deposit > current_deposit {
            let additional_deposit = deposit
                .checked_sub(current_deposit)
                .ok_or(ErrorCode::ArithmeticOverflow)?;
            let transfer_accounts = TransferChecked {
                from: self.maker_ata_a.to_account_info(),
                mint: self.mint_a.to_account_info(),
                to: self.vault.to_account_info(),
                authority: self.maker.to_account_info(),
            };
            let cpi_context = CpiContext::new(self.token_program.key(), transfer_accounts);
            transfer_checked(cpi_context, additional_deposit, self.mint_a.decimals)?;
        } else if deposit < current_deposit {
            let refund_amount = current_deposit
                .checked_sub(deposit)
                .ok_or(ErrorCode::ArithmeticOverflow)?;
            let signer_seeds: [&[&[u8]]; 1] = [&[
                ESCROW_SEED,
                self.maker.key.as_ref(),
                &self.escrow.seed.to_le_bytes()[..],
                &[self.escrow.bump],
            ]];
            let transfer_accounts = TransferChecked {
                from: self.vault.to_account_info(),
                mint: self.mint_a.to_account_info(),
                to: self.maker_ata_a.to_account_info(),
                authority: self.escrow.to_account_info(),
            };
            let cpi_context = CpiContext::new_with_signer(
                self.token_program.key(),
                transfer_accounts,
                &signer_seeds,
            );
            transfer_checked(cpi_context, refund_amount, self.mint_a.decimals)?;
        }

        self.escrow.receive = receive;
        Ok(())
    }
}
