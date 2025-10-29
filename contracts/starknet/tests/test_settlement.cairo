use starknet::ContractAddress;
use snforge_std::{
    declare, ContractClassTrait, DeclareResultTrait, start_cheat_chain_id_global,
    stop_cheat_chain_id_global, spy_events, EventSpyAssertionsTrait
};
use settlement_starknet::{
    ISettlementContractDispatcher, ISettlementContractDispatcherTrait, Order
};
// no direct keccak usage in tests; contract verifies

fn setup() -> (ISettlementContractDispatcher, ContractAddress) {
    let contract = declare("SettlementContract").unwrap().contract_class();
    
    // Mock VK (verification key)
    let vk: u256 = 0x123456789abcdef;
    
    // Initialize with empty roots array
    let roots: Array<u256> = array![];
    
    let mut constructor_calldata = array![];
    Serde::serialize(@roots.span(), ref constructor_calldata);
    Serde::serialize(@vk, ref constructor_calldata);
    
    let (contract_address, _) = contract.deploy(@constructor_calldata).unwrap();
    
    (
        ISettlementContractDispatcher { contract_address },
        contract_address
    )
}

#[test]
fn test_submit_order() {
    let (settlement, _) = setup();
    
    // Set chain ID to match order
    start_cheat_chain_id_global(1);
    
    let order = Order {
        source_chain_id: 1,
        destination_chain_id: 2,
        receiver: 0x123_u256,
        amount: 1000_u256,
        block_number: 100,
    };
    
    settlement.submit_order(order);
    
    let order_hash = settlement.hash_order(order);
    let status = settlement.get_order_status(order_hash);
    
    assert!(status == false, "Order should not be settled yet");
    
    stop_cheat_chain_id_global();
}

#[test]
#[should_panic(expected: ('Wrong chain id set',))]
fn test_submit_order_wrong_chain() {
    let (settlement, _) = setup();
    
    // Set chain ID to different value
    start_cheat_chain_id_global(1);
    
    let order = Order {
        source_chain_id: 2, // Different from actual chain ID
        destination_chain_id: 3,
        receiver: 0x123_u256,
        amount: 1000_u256,
        block_number: 100,
    };
    
    settlement.submit_order(order);
    
    stop_cheat_chain_id_global();
}

#[test]
fn test_reset_orders() {
    let (settlement, _) = setup();
    
    start_cheat_chain_id_global(1);
    
    let order = Order {
        source_chain_id: 1,
        destination_chain_id: 2,
        receiver: 0x123_u256,
        amount: 1000_u256,
        block_number: 100,
    };
    
    settlement.submit_order(order);
    let order_hash = settlement.hash_order(order);
    
    // Reset the order
    let order_hashes = array![order_hash];
    settlement.reset_orders(order_hashes.span());
    
    let status = settlement.get_order_status(order_hash);
    assert!(status == false, "Order should be reset");
    
    stop_cheat_chain_id_global();
}

#[test]
fn test_get_vk() {
    let (settlement, _) = setup();
    
    let vk = settlement.get_vk();
    assert!(vk == 0x123456789abcdef, "VK should match constructor value");
}

#[test]
fn test_event_emission_on_submit() {
    let (settlement, contract_address) = setup();
    
    let mut spy = spy_events();
    
    start_cheat_chain_id_global(1);
    
    let order = Order {
        source_chain_id: 1,
        destination_chain_id: 2,
        receiver: 0x123_u256,
        amount: 1000_u256,
        block_number: 100,
    };
    
    settlement.submit_order(order);
    
    spy.assert_emitted(@array![
        (
            contract_address,
            settlement_starknet::SettlementContract::Event::NewOrder(
                settlement_starknet::SettlementContract::NewOrder { order }
            )
        )
    ]);
    
    stop_cheat_chain_id_global();
}

#[test]
fn test_order_hash_matches_evm() {
    let (settlement, _) = setup();
    
    // Example 1 from proof.json (Base Sepolia)
    let order1 = Order {
        source_chain_id: 84532,
        destination_chain_id: 11155111,
        receiver: 0x3A1D60A48B1104a31133dFBC70E8a589ce8dE57a_u256,
        amount: 2500000000000000000_u256,
        block_number: 9452994,
    };
    let h1 = settlement.hash_order(order1);
    assert!(h1 == 0xd811c398160b6170623458b1e72c0405857dc075ec28135c78546aad5c8f148b, "hash mismatch: h1");

    // Example 2 from proof.json (Base Sepolia)
    let order2 = Order {
        source_chain_id: 84532,
        destination_chain_id: 11155111,
        receiver: 0xDc720ddDF0dDAecF594804618507a62D86D96F9c_u256,
        amount: 2500000000000000000_u256,
        block_number: 9452994,
    };
    let h2 = settlement.hash_order(order2);
    assert!(h2 == 0xf75ef566d2e46ecbda6bbf708d98f359267fc918d3c3d12680205b4b7a67f3d5, "hash mismatch: h2");
}

#[test]
fn test_merkle_proof_verification() {
    let (settlement, _) = setup();
    
    // Test data from proof.json - Real order from Base Sepolia
    // This demonstrates the Merkle proof verification logic that happens in settle_orders
    // Merkle root: 0xad3e6524f92d8b20ba27d71c814ffcf884c324d8408103675935a80e017a72d2
    let expected_merkle_root: u256 = 0xad3e6524f92d8b20ba27d71c814ffcf884c324d8408103675935a80e017a72d2;
    
    // Order hash from Base Sepolia (leaf_index: 1)
    let order_hash: u256 = 0xd811c398160b6170623458b1e72c0405857dc075ec28135c78546aad5c8f148b;
    
    // Merkle proof path
    let proof: Array<u256> = array![
        0x9c2e5b288b96a039442ce42f7e6e0fec3ec9112737e193ef505c232fa8adfb03,
        0xca9dc94a7cbd2a273977993402c24b8b1e16165d2cc377136101181aff6eb9d9,
        0x7bb1aa457ddc25dc91a6ff38f5143e85b1c3c74ba1f601de6d3d5a526585cdf7,
        0xd68f8f5055106843f48085c6f0917b31917c6098a08a5dd8b717fd97fb33787b,
        0x1f05916d677d35a1c7f703aaeaeaefc755285cff5db35b73a752831b0e469389
    ];
    
    let leaf_index: u256 = 1;
    
    // Call the contract's public merkle verification helper
    let ok = settlement.verify_merkle_proof_public(proof.span(), expected_merkle_root, order_hash, leaf_index);
    assert!(ok, "Merkle proof verification failed in contract logic");
}

#[test]
fn test_multiple_orders_same_chain() {
    let (settlement, _) = setup();
    
    start_cheat_chain_id_global(84532);
    
    // Submit multiple orders from Base Sepolia
    let order1 = Order {
        source_chain_id: 84532,
        destination_chain_id: 11155111,
        receiver: 0x9eCD8efB5b592786b19cC776D58cD651B553e269,
        amount: 2500000000000000000_u256,
        block_number: 9452270,
    };
    
    let order2 = Order {
        source_chain_id: 84532,
        destination_chain_id: 11155111,
        receiver: 0xed5C648955a4157cbc66b74B0726BC761CfeeD2b_u256,
        amount: 1715935090000000000_u256,
        block_number: 9453085,
    };
    
    settlement.submit_order(order1);
    settlement.submit_order(order2);
    
    // Verify both orders have correct hashes
    let hash1 = settlement.hash_order(order1);
    let hash2 = settlement.hash_order(order2);
    
    let expected_hash1: u256 = 0x40429c3805cf9374550099262a01bf408ca7475edd2f38e1a2a7bf7b7cb7725c;
    let expected_hash2: u256 = 0xc7e5b3e5385bd9b9b6e0807ce2550fb9e81649caee39021f6d21a7c12e2dacd4;
    
    assert!(hash1 == expected_hash1, "Order 1 hash must match");
    assert!(hash2 == expected_hash2, "Order 2 hash must match");
    
    assert!(hash1 != hash2, "Different orders must have different hashes");
    
    stop_cheat_chain_id_global();
}

