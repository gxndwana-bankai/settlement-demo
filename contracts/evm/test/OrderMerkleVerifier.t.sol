// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/OrderMerkleVerifier.sol";
import "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";

contract OrderMerkleVerifierTest is Test {
    SettlementContract public verifier;

    function setUp() public {
        bytes32[] memory roots = new bytes32[](0);
        bytes32 vk = bytes32(0);
        address mockVerifier = address(0);
        verifier = new SettlementContract(roots, vk, mockVerifier);
    }
    
    function verifyOrder(
        SettlementContract.Order memory order,
        bytes32[] memory proof,
        bytes32 root
    ) internal view returns (bool) {
        bytes32 leaf = verifier.hashOrder(order);
        return MerkleProof.verify(proof, root, leaf);
    }

    /// @notice Test using Rust-generated JSON data
    function test_VerifyRustGeneratedProofs() public {
        string memory json = vm.readFile("./src/fixtures/merkle_proofs.json");
        bytes32 root = vm.parseJsonBytes32(json, ".root");
        
        // Verify first order
        {
            SettlementContract.Order memory order = SettlementContract.Order({
                sourceChainId: uint64(vm.parseJsonUint(json, ".proofs[0].order.source_chain_id")),
                destinationChainId: uint64(vm.parseJsonUint(json, ".proofs[0].order.destination_chain_id")),
                receiver: vm.parseJsonAddress(json, ".proofs[0].order.receiver"),
                amount: vm.parseJsonUint(json, ".proofs[0].order.amount"),
                blockNumber: uint64(vm.parseJsonUint(json, ".proofs[0].order.block_number"))
            });
            
            bytes32[] memory proof = new bytes32[](1);
            proof[0] = vm.parseJsonBytes32(json, ".proofs[0].proof[0]");
            
            assertTrue(verifyOrder(order, proof, root), "Order 0 from JSON should be valid");
        }
        
        // Verify second order
        {
            SettlementContract.Order memory order = SettlementContract.Order({
                sourceChainId: uint64(vm.parseJsonUint(json, ".proofs[1].order.source_chain_id")),
                destinationChainId: uint64(vm.parseJsonUint(json, ".proofs[1].order.destination_chain_id")),
                receiver: vm.parseJsonAddress(json, ".proofs[1].order.receiver"),
                amount: vm.parseJsonUint(json, ".proofs[1].order.amount"),
                blockNumber: uint64(vm.parseJsonUint(json, ".proofs[1].order.block_number"))
            });
            
            bytes32[] memory proof = new bytes32[](1);
            proof[0] = vm.parseJsonBytes32(json, ".proofs[1].proof[0]");
            
            assertTrue(verifyOrder(order, proof, root), "Order 1 from JSON should be valid");
        }
        
        console.log("Successfully verified all Rust-generated proofs from JSON!");
        console.log("Root:");
        console.logBytes32(root);
    }
    
    /// @notice Test Merkle proof verification with manual calculation
    /// @dev This test verifies the Merkle tree construction matches expectations
    function test_VerifyMerkleProof() public view {
        // Create test orders
        SettlementContract.Order memory order1 = SettlementContract.Order({
            sourceChainId: 2,
            destinationChainId: 1,
            receiver: 0x797b212C0a4cB61DEC7dC491B632b72D854e03fd,
            amount: 273418440000000000,
            blockNumber: 9451455
        });

        SettlementContract.Order memory order2 = SettlementContract.Order({
            sourceChainId: 1,
            destinationChainId: 2,
            receiver: 0x1234567890123456789012345678901234567890,
            amount: 100000000000000000,
            blockNumber: 1000000
        });

        // For a tree with 2 leaves, each leaf's proof contains the other leaf
        bytes32 leaf1 = verifier.hashOrder(order1);
        bytes32 leaf2 = verifier.hashOrder(order2);
        
        // Calculate root manually (smaller hash first in OpenZeppelin/rs-merkle)
        bytes32 root;
        if (uint256(leaf1) < uint256(leaf2)) {
            root = keccak256(abi.encodePacked(leaf1, leaf2));
        } else {
            root = keccak256(abi.encodePacked(leaf2, leaf1));
        }

        console.log("Computed root:");
        console.logBytes32(root);

        // Create proof for order1 (proof contains leaf2)
        bytes32[] memory proof1 = new bytes32[](1);
        proof1[0] = leaf2;

        // Verify order1
        bool valid1 = verifyOrder(order1, proof1, root);
        assertTrue(valid1, "Order 1 should be valid");

        // Create proof for order2 (proof contains leaf1)
        bytes32[] memory proof2 = new bytes32[](1);
        proof2[0] = leaf1;

        // Verify order2
        bool valid2 = verifyOrder(order2, proof2, root);
        assertTrue(valid2, "Order 2 should be valid");
    }

    /// @notice Test invalid proof rejection
    function test_RejectInvalidProof() public view {
        SettlementContract.Order memory order = SettlementContract.Order({
            sourceChainId: 2,
            destinationChainId: 1,
            receiver: 0x797b212C0a4cB61DEC7dC491B632b72D854e03fd,
            amount: 273418440000000000,
            blockNumber: 9451455
        });

        bytes32[] memory invalidProof = new bytes32[](1);
        invalidProof[0] = bytes32(uint256(123456));
        
        bytes32 fakeRoot = bytes32(uint256(789012));

        bool valid = verifyOrder(order, invalidProof, fakeRoot);
        assertFalse(valid, "Invalid proof should be rejected");
    }

    /// @notice Test with 3 orders (odd leaf count)
    function test_VerifyProofs_3Orders() public {
        string memory json = vm.readFile("./src/fixtures/merkle_proofs_3.json");
        bytes32 root = vm.parseJsonBytes32(json, ".root");
        
        console.log("Testing 3 orders - Root:");
        console.logBytes32(root);
        
        // Verify all 3 orders
        for (uint i = 0; i < 3; i++) {
            string memory basePath = string.concat(".proofs[", vm.toString(i), "]");
            
            SettlementContract.Order memory order = SettlementContract.Order({
                sourceChainId: uint64(vm.parseJsonUint(json, string.concat(basePath, ".order.source_chain_id"))),
                destinationChainId: uint64(vm.parseJsonUint(json, string.concat(basePath, ".order.destination_chain_id"))),
                receiver: vm.parseJsonAddress(json, string.concat(basePath, ".order.receiver")),
                amount: vm.parseJsonUint(json, string.concat(basePath, ".order.amount")),
                blockNumber: uint64(vm.parseJsonUint(json, string.concat(basePath, ".order.block_number")))
            });
            
            // Parse proof array
            bytes32[] memory proof = new bytes32[](2);
            proof[0] = vm.parseJsonBytes32(json, string.concat(basePath, ".proof[0]"));
            proof[1] = vm.parseJsonBytes32(json, string.concat(basePath, ".proof[1]"));
            
            bool valid = verifyOrder(order, proof, root);
            assertTrue(valid, string.concat("Order ", vm.toString(i), " should be valid"));
        }
        
        console.log("All 3 orders verified successfully!");
    }

    /// @notice Test with 5 orders (odd leaf count)
    function test_VerifyProofs_5Orders() public {
        string memory json = vm.readFile("./src/fixtures/merkle_proofs_5.json");
        bytes32 root = vm.parseJsonBytes32(json, ".root");
        
        console.log("Testing 5 orders - Root:");
        console.logBytes32(root);
        
        // Verify all 5 orders
        for (uint i = 0; i < 5; i++) {
            string memory basePath = string.concat(".proofs[", vm.toString(i), "]");
            
            SettlementContract.Order memory order = SettlementContract.Order({
                sourceChainId: uint64(vm.parseJsonUint(json, string.concat(basePath, ".order.source_chain_id"))),
                destinationChainId: uint64(vm.parseJsonUint(json, string.concat(basePath, ".order.destination_chain_id"))),
                receiver: vm.parseJsonAddress(json, string.concat(basePath, ".order.receiver")),
                amount: vm.parseJsonUint(json, string.concat(basePath, ".order.amount")),
                blockNumber: uint64(vm.parseJsonUint(json, string.concat(basePath, ".order.block_number")))
            });
            
            // Parse proof array (3 elements for a tree with 5 leaves)
            bytes32[] memory proof = new bytes32[](3);
            proof[0] = vm.parseJsonBytes32(json, string.concat(basePath, ".proof[0]"));
            proof[1] = vm.parseJsonBytes32(json, string.concat(basePath, ".proof[1]"));
            proof[2] = vm.parseJsonBytes32(json, string.concat(basePath, ".proof[2]"));
            
            bool valid = verifyOrder(order, proof, root);
            assertTrue(valid, string.concat("Order ", vm.toString(i), " should be valid"));
        }
        
        console.log("All 5 orders verified successfully!");
    }
}

