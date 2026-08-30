//! External stock-compatibility tests for the operating-system TCP boundary.

use std::error::Error;

use solarity_network::{TcpEndpoint, TcpTransport, TransportError};

/// Realm authorities retain DNS/IPv4 and bracketed IPv6 syntax without guessing ports.
#[test]
fn tcp_endpoints_validate_stock_realm_authorities() -> Result<(), Box<dyn Error>> {
    let dns = TcpEndpoint::parse("realm.example.test:8085")?;
    assert_eq!(dns.host(), "realm.example.test");
    assert_eq!(dns.port(), 8_085);
    assert_eq!(dns.to_string(), "realm.example.test:8085");

    let ipv6 = TcpEndpoint::parse("[2001:db8::7]:3724")?;
    assert_eq!(ipv6.host(), "2001:db8::7");
    assert_eq!(ipv6.port(), 3_724);
    assert_eq!(ipv6.to_string(), "[2001:db8::7]:3724");

    for invalid in ["realm", ":8085", "realm:0", "2001:db8::7:3724"] {
        assert!(matches!(
            TcpEndpoint::parse(invalid),
            Err(TransportError::InvalidEndpoint { .. })
        ));
    }
    Ok(())
}

/// A loopback connection owns one socket and enables stock low-latency writes.
#[test]
fn tcp_transport_connects_once_and_enables_nodelay() -> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let endpoint = TcpEndpoint::new(address.ip().to_string(), address.port())?;
        let accept_task = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await?;
            Ok::<_, std::io::Error>((stream, peer))
        });

        let stream = TcpTransport::connect(&endpoint).await?;
        assert!(stream.nodelay()?);
        assert_eq!(stream.peer_addr()?.port(), address.port());
        let (_accepted, peer) = accept_task.await??;
        assert_eq!(peer, stream.local_addr()?);
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

/// A refused connection remains one typed failure and is not retried in the background.
#[test]
fn tcp_transport_reports_refused_connection() -> Result<(), Box<dyn Error + Send + Sync>> {
    runtime()?.block_on(async {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        drop(listener);
        let endpoint = TcpEndpoint::new(address.ip().to_string(), address.port())?;

        let error = match TcpTransport::connect(&endpoint).await {
            Err(error) => error,
            Ok(_) => return Err("connection unexpectedly succeeded after listener closed".into()),
        };
        assert!(matches!(error, TransportError::Connect { .. }));
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
}

fn runtime() -> Result<tokio::runtime::Runtime, Box<dyn Error + Send + Sync>> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}
