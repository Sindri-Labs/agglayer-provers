use std::collections::HashMap;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use aggchain_proof_builder::{AggchainProofBuilder, AggchainProofBuilderResponse};
use aggchain_proof_core::proof::{AggchainProof, InclusionProof, L1InfoTreeLeaf};
use aggkit_prover_types::Hash;
use futures::{FutureExt as _, TryFutureExt};
use proposer_service::{ProposerRequest, ProposerService};
use tower::{util::BoxCloneService, ServiceExt as _};

use crate::config::AggchainProofServiceConfig;
use crate::error::Error;

/// A request for the AggchainProofService to generate the
/// aggchain proof for the range of blocks.
#[derive(Default, Clone, Debug)]
#[allow(unused)]
pub struct AggchainProofServiceRequest {
    /// Aggchain proof starting block
    pub start_block: u64,
    /// Max number of blocks that the aggchain proof is allowed to contain
    pub max_block: u64,
    /// Root hash of the L1 info tree.
    pub l1_info_tree_root_hash: Hash,
    /// Particular leaf of the l1 info tree corresponding
    /// to the max_block.
    pub l1_info_tree_leaf: L1InfoTreeLeaf,
    /// Inclusion proof of the l1 info tree leaf to the
    /// l1 info tree root.
    pub l1_info_tree_merkle_proof: [Hash; 32],
    /// Map of the Global Exit Roots with their inclusion proof.
    /// Note: the GER (string) is a base64 encoded string of the GER digest.
    pub ger_inclusion_proofs: HashMap<String, InclusionProof>,
}

/// Resulting generated Aggchain proof
pub struct AggchainProofServiceResponse {
    /// Aggchain proof generated by the `aggchain-proof-builder` service
    /// per `agg-sender` request.
    pub proof: AggchainProof,
    /// First block in the aggchain proof.
    pub start_block: u64,
    /// Last block in the aggchain proof (inclusive).
    pub end_block: u64,
    /// Local exit root calculated by the aggkit-prover for all the bridge
    /// changes included in the proof. Mismatch between LER calculation on the
    /// agglayer and the L2 is possible due to the field -
    /// `forceUpdateGlobalExitRoot`.
    pub local_exit_root_hash: Vec<u8>,
    /// Custom chain data calculated by the `aggkit-prover`, required by the
    /// agg-sender to fill in related certificate field.
    /// Consists off the two bytes for aggchain selector, 32 bytes for the
    /// output_root (new state root) and the l2 end block number.
    pub custom_chain_data: Vec<u8>,
}

/// The Aggchain proof service is responsible for orchestrating an Aggchain
/// proof generation.
///
/// The Aggchain proof is generated by fetching the Aggregated FEP from the
/// proposer service and the `aggchain-proof-builder` service to generate the
/// Aggchain proof.
#[derive(Clone)]
pub struct AggchainProofService {
    pub(crate) proposer_service: BoxCloneService<
        proposer_service::ProposerRequest,
        proposer_service::ProposerResponse,
        proposer_service::Error,
    >,
    pub(crate) aggchain_proof_builder: BoxCloneService<
        aggchain_proof_builder::AggchainProofBuilderRequest,
        aggchain_proof_builder::AggchainProofBuilderResponse,
        aggchain_proof_builder::Error,
    >,
}

impl AggchainProofService {
    pub fn new(config: &AggchainProofServiceConfig) -> Result<Self, Error> {
        let l1_rpc_client = Arc::new(
            prover_alloy::AlloyProvider::new(
                &config.proposer_service.l1_rpc_endpoint,
                prover_alloy::DEFAULT_HTTP_RPC_NODE_INITIAL_BACKOFF_MS,
                prover_alloy::DEFAULT_HTTP_RPC_NODE_BACKOFF_MAX_RETRIES,
            )
            .map_err(Error::AlloyProviderError)?,
        );

        let proposer_service = tower::ServiceBuilder::new()
            .service(ProposerService::new(
                &config.proposer_service,
                l1_rpc_client.clone(),
            )?)
            .boxed_clone();

        let aggchain_proof_builder = tower::ServiceBuilder::new()
            .service(AggchainProofBuilder::new(&config.aggchain_proof_builder)?)
            .boxed_clone();

        Ok(AggchainProofService {
            proposer_service,
            aggchain_proof_builder,
        })
    }
}

impl tower::Service<AggchainProofServiceRequest> for AggchainProofService {
    type Response = AggchainProofServiceResponse;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        std::task::ready!(self.proposer_service.poll_ready(cx)?);

        self.aggchain_proof_builder
            .poll_ready(cx)
            .map_err(Error::from)
    }

    fn call(&mut self, req: AggchainProofServiceRequest) -> Self::Future {
        let l1_block_number = req.max_block;

        let proposer_request = ProposerRequest {
            start_block: req.start_block,
            max_block: req.max_block,
            l1_block_number,
        };

        let mut proof_builder = self.aggchain_proof_builder.clone();

        self.proposer_service
            .call(proposer_request)
            .map_err(Error::from)
            .and_then(move |agg_span_proof_response| {
                let aggchain_proof_builder_request =
                    aggchain_proof_builder::AggchainProofBuilderRequest {
                        agg_span_proof: agg_span_proof_response.agg_span_proof,
                        start_block: agg_span_proof_response.start_block,
                        end_block: agg_span_proof_response.end_block,
                        l1_info_tree_merkle_proof: req.l1_info_tree_merkle_proof,
                        l1_info_tree_leaf: req.l1_info_tree_leaf,
                        l1_info_tree_root_hash: req.l1_info_tree_root_hash,
                        ger_inclusion_proofs: req.ger_inclusion_proofs,
                    };

                proof_builder
                    .call(aggchain_proof_builder_request)
                    .map_err(Error::from)
                    .map(move |aggchain_proof_builder_result| {
                        let agg_span_proof_response: AggchainProofBuilderResponse =
                            aggchain_proof_builder_result?;
                        Ok(AggchainProofServiceResponse {
                            proof: agg_span_proof_response.proof,
                            start_block: agg_span_proof_response.start_block,
                            end_block: agg_span_proof_response.end_block,
                            local_exit_root_hash: Default::default(),
                            custom_chain_data: Default::default(),
                        })
                    })
            })
            .boxed()
    }
}
