use super::chain_client::{ChainClient, ChainConfig, OrderProofJson, ProofData, Transaction};
use alloy_network::EthereumWallet;
use alloy_primitives::{Address, Bytes, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolCall};
use async_trait::async_trait;
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

pub struct EvmClient {
    config: ChainConfig,
    private_key: String,
}

impl EvmClient {
    pub fn new(
        config: ChainConfig,
        private_key: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            config,
            private_key,
        })
    }

    fn create_provider(&self) -> Result<impl Provider, Box<dyn std::error::Error>> {
        let signer: PrivateKeySigner = self.private_key.parse()?;
        let wallet = EthereumWallet::from(signer);
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(self.config.rpc_url.parse()?);
        Ok(provider)
    }
}

#[async_trait]
impl ChainClient for EvmClient {
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

        let provider = self.create_provider()?;
        let contract_address = Address::from_str(&self.config.contract_address)?;

        for (i, tx) in filtered_txs.iter().enumerate() {
            println!("📤 [{}/{}] Submitting order...", i + 1, filtered_txs.len());

            let receiver = Address::from_str(&tx.receiver)?;
            let amount = alloy_primitives::U256::from_str(&tx.amount)?;

            let order = settlement_lib::Order {
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
        println!("   Contract: {}", self.config.contract_address);
        println!("   Public values: {} bytes", public_values.len());
        println!("   Proof: {} bytes", proof_bytes.len());
        println!("   Calldata: {} bytes", calldata.len());
        println!();

        let contract_address = Address::from_str(&self.config.contract_address)?;
        let tx = TransactionRequest::default()
            .to(contract_address)
            .input(calldata.into());

        if dry_run {
            println!("✅ Dry run mode - transaction prepared successfully but not sent");
            return Ok(());
        }

        let provider = self.create_provider()?;

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

    async fn reset_orders(
        &self,
        proof_data: &ProofData,
        dry_run: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Resetting orders on {}...\n", self.config.chain.name());

        let chain_id_str = self.config.chain.chain_id().to_string();

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
            println!("⏭️  No orders to reset");
            return Ok(());
        }

        println!(
            "📍 {} ({} orders)",
            self.config.chain.name(),
            order_hashes.len()
        );
        println!("   Contract: {}", self.config.contract_address);

        let call = resetOrdersCall {
            orderHashes: order_hashes,
        };
        let calldata = call.abi_encode();

        let contract_address = Address::from_str(&self.config.contract_address)?;
        let tx = TransactionRequest::default()
            .to(contract_address)
            .input(calldata.into());

        if dry_run {
            println!("   ✅ Dry run - transaction prepared");
            return Ok(());
        }

        let provider = self.create_provider()?;

        println!("   📤 Sending transaction...");

        let pending_tx = provider.send_transaction(tx).await?;
        let tx_hash = pending_tx.tx_hash();

        println!("   Tx: {tx_hash}");

        let receipt = pending_tx.get_receipt().await?;

        if receipt.status() {
            println!("   ✅ Success (Gas: {})", receipt.gas_used);
        } else {
            println!("   ❌ Failed");
            return Err("Transaction reverted".into());
        }

        Ok(())
    }
}
