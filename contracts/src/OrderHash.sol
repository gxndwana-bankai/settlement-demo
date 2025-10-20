// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract OrderHash {
    struct Order {
        uint64 source_chain_id;
        uint64 destination_chain_id;
        address receiver;
        uint256 amount;
        uint64 block_number;
    }

    /// @notice Computes the Keccak256 hash of an Order struct
    /// @dev This matches the Rust implementation using abi.encode
    /// @param order The Order struct to hash
    /// @return The Keccak256 hash of the encoded order
    function hashOrder(Order memory order) public pure returns (bytes32) {
        return keccak256(abi.encode(order));
    }
}

