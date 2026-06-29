use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{error::ErrorCode, Escrow, CANCEL_TIMELOCK_SECONDS};

#[derive(Accounts)]
pub struct Cancel<'info> {
    // mut required: `close = maker` and close_account both deposit lamports into maker
    #[account(mut)]
    pub maker: Signer<'info>,
    #[account(
        mut,
        close = maker,
        seeds = [b"escrow", maker.key().as_ref(), &escrow.seed.to_le_bytes()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
    )]
    pub maker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = escrow.mint_a,
        associated_token::authority = escrow,
    )]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<Cancel>) -> Result<()> {
    // Read "now" from the Clock sysvar and compare it against the created_at we stored during `make`. Both are i64 seconds, so the difference is the escrow's age in seconds
    let now = Clock::get()?.unix_timestamp;
    let elapsed = now - ctx.accounts.escrow.created_at;

    // require! returns the error (and logs its #[msg]) when the condition is false. We check before moving any tokens, so a rejected cancel leaves the vault and escrow untouched: nothing happens until the lock has elapsed
    require!(
        elapsed >= CANCEL_TIMELOCK_SECONDS,
        ErrorCode::EscrowStillLocked
    );

    let cpi_accounts = anchor_spl::token_interface::TransferChecked {
        from: ctx.accounts.vault_a.to_account_info(),
        mint: ctx.accounts.mint_a.to_account_info(),
        to: ctx.accounts.maker_ata_a.to_account_info(),
        authority: ctx.accounts.escrow.to_account_info(),
    };
    let seeds = &[ // This must be matched with the close_vault function below!
        &b"escrow"[..],
        ctx.accounts.escrow.maker.as_ref(),
        &ctx.accounts.escrow.seed.to_le_bytes(),
        &[ctx.accounts.escrow.bump],
    ];
    let signer = &[&seeds[..]];
    let cpi_ctx = CpiContext::new_with_signer(ctx.accounts.token_program.key(), cpi_accounts, signer);
    anchor_spl::token_interface::transfer_checked(cpi_ctx, ctx.accounts.vault_a.amount, ctx.accounts.mint_a.decimals)?;

    close_vault(ctx)
}

pub fn close_vault(ctx: Context<Cancel>) -> Result<()> {
    let cpi_accounts = anchor_spl::token_interface::CloseAccount {
        account: ctx.accounts.vault_a.to_account_info(),
        destination: ctx.accounts.maker.to_account_info(),
        authority: ctx.accounts.escrow.to_account_info(),
    };

    // Must match the escrow PDA seeds from `make` and the constraint above (line 51):
    // [b"escrow", maker, seed, bump]. The original dropped `seed`, which
    // derives a different address, so the escrow can no longer sign for the
    // vault and close_account fails. This now matches the seeds in `handler`.
    let seeds = &[
        &b"escrow"[..],
        ctx.accounts.escrow.maker.as_ref(),
        &ctx.accounts.escrow.seed.to_le_bytes(),
        &[ctx.accounts.escrow.bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.key(), 
        cpi_accounts, 
        signer_seeds
    );
    anchor_spl::token_interface::close_account(cpi_ctx)
}