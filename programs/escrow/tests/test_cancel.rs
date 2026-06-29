use anchor_lang::{
    prelude::Clock, solana_program::instruction::Instruction, AccountDeserialize, InstructionData,
    ToAccountMetas,
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

/// Runs the `make` instruction so cancel tests start from a valid escrow state
fn run_make(
    svm: &mut LiteSVM,
    program_id: Pubkey,
    maker: &Keypair,
    mint_a_pk: Pubkey,
    mint_b_pk: Pubkey,
    seed: u16,
    amount_a: u64,
    amount_b: u64,
) -> (Pubkey, Pubkey) {
    let maker_pk = maker.pubkey();
    let (escrow_pda, _bump) = Pubkey::find_program_address(
        &[b"escrow", maker_pk.as_ref(), &seed.to_le_bytes()],
        &program_id,
    );
    let vault_a = get_associated_token_address(&escrow_pda, &mint_a_pk);

    let maker_ata_a = get_associated_token_address(&maker_pk, &mint_a_pk);

    let instruction = Instruction::new_with_bytes(
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
    let msg = Message::new_with_blockhash(&[instruction], Some(&maker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[maker]).unwrap();
    svm.send_transaction(tx).expect("make failed in cancel test setup");

    (escrow_pda, vault_a)
}

#[test]
fn test_cancel_returns_tokens_to_maker() {
    let program_id = escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, bytes).unwrap();

    let maker = Keypair::new();
    let mint_a = Keypair::new();
    let mint_b = Keypair::new();

    let maker_pk = maker.pubkey();
    let mint_a_pk = mint_a.pubkey();
    let mint_b_pk = mint_b.pubkey();

    svm.airdrop(&maker_pk, 10_000_000_000).unwrap();

    setup_mint(&mut svm, &mint_a, &maker_pk, 6);
    setup_mint(&mut svm, &mint_b, &maker_pk, 6);

    let seed: u16 = 42;
    let amount_a: u64 = 1_000_000;
    let amount_b: u64 = 500_000;

    let maker_ata_a = get_associated_token_address(&maker_pk, &mint_a_pk);
    setup_token_account(&mut svm, maker_ata_a, mint_a_pk, maker_pk, amount_a);

    let (escrow_pda, vault_a) =
        run_make(&mut svm, program_id, &maker, mint_a_pk, mint_b_pk, seed, amount_a, amount_b);

    // Vault holds amount_a before cancel
    let vault_before = svm.get_account(&vault_a).unwrap();
    let vault_token_before = TokenAccount::unpack(&vault_before.data).unwrap();
    assert_eq!(vault_token_before.amount, amount_a);

    // maker_ata_a is empty after make moved the tokens out
    let ata_before = svm.get_account(&maker_ata_a).unwrap();
    let ata_token_before = TokenAccount::unpack(&ata_before.data).unwrap();
    assert_eq!(ata_token_before.amount, 0);

    // The program now timelocks cancellation for 5 minutes after creation.
    // Rather than sleep in real time, we fast forward the Clock sysvar so the
    // stored created_at is far enough in the past for the guard to pass.
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += 5 * 60 + 1; // one second past the 5 minute lock
    svm.set_sysvar(&clock);

    let cancel_ix = Instruction::new_with_bytes(
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
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[cancel_ix], Some(&maker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&maker]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "cancel transaction failed: {:?}", res.err());

    // Tokens returned to maker's ATA
    let ata_after = svm.get_account(&maker_ata_a).unwrap();
    let ata_token_after = TokenAccount::unpack(&ata_after.data).unwrap();
    assert_eq!(ata_token_after.amount, amount_a, "maker did not receive tokens back");

    // Vault closed: account should no longer exist
    assert!(
        svm.get_account(&vault_a).is_none(),
        "vault_a should be closed after cancel"
    );

    // Escrow PDA closed: account should no longer exist
    assert!(
        svm.get_account(&escrow_pda).is_none(),
        "escrow PDA should be closed after cancel"
    );
}

#[test]
fn test_cancel_fails_for_non_maker() {
    let program_id = escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, bytes).unwrap();

    let maker = Keypair::new();
    let attacker = Keypair::new();
    let mint_a = Keypair::new();
    let mint_b = Keypair::new();

    let maker_pk = maker.pubkey();
    let attacker_pk = attacker.pubkey();
    let mint_a_pk = mint_a.pubkey();
    let mint_b_pk = mint_b.pubkey();

    svm.airdrop(&maker_pk, 10_000_000_000).unwrap();
    svm.airdrop(&attacker_pk, 10_000_000_000).unwrap();

    setup_mint(&mut svm, &mint_a, &maker_pk, 6);
    setup_mint(&mut svm, &mint_b, &maker_pk, 6);

    let seed: u16 = 7;
    let amount_a: u64 = 2_000_000;
    let amount_b: u64 = 1_000_000;

    let maker_ata_a = get_associated_token_address(&maker_pk, &mint_a_pk);
    setup_token_account(&mut svm, maker_ata_a, mint_a_pk, maker_pk, amount_a);

    let (escrow_pda, vault_a) =
        run_make(&mut svm, program_id, &maker, mint_a_pk, mint_b_pk, seed, amount_a, amount_b);

    // Attacker tries to cancel with maker's escrow but signs as attacker.
    // The escrow seeds include maker's pubkey, so the PDA won't match attacker's key.
    let cancel_ix = Instruction::new_with_bytes(
        program_id,
        &escrow::instruction::Cancel {}.data(),
        escrow::accounts::Cancel {
            maker: attacker_pk,
            escrow: escrow_pda,
            mint_a: mint_a_pk,
            maker_ata_a: get_associated_token_address(&attacker_pk, &mint_a_pk),
            vault_a,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[cancel_ix], Some(&attacker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&attacker]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "cancel should have rejected a non-maker signer");
}

#[test]
fn test_cancel_fails_while_timelocked() {
    let program_id = escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, bytes).unwrap();

    let maker = Keypair::new();
    let mint_a = Keypair::new();
    let mint_b = Keypair::new();

    let maker_pk = maker.pubkey();
    let mint_a_pk = mint_a.pubkey();
    let mint_b_pk = mint_b.pubkey();

    svm.airdrop(&maker_pk, 10_000_000_000).unwrap();

    setup_mint(&mut svm, &mint_a, &maker_pk, 6);
    setup_mint(&mut svm, &mint_b, &maker_pk, 6);

    let seed: u16 = 99;
    let amount_a: u64 = 1_000_000;
    let amount_b: u64 = 500_000;

    let maker_ata_a = get_associated_token_address(&maker_pk, &mint_a_pk);
    setup_token_account(&mut svm, maker_ata_a, mint_a_pk, maker_pk, amount_a);

    let (escrow_pda, vault_a) =
        run_make(&mut svm, program_id, &maker, mint_a_pk, mint_b_pk, seed, amount_a, amount_b);

    // Cancel immediately, without advancing the clock. created_at was stamped
    // during make, so elapsed is ~0 seconds, well under the 5 minute lock.
    let cancel_ix = Instruction::new_with_bytes(
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
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[cancel_ix], Some(&maker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&maker]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "cancel should be rejected before the timelock elapses");

    // The guard runs before any tokens move, so nothing changed on chain:
    // the escrow PDA and the vault must both still exist with funds intact.
    let escrow_after = svm.get_account(&escrow_pda);
    assert!(escrow_after.is_some(), "escrow must survive a rejected cancel");

    let vault_after = svm.get_account(&vault_a).expect("vault must still exist");
    let vault_token_after = TokenAccount::unpack(&vault_after.data).unwrap();
    assert_eq!(vault_token_after.amount, amount_a, "vault funds must be untouched");
}
