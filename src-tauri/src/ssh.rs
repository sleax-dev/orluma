//! SSH to the servers. One connection per server carries everything: the agent's API (a channel to its Unix
//! socket), commands during setup, and the reverse tunnel a server copies to this PC through.

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use russh::client::{self, Handle, Msg};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const AGENT_SOCKET: &str = "/run/orluma/agent.sock";

#[derive(Debug)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl From<russh::Error> for Error {
    fn from(e: russh::Error) -> Self {
        Error(e.to_string())
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}
impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Self {
        Error(e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub struct ClientHandler {
    /// The fingerprint we trust; `None` on first contact.
    expected: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
    /// Where connections the server opens back to us (reverse tunnel) are sent.
    forward_to: Arc<Mutex<Option<SocketAddr>>>,
}

pub fn fingerprint(k: &PublicKeyOrCertificate) -> String {
    match k {
        PublicKeyOrCertificate::PublicKey { key, .. } => key.fingerprint(HashAlg::Sha256).to_string(),
        PublicKeyOrCertificate::Certificate(c) => c.public_key().fingerprint(HashAlg::Sha256).to_string(),
    }
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> std::result::Result<bool, Self::Error> {
        let fp = fingerprint(key);
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = Some(fp.clone());
        // A pinned server must present the same key; a new one is accepted here and shown to the user,
        // who confirms it before the server is saved.
        Ok(self.expected.as_ref().is_none_or(|e| e == &fp))
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> std::result::Result<(), Self::Error> {
        let target = *self.forward_to.lock().unwrap_or_else(|e| e.into_inner());
        let Some(target) = target else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            if let Ok(mut local) = tokio::net::TcpStream::connect(target).await {
                let mut remote = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut remote, &mut local).await;
            }
        });
        Ok(())
    }
}

pub enum Auth {
    Key(Arc<PrivateKey>),
    Password(String),
}

pub struct Conn {
    handle: Handle<ClientHandler>,
    forward_to: Arc<Mutex<Option<SocketAddr>>>,
    remote_port: Mutex<Option<u32>>,
    pub fingerprint: String,
}

pub struct Exec {
    pub code: u32,
    pub stdout: String,
    pub stderr: String,
}

/// Connects and logs in. With `expected` set, a server whose host key differs is refused before any
/// credential is sent.
pub async fn connect(host: &str, port: u16, user: &str, auth: Auth, expected: Option<String>) -> Result<Conn> {
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(20)),
        keepalive_max: 3,
        ..Default::default()
    });
    let seen = Arc::new(Mutex::new(None));
    let forward_to = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        expected: expected.clone(),
        seen: seen.clone(),
        forward_to: forward_to.clone(),
    };
    let fut = client::connect(config, (host, port), handler);
    let mut handle = match tokio::time::timeout(Duration::from_secs(20), fut).await {
        Err(_) => return Err(Error(format!("{host}:{port} did not answer within 20 seconds"))),
        Ok(Err(e)) => {
            let seen_fp = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let (Some(want), Some(got)) = (&expected, &seen_fp)
                && want != got
            {
                return Err(Error(format!(
                    "HOST_KEY_CHANGED: {host} presented a different host key ({got}) than the one saved ({want}). \
                     If the server was reinstalled, remove it from Orluma and add it again."
                )));
            }
            return Err(Error(e.to_string()));
        }
        Ok(Ok(h)) => h,
    };
    let ok = match auth {
        Auth::Key(k) => {
            let hash = handle.best_supported_rsa_hash().await.ok().flatten().flatten();
            handle
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(k, hash))
                .await?
                .success()
        }
        Auth::Password(p) => handle.authenticate_password(user, p).await?.success(),
    };
    if !ok {
        return Err(Error(format!("AUTH_FAILED: {user}@{host} refused the login")));
    }
    let fingerprint = seen
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default();
    Ok(Conn {
        handle,
        forward_to,
        remote_port: Mutex::new(None),
        fingerprint,
    })
}

/// Only reads the host key, for "is this the server you meant?" before any password is typed.
pub async fn probe(host: &str, port: u16) -> Result<String> {
    let seen = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        expected: None,
        seen: seen.clone(),
        forward_to: Arc::new(Mutex::new(None)),
    };
    let fut = client::connect(Arc::new(client::Config::default()), (host, port), handler);
    match tokio::time::timeout(Duration::from_secs(15), fut).await {
        Err(_) => Err(Error(format!("{host}:{port} did not answer within 15 seconds"))),
        Ok(Err(e)) => Err(Error(e.to_string())),
        Ok(Ok(h)) => {
            let _ = h.disconnect(russh::Disconnect::ByApplication, "", "en").await;
            seen.lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .ok_or_else(|| Error("no host key".into()))
        }
    }
}

impl Conn {
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    /// Runs a command; `stdin` is written to it and closed.
    pub async fn exec(&self, cmd: &str, stdin: Option<&[u8]>) -> Result<Exec> {
        let mut ch = self.handle.channel_open_session().await?;
        ch.exec(true, cmd).await?;
        if let Some(data) = stdin {
            ch.data(data).await?;
        }
        ch.eof().await?;
        let (mut out, mut err, mut code) = (Vec::new(), Vec::new(), None);
        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => err.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                _ => {}
            }
        }
        Ok(Exec {
            code: code.unwrap_or(255),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }

    async fn agent_sender(&self) -> Result<hyper::client::conn::http1::SendRequest<Full<Bytes>>> {
        let ch = self
            .handle
            .channel_open_direct_streamlocal(AGENT_SOCKET)
            .await
            .map_err(|e| {
                Error(format!(
                    "AGENT_UNREACHABLE: the Orluma agent does not answer on this server ({e})"
                ))
            })?;
        let io = hyper_util::rt::TokioIo::new(ch.into_stream());
        let (sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        Ok(sender)
    }

    /// One request to the agent; the answer's status and body.
    pub async fn agent(&self, method: &str, path: &str, body: Option<Vec<u8>>) -> Result<(u16, Bytes)> {
        let res = self.agent_raw(method, path, body).await?;
        let status = res.status().as_u16();
        let bytes = res.into_body().collect().await?.to_bytes();
        Ok((status, bytes))
    }

    /// The response itself, for streams (events, downloads).
    pub async fn agent_raw(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<hyper::Response<hyper::body::Incoming>> {
        let mut sender = self.agent_sender().await?;
        let req = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header("host", "agent")
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body.unwrap_or_default())))
            .map_err(|e| Error(e.to_string()))?;
        Ok(sender.send_request(req).await?)
    }

    /// Asks the server to listen on 127.0.0.1:<port> and send what arrives to `target` on this PC. The same
    /// port is reused for the life of the connection.
    pub async fn reverse_tunnel(&self, target: SocketAddr) -> Result<u32> {
        *self.forward_to.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
        if let Some(p) = *self.remote_port.lock().unwrap_or_else(|e| e.into_inner()) {
            return Ok(p);
        }
        let port = self.handle.tcpip_forward("127.0.0.1", 0).await?;
        if port == 0 {
            return Err(Error(
                "the server refused the tunnel (AllowTcpForwarding is off in sshd_config)".into(),
            ));
        }
        *self.remote_port.lock().unwrap_or_else(|e| e.into_inner()) = Some(port);
        Ok(port)
    }

    /// Listens on 127.0.0.1:`port` on this PC and carries each connection to 127.0.0.1:`port` on the server
    /// (an SSH direct-tcpip channel), until the returned task is aborted. Used while a cloud sign-in page sends
    /// the browser back to rclone on the server.
    pub async fn local_forward(self: &Arc<Self>, port: u16) -> Result<tokio::task::JoinHandle<()>> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| {
            Error(format!(
                "port {port} on this PC is in use ({e}); close what uses it and try again"
            ))
        })?;
        let conn = self.clone();
        Ok(tokio::spawn(async move {
            while let Ok((mut tcp, _)) = listener.accept().await {
                let conn = conn.clone();
                tokio::spawn(async move {
                    if let Ok(ch) = conn
                        .handle
                        .channel_open_direct_tcpip("127.0.0.1", u32::from(port), "127.0.0.1", 0)
                        .await
                    {
                        let mut remote = ch.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut remote).await;
                    }
                });
            }
        }))
    }

    pub async fn close(&self) {
        let _ = self.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;
    }
}
