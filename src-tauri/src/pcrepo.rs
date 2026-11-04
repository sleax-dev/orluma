//! "This PC" as a destination: a restic REST server inside the app, on 127.0.0.1 only, reached by a server
//! through the reverse SSH tunnel. The server's agent runs `restic copy` into it; nothing on the PC needs
//! restic, and nothing listens on the network.
//!
//! The protocol is restic's REST backend (v1 and v2). Every file but `config` is named by the SHA-256 of
//! its content, so a chunk that arrives damaged is refused instead of stored.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const TYPES: &[&str] = &["data", "keys", "locks", "snapshots", "index"];

pub struct Server {
    pub addr: SocketAddr,
    pub user: String,
    pub password: String,
}

struct Ctx {
    root: PathBuf,
    auth: String,
}

pub async fn start(root: PathBuf) -> std::io::Result<Server> {
    tokio::fs::create_dir_all(&root).await?;
    let user = "orluma".to_string();
    let password = crate::store::random_token(32);
    use base64::Engine;
    let auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    );
    let ctx = Arc::new(Ctx { root, auth });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let app = axum::Router::new().fallback(handle).with_state(ctx);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(Server { addr, user, password })
}

fn status(code: StatusCode) -> Response {
    code.into_response()
}

fn valid_repo(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn valid_name(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn file_path(repo: &Path, kind: &str, name: &str) -> PathBuf {
    if kind == "data" {
        repo.join("data").join(&name[..2]).join(name)
    } else {
        repo.join(kind).join(name)
    }
}

async fn handle(State(ctx): State<Arc<Ctx>>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    if parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) != Some(ctx.auth.as_str()) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"orluma\"")],
        )
            .into_response();
    }
    let path = parts.uri.path().trim_matches('/').to_string();
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let Some(repo_name) = segs.first().copied().filter(|r| valid_repo(r)) else {
        return status(StatusCode::NOT_FOUND);
    };
    let repo = ctx.root.join(repo_name);
    let method = parts.method.clone();
    let query = parts.uri.query().unwrap_or("");
    match (segs.len(), &method) {
        (1, &Method::POST) if query.contains("create=true") => create(&repo).await,
        (2, m) if segs[1] == "config" => match *m {
            Method::GET | Method::HEAD => read_file(&repo.join("config"), &parts.headers, m == Method::HEAD).await,
            Method::POST => write_file(&repo.join("config"), body, None).await,
            Method::DELETE => remove(&repo.join("config")).await,
            _ => status(StatusCode::METHOD_NOT_ALLOWED),
        },
        (2, &Method::GET) if TYPES.contains(&segs[1]) => list(&repo, segs[1], &parts.headers).await,
        (3, m) if TYPES.contains(&segs[1]) && valid_name(segs[2]) => {
            let p = file_path(&repo, segs[1], segs[2]);
            match *m {
                Method::GET | Method::HEAD => read_file(&p, &parts.headers, m == Method::HEAD).await,
                Method::POST => write_file(&p, body, Some(segs[2])).await,
                Method::DELETE => remove(&p).await,
                _ => status(StatusCode::METHOD_NOT_ALLOWED),
            }
        }
        _ => status(StatusCode::NOT_FOUND),
    }
}

async fn create(repo: &Path) -> Response {
    for t in TYPES {
        if tokio::fs::create_dir_all(repo.join(t)).await.is_err() {
            return status(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    for i in 0..=255u8 {
        let _ = tokio::fs::create_dir_all(repo.join("data").join(format!("{i:02x}"))).await;
    }
    status(StatusCode::OK)
}

async fn list(repo: &Path, kind: &str, headers: &HeaderMap) -> Response {
    let v2 = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/vnd.x.restic.rest.v2"));
    let dir = repo.join(kind);
    let mut entries = Vec::new();
    let mut dirs = vec![dir.clone()];
    if kind == "data" {
        dirs.clear();
        if let Ok(mut rd) = tokio::fs::read_dir(&dir).await {
            while let Ok(Some(e)) = rd.next_entry().await {
                dirs.push(e.path());
            }
        }
    }
    for d in dirs {
        let Ok(mut rd) = tokio::fs::read_dir(&d).await else {
            continue;
        };
        while let Ok(Some(e)) = rd.next_entry().await {
            let name = e.file_name().to_string_lossy().into_owned();
            if !valid_name(&name) {
                continue;
            }
            let size = e.metadata().await.map(|m| m.len()).unwrap_or(0);
            entries.push((name, size));
        }
    }
    if !repo.join(kind).exists() {
        return status(StatusCode::NOT_FOUND);
    }
    if v2 {
        let body: Vec<_> = entries
            .into_iter()
            .map(|(name, size)| serde_json::json!({"name": name, "size": size}))
            .collect();
        (
            [(header::CONTENT_TYPE, "application/vnd.x.restic.rest.v2")],
            axum::Json(body),
        )
            .into_response()
    } else {
        axum::Json(entries.into_iter().map(|(n, _)| n).collect::<Vec<_>>()).into_response()
    }
}

fn range(headers: &HeaderMap, len: u64) -> Option<(u64, u64)> {
    let v = headers.get(header::RANGE)?.to_str().ok()?.strip_prefix("bytes=")?;
    let (a, b) = v.split_once('-')?;
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() {
        len.saturating_sub(1)
    } else {
        b.parse::<u64>().ok()?.min(len.saturating_sub(1))
    };
    (start <= end && start < len).then_some((start, end))
}

async fn read_file(p: &Path, headers: &HeaderMap, head: bool) -> Response {
    let Ok(mut f) = tokio::fs::File::open(p).await else {
        return status(StatusCode::NOT_FOUND);
    };
    let len = f.metadata().await.map(|m| m.len()).unwrap_or(0);
    if head {
        return ([(header::CONTENT_LENGTH, len.to_string())], ()).into_response();
    }
    match range(headers, len) {
        Some((start, end)) => {
            let n = end - start + 1;
            if f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                return status(StatusCode::INTERNAL_SERVER_ERROR);
            }
            let mut buf = vec![0u8; n as usize];
            if f.read_exact(&mut buf).await.is_err() {
                return status(StatusCode::INTERNAL_SERVER_ERROR);
            }
            (
                StatusCode::PARTIAL_CONTENT,
                [(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))],
                buf,
            )
                .into_response()
        }
        None => {
            let stream = tokio_util::io::ReaderStream::new(f);
            ([(header::CONTENT_LENGTH, len.to_string())], Body::from_stream(stream)).into_response()
        }
    }
}

/// Writes through a temporary file; with `expect`, the content must hash to that name.
async fn write_file(p: &Path, body: Body, expect: Option<&str>) -> Response {
    if tokio::fs::try_exists(p).await.unwrap_or(false) {
        return status(StatusCode::FORBIDDEN);
    }
    if let Some(parent) = p.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let tmp = p.with_extension(format!("tmp-{}", crate::store::random_token(6)));
    let Ok(mut f) = tokio::fs::File::create(&tmp).await else {
        return status(StatusCode::INTERNAL_SERVER_ERROR);
    };
    use sha2_lite::Sha256;
    let mut hash = Sha256::new();
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            let _ = tokio::fs::remove_file(&tmp).await;
            return status(StatusCode::BAD_REQUEST);
        };
        hash.update(&chunk);
        if f.write_all(&chunk).await.is_err() {
            let _ = tokio::fs::remove_file(&tmp).await;
            return status(StatusCode::INSUFFICIENT_STORAGE);
        }
    }
    if f.sync_all().await.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
        return status(StatusCode::INTERNAL_SERVER_ERROR);
    }
    drop(f);
    if let Some(name) = expect
        && hash.hex() != name.to_ascii_lowercase()
    {
        let _ = tokio::fs::remove_file(&tmp).await;
        return status(StatusCode::BAD_REQUEST);
    }
    match tokio::fs::rename(&tmp, p).await {
        Ok(()) => status(StatusCode::OK),
        Err(_) => status(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn remove(p: &Path) -> Response {
    match tokio::fs::remove_file(p).await {
        Ok(()) => status(StatusCode::OK),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

/// Size of everything stored under the PC root, per server repository.
pub fn usage(root: &Path) -> Vec<(String, u64)> {
    fn walk(p: &Path) -> u64 {
        std::fs::read_dir(p)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| match e.file_type() {
                        Ok(t) if t.is_dir() => walk(&e.path()),
                        _ => e.metadata().map(|m| m.len()).unwrap_or(0),
                    })
                    .sum()
            })
            .unwrap_or(0)
    }
    std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .map(|e| (e.file_name().to_string_lossy().into_owned(), walk(&e.path())))
                .collect()
        })
        .unwrap_or_default()
}

/// SHA-256 without another dependency tree: the `sha2` crate is already in the build through russh.
mod sha2_lite {
    use sha2::Digest;
    pub struct Sha256(sha2::Sha256);
    impl Sha256 {
        pub fn new() -> Self {
            Self(sha2::Sha256::new())
        }
        pub fn update(&mut self, b: &[u8]) {
            self.0.update(b)
        }
        pub fn hex(self) -> String {
            self.0.finalize().iter().map(|b| format!("{b:02x}")).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real restic against this server: init, backup, full read-back check, restore, forget and prune.
    /// Skipped where restic is not installed (CI); run locally with restic on PATH.
    #[tokio::test(flavor = "multi_thread")]
    async fn restic_works_against_it() {
        if std::process::Command::new("restic").arg("version").output().is_err() {
            eprintln!("restic not installed: skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("orluma-pcrepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        let big: Vec<u8> = (0..3_000_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        std::fs::write(src.join("sub/big.bin"), &big).unwrap();
        let s = start(dir.join("repos")).await.unwrap();
        let url = format!("rest:http://{}:{}@{}/srv1/", s.user, s.password, s.addr);
        let cache = dir.join("cache");
        let cleanup = dir.clone();
        tokio::task::spawn_blocking(move || {
            let restic = |args: &[&str]| {
                let out = std::process::Command::new("restic")
                    .args(args)
                    .env("RESTIC_REPOSITORY", &url)
                    .env("RESTIC_PASSWORD", "pw")
                    .env("RESTIC_CACHE_DIR", &cache)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "restic {args:?}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                String::from_utf8_lossy(&out.stdout).into_owned()
            };
            let run = |args: Vec<String>| {
                let a: Vec<&str> = args.iter().map(String::as_str).collect();
                restic(&a)
            };
            let srcs = src.display().to_string();
            let out = dir.join("out").display().to_string();
            run(vec!["init".into()]);
            run(vec!["backup".into(), srcs.clone()]);
            std::fs::write(format!("{srcs}/a.txt"), "changed").unwrap();
            run(vec!["backup".into(), srcs.clone()]);
            run(vec!["check".into(), "--read-data".into()]);
            run(vec!["restore".into(), "latest".into(), "--target".into(), out.clone()]);
            assert_eq!(std::fs::read(format!("{out}{srcs}/sub/big.bin")).unwrap(), big);
            assert_eq!(
                std::fs::read_to_string(format!("{out}{srcs}/a.txt")).unwrap(),
                "changed"
            );
            run(vec![
                "forget".into(),
                "--keep-last".into(),
                "1".into(),
                "--prune".into(),
            ]);
            run(vec!["check".into(), "--read-data".into()]);
            let n = run(vec!["snapshots".into(), "--json".into()]);
            assert_eq!(n.matches("\"short_id\"").count(), 1, "{n}");
        })
        .await
        .unwrap();

        // no password, no entry; a blob that does not match its name is refused
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::new();
        let base = format!("http://{}/srv1", s.addr);
        let r = http.get(format!("{base}/config")).send().await.unwrap();
        assert_eq!(r.status(), 401);
        let r = http
            .post(format!("{base}/data/{}", "0".repeat(64)))
            .basic_auth(&s.user, Some(&s.password))
            .body("not what the name says")
            .send()
            .await
            .unwrap();
        assert!(r.status().is_client_error(), "{}", r.status());
        let r = http
            .get(format!("http://{}/../etc/config", s.addr))
            .basic_auth(&s.user, Some(&s.password))
            .send()
            .await
            .unwrap();
        assert!(!r.status().is_success());
        let _ = std::fs::remove_dir_all(&cleanup);
    }
}
