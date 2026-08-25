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
const MAX_CONTRIBUTIONS: usize = 200;
const MAX_CONTRIBUTION_BYTES: usize = 12_000_000;
const MAX_HTTP_REQUEST_BYTES: usize = 3_000_000;
const MAX_TUNNEL_START_ATTEMPTS: usize = 3;
const TUNNEL_HEALTH_ATTEMPTS: usize = 4;

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
    contribution_token: String,
    contributions: Vec<EncryptedContribution>,
    contribution_bytes: usize,
    next_sequence: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedContribution {
    sequence: u64,
    envelope: EncryptedShareEnvelope,
    received_at: String,
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
    let existing_url = {
        let current = state
            .lock()
            .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
        current
            .running
            .then(|| current.public_url.clone())
            .flatten()
    };
    if let Some(public_url) = existing_url {
        if verify_public_tunnel(&public_url).await.is_ok() {
            let current = state
                .lock()
                .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
            if current.running && current.public_url.as_deref() == Some(public_url.as_str()) {
                return Ok(status_from(&current));
            }
        } else {
            let mut current = state
                .lock()
                .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
            if current.public_url.as_deref() == Some(public_url.as_str()) {
                stop_inner(&mut current);
            }
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("Não foi possível abrir o servidor temporário: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let shutdown = Arc::new(AtomicBool::new(false));
    let server_shutdown = shutdown.clone();
    let shares = {
        let current = state
            .lock()
            .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
        current.shares.clone()
    };
    std::thread::spawn(move || run_http_server(listener, server_shutdown, shares));

    let mut public_url = None;
    let mut tunnel = None;
    let mut attempt_errors = Vec::new();

    for attempt in 1..=MAX_TUNNEL_START_ATTEMPTS {
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
                attempt_errors.push(format!("tentativa {attempt}: não iniciou ({error})"));
                continue;
            }
        };

        let (url_sender, url_receiver) = mpsc::sync_channel::<Result<String, String>>(1);
        let monitor_app = app.clone();
        let tunnel_alive = Arc::new(AtomicBool::new(true));
        let monitor_alive = tunnel_alive.clone();
        tauri::async_runtime::spawn(async move {
            let mut url_sent = false;
            let mut published_url: Option<String> = None;
            let mut output_buffer = Vec::with_capacity(4096);
            while let Some(event) = events.recv().await {
                match event {
                    CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => {
                        if !url_sent {
                            output_buffer.extend_from_slice(&bytes);
                            if output_buffer.len() > 16_384 {
                                output_buffer.drain(..output_buffer.len() - 16_384);
                            }
                            if let Some(url) = extract_quick_tunnel_url(&output_buffer) {
                                let _ = url_sender.send(Ok(url.clone()));
                                published_url = Some(url);
                                url_sent = true;
                            }
                        }
                    }
                    CommandEvent::Error(error) => {
                        monitor_alive.store(false, Ordering::Relaxed);
                        if !url_sent {
                            let _ = url_sender.send(Err(error));
                        }
                        if let Some(url) = published_url.as_deref() {
                            mark_tunnel_stopped(&monitor_app, url);
                        }
                        break;
                    }
                    CommandEvent::Terminated(payload) => {
                        monitor_alive.store(false, Ordering::Relaxed);
                        if !url_sent {
                            let _ = url_sender.send(Err(format!(
                                "O túnel encerrou antes de publicar a URL (código {:?}).",
                                payload.code
                            )));
                        }
                        if let Some(url) = published_url.as_deref() {
                            mark_tunnel_stopped(&monitor_app, url);
                        }
                        break;
                    }
                    _ => {}
                }
            }
        });

        let received_url = tauri::async_runtime::spawn_blocking(move || {
            url_receiver
                .recv_timeout(Duration::from_secs(35))
                .map_err(|_| "o Cloudflare não retornou uma URL em 35 segundos".to_string())?
        })
        .await
        .map_err(|error| error.to_string())?;

        match received_url {
            Ok(url) => match verify_public_tunnel(&url).await {
                Ok(()) if tunnel_alive.load(Ordering::Relaxed) => {
                    public_url = Some(url);
                    tunnel = Some(child);
                    break;
                }
                Ok(()) => {
                    attempt_errors.push(format!(
                        "tentativa {attempt}: o túnel encerrou após publicar a URL"
                    ));
                    let _ = child.kill();
                }
                Err(error) => {
                    attempt_errors.push(format!("tentativa {attempt}: {error}"));
                    let _ = child.kill();
                }
            },
            Err(error) => {
                attempt_errors.push(format!("tentativa {attempt}: {error}"));
                let _ = child.kill();
            }
        }
    }

    let (public_url, tunnel) = match (public_url, tunnel) {
        (Some(public_url), Some(tunnel)) => (public_url, tunnel),
        _ => {
            shutdown.store(true, Ordering::Relaxed);
            return Err(format!(
                "O Cloudflare não publicou um endereço acessível após {MAX_TUNNEL_START_ATTEMPTS} tentativas. {}",
                attempt_errors.join("; ")
            ));
        }
    };

    let mut current = state
        .lock()
        .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
    current.running = true;
    current.public_url = Some(public_url);
    current.local_port = Some(port);
    current.server_shutdown = Some(shutdown);
    current.tunnel = Some(tunnel);
    Ok(status_from(&current))
}

async fn verify_public_tunnel(public_url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(7))
        .user_agent("NarraHub-Tunnel-Health/1")
        .build()
        .map_err(|error| format!("não foi possível preparar a verificação pública ({error})"))?;
    let health_url = format!("{}/health", public_url.trim_end_matches('/'));
    let mut last_error = "resposta pública inválida".to_string();

    for attempt in 1..=TUNNEL_HEALTH_ATTEMPTS {
        match client.get(&health_url).send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if is_valid_tunnel_health(status.as_u16(), &body) {
                    return Ok(());
                }
                last_error =
                    format!("HTTPS retornou status {status} sem a confirmação do NarraHub");
            }
            Err(error) => {
                last_error = if error.is_connect() {
                    "o endereço público não resolveu no DNS ou recusou a conexão".to_string()
                } else if error.is_timeout() {
                    "a verificação HTTPS excedeu o tempo limite".to_string()
                } else {
                    format!("a verificação HTTPS falhou ({error})")
                };
            }
        }

        if attempt < TUNNEL_HEALTH_ATTEMPTS {
            let _ = tauri::async_runtime::spawn_blocking(|| {
                std::thread::sleep(Duration::from_secs(1));
            })
            .await;
        }
    }

    Err(last_error)
}

fn is_valid_tunnel_health(status: u16, body: &str) -> bool {
    if !(200..300).contains(&status) {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .is_some_and(|payload| {
            payload.get("ok").and_then(|value| value.as_bool()) == Some(true)
                && payload.get("service").and_then(|value| value.as_str()) == Some("narrahub-share")
        })
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
    contribution_token: String,
) -> Result<CreatedOnlineShare, String> {
    validate_envelope(&envelope, expires_in_days)?;
    if contribution_token.len() != 43 || !is_base64_url(&contribution_token) {
        return Err("Token de colaboração inválido.".to_string());
    }
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
                contribution_token,
                contributions: Vec::new(),
                contribution_bytes: 0,
                next_sequence: 1,
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
pub fn online_share_contributions(
    state: State<'_, Mutex<OnlineShareState>>,
    id: String,
    revoke_token: String,
    after_sequence: u64,
) -> Result<Vec<EncryptedContribution>, String> {
    let state = state
        .lock()
        .map_err(|_| "Estado do compartilhamento indisponível".to_string())?;
    let shares = state
        .shares
        .lock()
        .map_err(|_| "Conteúdo compartilhado indisponível".to_string())?;
    let record = shares
        .get(&id)
        .filter(|record| constant_time_eq(&record.revoke_token, &revoke_token))
        .ok_or_else(|| "Compartilhamento ou token de revisão inválido.".to_string())?;
    Ok(record
        .contributions
        .iter()
        .filter(|item| item.sequence > after_sequence)
        .cloned()
        .collect())
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

fn mark_tunnel_stopped(app: &AppHandle, expected_url: &str) {
    if let Some(state) = app.try_state::<Mutex<OnlineShareState>>() {
        if let Ok(mut state) = state.lock() {
            if state.public_url.as_deref() != Some(expected_url) {
                return;
            }
            if let Some(shutdown) = state.server_shutdown.take() {
                shutdown.store(true, Ordering::Relaxed);
            }
            state.running = false;
            state.public_url = None;
            state.local_port = None;
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

fn validate_contribution_envelope(envelope: &EncryptedShareEnvelope) -> Result<(), String> {
    if envelope.version != 1 || envelope.algorithm != "A256GCM" {
        return Err("Versão ou algoritmo da contribuição não suportado.".to_string());
    }
    if envelope.iv.len() != 16 || !is_base64_url(&envelope.iv) {
        return Err("IV da contribuição inválido.".to_string());
    }
    if envelope.ciphertext.len() < 24
        || envelope.ciphertext.len() > MAX_CIPHERTEXT_LENGTH
        || !is_base64_url(&envelope.ciphertext)
    {
        return Err("Contribuição cifrada inválida ou acima do limite.".to_string());
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
    let request = read_http_request(&mut stream)?;
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "Requisição HTTP inválida.".to_string())?;
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let body = &request[header_end + 4..];
    let request_line = headers.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let request_target = parts.next().unwrap_or("/");
    let path = request_target.split('?').next().unwrap_or("/");
    if method != "GET" && method != "HEAD" && method != "POST" {
        return write_response(
            &mut stream,
            405,
            "application/json; charset=utf-8",
            r#"{"error":"Método não permitido."}"#.as_bytes(),
            method == "HEAD",
        );
    }

    let (status, content_type, response_body) = if path == "/health" {
        (200, "application/json; charset=utf-8", br#"{"ok":true,"service":"narrahub-share","encryption":"client-side","lifetime":"app-session"}"#.to_vec())
    } else if path == "/viewer.js" {
        (
            200,
            "text/javascript; charset=utf-8",
            VIEWER_JS.as_bytes().to_vec(),
        )
    } else if path == "/viewer.css" {
        (
            200,
            "text/css; charset=utf-8",
            VIEWER_CSS.as_bytes().to_vec(),
        )
    } else if path == "/favicon.ico" {
        (204, "image/x-icon", Vec::new())
    } else if let Some((id, resource)) = parse_share_resource(path) {
        if resource == "contributions" && method == "POST" {
            let token = header_value(&headers, "x-narrahub-contribution-token").unwrap_or_default();
            match append_contribution(shares, id, token, body) {
                Ok(contribution) => (
                    201,
                    "application/json; charset=utf-8",
                    json!({ "sequence": contribution.sequence, "receivedAt": contribution.received_at }).to_string().into_bytes(),
                ),
                Err((status, message)) => (
                    status,
                    "application/json; charset=utf-8",
                    json!({ "error": message }).to_string().into_bytes(),
                ),
            }
        } else if resource == "contributions" && (method == "GET" || method == "HEAD") {
            let after = query_parameter(request_target, "after")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            match read_contributions(shares, id, after) {
                Some(items) => (
                    200,
                    "application/json; charset=utf-8",
                    json!({ "items": items }).to_string().into_bytes(),
                ),
                None => (
                    404,
                    "application/json; charset=utf-8",
                    br#"{"error":"Compartilhamento inexistente, encerrado ou expirado."}"#.to_vec(),
                ),
            }
        } else {
            (
                404,
                "application/json; charset=utf-8",
                r#"{"error":"Rota não encontrada."}"#.as_bytes().to_vec(),
            )
        }
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
                (
                    200,
                    "application/json; charset=utf-8",
                    payload.to_string().into_bytes(),
                )
            }
            None => (
                404,
                "application/json; charset=utf-8",
                br#"{"error":"Compartilhamento inexistente, encerrado ou expirado."}"#.to_vec(),
            ),
        }
    } else if path.starts_with("/s/") {
        (
            200,
            "text/html; charset=utf-8",
            VIEWER_HTML.as_bytes().to_vec(),
        )
    } else {
        (
            404,
            "application/json; charset=utf-8",
            r#"{"error":"Rota não encontrada."}"#.as_bytes().to_vec(),
        )
    };
    write_response(
        &mut stream,
        status,
        content_type,
        &response_body,
        method == "HEAD",
    )
}

fn read_http_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut request = Vec::with_capacity(8192);
    let mut chunk = [0_u8; 8192];
    let mut expected_length: Option<usize> = None;
    loop {
        let length = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if length == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..length]);
        if request.len() > MAX_HTTP_REQUEST_BYTES {
            return Err("Requisição acima do limite permitido.".to_string());
        }
        if expected_length.is_none() {
            if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = header_value(&headers, "content-length")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                expected_length = Some(header_end + 4 + content_length);
            }
        }
        if expected_length.is_some_and(|expected| request.len() >= expected) {
            break;
        }
    }
    Ok(request)
}

fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn parse_share_resource(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/v1/shares/")?;
    let (id, resource) = rest.split_once('/')?;
    Some((id, resource))
}

fn query_parameter<'a>(target: &'a str, name: &str) -> Option<&'a str> {
    target.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

fn append_contribution(
    shares: &Arc<Mutex<HashMap<String, ShareRecord>>>,
    id: &str,
    token: &str,
    body: &[u8],
) -> Result<EncryptedContribution, (u16, String)> {
    let envelope: EncryptedShareEnvelope = serde_json::from_slice(body)
        .map_err(|_| (400, "Contribuição cifrada inválida.".to_string()))?;
    validate_contribution_envelope(&envelope).map_err(|message| (400, message))?;
    let mut shares = shares
        .lock()
        .map_err(|_| (500, "Conteúdo compartilhado indisponível.".to_string()))?;
    let record = shares.get_mut(id).ok_or_else(|| {
        (
            404,
            "Compartilhamento inexistente, encerrado ou expirado.".to_string(),
        )
    })?;
    if !constant_time_eq(&record.contribution_token, token) {
        return Err((403, "Token de colaboração inválido.".to_string()));
    }
    let contribution_size = envelope.ciphertext.len();
    if record.contributions.len() >= MAX_CONTRIBUTIONS
        || record.contribution_bytes + contribution_size > MAX_CONTRIBUTION_BYTES
    {
        return Err((429, "A sessão atingiu o limite de contribuições. Peça ao autor para revisar e abrir uma nova sessão.".to_string()));
    }
    let contribution = EncryptedContribution {
        sequence: record.next_sequence,
        envelope,
        received_at: Utc::now().to_rfc3339(),
    };
    record.next_sequence += 1;
    record.contribution_bytes += contribution_size;
    record.contributions.push(contribution.clone());
    Ok(contribution)
}

fn read_contributions(
    shares: &Arc<Mutex<HashMap<String, ShareRecord>>>,
    id: &str,
    after: u64,
) -> Option<Vec<EncryptedContribution>> {
    let shares = shares.lock().ok()?;
    let record = shares.get(id)?;
    Some(
        record
            .contributions
            .iter()
            .filter(|item| item.sequence > after)
            .cloned()
            .collect(),
    )
}

fn read_share(shares: &Arc<Mutex<HashMap<String, ShareRecord>>>, id: &str) -> Option<ShareRecord> {
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
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nPermissions-Policy: camera=(), microphone=(), geolocation=()\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'self'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| {
            if head_only {
                Ok(())
            } else {
                stream.write_all(body)
            }
        })
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
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
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
    fn validates_only_the_narrahub_public_health_response() {
        assert!(is_valid_tunnel_health(
            200,
            r#"{"ok":true,"service":"narrahub-share","encryption":"client-side"}"#
        ));
        assert!(!is_valid_tunnel_health(502, "cloudflare bad gateway"));
        assert!(!is_valid_tunnel_health(
            200,
            r#"{"ok":true,"service":"another-service"}"#
        ));
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
