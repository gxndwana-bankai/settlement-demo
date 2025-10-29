use anchor_lang::prelude::*;

#[account]
pub struct SettlementState {
    pub vkey_hash: [u8; 32],
    pub bump: u8,
}

impl SettlementState {
    pub const SPACE: usize = 8  // discriminator
        + 32 // vkey_hash
        + 1; // bump
}

#[account]
pub struct OrderStatus {
    pub order_hash: [u8; 32],
    pub settled: bool,
    pub bump: u8,
}

impl OrderStatus {
    pub const SPACE: usize = 8  // discriminator
        + 32 // order_hash
        + 1  // settled
        + 1; // bump
}
