//! A atualização do Android na fronteira com o frontend.
//!
//! ```text
//! android_update_check     consulta as releases; guarda a novidade AQUI, no Rust
//! android_update_download  baixa a novidade guardada, com progresso
//! android_update_install   confere o SHA de novo e abre o instalador do Android
//! ```
//!
//! **Nenhum comando recebe URL, caminho ou hash.** A tela não escolhe de onde baixar nem o que
//! instalar: ela só pede o próximo passo, e o Rust usa o que ele mesmo verificou. Um frontend
//! comprometido não consegue fazer o app baixar ou instalar um arquivo arbitrário por aqui.
//!
//! No desktop os comandos existem (o `invoke_handler` é um só) e respondem "não suportado": lá a
//! atualização é do `tauri-plugin-updater`, com manifesto assinado.

use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::application::atualizacao_android::{
    baixar, conferir_antes_de_instalar, verificar, Novidade, PacoteBaixado,
};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};

#[derive(Default)]
struct Interno {
    novidade: Option<Novidade>,
    pacote: Option<PacoteBaixado>,
}

/// O que a verificação encontrou, guardado entre os passos.
#[derive(Default)]
pub struct EstadoAtualizacaoAndroid(Mutex<Interno>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificacaoAndroid {
    pub suportado: bool,
    pub versao_atual: String,
    pub novidade: Option<Novidade>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressoDoDownload {
    pub baixados: u64,
    pub total: u64,
    pub percentual: u8,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoDoInstalador {
    /// `instalador-aberto` ou `permissao-necessaria`.
    pub estado: String,
}

fn falha(motivo: impl std::fmt::Display) -> DatabaseCommandError {
    DatabaseCommandError::validation(motivo.to_string())
}

fn trancar<'a>(
    estado: &'a State<'_, EstadoAtualizacaoAndroid>,
) -> DatabaseCommandResult<std::sync::MutexGuard<'a, Interno>> {
    estado
        .0
        .lock()
        .map_err(|_| DatabaseCommandError::storage("o estado da atualização ficou inconsistente"))
}

/// Este aparelho usa a atualização por APK?
#[tauri::command]
pub fn android_update_supported() -> bool {
    cfg!(target_os = "android")
}

/// Consulta as GitHub Releases.
#[tauri::command]
pub async fn android_update_check(
    app: AppHandle,
    estado: State<'_, EstadoAtualizacaoAndroid>,
) -> DatabaseCommandResult<VerificacaoAndroid> {
    let versao_atual = app.package_info().version.to_string();
    if !cfg!(target_os = "android") {
        return Ok(VerificacaoAndroid {
            suportado: false,
            versao_atual,
            novidade: None,
        });
    }

    let novidade = verificar(&versao_atual).await.map_err(falha)?;
    {
        let mut interno = trancar(&estado)?;
        // Uma verificação nova invalida um download antigo de outra versão.
        if interno.novidade.as_ref().map(|n| &n.versao) != novidade.as_ref().map(|n| &n.versao) {
            interno.pacote = None;
        }
        interno.novidade = novidade.clone();
    }
    Ok(VerificacaoAndroid {
        suportado: true,
        versao_atual,
        novidade,
    })
}

/// Baixa a versão encontrada pela verificação, conferindo o SHA-256.
#[tauri::command]
pub async fn android_update_download(
    app: AppHandle,
    estado: State<'_, EstadoAtualizacaoAndroid>,
    progresso: Channel<ProgressoDoDownload>,
) -> DatabaseCommandResult<String> {
    if !cfg!(target_os = "android") {
        return Err(falha("A atualização por APK existe somente no Android."));
    }
    let novidade = trancar(&estado)?.novidade.clone().ok_or_else(|| {
        falha("Nenhuma versão nova foi encontrada. Verifique as atualizações antes de baixar.")
    })?;

    let pasta = app
        .path()
        .app_cache_dir()
        .map_err(|e| DatabaseCommandError::storage(e.to_string()))?
        .join("atualizacao");
    // Downloads antigos não ficam ocupando espaço: a pasta guarda só o que está em curso.
    let _ = std::fs::remove_dir_all(&pasta);

    let versao_atual = app.package_info().version.to_string();
    let mut ultimo_percentual: Option<u8> = None;
    let pacote = baixar(&novidade, &versao_atual, &pasta, |baixados, total| {
        let percentual = (baixados.min(total) * 100).checked_div(total).unwrap_or(0) as u8;
        // Um evento por ponto percentual, não por bloco: um APK de 60 MB em blocos de 16 KB
        // seriam milhares de mensagens atravessando o IPC para a mesma barra.
        if ultimo_percentual != Some(percentual) {
            ultimo_percentual = Some(percentual);
            let _ = progresso.send(ProgressoDoDownload {
                baixados,
                total,
                percentual,
            });
        }
    })
    .await
    .map_err(falha)?;

    let versao = pacote.versao.clone();
    trancar(&estado)?.pacote = Some(pacote);
    Ok(versao)
}

/// Confere o APK baixado de novo e abre o instalador do Android.
#[tauri::command]
pub async fn android_update_install(
    app: AppHandle,
    estado: State<'_, EstadoAtualizacaoAndroid>,
) -> DatabaseCommandResult<EstadoDoInstalador> {
    let pacote = trancar(&estado)?
        .pacote
        .clone()
        .ok_or_else(|| falha("Nenhuma atualização foi baixada ainda."))?;

    conferir_antes_de_instalar(&pacote.caminho, &pacote.sha256).map_err(falha)?;
    abrir_instalador(&app, &pacote)
}

#[cfg(target_os = "android")]
fn abrir_instalador(
    app: &AppHandle,
    pacote: &PacoteBaixado,
) -> DatabaseCommandResult<EstadoDoInstalador> {
    let instalador = app.state::<Instalador>();
    instalador
        .0
        .run_mobile_plugin::<EstadoDoInstalador>(
            "abrirInstalador",
            serde_json::json!({ "caminho": pacote.caminho.to_string_lossy() }),
        )
        .map_err(|e| falha(format!("O instalador do Android não pôde ser aberto: {e}")))
}

#[cfg(not(target_os = "android"))]
fn abrir_instalador(
    _app: &AppHandle,
    _pacote: &PacoteBaixado,
) -> DatabaseCommandResult<EstadoDoInstalador> {
    Err(falha("A atualização por APK existe somente no Android."))
}

/// A ponte com o `InstaladorPlugin.kt`.
#[cfg(target_os = "android")]
pub struct Instalador(tauri::plugin::PluginHandle<tauri::Wry>);

/// Registra o plugin nativo que abre o instalador. No desktop é um plugin vazio.
pub fn plugin_instalador() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("narrahub-instalador")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle =
                    _api.register_android_plugin("com.narrahub.app", "InstaladorPlugin")?;
                _app.manage(Instalador(handle));
            }
            Ok(())
        })
        .build()
}
