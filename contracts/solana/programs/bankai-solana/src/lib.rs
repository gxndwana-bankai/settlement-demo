use anchor_lang::prelude::*;

pub mod merkle;
pub mod state;

use merkle::verify_merkle_proof_keccak;
use state::*;

declare_id!("9GrAFZAUtkj69pJRh1dzHnKxF8aZaEVFcYAN5RjMmb2f");
const BANKAI_VKEY_HASH: &str = "0x001ef62344ca35708c7af9dc5cda28683244720d303d480b47d82136ede2b8ed";
#[program]
pub mod bankai_solana {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, vkey_hash: [u8; 32]) -> Result<()> {
        let state = &mut ctx.accounts.state;
        state.vkey_hash = vkey_hash;
        state.bump = ctx.bumps.state;
        Ok(())
    }

    pub fn submit_order(
        ctx: Context<SubmitOrder>,
        order: Order,
        order_hash: [u8; 32],
    ) -> Result<()> {
        // Sanity check: recompute hash and ensure it matches provided order_hash
        let computed = order_hash_keccak(&order);
        require!(computed == order_hash, SettlementError::InvalidOrderHash);
        let order_status = &mut ctx.accounts.order_status;
        order_status.order_hash = computed;
        order_status.settled = false;
        order_status.bump = ctx.bumps.order_status;
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
        let vk = sp1_solana::GROTH16_VK_5_0_0_BYTES;
        sp1_solana::verify_proof(&groth16_proof, &sp1_public_inputs, BANKAI_VKEY_HASH, vk)
            .map_err(|_| error!(SettlementError::InvalidProof))?;

        // Extract merkle root from bytes 8..40
        require!(
            sp1_public_inputs.len() >= 40,
            SettlementError::InvalidPublicInputs
        );
        let merkle_root: [u8; 32] = sp1_public_inputs[8..40]
            .try_into()
            .map_err(|_| error!(SettlementError::InvalidPublicInputs))?;

        for (i, op) in order_proofs.iter().enumerate() {
            // Recompute order hash from full order
            let h = order_hash_keccak(&op.order);
            require!(h == op.order_hash, SettlementError::InvalidOrderHash);

            let ok = verify_merkle_proof_keccak(&h, &op.proof, &merkle_root);
            require!(ok, SettlementError::InvalidMerkleProof);

            // Use remaining accounts to access the PDA for this order
            let acct_info = ctx
                .remaining_accounts
                .get(i)
                .ok_or(error!(SettlementError::InvalidPublicInputs))?;

            // Ensure PDA address matches seeds
            let (expected_pda, _bump) =
                Pubkey::find_program_address(&[b"order", &h], ctx.program_id);
            require_keys_eq!(expected_pda, *acct_info.key);

            // Deserialize, update, and serialize back
            let mut data: state::OrderStatus =
                state::OrderStatus::try_deserialize(&mut &acct_info.data.borrow()[..])?;
            require!(data.order_hash == h, SettlementError::InvalidOrderHash);
            data.settled = true;
            let mut data_buf = acct_info.data.borrow_mut();
            let mut cursor = std::io::Cursor::new(&mut data_buf[..]);
            data.try_serialize(&mut cursor)?;

            emit!(OrderSettled { order_hash: h });
        }

        Ok(())
    }

    pub fn reset_orders(ctx: Context<ResetOrders>, order_hashes: Vec<[u8; 32]>) -> Result<()> {
        for (i, h) in order_hashes.iter().enumerate() {
            let acct_info = ctx
                .remaining_accounts
                .get(i)
                .ok_or(error!(SettlementError::InvalidPublicInputs))?;

            let (expected_pda, _bump) =
                Pubkey::find_program_address(&[b"order", h.as_ref()], ctx.program_id);
            require_keys_eq!(expected_pda, *acct_info.key);

            let mut data: state::OrderStatus =
                state::OrderStatus::try_deserialize(&mut &acct_info.data.borrow()[..])?;
            require!(data.order_hash == *h, SettlementError::InvalidOrderHash);
            data.settled = false;
            let mut data_buf = acct_info.data.borrow_mut();
            let mut cursor = std::io::Cursor::new(&mut data_buf[..]);
            data.try_serialize(&mut cursor)?;
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
    use solana_program::keccak::hashv;

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

#[event]
pub struct OrderSettled {
    pub order_hash: [u8; 32],
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
#[instruction(order: Order, order_hash: [u8; 32])]
pub struct SubmitOrder<'info> {
    #[account(mut, seeds = [b"state"], bump = state.bump)]
    pub state: Account<'info, SettlementState>,
    #[account(
        init,
        payer = payer,
        space = OrderStatus::SPACE,
        seeds = [b"order", order_hash.as_ref()],
        bump
    )]
    pub order_status: Account<'info, OrderStatus>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
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
