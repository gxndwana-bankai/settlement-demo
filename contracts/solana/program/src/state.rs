use anchor_lang::prelude::*;

#[account]
pub struct SettlementState {
    pub vkey_hash: [u8; 32],
    pub orders_pending: Vec<[u8; 32]>,
    pub orders_settled: Vec<[u8; 32]>,
    pub bump: u8,
}

impl SettlementState {
    pub const SPACE: usize = 8  // discriminator
        + 32 // vkey_hash
        + 4 + 32 * 1024 // orders_pending vec cap placeholder
        + 4 + 32 * 1024 // orders_settled vec cap placeholder
        + 1; // bump
}
