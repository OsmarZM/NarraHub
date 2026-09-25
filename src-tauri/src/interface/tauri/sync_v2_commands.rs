//! A primeira porta do Sync V2 para o frontend (etapa 14, fatia 2).
//!
//! ## O que existia antes disto
//!
//! Nada. O `invoke_handler` registrava 108 comandos e nenhum era do V2 — treze
//! etapas de motor sem um único chamador de produto. O que o usuário alcançava
//! ao sincronizar era o **V1** (`src-tauri/src/sync.rs`), que copia 17 tabelas
//! inteiras sem passar pelo log de eventos.
//!
//! ## As duas regras desta fronteira
//!
//! ```text
//! 1  `device_id` NUNCA e parametro de entrada
//! 2  nenhum caminho de arquivo atravessa
//! ```
//!
//! A primeira é a invariante das etapas 2.5 e 8 chegando à fronteira: quem
//! responde "que aparelho é este" é a identidade Ed25519 em arquivo, via
//! [`super::sync_identity`]. Um `device_id` recebido de fora seria um `&str`
//! escolhido pelo chamador, e o roster autoriza por identidade — não por
//! afirmação. A etapa 8 existe inteira por causa disso.
//!
//! A segunda é a mesma do `blob_commands`: o mesmo acervo tem que funcionar no
//! Windows e no Android, e caminho local é exatamente o que não viaja.
//!
//! ## O V1 não é tocado
//!
//! Decisão registrada: congelar e substituir, **sem coexistir**. Estes comandos
//! não chamam nada do V1, não leem o estado dele e não constroem
//! interoperabilidade. Um acervo com parte das escritas vindas do snapshot do
//! V1 e parte da causalidade do V2 teria estado cuja origem o V2 não consegue
//! explicar.

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::application::sync_panorama::{panorama, Panorama};
use crate::application::sync_qr_pin;
use crate::application::sync_sessao::{
    atender_conexao, parear_por_pin, sincronizar_com, Contexto, ResultadoDaSessao,
};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::infrastructure::sync_pake::{Codigos, VALIDADE};

/// Quanto uma sessão pode ficar sem resposta antes de desistir.
///
/// Mais folgado que o `ESPERA_PADRAO` do fio porque um bootstrap num celular
/// modesto passa segundos semeando antes de responder.
const ESPERA_DA_SESSAO: Duration = Duration::from_secs(60);

/// Evento que a escuta emite ao terminar cada sessão que ela atendeu (I-BUG-03).
///
/// Quem escuta não chamou comando nenhum: a sessão chega pela rede, grava no banco e, sem este
/// aviso, a tela continua mostrando o acervo de antes até o app ser reaberto.
pub const EVENTO_SESSAO_ATENDIDA: &str = "sync-v2-sessao-atendida";

/// O que a tela recebe quando uma sessão atendida termina.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessaoAtendida {
    pub resultado: Option<ResultadoDaSessao>,
    pub erro: Option<String>,
}

/// O estado do Sync V2 neste aparelho.
///
/// Só leitura. A escuta e o pareamento entram na fatia 3, onde existe
/// transporte para elas conversarem — uma escuta que aceita conexão e fecha
/// seria porta pintada na parede.
///
/// Sem parâmetro nenhum, e isso é o contrato: não há o que um chamador possa
/// afirmar sobre quem ele é.
#[tauri::command]
pub fn sync_v2_panorama(app: AppHandle) -> DatabaseCommandResult<Panorama> {
    panorama(&super::database(&app)?, &super::sync_identity(&app)?)
}

// ═══════════════════════════════════════════════════════════════════════════
// Escuta, PIN, pareamento e sincronização (fatia 4)
// ═══════════════════════════════════════════════════════════════════════════

/// O que a tela vê da escuta.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoDaEscutaV2 {
    pub escutando: bool,
    pub porta: Option<u16>,
    /// Para digitar no outro aparelho. Sem descoberta automática nesta etapa.
    pub enderecos: Vec<String>,
    /// `1234 5678`, ou nada quando não há código valendo.
    pub pin: Option<String>,
    pub ultimo_resultado: Option<ResultadoDaSessao>,
    pub ultimo_erro: Option<String>,
}

struct EscutaAtiva {
    porta: u16,
    parar: Arc<AtomicBool>,
    codigos: Arc<Mutex<Codigos>>,
    pin: Option<(String, Instant)>,
}

#[derive(Default)]
struct Interno {
    escuta: Option<EscutaAtiva>,
    ultimo_resultado: Option<ResultadoDaSessao>,
    ultimo_erro: Option<String>,
}

/// O estado do Sync V2 que o Tauri gerencia.
///
/// **Não é o estado do V1**, e não conversa com ele: a decisão é congelar e
/// substituir, sem coexistência. A trava que impede os dois ativos juntos fica
/// na tela.
#[derive(Default, Clone)]
pub struct EstadoV2(Arc<Mutex<Interno>>);

fn trancar(estado: &EstadoV2) -> DatabaseCommandResult<std::sync::MutexGuard<'_, Interno>> {
    estado
        .0
        .lock()
        .map_err(|_| DatabaseCommandError::storage("o estado da sincronização ficou inconsistente"))
}

fn falha(motivo: impl std::fmt::Display) -> DatabaseCommandError {
    DatabaseCommandError::validation(motivo.to_string())
}

/// O endereço desta máquina na rede local.
///
/// `connect` num socket UDP **não envia pacote nenhum**: só pede ao sistema a
/// rota, e com ela o endereço de saída. É o jeito portável de descobrir o IP
/// da interface certa sem enumerar placas — e funciona igual no Windows e no
/// Android.
fn enderecos_locais(porta: u16) -> Vec<String> {
    let descoberto = UdpSocket::bind(("0.0.0.0", 0))
        .and_then(|socket| {
            socket.connect(("8.8.8.8", 53))?;
            socket.local_addr()
        })
        .ok()
        .map(|endereco| endereco.ip())
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified());
    descoberto
        .map(|ip| vec![format!("{ip}:{porta}")])
        .unwrap_or_default()
}

fn retrato(interno: &Interno) -> EstadoDaEscutaV2 {
    let (escutando, porta, pin) = match &interno.escuta {
        Some(escuta) => {
            // O PIN só aparece enquanto ainda vale: dentro da validade e não
            // consumido por um pareamento que deu certo.
            let aberto = escuta
                .codigos
                .lock()
                .map(|codigos| codigos.unico_aberto().is_ok())
                .unwrap_or(false);
            let pin = escuta
                .pin
                .as_ref()
                .filter(|(_, emitido)| aberto && emitido.elapsed() <= VALIDADE)
                .map(|(legivel, _)| legivel.clone());
            (true, Some(escuta.porta), pin)
        }
        None => (false, None, None),
    };
    EstadoDaEscutaV2 {
        escutando,
        porta,
        enderecos: porta.map(enderecos_locais).unwrap_or_default(),
        pin,
        ultimo_resultado: interno.ultimo_resultado.clone(),
        ultimo_erro: interno.ultimo_erro.clone(),
    }
}

fn contexto_de<'a>(
    database: &'a crate::infrastructure::sqlite::SqliteDatabase,
    store: &'a crate::infrastructure::blob_store::BlobStore,
    identidade: &'a crate::domain::identity::DeviceIdentity,
    nome: &'a str,
) -> Contexto<'a> {
    Contexto {
        database,
        store,
        identidade,
        nome_local: nome,
        espera: ESPERA_DA_SESSAO,
    }
}

/// Estado da escuta, para a tela.
#[tauri::command]
pub fn sync_v2_estado(estado: State<'_, EstadoV2>) -> DatabaseCommandResult<EstadoDaEscutaV2> {
    Ok(retrato(&*trancar(&estado)?))
}

/// Abre a escuta e emite um PIN.
///
/// Uma conexão por vez, na mesma thread: duas sessões simultâneas no mesmo
/// banco disputariam o `IMMEDIATE` do `semear` e do `receber_eventos`, e a
/// segunda perderia por `busy` no meio do protocolo. Sequencial é o
/// conservador.
#[tauri::command]
pub fn sync_v2_escuta_iniciar(
    app: AppHandle,
    estado: State<'_, EstadoV2>,
    porta: u16,
    nome: String,
) -> DatabaseCommandResult<EstadoDaEscutaV2> {
    let mut interno = trancar(&estado)?;
    if interno.escuta.is_some() {
        return Ok(retrato(&interno));
    }

    let escuta = TcpListener::bind(("0.0.0.0", porta)).map_err(|erro| {
        falha(format!(
            "A porta {porta} não pôde ser aberta ({erro}). Outro programa pode estar usando."
        ))
    })?;
    let porta_real = escuta.local_addr().map_err(falha)?.port();

    let mut codigos = Codigos::default();
    let (_, legivel) = codigos.emitir();
    let codigos = Arc::new(Mutex::new(codigos));
    let parar = Arc::new(AtomicBool::new(false));

    let para_thread = (
        app.clone(),
        estado.inner().clone(),
        Arc::clone(&codigos),
        Arc::clone(&parar),
        nome.clone(),
    );
    std::thread::Builder::new()
        .name("sync-v2-escuta".into())
        .spawn(move || {
            let (app, estado, codigos, parar, nome) = para_thread;
            for conexao in escuta.incoming() {
                // Parar acorda o `accept` com uma conexão de si mesmo; a
                // checagem vem antes de atender, para essa conexão não virar
                // sessão.
                if parar.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut fluxo) = conexao else { continue };
                let resultado = atender(&app, &codigos, &nome, &mut fluxo);
                let aviso = match &resultado {
                    Ok(r) => SessaoAtendida {
                        resultado: Some(r.clone()),
                        erro: None,
                    },
                    Err(erro) => SessaoAtendida {
                        resultado: None,
                        erro: Some(erro.message.clone()),
                    },
                };
                if let Ok(mut interno) = estado.0.lock() {
                    match resultado {
                        Ok(r) => {
                            interno.ultimo_resultado = Some(r);
                            interno.ultimo_erro = None;
                        }
                        Err(erro) => interno.ultimo_erro = Some(erro.message),
                    }
                }
                // Sem janela para ouvir, o aviso se perde e o banco continua certo: falhar aqui
                // não pode derrubar a escuta.
                let _ = app.emit(EVENTO_SESSAO_ATENDIDA, aviso);
            }
        })
        .map_err(falha)?;

    interno.escuta = Some(EscutaAtiva {
        porta: porta_real,
        parar,
        codigos,
        pin: Some((legivel, Instant::now())),
    });
    Ok(retrato(&interno))
}

fn atender(
    app: &AppHandle,
    codigos: &Arc<Mutex<Codigos>>,
    nome: &str,
    fluxo: &mut TcpStream,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let database = super::database(app)?;
    let store = super::blob_store(app)?;
    let identidade = super::sync_identity(app)?;
    let mut codigos = codigos
        .lock()
        .map_err(|_| DatabaseCommandError::storage("códigos de pareamento inconsistentes"))?;
    atender_conexao(
        fluxo,
        &mut codigos,
        &contexto_de(&database, &store, &identidade, nome),
    )
}

/// Encerra a escuta. O PIN aberto deixa de valer junto.
#[tauri::command]
pub fn sync_v2_escuta_parar(
    estado: State<'_, EstadoV2>,
) -> DatabaseCommandResult<EstadoDaEscutaV2> {
    let mut interno = trancar(&estado)?;
    if let Some(escuta) = interno.escuta.take() {
        escuta.parar.store(true, Ordering::SeqCst);
        // Acorda o `accept` bloqueado. Sem isso a thread só perceberia o
        // pedido de parar na próxima conexão de verdade, e a porta ficaria
        // ocupada até lá.
        let _ = TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], escuta.porta)),
            Duration::from_secs(1),
        );
    }
    Ok(retrato(&interno))
}

/// Troca o código aberto por um novo. O anterior deixa de valer.
#[tauri::command]
pub fn sync_v2_pin_novo(estado: State<'_, EstadoV2>) -> DatabaseCommandResult<EstadoDaEscutaV2> {
    let mut interno = trancar(&estado)?;
    let Some(escuta) = interno.escuta.as_mut() else {
        return Err(falha(
            "A escuta está desligada. Ligue a escuta para gerar um código.",
        ));
    };
    let legivel = {
        let mut codigos = escuta
            .codigos
            .lock()
            .map_err(|_| DatabaseCommandError::storage("códigos de pareamento inconsistentes"))?;
        // Um código de cada vez: o anfitrião resolve o PIN pelo único aberto,
        // e dois abertos transformariam três tentativas em seis.
        *codigos = Codigos::default();
        codigos.emitir().1
    };
    escuta.pin = Some((legivel, Instant::now()));
    Ok(retrato(&interno))
}

/// O conteúdo do QR da escuta aberta — pareamento por PIN assistido por QR (NH-084, PR B).
///
/// `None` quando não há o que mostrar: escuta fechada, código vencido ou já usado, ou nenhum endereço
/// local. O QR só existe enquanto o PIN vale; quem decide isso é a escuta, e é ela que recusa depois.
/// O conteúdo carrega o PIN: nunca vai para log.
#[tauri::command]
pub fn sync_v2_qr(estado: State<'_, EstadoV2>) -> DatabaseCommandResult<Option<String>> {
    let interno = trancar(&estado)?;
    let retrato = retrato(&interno);
    let (Some(pin), Some(endereco)) = (retrato.pin, retrato.enderecos.first()) else {
        return Ok(None);
    };
    let digitos: String = pin.chars().filter(char::is_ascii_digit).collect();
    Ok(sync_qr_pin::montar(endereco, &digitos).ok())
}

/// Pareia pelo texto cru que o leitor de QR devolveu. O Rust interpreta; conteúdo recusado não chega
/// à rede. Depois disso é o pareamento por PIN, sem atalho nenhum.
#[tauri::command]
pub async fn sync_v2_parear_por_qr(
    app: AppHandle,
    estado: State<'_, EstadoV2>,
    conteudo: String,
    nome: String,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let app_da_sessao = app.clone();
    let resultado = tauri::async_runtime::spawn_blocking(move || {
        let database = super::database(&app_da_sessao)?;
        let store = super::blob_store(&app_da_sessao)?;
        let identidade = super::sync_identity(&app_da_sessao)?;
        sync_qr_pin::parear_por_qr(
            &conteudo,
            &contexto_de(&database, &store, &identidade, &nome),
        )
    })
    .await
    .map_err(|erro| DatabaseCommandError::storage(erro.to_string()))?;
    registrar(&estado, &resultado)?;
    resultado
}

/// Pareia com o aparelho que mostra o PIN. Pode terminar em bootstrap.
///
/// `async` com `spawn_blocking`: a sessão faz rede e pode levar dezenas de
/// segundos num bootstrap, e um comando síncrono travaria a janela inteira.
#[tauri::command]
pub async fn sync_v2_parear(
    app: AppHandle,
    estado: State<'_, EstadoV2>,
    endereco: String,
    pin: String,
    nome: String,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let app_da_sessao = app.clone();
    let resultado = tauri::async_runtime::spawn_blocking(move || {
        let database = super::database(&app_da_sessao)?;
        let store = super::blob_store(&app_da_sessao)?;
        let identidade = super::sync_identity(&app_da_sessao)?;
        parear_por_pin(
            &endereco,
            &pin,
            &contexto_de(&database, &store, &identidade, &nome),
        )
    })
    .await
    .map_err(|erro| DatabaseCommandError::storage(erro.to_string()))?;
    registrar(&estado, &resultado)?;
    resultado
}

/// Sincroniza com um aparelho já pareado.
#[tauri::command]
pub async fn sync_v2_sincronizar(
    app: AppHandle,
    estado: State<'_, EstadoV2>,
    endereco: String,
    nome: String,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let app_da_sessao = app.clone();
    let resultado = tauri::async_runtime::spawn_blocking(move || {
        let database = super::database(&app_da_sessao)?;
        let store = super::blob_store(&app_da_sessao)?;
        let identidade = super::sync_identity(&app_da_sessao)?;
        sincronizar_com(
            &endereco,
            &contexto_de(&database, &store, &identidade, &nome),
        )
    })
    .await
    .map_err(|erro| DatabaseCommandError::storage(erro.to_string()))?;
    registrar(&estado, &resultado)?;
    resultado
}

fn registrar(
    estado: &EstadoV2,
    resultado: &DatabaseCommandResult<ResultadoDaSessao>,
) -> DatabaseCommandResult<()> {
    let mut interno = trancar(estado)?;
    match resultado {
        Ok(r) => {
            interno.ultimo_resultado = Some(r.clone());
            interno.ultimo_erro = None;
        }
        Err(erro) => interno.ultimo_erro = Some(erro.message.clone()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// **Nenhum comando desta fronteira recebe `device_id`.**
    ///
    /// Gate textual, e é o tipo de gate que se justifica: a propriedade é
    /// sobre a *assinatura* dos comandos, e um teste de comportamento não
    /// alcança um parâmetro que não deveria existir. Ele falha no momento em
    /// que alguém acrescentar `device_id: String` a qualquer comando daqui —
    /// que é exatamente o atalho tentador quando a tela "já sabe" o id.
    ///
    /// O que a tela sabe é o que este módulo contou a ela. Deixar isso voltar
    /// como entrada transformaria identidade em parâmetro, e a etapa 8 existe
    /// para que não seja.
    #[test]
    fn nenhum_comando_recebe_device_id_como_parametro() {
        let fonte = include_str!("sync_v2_commands.rs");

        // Recorta só o que está antes do módulo de teste: as strings deste
        // próprio gate contêm "device_id" e passariam por ocorrência real.
        let codigo = fonte
            .split_once("#[cfg(test)]")
            .map(|(antes, _)| antes)
            .expect("o módulo de teste tinha que existir");

        // Só as ASSINATURAS dos comandos: do `#[tauri::command]` até a primeira
        // chave. A primeira versão varria o arquivo inteiro e reprovou por um
        // caminho de tipo — `crate::domain::identity::DeviceIdentity` numa
        // função auxiliar, que nem é comando. A propriedade é sobre o que o
        // CHAMADOR pode passar, e isso mora na assinatura.
        let assinaturas: Vec<&str> = codigo
            .split("#[tauri::command]")
            .skip(1)
            .map(|resto| resto.split_once('{').map(|(sig, _)| sig).unwrap_or(resto))
            .collect();

        assert!(
            assinaturas.len() >= 7,
            "a varredura achou {} comandos; esperava os sete do V2. O gate perdeu o alvo.",
            assinaturas.len()
        );

        for assinatura in &assinaturas {
            for proibido in [
                "device_id",
                "deviceId",
                "identity",
                "identidade",
                "public_key",
                "chave_publica",
                "DeviceIdentity",
            ] {
                assert!(
                    !assinatura.contains(proibido),
                    "um comando passou a receber `{proibido}` de fora:\n{assinatura}\n\
                     Identidade não é parâmetro: quem responde quem este aparelho é são a \
                     chave Ed25519 em arquivo e o roster, nunca o chamador."
                );
            }
        }
    }

    /// E nenhum caminho de arquivo atravessa.
    ///
    /// Mesma regra do `blob_commands`, pelo mesmo motivo: caminho local não
    /// viaja entre Windows e Android.
    #[test]
    fn nenhum_comando_recebe_nem_devolve_caminho() {
        let fonte = include_str!("sync_v2_commands.rs");
        let codigo = fonte
            .split_once("#[cfg(test)]")
            .map(|(antes, _)| antes)
            .expect("o módulo de teste tinha que existir");

        for proibido in ["PathBuf", "path:", "caminho:", "&Path"] {
            assert!(
                !codigo.contains(proibido),
                "`{proibido}` apareceu na fronteira. O mesmo acervo tem que funcionar nos \
                 dois sistemas, e caminho local é o que não viaja."
            );
        }
    }

    /// **Esta fronteira não fala com o Sync V1.**
    ///
    /// A decisão é congelar e substituir, sem coexistência. O gate existe
    /// porque o atalho — "aproveita o `SyncState` que já está lá" — é o que
    /// produziria um acervo escrito pelos dois mecanismos, com estado que o V2
    /// não consegue explicar.
    #[test]
    fn a_fronteira_do_v2_nao_toca_no_v1() {
        let fonte = include_str!("sync_v2_commands.rs");
        let codigo = fonte
            .split_once("#[cfg(test)]")
            .map(|(antes, _)| antes)
            .expect("o módulo de teste tinha que existir");

        for proibido in ["crate::sync::", "SyncState", "sync_start", "sync_connect"] {
            assert!(
                !codigo.contains(proibido),
                "`{proibido}` é do Sync V1, que saiu do runtime na etapa G: nada se apoia nele, e \
                 não existe interoperabilidade V1 <-> V2."
            );
        }
    }
}
