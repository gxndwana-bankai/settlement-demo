//! An end-to-end example of using the SP1 SDK to generate a proof of a program that can be executed
//! or have a core proof generated.
//!
//! You can run this script using the following command:
//! ```shell
//! RUST_LOG=info cargo run --release -- --execute
//! ```
//! or
//! ```shell
//! RUST_LOG=info cargo run --release -- --prove
//! ```

use alloy_primitives::hex::FromHex;
use alloy_primitives::{Address, FixedBytes, U256};
use alloy_sol_types::SolType;
use bankai_sdk::{Bankai, HashingFunctionDto, Network};
use clap::Parser;
use settlement_lib::{ClaimedExecution, Order};
use sp1_sdk::Prover;
use sp1_sdk::{include_elf, network::NetworkMode, ProverClient, SP1Stdin};
/// The ELF (executable and linkable format) file for the Succinct RISC-V zkVM.
pub const SETTLEMENT_ELF: &[u8] = include_elf!("settlement-program");

/// The arguments for the command.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    execute: bool,

    #[arg(long)]
    prove: bool,
}

#[tokio::main]
async fn main() {
    // Setup the logger.
    sp1_sdk::utils::setup_logger();
    dotenv::dotenv().ok();

    // Parse the command line arguments.
    let args = Args::parse();

    if args.execute == args.prove {
        eprintln!("Error: You must specify either --execute or --prove");
        std::process::exit(1);
    }

    let private_key = std::env::var("NETWORK_PRIVATE_KEY").unwrap();
    let client = ProverClient::builder()
        .network_for(NetworkMode::Mainnet)
        .private_key(&private_key)
        .build();

    let exec_rpc = std::env::var("EXECUTION_RPC").ok();
    let bankai = Bankai::new(Network::Sepolia, exec_rpc.clone(), None);

    // Setup new batch for Sepolia Network
    let mut bankai_batch = bankai
        .init_batch(Network::Sepolia, None, HashingFunctionDto::Keccak)
        .await
        .unwrap();

    // Add example orders to the batch
    let orders = example_orders();

    // Add evm transactions to the batch
    for order in orders.clone() {
        bankai_batch = bankai_batch.evm_tx(order.1.tx_hash);
    }

    // Execute the batch, generating all proofs for the added transactions
    let batch_result = bankai_batch.execute().await.unwrap();

    // Serialize the batch result to bytes
    let proof_bytes = serde_json::to_vec(&batch_result).expect("JSON serialization failed");

    let mut stdin = SP1Stdin::new();
    stdin.write_vec(proof_bytes);
    stdin.write(&orders);

    if args.execute {
        // Execute the program
        let (output, report) = client.execute(SETTLEMENT_ELF, &stdin).run().unwrap();
        println!("Program executed successfully.");

        // // Read the output.
        // let decoded = PublicValuesStruct::abi_decode(output.as_slice()).unwrap();
        // let PublicValuesStruct { n, a, b } = decoded;
        // println!("n: {}", n);
        // println!("a: {}", a);
        // println!("b: {}", b);

        // let (expected_a, expected_b) = fibonacci_lib::fibonacci(n);
        // assert_eq!(a, expected_a);
        // assert_eq!(b, expected_b);
        // println!("Values are correct!");

        // Record the number of cycles executed.
        println!("Number of cycles: {}", report.total_instruction_count());
    } else {
        // Setup the program for proving.
        let (pk, vk) = client.setup(SETTLEMENT_ELF);

        // Generate the proof
        let proof = client
            .prove(&pk, &stdin)
            .run()
            .expect("failed to generate proof");

        println!("Successfully generated proof!");

        // Verify the proof.
        client.verify(&proof, &vk).expect("failed to verify proof");
        println!("Successfully verified proof!");
    }
}

fn example_orders() -> Vec<(settlement_lib::Order, settlement_lib::ClaimedExecution)> {
    vec![(
        Order {
            source_chain_id: 2,
            destination_chain_id: 1,
            receiver: Address::from_hex("0x797b212C0a4cB61DEC7dC491B632b72D854e03fd").unwrap(),
            amount: U256::from(273418440000000000u64),
            block_number: 9451455u64.into(),
        },
        ClaimedExecution {
            chain_id: 1,
            tx_hash: FixedBytes::<32>::from_hex(
                "0xfadaa0be61861ff1c2fbe8ef5f29359f1d809005fed8bd3b0dc3dee3fa48a04d",
            )
            .unwrap(),
        },
    )]
}
