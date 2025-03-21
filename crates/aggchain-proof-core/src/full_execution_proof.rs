use alloy_primitives::{Address, PrimitiveSignature, B256};
use serde::{Deserialize, Serialize};
use sha2::{Digest as Sha256Digest, Sha256};

use crate::{
    error::ProofError, keccak::digest::Digest, keccak::keccak256_combine, vkey_hash::HashU32,
};

/// Hardcoded hash of the "aggregation vkey".
/// NOTE: Format being `hash_u32()` of the `SP1StarkVerifyingKey`.
pub const AGGREGATION_VKEY_HASH: HashU32 = [0u32; 8]; // TODO: to put the right value

/// Specific commitment for the range proofs.
pub const RANGE_VKEY_COMMITMENT: [u8; 32] = [0u8; 32]; // TODO: to put the right value

/// Hardcoded for now, might see if we might need it as input
pub const OUTPUT_ROOT_VERSION: [u8; 32] = [0u8; 32];

/// Public values to verify the FEP.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FepPublicValues {
    pub l1_head: Digest,
    pub claim_block_num: u32,
    pub rollup_config_hash: Digest,
    pub prev_state_root: Digest,
    pub prev_withdrawal_storage_root: Digest,
    pub prev_block_hash: Digest,
    pub new_state_root: Digest,
    pub new_withdrawal_storage_root: Digest,
    pub new_block_hash: Digest,

    /// Trusted sequencer address.
    pub trusted_sequencer: Address,
    /// Signature in the "OptimisticMode" case.
    pub signature_optimistic_mode: Option<PrimitiveSignature>,
}

impl FepPublicValues {
    pub fn hash(&self) -> [u8; 32] {
        let public_values = [
            self.l1_head.as_slice(),
            self.compute_l2_pre_root().as_slice(),
            self.compute_claim_root().as_slice(),
            &self.claim_block_num.to_be_bytes(),
            self.rollup_config_hash.as_slice(),
            RANGE_VKEY_COMMITMENT.as_slice(),
        ]
        .concat();

        Sha256::digest(&public_values).into()
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum OptimisticMode {
    Sp1 = 0,
    Ecdsa = 1,
}

impl FepPublicValues {
    /// Compute the chain-specific commitment forwarded to the PP.
    pub fn aggchain_params(&self) -> Digest {
        let optimistic_mode: u8 = self.optimistic_mode() as u8;

        keccak256_combine([
            self.compute_l2_pre_root().as_slice(),
            self.compute_claim_root().as_slice(),
            &self.claim_block_num.to_be_bytes(),
            self.rollup_config_hash.as_slice(),
            &optimistic_mode.to_be_bytes(),
            self.trusted_sequencer.as_slice(),
        ])
    }

    fn optimistic_mode(&self) -> OptimisticMode {
        if self.signature_optimistic_mode.is_some() {
            OptimisticMode::Ecdsa
        } else {
            OptimisticMode::Sp1
        }
    }

    /// Verify one ECDSA or the sp1 proof.
    pub fn verify(&self) -> Result<(), ProofError> {
        if let Some(signature) = self.signature_optimistic_mode {
            // Verify only one ECDSA on the public inputs
            let recovered_signer = signature
                .recover_address_from_prehash(&B256::new(self.hash()))
                .map_err(|_| ProofError::InvalidSignature)?;

            if recovered_signer != self.trusted_sequencer {
                return Err(ProofError::InvalidSigner {
                    declared: self.trusted_sequencer,
                    recovered: recovered_signer,
                });
            }

            Ok(())
        } else {
            // Verify the FEP stark proof.
            #[cfg(not(target_os = "zkvm"))]
            unreachable!("verify_sp1_proof is not callable outside of SP1");

            #[cfg(target_os = "zkvm")]
            {
                sp1_zkvm::lib::verify::verify_sp1_proof(
                    &AGGREGATION_VKEY_HASH,
                    &self.hash().into(),
                );

                return Ok(());
            }
        }
    }
}

impl FepPublicValues {
    /// Compute l2 pre root.
    pub fn compute_l2_pre_root(&self) -> Digest {
        compute_output_root(
            self.prev_state_root.0,
            self.prev_withdrawal_storage_root.0,
            self.prev_block_hash.0,
        )
    }

    /// Compute claim root.
    pub fn compute_claim_root(&self) -> Digest {
        compute_output_root(
            self.new_state_root.0,
            self.new_withdrawal_storage_root.0,
            self.new_block_hash.0,
        )
    }
}

/// Compute output root as defined here:
/// https://specs.optimism.io/protocol/proposals.html#l2-output-commitment-construction
pub(crate) fn compute_output_root(
    state_root: [u8; 32],
    withdrawal_storage_root: [u8; 32],
    block_hash: [u8; 32],
) -> Digest {
    keccak256_combine([
        OUTPUT_ROOT_VERSION,
        state_root,
        withdrawal_storage_root,
        block_hash,
    ])
}

#[cfg(test)]
mod tests {
    use crate::full_execution_proof::compute_output_root;

    #[test]
    fn test_compute_output_root_expected_value() {
        // Provided inputs from the rpc endpoint: optimism_outputAtBlock
        let state_hex = "0xc82b7f91a1c9e78463653c6ec44a579062426d71d3404325fa5f129615e0473d";
        let withdrawal_hex = "0x8ed4baae3a927be3dea54996b4d5899f8c01e7594bf50b17dc1e741388ce3d12";
        let block_hash_hex = "0x61438199094c9db8d5c154034de9940712805469459346ed1b4e0fa57da5519b";
        let expected_output_root_hex =
            "0x720311395abb5216bee64000575e07dd3b64847b9f88d4d77b64e6aa28fc93a2";

        let state = hex_str_to_array(state_hex);
        let withdrawal = hex_str_to_array(withdrawal_hex);
        let block_hash = hex_str_to_array(block_hash_hex);
        let expected_output_root = hex_str_to_array(expected_output_root_hex);

        let computed_output_root = compute_output_root(state, withdrawal, block_hash).0;
        assert_eq!(
            computed_output_root, expected_output_root,
            "compute_output_root should return the expected hash"
        );
    }

    fn hex_str_to_array(s: &str) -> [u8; 32] {
        let s = s.trim_start_matches("0x");
        let bytes = hex::decode(s).expect("Decoding hex string failed");
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        array
    }
}
