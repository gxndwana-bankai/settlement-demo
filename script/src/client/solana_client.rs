use super::chain_client::{ChainClient, ChainConfig, OrderProofJson, ProofData, Transaction};
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program::ID as SYSTEM_PROGRAM_ID,
    transaction::Transaction as SolanaTransaction,
};
use std::fs;
use std::str::FromStr;

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Order {
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    pub receiver: [u8; 20],
    pub amount: [u8; 32],
    pub block_number: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct OrderProof {
    pub order: Order,
    pub order_hash: [u8; 32],
    pub proof: Vec<[u8; 32]>,
}

fn compute_order_hash(order: &Order) -> [u8; 32] {
    use solana_sdk::keccak::hashv;

    let mut w1 = [0u8; 32];
    w1[24..].copy_from_slice(&order.source_chain_id.to_be_bytes());
    let mut w2 = [0u8; 32];
    w2[24..].copy_from_slice(&order.destination_chain_id.to_be_bytes());
    let mut w3 = [0u8; 32];
    w3[12..].copy_from_slice(&order.receiver);
    let mut w5 = [0u8; 32];
    w5[24..].copy_from_slice(&order.block_number.to_be_bytes());

    hashv(&[&w1, &w2, &w3, &order.amount, &w5]).to_bytes()
}

fn get_discriminator(namespace: &str, name: &str) -> [u8; 8] {
    let preimage = format!("{namespace}:{name}");
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&solana_sdk::hash::hash(preimage.as_bytes()).to_bytes()[..8]);
    discriminator
}

pub struct SolanaClient {
    config: ChainConfig,
    keypair: Keypair,
    rpc_client: RpcClient,
    program_id: Pubkey,
}

impl SolanaClient {
    pub fn new(
        config: ChainConfig,
        solana_private_key: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let keypair = load_solana_keypair(&solana_private_key)?;
        let rpc_client =
            RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());
        let program_id = Pubkey::from_str(&config.contract_address)?;

        Ok(Self {
            config,
            keypair,
            rpc_client,
            program_id,
        })
    }

    fn get_state_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"state"], &self.program_id)
    }

    fn get_order_pda(&self, order_hash: &[u8; 32]) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"order", order_hash], &self.program_id)
    }
}

fn load_solana_keypair(private_key_str: &str) -> Result<Keypair, Box<dyn std::error::Error>> {
    if private_key_str.trim().starts_with('[') {
        let bytes: Vec<u8> = serde_json::from_str(private_key_str)?;
        if bytes.len() != 64 {
            return Err(format!("Solana keypair must be 64 bytes, got {}", bytes.len()).into());
        }
        return Ok(Keypair::try_from(bytes.as_slice())?);
    }

    if let Ok(bytes) = bs58::decode(private_key_str).into_vec() {
        if bytes.len() == 64 {
            return Ok(Keypair::try_from(bytes.as_slice())?);
        }
    }

    if let Ok(contents) = fs::read_to_string(private_key_str) {
        let bytes: Vec<u8> = serde_json::from_str(&contents)?;
        if bytes.len() != 64 {
            return Err(format!(
                "Solana keypair file must contain 64 bytes, got {}",
                bytes.len()
            )
            .into());
        }
        return Ok(Keypair::try_from(bytes.as_slice())?);
    }

    Err(
        "Invalid Solana private key format. Expected JSON array, base58 string, or file path"
            .into(),
    )
}

#[async_trait]
impl ChainClient for SolanaClient {
    async fn initialize(&self, dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Initializing Solana program state...\n");

        println!("👤 Sender: {}", self.keypair.pubkey());
        println!("📋 Program ID: {}", self.program_id);
        println!();

        // Parse the vkey hash from the constant in the program
        const BANKAI_VKEY_HASH: &str =
            "0x001ef62344ca35708c7af9dc5cda28683244720d303d480b47d82136ede2b8ed";
        let vkey_hash_hex = BANKAI_VKEY_HASH.trim_start_matches("0x");
        let vkey_hash_bytes = hex::decode(vkey_hash_hex)?;
        let mut vkey_hash = [0u8; 32];
        vkey_hash.copy_from_slice(&vkey_hash_bytes);

        let (state_pda, _) = self.get_state_pda();

        println!("   State PDA: {state_pda}");
        println!("   VKey Hash: {BANKAI_VKEY_HASH}");
        println!();

        if dry_run {
            println!("✅ Dry run mode - transaction prepared successfully but not sent");
            return Ok(());
        }

        let discriminator = get_discriminator("global", "initialize");
        let mut instruction_data = Vec::new();
        instruction_data.extend_from_slice(&discriminator);
        instruction_data.extend_from_slice(&borsh::to_vec(&vkey_hash)?);

        let accounts = vec![
            AccountMeta::new(state_pda, false),
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ];

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: instruction_data,
        };

        println!("📤 Sending transaction...");

        let recent_blockhash = self.rpc_client.get_latest_blockhash()?;
        let transaction = SolanaTransaction::new_signed_with_payer(
            &[instruction],
            Some(&self.keypair.pubkey()),
            &[&self.keypair],
            recent_blockhash,
        );

        match self.rpc_client.send_and_confirm_transaction(&transaction) {
            Ok(signature) => {
                println!("   Tx signature: {signature}");
                println!("\n✅ Program initialized successfully!");
                println!("\nYou can now submit orders using: cargo run --bin cli -- submit solana-devnet");
            }
            Err(e) => {
                println!("\n❌ Initialization failed: {e}");
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn submit_orders(
        &self,
        transactions: &[Transaction],
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("📝 Submitting orders to {}...\n", self.config.chain.name());

        println!("👤 Sender: {}", self.keypair.pubkey());
        println!("📋 Program ID: {}", self.program_id);
        println!();

        let filtered_txs: Vec<&Transaction> = transactions
            .iter()
            .filter(|tx| tx.source_chain_id == self.config.chain.chain_id())
            .collect();

        if filtered_txs.is_empty() {
            println!(
                "ℹ️  No orders found with source {}",
                self.config.chain.name()
            );
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

        let (state_pda, _) = self.get_state_pda();

        for (i, tx) in filtered_txs.iter().enumerate() {
            println!("📤 [{}/{}] Submitting order...", i + 1, filtered_txs.len());

            let receiver_hex = tx.receiver.trim_start_matches("0x");
            let receiver_bytes = hex::decode(receiver_hex)?;
            if receiver_bytes.len() != 20 {
                return Err(
                    format!("Invalid receiver address length: {}", receiver_bytes.len()).into(),
                );
            }
            let mut receiver = [0u8; 20];
            receiver.copy_from_slice(&receiver_bytes);

            let amount = alloy_primitives::U256::from_str(&tx.amount)?;
            let amount_bytes: [u8; 32] = amount.to_be_bytes();

            let order = Order {
                source_chain_id: tx.source_chain_id,
                destination_chain_id: tx.destination_chain_id,
                receiver,
                amount: amount_bytes,
                block_number: tx.block_number,
            };

            let order_hash = compute_order_hash(&order);
            println!("   Order hash: 0x{}", hex::encode(order_hash));

            let (order_pda, _) = self.get_order_pda(&order_hash);

            if dry_run {
                println!("   ✅ Dry run - transaction prepared\n");
                continue;
            }

            let discriminator = get_discriminator("global", "submit_order");
            let mut instruction_data = Vec::new();
            instruction_data.extend_from_slice(&discriminator);
            instruction_data.extend_from_slice(&borsh::to_vec(&order)?);
            instruction_data.extend_from_slice(&order_hash);

            let accounts = vec![
                AccountMeta::new(state_pda, false),
                AccountMeta::new(order_pda, false),
                AccountMeta::new(self.keypair.pubkey(), true),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            ];

            let instruction = Instruction {
                program_id: self.program_id,
                accounts,
                data: instruction_data,
            };

            let recent_blockhash = self.rpc_client.get_latest_blockhash()?;
            let transaction = SolanaTransaction::new_signed_with_payer(
                &[instruction],
                Some(&self.keypair.pubkey()),
                &[&self.keypair],
                recent_blockhash,
            );

            match self.rpc_client.send_and_confirm_transaction(&transaction) {
                Ok(signature) => {
                    println!("   Tx signature: {signature}");
                    println!("   ✅ Success\n");
                }
                Err(e) => {
                    println!("   ❌ Failed: {e}\n");
                    return Err(e.into());
                }
            }
        }

        if dry_run {
            println!("🔍 Dry run completed - no transactions sent");
        } else {
            println!("✅ All orders submitted successfully!");
        }

        Ok(())
    }

    async fn settle_orders(
        &self,
        proof_data: &ProofData,
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("⚡ Settling orders on {}...\n", self.config.chain.name());

        let mut orders_to_settle: Vec<OrderProofJson> = Vec::new();

        for order_proofs in proof_data.proofs_by_source_chain.values() {
            for order_proof in order_proofs {
                if order_proof.order.source_chain_id == self.config.chain.chain_id() {
                    orders_to_settle.push(order_proof.clone());
                }
            }
        }

        if orders_to_settle.is_empty() {
            println!("ℹ️  No orders found for {}", self.config.chain.name());
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

        let order_proofs: Vec<OrderProof> = orders_to_settle
            .iter()
            .map(|op| {
                let order_hash_bytes = hex::decode(op.order_hash.trim_start_matches("0x")).unwrap();
                let mut order_hash = [0u8; 32];
                order_hash.copy_from_slice(&order_hash_bytes);

                let proof: Vec<[u8; 32]> = op
                    .proof
                    .iter()
                    .map(|p| {
                        let bytes = hex::decode(p.trim_start_matches("0x")).unwrap();
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        arr
                    })
                    .collect();

                let receiver_bytes = op.order.receiver.as_slice();
                let mut receiver = [0u8; 20];
                receiver.copy_from_slice(&receiver_bytes[..20]);

                let amount_bytes = op.order.amount.to_be_bytes();

                OrderProof {
                    order: Order {
                        source_chain_id: op.order.source_chain_id,
                        destination_chain_id: op.order.destination_chain_id,
                        receiver,
                        amount: amount_bytes,
                        block_number: op.order.block_number,
                    },
                    order_hash,
                    proof,
                }
            })
            .collect();

        let sp1_public_inputs = hex::decode(proof_data.public_values.trim_start_matches("0x"))?;
        let groth16_proof = hex::decode(proof_data.proof.trim_start_matches("0x"))?;

        println!("📋 Transaction Details:");
        println!("   Program ID: {}", self.program_id);
        println!("   Public values: {} bytes", sp1_public_inputs.len());
        println!("   Proof: {} bytes", groth16_proof.len());
        println!();

        if dry_run {
            println!("✅ Dry run mode - transaction prepared successfully but not sent");
            return Ok(());
        }

        let discriminator = get_discriminator("global", "settle_orders");
        let mut instruction_data = Vec::new();
        instruction_data.extend_from_slice(&discriminator);
        instruction_data.extend_from_slice(&borsh::to_vec(&sp1_public_inputs)?);
        instruction_data.extend_from_slice(&borsh::to_vec(&groth16_proof)?);
        instruction_data.extend_from_slice(&borsh::to_vec(&order_proofs)?);

        let (state_pda, _) = self.get_state_pda();

        let mut accounts = vec![AccountMeta::new(state_pda, false)];

        for op in &order_proofs {
            let (order_pda, _) = self.get_order_pda(&op.order_hash);
            accounts.push(AccountMeta::new(order_pda, false));
        }

        let settle_instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: instruction_data,
        };

        // Add compute budget instruction to increase compute units for proof verification
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

        println!("📤 Sending transaction...");
        println!("   Using increased compute budget: 1,400,000 units");

        let recent_blockhash = self.rpc_client.get_latest_blockhash()?;
        let transaction = SolanaTransaction::new_signed_with_payer(
            &[compute_budget_ix, settle_instruction],
            Some(&self.keypair.pubkey()),
            &[&self.keypair],
            recent_blockhash,
        );

        match self.rpc_client.send_and_confirm_transaction(&transaction) {
            Ok(signature) => {
                println!("   Tx signature: {signature}");
                println!("\n✅ Settlement successful!");
            }
            Err(e) => {
                println!("\n❌ Transaction failed: {e}");
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn reset_orders(
        &self,
        proof_data: &ProofData,
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Resetting orders on {}...\n", self.config.chain.name());

        let chain_id_str = self.config.chain.chain_id().to_string();

        let order_hashes: Vec<[u8; 32]> = proof_data
            .proofs_by_source_chain
            .get(&chain_id_str)
            .map(|orders| {
                orders
                    .iter()
                    .map(|op| {
                        let bytes = hex::decode(op.order_hash.trim_start_matches("0x")).unwrap();
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        arr
                    })
                    .collect()
            })
            .unwrap_or_default();

        if order_hashes.is_empty() {
            println!("⏭️  No orders to reset");
            return Ok(());
        }

        println!(
            "📍 {} ({} orders)",
            self.config.chain.name(),
            order_hashes.len()
        );
        println!("   Program ID: {}", self.program_id);

        if dry_run {
            println!("   ✅ Dry run - transaction prepared");
            return Ok(());
        }

        let discriminator = get_discriminator("global", "reset_orders");
        let mut instruction_data = Vec::new();
        instruction_data.extend_from_slice(&discriminator);
        instruction_data.extend_from_slice(&borsh::to_vec(&order_hashes)?);

        let (state_pda, _) = self.get_state_pda();

        let mut accounts = vec![AccountMeta::new(state_pda, false)];

        for order_hash in &order_hashes {
            let (order_pda, _) = self.get_order_pda(order_hash);
            accounts.push(AccountMeta::new(order_pda, false));
        }

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: instruction_data,
        };

        println!("   📤 Sending transaction...");

        let recent_blockhash = self.rpc_client.get_latest_blockhash()?;
        let transaction = SolanaTransaction::new_signed_with_payer(
            &[instruction],
            Some(&self.keypair.pubkey()),
            &[&self.keypair],
            recent_blockhash,
        );

        match self.rpc_client.send_and_confirm_transaction(&transaction) {
            Ok(signature) => {
                println!("   Tx: {signature}");
                println!("   ✅ Success");
            }
            Err(e) => {
                println!("   ❌ Failed: {e}");
                return Err(e.into());
            }
        }

        Ok(())
    }
}
