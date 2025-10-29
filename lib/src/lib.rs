use alloy_primitives::{keccak256, FixedBytes};
use alloy_sol_types::{sol, SolType};
use serde::{Deserialize, Serialize};

sol! {
    #[derive(Debug, Serialize, Deserialize)]
    struct Order {
        uint64 source_chain_id;
        uint64 destination_chain_id;
        address receiver;
        uint256 amount;
        uint64 block_number;
    }
}

impl Order {
    /// Computes the Keccak256 hash of the Order struct.
    /// This matches Solidity's `keccak256(abi.encode(order))`.
    pub fn hash(&self) -> FixedBytes<32> {
        let encoded = Order::abi_encode(self);
        keccak256(&encoded)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedExecution {
    pub chain_id: u64,
    pub tx_hash: FixedBytes<32>,
}

/// Hash a pair of nodes, matching OpenZeppelin's commutativeKeccak256 behavior
fn hash_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut combined = Vec::with_capacity(64);
    if a <= b {
        combined.extend_from_slice(a);
        combined.extend_from_slice(b);
    } else {
        combined.extend_from_slice(b);
        combined.extend_from_slice(a);
    }
    keccak256(&combined).0
}

/// Proof data for a single order in the Merkle tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderProof {
    pub order: Order,
    pub proof: Vec<FixedBytes<32>>,
    pub leaf_index: usize,
}

/// Complete Merkle tree data including root and all proofs
#[derive(Debug, Serialize, Deserialize)]
pub struct MerkleTreeData {
    pub root: FixedBytes<32>,
    pub proofs: Vec<OrderProof>,
}

/// Generates a Merkle root from an array of orders matching OpenZeppelin's implementation
pub fn generate_merkle_root(orders: &[Order]) -> FixedBytes<32> {
    if orders.is_empty() {
        return FixedBytes::ZERO;
    }

    let mut hashes: Vec<[u8; 32]> = orders.iter().map(|order| order.hash().0).collect();

    while hashes.len() > 1 {
        let mut next_level = Vec::new();

        for i in (0..hashes.len()).step_by(2) {
            if i + 1 < hashes.len() {
                next_level.push(hash_pair(&hashes[i], &hashes[i + 1]));
            } else {
                next_level.push(hash_pair(&hashes[i], &hashes[i]));
            }
        }

        hashes = next_level;
    }

    FixedBytes::from_slice(&hashes[0])
}

/// Generates Merkle proofs for all orders matching OpenZeppelin's implementation
pub fn generate_all_proofs(orders: &[Order]) -> MerkleTreeData {
    if orders.is_empty() {
        return MerkleTreeData {
            root: FixedBytes::ZERO,
            proofs: vec![],
        };
    }

    let leaves: Vec<[u8; 32]> = orders.iter().map(|order| order.hash().0).collect();

    // Build the tree and collect proofs
    let root = build_tree_and_get_root(&leaves);
    let proofs: Vec<OrderProof> = orders
        .iter()
        .enumerate()
        .map(|(index, order)| {
            let proof_hashes = generate_proof(&leaves, index);
            OrderProof {
                order: order.clone(),
                proof: proof_hashes
                    .iter()
                    .map(|hash| FixedBytes::<32>::from_slice(hash))
                    .collect(),
                leaf_index: index,
            }
        })
        .collect();

    MerkleTreeData {
        root: root.into(),
        proofs,
    }
}

/// Build the Merkle tree and return the root
fn build_tree_and_get_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }

    let mut hashes = leaves.to_vec();

    while hashes.len() > 1 {
        let mut next_level = Vec::new();
        for i in (0..hashes.len()).step_by(2) {
            if i + 1 < hashes.len() {
                next_level.push(hash_pair(&hashes[i], &hashes[i + 1]));
            } else {
                next_level.push(hash_pair(&hashes[i], &hashes[i]));
            }
        }
        hashes = next_level;
    }

    hashes[0]
}

/// Generate a Merkle proof for a leaf at the given index
fn generate_proof(leaves: &[[u8; 32]], index: usize) -> Vec<[u8; 32]> {
    if leaves.is_empty() || index >= leaves.len() {
        return vec![];
    }

    let mut proof = Vec::new();
    let mut hashes = leaves.to_vec();
    let mut current_index = index;

    while hashes.len() > 1 {
        let mut next_level = Vec::new();

        for i in (0..hashes.len()).step_by(2) {
            if i + 1 < hashes.len() {
                // If this pair contains our current index, add the sibling to the proof
                if i == current_index || i + 1 == current_index {
                    let sibling = if i == current_index {
                        hashes[i + 1]
                    } else {
                        hashes[i]
                    };
                    proof.push(sibling);
                }
                next_level.push(hash_pair(&hashes[i], &hashes[i + 1]));
            } else {
                // Odd number of nodes, duplicate the last one
                if i == current_index {
                    proof.push(hashes[i]);
                }
                next_level.push(hash_pair(&hashes[i], &hashes[i]));
            }
        }

        current_index /= 2;
        hashes = next_level;
    }

    proof
}
