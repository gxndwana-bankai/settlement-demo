# Solana Settlement Program (Anchor)

This Anchor program verifies SP1 Groth16 proofs and decommits Merkle proofs of orders, mirroring the EVM `SettlementContract`.

## Build & Deploy (Devnet)

- Ensure Solana edge CLI and Anchor are installed.
- Build program:

```bash
anchor build
```

- Deploy (example):
```bash
solana config set -ud
solana program deploy target/deploy/settlement_solana_program.so
```

## Instructions
- initialize(vkey_hash)
- submit_order(order)
- settle_orders(sp1_public_inputs, groth16_proof, order_proofs)
- reset_orders(order_hashes)

The Merkle hashing uses Keccak-256 with sorted pair hashing to match OpenZeppelin.

Reference: `sp1-solana` verifier usage `verify_proof`.


