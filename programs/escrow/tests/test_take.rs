use anchor_lang::{
    solana_program::instruction::Instruction, InstructionData, ToAccountMetas,
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

// These helpers mirror test_make.rs / test_cancel.rs. Each integration test
// file is compiled as its own crate, so the helpers are duplicated rather than
// shared. We pre-bake account state with svm.set_account instead of running the
// real SPL token instructions: it is faster and keeps the test focused on the
// escrow program under test.

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

/// Runs the `make` instruction so take tests start from a valid escrow state.
/// The maker deposits `amount_a` of mint_a into the vault and asks for
/// `amount_b` of mint_b in return.
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
    svm.send_transaction(tx).expect("make failed in take test setup");

    (escrow_pda, vault_a)
}

#[test]
fn test_take_completes_the_swap() {
    let program_id = escrow::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, bytes).unwrap();

    // maker offers mint_a and wants mint_b; taker is the counterparty.
    let maker = Keypair::new();
    let taker = Keypair::new();
    let mint_a = Keypair::new();
    let mint_b = Keypair::new();

    let maker_pk = maker.pubkey();
    let taker_pk = taker.pubkey();
    let mint_a_pk = mint_a.pubkey();
    let mint_b_pk = mint_b.pubkey();

    // Both parties need lamports: maker pays rent for the escrow/vault in make,
    // taker pays rent for the ATAs that take creates via init_if_needed.
    svm.airdrop(&maker_pk, 10_000_000_000).unwrap();
    svm.airdrop(&taker_pk, 10_000_000_000).unwrap();

    setup_mint(&mut svm, &mint_a, &maker_pk, 6);
    setup_mint(&mut svm, &mint_b, &maker_pk, 6);

    let seed: u16 = 42;
    let amount_a: u64 = 1_000_000; // offered by maker
    let amount_b: u64 = 500_000; // requested from taker

    // Fund the two accounts that must already exist before take runs:
    // maker_ata_a holds the tokens make will move into the vault, and
    // taker_ata_b holds the tokens the taker pays to the maker.
    let maker_ata_a = get_associated_token_address(&maker_pk, &mint_a_pk);
    setup_token_account(&mut svm, maker_ata_a, mint_a_pk, maker_pk, amount_a);

    let taker_ata_b = get_associated_token_address(&taker_pk, &mint_b_pk);
    setup_token_account(&mut svm, taker_ata_b, mint_b_pk, taker_pk, amount_b);

    // ATAs created by the take instruction (init_if_needed). We only derive the
    // addresses here; we do not pre-create them.
    let taker_ata_a = get_associated_token_address(&taker_pk, &mint_a_pk);
    let maker_ata_b = get_associated_token_address(&maker_pk, &mint_b_pk);

    let (escrow_pda, vault_a) =
        run_make(&mut svm, program_id, &maker, mint_a_pk, mint_b_pk, seed, amount_a, amount_b);

    // Sanity check: after make the vault holds amount_a.
    let vault_before = svm.get_account(&vault_a).unwrap();
    let vault_token_before = TokenAccount::unpack(&vault_before.data).unwrap();
    assert_eq!(vault_token_before.amount, amount_a);

    let take_ix = Instruction::new_with_bytes(
        program_id,
        &escrow::instruction::Take {}.data(),
        escrow::accounts::Take {
            taker: taker_pk,
            maker: maker_pk,
            escrow: escrow_pda,
            mint_a: mint_a_pk,
            mint_b: mint_b_pk,
            taker_ata_a,
            taker_ata_b,
            maker_ata_b,
            vault_a,
            system_program: anchor_lang::system_program::ID,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: spl_associated_token_account_interface::program::ID,
        }
        .to_account_metas(None),
    );

    // The taker is the fee payer and the only required signer: it authorizes the
    // mint_b transfer to the maker, while the vault transfer is signed by the
    // escrow PDA inside the program.
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[take_ix], Some(&taker_pk), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&taker]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "take transaction failed: {:?}", res.err());

    // Taker received the offered tokens (amount_a of mint_a) from the vault.
    let taker_ata_a_acc = svm.get_account(&taker_ata_a).expect("taker_ata_a should exist");
    let taker_token_a = TokenAccount::unpack(&taker_ata_a_acc.data).unwrap();
    assert_eq!(taker_token_a.amount, amount_a, "taker did not receive mint_a");

    // Maker received the requested tokens (amount_b of mint_b) from the taker.
    let maker_ata_b_acc = svm.get_account(&maker_ata_b).expect("maker_ata_b should exist");
    let maker_token_b = TokenAccount::unpack(&maker_ata_b_acc.data).unwrap();
    assert_eq!(maker_token_b.amount, amount_b, "maker did not receive mint_b");

    // Taker's mint_b balance is fully spent on the swap.
    let taker_ata_b_acc = svm.get_account(&taker_ata_b).unwrap();
    let taker_token_b = TokenAccount::unpack(&taker_ata_b_acc.data).unwrap();
    assert_eq!(taker_token_b.amount, 0, "taker's mint_b should be spent");

    // Vault and escrow are both closed once the swap settles.
    assert!(svm.get_account(&vault_a).is_none(), "vault_a should be closed after take");
    assert!(svm.get_account(&escrow_pda).is_none(), "escrow PDA should be closed after take");
}
