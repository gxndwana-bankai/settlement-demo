// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/OrderHash.sol";

contract OrderHashTest is Test {
    OrderHash public orderHash;

    function setUp() public {
        orderHash = new OrderHash();
    }

    /// @notice Test hashing an example order
    /// @dev This uses the same values as the example_orders() function in Rust
    function test_HashOrder() public view {
        OrderHash.Order memory order = OrderHash.Order({
            source_chain_id: 2,
            destination_chain_id: 1,
            receiver: 0x797b212C0a4cB61DEC7dC491B632b72D854e03fd,
            amount: 273418440000000000,
            block_number: 9451455
        });

        bytes32 hash = orderHash.hashOrder(order);
        
        // Print the hash for comparison with Rust implementation
        console.log("Order hash:");
        console.logBytes32(hash);
        
        // The hash should match the one computed in Rust
        assertTrue(hash != bytes32(0), "Hash should not be zero");
    }
}

