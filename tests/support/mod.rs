//! Shared helpers for the pluggable-backend acceptance tests
//! (`tests/backend_ac*.rs`). Lives in a `support/` subdirectory so cargo does
//! not compile it as its own integration-test binary (the `tests/common`
//! convention).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::missing_panics_doc,
    dead_code,
    unreachable_pub
)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Path to `tests/stubs/codex_stub.sh`, relative to the crate root.
pub fn codex_stub_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/stubs/codex_stub.sh")
}

/// Path to `tests/stubs/claude_stub.sh`, relative to the crate root.
pub fn claude_stub_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/stubs/claude_stub.sh")
}

/// A path that resolves to nothing, for forcing a backend unavailable.
pub fn missing_bin() -> PathBuf {
    PathBuf::from("/nonexistent/ac-judge-test-bin-does-not-exist")
}

/// A minimal single-request-at-a-time HTTP/1.1 server standing in for
/// `api.anthropic.com`. Every request gets the same canned Anthropic
/// Messages response: `{"content":[{"type":"text","text":"<verdict_json>"}]}`.
///
/// Records whether an `x-api-key` header was seen on any request, and how
/// many requests were served, for AC2 / AC6-style assertions.
pub struct StubApiServer {
    addr: SocketAddr,
    saw_api_key: Arc<AtomicBool>,
    request_count: Arc<AtomicUsize>,
}

impl StubApiServer {
    /// Start the server on an OS-assigned loopback port, replying with
    /// `verdict_json` as the message text on every request.
    #[must_use]
    pub fn start(verdict_json: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub listener");
        let addr = listener.local_addr().expect("stub listener local addr");
        let saw_api_key = Arc::new(AtomicBool::new(false));
        let request_count = Arc::new(AtomicUsize::new(0));
        let verdict_json = verdict_json.to_owned();
        let saw_api_key_bg = Arc::clone(&saw_api_key);
        let request_count_bg = Arc::clone(&request_count);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                serve_one(
                    &mut stream,
                    &verdict_json,
                    &saw_api_key_bg,
                    &request_count_bg,
                );
            }
        });
        Self {
            addr,
            saw_api_key,
            request_count,
        }
    }

    /// The `--api-endpoint`-style URL to point `AC_JUDGE_API_ENDPOINT` at.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("http://{}/v1/messages", self.addr)
    }

    /// Whether any served request carried an `x-api-key` header.
    #[must_use]
    pub fn saw_api_key(&self) -> bool {
        self.saw_api_key.load(Ordering::SeqCst)
    }

    /// How many requests have been served so far.
    #[must_use]
    pub fn request_count(&self) -> usize {
        self.request_count.load(Ordering::SeqCst)
    }
}

fn serve_one(
    stream: &mut TcpStream,
    verdict_json: &str,
    saw_api_key: &Arc<AtomicBool>,
    request_count: &Arc<AtomicUsize>,
) {
    let mut buf = [0_u8; 8192];
    // One read is enough for our small test request bodies; on loopback the
    // whole HTTP request arrives in a single TCP segment in practice.
    let n = stream.read(&mut buf).unwrap_or(0);
    let text = String::from_utf8_lossy(&buf[..n]);
    if text.to_ascii_lowercase().contains("x-api-key:") {
        saw_api_key.store(true, Ordering::SeqCst);
    }
    request_count.fetch_add(1, Ordering::SeqCst);

    let escaped_text = serde_json::to_string(verdict_json).unwrap_or_else(|_| "\"\"".to_owned());
    let body = format!(r#"{{"content":[{{"type":"text","text":{escaped_text}}}]}}"#);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// The default canned passing verdict JSON string used across these tests.
pub const PASSING_VERDICT: &str = r#"{"behavior_match":"yes","assertion_kind":"asserts-invariant","confidence":0.9,"reasoning":"stub verdict"}"#;

/// Write a one-AC PRD (`- **AC1**: <text>.`) and a matching `tests/ac1_*.rs`
/// fixture under a fresh temp crate root, returning the temp dir (keep it
/// alive for the duration of the test) and the PRD path.
#[must_use]
pub fn one_ac_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("tests")).expect("mkdir tests");
    std::fs::write(
        root.join("tests/ac1_basic.rs"),
        "#[test]\nfn ac1_x() { assert!(true); }\n",
    )
    .expect("write fixture test");
    let prd = root.join("PRD.md");
    std::fs::write(
        &prd,
        "## Acceptance criteria\n\n- **AC1**: the thing happens.\n",
    )
    .expect("write PRD");
    (dir, prd)
}
