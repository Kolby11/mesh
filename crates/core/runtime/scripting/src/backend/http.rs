use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const CACHE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub cache: bool,
}

fn default_method() -> String {
    "GET".into()
}
fn default_timeout() -> u64 {
    15_000
}

impl Request {
    pub(super) fn validate(&self) -> Result<(), String> {
        let uri: ureq::http::Uri = self
            .url
            .parse()
            .map_err(|error| format!("invalid HTTP URL: {error}"))?;
        if !matches!(uri.scheme_str(), Some("http" | "https"))
            || uri
                .authority()
                .is_none_or(|authority| authority.as_str().contains('@'))
        {
            return Err(
                "HTTP requests require an http/https URL without embedded credentials".into(),
            );
        }
        if !(1..=60_000).contains(&self.timeout_ms) {
            return Err("HTTP timeout_ms must be between 1 and 60000".into());
        }
        if self.body.len() > MAX_REQUEST_BYTES
            || self.url.len() > 8192
            || self.headers.len() > 64
            || self
                .headers
                .iter()
                .any(|(name, value)| name.len() + value.len() > 8192)
        {
            return Err("HTTP request exceeds size limits".into());
        }
        self.method
            .parse::<ureq::http::Method>()
            .map_err(|error| error.to_string())?;
        if self.cache && self.method != "GET" {
            return Err("HTTP cache is only available for GET".into());
        }
        Ok(())
    }
}

pub(super) fn cache_root(module: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        });
    base.join("mesh/http")
        .join(format!("{:x}", Sha256::digest(module.as_bytes())))
}

pub(super) fn request(spec: Request, output_limit: usize, cache_root: &Path) -> Value {
    match execute(spec, output_limit, cache_root) {
        Ok(value) => value,
        Err(error) => {
            json!({"success": false, "status": 0, "headers": {}, "body": "", "path": "", "error": error})
        }
    }
}

fn execute(spec: Request, output_limit: usize, cache_root: &Path) -> Result<Value, String> {
    spec.validate()?;
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(spec.timeout_ms)))
        .max_redirects(5)
        .https_only(
            spec.url
                .parse::<ureq::http::Uri>()
                .ok()
                .is_some_and(|uri| uri.scheme_str() == Some("https")),
        )
        .http_status_as_error(false)
        .build();
    let agent: ureq::Agent = config.into();
    let mut builder = ureq::http::Request::builder()
        .method(spec.method.as_str())
        .uri(&spec.url);
    for (name, value) in &spec.headers {
        builder = builder.header(name, value);
    }
    let request = builder.body(spec.body).map_err(|error| error.to_string())?;
    let mut response = agent.run(request).map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let success = (200..300).contains(&status);
    let mut headers = BTreeMap::<String, String>::new();
    let mut header_bytes = 0;
    for (name, value) in response.headers() {
        let value = value.to_str().map_err(|error| error.to_string())?;
        header_bytes += name.as_str().len() + value.len();
        if header_bytes > output_limit / 4 {
            return Err("HTTP response headers exceed output limit".into());
        }
        headers
            .entry(name.to_string())
            .and_modify(|previous| {
                previous.push_str(", ");
                previous.push_str(value);
            })
            .or_insert_with(|| value.to_owned());
    }
    let limit = if spec.cache {
        MAX_BODY_BYTES
    } else {
        MAX_BODY_BYTES.min(output_limit / 8)
    };
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err(format!("HTTP response exceeds {limit} bytes"));
    }
    let (body, path) = if spec.cache && success {
        let path = cache_bytes(
            cache_root,
            &bytes,
            headers
                .get("content-type")
                .map(String::as_str)
                .unwrap_or(""),
        )?;
        (String::new(), path.to_string_lossy().into_owned())
    } else if spec.cache {
        (String::new(), String::new())
    } else {
        (
            String::from_utf8(bytes).map_err(|_| {
                "HTTP body is not UTF-8; use cache = true for binary data".to_string()
            })?,
            String::new(),
        )
    };
    Ok(
        json!({"success": success, "status": status, "headers": headers, "body": body, "path": path, "error": if success { String::new() } else { format!("HTTP {status}") }}),
    )
}

fn cache_bytes(root: &Path, bytes: &[u8], content_type: &str) -> Result<PathBuf, String> {
    use std::os::unix::fs::OpenOptionsExt;
    if !root.is_absolute() {
        return Err("HTTP cache needs an absolute XDG_CACHE_HOME or HOME".into());
    }
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let extension = match content_type.split(';').next().unwrap_or("").trim() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/webp" => "webp",
        _ => "bin",
    };
    let path = root.join(format!("{:x}.{extension}", Sha256::digest(bytes)));
    if !path.is_file() {
        let temporary = root.join(format!(
            ".{}-{}",
            std::process::id(),
            NEXT_CACHE_FILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let result = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(bytes)?;
            std::fs::rename(&temporary, &path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|error| error.to_string())?;
    }
    let mut files = std::fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .is_file()
                .then(|| (entry.path(), metadata.len(), metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| entry.2);
    let mut total: u64 = files.iter().map(|entry| entry.1).sum();
    let mut count = files.len();
    for (old, size, _) in files {
        if total <= CACHE_BYTES && count <= 128 {
            break;
        }
        if old != path && std::fs::remove_file(old).is_ok() {
            total = total.saturating_sub(size);
            count -= 1;
        }
    }
    Ok(path)
}

static NEXT_CACHE_FILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn response(reply: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/test", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let length = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..length]).starts_with("GET /test HTTP/1.1"));
            stream.write_all(reply).unwrap();
        });
        (url, handle)
    }

    fn spec(url: String) -> Request {
        Request {
            url,
            method: "GET".into(),
            headers: BTreeMap::new(),
            body: String::new(),
            timeout_ms: 1000,
            cache: false,
        }
    }

    #[test]
    fn http_returns_status_headers_and_bounded_text_without_exec() {
        let (url, server) = response(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nX-Test: yes\r\nConnection: close\r\n\r\nhello");
        let result = request(spec(url), 4096, Path::new("/unused"));
        server.join().unwrap();
        assert_eq!(result["success"], true, "{result}");
        assert_eq!(result["body"], "hello");
        assert_eq!(result["headers"]["x-test"], "yes");
        assert_eq!(result["status"], 200);
        let (url, server) = response(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 3\r\nConnection: close\r\n\r\nno!",
        );
        let result = request(spec(url), 4096, Path::new("/unused"));
        server.join().unwrap();
        assert_eq!(result["success"], false);
        assert_eq!(result["status"], 404);
        assert_eq!(result["body"], "no!");
    }

    #[test]
    fn http_rejects_oversize_bodies_and_caches_binary_with_an_image_extension() {
        let root = std::env::temp_dir().join(format!(
            "mesh-http-test-{}-{}",
            std::process::id(),
            NEXT_CACHE_FILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let (url, server) = response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n0123456789abcdefg",
        );
        let result = request(spec(url), 128, &root);
        server.join().unwrap();
        assert_eq!(result["success"], false);
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("response exceeds 16 bytes"),
            "{result}"
        );
        let (url, server) = response(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n\x89PNG");
        let mut input = spec(url);
        input.cache = true;
        let result = request(input, 4096, &root);
        server.join().unwrap();
        assert_eq!(result["success"], true, "{result}");
        let path = Path::new(result["path"].as_str().unwrap());
        assert!(path.starts_with(&root));
        assert_eq!(path.extension().unwrap(), "png");
        assert_eq!(std::fs::read(path).unwrap(), b"\x89PNG");
        assert_eq!(result["body"], "");
        std::fs::remove_dir_all(root).unwrap();
        for url in [
            "file:///etc/passwd",
            "ftp://example.test/file",
            "https://user:secret@example.test/",
        ] {
            assert!(spec(url.into()).validate().is_err());
        }
    }

    #[test]
    fn http_deadline_covers_waiting_for_the_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut input = spec(format!("http://{}/", listener.local_addr().unwrap()));
        input.timeout_ms = 25;
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(100));
        });
        let result = request(input, 4096, Path::new("/unused"));
        server.join().unwrap();
        assert_eq!(result["success"], false);
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("timeout"),
            "{result}"
        );
    }
}
