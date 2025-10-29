use super::chain_client::{ChainClient, ChainConfig, OrderProofJson, ProofData, Transaction};
use async_trait::async_trait;
use garaga_rs::calldata::full_proof_with_hints::groth16::{
    get_groth16_calldata_felt, get_sp1_vk, Groth16Proof,
};
use garaga_rs::definitions::CurveID;
use starknet::{
    accounts::{Account, ExecutionEncoding, SingleOwnerAccount},
    core::{
        types::{BlockId, BlockTag, Call, Felt},
        utils::get_selector_from_name,
    },
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider, Url},
    signers::{LocalWallet, SigningKey},
};
use std::str::FromStr;

pub struct StarknetClient {
    config: ChainConfig,
}

impl StarknetClient {
    pub fn new(config: ChainConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { config })
    }

    async fn create_account(
        &self,
    ) -> Result<
        SingleOwnerAccount<JsonRpcClient<HttpTransport>, LocalWallet>,
        Box<dyn std::error::Error>,
    > {
        let provider = JsonRpcClient::new(HttpTransport::new(Url::parse(&self.config.rpc_url)?));

        let private_key = std::env::var("STARKNET_PRIVATE_KEY")?;
        let private_key_felt = Felt::from_hex(&private_key)?;
        let signer = LocalWallet::from(SigningKey::from_secret_scalar(private_key_felt));

        let address = Felt::from_hex(&std::env::var("STARKNET_ACCOUNT_ADDRESS")?)?;

        let chain_id = provider.chain_id().await?;

        let mut account =
            SingleOwnerAccount::new(provider, signer, address, chain_id, ExecutionEncoding::New);

        // Use latest block instead of pending to avoid RPC issues
        account.set_block_id(BlockId::Tag(BlockTag::Latest));

        Ok(account)
    }

    fn generate_proof_calldata(
        &self,
        proof_data: &ProofData,
    ) -> Result<Vec<Felt>, Box<dyn std::error::Error>> {
        println!("🔧 Generating proof calldata on the fly with Garaga...");

        // Load proof.json
        let proof_json = std::fs::read_to_string("proof.json")?;
        let proof_obj: serde_json::Value = serde_json::from_str(&proof_json)?;

        let vkey = proof_obj["vkey"]
            .as_str()
            .ok_or("Missing vkey in proof.json")?;
        let public_values = proof_obj["publicValues"]
            .as_str()
            .ok_or("Missing publicValues in proof.json")?;
        let proof = proof_obj["proof"]
            .as_str()
            .ok_or("Missing proof in proof.json")?;

        // Decode hex strings
        let vkey_bytes = hex::decode(vkey.trim_start_matches("0x"))?;
        let public_values_bytes = hex::decode(public_values.trim_start_matches("0x"))?;
        let proof_bytes = hex::decode(proof.trim_start_matches("0x"))?;

        // Generate Groth16 proof with Garaga
        let sp1_groth16_vk = get_sp1_vk();
        let groth16_proof = Groth16Proof::from_sp1(vkey_bytes, public_values_bytes, proof_bytes);
        let calldata_bigint =
            get_groth16_calldata_felt(&groth16_proof, &sp1_groth16_vk, CurveID::BN254)?;

        // Convert to Starknet Felt
        let calldata: Vec<Felt> = calldata_bigint
            .iter()
            .map(|bigint| {
                let hex_str = format!("{:064x}", bigint.to_biguint());
                Felt::from_hex(&format!("0x{hex_str}")).unwrap()
            })
            .collect();

        println!("   ✅ Generated {} calldata elements", calldata.len());

        Ok(calldata)
    }
}

#[async_trait]
impl ChainClient for StarknetClient {
    async fn submit_orders(
        &self,
        transactions: &[Transaction],
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("📝 Submitting orders to {}...\n", self.config.chain.name());

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

        println!("📋 Contract: {}", self.config.contract_address);
        println!();

        let account = self.create_account().await?;
        let contract_address = Felt::from_hex(&self.config.contract_address)?;

        for (i, tx) in filtered_txs.iter().enumerate() {
            println!("📤 [{}/{}] Submitting order...", i + 1, filtered_txs.len());

            // Convert chain IDs and block number to u64
            let source_chain_id = Felt::from(tx.source_chain_id);
            let destination_chain_id = Felt::from(tx.destination_chain_id);
            let block_number = Felt::from(tx.block_number);

            // Convert receiver address to u256 (low, high)
            // Ethereum address is 20 bytes, pad to 32 bytes for u256
            let receiver_hex = tx.receiver.trim_start_matches("0x");
            let mut receiver_bytes = [0u8; 32];
            let addr_bytes = hex::decode(receiver_hex)?;
            receiver_bytes[32 - addr_bytes.len()..].copy_from_slice(&addr_bytes);

            let receiver_low = Felt::from_bytes_be_slice(&receiver_bytes[16..]);
            let receiver_high = Felt::from_bytes_be_slice(&receiver_bytes[..16]);

            // Convert amount to u256 (low, high)
            let amount_u256 = u128::from_str(&tx.amount)?;
            let amount_low = Felt::from(amount_u256);
            let amount_high = Felt::ZERO;

            // Order struct serialization: [source_chain_id, destination_chain_id, receiver_low, receiver_high, amount_low, amount_high, block_number]
            let calldata = vec![
                source_chain_id,      // u64
                destination_chain_id, // u64
                receiver_low,         // u256.low
                receiver_high,        // u256.high
                amount_low,           // u256.low
                amount_high,          // u256.high
                block_number,         // u64
            ];

            println!(
                "   Order: {}-{} amount {} block {}",
                tx.source_chain_id, tx.destination_chain_id, tx.amount, tx.block_number
            );
            println!("   Calldata: {calldata:?}");

            if dry_run {
                println!("   ✅ Dry run - transaction prepared\n");
                continue;
            }

            let call = Call {
                to: contract_address,
                selector: get_selector_from_name("submit_order")?,
                calldata,
            };

            match account
                .execute_v3(vec![call])
                .l1_gas(2_000_000u64)
                .l1_gas_price(1_000_000_000u128)
                .send()
                .await
            {
                Ok(result) => {
                    println!("   Tx hash: {:#064x}", result.transaction_hash);
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
        println!();

        let proof_calldata = self.generate_proof_calldata(proof_data)?;
        println!();

        let mut calldata = Vec::new();

        calldata.push(Felt::from(proof_calldata.len()));
        calldata.extend(proof_calldata);

        calldata.push(Felt::from(orders_to_settle.len()));

        for order_proof in &orders_to_settle {
            let order_hash_bytes = hex::decode(order_proof.order_hash.trim_start_matches("0x"))?;
            let order_hash_low = Felt::from_bytes_be_slice(&order_hash_bytes[16..]);
            let order_hash_high = Felt::from_bytes_be_slice(&order_hash_bytes[..16]);

            calldata.push(order_hash_low);
            calldata.push(order_hash_high);

            calldata.push(Felt::from(order_proof.proof.len()));
            for proof_element in &order_proof.proof {
                let proof_bytes = hex::decode(proof_element.trim_start_matches("0x"))?;
                let proof_low = Felt::from_bytes_be_slice(&proof_bytes[16..]);
                let proof_high = Felt::from_bytes_be_slice(&proof_bytes[..16]);
                calldata.push(proof_low);
                calldata.push(proof_high);
            }

            let leaf_index_low = Felt::from(order_proof.leaf_index as u64);
            let leaf_index_high = Felt::ZERO;
            calldata.push(leaf_index_low);
            calldata.push(leaf_index_high);
        }

        println!("📋 Transaction Details:");
        println!("   Contract: {}", self.config.contract_address);
        println!("   Total calldata elements: {}", calldata.len());
        println!();

        if dry_run {
            println!("✅ Dry run mode - transaction prepared successfully but not sent");
            return Ok(());
        }

        let account = self.create_account().await?;
        let contract_address = Felt::from_hex(&self.config.contract_address)?;

        println!("📤 Sending transaction...");

        let call = Call {
            to: contract_address,
            selector: get_selector_from_name("settle_orders")?,
            calldata,
        };

        match account
            .execute_v3(vec![call])
            .l1_gas(50_000_000u64)
            .l1_gas_price(1_000_000_000u128)
            .send()
            .await
        {
            Ok(result) => {
                println!("   Tx hash: {:#064x}", result.transaction_hash);
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

        let order_hashes: Vec<(Felt, Felt)> = proof_data
            .proofs_by_source_chain
            .get(&chain_id_str)
            .map(|orders| {
                orders
                    .iter()
                    .map(|op| {
                        let bytes = hex::decode(op.order_hash.trim_start_matches("0x")).unwrap();
                        let low = Felt::from_bytes_be_slice(&bytes[16..]);
                        let high = Felt::from_bytes_be_slice(&bytes[..16]);
                        (low, high)
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
        println!("   Contract: {}", self.config.contract_address);

        let mut calldata = vec![Felt::from(order_hashes.len())];
        for (low, high) in order_hashes {
            calldata.push(low);
            calldata.push(high);
        }

        if dry_run {
            println!("   ✅ Dry run - transaction prepared");
            return Ok(());
        }

        let account = self.create_account().await?;
        let contract_address = Felt::from_hex(&self.config.contract_address)?;

        println!("   📤 Sending transaction...");

        let call = Call {
            to: contract_address,
            selector: get_selector_from_name("reset_orders")?,
            calldata,
        };

        match account
            .execute_v3(vec![call])
            .l1_gas(5_000_000u64)
            .l1_gas_price(1_000_000_000u128)
            .send()
            .await
        {
            Ok(result) => {
                println!("   Tx: {:#064x}", result.transaction_hash);
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
