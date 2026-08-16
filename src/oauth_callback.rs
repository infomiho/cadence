use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use url::Url;

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(180);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_HEADER_BYTES: usize = 8 * 1024;

pub(crate) struct OAuthCallback {
    stream: TcpStream,
    url: Url,
}

impl OAuthCallback {
    pub(crate) fn url(&self) -> &Url {
        &self.url
    }

    pub(crate) async fn respond_html(&mut self, body: &str) -> Result<()> {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        self.stream.write_all(response.as_bytes()).await?;
        Ok(())
    }
}

pub(crate) async fn receive_callback(
    listener: &TcpListener,
    expected_path: &str,
) -> Result<OAuthCallback> {
    timeout(AUTHORIZATION_TIMEOUT, async {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let Ok(Ok(target)) = timeout(REQUEST_TIMEOUT, read_request_target(&mut stream)).await
            else {
                continue;
            };
            let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else {
                continue;
            };
            if url.path() == expected_path {
                return Ok(OAuthCallback { stream, url });
            }
        }
    })
    .await
    .context("OAuth callback timed out")?
}

async fn read_request_target(reader: &mut (impl AsyncRead + Unpin)) -> Result<String> {
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            bail!("OAuth callback connection closed before sending headers");
        }
        if request.len().saturating_add(read) > MAX_HEADER_BYTES {
            bail!("OAuth callback headers exceed {MAX_HEADER_BYTES} bytes");
        }
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            break;
        }
    }

    let request = std::str::from_utf8(&request).context("OAuth callback headers are not UTF-8")?;
    let mut parts = request
        .lines()
        .next()
        .context("OAuth callback omitted the request line")?
        .split_whitespace();
    let method = parts.next();
    let target = parts.next();
    let version = parts.next();
    if method != Some("GET")
        || !matches!(version, Some("HTTP/1.0" | "HTTP/1.1"))
        || parts.next().is_some()
    {
        bail!("OAuth callback request line is invalid");
    }
    let Some(target) = target else {
        bail!("OAuth callback omitted the request target");
    };
    if !target.starts_with('/') {
        bail!("OAuth callback request target is invalid");
    }
    Ok(target.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn receives_fragmented_callback_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let receiver = tokio::spawn(async move { receive_callback(&listener, "/callback").await });
        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(b"GET /call").await.unwrap();
        client
            .write_all(b"back?code=secret HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let callback = receiver.await.unwrap().unwrap();
        assert_eq!(callback.url().path(), "/callback");
        assert_eq!(callback.url().query(), Some("code=secret"));
    }

    #[tokio::test]
    async fn rejects_oversized_callback_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = tokio::spawn(async move {
            let mut stream = TcpStream::connect(address).await.unwrap();
            stream
                .write_all(&vec![b'a'; MAX_HEADER_BYTES + 1])
                .await
                .unwrap();
        });
        let (mut stream, _) = listener.accept().await.unwrap();

        assert!(read_request_target(&mut stream).await.is_err());
        client.await.unwrap();
    }
}
