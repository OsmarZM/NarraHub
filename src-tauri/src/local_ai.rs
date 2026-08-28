use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use sysinfo::{Disks, System};
use tauri::{AppHandle, Emitter, Manager, State};

const LOCAL_AI_PORT: u16 = 11439;
const LLAMA_RELEASE: &str = "b10612";
const LLAMA_RUNTIME_NAME: &str = "llama-b10612-bin-win-cpu-x64.zip";
const LLAMA_RUNTIME_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b10612/llama-b10612-bin-win-cpu-x64.zip";
const LLAMA_RUNTIME_SIZE: u64 = 18_067_753;
const LLAMA_RUNTIME_SHA256: &str =
    "4481a3550d4b70132fb7e1f1973cc8c19e761a9c64d3f37fa78241dd3fcdf5b5";
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    logical_cores: usize,
    physical_cores: usize,
    total_memory_gb: f64,
    available_memory_gb: f64,
    available_storage_gb: f64,
    cpu: String,
    architecture: String,
    avx2: bool,
    gpu: Option<String>,
    gpu_memory_gb: Option<f64>,
    score: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfile {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    download_bytes: u64,
    minimum_memory_gb: f64,
    use_cases: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAiStatus {
    state: String,
    installed: bool,
    running: bool,
    supported: bool,
    message: String,
    installed_profile: Option<String>,
    model_alias: Option<String>,
    hardware: HardwareProfile,
    recommended: ModelProfile,
    profiles: Vec<ModelProfile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    stage: String,
    percent: u8,
    downloaded_bytes: u64,
    total_bytes: u64,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallationManifest {
    version: u8,
    profile: String,
    runtime_release: String,
    runtime_path: String,
    model_path: String,
    model_alias: String,
    installed_at: String,
}

struct ModelDownload {
    profile: ModelProfile,
    file_name: &'static str,
    url: &'static str,
    sha256: &'static str,
}

#[derive(Default)]
pub struct LocalAiRuntimeState {
    child: Option<Child>,
    installing: bool,
    last_error: Option<String>,
    hardware: Option<HardwareProfile>,
}

fn profiles() -> Vec<ModelProfile> {
    vec![
        ModelProfile {
            id: "lite",
            name: "NarraAI Lite",
            description: "Mais leve para tarefas rápidas e computadores com pouca memória.",
            download_bytes: 428_970_080,
            minimum_memory_gb: 4.0,
            use_cases: &["nomes", "tags", "resumos curtos"],
        },
        ModelProfile {
            id: "standard",
            name: "NarraAI Standard",
            description: "Equilíbrio entre qualidade, velocidade e consumo para escrita diária.",
            download_bytes: 1_282_439_264,
            minimum_memory_gb: 8.0,
            use_cases: &["reescrita", "diálogos", "resumos de capítulo"],
        },
        ModelProfile {
            id: "advanced",
            name: "NarraAI Advanced",
            description: "Mais contexto e qualidade para análises e desenvolvimento narrativo.",
            download_bytes: 2_497_280_256,
            minimum_memory_gb: 16.0,
            use_cases: &["análise narrativa", "brainstorming", "capítulos longos"],
        },
    ]
}

fn model_download(profile_id: &str) -> Option<ModelDownload> {
    let profile = profiles()
        .into_iter()
        .find(|profile| profile.id == profile_id)?;
    match profile_id {
        "lite" => Some(ModelDownload {
            profile,
            file_name: "Qwen3-0.6B-Q4_0.gguf",
            url: "https://huggingface.co/ggml-org/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_0.gguf",
            sha256: "da2572f16c06133561ce56accaa822216f2391ef4d37fba427801cd6736417d4",
        }),
        "standard" => Some(ModelDownload {
            profile,
            file_name: "Qwen3-1.7B-Q4_K_M.gguf",
            url: "https://huggingface.co/ggml-org/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf",
            sha256: "d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5",
        }),
        "advanced" => Some(ModelDownload {
            profile,
            file_name: "Qwen3-4B-Q4_K_M.gguf",
            url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
            sha256: "7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5",
        }),
        _ => None,
    }
}

fn gpu_profile() -> (Option<String>, Option<f64>) {
    #[cfg(windows)]
    {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-CimInstance Win32_VideoController | Select-Object -First 1 Name,AdapterRAM | ConvertTo-Json -Compress",
            ])
            .creation_flags(0x08000000)
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                #[derive(Deserialize)]
                #[serde(rename_all = "PascalCase")]
                struct GpuInfo {
                    name: Option<String>,
                    adapter_ram: Option<u64>,
                }
                if let Ok(info) = serde_json::from_slice::<GpuInfo>(&output.stdout) {
                    return (info.name, info.adapter_ram.map(|bytes| bytes as f64 / GIB));
                }
            }
        }
    }
    (None, None)
}

fn hardware_profile() -> HardwareProfile {
    let mut system = System::new_all();
    system.refresh_all();
    let logical_cores = system.cpus().len().max(1);
    let physical_cores = System::physical_core_count().unwrap_or(logical_cores);
    let total_memory_gb = system.total_memory() as f64 / GIB;
    let available_memory_gb = system.available_memory() as f64 / GIB;
    let available_storage_gb = Disks::new_with_refreshed_list()
        .iter()
        .map(|disk| disk.available_space())
        .max()
        .unwrap_or(0) as f64
        / GIB;
    let cpu = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "CPU não identificada".into());
    let (gpu, gpu_memory_gb) = gpu_profile();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let avx2 = std::is_x86_feature_detected!("avx2");
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let avx2 = false;

    let cpu_score = match logical_cores {
        0..=3 => 8,
        4..=7 => 18,
        8..=11 => 25,
        _ => 30,
    };
    let memory_score = if total_memory_gb >= 16.0 {
        30
    } else if total_memory_gb >= 8.0 {
        22
    } else {
        10
    };
    let gpu_name = gpu.clone().unwrap_or_default().to_lowercase();
    let gpu_score =
        if gpu_name.contains("nvidia") || gpu_name.contains("radeon") || gpu_name.contains("arc") {
            if gpu_memory_gb.unwrap_or(0.0) >= 4.0 {
                30
            } else {
                22
            }
        } else if gpu.is_some() {
            10
        } else {
            0
        };
    let storage_score = if available_storage_gb >= 10.0 {
        10
    } else if available_storage_gb >= 4.0 {
        6
    } else {
        0
    };

    HardwareProfile {
        logical_cores,
        physical_cores,
        total_memory_gb: round_one(total_memory_gb),
        available_memory_gb: round_one(available_memory_gb),
        available_storage_gb: round_one(available_storage_gb),
        cpu,
        architecture: std::env::consts::ARCH.into(),
        avx2,
        gpu,
        gpu_memory_gb: gpu_memory_gb.map(round_one),
        score: (cpu_score + memory_score + gpu_score + storage_score).min(100),
    }
}

fn round_one(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn recommended_profile(hardware: &HardwareProfile) -> ModelProfile {
    let id = if hardware.available_memory_gb < 5.0 || hardware.total_memory_gb < 8.0 {
        "lite"
    } else if hardware.available_memory_gb >= 10.0 && hardware.score >= 66 {
        "advanced"
    } else {
        "standard"
    };
    profiles()
        .into_iter()
        .find(|profile| profile.id == id)
        .expect("known AI profile")
}

fn ai_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|path| path.join("ai"))
        .map_err(|error| format!("Não foi possível localizar a pasta da IA local: {error}"))
}

fn manifest_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(ai_root(app)?.join("installation.json"))
}

fn load_manifest(app: &AppHandle) -> Option<InstallationManifest> {
    let content = fs::read_to_string(manifest_path(app).ok()?).ok()?;
    let manifest = serde_json::from_str::<InstallationManifest>(&content).ok()?;
    if Path::new(&manifest.runtime_path).is_file() && Path::new(&manifest.model_path).is_file() {
        Some(manifest)
    } else {
        None
    }
}

fn is_port_ready() -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], LOCAL_AI_PORT));
    TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok()
}

fn process_exit_message(status: std::process::ExitStatus) -> String {
    let code = status.code().unwrap_or_default();
    if code == -1_073_741_819 {
        "O motor local encerrou por uma falha de acesso à memória (0xc0000005). Reinicie em modo seguro ou instale o perfil recomendado para este computador.".into()
    } else {
        format!("O motor local encerrou inesperadamente ({status}).")
    }
}

fn inspect_child(runtime: &mut LocalAiRuntimeState) {
    if let Some(child) = runtime.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                runtime.last_error = Some(process_exit_message(status));
                runtime.child = None;
            }
            Ok(None) => {}
            Err(error) => {
                runtime.last_error =
                    Some(format!("Não foi possível consultar o motor local: {error}"));
                runtime.child = None;
            }
        }
    }
}

fn start_managed_engine(app: &AppHandle, runtime: &mut LocalAiRuntimeState) -> Result<(), String> {
    if is_port_ready() {
        runtime.last_error = None;
        return Ok(());
    }
    inspect_child(runtime);
    if runtime.child.is_some() {
        return Ok(());
    }
    let manifest = load_manifest(app)
        .ok_or_else(|| "A IA local ainda não foi instalada neste dispositivo.".to_owned())?;
    let log_dir = ai_root(app)?.join("logs");
    fs::create_dir_all(&log_dir)
        .map_err(|error| format!("Não foi possível criar a pasta de logs da IA: {error}"))?;
    let log_path = log_dir.join("llama-server.log");
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| format!("Não foi possível abrir o log da IA: {error}"))?;
    let stderr = stdout
        .try_clone()
        .map_err(|error| format!("Não foi possível preparar o log da IA: {error}"))?;
    let threads = runtime
        .hardware
        .get_or_insert_with(hardware_profile)
        .physical_cores
        .clamp(2, 12)
        .to_string();
    let mut command = Command::new(&manifest.runtime_path);
    command
        .args([
            "--model",
            &manifest.model_path,
            "--alias",
            &manifest.model_alias,
            "--host",
            "127.0.0.1",
            "--port",
            &LOCAL_AI_PORT.to_string(),
            "--ctx-size",
            "4096",
            "--threads",
            &threads,
            "--n-gpu-layers",
            "0",
            "--load-mode",
            "none",
            "--sleep-idle-seconds",
            "300",
            "--no-webui",
        ])
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(parent) = Path::new(&manifest.runtime_path).parent() {
        command.current_dir(parent);
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let child = command
        .spawn()
        .map_err(|error| format!("Não foi possível iniciar a IA local: {error}"))?;
    runtime.child = Some(child);
    runtime.last_error = None;
    Ok(())
}

#[tauri::command]
pub fn local_ai_status(
    app: AppHandle,
    state: State<'_, Mutex<LocalAiRuntimeState>>,
) -> Result<LocalAiStatus, String> {
    let supported = cfg!(all(windows, target_arch = "x86_64"));
    let manifest = load_manifest(&app);
    let mut runtime = state
        .lock()
        .map_err(|_| "O gerenciador da IA local ficou indisponível.".to_owned())?;
    let hardware = runtime
        .hardware
        .get_or_insert_with(hardware_profile)
        .clone();
    let recommended = recommended_profile(&hardware);
    inspect_child(&mut runtime);
    let running = is_port_ready();
    let installed = manifest.is_some();
    let (state_name, message) = if !supported {
        (
            "unsupported",
            "A instalação gerenciada está disponível inicialmente para Windows 64 bits.".to_owned(),
        )
    } else if runtime.installing {
        (
            "installing",
            "Instalando os componentes locais autorizados pelo usuário.".to_owned(),
        )
    } else if running {
        (
            "ready",
            "IA local disponível. O modelo é liberado da memória após 5 minutos sem uso."
                .to_owned(),
        )
    } else if let Some(error) = runtime.last_error.clone() {
        ("error", error)
    } else if installed {
        (
            "stopped",
            "IA instalada e aguardando inicialização.".to_owned(),
        )
    } else {
        ("not_installed", "A IA local não está instalada. O NarraHub pedirá sua autorização antes de baixar qualquer arquivo.".to_owned())
    };
    Ok(LocalAiStatus {
        state: state_name.into(),
        installed,
        running,
        supported,
        message,
        installed_profile: manifest.as_ref().map(|item| item.profile.clone()),
        model_alias: manifest.as_ref().map(|item| item.model_alias.clone()),
        hardware,
        recommended,
        profiles: profiles(),
    })
}

#[tauri::command]
pub fn start_local_ai_engine(
    app: AppHandle,
    state: State<'_, Mutex<LocalAiRuntimeState>>,
) -> Result<bool, String> {
    let mut runtime = state
        .lock()
        .map_err(|_| "O gerenciador da IA local ficou indisponível.".to_owned())?;
    start_managed_engine(&app, &mut runtime)?;
    Ok(true)
}

#[tauri::command]
pub fn restart_local_ai_engine(
    app: AppHandle,
    state: State<'_, Mutex<LocalAiRuntimeState>>,
) -> Result<bool, String> {
    let mut runtime = state
        .lock()
        .map_err(|_| "O gerenciador da IA local ficou indisponível.".to_owned())?;
    if let Some(mut child) = runtime.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    runtime.last_error = None;
    start_managed_engine(&app, &mut runtime)?;
    Ok(true)
}

#[tauri::command]
pub async fn install_local_ai(
    app: AppHandle,
    profile: String,
    state: State<'_, Mutex<LocalAiRuntimeState>>,
) -> Result<LocalAiStatus, String> {
    {
        let mut runtime = state
            .lock()
            .map_err(|_| "O gerenciador da IA local ficou indisponível.".to_owned())?;
        if runtime.installing {
            return Err("A instalação da IA local já está em andamento.".into());
        }
        runtime.installing = true;
        runtime.last_error = None;
    }
    let result = install_local_ai_inner(&app, &profile).await;
    {
        let mut runtime = state
            .lock()
            .map_err(|_| "O gerenciador da IA local ficou indisponível.".to_owned())?;
        runtime.installing = false;
        if let Err(error) = &result {
            runtime.last_error = Some(error.clone());
        }
        if result.is_ok() {
            if let Err(error) = start_managed_engine(&app, &mut runtime) {
                runtime.last_error = Some(error.clone());
                return Err(error);
            }
        }
    }
    result?;
    local_ai_status(app, state)
}

async fn install_local_ai_inner(app: &AppHandle, profile_id: &str) -> Result<(), String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err(
            "A instalação gerenciada está disponível inicialmente para Windows 64 bits.".into(),
        );
    }
    let model = model_download(profile_id)
        .ok_or_else(|| "O perfil de IA escolhido não é válido.".to_owned())?;
    let hardware = hardware_profile();
    let required = model.profile.download_bytes + LLAMA_RUNTIME_SIZE + 1_073_741_824;
    if hardware.available_storage_gb * GIB < required as f64 {
        return Err(format!(
            "Espaço insuficiente. Libere pelo menos {:.1} GB para instalar este perfil.",
            required as f64 / GIB
        ));
    }
    if hardware.total_memory_gb + 0.1 < model.profile.minimum_memory_gb {
        return Err(format!("Este perfil requer pelo menos {:.0} GB de memória. Use o perfil recomendado pelo NarraHub.", model.profile.minimum_memory_gb));
    }

    let root = ai_root(app)?;
    let download_dir = root.join("downloads");
    let runtime_dir = root.join("runtime").join(LLAMA_RELEASE);
    let model_dir = root.join("models");
    fs::create_dir_all(&download_dir)
        .map_err(|error| format!("Não foi possível preparar a instalação: {error}"))?;
    fs::create_dir_all(&runtime_dir)
        .map_err(|error| format!("Não foi possível preparar o motor local: {error}"))?;
    fs::create_dir_all(&model_dir)
        .map_err(|error| format!("Não foi possível preparar a pasta de modelos: {error}"))?;

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "NarraHub/",
            env!("CARGO_PKG_VERSION"),
            " local-ai-installer"
        ))
        .build()
        .map_err(|error| format!("Não foi possível preparar o download: {error}"))?;
    let runtime_zip = download_dir.join(LLAMA_RUNTIME_NAME);
    download_verified(
        app,
        &client,
        LLAMA_RUNTIME_URL,
        &runtime_zip,
        LLAMA_RUNTIME_SIZE,
        LLAMA_RUNTIME_SHA256,
        2,
        18,
        "runtime",
        "Baixando o motor de escrita local",
    )
    .await?;
    emit_progress(
        app,
        "runtime",
        20,
        LLAMA_RUNTIME_SIZE,
        LLAMA_RUNTIME_SIZE,
        "Preparando o motor local",
    )?;
    extract_zip(&runtime_zip, &runtime_dir)?;
    let runtime_path = runtime_dir.join("llama-server.exe");
    if !runtime_path.is_file() {
        return Err("O pacote do motor local não contém llama-server.exe.".into());
    }

    let model_path = model_dir.join(model.file_name);
    download_verified(
        app,
        &client,
        model.url,
        &model_path,
        model.profile.download_bytes,
        model.sha256,
        22,
        96,
        "model",
        "Baixando o perfil de escrita recomendado",
    )
    .await?;
    let manifest = InstallationManifest {
        version: 1,
        profile: model.profile.id.into(),
        runtime_release: LLAMA_RELEASE.into(),
        runtime_path: runtime_path.to_string_lossy().into_owned(),
        model_path: model_path.to_string_lossy().into_owned(),
        model_alias: "narrahub-local".into(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("Não foi possível registrar a instalação: {error}"))?;
    fs::write(manifest_path(app)?, manifest_json)
        .map_err(|error| format!("Não foi possível concluir a instalação: {error}"))?;
    emit_progress(
        app,
        "complete",
        100,
        model.profile.download_bytes,
        model.profile.download_bytes,
        "IA local instalada com segurança",
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_verified(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    expected_size: u64,
    expected_sha256: &str,
    start_percent: u8,
    end_percent: u8,
    stage: &str,
    message: &str,
) -> Result<(), String> {
    if destination.is_file() && verify_file(destination, expected_size, expected_sha256)? {
        emit_progress(
            app,
            stage,
            end_percent,
            expected_size,
            expected_size,
            message,
        )?;
        return Ok(());
    }
    let partial = destination.with_extension(format!(
        "{}.part",
        destination
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("download")
    ));
    if partial.exists() {
        fs::remove_file(&partial)
            .map_err(|error| format!("Não foi possível limpar um download incompleto: {error}"))?;
    }
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Falha ao iniciar o download da IA local: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "O servidor respondeu com HTTP {} durante o download da IA.",
            response.status()
        ));
    }
    let mut file = File::create(&partial)
        .map_err(|error| format!("Não foi possível criar o arquivo de download: {error}"))?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0_u64;
    let mut hasher = Sha256::new();
    let mut last_percent = 0_u8;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("O download da IA foi interrompido: {error}"))?;
        file.write_all(&chunk)
            .map_err(|error| format!("Não foi possível gravar o download da IA: {error}"))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        let portion = (downloaded.saturating_mul((end_percent - start_percent) as u64)
            / expected_size.max(1)) as u8;
        let percent = start_percent.saturating_add(portion).min(end_percent);
        if percent != last_percent {
            emit_progress(app, stage, percent, downloaded, expected_size, message)?;
            last_percent = percent;
        }
    }
    file.flush()
        .map_err(|error| format!("Não foi possível concluir a gravação da IA: {error}"))?;
    if downloaded != expected_size {
        return Err(format!(
            "O download ficou incompleto: esperado {expected_size} bytes, recebido {downloaded}."
        ));
    }
    let hash = format!("{:x}", hasher.finalize());
    if hash != expected_sha256 {
        let _ = fs::remove_file(&partial);
        return Err(
            "A verificação de segurança do download falhou. Nenhum componente foi instalado."
                .into(),
        );
    }
    fs::rename(&partial, destination)
        .map_err(|error| format!("Não foi possível finalizar o arquivo baixado: {error}"))?;
    Ok(())
}

fn verify_file(path: &Path, expected_size: u64, expected_sha256: &str) -> Result<bool, String> {
    if fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or_default()
        != expected_size
    {
        return Ok(false);
    }
    let mut file = File::open(path)
        .map_err(|error| format!("Não foi possível validar o arquivo local: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Não foi possível validar o arquivo local: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()) == expected_sha256)
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("Não foi possível abrir o pacote do motor local: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("O pacote do motor local é inválido: {error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Não foi possível ler o pacote do motor local: {error}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| format!("Não foi possível preparar o motor local: {error}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Não foi possível preparar o motor local: {error}"))?;
        }
        let mut target = File::create(&output)
            .map_err(|error| format!("Não foi possível extrair o motor local: {error}"))?;
        std::io::copy(&mut entry, &mut target)
            .map_err(|error| format!("Não foi possível extrair o motor local: {error}"))?;
    }
    Ok(())
}

fn emit_progress(
    app: &AppHandle,
    stage: &str,
    percent: u8,
    downloaded_bytes: u64,
    total_bytes: u64,
    message: &str,
) -> Result<(), String> {
    app.emit(
        "local-ai-install-progress",
        InstallProgress {
            stage: stage.into(),
            percent,
            downloaded_bytes,
            total_bytes,
            message: message.into(),
        },
    )
    .map_err(|error| format!("Não foi possível atualizar o progresso da instalação: {error}"))
}

pub fn stop_on_exit(app: &AppHandle) {
    if let Ok(mut runtime) = app.state::<Mutex<LocalAiRuntimeState>>().lock() {
        if let Some(mut child) = runtime.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware(total: f64, available: f64, score: u8) -> HardwareProfile {
        HardwareProfile {
            logical_cores: 8,
            physical_cores: 4,
            total_memory_gb: total,
            available_memory_gb: available,
            available_storage_gb: 100.0,
            cpu: "test".into(),
            architecture: "x86_64".into(),
            avx2: true,
            gpu: None,
            gpu_memory_gb: None,
            score,
        }
    }

    #[test]
    fn recommends_lite_when_memory_is_constrained() {
        assert_eq!(recommended_profile(&hardware(6.0, 3.0, 60)).id, "lite");
    }

    #[test]
    fn recommends_standard_for_typical_computer() {
        assert_eq!(recommended_profile(&hardware(12.0, 7.0, 62)).id, "standard");
    }

    #[test]
    fn recommends_advanced_only_with_memory_and_score() {
        assert_eq!(
            recommended_profile(&hardware(32.0, 18.0, 78)).id,
            "advanced"
        );
    }

    #[test]
    fn every_profile_has_a_verified_model_download() {
        for profile in profiles() {
            let download = model_download(profile.id).expect("download mapping");
            assert_eq!(download.sha256.len(), 64);
            assert_eq!(download.profile.download_bytes, profile.download_bytes);
        }
    }
}
