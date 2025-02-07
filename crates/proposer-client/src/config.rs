use std::str::FromStr;
use std::time::Duration;

use prover_utils::from_env_or_default;
use serde::{Deserialize, Serialize};
use url::Url;

/// The default proposer service endpoint
const DEFAULT_PROPOSER_SERVICE_ENDPOINT: &str = "127.0.0.1:3000";

/// The default url endpoint for the grpc cluster service
const DEFAULT_SP1_CLUSTER_ENDPOINT: &str = "127.0.0.1:5432";

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ProposerClientConfig {
    /// The proposer service http endpoint
    pub proposer_endpoint: Url,
    /// The sp1 proving cluster endpoint
    pub sp1_cluster_endpoint: Url,
    /// Network prover program
    pub prover_program: Vec<u8>,
    /// Proving timeout in seconds
    #[serde(default = "default_timeout")]
    pub proving_timeout: Duration,
}

impl Default for ProposerClientConfig {
    fn default() -> Self {
        Self {
            proposer_endpoint: default_proposer_service_endpoint(),
            sp1_cluster_endpoint: default_sp1_cluster_endpoint(),
            prover_program: vec![],
            proving_timeout: default_timeout(),
        }
    }
}

fn default_proposer_service_endpoint() -> Url {
    from_env_or_default(
        "PROPOSER_SERVICE_ENDPOINT",
        Url::from_str(DEFAULT_PROPOSER_SERVICE_ENDPOINT).unwrap(),
    )
}

fn default_sp1_cluster_endpoint() -> Url {
    from_env_or_default(
        "SP1_CLUSTER_ENDPOINT",
        Url::from_str(DEFAULT_SP1_CLUSTER_ENDPOINT).unwrap(),
    )
}

fn default_timeout() -> Duration {
    Duration::from_secs(3600)
}
