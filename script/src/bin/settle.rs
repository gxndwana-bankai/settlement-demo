use alloy_network::EthereumWallet;
use alloy_primitives::{Address, Bytes, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolCall};
use clap::Parser;
use dotenv;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;

sol! {
    #[derive(Debug)]
    struct Order {
        uint64 sourceChainId;
        uint64 destinationChainId;
        address receiver;
        uint256 amount;
        uint64 blockNumber;
    }

    #[derive(Debug)]
    struct OrderProof {
        bytes32 orderHash;
        bytes32[] proof;
        uint256 leafIndex;
    }

    function settleOrders(
        bytes calldata publicValues,
        bytes calldata proofBytes,
        OrderProof[] memory orderProofs
    ) external;
}

#[derive(Debug, Deserialize, Serialize)]
struct ProofData {
    proof: String,
    #[serde(rename = "publicValues")]
    public_values: String,
    vkey: String,
    #[serde(rename = "merkleRoot")]
    merkle_root: String,
    #[serde(rename = "proofsBySourceChain")]
    proofs_by_source_chain: BTreeMap<String, Vec<OrderProofJson>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OrderProofJson {
    order: OrderJson,
    order_hash: String,
    proof: Vec<String>,
    leaf_index: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OrderJson {
    source_chain_id: u64,
    destination_chain_id: u64,
    receiver: String,
    amount: String,
    block_number: u64,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Settle orders on a destination chain", long_about = None)]
struct Args {
    /// Path to the proof JSON file
    #[arg(short, long, default_value = "proof.json")]
    proof_file: String,

    /// Destination chain ID to settle orders for
    #[arg(short, long)]
    chain_id: u64,

    /// Settlement contract address on the destination chain
    /// Falls back to VERIFIER_CONTRACT env var
    #[arg(long, env = "VERIFIER_CONTRACT")]
    contract: Option<String>,

    /// RPC URL for the destination chain
    /// Falls back to BASE_SEPOLIA_RPC or ARB_SEPOLIA_RPC based on chain_id
    #[arg(short, long)]
    rpc_url: Option<String>,

    /// Private key for signing transactions
    #[arg(short = 'k', long, env = "PRIVATE_KEY")]
    private_key: String,

    /// Dry run mode - don't actually send the transaction
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let args = Args::parse();

    // Resolve contract address
    let contract = args
        .contract
        .ok_or("Contract address not provided. Set via --contract or VERIFIER_CONTRACT env var")?;

    // Resolve RPC URL - auto-detect based on chain ID if not provided
    let rpc_url = args.rpc_url.or_else(|| {
        match args.chain_id {
            84532 => std::env::var("BASE_SEPOLIA_RPC").ok(), // Base Sepolia
            421614 => std::env::var("ARB_SEPOLIA_RPC").ok(), // Arbitrum Sepolia
            _ => None,
        }
    }).ok_or_else(|| {
        format!(
            "RPC URL not provided. Set via --rpc-url or set BASE_SEPOLIA_RPC/ARB_SEPOLIA_RPC env var for chain {}",
            args.chain_id
        )
    })?;

    // Read and parse the proof JSON file
    println!("Reading proof data from: {}", args.proof_file);
    let proof_json = fs::read_to_string(&args.proof_file)?;
    let proof_data: ProofData = serde_json::from_str(&proof_json)?;

    // Filter orders by destination chain ID
    let mut orders_to_settle: Vec<OrderProofJson> = Vec::new();

    for (_source_chain, order_proofs) in &proof_data.proofs_by_source_chain {
        for order_proof in order_proofs {
            if order_proof.order.destination_chain_id == args.chain_id {
                orders_to_settle.push(order_proof.clone());
            }
        }
    }

    if orders_to_settle.is_empty() {
        println!(
            "No orders found for destination chain ID: {}",
            args.chain_id
        );
        return Ok(());
    }

    println!(
        "Found {} orders to settle for chain {}",
        orders_to_settle.len(),
        args.chain_id
    );

    // Convert to Solidity types
    let order_proofs: Vec<OrderProof> = orders_to_settle
        .iter()
        .map(|op| {
            // Parse hex strings to FixedBytes
            let order_hash_bytes = hex::decode(op.order_hash.trim_start_matches("0x")).unwrap();
            let order_hash = FixedBytes::<32>::from_slice(&order_hash_bytes);

            let proof: Vec<FixedBytes<32>> = op
                .proof
                .iter()
                .map(|p| {
                    let bytes = hex::decode(p.trim_start_matches("0x")).unwrap();
                    FixedBytes::<32>::from_slice(&bytes)
                })
                .collect();

            OrderProof {
                orderHash: order_hash,
                proof,
                leafIndex: alloy_primitives::U256::from(op.leaf_index),
            }
        })
        .collect();

    // Prepare transaction data
    let public_values_bytes = hex::decode(proof_data.public_values.trim_start_matches("0x"))?;
    let public_values = Bytes::from(public_values_bytes);

    let proof_bytes_vec = hex::decode(proof_data.proof.trim_start_matches("0x"))?;
    let proof_bytes = Bytes::from(proof_bytes_vec);

    let call = settleOrdersCall {
        publicValues: public_values.clone(),
        proofBytes: proof_bytes.clone(),
        orderProofs: order_proofs,
    };

    let calldata = call.abi_encode();

    println!("Preparing transaction...");
    println!("Contract: {}", contract);
    println!("RPC: {}", rpc_url);
    println!("Public values length: {} bytes", public_values.len());
    println!("Proof length: {} bytes", proof_bytes.len());
    println!("Number of order proofs: {}", orders_to_settle.len());

    let contract_address = Address::from_str(&contract)?;

    println!("\nTransaction Details:");
    println!("To: {}", contract_address);
    println!("Calldata size: {} bytes", calldata.len());

    // Build transaction
    let tx = TransactionRequest::default()
        .to(contract_address)
        .input(calldata.into());

    if args.dry_run {
        println!("\n✅ Dry run mode - transaction prepared successfully but not sent.");
        return Ok(());
    }

    // Set up signer
    let signer: PrivateKeySigner = args.private_key.parse()?;
    let wallet = EthereumWallet::from(signer);
    let sender_address = wallet.default_signer().address();

    println!("Sender address: {}", sender_address);

    // Set up provider with wallet
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.parse()?);

    println!("\n📤 Sending transaction...");

    let pending_tx = provider.send_transaction(tx).await?;
    let tx_hash = pending_tx.tx_hash();

    println!("Transaction hash: {}", tx_hash);
    println!("Waiting for confirmation...");

    let receipt = pending_tx.get_receipt().await?;

    if receipt.status() {
        println!("\n✅ Transaction successful!");
        println!("Block number: {}", receipt.block_number.unwrap_or_default());
        println!("Gas used: {}", receipt.gas_used);
    } else {
        println!("\n❌ Transaction failed!");
    }

    Ok(())
}
