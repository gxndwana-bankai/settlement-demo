use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use settlement_lib::Order;
use std::collections::BTreeMap;

#[async_trait]
pub trait ChainClient: Send + Sync {
    async fn initialize(&self, _dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
        Err("Initialize not supported for this chain type".into())
    }

    async fn submit_orders(
        &self,
        transactions: &[Transaction],
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>>;

    async fn settle_orders(
        &self,
        proof_data: &ProofData,
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>>;

    async fn reset_orders(
        &self,
        proof_data: &ProofData,
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProofData {
    pub proof: String,
    #[serde(rename = "publicValues")]
    pub public_values: String,
    pub vkey: String,
    #[serde(rename = "merkleRoot")]
    pub merkle_root: String,
    #[serde(rename = "proofsBySourceChain")]
    pub proofs_by_source_chain: BTreeMap<String, Vec<OrderProofJson>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OrderProofJson {
    pub order: Order,
    pub order_hash: String,
    pub proof: Vec<String>,
    pub leaf_index: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Transaction {
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    pub receiver: String,
    pub amount: String,
    pub block_number: u64,
    pub tx_hash: String,
}

#[derive(Debug, Clone)]
pub enum Chain {
    BaseSepolia,
    ArbitrumSepolia,
    SolanaDevnet,
    StarknetSepolia,
}

impl Chain {
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name.to_lowercase().as_str() {
            "base-sepolia" | "base" => Ok(Chain::BaseSepolia),
            "arbitrum-sepolia" | "arbitrum" | "arb" => Ok(Chain::ArbitrumSepolia),
            "solana-devnet" | "solana" => Ok(Chain::SolanaDevnet),
            "starknet-sepolia" | "starknet" => Ok(Chain::StarknetSepolia),
            _ => Err(format!(
                "Unknown chain: {name}. Supported: base-sepolia, arbitrum-sepolia, solana-devnet, starknet-sepolia"
            )),
        }
    }

    pub fn chain_id(&self) -> u64 {
        match self {
            Chain::BaseSepolia => 84532,
            Chain::ArbitrumSepolia => 421614,
            Chain::SolanaDevnet => 103,
            Chain::StarknetSepolia => 393402133025997798, // Numeric representation for Starknet Sepolia
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Chain::BaseSepolia => "Base Sepolia",
            Chain::ArbitrumSepolia => "Arbitrum Sepolia",
            Chain::SolanaDevnet => "Solana Devnet",
            Chain::StarknetSepolia => "Starknet Sepolia",
        }
    }

    pub fn is_evm(&self) -> bool {
        matches!(self, Chain::BaseSepolia | Chain::ArbitrumSepolia)
    }

    pub fn is_solana(&self) -> bool {
        matches!(self, Chain::SolanaDevnet)
    }

    pub fn is_starknet(&self) -> bool {
        matches!(self, Chain::StarknetSepolia)
    }

    fn rpc_url(&self) -> Result<String, String> {
        let env_var = match self {
            Chain::BaseSepolia => "BASE_SEPOLIA_RPC",
            Chain::ArbitrumSepolia => "ARB_SEPOLIA_RPC",
            Chain::SolanaDevnet => "SOLANA_DEVNET_RPC",
            Chain::StarknetSepolia => "STARKNET_SEPOLIA_RPC",
        };
        std::env::var(env_var).map_err(|_| format!("{env_var} environment variable not set"))
    }

    fn contract_address(&self) -> Result<String, String> {
        let env_var = match self {
            Chain::BaseSepolia => "BASE_SEPOLIA_CONTRACT",
            Chain::ArbitrumSepolia => "ARB_SEPOLIA_CONTRACT",
            Chain::SolanaDevnet => "SOLANA_DEVNET_PROGRAM",
            Chain::StarknetSepolia => "STARKNET_SEPOLIA_CONTRACT",
        };
        std::env::var(env_var).map_err(|_| format!("{env_var} environment variable not set"))
    }
}

pub struct ChainConfig {
    pub chain: Chain,
    pub rpc_url: String,
    pub contract_address: String,
}

impl ChainConfig {
    pub fn load(chain: Chain) -> Result<Self, String> {
        let rpc_url = chain.rpc_url()?;
        let contract_address = chain.contract_address()?;
        Ok(Self {
            chain,
            rpc_url,
            contract_address,
        })
    }
}
