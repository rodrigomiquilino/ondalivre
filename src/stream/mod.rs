//! Download de streams HTTP/HTTPS para arquivo temporário.
use std::io::Read;
use std::path::PathBuf;

pub fn fetch_stream_to_temp(url: &str, max_bytes: u64) -> anyhow::Result<PathBuf> {
    let url = url.trim();
    if url.is_empty() {
        anyhow::bail!("URL vazia");
    }
    let path = PathBuf::from(url);
    if path.exists() {
        return Ok(path);
    }
    let tmp = std::env::temp_dir().join(format!("ol_stream_{}.bin", uuid::Uuid::new_v4()));
    if url.starts_with("https://") || url.starts_with("http://") {
        let resp = ureq::get(url)
            .timeout(std::time::Duration::from_secs(25))
            .call()
            .map_err(|e| anyhow::anyhow!("stream fetch: {e}"))?;
        let mut reader = resp.into_reader();
        let mut file = std::fs::File::create(&tmp)?;
        let mut limited = reader.by_ref().take(max_bytes);
        std::io::copy(&mut limited, &mut file)?;
        return Ok(tmp);
    }
    anyhow::bail!("URL não suportada: {url}")
}

pub fn is_https(url: &str) -> bool {
    url.trim().starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn https_rustls_fetch() {
        // Prefer public HTTPS to exercise rustls/webpki path used by ureq.
        match fetch_stream_to_temp("https://example.com", 16384) {
            Ok(p) => assert!(p.exists() && std::fs::metadata(&p).unwrap().len() > 0),
            Err(e) => panic!("HTTPS/rustls fetch failed: {e}"),
        }
    }

    #[test]
    fn http_local_fetch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                let body = b"RIFF....WAVEfmt ";
                let resp = format!(
                    "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
                let _ = s.write_all(body);
            }
        });
        let url = format!("http://127.0.0.1:{port}/x.wav");
        let p = fetch_stream_to_temp(&url, 1024).unwrap();
        assert!(p.exists());
        let data = std::fs::read(&p).unwrap();
        assert!(data.starts_with(b"RIFF"));
    }
}
