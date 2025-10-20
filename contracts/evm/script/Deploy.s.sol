// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/OrderMerkleVerifier.sol";

contract DeploySettlement is Script {
    function run() external {
        // Read environment variables
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        bytes32 vkey = vm.envBytes32("VKEY");
        address sp1Verifier = vm.envAddress("SP1_VERIFIER");
        
        vm.startBroadcast(deployerPrivateKey);
        
        // Deploy with empty order roots array
        bytes32[] memory roots = new bytes32[](0);
        SettlementContract settlement = new SettlementContract(roots, vkey, sp1Verifier);
        
        console.log("SettlementContract deployed to:", address(settlement));
        console.log("VKey:", uint256(vkey));
        console.log("SP1 Verifier:", sp1Verifier);
        
        vm.stopBroadcast();
    }
}

