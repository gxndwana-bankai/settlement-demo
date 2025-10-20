use solana_program::keccak::hashv;

fn hash_pair(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (left, right) = if a <= b { (a, b) } else { (b, a) };
    hashv(&[left, right]).to_bytes()
}

pub fn verify_merkle_proof_keccak(leaf: &[u8; 32], proof: &[[u8; 32]], root: &[u8; 32]) -> bool {
    let mut computed = *leaf;
    for p in proof.iter() {
        computed = hash_pair(&computed, p);
    }
    &computed == root
}
