use anchor_lang::prelude::*;

pub mod merkle;
pub mod state;

use merkle::verify_merkle_proof_keccak;
use state::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod settlement_solana_program {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, vkey_hash: [u8; 32]) -> Result<()> {
        let state = &mut ctx.accounts.state;
        state.vkey_hash = vkey_hash;
        state.bump = *ctx.bumps.get("state").unwrap();
        Ok(())
    }

    pub fn submit_order(ctx: Context<SubmitOrder>, order: Order) -> Result<()> {
        let state = &mut ctx.accounts.state;
        let order_hash = order_hash_keccak(&order);
        state.orders_pending.push(order_hash);
        Ok(())
    }

    pub fn settle_orders(
        ctx: Context<SettleOrders>,
        sp1_public_inputs: Vec<u8>,
        groth16_proof: Vec<u8>,
        order_proofs: Vec<OrderProof>,
    ) -> Result<()> {
        let state = &mut ctx.accounts.state;

        // Verify SP1 Groth16 proof using on-chain precompiles
        let vk = sp1_solana::GROTH16_VK_2_0_0_BYTES;
        sp1_solana::verify_proof(
            &groth16_proof,
            &sp1_public_inputs,
            &hex_string(state.vkey_hash),
            vk,
        )
        .map_err(|_| error!(SettlementError::InvalidProof))?;

        // Extract merkle root from bytes 8..40
        require!(
            sp1_public_inputs.len() >= 40,
            SettlementError::InvalidPublicInputs
        );
        let merkle_root: [u8; 32] = sp1_public_inputs[8..40]
            .try_into()
            .map_err(|_| error!(SettlementError::InvalidPublicInputs))?;

        for op in order_proofs.iter() {
            // Recompute order hash from full order
            let h = order_hash_keccak(&op.order);
            require!(h == op.order_hash, SettlementError::InvalidOrderHash);

            let ok = verify_merkle_proof_keccak(&h, &op.proof, &merkle_root);
            require!(ok, SettlementError::InvalidMerkleProof);

            state.orders_settled.push(h);
        }

        Ok(())
    }

    pub fn reset_orders(ctx: Context<ResetOrders>, order_hashes: Vec<[u8; 32]>) -> Result<()> {
        let state = &mut ctx.accounts.state;
        for h in order_hashes {
            if let Some(pos) = state.orders_settled.iter().position(|x| x == &h) {
                state.orders_settled.swap_remove(pos);
            }
        }
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct Order {
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    pub receiver: [u8; 20],
    pub amount: [u8; 32],
    pub block_number: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct OrderProof {
    pub order: Order,
    pub order_hash: [u8; 32],
    pub proof: Vec<[u8; 32]>,
}

fn order_hash_keccak(order: &Order) -> [u8; 32] {
    // keccak256(abi.encode(order)) with Solidity static encoding (32-byte words)
    use solana_program::keccak::hash;
    let mut encoded = Vec::with_capacity(32 * 5);

    // uint64 -> left-padded 32 bytes
    let mut word = [0u8; 32];
    word[24..].copy_from_slice(&order.source_chain_id.to_be_bytes());
    encoded.extend_from_slice(&word);

    word = [0u8; 32];
    word[24..].copy_from_slice(&order.destination_chain_id.to_be_bytes());
    encoded.extend_from_slice(&word);

    // address -> left-padded 32 bytes (12 zero bytes + 20 address bytes)
    word = [0u8; 32];
    word[12..32].copy_from_slice(&order.receiver);
    encoded.extend_from_slice(&word);

    // uint256 -> 32 bytes big-endian as provided
    encoded.extend_from_slice(&order.amount);

    // uint64 block_number -> left-padded 32 bytes
    word = [0u8; 32];
    word[24..].copy_from_slice(&order.block_number.to_be_bytes());
    encoded.extend_from_slice(&word);

    hash(&encoded).to_bytes()
}

fn hex_string(bytes32: [u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes32 {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

#[error_code]
pub enum SettlementError {
    #[msg("Invalid Groth16 proof")]
    InvalidProof,
    #[msg("Invalid public inputs layout")]
    InvalidPublicInputs,
    #[msg("Invalid Merkle proof")]
    InvalidMerkleProof,
    #[msg("Order hash mismatch")]
    InvalidOrderHash,
}

#[derive(Accounts)]
#[instruction()]
pub struct Initialize<'info> {
    #[account(init, payer = payer, space = SettlementState::SPACE, seeds = [b"state"], bump)]
    pub state: Account<'info, SettlementState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitOrder<'info> {
    #[account(mut, seeds = [b"state"], bump = state.bump)]
    pub state: Account<'info, SettlementState>,
}

#[derive(Accounts)]
pub struct SettleOrders<'info> {
    #[account(mut, seeds = [b"state"], bump = state.bump)]
    pub state: Account<'info, SettlementState>,
}

#[derive(Accounts)]
pub struct ResetOrders<'info> {
    #[account(mut, seeds = [b"state"], bump = state.bump)]
    pub state: Account<'info, SettlementState>,
}
