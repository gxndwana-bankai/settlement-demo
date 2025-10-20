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
use std::str::FromStr;

use alloy_primitives::hex::FromHex;
use alloy_primitives::{Address, FixedBytes, U256};
use bankai_sdk::{Bankai, HashingFunctionDto, Network};
use clap::Parser;
use serde::{Deserialize, Serialize};
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

    /// Path to the transactions JSON file
    #[arg(long, default_value = "txs.json")]
    txs_file: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Transaction {
    source_chain_id: u64,
    destination_chain_id: u64,
    receiver: String,
    amount: String,
    block_number: u64,
    tx_hash: String,
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
    let orders = load_orders(&args.txs_file).expect("Failed to load orders from JSON file");

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

fn load_orders(
    txs_file: &str,
) -> Result<Vec<(Order, ClaimedExecution)>, Box<dyn std::error::Error>> {
    let txs_json = std::fs::read_to_string(txs_file)?;
    let transactions: Vec<Transaction> = serde_json::from_str(&txs_json)?;

    let orders = transactions
        .into_iter()
        .map(|tx| {
            let order = Order {
                source_chain_id: tx.source_chain_id,
                destination_chain_id: tx.destination_chain_id,
                receiver: Address::from_str(&tx.receiver)
                    .unwrap_or_else(|_| panic!("Invalid receiver address: {}", tx.receiver)),
                amount: U256::from_str(&tx.amount)
                    .unwrap_or_else(|_| panic!("Invalid amount: {}", tx.amount)),
                block_number: tx.block_number,
            };

            let claimed_execution = ClaimedExecution {
                chain_id: tx.destination_chain_id,
                tx_hash: FixedBytes::<32>::from_hex(&tx.tx_hash)
                    .unwrap_or_else(|_| panic!("Invalid tx_hash: {}", tx.tx_hash)),
            };

            (order, claimed_execution)
        })
        .collect();

    Ok(orders)
}
