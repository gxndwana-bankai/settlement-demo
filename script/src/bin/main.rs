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

use std::fs::File;
use std::io::Write;

use alloy_primitives::hex::FromHex;
use alloy_primitives::{Address, FixedBytes, U256};
use bankai_sdk::{Bankai, HashingFunctionDto, Network};
use clap::Parser;
use settlement_lib::{generate_all_proofs, ClaimedExecution, Order};
use sp1_sdk::HashableKey;
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

    let orders: Vec<Order> = orders.iter().map(|(order, _)| order.clone()).collect();
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(proof_bytes);
    stdin.write(&orders);

    if args.execute {
        // Execute the program
        let (output, report) = client.execute(SETTLEMENT_ELF, &stdin).run().unwrap();
        println!("Program executed successfully. {output:?}");

        let output_root = FixedBytes::<32>::from_slice(&output.as_slice()[8..40]);
        println!("Output Root: {output_root:?}");
        // Record the number of cycles executed.
        println!("Number of cycles: {}", report.total_instruction_count());
    } else {
        // Setup the program for proving.
        let (pk, vk) = client.setup(SETTLEMENT_ELF);
        println!("vk: {:?}", vk.bytes32());

        // Generate the proof
        let proof = client
            .prove(&pk, &stdin)
            .groth16()
            .run()
            .expect("failed to generate proof");

        // Read the output.
        let output_root = FixedBytes::<32>::from_slice(&proof.public_values.as_slice()[8..40]);
        println!("Output Root: {output_root:?}");

        let merkle_proof = generate_all_proofs(orders.as_slice());

        assert_eq!(
            merkle_proof.root,
            FixedBytes::<32>::from_slice(&proof.public_values.as_slice()[8..40])
        );

        // Group proofs by source chain ID
        let mut proofs_by_chain: std::collections::BTreeMap<u64, Vec<_>> =
            std::collections::BTreeMap::new();
        for order_proof in merkle_proof.proofs {
            let order_hash = order_proof.order.hash();
            proofs_by_chain
                .entry(order_proof.order.source_chain_id)
                .or_default()
                .push(serde_json::json!({
                    "order": {
                        "source_chain_id": order_proof.order.source_chain_id,
                        "destination_chain_id": order_proof.order.destination_chain_id,
                        "receiver": order_proof.order.receiver.to_string(),
                        "amount": order_proof.order.amount.to_string(),
                        "block_number": order_proof.order.block_number,
                    },
                    "order_hash": order_hash.to_string(),
                    "proof": order_proof.proof.iter()
                        .map(|h| h.to_string())
                        .collect::<Vec<_>>(),
                    "leaf_index": order_proof.leaf_index,
                }));
        }

        // Write to file for easy access
        let fixture = serde_json::json!({
            "proof": format!("0x{}", hex::encode(proof.bytes())),
            "publicValues": format!("0x{}", hex::encode(proof.public_values.as_slice())),
            "vkey": vk.bytes32().to_string(),
            "merkleRoot": merkle_proof.root.to_string(),
            "proofsBySourceChain": proofs_by_chain,
        });

        let mut file = File::create("proof.json").expect("Failed to create file");
        file.write_all(fixture.to_string().as_bytes())
            .expect("Failed to write to file");

        println!("Successfully generated proof!");

        // Verify the proof.
        client.verify(&proof, &vk).expect("failed to verify proof");
        println!("Successfully verified proof!");
    }
}

fn example_orders() -> Vec<(settlement_lib::Order, settlement_lib::ClaimedExecution)> {
    vec![
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x797b212C0a4cB61DEC7dC491B632b72D854e03fd").unwrap(),
                amount: U256::from(273418440000000000u64),
                block_number: 9451455,
            },
            ClaimedExecution {
                chain_id: 1,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xfadaa0be61861ff1c2fbe8ef5f29359f1d809005fed8bd3b0dc3dee3fa48a04d",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 421614,        // Arb Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x956962C34687A954e611A83619ABaA37Ce6bC78A").unwrap(),
                amount: U256::from(100000000000000000u64),
                block_number: 9452270,
            },
            ClaimedExecution {
                chain_id: 1,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x51e90fc338ce7b1ea736d523284668732c7c38c8a35dbcd71e7f239d9aca0b81",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x1E8447a24De0977C70138BDdacFC67bf2A2b333a").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452282,
            },
            ClaimedExecution {
                chain_id: 1,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xa5fe7b6f5690b73777b5e161b56b10ec19247e76ea8b7a4a0f5482c9e98ac6dd",
                )
                .unwrap(),
            },
        ),
    ]
}
