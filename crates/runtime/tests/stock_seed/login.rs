//! External tests for cancellable runtime login ownership.

use std::error::Error;
use std::net::Ipv4Addr;

use solarity_network::{GruntLoginOptions, LoginLocale, TcpEndpoint};
use solarity_runtime::{
    LoginConfiguration, RuntimeLoginCoordinator, RuntimeLoginError, RuntimeLoginState,
};
use tokio::io::AsyncReadExt;
use tokio::runtime::Builder;

/// Cancellation aborts the sole task and closes its in-progress transport.
#[test]
fn login_coordinator_cancels_the_owned_transport() -> Result<(), Box<dyn Error>> {
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_io()
        .build()?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))?;
    let address = listener.local_addr()?;
    let configuration = LoginConfiguration::new(
        TcpEndpoint::new(address.ip().to_string(), address.port())?,
        GruntLoginOptions::new(LoginLocale::EnUs, -240, Ipv4Addr::LOCALHOST),
    );
    let mut coordinator = RuntimeLoginCoordinator::new(configuration);

    coordinator.begin(runtime.handle(), "testaccount", "hunter2")?;
    let (mut server, _peer) = runtime.block_on(listener.accept())?;
    assert_eq!(coordinator.state(), RuntimeLoginState::Authenticating);
    assert!(matches!(
        coordinator.begin(runtime.handle(), "testaccount", "hunter2"),
        Err(RuntimeLoginError::AlreadyActive)
    ));
    assert!(coordinator.cancel());
    assert_eq!(coordinator.state(), RuntimeLoginState::Idle);

    let closed_bytes = runtime.block_on(async {
        let mut challenge = [0_u8; 64];
        loop {
            let count = server.read(&mut challenge).await?;
            if count == 0 {
                return Ok::<usize, std::io::Error>(count);
            }
        }
    })?;
    assert_eq!(closed_bytes, 0);
    assert!(!coordinator.cancel());
    Ok(())
}

/// Stock-invalid credentials fail before task or socket admission.
#[test]
fn login_coordinator_rejects_credentials_before_admission() -> Result<(), Box<dyn Error>> {
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_io()
        .build()?;
    let configuration = LoginConfiguration::new(
        TcpEndpoint::new("127.0.0.1", 3724)?,
        GruntLoginOptions::new(LoginLocale::EnUs, -240, Ipv4Addr::LOCALHOST),
    );
    let mut coordinator = RuntimeLoginCoordinator::new(configuration);

    let result = coordinator.begin(runtime.handle(), "seventeen-byte-id", "hunter2");

    assert!(matches!(result, Err(RuntimeLoginError::Login(_))));
    assert_eq!(coordinator.state(), RuntimeLoginState::Idle);
    Ok(())
}
