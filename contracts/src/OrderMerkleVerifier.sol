// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";

contract OrderMerkleVerifier {
    struct Order {
        uint64 source_chain_id;
        uint64 destination_chain_id;
        address receiver;
        uint256 amount;
        uint64 block_number;
    }

    /// @notice Verifies that an order is part of the Merkle tree
    /// @param order The order to verify
    /// @param proof The Merkle proof
    /// @param root The Merkle root
    /// @return True if the order is valid, false otherwise
    function verifyOrder(
        Order memory order,
        bytes32[] memory proof,
        bytes32 root
    ) public pure returns (bool) {
        bytes32 leaf = hashOrder(order);
        return MerkleProof.verify(proof, root, leaf);
    }

    /// @notice Hashes an order using keccak256
    /// @param order The order to hash
    /// @return The hash of the order
    function hashOrder(Order memory order) public pure returns (bytes32) {
        return keccak256(abi.encode(order));
    }
}

