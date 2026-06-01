use crate::config::BackendConfig;
use tokio::io;
use tokio::net::TcpStream;

pub async fn proxy_connection(
    mut client_stream: TcpStream,
    backend: BackendConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut backend_stream = TcpStream::connect(backend.backend_address).await?;
    io::copy_bidirectional(&mut client_stream, &mut backend_stream).await?;

    Ok(())
}
