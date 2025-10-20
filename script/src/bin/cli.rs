use alloy_network::EthereumWallet;
use alloy_primitives::{Address, Bytes, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolCall};
use clap::{Parser, Subcommand};
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

    function resetOrders(bytes32[] memory orderHashes) external;
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

#[derive(Debug, Clone)]
enum Chain {
    BaseSepolia,
    ArbitrumSepolia,
}

impl Chain {
    fn from_name(name: &str) -> Result<Self, String> {
        match name.to_lowercase().as_str() {
            "base-sepolia" | "base" => Ok(Chain::BaseSepolia),
            "arbitrum-sepolia" | "arbitrum" | "arb" => Ok(Chain::ArbitrumSepolia),
            _ => Err(format!(
                "Unknown chain: {name}. Supported: base-sepolia, arbitrum-sepolia"
            )),
        }
    }

    fn chain_id(&self) -> u64 {
        match self {
            Chain::BaseSepolia => 84532,
            Chain::ArbitrumSepolia => 421614,
        }
    }

    fn name(&self) -> &str {
        match self {
            Chain::BaseSepolia => "Base Sepolia",
            Chain::ArbitrumSepolia => "Arbitrum Sepolia",
        }
    }

    fn rpc_url(&self) -> Result<String, String> {
        let env_var = match self {
            Chain::BaseSepolia => "BASE_SEPOLIA_RPC",
            Chain::ArbitrumSepolia => "ARB_SEPOLIA_RPC",
        };
        std::env::var(env_var).map_err(|_| format!("{env_var} environment variable not set"))
    }

    fn contract_address(&self) -> Result<String, String> {
        let env_var = match self {
            Chain::BaseSepolia => "BASE_SEPOLIA_CONTRACT",
            Chain::ArbitrumSepolia => "ARB_SEPOLIA_CONTRACT",
        };
        std::env::var(env_var).map_err(|_| format!("{env_var} environment variable not set"))
    }
}

struct ChainConfig {
    chain: Chain,
    rpc_url: String,
    contract_address: String,
}

impl ChainConfig {
    fn load(chain: Chain) -> Result<Self, String> {
        let rpc_url = chain.rpc_url()?;
        let contract_address = chain.contract_address()?;
        Ok(Self {
            chain,
            rpc_url,
            contract_address,
        })
    }
}

#[derive(Parser, Debug)]
#[command(name = "settlement-cli")]
#[command(about = "CLI for interacting with settlement contracts", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to the proof JSON file
    #[arg(short, long, default_value = "proof.json", global = true)]
    proof_file: String,

    /// Private key for signing transactions (from PRIVATE_KEY env var)
    #[arg(
        short = 'k',
        long,
        env = "PRIVATE_KEY",
        global = true,
        hide_env_values = true
    )]
    private_key: Option<String>,

    /// Dry run mode - don't actually send transactions
    #[arg(long, global = true)]
    dry_run: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Reset orders on all chains
    Reset,
    /// Settle orders on a specific chain
    Settle {
        /// Chain name (base-sepolia, arbitrum-sepolia)
        chain: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    let private_key = cli
        .private_key
        .ok_or("PRIVATE_KEY must be set via environment variable or --private-key flag")?;

    let proof_json = fs::read_to_string(&cli.proof_file)?;
    let proof_data: ProofData = serde_json::from_str(&proof_json)?;

    match cli.command {
        Commands::Reset => {
            reset_orders(&proof_data, &private_key, cli.dry_run).await?;
        }
        Commands::Settle { chain } => {
            let chain = Chain::from_name(&chain)?;
            settle_orders(&proof_data, chain, &private_key, cli.dry_run).await?;
        }
    }

    Ok(())
}

async fn reset_orders(
    proof_data: &ProofData,
    private_key: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Resetting orders on all chains...\n");

    let chains = vec![Chain::BaseSepolia, Chain::ArbitrumSepolia];

    for chain in chains {
        let chain_id_str = chain.chain_id().to_string();

        let order_hashes: Vec<FixedBytes<32>> = proof_data
            .proofs_by_source_chain
            .get(&chain_id_str)
            .map(|orders| {
                orders
                    .iter()
                    .map(|op| {
                        let bytes = hex::decode(op.order_hash.trim_start_matches("0x")).unwrap();
                        FixedBytes::<32>::from_slice(&bytes)
                    })
                    .collect()
            })
            .unwrap_or_default();

        if order_hashes.is_empty() {
            println!("⏭️  {} - No orders to reset", chain.name());
            continue;
        }

        let config = match ChainConfig::load(chain.clone()) {
            Ok(c) => c,
            Err(e) => {
                println!("⚠️  {} - Skipping: {}", chain.name(), e);
                continue;
            }
        };

        println!("📍 {} ({} orders)", config.chain.name(), order_hashes.len());
        println!("   Contract: {}", config.contract_address);

        let call = resetOrdersCall {
            orderHashes: order_hashes,
        };
        let calldata = call.abi_encode();

        let contract_address = Address::from_str(&config.contract_address)?;
        let tx = TransactionRequest::default()
            .to(contract_address)
            .input(calldata.into());

        if dry_run {
            println!("   ✅ Dry run - transaction prepared");
            continue;
        }

        let signer: PrivateKeySigner = private_key.parse()?;
        let wallet = EthereumWallet::from(signer);

        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(config.rpc_url.parse()?);

        println!("   📤 Sending transaction...");

        let pending_tx = provider.send_transaction(tx).await?;
        let tx_hash = pending_tx.tx_hash();

        println!("   Tx: {tx_hash}");

        let receipt = pending_tx.get_receipt().await?;

        if receipt.status() {
            println!("   ✅ Success (Gas: {})", receipt.gas_used);
        } else {
            println!("   ❌ Failed");
        }
        println!();
    }

    if dry_run {
        println!("🔍 Dry run completed - no transactions sent");
    } else {
        println!("✅ All reset operations completed");
    }

    Ok(())
}

async fn settle_orders(
    proof_data: &ProofData,
    chain: Chain,
    private_key: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ Settling orders on {}...\n", chain.name());

    let config = ChainConfig::load(chain.clone())?;

    let mut orders_to_settle: Vec<OrderProofJson> = Vec::new();

    for order_proofs in proof_data.proofs_by_source_chain.values() {
        for order_proof in order_proofs {
            if order_proof.order.source_chain_id == chain.chain_id() {
                orders_to_settle.push(order_proof.clone());
            }
        }
    }

    if orders_to_settle.is_empty() {
        let chain_name = chain.name();
        println!("ℹ️  No orders found for {chain_name}");
        return Ok(());
    }

    println!("📦 Found {} orders to settle", orders_to_settle.len());
    for (i, order) in orders_to_settle.iter().enumerate() {
        println!(
            "   {}. {} → {} (amount: {} wei)",
            i + 1,
            order.order.source_chain_id,
            order.order.receiver,
            order.order.amount
        );
    }
    println!();

    let order_proofs: Vec<OrderProof> = orders_to_settle
        .iter()
        .map(|op| {
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

    println!("📋 Transaction Details:");
    println!("   Contract: {}", config.contract_address);
    println!("   Public values: {} bytes", public_values.len());
    println!("   Proof: {} bytes", proof_bytes.len());
    println!("   Calldata: {} bytes", calldata.len());
    println!();

    let contract_address = Address::from_str(&config.contract_address)?;
    let tx = TransactionRequest::default()
        .to(contract_address)
        .input(calldata.into());

    if dry_run {
        println!("✅ Dry run mode - transaction prepared successfully but not sent");
        return Ok(());
    }

    let signer: PrivateKeySigner = private_key.parse()?;
    let wallet = EthereumWallet::from(signer);
    let sender_address = wallet.default_signer().address();

    println!("👤 Sender: {sender_address}");

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(config.rpc_url.parse()?);

    println!("📤 Sending transaction...");

    let pending_tx = provider.send_transaction(tx).await?;
    let tx_hash = pending_tx.tx_hash();

    println!("   Tx hash: {tx_hash}");
    println!("   Waiting for confirmation...");

    let receipt = pending_tx.get_receipt().await?;

    if receipt.status() {
        let block_num = receipt.block_number.unwrap_or_default();
        let gas_used = receipt.gas_used;
        println!("\n✅ Settlement successful!");
        println!("   Block: {block_num}");
        println!("   Gas used: {gas_used}");
    } else {
        println!("\n❌ Transaction failed!");
        return Err("Transaction reverted".into());
    }

    Ok(())
}
