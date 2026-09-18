use {
    anchor_lang::{
        prelude::msg, solana_program::instruction::Instruction, solana_program::program_pack::Pack,
        system_program::ID as SYSTEM_PROGRAM_ID, AccountDeserialize, InstructionData,
        ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{self, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        token::spl_token,
    },
    litesvm::LiteSVM,
    litesvm_token::{
        spl_token::ID as TOKEN_PROGRAM_ID, CreateAssociatedTokenAccount, CreateMint, MintTo,
    },
    solana_keypair::Keypair,
    solana_message::Message,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::Transaction,
};

// Setup function to initialize LiteSVM and create a payer keypair
fn setup() -> (LiteSVM, Keypair) {
    let program_id = escrowq32026::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/escrowq32026.so"
    ));
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    // Return the LiteSVM instance and payer keypair
    (svm, payer)
}

#[test]
fn test_make_and_refund() {
    // Setup the test environment by initializing LiteSVM and creating a payer keypair
    let (mut program, payer) = setup();

    // Get the maker's public key from the payer keypair
    let maker = payer.pubkey();

    // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
    // This done using litesvm-token's CreateMint utility which creates the mint in the LiteSVM environment
    let mint_a = CreateMint::new(&mut program, &payer)
        .decimals(6)
        .authority(&maker)
        .send()
        .unwrap();
    msg!("Mint A: {}\n", mint_a);

    let mint_b = CreateMint::new(&mut program, &payer)
        .decimals(6)
        .authority(&maker)
        .send()
        .unwrap();
    msg!("Mint B: {}\n", mint_b);

    // Create the maker's associated token account for Mint A
    // This is done using litesvm-token's CreateAssociatedTokenAccount utility
    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
        .owner(&maker)
        .send()
        .unwrap();
    msg!("Maker ATA A: {}\n", maker_ata_a);

    // Derive the PDA for the escrow account using the maker's public key and a seed value
    let escrow = Pubkey::find_program_address(
        &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()],
        &escrowq32026::id(),
    )
    .0;
    msg!("Escrow PDA: {}\n", escrow);

    // Derive the PDA for the vault associated token account using the escrow PDA and Mint A
    let vault = associated_token::get_associated_token_address(&escrow, &mint_a);
    msg!("Vault PDA: {}\n", vault);

    // Mint 1,000 tokens (with 6 decimal places) of Mint A to the maker's associated token account
    MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1_000_000_000)
        .send()
        .unwrap();

    // Create the "Make" instruction to deposit tokens into the escrow
    let make_ix = Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Make {
            maker,
            mint_a,
            mint_b,
            maker_ata_a,
            escrow,
            vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Make {
            deposit: 10_000_000,
            seed: 123u64,
            receive: 10_000_000,
            expiration: 17780206209,
        }
        .data(),
    };

    // Create and send the transaction containing the "Make" instruction
    let message = Message::new(&[make_ix], Some(&payer.pubkey()));
    let recent_blockhash = program.latest_blockhash();

    let transaction = Transaction::new(&[&payer], message, recent_blockhash);

    // Send the transaction and capture the result
    let tx = program.send_transaction(transaction).unwrap();

    // Log transaction details
    msg!("\n\nMake transaction sucessfull");
    msg!("CUs Consumed: {}", tx.compute_units_consumed);
    msg!("Tx Signature: {}", tx.signature);

    // Verify the vault account and escrow account data after the "Make" instruction
    let vault_account = program.get_account(&vault).unwrap();
    let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();
    assert_eq!(vault_data.amount, 10_000_000);
    assert_eq!(vault_data.owner, escrow);
    assert_eq!(vault_data.mint, mint_a);

    let escrow_account = program.get_account(&escrow).unwrap();
    let escrow_data =
        escrowq32026::state::Escrow::try_deserialize(&mut escrow_account.data.as_ref()).unwrap();
    assert_eq!(escrow_data.seed, 123u64);
    assert_eq!(escrow_data.maker, maker);
    assert_eq!(escrow_data.mint_a, mint_a);
    assert_eq!(escrow_data.mint_b, mint_b);
    assert_eq!(escrow_data.receive, 10_000_000);

    // Create the "Refund" instruction to refund tokens back to the maker
    let refund_ix = Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Refund {
            maker,
            mint_a,
            maker_ata_a,
            escrow,
            vault,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Refund {}.data(),
    };

    // Create and send the transaction containing the "Refund" instruction
    let message = Message::new(&[refund_ix], Some(&payer.pubkey()));
    let recent_blockhash = program.latest_blockhash();

    let transaction = Transaction::new(&[&payer], message, recent_blockhash);

    // Send the transaction and capture the result
    let tx = program.send_transaction(transaction).unwrap();

    // Log transaction details
    msg!("\n\nRefund transaction sucessful");
    msg!("CUs Consumed: {}", tx.compute_units_consumed);
    msg!("Tx Signature: {}", tx.signature);
    assert!(program.get_account(&escrow).is_none());
    assert!(program.get_account(&vault).is_none());
}

#[test]
fn test_make_and_take() {
    let (mut program, payer) = setup();
    let maker = payer.pubkey();
    let taker = Keypair::new();

    let mint_a = CreateMint::new(&mut program, &payer)
        .decimals(6)
        .authority(&maker)
        .send()
        .unwrap();
    let mint_b = CreateMint::new(&mut program, &payer)
        .decimals(6)
        .authority(&maker)
        .send()
        .unwrap();

    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
        .owner(&maker)
        .send()
        .unwrap();
    let maker_ata_b = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_b)
        .owner(&maker)
        .send()
        .unwrap();
    let taker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
        .owner(&taker.pubkey())
        .send()
        .unwrap();
    let taker_ata_b = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_b)
        .owner(&taker.pubkey())
        .send()
        .unwrap();

    MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1_000_000_000)
        .send()
        .unwrap();
    MintTo::new(&mut program, &payer, &mint_b, &taker_ata_b, 5_000_000)
        .send()
        .unwrap();

    let escrow = Pubkey::find_program_address(
        &[b"escrow", maker.as_ref(), &456u64.to_le_bytes()],
        &escrowq32026::id(),
    )
    .0;
    let vault = associated_token::get_associated_token_address(&escrow, &mint_a);

    let make_ix = Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Make {
            maker,
            mint_a,
            mint_b,
            maker_ata_a,
            escrow,
            vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Make {
            deposit: 10_000_000,
            seed: 456u64,
            receive: 2_000_000,
            expiration: 4_000_000_000,
        }
        .data(),
    };

    let message = Message::new(&[make_ix], Some(&maker));
    let transaction = Transaction::new(&[&payer], message, program.latest_blockhash());
    program.send_transaction(transaction).unwrap();

    let take_ix = Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Take {
            taker: taker.pubkey(),
            maker,
            mint_a,
            mint_b,
            taker_ata_a,
            taker_ata_b,
            maker_ata_b,
            escrow,
            vault,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Take {}.data(),
    };

    let message = Message::new(&[take_ix], Some(&maker));
    let transaction = Transaction::new(&[&payer, &taker], message, program.latest_blockhash());
    program.send_transaction(transaction).unwrap();

    let maker_a =
        spl_token::state::Account::unpack(&program.get_account(&maker_ata_a).unwrap().data)
            .unwrap();
    let maker_b =
        spl_token::state::Account::unpack(&program.get_account(&maker_ata_b).unwrap().data)
            .unwrap();
    let taker_a =
        spl_token::state::Account::unpack(&program.get_account(&taker_ata_a).unwrap().data)
            .unwrap();
    let taker_b =
        spl_token::state::Account::unpack(&program.get_account(&taker_ata_b).unwrap().data)
            .unwrap();

    assert_eq!(maker_a.amount, 990_000_000);
    assert_eq!(maker_b.amount, 2_000_000);
    assert_eq!(taker_a.amount, 10_000_000);
    assert_eq!(taker_b.amount, 3_000_000);
    assert!(program.get_account(&escrow).is_none());
    assert!(program.get_account(&vault).is_none());

    let update_ix = Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Update {
            maker,
            mint_a,
            maker_ata_a,
            escrow,
            vault,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Update {
            deposit: 1_000_000,
            receive: 1_000_000,
        }
        .data(),
    };
    let message = Message::new(&[update_ix], Some(&maker));
    let transaction = Transaction::new(&[&payer], message, program.latest_blockhash());
    assert!(program.send_transaction(transaction).is_err());
}

struct UpdateFixture {
    program: litesvm::LiteSVM,
    payer: Keypair,
    mint_a: Pubkey,
    maker_ata_a: Pubkey,
    escrow: Pubkey,
    vault: Pubkey,
}

fn setup_update_fixture(deposit: u64, receive: u64, seed: u64) -> UpdateFixture {
    let (mut program, payer) = setup();
    let maker = payer.pubkey();

    let mint_a = CreateMint::new(&mut program, &payer)
        .decimals(6)
        .authority(&maker)
        .send()
        .unwrap();
    let mint_b = CreateMint::new(&mut program, &payer)
        .decimals(6)
        .authority(&maker)
        .send()
        .unwrap();
    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
        .owner(&maker)
        .send()
        .unwrap();

    MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1_000)
        .send()
        .unwrap();

    let escrow = Pubkey::find_program_address(
        &[b"escrow", maker.as_ref(), &seed.to_le_bytes()],
        &escrowq32026::id(),
    )
    .0;
    let vault = associated_token::get_associated_token_address(&escrow, &mint_a);

    let make_ix = Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Make {
            maker,
            mint_a,
            mint_b,
            maker_ata_a,
            escrow,
            vault,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Make {
            deposit,
            seed,
            receive,
            expiration: 4_000_000_000,
        }
        .data(),
    };
    let message = Message::new(&[make_ix], Some(&maker));
    let transaction = Transaction::new(&[&payer], message, program.latest_blockhash());
    program.send_transaction(transaction).unwrap();

    UpdateFixture {
        program,
        payer,
        mint_a,
        maker_ata_a,
        escrow,
        vault,
    }
}

fn update_instruction(fixture: &UpdateFixture, deposit: u64, receive: u64) -> Instruction {
    Instruction {
        program_id: escrowq32026::id(),
        accounts: escrowq32026::accounts::Update {
            maker: fixture.payer.pubkey(),
            mint_a: fixture.mint_a,
            maker_ata_a: fixture.maker_ata_a,
            escrow: fixture.escrow,
            vault: fixture.vault,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: escrowq32026::instruction::Update { deposit, receive }.data(),
    }
}

#[test]
fn test_update_receive_amount() {
    let mut fixture = setup_update_fixture(100, 50, 1_001);
    let update_ix = update_instruction(&fixture, 100, 80);
    let message = Message::new(&[update_ix], Some(&fixture.payer.pubkey()));
    let transaction = Transaction::new(
        &[&fixture.payer],
        message,
        fixture.program.latest_blockhash(),
    );
    fixture.program.send_transaction(transaction).unwrap();

    let escrow_account = fixture.program.get_account(&fixture.escrow).unwrap();
    let escrow_data =
        escrowq32026::state::Escrow::try_deserialize(&mut escrow_account.data.as_ref()).unwrap();
    let vault_account = fixture.program.get_account(&fixture.vault).unwrap();
    let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();

    assert_eq!(escrow_data.receive, 80);
    assert_eq!(vault_data.amount, 100);
}

#[test]
fn test_update_decreases_deposit() {
    let mut fixture = setup_update_fixture(100, 50, 1_002);
    let update_ix = update_instruction(&fixture, 20, 50);
    let message = Message::new(&[update_ix], Some(&fixture.payer.pubkey()));
    let transaction = Transaction::new(
        &[&fixture.payer],
        message,
        fixture.program.latest_blockhash(),
    );
    fixture.program.send_transaction(transaction).unwrap();

    let maker_account = fixture.program.get_account(&fixture.maker_ata_a).unwrap();
    let maker_data = spl_token::state::Account::unpack(&maker_account.data).unwrap();
    let vault_account = fixture.program.get_account(&fixture.vault).unwrap();
    let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();

    assert_eq!(maker_data.amount, 980);
    assert_eq!(vault_data.amount, 20);
}

#[test]
fn test_update_increases_deposit() {
    let mut fixture = setup_update_fixture(100, 50, 1_003);
    let update_ix = update_instruction(&fixture, 120, 75);
    let message = Message::new(&[update_ix], Some(&fixture.payer.pubkey()));
    let transaction = Transaction::new(
        &[&fixture.payer],
        message,
        fixture.program.latest_blockhash(),
    );
    fixture.program.send_transaction(transaction).unwrap();

    let maker_account = fixture.program.get_account(&fixture.maker_ata_a).unwrap();
    let maker_data = spl_token::state::Account::unpack(&maker_account.data).unwrap();
    let vault_account = fixture.program.get_account(&fixture.vault).unwrap();
    let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();
    let escrow_account = fixture.program.get_account(&fixture.escrow).unwrap();
    let escrow_data =
        escrowq32026::state::Escrow::try_deserialize(&mut escrow_account.data.as_ref()).unwrap();

    assert_eq!(maker_data.amount, 880);
    assert_eq!(vault_data.amount, 120);
    assert_eq!(escrow_data.receive, 75);
}

#[test]
fn test_update_rejects_zero_receive() {
    let mut fixture = setup_update_fixture(100, 50, 1_004);
    let update_ix = update_instruction(&fixture, 100, 0);
    let message = Message::new(&[update_ix], Some(&fixture.payer.pubkey()));
    let transaction = Transaction::new(
        &[&fixture.payer],
        message,
        fixture.program.latest_blockhash(),
    );

    assert!(fixture.program.send_transaction(transaction).is_err());

    let escrow_account = fixture.program.get_account(&fixture.escrow).unwrap();
    let escrow_data =
        escrowq32026::state::Escrow::try_deserialize(&mut escrow_account.data.as_ref()).unwrap();
    let vault_account = fixture.program.get_account(&fixture.vault).unwrap();
    let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();

    assert_eq!(escrow_data.receive, 50);
    assert_eq!(vault_data.amount, 100);
}

#[test]
fn test_update_with_insufficient_balance_is_atomic() {
    let mut fixture = setup_update_fixture(100, 50, 1_005);
    let update_ix = update_instruction(&fixture, 1_001, 75);
    let message = Message::new(&[update_ix], Some(&fixture.payer.pubkey()));
    let transaction = Transaction::new(
        &[&fixture.payer],
        message,
        fixture.program.latest_blockhash(),
    );

    assert!(fixture.program.send_transaction(transaction).is_err());

    let maker_account = fixture.program.get_account(&fixture.maker_ata_a).unwrap();
    let maker_data = spl_token::state::Account::unpack(&maker_account.data).unwrap();
    let vault_account = fixture.program.get_account(&fixture.vault).unwrap();
    let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();
    let escrow_account = fixture.program.get_account(&fixture.escrow).unwrap();
    let escrow_data =
        escrowq32026::state::Escrow::try_deserialize(&mut escrow_account.data.as_ref()).unwrap();

    assert_eq!(maker_data.amount, 900);
    assert_eq!(vault_data.amount, 100);
    assert_eq!(escrow_data.receive, 50);
}
