use proposer_client::error::Error as ProposerClientError;

use crate::VKeyHash;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    AlloyProviderError(anyhow::Error),

    #[error("Proposer client error: {0}")]
    Client(#[from] ProposerClientError),

    #[error("Unable to create network prover")]
    UnableToCreateNetworkProver(#[source] anyhow::Error),

    #[error("Unsupported aggregation proof mode {0:?}")]
    UnsupportedAggregationProofMode(sp1_sdk::SP1ProofMode),

    #[error("Aggregation proof vkey mismatch (got: {got:?}, expected: {expected:?})")]
    AggregationVKeyMismatch { got: VKeyHash, expected: VKeyHash },
}
