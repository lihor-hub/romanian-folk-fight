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
    let relative = if url_path == "/" {
        "index.html"
    } else {
        url_path.trim_start_matches('/')
    };
    let file_path = dist_dir.join(relative);

    let (status, body) = match std::fs::read(&file_path) {
        Ok(bytes) => (200u16, bytes),
        Err(_) => (404u16, Vec::new()),
    };
    log.lock()
        .unwrap()
        .push(format!("{status} {url_path} ({} bytes)", body.len()));

    let mut response = tiny_http::Response::from_data(body).with_status_code(status);
    if let Ok(header) =
        tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type(&file_path).as_bytes())
    {
        response = response.with_header(header);
    }
    let _ = request.respond(response);
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
