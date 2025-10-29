# Starknet Settlement Contract

A zkVM-powered settlement contract for cross-chain order execution on Starknet.

## Features

- **Submit Orders**: Users submit cross-chain transfer orders
- **Batch Settlement**: Settle multiple orders with a single SP1 zero-knowledge proof
- **Merkle Verification**: Each order is verified against a merkle root proven by SP1
- **SP1 Integration**: Uses Garaga's SP1 verifier for proof validation

## Quick Start

### Build

```bash
scarb build
```

### Test

```bash
scarb test
# or
snforge test
```

### Deploy to Testnet

```bash
./deploy.sh sepolia <your-account-name>
```

For detailed deployment instructions, see [DEPLOYMENT.md](./DEPLOYMENT.md)

## Contract Interface

### Write Functions

- `submit_order(order: Order)` - Submit a new cross-chain order
- `settle_orders(proof: Array<felt252>, order_proofs: Span<OrderProof>)` - Settle orders with SP1 proof
- `reset_orders(order_hashes: Span<u256>)` - Reset order status (for testing)

### Read Functions

- `hash_order(order: Order) -> u256` - Compute the hash of an order
- `get_order_status(order_hash: u256) -> bool` - Check if an order is settled
- `get_vk() -> u256` - Get the verification key
- `verify_merkle_proof_public(...)` - Verify a merkle proof (testing utility)

## Order Structure

```cairo
struct Order {
    source_chain_id: u64,        // Chain ID where order originated
    destination_chain_id: u64,   // Target chain for settlement
    receiver: u256,              // EVM-style receiver address
    amount: u256,                // Amount to transfer
    block_number: u64,           // Block number of order
}
```

## Architecture

1. **Order Submission**: Users submit orders on the source chain
2. **Proof Generation**: Off-chain, SP1 generates a proof of multiple orders
3. **Batch Settlement**: Contract verifies SP1 proof and settles all orders atomically
4. **Merkle Verification**: Each order is verified against the proven merkle root

## SP1 Verifier

This contract uses the Garaga SP1 verifier deployed on Starknet:
- **Class Hash**: `0x79b72f62c1c6aad55c0ee0ecc68132a32db268306a19c451c35191080b7b611`
- **Verifier Function**: `verify_sp1_groth16_proof_bn254`

## Development

### Project Structure

```
contracts/starknet/
├── src/
│   └── lib.cairo           # Main contract implementation
├── tests/
│   ├── test_settlement.cairo       # Unit tests
│   └── test_settlement_fork.cairo  # Fork tests
├── deploy.sh               # Deployment script
├── Scarb.toml             # Package configuration
├── snfoundry.toml         # Network configuration
└── DEPLOYMENT.md          # Deployment guide
```

### Testing

Run unit tests:
```bash
snforge test
```

Run fork tests (requires RPC access):
```bash
snforge test --fork-url https://starknet-sepolia.public.blastapi.io/rpc/v0_7
```

## Resources

- [Starknet Documentation](https://docs.starknet.io/)
- [Cairo Book](https://book.cairo-lang.org/)
- [Starknet Foundry](https://foundry-rs.github.io/starknet-foundry/)
- [SP1 Documentation](https://docs.succinct.xyz/)
- [Garaga Verifier](https://github.com/keep-starknet-strange/garaga)

