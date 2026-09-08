use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn request(address: std::net::SocketAddr, method: &str, path: &str) -> String {
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    // Exercise malformed GET bodies and valid work-submission POST bodies.
    let body = if method == "POST" {
        r#"{"goal":"run work","sender":"a","recipient_handle":"b","intent":"work","payload":{}}"#
    } else {
        "!"
    };
    let size = body.len();
    let wire = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {size}\r\n\r\n{body}"
    );
    socket.write_all(wire.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        socket.read_to_end(&mut bytes),
    )
    .await
    .unwrap()
    .unwrap();
    String::from_utf8(bytes).unwrap()
}
