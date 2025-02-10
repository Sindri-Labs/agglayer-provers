use std::{path::PathBuf, sync::Arc};

use prover_engine::ProverEngine;
use sp1_sdk::HashableKey;
#[cfg(feature = "testutils")]
pub use testutils::start_prover;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[cfg(feature = "testutils")]
pub mod fake;
pub mod prover;
mod rpc;

/// This is the main prover entrypoint.
///
/// This function starts everything needed to run an Agglayer Prover.
/// Starting by a Tokio runtime which can be used by the different components.
/// The configuration file is parsed and used to configure the prover.
///
/// This function returns on fatal error or after graceful shutdown has
/// completed.
pub fn main(cfg: PathBuf, version: &str, program: &'static [u8]) -> anyhow::Result<()> {
    let config = Arc::new(agglayer_prover_config::ProverConfig::try_load(&cfg)?);

    // Initialize the logger
    prover_logger::tracing(&config.log);

    let global_cancellation_token = CancellationToken::new();

    info!("Starting agglayer prover version info: {}", version);

    let prover_runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("agglayer-prover-runtime")
        .enable_all()
        .build()?;

    let metrics_runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("metrics-runtime")
        .worker_threads(2)
        .enable_all()
        .build()?;

    let pp_service =
        prover_runtime.block_on(async { crate::prover::Prover::create_service(&config, program) });

    _ = ProverEngine::builder()
        .add_rpc_service(pp_service)
        .set_rpc_runtime(prover_runtime)
        .set_metrics_runtime(metrics_runtime)
        .set_cancellation_token(global_cancellation_token)
        .set_rpc_socket_addr(config.grpc_endpoint)
        .set_metric_socket_addr(config.telemetry.addr)
        .start();

    Ok(())
}
pub fn get_vkey(program: &'static [u8]) -> String {
    let vkey = prover_executor::Executor::get_vkey(program);
    vkey.bytes32().to_string()
}

#[cfg(feature = "testutils")]
mod testutils {
    use std::sync::Arc;

    use agglayer_prover_config::ProverConfig;
    use tokio_util::sync::CancellationToken;

    use super::prover::Prover;

    #[tokio::main]
    pub async fn start_prover(
        config: Arc<ProverConfig>,
        global_cancellation_token: CancellationToken,
        program: &'static [u8],
    ) {
        let prover = Prover::builder()
            .config(config)
            .cancellation_token(global_cancellation_token)
            .program(program)
            .set_rpc_socket_addr(config.grpc_endpoint)
            .set_metric_socket_addr(config.telemetry.addr)
            .start()
            .await
            .unwrap();
        prover.await_shutdown().await;
    }
}
