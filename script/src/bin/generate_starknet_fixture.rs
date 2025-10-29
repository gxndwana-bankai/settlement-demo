use garaga_rs::calldata::full_proof_with_hints::groth16::{
    get_groth16_calldata_felt, get_sp1_vk, Groth16Proof,
};
use garaga_rs::definitions::CurveID;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize)]
struct ProofData {
    proof: String,
    #[serde(rename = "publicValues")]
    public_values: String,
    vkey: String,
    #[serde(rename = "merkleRoot")]
    merkle_root: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Generating Starknet proof fixture...\n");

    let proof_json = fs::read_to_string("proof.json")?;
    let proof_data: ProofData = serde_json::from_str(&proof_json)?;

    println!("📋 Proof Information:");
    println!("  VKey: {}", proof_data.vkey);
    println!("  Merkle Root: {}", proof_data.merkle_root);
    println!();

    let sp1_groth16_vk = get_sp1_vk();

    let vkey_bytes: Vec<u8> = hex::decode(proof_data.vkey.trim_start_matches("0x"))?;
    let public_values_bytes: Vec<u8> =
        hex::decode(proof_data.public_values.trim_start_matches("0x"))?;
    let proof_bytes: Vec<u8> = hex::decode(proof_data.proof.trim_start_matches("0x"))?;

    println!("🔧 Preprocessing with Garaga...");
    let groth16_proof = Groth16Proof::from_sp1(vkey_bytes, public_values_bytes, proof_bytes);

    let calldata = get_groth16_calldata_felt(&groth16_proof, &sp1_groth16_vk, CurveID::BN254)?;

    println!("✅ Generated {} calldata elements", calldata.len());
    println!();

    let calldata_hex: Vec<String> = calldata
        .iter()
        .map(|felt| format!("0x{:064x}", felt.to_biguint()))
        .collect();

    #[derive(Serialize)]
    struct Fixture {
        vkey: String,
        merkle_root: String,
        proof_calldata: Vec<String>,
        proof_calldata_length: usize,
    }

    let fixture = Fixture {
        vkey: proof_data.vkey,
        merkle_root: proof_data.merkle_root,
        proof_calldata: calldata_hex,
        proof_calldata_length: calldata.len(),
    };

    let output_path = "../contracts/starknet/src/fixtures/proof_fixture.json";
    fs::write(output_path, serde_json::to_string_pretty(&fixture)?)?;

    println!("💾 Fixture saved to: {output_path}");
    println!();
    println!("🎯 You can now use this fixture in your Cairo tests!");

    Ok(())
}
