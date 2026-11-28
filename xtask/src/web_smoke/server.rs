//! Ephemeral static file server for the built `dist/` directory.
//!
//! Binds `127.0.0.1:0` -- a random, OS-assigned free port, so nothing has to
//! guess or configure a port -- and serves whatever `trunk build --release`
//! produced, on its own background thread, until the returned [`StaticServer`]
//! is dropped (its `Drop` impl unblocks the server loop and joins the thread,
//! so cleanup happens even when a scenario step fails/returns early via `?`).
//!
//! Deliberately not `trunk serve`: that command owns a fixed configured port
//! (`Trunk.toml`'s `[serve] port = 8080`) and injects its own live-reload
//! script/websocket into the served HTML. Neither belongs in a scenario that
//! wants a genuinely cold, deterministic first paint of the *release*
//! bundle -- `trunk build --release` produces `dist/` once, and this module
//! only ever serves those static bytes back, unmodified.
//!
//! Requests use origin-form paths, percent-decoded once. Parent/dot and
//! platform-ambiguous components reject before resolution; only regular files
//! canonically inside the startup-resolved bundle root are read. As with the
//! bundle validator, the filesystem is trusted: no hostile concurrent mutation
//! between canonicalization and read is supported.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct StaticServer {
    port: u16,
    server: Arc<tiny_http::Server>,
    handle: Option<JoinHandle<()>>,
    request_log: Arc<Mutex<Vec<String>>>,
}

impl StaticServer {
    /// Starts serving `dist_dir` on a random free `127.0.0.1` port.
    pub fn start(dist_dir: PathBuf) -> Result<Self, String> {
        let dist_dir = std::fs::canonicalize(dist_dir)
            .map_err(|e| format!("failed to resolve static-server root: {e}"))?;
        if !dist_dir.is_dir() {
            return Err("static-server root is not a directory".to_owned());
        }
        let server = tiny_http::Server::http("127.0.0.1:0")
            .map_err(|e| format!("failed to bind an ephemeral static-server port: {e}"))?;
        let server = Arc::new(server);
        let port = match server.server_addr() {
            tiny_http::ListenAddr::IP(addr) => addr.port(),
            #[allow(unreachable_patterns)]
            other => {
                return Err(format!(
                    "static server bound a non-IP address ({other:?}); expected a TCP port"
                ));
            }
        };

        let request_log = Arc::new(Mutex::new(Vec::new()));
        let worker_server = server.clone();
        let worker_log = request_log.clone();
        let handle = std::thread::spawn(move || serve_loop(worker_server, dist_dir, worker_log));

        Ok(Self {
            port,
            server,
            handle: Some(handle),
            request_log,
        })
    }

    /// The randomly assigned port `base_url` is built from -- kept public
    /// (even though nothing in this crate reads it directly today) since a
    /// later scenario's diagnostics may want it without reparsing `base_url`.
    #[allow(dead_code)]
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// A snapshot of every request handled so far, formatted as one
    /// `<status> <path>` line per request -- written verbatim into
    /// `server.log` by a scenario's failure/success artifact capture.
    pub fn request_log(&self) -> Vec<String> {
        self.request_log.lock().unwrap().clone()
    }
}

impl Drop for StaticServer {
    fn drop(&mut self) {
        // Wakes the blocking `recv()` in `serve_loop` so the thread can
        // observe the server being torn down and exit, instead of the
        // thread (and the `Server`/listening socket it owns) leaking past
        // this scenario -- runs even when the caller returned early via `?`.
        self.server.unblock();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn serve_loop(server: Arc<tiny_http::Server>, dist_dir: PathBuf, log: Arc<Mutex<Vec<String>>>) {
    loop {
        match server.recv() {
            Ok(request) => handle_request(request, &dist_dir, &log),
            // `unblock()` (called from `Drop`) surfaces here as a recv error;
            // either way, a broken server means it's time to stop serving.
            Err(_) => return,
        }
    }
}

fn handle_request(request: tiny_http::Request, dist_dir: &Path, log: &Arc<Mutex<Vec<String>>>) {
    let url_path = request.url().split('?').next().unwrap_or("/").to_string();
    let file_path = relative_request_path(&url_path).map(|relative| dist_dir.join(relative));
    let readable = file_path
        .as_ref()
        .and_then(|path| std::fs::canonicalize(path).ok())
        .filter(|path| path.starts_with(dist_dir) && path.is_file());
    // Read the checked canonical target, not the unresolved alias. Containment
    // uses Path components (dist-sibling is not inside dist), not string prefixes.
    let (status, body) = readable
        .and_then(|path| std::fs::read(path).ok())
        .map_or((404u16, Vec::new()), |bytes| (200u16, bytes));
    log.lock()
        .unwrap()
        .push(format!("{status} {url_path} ({} bytes)", body.len()));

    let mut response = tiny_http::Response::from_data(body).with_status_code(status);
    // MIME follows the requested name, including safe aliases and missing files.
    let mime = file_path
        .as_deref()
        .map_or("application/octet-stream", content_type);
    if let Ok(header) = tiny_http::Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()) {
        response = response.with_header(header);
    }
    let _ = request.respond(response);
}

fn relative_request_path(raw: &str) -> Option<PathBuf> {
    if !raw.starts_with('/') || raw.contains('#') {
        return None;
    }
    let mut decoded = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = char::from(*bytes.get(index + 1)?).to_digit(16)?;
            let low = char::from(*bytes.get(index + 2)?).to_digit(16)?;
            decoded.push((high * 16 + low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded).ok()?;
    if decoded
        .bytes()
        .any(|b| b.is_ascii_control() || matches!(b, b'\\' | b':'))
    {
        return None;
    }
    let relative = decoded.strip_prefix('/')?;
    if relative.is_empty() {
        return Some(PathBuf::from("index.html"));
    }
    // Reject Windows drive/UNC/ADS and trailing-dot/space aliases on every host,
    // not just when the current OS happens to interpret those spellings.
    if relative
        .split('/')
        .any(|part| part.is_empty() || matches!(part, "." | "..") || part.ends_with(['.', ' ']))
    {
        return None;
    }
    let path = PathBuf::from(relative);
    path.components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
        .then_some(path)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("png") => "image/png",
        Some("ttf") => "font/ttf",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct Fixture {
        base: PathBuf,
        dist: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let base = std::env::temp_dir().join(format!(
                "rff-http-{}-{nonce}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let dist = base.join("dist");
            std::fs::create_dir_all(dist.join("assets/nested")).unwrap();
            std::fs::create_dir_all(base.join("dist-sibling")).unwrap();
            std::fs::write(dist.join("index.html"), "fixture index").unwrap();
            std::fs::write(base.join("outside-fixture.txt"), "OUTSIDE-OWNED-FIXTURE").unwrap();
            std::fs::write(
                base.join("dist-sibling/sentinel.txt"),
                "SIBLING-OWNED-FIXTURE",
            )
            .unwrap();
            Self { base, dist }
        }
        fn server(&self) -> StaticServer {
            StaticServer::start(self.dist.clone()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    struct Reply {
        status: u16,
        mime: String,
        body: Vec<u8>,
    }
    fn get(server: &StaticServer, target: &str) -> Reply {
        // Do not use a URL client: it may normalize away the traversal under test.
        let mut stream = TcpStream::connect(("127.0.0.1", server.port())).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(
            stream,
            "GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        let split = bytes.windows(4).position(|p| p == b"\r\n\r\n").unwrap();
        let headers = std::str::from_utf8(&bytes[..split]).unwrap();
        let status = headers
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let mime = headers
            .lines()
            .find_map(|l| {
                l.split_once(':')
                    .filter(|(key, _)| key.eq_ignore_ascii_case("content-type"))
                    .map(|(_, value)| value.trim().to_owned())
            })
            .unwrap_or_else(|| panic!("missing Content-Type for {target}: {headers}"));
        Reply {
            status,
            mime,
            body: bytes[split + 4..].to_vec(),
        }
    }

    #[test]
    fn raw_parent_requests_cannot_read_owned_sentinels_outside_dist() {
        let f = Fixture::new();
        let server = f.server();
        for target in [
            "/../outside-fixture.txt",
            "/assets/../../outside-fixture.txt",
            "/../dist-sibling/sentinel.txt",
            "/assets/../index.html",
        ] {
            let reply = get(&server, target);
            assert_eq!(reply.status, 404, "{target}: {:?}", reply.body);
            assert!(reply.body.is_empty(), "{target}");
        }
    }

    #[test]
    fn encoded_and_platform_ambiguous_request_paths_are_not_served() {
        let f = Fixture::new();
        let server = f.server();
        for target in [
            "/%2e%2e/outside-fixture.txt",
            "/.%2E/outside-fixture.txt",
            "/assets/%2E%2e/%2e%2e/outside-fixture.txt",
            "/%252e%252e/outside-fixture.txt",
            "/%2f../outside-fixture.txt",
            "/%5c..%5coutside-fixture.txt",
            "/..\\outside-fixture.txt",
            "//../outside-fixture.txt",
            "///../outside-fixture.txt",
            "/C:/outside-fixture.txt",
            "/C:outside-fixture.txt",
            "/assets/safe.js:stream",
            "/assets/..%20/outside-fixture.txt",
            "/assets/.../outside-fixture.txt",
            "/./index.html",
            "//index.html",
            "/%00index.html",
            "/%0aindex.html",
            "/%GG",
            "/%2",
            "/%ff",
            "http://example.invalid/../outside-fixture.txt",
            "C:/outside-fixture.txt",
        ] {
            let reply = get(&server, target);
            assert_eq!(reply.status, 404, "{target}: {:?}", reply.body);
            assert!(reply.body.is_empty(), "{target}");
        }
    }

    #[test]
    fn safe_index_nested_assets_queries_mime_and_request_logging_are_preserved() {
        let f = Fixture::new();
        let server = f.server();
        for target in ["/", "/index.html?cache=../outside-fixture.txt"] {
            let reply = get(&server, target);
            assert_eq!(reply.status, 200);
            assert_eq!(reply.body, b"fixture index");
            assert_eq!(reply.mime, "text/html; charset=utf-8");
        }
        for (extension, mime) in [
            ("js", "text/javascript"),
            ("wasm", "application/wasm"),
            ("css", "text/css"),
            ("png", "image/png"),
            ("ttf", "font/ttf"),
            ("json", "application/json"),
            ("svg", "image/svg+xml"),
            ("ico", "image/x-icon"),
            ("bin", "application/octet-stream"),
        ] {
            let relative = format!("assets/nested/safe.{extension}");
            std::fs::write(f.dist.join(&relative), b"owned safe asset").unwrap();
            let reply = get(&server, &format!("/{relative}?v=%GG"));
            assert_eq!(reply.status, 200);
            assert_eq!(reply.body, b"owned safe asset");
            assert_eq!(reply.mime, mime);
        }
        let missing = get(&server, "/assets/missing.png?cache=1");
        assert_eq!(missing.status, 404);
        assert!(missing.body.is_empty());
        assert_eq!(missing.mime, "image/png");
        let log = server.request_log();
        assert_eq!(log[0], "200 / (13 bytes)");
        assert_eq!(log[1], "200 /index.html (13 bytes)");
        assert_eq!(log.last().unwrap(), "404 /assets/missing.png (0 bytes)");
        assert!(log.iter().all(|line| !line.contains('?')));
    }

    #[test]
    fn encoded_safe_names_are_decoded_once_and_directories_remain_unreadable() {
        let f = Fixture::new();
        let server = f.server();
        std::fs::write(f.dist.join("assets/safe name.txt"), "space asset").unwrap();
        std::fs::write(f.dist.join("assets/șorț.txt"), "unicode asset").unwrap();
        std::fs::create_dir(f.dist.join("assets/%2e%2e")).unwrap();
        std::fs::write(
            f.dist.join("assets/%2e%2e/safe.txt"),
            "literal percent asset",
        )
        .unwrap();
        for (target, body) in [
            ("/assets/safe%20name.txt", "space asset"),
            ("/assets/%C8%99or%C8%9B.txt", "unicode asset"),
            ("/assets/%252e%252e/safe.txt", "literal percent asset"),
        ] {
            let reply = get(&server, target);
            assert_eq!(reply.status, 200);
            assert_eq!(reply.body, body.as_bytes());
        }
        let directory = get(&server, "/assets");
        assert_eq!(directory.status, 404);
        assert!(directory.body.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn canonical_containment_rejects_file_directory_and_chained_symlink_escapes() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        std::fs::write(f.dist.join("assets/safe.bin"), "safe script").unwrap();
        symlink(
            f.base.join("outside-fixture.txt"),
            f.dist.join("file-link.txt"),
        )
        .unwrap();
        symlink(f.base.join("dist-sibling"), f.dist.join("escape")).unwrap();
        symlink(f.dist.join("escape"), f.dist.join("assets/chain")).unwrap();
        symlink(
            f.dist.join("assets/safe.bin"),
            f.dist.join("inside-link.js"),
        )
        .unwrap();
        symlink(&f.dist, f.base.join("root-link")).unwrap();
        let server = StaticServer::start(f.base.join("root-link")).unwrap();
        for target in [
            "/file-link.txt",
            "/escape/sentinel.txt",
            "/%65scape/sentinel.txt",
            "/assets/chain/sentinel.txt",
        ] {
            let reply = get(&server, target);
            assert_eq!(reply.status, 404, "{target}: {:?}", reply.body);
            assert!(reply.body.is_empty());
        }
        let safe = get(&server, "/inside-link.js");
        assert_eq!(safe.status, 200);
        assert_eq!(safe.body, b"safe script");
        assert_eq!(safe.mime, "text/javascript");
    }
}
