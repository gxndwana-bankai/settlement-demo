//! A simple program that takes a number `n` as input, and writes the `n-1`th and `n`th fibonacci
//! number as an output.

// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_consensus::Transaction;
use bankai_types::ProofWrapper;
use bankai_verify::verify_batch_proof;
use settlement_lib::{generate_merkle_root, Order};

pub fn main() {
    // Read an input to the program.
    println!("Entering zkVM...");
    let proof_batch = sp1_zkvm::io::read::<ProofWrapper>();
    let orders = sp1_zkvm::io::read::<Vec<Order>>();
    println!("Retrieved Inputs...");

    // verify the proof, containing all the claimed executions
    let res = verify_batch_proof(proof_batch).unwrap();

    // iterate throught the orders, asserting they match the veried txs
    for (index, order) in orders.iter().enumerate() {
        println!("Verifying Order: {index:?}");
        let tx = &res.evm.tx[index];

        assert_eq!(tx.to(), Some(order.receiver));
        assert_eq!(tx.value(), order.amount);
        assert_eq!(tx.chain_id(), Some(order.destination_chain_id));
    }
    println!("All orders ok! Merkelizing...");

    let root = generate_merkle_root(orders.as_slice());
    println!("Verification Root: {root:?}");

    sp1_zkvm::io::commit_slice(root.as_slice());
}
