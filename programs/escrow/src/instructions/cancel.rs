use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{error::ErrorCode, Escrow, CANCEL_DELAY_SECONDS};

#[derive(Accounts)]
pub struct Cancel<'info> {
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
    // The time lock is checked before anything moves, so a rejected cancel leaves
    // the vault exactly as it was.
    //
    // `checked_add` rather than `+`: `created_at` comes out of stored state, and a
    // release build wraps on overflow instead of panicking. A garbage timestamp near
    // i64::MAX would wrap to a large negative number and make the comparison below
    // trivially true — the lock would open instead of holding.
    let now = Clock::get()?.unix_timestamp;
    let unlock_at = ctx
        .accounts
        .escrow
        .created_at
        .checked_add(CANCEL_DELAY_SECONDS)
        .ok_or(ErrorCode::TimeLockActive)?;

    // `>=`, so cancelling exactly on the boundary is allowed: at created_at + 300 the
    // five minutes have, in fact, passed.
    require!(now >= unlock_at, ErrorCode::TimeLockActive);

    let cpi_accounts = anchor_spl::token_interface::TransferChecked {
        from: ctx.accounts.vault_a.to_account_info(),
        mint: ctx.accounts.mint_a.to_account_info(),
        to: ctx.accounts.maker_ata_a.to_account_info(),
        authority: ctx.accounts.escrow.to_account_info(),
    };
    let seeds = &[
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

    let seeds = &[
        &b"escrow"[..],
        ctx.accounts.escrow.maker.as_ref(),
        &ctx.accounts.escrow.seed.to_le_bytes(),
        &[ctx.accounts.escrow.bump]
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.key(), 
        cpi_accounts, 
        signer_seeds
    );
    anchor_spl::token_interface::close_account(cpi_ctx)
}