//! Moving backups from one PC to another on the same network, no server and no cloud in between.
//!
//! The sending PC listens on a random port and advertises itself over mDNS; the receiving PC finds it, the
//! user types the code the sender shows, and the repository files are copied across. The code is short, so it
//! never travels and is never checked directly: both sides run SPAKE2 on it, which leaves an eavesdropper with
//! nothing to guess offline, and only after that agree on a key. Everything after is encrypted and
//! authenticated with that key (ChaCha20-Poly1305 frames, HMAC on requests). Three wrong codes close the offer.
//!
//! What moves are restic repository files, already encrypted by restic; the repository password goes along
//! inside the encrypted channel, so the other PC can read what it received. Every file but `config` is named by
//! the SHA-256 of its content, so a damaged or swapped file is refused.

use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use futures_util::StreamExt;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SERVICE: &str = "_orluma-share._tcp.local.";
const FRAME: usize = 1 << 20;
const MAX_FAILS: u32 = 3;

type Res<T> = Result<T, String>;

// ---------------------------------------------------------------- crypto

#[derive(Clone)]
struct Keys {
    enc: [u8; 32],
    mac: [u8; 32],
}

fn derive(shared: &[u8]) -> Keys {
    let h = Hkdf::<Sha256>::new(Some(b"orluma-lan-v1"), shared);
    let (mut enc, mut mac) = ([0u8; 32], [0u8; 32]);
    h.expand(b"enc", &mut enc).expect("32 bytes");
    h.expand(b"mac", &mut mac).expect("32 bytes");
    Keys { enc, mac }
}

fn hmac(k: &Keys) -> Hmac<Sha256> {
    <Hmac<Sha256> as Mac>::new_from_slice(&k.mac).expect("any key length")
}

fn tag(k: &Keys, label: &str, data: &str) -> Vec<u8> {
    let mut m = hmac(k);
    m.update(label.as_bytes());
    m.update(b"\0");
    m.update(data.as_bytes());
    m.finalize().into_bytes().to_vec()
}

fn tag_ok(k: &Keys, label: &str, data: &str, got: &[u8]) -> bool {
    let mut m = hmac(k);
    m.update(label.as_bytes());
    m.update(b"\0");
    m.update(data.as_bytes());
    m.verify_slice(got).is_ok()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    s.len()
        .is_multiple_of(2)
        .then(|| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
                .collect()
        })
        .flatten()
}

/// One encrypted frame: nonce (12) | length (4, big endian) | ciphertext. `aad` ties it to its request.
fn seal(k: &Keys, counter: u64, aad: &str, plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    let ct = ChaCha20Poly1305::new((&k.enc).into())
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: aad.as_bytes(),
            },
        )
        .expect("encrypting in memory");
    let mut out = Vec::with_capacity(16 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&(ct.len() as u32).to_be_bytes());
    out.extend_from_slice(&ct);
    out
}

/// Takes frames out of a byte stream as they complete.
struct Frames {
    buf: Vec<u8>,
    keys: Keys,
    aad: String,
}

impl Frames {
    fn push(&mut self, bytes: &[u8]) -> Res<Vec<Vec<u8>>> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            if self.buf.len() < 16 {
                break;
            }
            let len = u32::from_be_bytes([self.buf[12], self.buf[13], self.buf[14], self.buf[15]]) as usize;
            if len > FRAME + 64 {
                return Err("the other PC sent something that is not a backup frame".into());
            }
            if self.buf.len() < 16 + len {
                break;
            }
            let frame: Vec<u8> = self.buf.drain(..16 + len).collect();
            let plain = ChaCha20Poly1305::new((&self.keys.enc).into())
                .decrypt(
                    Nonce::from_slice(&frame[..12]),
                    Payload {
                        msg: &frame[16..],
                        aad: self.aad.as_bytes(),
                    },
                )
                .map_err(|_| "a frame failed its integrity check: the transfer was altered".to_string())?;
            out.push(plain);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- what is on offer

#[derive(Clone, Serialize, Deserialize)]
struct FileEntry {
    id: u32,
    rel: String,
    size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct RepoOffer {
    folder: String,
    password: String,
    files: Vec<FileEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    from: String,
    repos: Vec<RepoOffer>,
}

#[derive(Clone, Serialize, Default)]
pub struct Progress {
    /// scanning, waiting, receiving, done
    pub phase: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
}

fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf, u64)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(Result::ok) {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            if e.file_name() != "locks" {
                walk(base, &p, out);
            }
        } else if t.is_file() {
            let Ok(rel) = p.strip_prefix(base) else { continue };
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if !rel.ends_with(".tmp") && !rel.ends_with(".part") {
                out.push((rel, p.clone(), e.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
}

// ---------------------------------------------------------------- sending

enum Auth {
    Open,
    Pending(Keys),
    Ready(Keys),
}

struct Ctx {
    code: String,
    manifest: Vec<u8>,
    files: Vec<PathBuf>,
    total: u64,
    auth: Mutex<(Auth, u32)>,
    counter: AtomicU64,
    sent: AtomicU64,
    done: AtomicU64,
    on: Box<dyn Fn(Progress) + Send + Sync>,
    stop: tokio_util::sync::CancellationToken,
}

pub struct Share {
    pub code: String,
    pub port: u16,
    stop: tokio_util::sync::CancellationToken,
    daemon: Option<mdns_sd::ServiceDaemon>,
}

impl Share {
    pub fn stop(&self) {
        self.stop.cancel();
        if let Some(d) = &self.daemon {
            let _ = d.shutdown();
        }
    }
}

fn pairing_code() -> String {
    let mut b = [0u8; 4];
    rand::fill(&mut b);
    format!("{:08}", u32::from_be_bytes(b) % 100_000_000)
}

/// Starts offering `repos` (folder name, password) found under `root`.
pub async fn share(
    root: &Path,
    repos: Vec<(String, String)>,
    name: &str,
    on: impl Fn(Progress) + Send + Sync + 'static,
) -> Res<Share> {
    let (mut offers, mut files, mut total) = (Vec::new(), Vec::new(), 0u64);
    for (folder, password) in repos {
        let base = root.join(&folder);
        let mut found = Vec::new();
        walk(&base, &base, &mut found);
        if !found.iter().any(|(r, _, _)| r == "config") {
            return Err(format!("{folder} is not a backup folder (no repository config)"));
        }
        let mut list = Vec::new();
        for (rel, path, size) in found {
            list.push(FileEntry {
                id: files.len() as u32,
                rel,
                size,
            });
            files.push(path);
            total += size;
        }
        offers.push(RepoOffer {
            folder,
            password,
            files: list,
        });
    }
    if offers.is_empty() {
        return Err("choose something to send".into());
    }
    let manifest = serde_json::to_vec(&Manifest {
        from: name.to_string(),
        repos: offers,
    })
    .map_err(|e| e.to_string())?;
    let stop = tokio_util::sync::CancellationToken::new();
    let code = pairing_code();
    let ctx = Arc::new(Ctx {
        code: code.clone(),
        manifest,
        files,
        total,
        auth: Mutex::new((Auth::Open, 0)),
        counter: AtomicU64::new(1),
        sent: AtomicU64::new(0),
        done: AtomicU64::new(0),
        on: Box::new(on),
        stop: stop.clone(),
    });
    (ctx.on)(Progress {
        phase: "waiting".into(),
        bytes_total: total,
        files_total: ctx.files.len() as u64,
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let app = axum::Router::new()
        .route("/pair1", post(pair1))
        .route("/pair2", post(pair2))
        .route("/manifest", get(manifest_route))
        .route("/f/{id}", get(file_route))
        .with_state(ctx.clone());
    let s2 = stop.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move { s2.cancelled().await })
            .await;
    });
    // an offer left open is an open door: it closes itself
    let s3 = stop.clone();
    tokio::spawn(async move {
        tokio::select! {
            () = tokio::time::sleep(std::time::Duration::from_secs(30 * 60)) => s3.cancel(),
            () = s3.cancelled() => {}
        }
    });
    let daemon = advertise(name, port);
    Ok(Share {
        code,
        port,
        stop,
        daemon,
    })
}

fn advertise(name: &str, port: u16) -> Option<mdns_sd::ServiceDaemon> {
    let d = mdns_sd::ServiceDaemon::new().ok()?;
    let host = format!("{}.local.", name.replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
    let info = mdns_sd::ServiceInfo::new(SERVICE, name, &host, "", port, &[("v", "1")][..])
        .ok()?
        .enable_addr_auto();
    d.register(info).ok()?;
    Some(d)
}

fn bad(code: StatusCode, msg: &str) -> Response {
    (code, msg.to_string()).into_response()
}

async fn pair1(State(c): State<Arc<Ctx>>, body: axum::body::Bytes) -> Response {
    let mut g = c.auth.lock().unwrap_or_else(|e| e.into_inner());
    if g.1 >= MAX_FAILS {
        c.stop.cancel();
        return bad(StatusCode::TOO_MANY_REQUESTS, "offer closed after wrong codes");
    }
    if matches!(g.0, Auth::Ready(_)) {
        return bad(StatusCode::CONFLICT, "already paired");
    }
    let (st, msg) = Spake2::<Ed25519Group>::start_b(
        &Password::new(c.code.as_bytes()),
        &Identity::new(b"orluma-receiver"),
        &Identity::new(b"orluma-sender"),
    );
    match st.finish(&body) {
        Ok(k) => {
            g.0 = Auth::Pending(derive(&k));
            msg.into_response()
        }
        Err(_) => {
            g.1 += 1;
            bad(StatusCode::BAD_REQUEST, "bad pairing message")
        }
    }
}

async fn pair2(State(c): State<Arc<Ctx>>, body: axum::body::Bytes) -> Response {
    let mut g = c.auth.lock().unwrap_or_else(|e| e.into_inner());
    let Auth::Pending(k) = std::mem::replace(&mut g.0, Auth::Open) else {
        return bad(StatusCode::CONFLICT, "pair first");
    };
    if tag_ok(&k, "receiver", "", &body) {
        let reply = tag(&k, "sender", "");
        g.0 = Auth::Ready(k);
        reply.into_response()
    } else {
        g.1 += 1;
        if g.1 >= MAX_FAILS {
            c.stop.cancel();
        }
        bad(StatusCode::FORBIDDEN, "wrong code")
    }
}

fn authed(c: &Ctx, h: &HeaderMap, path: &str) -> Option<Keys> {
    let g = c.auth.lock().unwrap_or_else(|e| e.into_inner());
    let Auth::Ready(k) = &g.0 else { return None };
    let got = unhex(h.get("x-mac")?.to_str().ok()?)?;
    tag_ok(k, "req", path, &got).then(|| k.clone())
}

async fn manifest_route(State(c): State<Arc<Ctx>>, h: HeaderMap) -> Response {
    let Some(k) = authed(&c, &h, "/manifest") else {
        return bad(StatusCode::FORBIDDEN, "not paired");
    };
    let mut out = Vec::new();
    for chunk in c.manifest.chunks(FRAME) {
        out.extend(seal(&k, c.counter.fetch_add(1, Ordering::Relaxed), "/manifest", chunk));
    }
    out.into_response()
}

async fn file_route(State(c): State<Arc<Ctx>>, AxPath(id): AxPath<u32>, h: HeaderMap) -> Response {
    let path = format!("/f/{id}");
    let Some(k) = authed(&c, &h, &path) else {
        return bad(StatusCode::FORBIDDEN, "not paired");
    };
    let Some(p) = c.files.get(id as usize).cloned() else {
        return bad(StatusCode::NOT_FOUND, "no such file");
    };
    let Ok(f) = tokio::fs::File::open(&p).await else {
        return bad(StatusCode::NOT_FOUND, "cannot read");
    };
    let stream = futures_util::stream::unfold((f, c.clone(), k, path, false), |(mut f, c, k, path, end)| async move {
        if end {
            return None;
        }
        let mut buf = vec![0u8; FRAME];
        let mut n = 0;
        while n < FRAME {
            match f.read(&mut buf[n..]).await {
                Ok(0) => break,
                Ok(m) => n += m,
                Err(e) => return Some((Err(e), (f, c, k, path, true))),
            }
        }
        if n == 0 {
            c.done.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let frame = seal(&k, c.counter.fetch_add(1, Ordering::Relaxed), &path, &buf[..n]);
        let sent = c.sent.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
        (c.on)(Progress {
            phase: "sending".into(),
            bytes_done: sent,
            bytes_total: c.total,
            files_done: c.done.load(Ordering::Relaxed),
            files_total: c.files.len() as u64,
        });
        Some((Ok::<_, std::io::Error>(frame), (f, c, k, path, n < FRAME)))
    });
    Body::from_stream(stream).into_response()
}

// ---------------------------------------------------------------- finding and receiving

#[derive(Clone, Serialize)]
pub struct Peer {
    pub name: String,
    /// ip:port
    pub addr: String,
}

/// Other PCs offering something right now.
pub async fn scan() -> Vec<Peer> {
    tokio::task::spawn_blocking(|| {
        let Ok(d) = mdns_sd::ServiceDaemon::new() else {
            return Vec::new();
        };
        let Ok(rx) = d.browse(SERVICE) else { return Vec::new() };
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut out: Vec<Peer> = Vec::new();
        while let Some(left) = end.checked_duration_since(std::time::Instant::now()) {
            let Ok(ev) = rx.recv_timeout(left) else { break };
            if let mdns_sd::ServiceEvent::ServiceResolved(info) = ev {
                let name = info.get_fullname().split("._").next().unwrap_or("PC").to_string();
                let ip = info.get_addresses_v4().into_iter().next().map(|a| a.to_string());
                if let Some(ip) = ip {
                    let addr = format!("{ip}:{}", info.get_port());
                    if !out.iter().any(|p| p.addr == addr) {
                        out.push(Peer { name, addr });
                    }
                }
            }
        }
        let _ = d.shutdown();
        out
    })
    .await
    .unwrap_or_default()
}

/// The address other PCs reach this one at, for typing in by hand when mDNS is blocked.
pub fn local_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?; // no packet is sent: this only picks the outgoing interface
    Some(s.local_addr().ok()?.ip().to_string())
}

fn safe_rel(rel: &str) -> bool {
    !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_))) && !rel.contains('\\')
}

fn slug(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(24)
        .collect();
    t.trim_matches('-').to_string()
}

pub struct Received {
    /// The folder under the PC root, and the password of the repository in it.
    pub repos: Vec<(String, String)>,
}

pub async fn receive(addr: &str, code: &str, root: &Path, on: impl Fn(Progress) + Send + Sync) -> Res<Received> {
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let url = |p: &str| format!("http://{addr}{p}");
    on(Progress {
        phase: "pairing".into(),
        ..Default::default()
    });
    let (st, msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(code.trim().as_bytes()),
        &Identity::new(b"orluma-receiver"),
        &Identity::new(b"orluma-sender"),
    );
    let r = http
        .post(url("/pair1"))
        .body(msg)
        .send()
        .await
        .map_err(|e| format!("cannot reach the other PC: {e}"))?;
    if !r.status().is_success() {
        return Err(r.text().await.unwrap_or_default());
    }
    let shared = st
        .finish(&r.bytes().await.map_err(|e| e.to_string())?)
        .map_err(|_| "pairing failed".to_string())?;
    let keys = derive(&shared);
    let r = http
        .post(url("/pair2"))
        .body(tag(&keys, "receiver", ""))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if r.status() == StatusCode::FORBIDDEN {
        return Err("WRONG_CODE".into());
    }
    if !r.status().is_success() {
        return Err(r.text().await.unwrap_or_default());
    }
    if !tag_ok(&keys, "sender", "", &r.bytes().await.map_err(|e| e.to_string())?) {
        return Err("the other PC did not prove it knows the code".into());
    }

    let get = |path: String| {
        let (http, k) = (http.clone(), keys.clone());
        let u = url(&path);
        async move {
            let r = http
                .get(u)
                .header("x-mac", hex(&tag(&k, "req", &path)))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !r.status().is_success() {
                return Err(format!("the other PC refused: {}", r.status()));
            }
            Ok((
                r,
                Frames {
                    buf: Vec::new(),
                    keys: k,
                    aad: path,
                },
            ))
        }
    };
    let (r, mut fr) = get("/manifest".into()).await?;
    let mut raw = Vec::new();
    let mut s = r.bytes_stream();
    while let Some(b) = s.next().await {
        for p in fr.push(&b.map_err(|e| e.to_string())?)? {
            raw.extend(p);
        }
    }
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let total: u64 = manifest.repos.iter().flat_map(|r| &r.files).map(|f| f.size).sum();
    let count: u64 = manifest.repos.iter().map(|r| r.files.len() as u64).sum();
    let (mut done, mut files_done) = (0u64, 0u64);
    let prefix = format!("in-{}", slug(&manifest.from));
    let mut repos = Vec::new();
    for offer in &manifest.repos {
        if !offer
            .folder
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || offer.folder.is_empty()
        {
            return Err("the other PC offered a folder with an odd name".into());
        }
        let folder = format!("{prefix}-{}", offer.folder);
        let base = root.join(&folder);
        for d in ["data", "keys", "locks", "snapshots", "index"] {
            tokio::fs::create_dir_all(base.join(d))
                .await
                .map_err(|e| e.to_string())?;
        }
        for f in &offer.files {
            if !safe_rel(&f.rel) {
                return Err("the other PC offered a path that leaves the folder".into());
            }
            let dest = base.join(&f.rel);
            if tokio::fs::metadata(&dest).await.is_ok_and(|m| m.len() == f.size) {
                done += f.size;
                files_done += 1;
                continue;
            }
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
            }
            let tmp = dest.with_extension("part");
            let mut out = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
            let (r, mut fr) = get(format!("/f/{}", f.id)).await?;
            let (mut hasher, mut got) = (Sha256::new(), 0u64);
            let mut s = r.bytes_stream();
            while let Some(b) = s.next().await {
                for p in fr.push(&b.map_err(|e| e.to_string())?)? {
                    hasher.update(&p);
                    out.write_all(&p).await.map_err(|e| e.to_string())?;
                    got += p.len() as u64;
                    done += p.len() as u64;
                    on(Progress {
                        phase: "receiving".into(),
                        bytes_done: done,
                        bytes_total: total,
                        files_done,
                        files_total: count,
                    });
                }
            }
            out.flush().await.map_err(|e| e.to_string())?;
            drop(out);
            let name = Path::new(&f.rel)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if got != f.size || (f.rel != "config" && hex(&hasher.finalize()) != name) {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(format!("{} arrived damaged; nothing was kept of it", f.rel));
            }
            tokio::fs::rename(&tmp, &dest).await.map_err(|e| e.to_string())?;
            files_done += 1;
        }
        repos.push((folder, offer.password.clone()));
    }
    on(Progress {
        phase: "done".into(),
        bytes_done: total,
        bytes_total: total,
        files_done: count,
        files_total: count,
    });
    Ok(Received { repos })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_reject_tampering() {
        let k = derive(b"shared secret");
        let mut sealed = seal(&k, 1, "/f/1", b"hello backup");
        sealed.extend(seal(&k, 2, "/f/1", b"second"));
        let mut f = Frames {
            buf: Vec::new(),
            keys: k.clone(),
            aad: "/f/1".into(),
        };
        let mut got = Vec::new();
        for chunk in sealed.chunks(7) {
            got.extend(f.push(chunk).unwrap());
        }
        assert_eq!(got, [b"hello backup".to_vec(), b"second".to_vec()]);
        // the same frames under another request path, or flipped, do not open
        let mut other = Frames {
            buf: Vec::new(),
            keys: k.clone(),
            aad: "/f/2".into(),
        };
        assert!(other.push(&sealed).is_err());
        let mut flipped = sealed.clone();
        *flipped.last_mut().unwrap() ^= 1;
        let mut f = Frames {
            buf: Vec::new(),
            keys: k,
            aad: "/f/1".into(),
        };
        assert!(f.push(&flipped).is_err());
    }

    fn fake_repo(root: &Path) -> String {
        let blob = b"some encrypted chunk".to_vec();
        let blob2 = blob.clone();
        let name = hex(&Sha256::digest(&blob));
        let base = root.join("srv1");
        for (rel, body) in [
            ("config", b"cfg".to_vec()),
            (&format!("data/{}/{name}", &name[..2]), blob),
            ("keys/k", blob2),
        ] {
            let rel = if rel == "keys/k" {
                format!("keys/{name}")
            } else {
                rel.to_string()
            };
            std::fs::create_dir_all(base.join(&rel).parent().unwrap()).unwrap();
            std::fs::write(base.join(rel), body).unwrap();
        }
        name
    }

    #[tokio::test]
    async fn two_pcs_exchange_a_repository() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tmp = std::env::temp_dir().join(format!("orluma-lan-{}", std::process::id()));
        let (a, b) = (tmp.join("a"), tmp.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let name = fake_repo(&a);
        let sh = share(&a, vec![("srv1".into(), "repo-pw".into())], "pc-a", |_| {})
            .await
            .unwrap();
        let addr = format!("127.0.0.1:{}", sh.port);
        // a wrong code gets nothing, and three of them close the offer
        for _ in 0..3 {
            assert_eq!(
                receive(&addr, "00000000", &b, |_| {}).await.err().as_deref(),
                Some("WRONG_CODE")
            );
        }
        assert!(receive(&addr, &sh.code, &b, |_| {}).await.is_err());
        sh.stop();
        let sh = share(&a, vec![("srv1".into(), "repo-pw".into())], "pc-a", |_| {})
            .await
            .unwrap();
        let got = receive(&format!("127.0.0.1:{}", sh.port), &sh.code, &b, |_| {})
            .await
            .unwrap();
        assert_eq!(got.repos, [("in-pc-a-srv1".to_string(), "repo-pw".to_string())]);
        let dir = b.join("in-pc-a-srv1");
        assert_eq!(std::fs::read(dir.join("config")).unwrap(), b"cfg");
        assert_eq!(
            std::fs::read(dir.join(format!("data/{}/{name}", &name[..2]))).unwrap(),
            b"some encrypted chunk"
        );
        sh.stop();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn paths_stay_inside() {
        assert!(safe_rel("data/ab/abcdef"));
        assert!(!safe_rel("../x"));
        assert!(!safe_rel("/etc/passwd"));
        assert!(!safe_rel("a\\b"));
    }
}
