// EVM interop: no Starknet addresses in the Order; receiver is an EVM address in u256

#[starknet::interface]
pub trait ISettlementContract<TContractState> {
    fn submit_order(ref self: TContractState, order: Order);
    fn settle_orders(
        ref self: TContractState,
        proof: Array<felt252>,
        order_proofs: Span<OrderProof>
    );
    fn reset_orders(ref self: TContractState, order_hashes: Span<u256>);
    fn hash_order(self: @TContractState, order: Order) -> u256;
    fn get_order_status(self: @TContractState, order_hash: u256) -> bool;
    fn get_vk(self: @TContractState) -> u256;
    // Test/utility views
    fn verify_merkle_proof_public(
        self: @TContractState,
        proof: Span<u256>,
        root: u256,
        leaf: u256,
        index: u256
    ) -> bool;
    fn verify_sp1_proof_view(self: @TContractState, proof: Array<felt252>) -> Option<(u256, Span<u256>)>;
}

#[derive(Copy, Drop, Serde, starknet::Store)]
pub struct Order {
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    pub receiver: u256,
    pub amount: u256,
    pub block_number: u64,
}

#[derive(Copy, Drop, Serde)]
pub struct OrderProof {
    pub order_hash: u256,
    pub proof: Span<u256>,
    pub leaf_index: u256,
}

#[starknet::contract]
pub mod SettlementContract {
    use super::{Order, OrderProof};
    use starknet::{SyscallResultTrait, get_tx_info};
    use starknet::syscalls::library_call_syscall;
    use core::keccak::keccak_u256s_be_inputs;
    use core::integer;
    use starknet::storage::{
        Map, StoragePathEntry, StoragePointerReadAccess, StoragePointerWriteAccess
    };

    /// SP1 Verifier class hash deployed and maintained by the Garaga library.
    /// Class hash: 0x79b72f62c1c6aad55c0ee0ecc68132a32db268306a19c451c35191080b7b611
    const SP1_VERIFIER_CLASS_HASH: felt252 =
        0x79b72f62c1c6aad55c0ee0ecc68132a32db268306a19c451c35191080b7b611;

    #[storage]
    struct Storage {
        order_mapping: Map<u256, bool>,
        vk: u256,
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OrderSettled: OrderSettled,
        NewOrder: NewOrder,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OrderSettled {
        pub order_hash: u256,
    }

    #[derive(Drop, starknet::Event)]
    pub struct NewOrder {
        pub order: Order,
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        roots: Span<u256>,
        vk: u256
    ) {
        let mut i: u32 = 0;
        loop {
            if i >= roots.len() {
                break;
            }
            self.order_mapping.entry(*roots.at(i)).write(false);
            i += 1;
        };
        self.vk.write(vk);
    }

    #[abi(embed_v0)]
    impl SettlementContractImpl of super::ISettlementContract<ContractState> {
        fn submit_order(ref self: ContractState, order: Order) {
            let tx_info = get_tx_info().unbox();
            let current_chain_id: u64 = tx_info.chain_id.try_into().unwrap();
            
            assert(order.source_chain_id == current_chain_id, 'Wrong chain id set');
            
            let order_hash = self.hash_order(order);
            assert(!self.order_mapping.entry(order_hash).read(), 'Order already exists');
            
            self.order_mapping.entry(order_hash).write(false);
            self.emit(NewOrder { order });
        }

        fn settle_orders(
            ref self: ContractState,
            proof: Array<felt252>,
            order_proofs: Span<OrderProof>
        ) {
            // Step 1: Call the Garaga SP1 Verifier to validate the proof cryptographically
            let mut result_serialized = library_call_syscall(
                SP1_VERIFIER_CLASS_HASH.try_into().unwrap(),
                selector!("verify_sp1_groth16_proof_bn254"),
                proof.span(),
            )
                .unwrap_syscall();

            // Step 2: Deserialize the verification result
            let result = Serde::<Option<(u256, Span<u256>)>>::deserialize(ref result_serialized)
                .unwrap();

            // Step 3: Check if cryptographic verification succeeded
            assert(result.is_some(), 'Proof verification failed');

            // Step 4: Extract verification key and public inputs
            let (vk, public_inputs) = result.unwrap();

            // Step 5: Verify this proof corresponds to our expected SP1 program
            assert(vk == self.vk.read(), 'Wrong program');

            // Step 6: Extract merkle root from public inputs
            // The public inputs contain the merkle root from the SP1 program
            let merkle_root = self._extract_merkle_root(public_inputs);

            // Step 7: Verify and settle each order
            let mut i: u32 = 0;
            loop {
                if i >= order_proofs.len() {
                    break;
                }
                let order_proof = *order_proofs.at(i);
                
                let valid = self._verify_merkle_proof(
                    order_proof.proof,
                    merkle_root,
                    order_proof.order_hash,
                    order_proof.leaf_index
                );
                
                assert(valid, 'Invalid merkle proof');
                
                self.order_mapping.entry(order_proof.order_hash).write(true);
                self.emit(OrderSettled { order_hash: order_proof.order_hash });
                
                i += 1;
            };
        }

        fn reset_orders(ref self: ContractState, order_hashes: Span<u256>) {
            let mut i: u32 = 0;
            loop {
                if i >= order_hashes.len() {
                    break;
                }
                self.order_mapping.entry(*order_hashes.at(i)).write(false);
                i += 1;
            };
        }

        fn hash_order(self: @ContractState, order: Order) -> u256 {
            // Use keccak to match EVM encoding
            let mut data: Array<u256> = ArrayTrait::new();
            
            // Convert all fields to u256 for consistent hashing
            data.append(order.source_chain_id.into());
            data.append(order.destination_chain_id.into());
            
            // Receiver already provided as EVM-style u256 address
            data.append(order.receiver);
            
            data.append(order.amount);
            data.append(order.block_number.into());

            // Use keccak (big-endian inputs), then reverse bytes to match Solidity's big-endian output
            let hashed = keccak_u256s_be_inputs(data.span());
            let low: u128 = hashed.low;
            let high: u128 = hashed.high;
            let reversed_low = integer::u128_byte_reverse(low);
            let reversed_high = integer::u128_byte_reverse(high);
            u256 { low: reversed_high, high: reversed_low }
        }

        fn get_order_status(self: @ContractState, order_hash: u256) -> bool {
            self.order_mapping.entry(order_hash).read()
        }

        fn get_vk(self: @ContractState) -> u256 {
            self.vk.read()
        }

        fn verify_merkle_proof_public(
            self: @ContractState,
            proof: Span<u256>,
            root: u256,
            leaf: u256,
            index: u256
        ) -> bool {
            self._verify_merkle_proof(proof, root, leaf, index)
        }

        fn verify_sp1_proof_view(self: @ContractState, proof: Array<felt252>) -> Option<(u256, Span<u256>)> {
            let mut result_serialized = library_call_syscall(
                SP1_VERIFIER_CLASS_HASH.try_into().unwrap(),
                selector!("verify_sp1_groth16_proof_bn254"),
                proof.span(),
            ).unwrap_syscall();

            Serde::<Option<(u256, Span<u256>)>>::deserialize(ref result_serialized).unwrap()
        }
    }

    #[generate_trait]
    impl InternalFunctions of InternalFunctionsTrait {
        fn _extract_merkle_root(
            self: @ContractState,
            public_inputs: Span<u256>
        ) -> u256 {
            // Extract merkle root from public inputs
            // The public inputs from the SP1 program contain the merkle root
            // In the EVM version, merkle root is at bytes 8..40 of publicValues
            // Here it should be the first or second element depending on the SP1 program output
            assert(public_inputs.len() >= 1, 'Invalid public inputs length');
            
            // Return the first public input as the merkle root
            // This assumes the SP1 program outputs the merkle root as the first public value
            *public_inputs.at(0)
        }

        fn _verify_merkle_proof(
            self: @ContractState,
            proof: Span<u256>,
            root: u256,
            leaf: u256,
            index: u256
        ) -> bool {
            let mut computed_hash = leaf;
            
            let mut i: u32 = 0;
            loop {
                if i >= proof.len() {
                    break;
                }
                
                let proof_element = *proof.at(i);
                // OpenZeppelin MerkleProof: sorted pair hashing (order-independent)
                let (left, right) = if computed_hash <= proof_element {
                    (computed_hash, proof_element)
                } else {
                    (proof_element, computed_hash)
                };
                let mut hash_input: Array<u256> = ArrayTrait::new();
                hash_input.append(left);
                hash_input.append(right);
                let h = keccak_u256s_be_inputs(hash_input.span());
                let low: u128 = h.low;
                let high: u128 = h.high;
                let reversed_low = integer::u128_byte_reverse(low);
                let reversed_high = integer::u128_byte_reverse(high);
                computed_hash = u256 { low: reversed_high, high: reversed_low };
                
                i += 1;
            };
            
            computed_hash == root
        }
    }

}

