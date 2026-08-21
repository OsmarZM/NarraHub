use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use uuid::Uuid;

const VIEWER_HTML: &str = include_str!("../../services/share-api/public/viewer.html");
const VIEWER_JS: &str = include_str!("../../services/share-api/public/viewer.js");
const VIEWER_CSS: &str = include_str!("../../services/share-api/public/viewer.css");
const MAX_CIPHERTEXT_LENGTH: usize = 2_800_000;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedShareEnvelope {
    version: u8,
    algorithm: String,
    iv: String,
    ciphertext: String,
}

#[derive(Debug, Clone)]
struct ShareRecord {
    envelope: EncryptedShareEnvelope,
    expires_at: String,
    revoke_token: String,
}

pub struct OnlineShareState {
    running: bool,
    public_url: Option<String>,
    local_port: Option<u16>,
    server_shutdown: Option<Arc<AtomicBool>>,
    tunnel: Option<CommandChild>,
    shares: Arc<Mutex<HashMap<String, ShareRecord>>>,
}

impl Default for OnlineShareState {
    fn default() -> Self {
        Self {
            running: false,
            public_url: None,
            local_port: None,
            server_shutdown: None,
            tunnel: None,
            shares: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineShareStatus {
    running: bool,
    public_url: Option<String>,
    share_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedOnlineShare {
    id: String,
    url: String,
    expires_at: String,
    revoke_token: String,
}

#[tauri::command]
pub fn online_share_status(
    state: State<'_, Mutex<OnlineShareState>>,
) -> Result<OnlineShareStatus, String> {
    let state = state
        .lock()
        .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
    Ok(status_from(&state))
}

#[tauri::command]
pub async fn online_share_start(
    app: AppHandle,
    state: State<'_, Mutex<OnlineShareState>>,
) -> Result<OnlineShareStatus, String> {
    {
        let current = state
            .lock()
            .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
        if current.running {
            return Ok(status_from(&current));
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("Não foi possível abrir o servidor temporário: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let shutdown = Arc::new(AtomicBool::new(false));
    let server_shutdown = shutdown.clone();
    let shares = {
        let current = state
            .lock()
            .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
        current.shares.clone()
    };
    std::thread::spawn(move || run_http_server(listener, server_shutdown, shares));

    let command = match app.shell().sidecar("cloudflared") {
        Ok(command) => command,
        Err(error) => {
            shutdown.store(true, Ordering::Relaxed);
            return Err(format!(
                "O componente Cloudflare Tunnel não está disponível: {error}"
            ));
        }
    };
    let (mut events, child) = match command
        .args([
            "tunnel",
            "--url",
            &format!("http://127.0.0.1:{port}"),
            "--no-autoupdate",
        ])
        .spawn()
    {
        Ok(process) => process,
        Err(error) => {
            shutdown.store(true, Ordering::Relaxed);
            return Err(format!(
                "Não foi possível iniciar o túnel temporário: {error}"
            ));
        }
    };

    let (url_sender, url_receiver) = mpsc::sync_channel::<Result<String, String>>(1);
    let monitor_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut url_sent = false;
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => {
                    if !url_sent {
                        if let Some(url) = extract_quick_tunnel_url(&bytes) {
                            let _ = url_sender.send(Ok(url));
                            url_sent = true;
                        }
                    }
                }
                CommandEvent::Error(error) => {
                    if !url_sent {
                        let _ = url_sender.send(Err(error));
                    }
                    mark_tunnel_stopped(&monitor_app);
                    break;
                }
                CommandEvent::Terminated(payload) => {
                    if !url_sent {
                        let _ = url_sender.send(Err(format!(
                            "O túnel encerrou antes de publicar a URL (código {:?}).",
                            payload.code
                        )));
                    }
                    mark_tunnel_stopped(&monitor_app);
                    break;
                }
                _ => {}
            }
        }
    });

    let public_url = tauri::async_runtime::spawn_blocking(move || {
        url_receiver
            .recv_timeout(Duration::from_secs(35))
            .map_err(|_| "O Cloudflare não retornou uma URL em 35 segundos.".to_string())?
    })
    .await
    .map_err(|error| error.to_string())?;

    let public_url = match public_url {
        Ok(url) => url,
        Err(error) => {
            shutdown.store(true, Ordering::Relaxed);
            let _ = child.kill();
            return Err(error);
        }
    };

    let mut current = state
        .lock()
        .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
    current.running = true;
    current.public_url = Some(public_url);
    current.local_port = Some(port);
    current.server_shutdown = Some(shutdown);
    current.tunnel = Some(child);
    Ok(status_from(&current))
}

#[tauri::command]
pub fn online_share_stop(
    state: State<'_, Mutex<OnlineShareState>>,
) -> Result<OnlineShareStatus, String> {
    let mut state = state
        .lock()
        .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
    stop_inner(&mut state);
    Ok(status_from(&state))
}

#[tauri::command]
pub fn online_share_create(
    state: State<'_, Mutex<OnlineShareState>>,
    envelope: EncryptedShareEnvelope,
    expires_in_days: u8,
) -> Result<CreatedOnlineShare, String> {
    validate_envelope(&envelope, expires_in_days)?;
    let state = state
        .lock()
        .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
    let public_url = state
        .public_url
        .as_ref()
        .filter(|_| state.running)
        .ok_or_else(|| "Inicie a sessão de compartilhamento antes de criar o link.".to_string())?;
    let id = compact_uuid(16);
    let revoke_token = compact_uuid(32);
    let expires_at = (Utc::now() + ChronoDuration::days(i64::from(expires_in_days))).to_rfc3339();
    state
        .shares
        .lock()
        .map_err(|_| "Conteúdo compartilhado indisponível".to_string())?
        .insert(
            id.clone(),
            ShareRecord {
                envelope,
                expires_at: expires_at.clone(),
                revoke_token: revoke_token.clone(),
            },
        );
    Ok(CreatedOnlineShare {
        id: id.clone(),
        url: format!("{public_url}/s/{id}"),
        expires_at,
        revoke_token,
    })
}

#[tauri::command]
pub fn online_share_revoke(
    state: State<'_, Mutex<OnlineShareState>>,
    id: String,
    revoke_token: String,
) -> Result<OnlineShareStatus, String> {
    let state = state
        .lock()
        .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
    let mut shares = state
        .shares
        .lock()
        .map_err(|_| "Conteúdo compartilhado indisponível".to_string())?;
    let authorized = shares
        .get(&id)
        .is_some_and(|record| constant_time_eq(&record.revoke_token, &revoke_token));
    if !authorized {
        return Err("Token de revogação inválido.".to_string());
    }
    shares.remove(&id);
    drop(shares);
    Ok(status_from(&state))
}

pub fn stop_on_exit(app: &AppHandle) {
    if let Some(state) = app.try_state::<Mutex<OnlineShareState>>() {
        if let Ok(mut state) = state.lock() {
            stop_inner(&mut state);
        }
    }
}

fn stop_inner(state: &mut OnlineShareState) {
    if let Some(shutdown) = state.server_shutdown.take() {
        shutdown.store(true, Ordering::Relaxed);
    }
    if let Some(child) = state.tunnel.take() {
        let _ = child.kill();
    }
    if let Ok(mut shares) = state.shares.lock() {
        shares.clear();
    }
    state.running = false;
    state.public_url = None;
    state.local_port = None;
}

fn mark_tunnel_stopped(app: &AppHandle) {
    if let Some(state) = app.try_state::<Mutex<OnlineShareState>>() {
        if let Ok(mut state) = state.lock() {
            if let Some(shutdown) = state.server_shutdown.take() {
                shutdown.store(true, Ordering::Relaxed);
            }
            state.running = false;
            state.public_url = None;
            state.local_port = None;
            if let Ok(mut shares) = state.shares.lock() {
                shares.clear();
            }
        }
    }
}

fn status_from(state: &OnlineShareState) -> OnlineShareStatus {
    let share_count = state.shares.lock().map(|shares| shares.len()).unwrap_or(0);
    OnlineShareStatus {
        running: state.running,
        public_url: state.public_url.clone(),
        share_count,
    }
}

fn validate_envelope(envelope: &EncryptedShareEnvelope, expires_in_days: u8) -> Result<(), String> {
    if envelope.version != 1 || envelope.algorithm != "A256GCM" {
        return Err("Versão ou algoritmo de criptografia não suportado.".to_string());
    }
    if envelope.iv.len() != 16 || !is_base64_url(&envelope.iv) {
        return Err("IV criptográfico inválido.".to_string());
    }
    if envelope.ciphertext.len() < 24
        || envelope.ciphertext.len() > MAX_CIPHERTEXT_LENGTH
        || !is_base64_url(&envelope.ciphertext)
    {
        return Err("Conteúdo cifrado inválido ou acima do limite de 2,8 MB.".to_string());
    }
    if ![1, 7, 30].contains(&expires_in_days) {
        return Err("Expiração inválida.".to_string());
    }
    Ok(())
}

fn run_http_server(
    listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    shares: Arc<Mutex<HashMap<String, ShareRecord>>>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = handle_http_request(stream, &shares);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(80));
            }
            Err(_) => break,
        }
    }
}

fn handle_http_request(
    mut stream: TcpStream,
    shares: &Arc<Mutex<HashMap<String, ShareRecord>>>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    let mut buffer = [0_u8; 8192];
    let length = stream.read(&mut buffer).map_err(|error| error.to_string())?;
    let request = String::from_utf8_lossy(&buffer[..length]);
    let request_line = request.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/").split('?').next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return write_response(
            &mut stream,
            405,
            "application/json; charset=utf-8",
            r#"{"error":"Método não permitido."}"#.as_bytes(),
            method == "HEAD",
        );
    }

    let (status, content_type, body) = if path == "/health" {
        (200, "application/json; charset=utf-8", br#"{"ok":true,"service":"narrahub-share","encryption":"client-side","lifetime":"app-session"}"#.to_vec())
    } else if path == "/viewer.js" {
        (200, "text/javascript; charset=utf-8", VIEWER_JS.as_bytes().to_vec())
    } else if path == "/viewer.css" {
        (200, "text/css; charset=utf-8", VIEWER_CSS.as_bytes().to_vec())
    } else if path == "/favicon.ico" {
        (204, "image/x-icon", Vec::new())
    } else if let Some(id) = path.strip_prefix("/v1/shares/") {
        match read_share(shares, id) {
            Some(record) => {
                let payload = json!({
                    "version": record.envelope.version,
                    "algorithm": record.envelope.algorithm,
                    "iv": record.envelope.iv,
                    "ciphertext": record.envelope.ciphertext,
                    "expiresAt": record.expires_at,
                });
                (200, "application/json; charset=utf-8", payload.to_string().into_bytes())
            }
            None => (404, "application/json; charset=utf-8", br#"{"error":"Compartilhamento inexistente, encerrado ou expirado."}"#.to_vec()),
        }
    } else if path.starts_with("/s/") {
        (200, "text/html; charset=utf-8", VIEWER_HTML.as_bytes().to_vec())
    } else {
        (
            404,
            "application/json; charset=utf-8",
            r#"{"error":"Rota não encontrada."}"#.as_bytes().to_vec(),
        )
    };
    write_response(&mut stream, status, content_type, &body, method == "HEAD")
}

fn read_share(
    shares: &Arc<Mutex<HashMap<String, ShareRecord>>>,
    id: &str,
) -> Option<ShareRecord> {
    if id.len() != 16 || !id.chars().all(|value| value.is_ascii_alphanumeric()) {
        return None;
    }
    let mut shares = shares.lock().ok()?;
    let expired = shares
        .get(id)
        .and_then(|record| chrono::DateTime::parse_from_rfc3339(&record.expires_at).ok())
        .is_some_and(|expires_at| expires_at <= Utc::now());
    if expired {
        shares.remove(id);
        return None;
    }
    shares.get(id).cloned()
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nPermissions-Policy: camera=(), microphone=(), geolocation=()\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| if head_only { Ok(()) } else { stream.write_all(body) })
        .map_err(|error| error.to_string())
}

fn extract_quick_tunnel_url(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let start = text.find("https://")?;
    let suffix = ".trycloudflare.com";
    let relative_end = text[start..].find(suffix)?;
    let end = start + relative_end + suffix.len();
    let candidate = &text[start..end];
    candidate
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, ':' | '/' | '-' | '.'))
        .then(|| candidate.to_string())
}

fn compact_uuid(length: usize) -> String {
    Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(length)
        .collect()
}

fn is_base64_url(value: &str) -> bool {
    value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_cloudflare_quick_tunnel_url() {
        let output = br#"INF +----------------------------------------------+\nINF |  https://fictional-world.trycloudflare.com  |"#;
        assert_eq!(
            extract_quick_tunnel_url(output).as_deref(),
            Some("https://fictional-world.trycloudflare.com")
        );
    }

    #[test]
    fn rejects_invalid_encrypted_envelopes() {
        let invalid = EncryptedShareEnvelope {
            version: 1,
            algorithm: "A256GCM".to_string(),
            iv: "short".to_string(),
            ciphertext: "open-content".to_string(),
        };
        assert!(validate_envelope(&invalid, 7).is_err());
    }
}
