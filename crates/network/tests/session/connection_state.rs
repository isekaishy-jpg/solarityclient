use std::error::Error;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// An idle character screen must notice a duplicate-account disconnect even
// when unread setup bytes precede the FIN, without desynchronizing its cipher.
#[test]
fn idle_connection_reports_closure_without_consuming_buffered_bytes() -> Result<(), Box<dyn Error>>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let mut client = TcpStream::connect(listener.local_addr()?).await?;
            let (mut server, _) = listener.accept().await?;
            assert!(!super::connection_closed(&client)?);
            server.write_all(b"encrypted header and body").await?;
            client.readable().await?;
            assert!(!super::connection_closed(&client)?);
            server.shutdown().await?;
            tokio::time::timeout(Duration::from_secs(2), async {
                while !super::connection_closed(&client)? {
                    tokio::task::yield_now().await;
                }
                Ok::<_, std::io::Error>(())
            })
            .await??;
            let mut bytes = Vec::new();
            client.read_to_end(&mut bytes).await?;
            assert_eq!(bytes, b"encrypted header and body");
            Ok::<_, Box<dyn Error>>(())
        })
}

// A kicked connection may use RST rather than the graceful FIN tested above.
#[test]
fn idle_connection_reports_abortive_peer_reset() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let client = TcpStream::connect(listener.local_addr()?).await?;
            let (server, _) = listener.accept().await?;
            assert!(!super::connection_closed(&client)?);
            server.set_zero_linger()?;
            drop(server);
            tokio::time::timeout(Duration::from_secs(2), async {
                while !super::connection_closed(&client)? {
                    tokio::task::yield_now().await;
                }
                Ok::<_, std::io::Error>(())
            })
            .await??;
            Ok(())
        })
}
