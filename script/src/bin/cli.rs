use clap::{Parser, Subcommand};
use settlement_script::client::{
    Chain, ChainClient, ChainConfig, EvmClient, ProofData, SolanaClient, Transaction,
};
use std::fs;

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

    /// EVM Private key for signing transactions (from PRIVATE_KEY env var)
    #[arg(
        short = 'k',
        long,
        env = "PRIVATE_KEY",
        global = true,
        hide_env_values = true
    )]
    private_key: Option<String>,

    /// Solana keypair path or JSON array (from SOLANA_PRIVATE_KEY env var)
    #[arg(
        long,
        env = "SOLANA_PRIVATE_KEY",
        global = true,
        hide_env_values = true
    )]
    solana_private_key: Option<String>,

    /// Dry run mode - don't actually send transactions
    #[arg(long, global = true)]
    dry_run: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize the Solana program state (only needed once)
    Initialize {
        /// Chain name (solana-devnet)
        chain: String,
    },
    /// Reset orders on one or all chains
    Reset {
        /// Chain name (base-sepolia, arbitrum-sepolia, solana-devnet, all)
        #[arg(default_value = "all")]
        chain: String,
    },
    /// Settle orders on a specific chain
    Settle {
        /// Chain name (base-sepolia, arbitrum-sepolia, solana-devnet)
        chain: String,
    },
    /// Submit orders from txs.json to a specific destination chain
    Submit {
        /// Chain name (base-sepolia, arbitrum-sepolia, solana-devnet)
        chain: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    let proof_json = fs::read_to_string(&cli.proof_file)?;
    let proof_data: ProofData = serde_json::from_str(&proof_json)?;

    match &cli.command {
        Commands::Initialize { chain } => {
            let chain = Chain::from_name(chain)?;
            if !chain.is_solana() {
                return Err("Initialize command is only supported for Solana chains".into());
            }
            let client = create_client(chain, &cli)?;
            client.initialize(cli.dry_run).await?;
        }
        Commands::Reset { chain } => {
            if chain.to_lowercase() == "all" {
                println!("🔄 Resetting orders on all chains...\n");
                for chain in [Chain::BaseSepolia, Chain::ArbitrumSepolia] {
                    match create_client(chain, &cli) {
                        Ok(client) => {
                            if let Err(e) = client.reset_orders(&proof_data, cli.dry_run).await {
                                eprintln!("Error resetting orders: {e}");
                            }
                            println!();
                        }
                        Err(e) => {
                            println!("⚠️  Skipping chain: {e}");
                            println!();
                        }
                    }
                }
                if cli.dry_run {
                    println!("🔍 Dry run completed - no transactions sent");
                } else {
                    println!("✅ All reset operations completed");
                }
            } else {
                let chain = Chain::from_name(chain)?;
                let client = create_client(chain, &cli)?;
                client.reset_orders(&proof_data, cli.dry_run).await?;
            }
        }
        Commands::Settle { chain } => {
            let chain = Chain::from_name(chain)?;
            let client = create_client(chain, &cli)?;
            client.settle_orders(&proof_data, cli.dry_run).await?;
        }
        Commands::Submit { chain } => {
            let chain = Chain::from_name(chain)?;
            let txs_json = fs::read_to_string(&cli.txs_file)?;
            let transactions: Vec<Transaction> = serde_json::from_str(&txs_json)?;
            let client = create_client(chain, &cli)?;
            client.submit_orders(&transactions, cli.dry_run).await?;
        }
    }

    Ok(())
}

fn create_client(
    chain: Chain,
    cli: &Cli,
) -> Result<Box<dyn ChainClient>, Box<dyn std::error::Error>> {
    let config = ChainConfig::load(chain.clone())?;

    match chain {
        Chain::BaseSepolia | Chain::ArbitrumSepolia => {
            let private_key = cli
                .private_key
                .as_ref()
                .ok_or("PRIVATE_KEY must be set for EVM chains")?;
            Ok(Box::new(EvmClient::new(config, private_key.clone())?))
        }
        Chain::SolanaDevnet => {
            let solana_private_key = cli
                .solana_private_key
                .as_ref()
                .ok_or("SOLANA_PRIVATE_KEY must be set for Solana chains")?;
            Ok(Box::new(SolanaClient::new(
                config,
                solana_private_key.clone(),
            )?))
        }
    }
}
