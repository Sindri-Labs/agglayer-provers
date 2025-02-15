use std::fmt::Display;

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{error::Error, ProposerRequest};

/// Proposer client that requests the generation
/// of the AggSpanProof from the proposer and gets
/// proof_id in response.
#[tonic::async_trait]
pub trait AggSpanProofProposer {
    async fn request_agg_proof(
        &self,
        request: AggSpanProofProposerRequest,
    ) -> Result<AggSpanProofProposerResponse, Error>;
}

pub struct ProposerRpcClient {
    client: reqwest::Client,
    url: String,
}

impl ProposerRpcClient {
    pub fn new(rpc_endpoint: &str) -> Result<Self, Error> {
        let headers = reqwest::header::HeaderMap::new();
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(ProposerRpcClient {
            client,
            url: rpc_endpoint.to_owned(),
        })
    }
}

#[tonic::async_trait]
impl AggSpanProofProposer for ProposerRpcClient {
    async fn request_agg_proof(
        &self,
        request: AggSpanProofProposerRequest,
    ) -> Result<AggSpanProofProposerResponse, Error> {
        let proof_response = self
            .client
            .post(format!("{}/request_agg_proof", self.url.as_str()))
            .json(&request)
            .send()
            .await?
            .json::<AggSpanProofProposerResponse>()
            .await?;

        info!(
            proof_id = proof_response.to_string(),
            "agg proof request submitted"
        );

        Ok(proof_response)
    }
}

/// Request format for the proposer `request_agg_proof`
#[derive(Deserialize, Serialize, Debug)]
pub struct AggSpanProofProposerRequest {
    pub start: u64,
    pub end: u64,
    pub l1_block_number: u64,
    pub l1_block_hash: B256,
}

impl From<AggSpanProofProposerRequest> for ProposerRequest {
    fn from(request: AggSpanProofProposerRequest) -> Self {
        ProposerRequest {
            start_block: request.start,
            max_block: request.end,
            l1_block_number: request.l1_block_number,
        }
    }
}

/// Response for the external proposer `request_span_proof` call
#[derive(Serialize, Deserialize, Debug)]
pub struct AggSpanProofProposerResponse {
    pub proof_id: B256,
    pub start_block: u64,
    pub end_block: u64,
}

impl Display for AggSpanProofProposerResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.proof_id)
    }
}
