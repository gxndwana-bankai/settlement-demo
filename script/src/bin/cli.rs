use alloy_network::EthereumWallet;
use alloy_primitives::{Address, Bytes, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolCall};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use settlement_lib::Order;
use std::collections::BTreeMap;
use std::fs;
use std::str::FromStr;

sol! {
    #[derive(Debug)]
    struct SolOrder {
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

    function submitOrder(SolOrder memory order) external;
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
    order: Order,
    order_hash: String,
    proof: Vec<String>,
    leaf_index: usize,
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

    /// Path to the transactions JSON file
    #[arg(short = 't', long, default_value = "txs.json", global = true)]
    txs_file: String,

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
    /// Reset orders on one or all chains
    Reset {
        /// Chain name (base-sepolia, arbitrum-sepolia, all)
        #[arg(default_value = "all")]
        chain: String,
    },
    /// Settle orders on a specific chain
    Settle {
        /// Chain name (base-sepolia, arbitrum-sepolia)
        chain: String,
    },
    /// Submit orders from txs.json to a specific destination chain
    Submit {
        /// Destination chain name (base-sepolia, arbitrum-sepolia)
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
        Commands::Reset { chain } => {
            reset_orders(&proof_data, &chain, &private_key, cli.dry_run).await?;
        }
        Commands::Settle { chain } => {
            let chain = Chain::from_name(&chain)?;
            settle_orders(&proof_data, chain, &private_key, cli.dry_run).await?;
        }
        Commands::Submit { chain } => {
            let chain = Chain::from_name(&chain)?;
            submit_orders(&cli.txs_file, chain, &private_key, cli.dry_run).await?;
        }
    }

    Ok(())
}

async fn reset_orders(
    proof_data: &ProofData,
    chain_name: &str,
    private_key: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let chains = if chain_name.to_lowercase() == "all" {
        println!("🔄 Resetting orders on all chains...\n");
        vec![Chain::BaseSepolia, Chain::ArbitrumSepolia]
    } else {
        let chain = Chain::from_name(chain_name)?;
        println!("🔄 Resetting orders on {}...\n", chain.name());
        vec![chain]
    };

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
    } else if chain_name.to_lowercase() == "all" {
        println!("✅ All reset operations completed");
    } else {
        println!("✅ Reset operation completed");
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

async fn submit_orders(
    txs_file: &str,
    chain: Chain,
    private_key: &str,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("📝 Submitting orders to {}...\n", chain.name());

    let config = ChainConfig::load(chain.clone())?;

    let txs_json = fs::read_to_string(txs_file)?;
    let transactions: Vec<Transaction> = serde_json::from_str(&txs_json)?;

    let filtered_txs: Vec<&Transaction> = transactions
        .iter()
        .filter(|tx| tx.source_chain_id == chain.chain_id())
        .collect();

    if filtered_txs.is_empty() {
        println!("ℹ️  No orders found with source {}", chain.name());
        return Ok(());
    }

    println!("📦 Found {} orders to submit", filtered_txs.len());
    for (i, tx) in filtered_txs.iter().enumerate() {
        println!(
            "   {}. From chain {} → {} (amount: {} wei, block: {})",
            i + 1,
            tx.source_chain_id,
            tx.receiver,
            tx.amount,
            tx.block_number
        );
    }
    println!();

    println!("📋 Contract: {}", config.contract_address);
    println!();

    let signer: PrivateKeySigner = private_key.parse()?;
    let wallet = EthereumWallet::from(signer);
    let sender_address = wallet.default_signer().address();

    println!("👤 Sender: {sender_address}\n");

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(config.rpc_url.parse()?);

    let contract_address = Address::from_str(&config.contract_address)?;

    for (i, tx) in filtered_txs.iter().enumerate() {
        println!("📤 [{}/{}] Submitting order...", i + 1, filtered_txs.len());

        let receiver = Address::from_str(&tx.receiver)?;
        let amount = alloy_primitives::U256::from_str(&tx.amount)?;

        let order = Order {
            source_chain_id: tx.source_chain_id,
            destination_chain_id: tx.destination_chain_id,
            receiver,
            amount,
            block_number: tx.block_number,
        };

        let order_hash = order.hash();

        println!("   Order hash: 0x{}", hex::encode(order_hash));

        let sol_order = SolOrder {
            sourceChainId: order.source_chain_id,
            destinationChainId: order.destination_chain_id,
            receiver: order.receiver,
            amount: order.amount,
            blockNumber: order.block_number,
        };

        let call = submitOrderCall { order: sol_order };
        let calldata = call.abi_encode();

        let tx_req = TransactionRequest::default()
            .to(contract_address)
            .input(calldata.into());

        if dry_run {
            println!("   ✅ Dry run - transaction prepared\n");
            continue;
        }

        let pending_tx = provider.send_transaction(tx_req).await?;
        let tx_hash = pending_tx.tx_hash();

        println!("   Tx hash: {tx_hash}");
        println!("   Waiting for confirmation...");

        let receipt = pending_tx.get_receipt().await?;

        if receipt.status() {
            println!("   ✅ Success (Gas: {})\n", receipt.gas_used);
        } else {
            println!("   ❌ Failed\n");
            return Err("Transaction reverted".into());
        }
    }

    if dry_run {
        println!("🔍 Dry run completed - no transactions sent");
    } else {
        println!("✅ All orders submitted successfully!");
    }

    Ok(())
}
