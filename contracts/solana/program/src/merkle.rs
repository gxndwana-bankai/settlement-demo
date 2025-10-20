use solana_program::keccak::hash;

fn hash_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (left, right) = if a <= b { (a, b) } else { (b, a) };
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(left);
    buf[32..].copy_from_slice(right);
    hash(&buf).to_bytes()
}

pub fn verify_merkle_proof_keccak(leaf: &[u8; 32], proof: &[[u8; 32]], root: &[u8; 32]) -> bool {
    let mut computed = *leaf;
    for p in proof.iter() {
        computed = hash_pair(&computed, p);
    }
    &computed == root
}


