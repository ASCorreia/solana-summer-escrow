use anchor_lang::{
    prelude::Clock,
    solana_program::instruction::Instruction,
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_program_option::COption;
use solana_program_pack::Pack;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use spl_associated_token_account_interface::address::get_associated_token_address;
use spl_token_interface::{
    state::{Account as TokenAccount, AccountState, Mint},
    ID as TOKEN_PROGRAM_ID,
};

fn setup_mint(svm: &mut LiteSVM, mint: &Keypair, authority: &Pubkey, decimals: u8) {
    let state = Mint {
        mint_authority: COption::Some(*authority),
        supply: 0,
        decimals,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    let mut data = [0u8; Mint::LEN];
    Mint::pack(state, &mut data).unwrap();
    svm.set_account(
        mint.pubkey(),
        Account {
            lamports: 1_000_000_000,
            data: data.to_vec(),
            owner: TOKEN_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

fn setup_token_account(
    svm: &mut LiteSVM,
    address: Pubkey,
    mint: Pubkey,
    owner: Pubkey,
    amount: u64,
) {
    let state = TokenAccount {
        mint,
        owner,
        amount,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    let mut data = [0u8; TokenAccount::LEN];
    TokenAccount::pack(state, &mut data).unwrap();
    svm.set_account(
        address,
        Account {
            lamports: 1_000_000_000,
            data: data.to_vec(),
            owner: TOKEN_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

fn setup_escrow(svm: &mut LiteSVM) -> (Keypair, Pubkey, Pubkey, Pubkey, u16) {
    let program_id = escrow::id();
    let maker = Keypair::new();
    let mint_a = Keypair::new();
    let mint_b = Keypair::new();

    let maker_pk = maker.pubkey();
    let mint_a_pk = mint_a.pubkey();
    let mint_b_pk = mint_b.pubkey();

    svm.airdrop(&maker_pk, 10_000_000_000).unwrap();

    setup_mint(svm, &mint_a, &maker_pk, 6);
    setup_mint(svm, &mint_b, &maker_pk, 6);

    let seed: u16 = 42;
    let amount_a: u64 = 1_000_000;
    let amount_b: u64 = 500_000;

    let maker_ata_a = get_associated_token_address(&maker_pk, &mint_a_pk);
    setup_token_account(svm, maker_ata_a, mint_a_pk, maker_pk, amount_a);

    let (escrow_pda, _bump) = Pubkey::find_program_address(
        &[b"escrow", maker_pk.as_ref(), &seed.to_le_bytes()],
        &program_id,
    );
    let vault_a = get_associated_token_address(&escrow_pda, &mint_a_pk);

    let make_ix = Instruction::new_with_bytes(
        program_id,
        &escrow::instruction::Make {
            seed,
            amount_a,
            amount_b,
        }
        .data(),
        escrow::accounts::Make {
            maker: maker_pk,
            mint_a: mint_a_pk,
            mint_b: mint_b_pk,
            escrow: escrow_pda,
            maker_ata_a,
            vault_a,
            system_program: anchor_lang::system_program::ID,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: spl_associated_token_account_interface::program::ID,
        }
        .to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[make_ix], Some(&maker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&maker]).unwrap();
    svm.send_transaction(tx).expect("make should succeed");

    (maker, mint_a_pk, maker_ata_a, escrow_pda, seed)
}

fn build_cancel_ix(
    program_id: Pubkey,
    maker_pk: Pubkey,
    escrow_pda: Pubkey,
    mint_a_pk: Pubkey,
    maker_ata_a: Pubkey,
    vault_a: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &escrow::instruction::Cancel {}.data(),
        escrow::accounts::Cancel {
            maker: maker_pk,
            escrow: escrow_pda,
            mint_a: mint_a_pk,
            maker_ata_a,
            vault_a,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

#[test]
fn cancel_too_early_fails() {
    let program_id = escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, bytes).unwrap();

    let (maker, mint_a_pk, maker_ata_a, escrow_pda, _seed) = setup_escrow(&mut svm);
    let maker_pk = maker.pubkey();
    let vault_a = get_associated_token_address(&escrow_pda, &mint_a_pk);

    // Attempt cancel immediately — clock is still at 0, lock has not elapsed
    let cancel_ix = build_cancel_ix(program_id, maker_pk, escrow_pda, mint_a_pk, maker_ata_a, vault_a);
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[cancel_ix], Some(&maker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&maker]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "cancel should fail before the time lock elapses");

    // Verify the failure is specifically our TimeLockActive error
    let err = res.unwrap_err();
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("TimeLockActive"),
        "expected TimeLockActive error, got: {logs}"
    );
}

#[test]
fn cancel_exactly_at_boundary_succeeds() {
    let program_id = escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, bytes).unwrap();

    let (maker, mint_a_pk, maker_ata_a, escrow_pda, _seed) = setup_escrow(&mut svm);
    let maker_pk = maker.pubkey();
    let vault_a = get_associated_token_address(&escrow_pda, &mint_a_pk);

    // Advance clock to exactly created_at + 300 (created_at == 0 on fresh LiteSVM)
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += 300;
    svm.set_sysvar(&clock);

    let cancel_ix = build_cancel_ix(program_id, maker_pk, escrow_pda, mint_a_pk, maker_ata_a, vault_a);
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[cancel_ix], Some(&maker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&maker]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "cancel exactly at boundary should succeed: {:?}", res.err());
}

#[test]
fn cancel_after_timelock_succeeds() {
    let program_id = escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, bytes).unwrap();

    let (maker, mint_a_pk, maker_ata_a, escrow_pda, _seed) = setup_escrow(&mut svm);
    let maker_pk = maker.pubkey();
    let vault_a = get_associated_token_address(&escrow_pda, &mint_a_pk);

    // Advance clock past the 5-minute lock
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += 301;
    svm.set_sysvar(&clock);

    let cancel_ix = build_cancel_ix(program_id, maker_pk, escrow_pda, mint_a_pk, maker_ata_a, vault_a);
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[cancel_ix], Some(&maker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&maker]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "cancel after time lock should succeed: {:?}", res.err());
}
