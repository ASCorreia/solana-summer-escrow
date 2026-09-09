# Escrow

This example demonstrates how to implement a trustless token escrow using the Anchor framework on Solana. Two parties can exchange SPL tokens without requiring mutual trust — the program holds the maker's tokens in a vault until the taker fulfills the agreed terms, or the maker cancels and reclaims them.

Javier's change
---

## Let's walk through the architecture:

For this program, we will have 1 main state account:

- An Escrow account

An Escrow account consists of:

```rust
#[account]
#[derive(InitSpace)]
pub struct Escrow {
    pub maker: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub amount_a: u64,
    pub amount_b: u64,
    pub seed: u16,
    pub bump: u8,
}
```

### In this state account, we will store:

- maker: The public key of the account that created the escrow offer.
- mint_a: The public key of the token mint that the maker is offering.
- mint_b: The public key of the token mint that the maker wants in return.
- amount_a: The amount of token A the maker deposits into the vault.
- amount_b: The amount of token B the maker expects to receive from the taker.
- seed: A u16 seed used to derive the Escrow PDA, allowing a single maker to create multiple concurrent escrows.
- bump: The canonical bump for the Escrow PDA.

---

### The maker will create an escrow offer and deposit token A into the vault. For that, we create the following context:

```rust
#[derive(Accounts)]
#[instruction(seed: u16)]
pub struct Make<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        payer = maker,
        space = 8 + Escrow::INIT_SPACE,
        seeds = [b"escrow", maker.key().as_ref(), &seed.to_le_bytes()],
        bump
    )]
    pub escrow: Account<'info, Escrow>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
    )]
    pub maker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}
```

Let's have a closer look at the accounts that we are passing in this context:

- maker: The account creating the escrow. He will be a signer of the transaction, and we mark his account as mutable as we will be deducting lamports from this account.

- mint_a: The token mint the maker is offering. No constraints beyond its existence are required here.

- mint_b: The token mint the maker wants in return. No constraints are required here either.

- escrow: The state account that we will initialize. We derive the Escrow PDA from the seeds `["escrow", maker_pubkey, seed]`, allowing a single maker to open multiple simultaneous escrows by varying the seed.

- maker_ata_a: The maker's associated token account for mint A. Marked mutable because tokens will be transferred out of it.

- vault_a: The escrow's associated token account for mint A, initialized in this instruction and owned by the escrow PDA. This account holds the deposited tokens until the offer is taken or cancelled.

- system_program: Program responsible for the initialization of any new account.

- token_program: The SPL Token (or Token-2022) program that manages the token accounts.

- associated_token_program: Required to derive and initialize associated token accounts.

### We then implement the handler for Make:

```rust
pub fn handler(ctx: Context<Make>, seed: u16, amount_a: u64, amount_b: u64) -> Result<()> {
    ctx.accounts.escrow.set_inner(Escrow {
        maker: ctx.accounts.maker.key(),
        mint_a: ctx.accounts.mint_a.key(),
        mint_b: ctx.accounts.mint_b.key(),
        amount_a,
        amount_b,
        seed,
        bump: ctx.bumps.escrow,
    });

    let cpi_accounts = TransferChecked {
        from: ctx.accounts.maker_ata_a.to_account_info(),
        mint: ctx.accounts.mint_a.to_account_info(),
        to: ctx.accounts.vault_a.to_account_info(),
        authority: ctx.accounts.maker.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);
    transfer_checked(cpi_ctx, amount_a, ctx.accounts.mint_a.decimals)
}
```

Here, we first populate the Escrow state account with all the relevant offer details, then perform a `transfer_checked` CPI to move `amount_a` tokens from the maker's token account into the vault held by the escrow PDA.

---

### The taker will accept the offer, sending token B to the maker and receiving token A from the vault. For that, we create the following context:

```rust
#[derive(Accounts)]
pub struct Take<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,
    #[account(mut)]
    pub maker: SystemAccount<'info>,
    #[account(
        mut,
        close = taker,
        has_one = mint_a,
        has_one = mint_b,
    )]
    pub escrow: Account<'info, Escrow>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_a,
        associated_token::authority = taker,
    )]
    pub taker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = taker,
    )]
    pub taker_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_b,
        associated_token::authority = maker,
    )]
    pub maker_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}
```

Let's have a closer look at the accounts that we are passing in this context:

- taker: The account accepting the escrow offer. He will be a signer of the transaction, and we mark his account as mutable as lamports may be deducted to initialize token accounts.

- maker: The original offer creator. Marked mutable to receive the rent lamports when the vault and escrow accounts are closed.

- escrow: The escrow state account. We validate that it references the correct mints via `has_one`, and close it at the end of this instruction, returning rent to the taker.

- mint_a: The token mint the maker offered (and the taker will receive).

- mint_b: The token mint the maker requested (and the taker will send).

- taker_ata_a: The taker's associated token account for mint A, where the escrowed tokens will be deposited. Initialized if it does not yet exist.

- taker_ata_b: The taker's associated token account for mint B, from which `amount_b` tokens will be transferred to the maker.

- maker_ata_b: The maker's associated token account for mint B, where the taker's payment lands. Initialized if it does not yet exist.

- vault_a: The escrow's token account for mint A. Tokens are transferred out of here to the taker, then the account is closed.

- system_program: Program responsible for the initialization of any new account.

- token_program: The SPL Token (or Token-2022) program.

- associated_token_program: Required to derive and initialize associated token accounts.

### We then implement the handler for Take:

```rust
pub fn handler(ctx: Context<Take>) -> Result<()> {
    // Transfer amount_b from taker to maker
    let cpi_accounts = anchor_spl::token_interface::TransferChecked { ... };
    anchor_spl::token_interface::transfer_checked(cpi_ctx, ctx.accounts.escrow.amount_b, ctx.accounts.mint_b.decimals)?;

    // Transfer amount_a from vault to taker
    let cpi_accounts = anchor_spl::token_interface::TransferChecked { ... };
    let seeds = &[b"escrow", ctx.accounts.escrow.maker.as_ref(), &[ctx.accounts.escrow.bump]];
    anchor_spl::token_interface::transfer_checked(cpi_ctx, ctx.accounts.escrow.amount_a, ctx.accounts.mint_a.decimals)?;

    close_vault(ctx)
}
```

Here, we perform two transfers atomically: first, `amount_b` tokens flow from the taker to the maker; then, `amount_a` tokens flow from the vault to the taker, using the escrow PDA as a signer via `new_with_signer`. Finally, we close the now-empty vault and return its rent to the maker.

---

### The maker can cancel the escrow at any time to reclaim their deposited tokens. For that, we create the following context:

```rust
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
```

Let's have a closer look at the accounts that we are passing in this context:

- maker: The original offer creator. Must be a signer to authorize the cancellation, ensuring only the maker can cancel their own escrow.

- escrow: The escrow state account. We verify the PDA derivation using the stored seed and bump, and close it at the end of the instruction, returning rent to the maker.

- mint_a: The token mint that was deposited into the vault.

- maker_ata_a: The maker's associated token account for mint A. Marked mutable to receive the returned tokens.

- vault_a: The escrow's token account for mint A holding the deposited tokens, which will be fully drained and closed.

- token_program: The SPL Token (or Token-2022) program.

### We then implement the handler for Cancel:

```rust
pub fn handler(ctx: Context<Cancel>) -> Result<()> {
    let cpi_accounts = anchor_spl::token_interface::TransferChecked { ... };
    let seeds = &[b"escrow", ctx.accounts.escrow.maker.as_ref(), &escrow.seed.to_le_bytes(), &[ctx.accounts.escrow.bump]];
    anchor_spl::token_interface::transfer_checked(cpi_ctx, ctx.accounts.escrow.amount_a, ctx.accounts.mint_a.decimals)?;

    close_vault(ctx)
}
```

Here, the escrow PDA signs a `transfer_checked` CPI to return all deposited tokens from the vault back to the maker's token account, then closes the now-empty vault to recover its rent.

---

This escrow program provides a trustless, non-custodial token swap mechanism on Solana. The maker locks token A in a program-controlled vault and specifies how much of token B they want in return. Any taker can fulfill the offer by sending the requested token B, atomically receiving token A in the same transaction. If no taker steps in, the maker can cancel at any time to reclaim their tokens — all without any intermediary.