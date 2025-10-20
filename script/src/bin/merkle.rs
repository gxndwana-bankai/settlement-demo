use alloy_primitives::hex::FromHex;
use alloy_primitives::{Address, U256};
use settlement_lib::{generate_all_proofs, Order};
use std::fs::File;
use std::io::Write;

fn main() {
    // Generate proofs for different order counts
    generate_and_export("merkle_proofs_2.json", &example_orders_2());
    generate_and_export("merkle_proofs_3.json", &example_orders_3());
    generate_and_export("merkle_proofs_5.json", &example_orders_5());

    // Keep the default 2-order file as well
    generate_and_export("merkle_proofs.json", &example_orders_2());

    println!("\nAll Merkle proof files generated successfully!");
}

fn generate_and_export(filename: &str, orders: &[Order]) {
    let merkle_data = generate_all_proofs(orders);

    println!("\n{} - Merkle Root: {}", filename, merkle_data.root);
    println!("Number of orders: {}", merkle_data.proofs.len());

    for (i, proof_data) in merkle_data.proofs.iter().enumerate() {
        println!("  Order {}: Proof length: {}", i, proof_data.proof.len());
    }

    let json = serde_json::to_string_pretty(&merkle_data).expect("Failed to serialize to JSON");

    let mut file = File::create(filename).expect("Failed to create file");
    file.write_all(json.as_bytes())
        .expect("Failed to write to file");
}

fn example_orders_2() -> Vec<Order> {
    vec![
        Order {
            source_chain_id: 2,
            destination_chain_id: 1,
            receiver: Address::from_hex("0x797b212C0a4cB61DEC7dC491B632b72D854e03fd").unwrap(),
            amount: U256::from(273418440000000000u64),
            block_number: 9451455,
        },
        Order {
            source_chain_id: 1,
            destination_chain_id: 2,
            receiver: Address::from_hex("0x1234567890123456789012345678901234567890").unwrap(),
            amount: U256::from(100000000000000000u64),
            block_number: 1000000,
        },
    ]
}

fn example_orders_3() -> Vec<Order> {
    vec![
        Order {
            source_chain_id: 2,
            destination_chain_id: 1,
            receiver: Address::from_hex("0x797b212C0a4cB61DEC7dC491B632b72D854e03fd").unwrap(),
            amount: U256::from(273418440000000000u64),
            block_number: 9451455,
        },
        Order {
            source_chain_id: 1,
            destination_chain_id: 2,
            receiver: Address::from_hex("0x1234567890123456789012345678901234567890").unwrap(),
            amount: U256::from(100000000000000000u64),
            block_number: 1000000,
        },
        Order {
            source_chain_id: 3,
            destination_chain_id: 1,
            receiver: Address::from_hex("0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd").unwrap(),
            amount: U256::from(500000000000000000u64),
            block_number: 2000000,
        },
    ]
}

fn example_orders_5() -> Vec<Order> {
    vec![
        Order {
            source_chain_id: 2,
            destination_chain_id: 1,
            receiver: Address::from_hex("0x797b212C0a4cB61DEC7dC491B632b72D854e03fd").unwrap(),
            amount: U256::from(273418440000000000u64),
            block_number: 9451455,
        },
        Order {
            source_chain_id: 1,
            destination_chain_id: 2,
            receiver: Address::from_hex("0x1234567890123456789012345678901234567890").unwrap(),
            amount: U256::from(100000000000000000u64),
            block_number: 1000000,
        },
        Order {
            source_chain_id: 3,
            destination_chain_id: 1,
            receiver: Address::from_hex("0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd").unwrap(),
            amount: U256::from(500000000000000000u64),
            block_number: 2000000,
        },
        Order {
            source_chain_id: 1,
            destination_chain_id: 3,
            receiver: Address::from_hex("0x5555555555555555555555555555555555555555").unwrap(),
            amount: U256::from(250000000000000000u64),
            block_number: 3000000,
        },
        Order {
            source_chain_id: 2,
            destination_chain_id: 3,
            receiver: Address::from_hex("0x9999999999999999999999999999999999999999").unwrap(),
            amount: U256::from(750000000000000000u64),
            block_number: 4000000,
        },
    ]
}
