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
                chain_id: 11155111,
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
                chain_id: 11155111,
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
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xa5fe7b6f5690b73777b5e161b56b10ec19247e76ea8b7a4a0f5482c9e98ac6dd",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xdE461aC2B3f61726855b50E599B66cE92C786877").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9453150,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xa5ca78a0322138d2fb4c97d5e970c00cc761428aed13dbc14a71e18c8a75ecad",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x34c5Eec25B29998263CB434bC703FB89f93D2a61").unwrap(),
                amount: U256::from(423014410000000000u64),
                block_number: 9452270,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x30683f26abd98d45bc4f20527d6a05c176804653dddaaac268e8f07a02c36474",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x08163E6500eEbFD27FEd670DE389d37EEa7ca3Cf").unwrap(),
                amount: U256::from(1529878990000000000u64),
                block_number: 9452270,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x40addac4f8c22ae08e4968a824db536a24153e9bc21abf8cd8acb492ab823f5b",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x9eCD8efB5b592786b19cC776D58cD651B553e269").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452270,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xeb7e7752fc50b59c9e3ede27b3286aaf70557c34c0d7ca5c9cc91f8f9a35d7b7",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xed5C648955a4157cbc66b74B0726BC761CfeeD2b").unwrap(),
                amount: U256::from(1715935090000000000u64),
                block_number: 9453085,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x1e1bec5736729bd2ad51d75cdd1739d5afd87e12ea17bb1f9287f700f6daa224",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x82A471Bd3516CD16Ac5c52F5Bd927492771F854c").unwrap(),
                amount: U256::from(2307311820000000000u64),
                block_number: 9452274,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xa9c13d105dd2faedd4f02744cdf1a82d1239333b8de03462d0e4869e61492a7f",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xaEcaBbc487b21fa0BC0BF1586fc49b050864DCf9").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452244,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xd3e34ee6115837640066f239c4b51ca2d7a5995613c8f5b684b68eb55bbf3a51",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x908318c2cEf4ed683c4d76953F6F3a202157DFDc").unwrap(),
                amount: U256::from(2405592970000000000u64),
                block_number: 9452241,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x12bd1f5ab6e9dbd71049c3a78ebbba021e0d84be963a593594f7bd9fe8837305",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x3cD6b56F522181cd15F3e5A954Db55266B2F1D1d").unwrap(),
                amount: U256::from(1082987720000000000u64),
                block_number: 9452291,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x7269740b7d7ec176a8ae33faf2e9d6938904eb0ea252ae755137555e3afa1064",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x61487CEd8F327f8fC1121714e253564Bdd983614").unwrap(),
                amount: U256::from(52348580000000000u64),
                block_number: 9452992,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x6ce7c138ee682b5d77b4576cda23591645798d03ade5e6ff26633fef1df0e1fe",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x2786e48814C2E562A92aE53187b848DBB312d1a4").unwrap(),
                amount: U256::from(2083474010000000000u64),
                block_number: 9452993,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xefc7b1487df313ab4880bbecd47b56c13dfba42316daae84992f2cddee19cc0b",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x3A1D60A48B1104a31133dFBC70E8a589ce8dE57a").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452994,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x5b4d803b91d43ff0198402645e7460490afc61f014438e331144c0c88249e413",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xDc720ddDF0dDAecF594804618507a62D86D96F9c").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452994,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x8ce637d1f88e3578a409ab8ecba738c729beb457a45b19030b0ce1228e58a76f",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xA1A0174FF39455Bd2D183EE06d97F9E19919AD01").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xcb243202a006cd63c6b7046bc1dbfeb74f00e2549dec8c4abd728e1c532edc9f",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xDF8d3846250EDF2bdC2d3dE7526347A985DEBe24").unwrap(),
                amount: U256::from(54323360000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x39cd07dc94d65879a71ac45ca56fd12becca817da2aa8e6545e616f6db25d9d2",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x985e45D9b9Aa0A6C3FE7E76E666739D084d20f4D").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xb081e82264886abbdfa0ff671307065f2f51a07d943c8e399c6b2aee01f4b08a",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xec0bA5EdEA85027b8cedAfeb555d99DCbAd76667").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x1a660d062a6dabc9843b00dab0d61de195df2bef157ed72c87e8e097c40c8b49",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0xc88De420E086A7c77854Fb2A0d1469B8e6122B14").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xc9130f580a304df255d6a34eab7f2c8fd1a1be097a8860a20f51062ceee5d866",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x173d1CBD497aBD16A0Ee25d350be14376860910E").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x0103ae905cc0fbb13baed9ab0d5010515f7aff219ddbfbcb2a92b7ca89197acf",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x7Cce62fAad3635e0974C470F3B03Bb711450DD17").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0x94229e6f8737bf8ead8d6092d98cf76abbc9f616e1c875c0d70077d4d2a2eef9",
                )
                .unwrap(),
            },
        ),
        (
            Order {
                source_chain_id: 84532,         // Base Sepolia
                destination_chain_id: 11155111, // Eth Sepolia
                receiver: Address::from_hex("0x9CdE6de52C9718FBc1D52ab71d4F5cFaf132B4FD").unwrap(),
                amount: U256::from(2500000000000000000u64),
                block_number: 9452998,
            },
            ClaimedExecution {
                chain_id: 11155111,
                tx_hash: FixedBytes::<32>::from_hex(
                    "0xb6bfba0343228d5d5978637d7c4d8781101f799124a73ce858c29013df27ba15",
                )
                .unwrap(),
            },
        ),
    ]
}
