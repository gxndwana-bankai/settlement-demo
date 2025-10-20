// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";


contract SettlementContract {
    struct Order {
        uint64 sourceChainId;
        uint64 destinationChainId;
        address receiver;
        uint256 amount;
        uint64 blockNumber;
    }

    struct OrderProof {
        bytes32 orderHash;
        bytes32[] proof;
        uint256 leafIndex;
    }

    event OrderSettled(bytes32 orderHash);
    event NewOrder(Order order);

    mapping(bytes32 => bool) public orderMapping;
    bytes32 public vk;
    address public verifier;

    constructor(bytes32[] memory roots, bytes32 _vk, address _verifier) {
        for (uint256 i = 0; i < roots.length; i++) {
            orderMapping[roots[i]] = false;
        }
        vk = _vk;
        verifier = _verifier;
    }

    function submitOrder(
        Order memory order
    ) public {
        require(order.sourceChainId == block.chainid, "Wrong chain id set");
        bytes32 orderHash = hashOrder(order);
        require(!orderMapping[orderHash], "Order already exists");
        orderMapping[orderHash] = false;
        emit NewOrder(order);
    }

    function settleOrders(
        bytes calldata publicValues,
        bytes calldata proofBytes,
        OrderProof[] memory orderProofs
    ) public {

        // verify the zk proof
        ISP1Verifier(verifier).verifyProof(vk, publicValues, proofBytes);
        
        // Extract merkle root from bytes 8..40 of publicValues
        bytes32 merkleRoot;
        assembly {
            merkleRoot := calldataload(add(publicValues.offset, 8))
        }
        
        for (uint256 i = 0; i < orderProofs.length; i++) {
            OrderProof memory orderProof = orderProofs[i];
            bool valid = MerkleProof.verify(orderProof.proof, merkleRoot, orderProof.orderHash);
            require(valid, "Invalid merkle proof");
            orderMapping[orderProof.orderHash] = true;
            emit OrderSettled(orderProof.orderHash);
        }
    }

    function resetOrders(bytes32[] memory orderHashes) public {
        for (uint256 i = 0; i < orderHashes.length; i++) {
            orderMapping[orderHashes[i]] = false;
        }
    }

    /// @notice Hashes an order using keccak256
    /// @param order The order to hash
    /// @return The hash of the order
    function hashOrder(Order memory order) public pure returns (bytes32) {
        return keccak256(abi.encode(order));
    }
}

