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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn test_proxy_connection() {
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut backend_socket, _) = backend_listener.accept().await.unwrap();
            let mut buffer = [0; 1024];
            let bytes_read = backend_socket.read(&mut buffer).await.unwrap();
            backend_socket
                .write_all(&buffer[..bytes_read])
                .await
                .unwrap();
        });

        let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_listener.local_addr().unwrap();
        let backend = BackendConfig {
            backend_address: backend_addr,
            backend_id: "backend-1".to_string(),
            weight: None,
        };

        let proxy_task = tokio::spawn(async move {
            let (client_stream, _) = client_listener.accept().await.unwrap();
            proxy_connection(client_stream, backend).await.unwrap();
        });

        let test_result = timeout(Duration::from_secs(2), async {
            let mut client = TcpStream::connect(client_addr).await.unwrap();
            client.write_all(b"ping").await.unwrap();

            let mut response = [0; 4];
            client.read_exact(&mut response).await.unwrap();

            assert_eq!(&response, b"ping");
        })
        .await;

        proxy_task.abort();
        test_result.unwrap();
    }
}
