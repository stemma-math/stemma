//! The local server of the read-back page. It serves the page, and writes the
//! person's marks and notes to `.stemma/reviews.json`, and nowhere else.
//!
//! It listens on 127.0.0.1 only. Other pages open in the browser must not be
//! able to write through it, so every write carries a random token that only
//! the served page knows, and requests that name another host or come from
//! another origin are refused.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Result, bail};
use serde_json::json;

use super::reviews::{Change, Reviews};
use super::{Entry, page};

/// The header that carries the session's token.
const TOKEN_HEADER: &str = "x-stemma-token";
/// The largest request the server reads.
const HEADERS_LIMIT: usize = 16 * 1024;
const BODY_LIMIT: usize = 64 * 1024;

/// The page's server, for one session of `stemma readback`.
pub(super) struct Server {
    library: PathBuf,
    title: String,
    entries: Vec<Entry>,
    port: u16,
    token: String,
    /// Held while the reviews are read, changed and written, so that two
    /// writes at once cannot lose a change.
    writes: Mutex<()>,
}

/// A request, as much of it as the server needs.
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A response.
struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn json(status: u16, value: serde_json::Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: value.to_string().into_bytes(),
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self::json(status, json!({ "error": message }))
    }

    fn write(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Content Too Large",
            415 => "Unsupported Media Type",
            _ => "Internal Server Error",
        };
        write!(
            stream,
            "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n\
X-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; \
script-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
            self.status,
            self.content_type,
            self.body.len()
        )?;
        stream.write_all(&self.body)
    }
}

/// A random token, as 32 hexadecimal digits.
fn random_token() -> String {
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if read.is_err() {
        // Without /dev/urandom: the standard library's hash keys, which it
        // draws from the operating system's randomness.
        use std::hash::{BuildHasher, Hasher};
        let state = std::collections::hash_map::RandomState::new();
        for (i, chunk) in bytes.chunks_mut(8).enumerate() {
            let mut hasher = state.build_hasher();
            hasher.write_usize(i);
            chunk.copy_from_slice(&hasher.finish().to_le_bytes());
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares two strings in a time that does not depend on where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Reads one request from a stream.
fn read_request(stream: &TcpStream) -> Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut read = reader
        .by_ref()
        .take(HEADERS_LIMIT as u64)
        .read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        bail!("no request line");
    };
    let (method, path) = (
        method.to_string(),
        target.split(['?', '#']).next().unwrap_or("/").to_string(),
    );
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        let n = reader
            .by_ref()
            .take((HEADERS_LIMIT - read.min(HEADERS_LIMIT)) as u64)
            .read_line(&mut line)?;
        read += n;
        if n == 0 || read >= HEADERS_LIMIT {
            bail!("the headers are too long, or cut short");
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let length = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .map_or(Ok(0), |(_, v)| v.parse::<usize>())?;
    if length > BODY_LIMIT {
        bail!("the body is too long");
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

impl Server {
    pub(super) fn new(library: PathBuf, title: String, entries: Vec<Entry>, port: u16) -> Self {
        Self {
            library,
            title,
            entries,
            port,
            token: random_token(),
            writes: Mutex::new(()),
        }
    }

    /// Answers requests until the process is stopped, each connection on a
    /// thread of its own, so that an idle connection (a browser's preconnect)
    /// holds up no other.
    pub(super) fn run(&self, listener: TcpListener) {
        std::thread::scope(|scope| {
            for stream in listener.incoming().flatten() {
                scope.spawn(move || {
                    let _ = self.handle(stream);
                });
            }
        });
    }

    fn handle(&self, mut stream: TcpStream) -> Result<()> {
        // A browser may open a connection and send nothing on it.
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let response = match read_request(&stream) {
            Ok(request) => self.answer(&request),
            Err(_) => Response::error(400, "a malformed request"),
        };
        response.write(&mut stream)?;
        Ok(())
    }

    /// Whether a value of the `Host` header names this server, so that pages
    /// of other sites that resolve to 127.0.0.1 cannot read the page.
    fn is_this_host(&self, host: Option<&str>) -> bool {
        let port = self.port;
        host.is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"))
    }

    fn answer(&self, request: &Request) -> Response {
        if !self.is_this_host(request.header("host")) {
            return Response::error(403, "this server answers only requests for itself");
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/" | "/index.html") => self.page(),
            ("POST", "/api/review") => self.review(request),
            (_, "/" | "/index.html" | "/api/review") => Response::error(405, "method not allowed"),
            _ => Response::error(404, "not found"),
        }
    }

    /// The page, with the person's marks as they are now.
    fn page(&self) -> Response {
        let page = Reviews::load(&self.library).and_then(|reviews| {
            page::render(&self.title, &self.entries, &reviews, Some(&self.token))
        });
        match page {
            Ok(html) => Response {
                status: 200,
                content_type: "text/html; charset=utf-8",
                body: html.into_bytes(),
            },
            Err(e) => Response::error(500, &format!("{e:#}")),
        }
    }

    /// Records a mark, a note or an archiving, from the page.
    fn review(&self, request: &Request) -> Response {
        if !request
            .header(TOKEN_HEADER)
            .is_some_and(|t| same(t, &self.token))
        {
            return Response::error(403, "a missing or wrong token");
        }
        let port = self.port;
        if let Some(origin) = request.header("origin")
            && origin != format!("http://127.0.0.1:{port}")
            && origin != format!("http://localhost:{port}")
        {
            return Response::error(403, "a request from another origin");
        }
        if !request
            .header("content-type")
            .is_some_and(|t| t.starts_with("application/json"))
        {
            return Response::error(415, "send JSON");
        }
        let Ok(change) = serde_json::from_slice::<Change>(&request.body) else {
            return Response::error(400, "a malformed change");
        };
        // The label only names a read-back of this session: it never makes a path.
        let Some(entry) = self.entries.iter().find(|e| e.label() == change.label) else {
            return Response::error(404, "no such read-back");
        };
        let formal = &entry.readback.formal;
        let _write = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        let recorded = Reviews::load(&self.library).and_then(|mut reviews| {
            let review = reviews.apply(&change, formal)?;
            reviews.save(&self.library)?;
            Ok((review, entry.state(&reviews)))
        });
        match recorded {
            Ok((review, state)) => Response::json(
                200,
                json!({
                    "label": change.label, "mark": review.mark, "note": review.note,
                    "outdated": review.outdated(formal), "state": state,
                }),
            ),
            Err(e) => Response::error(400, &format!("{e:#}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Readback, State};
    use super::*;

    fn library(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stemma-server-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn server(dir: &std::path::Path) -> Server {
        let readback = Readback {
            label: "even-add".into(),
            display: "Theorem".into(),
            module: "Alg.Even".into(),
            formal: "sha256:a".into(),
            prose: "The sum of even numbers is even.".into(),
            lean: "theorem even_add …".into(),
            readback: "For all naturals …".into(),
            agent: "claude".into(),
        };
        let entry = Entry {
            readback,
            freshness: State::Current,
            gone: false,
        };
        Server::new(dir.to_path_buf(), "Algebra".into(), vec![entry], 8001)
    }

    fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
        Request {
            method: method.into(),
            path: path.into(),
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    const HOST: (&str, &str) = ("host", "127.0.0.1:8001");
    const JSON: (&str, &str) = ("content-type", "application/json");

    #[test]
    fn the_page_carries_the_token() {
        let dir = library("page");
        let s = server(&dir);
        let r = s.answer(&request("GET", "/", &[HOST], ""));
        assert_eq!(r.status, 200);
        let html = String::from_utf8(r.body).unwrap();
        assert!(html.contains(&format!("content=\"{}\"", s.token)));
        assert!(html.contains("id=\"even-add\""));
        // Pages of other sites that resolve to 127.0.0.1 get nothing.
        let r = s.answer(&request("GET", "/", &[("host", "evil.example:8001")], ""));
        assert_eq!(r.status, 403);
        assert_eq!(s.answer(&request("GET", "/", &[], "")).status, 403);
        let r = s.answer(&request("GET", "/even-add.json", &[HOST], ""));
        assert_eq!(r.status, 404);
    }

    #[test]
    fn writes_need_the_token() {
        let dir = library("token");
        let s = server(&dir);
        let body = r#"{"label":"even-add","mark":"approved"}"#;
        let token = s.token.clone();
        for headers in [
            vec![HOST, JSON],
            vec![
                HOST,
                JSON,
                (TOKEN_HEADER, "0123456789abcdef0123456789abcdef"),
            ],
            vec![HOST, JSON, (TOKEN_HEADER, "")],
            vec![
                HOST,
                JSON,
                (TOKEN_HEADER, token.as_str()),
                ("origin", "https://evil.example"),
            ],
            vec![("host", "evil.example:8001"), JSON, (TOKEN_HEADER, &token)],
        ] {
            let r = s.answer(&request("POST", "/api/review", &headers, body));
            assert_eq!(r.status, 403, "{headers:?}");
        }
        assert!(!super::super::reviews::path(&dir).exists());
        // A preflight from another page is not answered either.
        let r = s.answer(&request("OPTIONS", "/api/review", &[HOST], ""));
        assert_eq!(r.status, 405);
        let r = s.answer(&request(
            "POST",
            "/api/review",
            &[
                HOST,
                JSON,
                (TOKEN_HEADER, &token),
                ("origin", "http://127.0.0.1:8001"),
            ],
            body,
        ));
        assert_eq!(r.status, 200);
        let answer: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(answer["mark"], "approved");
        assert_eq!(answer["state"], "current");
        let saved = Reviews::load(&dir).unwrap();
        assert_eq!(saved.get("even-add").formal, "sha256:a");
    }

    #[test]
    fn writes_go_only_to_the_reviews() {
        let dir = library("paths");
        let s = server(&dir);
        let token = s.token.clone();
        let headers = [HOST, JSON, (TOKEN_HEADER, token.as_str())];
        for label in ["../../etc/x", "missing", ""] {
            let body = json!({ "label": label, "mark": "read" }).to_string();
            let r = s.answer(&request("POST", "/api/review", &headers, &body));
            assert_eq!(r.status, 404, "{label}");
        }
        let r = s.answer(&request(
            "POST",
            "/api/review",
            &headers,
            r#"{"label":"even-add","path":"x"}"#,
        ));
        assert_eq!(r.status, 400);
        let r = s.answer(&request(
            "POST",
            "/api/review",
            &[HOST, (TOKEN_HEADER, token.as_str())],
            r#"{"label":"even-add"}"#,
        ));
        assert_eq!(r.status, 415);
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert!(entries.is_empty());
        let body = r#"{"label":"even-add","archived":true}"#;
        let r = s.answer(&request("POST", "/api/review", &headers, body));
        assert_eq!(r.status, 200);
        let answer: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(answer["state"], "archived");
        let names: Vec<_> = std::fs::read_dir(dir.join(".stemma"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["reviews.json"]);
    }

    #[test]
    fn serves_over_a_socket() {
        let dir = library("socket");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut s = server(&dir);
        s.port = port;
        let token = s.token.clone();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                s.handle(stream.unwrap()).unwrap();
            }
        });
        let send = |raw: String| {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(raw.as_bytes()).unwrap();
            let mut out = String::new();
            stream.read_to_string(&mut out).unwrap();
            out
        };
        let body = r#"{"label":"even-add","note":"Is n allowed to be 0?"}"#;
        let without = send(format!(
            "POST /api/review HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        assert!(without.starts_with("HTTP/1.1 403"), "{without}");
        let with = send(format!(
            "POST /api/review HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Stemma-Token: {token}\r\n\
Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        assert!(with.starts_with("HTTP/1.1 200"), "{with}");
        handle.join().unwrap();
        assert_eq!(
            Reviews::load(&dir).unwrap().get("even-add").note,
            "Is n allowed to be 0?"
        );
    }

    #[test]
    fn concurrent_writes_lose_nothing() {
        let dir = library("concurrent");
        let mut s = server(&dir);
        let labels: Vec<String> = (0..16).map(|i| format!("label-{i}")).collect();
        s.entries = labels
            .iter()
            .map(|label| {
                let mut readback = s.entries[0].readback.clone();
                readback.label = label.clone();
                Entry {
                    readback,
                    freshness: State::Current,
                    gone: false,
                }
            })
            .collect();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        s.port = port;
        let token = s.token.clone();
        // An idle connection, as a browser's preconnect, holds up no other.
        let _idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
        std::thread::spawn(move || s.run(listener));
        std::thread::scope(|scope| {
            for label in &labels {
                let token = &token;
                scope.spawn(move || {
                    let body = json!({ "label": label, "mark": "read" }).to_string();
                    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                    write!(
                        stream,
                        "POST /api/review HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
X-Stemma-Token: {token}\r\nContent-Type: application/json\r\n\
Content-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                    let mut out = String::new();
                    stream.read_to_string(&mut out).unwrap();
                    assert!(out.starts_with("HTTP/1.1 200"), "{out}");
                });
            }
        });
        let saved = Reviews::load(&dir).unwrap();
        for label in &labels {
            assert_eq!(
                saved.get(label).mark,
                super::super::reviews::Mark::Read,
                "{label}"
            );
        }
    }

    #[test]
    fn tokens_are_random() {
        let (a, b) = (random_token(), random_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(same(&a, &a.clone()) && !same(&a, &b) && !same(&a, ""));
    }
}
