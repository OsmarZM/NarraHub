//! A sessão de sincronização sobre o fio (etapa 14, fatia 4).
//!
//! ## O que ela costura
//!
//! ```text
//! fatia 1   sync_wire          quadros e mensagens cifradas sobre TCP
//! fatia 3   sync_pin_pairing   PIN -> SPAKE2 -> XXpsk0 -> prova Ed25519
//! etapa 12  sync_snapshot      capturar / semear, atomicos
//! etapa 13  blob_backfill      manifesto e transferencia verificada por SHA
//! etapa 6   sync_exchange      vetor e eventos_para
//! etapa 5   sync_session       receber_eventos, com cursor contiguo
//! ```
//!
//! **Nenhum desses contratos muda aqui.** Este módulo decide só a ordem das
//! mensagens de aplicação e quem fala quando.
//!
//! ## Os dois modos de conexão
//!
//! O primeiro quadro, ainda em claro, diz o que o visitante quer:
//!
//! ```text
//! MODO_PIN       pareamento por PIN; pode terminar em bootstrap
//! MODO_PAREADO   dois aparelhos que ja se conhecem; XX sem psk, e quem
//!                autoriza e o ROSTER de cada lado
//! ```
//!
//! O modo em claro não é segredo nem autoridade: quem autoriza é o PIN num
//! caso e o roster no outro, e um modo falsificado só leva o outro lado a
//! exigir a prova que o visitante não tem.
//!
//! ## Depois do PIN: quem doa e quem recebe
//!
//! Cada lado pergunta ao próprio banco se está fresco
//! ([`sync_snapshot::receptor_elegivel`]) e conta ao outro. A decisão é
//! determinística e os dois chegam à mesma:
//!
//! ```text
//! eu fresco   ele fresco   papel
//! ---------   ----------   -----------------------------------------------
//!    sim         nao       RECEPTOR  nao admite; recebe bundle, blobs, semeia
//!    nao         sim       DOADOR    admite; captura, envia, serve blobs
//!    sim         sim       PAR       os dois admitem; nada a semear
//!    nao         nao       PAR       os dois admitem; incremental
//! ```
//!
//! **O receptor fresco não admite** — decisão registrada. A etapa 12 exige
//! receptor com só o `self` no roster, e o `semear` traz o doador pelo merge
//! do roster do bundle. Admitir antes tornaria o bootstrap impossível, e isso
//! foi medido.
//!
//! ## Lock-step, e por quê
//!
//! Toda troca é "um fala, o outro responde". Com os dois escrevendo ao mesmo
//! tempo, duas mensagens grandes podem encher a janela do TCP nos dois sentidos
//! e travar a sessão com os dois lados esperando. O visitante sempre fala
//! primeiro.
//!
//! ## O incremental termina por falta de progresso, nunca por repetição
//!
//! Quem envia manda um lote; quem recebe aplica e devolve o vetor novo. Se o
//! vetor não andou — evento que ficou pendente por lacuna —, reenviar o mesmo
//! lote produziria o mesmo vetor para sempre. Sem progresso, a fase acaba; o
//! pendente fica no log e a próxima sessão tenta de novo, que é o contrato da
//! etapa 5.
//!
//! Não toca no Sync V1.

use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::application::sync_pin_pairing::{
    anfitriao_autentica, anfitriao_autentica_pareado, conectar, visitante_autentica,
    visitante_autentica_pareado, Parceiro, SessaoPareada,
};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::domain::sync::EventEnvelope;
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::sync_snapshot::{self, Categoria};
use crate::infrastructure::sqlite::{blob_backfill, sync_exchange, sync_session, SqliteDatabase};
use crate::infrastructure::sync_bundle_wire::{de_fio, para_fio, BundleNoFio};
use crate::infrastructure::sync_pake::Codigos;
use crate::infrastructure::sync_transport::Transporte;
use crate::infrastructure::sync_wire::{
    escrever_mensagem, escrever_quadro, ler_mensagem, ler_quadro,
};

/// Primeiro quadro de uma conexão de pareamento por PIN.
pub const MODO_PIN: &[u8] = b"narrahub.sync.v2.modo.pin";
/// Primeiro quadro de uma conexão entre aparelhos já pareados.
pub const MODO_PAREADO: &[u8] = b"narrahub.sync.v2.modo.pareado";

/// O que um aparelho precisa para participar de uma sessão.
pub struct Contexto<'a> {
    pub database: &'a SqliteDatabase,
    pub store: &'a BlobStore,
    pub identidade: &'a DeviceIdentity,
    pub nome_local: &'a str,
    pub espera: Duration,
}

/// O papel que este lado teve na sessão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Papel {
    Doador,
    Receptor,
    Par,
}

/// O que a sessão fez, para a tela e para os gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultadoDaSessao {
    pub parceiro: Parceiro,
    pub papel: Papel,
    /// Bootstrap aconteceu nesta sessão (como doador ou receptor).
    pub houve_bootstrap: bool,
    /// Blobs que chegaram a este lado e conferiram o SHA.
    pub blobs_recebidos: usize,
    /// Eventos que este lado mandou.
    pub eventos_enviados: usize,
    /// Eventos que este lado aplicou.
    pub eventos_aplicados: usize,
    /// Eventos recebidos que ficaram pendentes por lacuna.
    pub eventos_pendentes: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "dados")]
enum Mensagem {
    Estado {
        fresco: bool,
    },
    Autorizacao {
        aceito: bool,
        motivo: String,
    },
    Bundle(Box<BundleNoFio>),
    BootstrapImpossivel {
        motivo: String,
    },
    PedirBlob {
        hash: String,
    },
    /// A próxima mensagem são os bytes crus do blob.
    BlobSegue {
        hash: String,
    },
    BlobAusente {
        hash: String,
    },
    FimDosBlobs,
    Semeado {
        ok: bool,
        motivo: String,
    },
    Vetor(BTreeMap<String, i64>),
    Eventos(Vec<EventEnvelope>),
}

fn falha(motivo: impl std::fmt::Display) -> DatabaseCommandError {
    DatabaseCommandError::validation(motivo.to_string())
}

struct Canal<'a> {
    transporte: Transporte,
    fluxo: &'a mut TcpStream,
}

impl Canal<'_> {
    fn enviar(&mut self, mensagem: &Mensagem) -> DatabaseCommandResult<()> {
        let bytes = serde_json::to_vec(mensagem)
            .map_err(|erro| DatabaseCommandError::storage(erro.to_string()))?;
        escrever_mensagem(&mut self.transporte, self.fluxo, &bytes).map_err(falha)
    }

    fn receber(&mut self) -> DatabaseCommandResult<Mensagem> {
        let bytes = ler_mensagem(&mut self.transporte, self.fluxo).map_err(falha)?;
        serde_json::from_slice(&bytes)
            .map_err(|_| falha("O outro aparelho mandou uma mensagem que este não reconhece."))
    }

    fn enviar_bytes(&mut self, bytes: &[u8]) -> DatabaseCommandResult<()> {
        escrever_mensagem(&mut self.transporte, self.fluxo, bytes).map_err(falha)
    }

    fn receber_bytes(&mut self) -> DatabaseCommandResult<Vec<u8>> {
        ler_mensagem(&mut self.transporte, self.fluxo).map_err(falha)
    }

    /// Lock-step: quem fala primeiro envia e depois lê; o outro, o contrário.
    fn trocar(&mut self, minha: &Mensagem, falo_primeiro: bool) -> DatabaseCommandResult<Mensagem> {
        if falo_primeiro {
            self.enviar(minha)?;
            self.receber()
        } else {
            let dele = self.receber()?;
            self.enviar(minha)?;
            Ok(dele)
        }
    }
}

fn inesperada(esperada: &str, veio: &Mensagem) -> DatabaseCommandError {
    falha(format!(
        "O outro aparelho saiu do protocolo: esperava {esperada}, veio {veio:?}."
    ))
}

// ═══════════════════════════════════════════════════════════════════════════
// Pontas de entrada
// ═══════════════════════════════════════════════════════════════════════════

/// Atende uma conexão já aceita, em qualquer um dos dois modos.
pub fn atender_conexao(
    fluxo: &mut TcpStream,
    codigos: &mut Codigos,
    ctx: &Contexto<'_>,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    crate::infrastructure::sync_wire::ajustar_esperas(fluxo, ctx.espera).map_err(falha)?;
    let modo = ler_quadro(fluxo).map_err(falha)?;
    if modo == MODO_PIN {
        let pareada =
            anfitriao_autentica(fluxo, codigos, ctx.identidade, ctx.nome_local, ctx.espera)?;
        depois_do_pin(pareada, fluxo, ctx, false)
    } else if modo == MODO_PAREADO {
        let pareada =
            anfitriao_autentica_pareado(fluxo, ctx.identidade, ctx.nome_local, ctx.espera)?;
        sessao_pareada(pareada, fluxo, ctx, false)
    } else {
        Err(falha("Conexão com modo desconhecido. Nada foi trocado."))
    }
}

/// Conecta no endereço digitado e pareia por PIN; faz bootstrap se couber.
pub fn parear_por_pin(
    endereco: &str,
    pin: &str,
    ctx: &Contexto<'_>,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let mut fluxo = conectar(endereco)?;
    escrever_quadro(&mut fluxo, MODO_PIN).map_err(falha)?;
    let pareada = visitante_autentica(&mut fluxo, pin, ctx.identidade, ctx.nome_local, ctx.espera)?;
    depois_do_pin(pareada, &mut fluxo, ctx, true)
}

/// Sincroniza com um aparelho já pareado.
pub fn sincronizar_com(
    endereco: &str,
    ctx: &Contexto<'_>,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let mut fluxo = conectar(endereco)?;
    escrever_quadro(&mut fluxo, MODO_PAREADO).map_err(falha)?;
    let pareada =
        visitante_autentica_pareado(&mut fluxo, ctx.identidade, ctx.nome_local, ctx.espera)?;
    sessao_pareada(pareada, &mut fluxo, ctx, true)
}

// ═══════════════════════════════════════════════════════════════════════════
// Depois do PIN
// ═══════════════════════════════════════════════════════════════════════════

fn fresco(database: &SqliteDatabase) -> DatabaseCommandResult<bool> {
    let mut connection = database.write()?;
    sync_snapshot::receptor_elegivel(&mut connection)
}

fn papel_de(eu_fresco: bool, ele_fresco: bool) -> Papel {
    match (eu_fresco, ele_fresco) {
        (true, false) => Papel::Receptor,
        (false, true) => Papel::Doador,
        _ => Papel::Par,
    }
}

fn depois_do_pin(
    pareada: SessaoPareada,
    fluxo: &mut TcpStream,
    ctx: &Contexto<'_>,
    falo_primeiro: bool,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let parceiro = pareada.parceiro();
    let SessaoPareada {
        transporte,
        sessao,
        nome_do_outro,
    } = pareada;
    let mut canal = Canal { transporte, fluxo };

    let eu_fresco = fresco(ctx.database)?;
    let ele_fresco = match canal.trocar(&Mensagem::Estado { fresco: eu_fresco }, falo_primeiro)? {
        Mensagem::Estado { fresco } => fresco,
        outra => return Err(inesperada("Estado", &outra)),
    };
    let papel = papel_de(eu_fresco, ele_fresco);

    let mut resultado = ResultadoDaSessao {
        parceiro,
        papel,
        houve_bootstrap: false,
        blobs_recebidos: 0,
        eventos_enviados: 0,
        eventos_aplicados: 0,
        eventos_pendentes: 0,
    };

    let para_admitir = |database: &SqliteDatabase| -> DatabaseCommandResult<()> {
        let connection = database.write()?;
        crate::infrastructure::sqlite::sync_trust::admitir_por_pareamento(
            &connection,
            &sessao,
            &nome_do_outro,
        )
    };

    match papel {
        Papel::Receptor => {
            // NÃO admite. Ver o cabeçalho.
            resultado.blobs_recebidos = receber_bootstrap(&mut canal, ctx)?;
            resultado.houve_bootstrap = true;
        }
        Papel::Doador => {
            para_admitir(ctx.database)?;
            doar_bootstrap(&mut canal, ctx)?;
            resultado.houve_bootstrap = true;
        }
        Papel::Par => {
            para_admitir(ctx.database)?;
        }
    }

    // O incremental roda em todos os papéis. Depois de um bootstrap ele pega o
    // que o doador escreveu entre a captura e agora — que a etapa 12 garante
    // estar no incremental, e não entre os dois.
    incremental(&mut canal, ctx, falo_primeiro, &mut resultado)?;
    Ok(resultado)
}

// ═══════════════════════════════════════════════════════════════════════════
// Bootstrap
// ═══════════════════════════════════════════════════════════════════════════

fn doar_bootstrap(canal: &mut Canal<'_>, ctx: &Contexto<'_>) -> DatabaseCommandResult<()> {
    let captura = {
        let mut connection = ctx.database.write()?;
        sync_snapshot::capturar(&mut connection)?
    };
    let bundle = match captura {
        Ok(bundle) => bundle,
        Err(motivo) => {
            let texto = format!("{motivo:?}");
            canal.enviar(&Mensagem::BootstrapImpossivel {
                motivo: texto.clone(),
            })?;
            return Err(falha(format!(
                "Este acervo não pode ser enviado agora: {texto}."
            )));
        }
    };

    canal.enviar(&Mensagem::Bundle(Box::new(para_fio(&bundle))))?;
    servir_blobs(canal, ctx)?;

    match canal.receber()? {
        Mensagem::Semeado { ok: true, .. } => Ok(()),
        Mensagem::Semeado { ok: false, motivo } => Err(falha(format!(
            "O outro aparelho recebeu o acervo e não conseguiu gravá-lo: {motivo}."
        ))),
        outra => Err(inesperada("Semeado", &outra)),
    }
}

fn receber_bootstrap(canal: &mut Canal<'_>, ctx: &Contexto<'_>) -> DatabaseCommandResult<usize> {
    let fio = match canal.receber()? {
        Mensagem::Bundle(fio) => *fio,
        Mensagem::BootstrapImpossivel { motivo } => {
            return Err(falha(format!(
                "O outro aparelho não pode enviar o acervo agora: {motivo}."
            )))
        }
        outra => return Err(inesperada("Bundle", &outra)),
    };
    let bundle = de_fio(fio).map_err(falha)?;

    // Blobs PRIMEIRO, pela rede, cada um conferido pelo SHA no destino. É a
    // ordem da etapa 13: o bootstrap não se repete, e imagem que faltar agora
    // fica faltando para sempre.
    let resumo = pedir_blobs(canal, ctx.store, &bundle.blobs)?;

    let semeadura = {
        let mut connection = ctx.database.write()?;
        sync_snapshot::semear(&mut connection, ctx.store, ctx.identidade, &bundle)?
    };
    match semeadura {
        Ok(()) => {
            canal.enviar(&Mensagem::Semeado {
                ok: true,
                motivo: String::new(),
            })?;
            Ok(resumo)
        }
        Err(motivo) => {
            let texto = format!("{motivo:?}");
            canal.enviar(&Mensagem::Semeado {
                ok: false,
                motivo: texto.clone(),
            })?;
            Err(falha(format!(
                "O acervo recebido não pôde ser gravado: {texto}."
            )))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Blobs
// ═══════════════════════════════════════════════════════════════════════════

/// Pede cada hash que falta e publica só o que conferir.
///
/// A verificação é do `transferir_blobs` da etapa 13, que recebe a origem como
/// função exatamente para este caso: a rede é onde mentir é interessante. Um
/// peer que anuncia o hash da capa boa e manda outros bytes não publica nada.
fn pedir_blobs(
    canal: &mut Canal<'_>,
    store: &BlobStore,
    manifesto: &BTreeSet<String>,
) -> DatabaseCommandResult<usize> {
    let mut erro_de_rede: Option<DatabaseCommandError> = None;
    let resumo = {
        let buscar = |hash: &str| -> Result<Vec<u8>, String> {
            if erro_de_rede.is_some() {
                return Err("sessão interrompida".into());
            }
            let resposta = canal
                .enviar(&Mensagem::PedirBlob {
                    hash: hash.to_string(),
                })
                .and_then(|_| canal.receber());
            match resposta {
                Ok(Mensagem::BlobSegue { hash: veio }) if veio == hash => {
                    canal.receber_bytes().map_err(|erro| {
                        let texto = erro.message.clone();
                        erro_de_rede = Some(erro);
                        texto
                    })
                }
                Ok(Mensagem::BlobAusente { .. }) => Err("o outro aparelho não tem".into()),
                Ok(outra) => {
                    let erro = inesperada("BlobSegue", &outra);
                    let texto = erro.message.clone();
                    erro_de_rede = Some(erro);
                    Err(texto)
                }
                Err(erro) => {
                    let texto = erro.message.clone();
                    erro_de_rede = Some(erro);
                    Err(texto)
                }
            }
        };
        blob_backfill::transferir_blobs(buscar, store, manifesto)?
    };
    if let Some(erro) = erro_de_rede {
        return Err(erro);
    }
    canal.enviar(&Mensagem::FimDosBlobs)?;
    Ok(resumo.transferidos)
}

/// Responde pedidos de blob até o outro lado dizer que acabou.
fn servir_blobs(canal: &mut Canal<'_>, ctx: &Contexto<'_>) -> DatabaseCommandResult<()> {
    loop {
        match canal.receber()? {
            Mensagem::PedirBlob { hash } => match ctx.store.read(&hash) {
                // `read` confere o SHA antes de devolver: blob corrompido no
                // disco deste lado vira "ausente", e não bytes errados no outro.
                Ok(bytes) => {
                    canal.enviar(&Mensagem::BlobSegue { hash })?;
                    canal.enviar_bytes(&bytes)?;
                }
                Err(_) => canal.enviar(&Mensagem::BlobAusente { hash })?,
            },
            Mensagem::FimDosBlobs => return Ok(()),
            outra => return Err(inesperada("PedirBlob ou FimDosBlobs", &outra)),
        }
    }
}

/// Os blobs que este acervo referencia e este aparelho não tem verificados.
fn blobs_que_faltam(ctx: &Contexto<'_>) -> DatabaseCommandResult<BTreeSet<String>> {
    let manifesto = {
        let connection = ctx.database.read()?;
        blob_backfill::manifesto_de_blobs(
            &connection,
            &sync_snapshot::tabelas_de(Categoria::TransferidaNoBundle),
        )?
    };
    blob_backfill::blobs_faltantes(ctx.store, &manifesto)
}

// ═══════════════════════════════════════════════════════════════════════════
// Sessão entre pareados
// ═══════════════════════════════════════════════════════════════════════════

/// Confere, no roster DESTE aparelho, que o outro lado é membro ativo.
///
/// É o que impede a confiança de ser transitiva: uma sessão autenticada prova
/// quem é o outro, e só o roster diz se sincronizar com ele é permitido.
fn autorizado(
    database: &SqliteDatabase,
    device_id: &str,
) -> DatabaseCommandResult<Result<(), String>> {
    let connection = database.read()?;
    let estado: Option<String> = rusqlite::OptionalExtension::optional(connection.query_row(
        "SELECT state FROM sync_devices WHERE device_id = ?1 AND is_self = 0",
        [device_id],
        |row| row.get(0),
    ))
    .map_err(|erro| DatabaseCommandError::storage(erro.to_string()))?;
    Ok(match estado.as_deref() {
        Some("active") => Ok(()),
        Some(outro) => Err(format!("o aparelho está {outro} neste conjunto")),
        None => Err("o aparelho não está pareado com este".to_string()),
    })
}

fn sessao_pareada(
    pareada: SessaoPareada,
    fluxo: &mut TcpStream,
    ctx: &Contexto<'_>,
    falo_primeiro: bool,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let parceiro = pareada.parceiro();
    let mut canal = Canal {
        transporte: pareada.transporte,
        fluxo,
    };

    let minha = autorizado(ctx.database, &parceiro.device_id)?;
    let resposta = canal.trocar(
        &Mensagem::Autorizacao {
            aceito: minha.is_ok(),
            motivo: minha.clone().err().unwrap_or_default(),
        },
        falo_primeiro,
    )?;

    if let Err(motivo) = minha {
        return Err(falha(format!(
            "Sincronização recusada por este aparelho: {motivo}."
        )));
    }
    match resposta {
        Mensagem::Autorizacao { aceito: true, .. } => {}
        Mensagem::Autorizacao {
            aceito: false,
            motivo,
        } => {
            return Err(falha(format!(
                "O outro aparelho recusou a sincronização: {motivo}."
            )))
        }
        outra => return Err(inesperada("Autorizacao", &outra)),
    }

    let mut resultado = ResultadoDaSessao {
        parceiro,
        papel: Papel::Par,
        houve_bootstrap: false,
        blobs_recebidos: 0,
        eventos_enviados: 0,
        eventos_aplicados: 0,
        eventos_pendentes: 0,
    };
    incremental(&mut canal, ctx, falo_primeiro, &mut resultado)?;
    Ok(resultado)
}

// ═══════════════════════════════════════════════════════════════════════════
// Incremental
// ═══════════════════════════════════════════════════════════════════════════

fn vetor(database: &SqliteDatabase) -> DatabaseCommandResult<BTreeMap<String, i64>> {
    let connection = database.read()?;
    sync_exchange::vetor_local(&connection)
}

/// Eventos nos dois sentidos, e depois blobs nos dois sentidos.
///
/// Ordem fixa, e é a mesma dos dois lados: o visitante (quem fala primeiro)
/// envia antes, depois recebe. Blobs vêm por último porque o incremental aplica
/// a referência antes de o arquivo chegar — a etapa 13 aceita isso no
/// incremental, e o arquivo chega na mesma sessão.
fn incremental(
    canal: &mut Canal<'_>,
    ctx: &Contexto<'_>,
    falo_primeiro: bool,
    resultado: &mut ResultadoDaSessao,
) -> DatabaseCommandResult<()> {
    let meu = vetor(ctx.database)?;
    let dele = match canal.trocar(&Mensagem::Vetor(meu), falo_primeiro)? {
        Mensagem::Vetor(v) => v,
        outra => return Err(inesperada("Vetor", &outra)),
    };

    // As duas metades são espelho uma da outra, passo a passo:
    //
    //   visitante                     anfitrião
    //   enviar_eventos(vetor dele)    receber_eventos
    //   receber_eventos               enviar_eventos(vetor dele)
    //   puxar_blobs                   servir_blobs
    //   servir_blobs                  puxar_blobs
    //
    // O anfitrião envia contra o vetor que RECEBEU no começo, e isso está
    // certo: o vetor do visitante não muda por ele enviar. A primeira versão
    // esperava um vetor novo nesse ponto, e os dois lados ficavam lendo ao
    // mesmo tempo — o deadlock que o lock-step existe para impedir, escrito à
    // mão. Achado na leitura, antes do primeiro teste.
    if falo_primeiro {
        resultado.eventos_enviados += enviar_eventos(canal, ctx, dele)?;
        receber_eventos(canal, ctx, resultado)?;
        resultado.blobs_recebidos += puxar_blobs(canal, ctx)?;
        servir_blobs(canal, ctx)?;
    } else {
        receber_eventos(canal, ctx, resultado)?;
        resultado.eventos_enviados += enviar_eventos(canal, ctx, dele)?;
        servir_blobs(canal, ctx)?;
        resultado.blobs_recebidos += puxar_blobs(canal, ctx)?;
    }
    Ok(())
}

/// Envia lotes até o outro lado parar de avançar ou não faltar nada.
fn enviar_eventos(
    canal: &mut Canal<'_>,
    ctx: &Contexto<'_>,
    mut dele: BTreeMap<String, i64>,
) -> DatabaseCommandResult<usize> {
    let mut enviados = 0;
    loop {
        let lote = {
            let connection = ctx.database.read()?;
            sync_exchange::eventos_para(&connection, &dele)?
        };
        let vazio = lote.is_empty();
        enviados += lote.len();
        canal.enviar(&Mensagem::Eventos(lote))?;
        if vazio {
            return Ok(enviados);
        }
        let novo = match canal.receber()? {
            Mensagem::Vetor(v) => v,
            outra => return Err(inesperada("Vetor", &outra)),
        };
        if novo == dele {
            // Sem progresso: o que foi enviado ficou pendente do outro lado.
            // Encerra a fase com um lote vazio, que é o sinal combinado.
            canal.enviar(&Mensagem::Eventos(Vec::new()))?;
            return Ok(enviados);
        }
        dele = novo;
    }
}

/// Recebe lotes, aplica, e devolve o vetor a cada um.
fn receber_eventos(
    canal: &mut Canal<'_>,
    ctx: &Contexto<'_>,
    resultado: &mut ResultadoDaSessao,
) -> DatabaseCommandResult<()> {
    loop {
        let lote = match canal.receber()? {
            Mensagem::Eventos(lote) => lote,
            outra => return Err(inesperada("Eventos", &outra)),
        };
        if lote.is_empty() {
            // Lote vazio é o fim combinado da fase: quem envia não espera
            // vetor depois dele.
            return Ok(());
        }
        let relatorio = {
            let mut connection = ctx.database.write()?;
            sync_session::receber_eventos(&mut connection, &lote)?
        };
        resultado.eventos_aplicados += relatorio.aplicados;
        resultado.eventos_pendentes = relatorio.pendentes;
        canal.enviar(&Mensagem::Vetor(vetor(ctx.database)?))?;
    }
}

fn puxar_blobs(canal: &mut Canal<'_>, ctx: &Contexto<'_>) -> DatabaseCommandResult<usize> {
    let faltam = blobs_que_faltam(ctx)?;
    pedir_blobs(canal, ctx.store, &faltam)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync_bootstrap;
    use crate::application::sync_pin_pairing::escutar;
    use crate::application::{manuscript_service, universe_service};
    use crate::domain::manuscript::ChapterUpdate;
    use crate::infrastructure::blob_document::{hashes_do_documento, ATTR_BLOB, ATTR_MIME};
    use crate::infrastructure::blob_store::hash_dos_bytes;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use std::net::TcpListener;

    /// Um aparelho de verdade: banco, identidade em arquivo e blob store
    /// próprios. Nada é compartilhado entre dois `Aparelho` além do socket.
    struct Aparelho {
        banco: TemporaryDatabase,
        dados: std::path::PathBuf,
        identidade: DeviceIdentity,
        store: BlobStore,
        nome: &'static str,
    }

    impl Aparelho {
        fn novo(nome: &'static str) -> Self {
            let dados = std::env::temp_dir().join(format!("narrahub-e2e-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dados).expect("criar diretório");
            let banco = TemporaryDatabase::new();
            let identidade = sync_bootstrap::prepare(&dados, &banco.database).expect("arranque");
            let store = BlobStore::new(dados.clone());
            Self {
                banco,
                dados,
                identidade,
                store,
                nome,
            }
        }

        fn ctx(&self) -> Contexto<'_> {
            Contexto {
                database: &self.banco.database,
                store: &self.store,
                identidade: &self.identidade,
                nome_local: self.nome,
                espera: Duration::from_secs(30),
            }
        }

        fn capitulo(&self, id: &str) -> Option<crate::domain::manuscript::Chapter> {
            manuscript_service::get_chapter(&self.banco.database, id).expect("ler capítulo")
        }

        fn editar(&self, id: &str, conteudo: &str) {
            manuscript_service::update_chapter(
                &self.banco.database,
                &self.identidade,
                id,
                ChapterUpdate {
                    content: Some(conteudo.to_string()),
                    word_count: Some(conteudo.split_whitespace().count() as i64),
                    ..ChapterUpdate::default()
                },
            )
            .expect("editar capítulo");
        }

        fn roster(&self) -> Vec<String> {
            let connection = self.banco.connection();
            let mut consulta = connection
                .prepare("SELECT device_id FROM sync_devices WHERE is_self = 0 ORDER BY device_id")
                .expect("consulta");
            let linhas = consulta
                .query_map([], |row| row.get(0))
                .expect("linhas")
                .collect::<Result<Vec<String>, _>>()
                .expect("coletar");
            linhas
        }
    }

    impl Drop for Aparelho {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dados);
        }
    }

    fn destino(escuta: &TcpListener) -> String {
        format!(
            "127.0.0.1:{}",
            escuta.local_addr().expect("endereço").port()
        )
    }

    /// **O gate automatizado principal da Etapa 14.**
    ///
    /// Dois aparelhos, cada um com banco, identidade e blob store próprios, e
    /// um socket TCP real entre eles. Nenhuma função de troca é chamada por
    /// fora do fio: o que atravessa é exatamente o que a aplicação usa —
    /// `sync_wire`, Noise, PIN, `capturar`, `semear`, `transferir_blobs`,
    /// `eventos_para`, `receber_eventos`.
    ///
    /// ```text
    /// A tem universo, capitulo, imagem e blob fisico
    /// B nasce vazio
    ///
    /// 1  B digita o PIN de A    pareamento real, bootstrap real, blob real
    /// 2  B edita, conecta em A  evento V2 -> socket -> A aplica
    /// 3  A edita, conecta em B  evento V2 -> socket -> B aplica
    /// ```
    ///
    /// A fase 3 inverte quem conecta de propósito: prova que a escuta de B
    /// atende sessão pareada, e não só que A sabe atender.
    #[test]
    fn windows_e_android_convergem_ponta_a_ponta_sobre_tcp_real() {
        let a = Aparelho::novo("Desktop");
        let b = Aparelho::novo("Celular");

        // ── o acervo de A ───────────────────────────────────────────────────
        let universo = universe_service::create(
            &a.banco.database,
            &a.store,
            &a.identidade,
            "Terra Média",
            "",
            "",
        )
        .expect("universo");
        let historia = manuscript_service::create_story(
            &a.banco.database,
            &a.identidade,
            &universo.id,
            "Saga",
        )
        .expect("história");
        let livro = manuscript_service::create_book(
            &a.banco.database,
            &a.identidade,
            &historia.id,
            "Livro I",
        )
        .expect("livro");
        let capitulo = manuscript_service::create_chapter(
            &a.banco.database,
            &a.identidade,
            &livro.id,
            "Capítulo 1",
        )
        .expect("capítulo");

        let imagem: Vec<u8> = (0..4096_u32).map(|i| (i * 31 % 251) as u8).collect();
        let hash = a.store.put(&imagem).expect("publicar imagem");
        assert_eq!(hash, hash_dos_bytes(&imagem));

        let original = format!(
            "<p>Era uma vez um mapa.</p><img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\" alt=\"mapa\">"
        );
        a.editar(&capitulo.id, &original);
        assert!(
            !b.store.has(&hash).expect("has"),
            "B não podia ter a imagem antes"
        );

        // ── 1. pareamento por PIN + bootstrap ───────────────────────────────
        let escuta_a = escutar(0).expect("A escuta");
        let endereco_a = destino(&escuta_a);
        let mut codigos_a = Codigos::default();
        let (_, legivel) = codigos_a.emitir();
        let pin: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();

        let (em_a, em_b) = std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta_a.accept().expect("A aceita");
                atender_conexao(&mut fluxo, &mut codigos_a, &a.ctx())
            });
            let em_b = parear_por_pin(&endereco_a, &pin, &b.ctx());
            (servidor.join().expect("thread de A"), em_b)
        });
        let em_a = em_a.expect("A: sessão de pareamento");
        let em_b = em_b.expect("B: sessão de pareamento");

        assert_eq!(em_a.papel, Papel::Doador, "A tem acervo, B não: A doa");
        assert_eq!(em_b.papel, Papel::Receptor);
        assert!(em_a.houve_bootstrap && em_b.houve_bootstrap);
        assert_eq!(em_b.blobs_recebidos, 1, "exatamente a imagem do capítulo");
        assert_eq!(em_b.parceiro.device_id, a.identidade.device_id());
        assert_eq!(em_a.parceiro.device_id, b.identidade.device_id());

        // No B: capítulo, conteúdo, referência, blob físico, SHA.
        let no_b = b
            .capitulo(&capitulo.id)
            .expect("o capítulo tinha que existir em B");
        assert_eq!(no_b.content, original, "o conteúdo chegou diferente");
        let referencias = hashes_do_documento(&no_b.content).expect("ler referências");
        assert_eq!(
            referencias.into_iter().collect::<Vec<_>>(),
            vec![hash.clone()],
            "a referência de blob não confere"
        );
        assert!(
            b.store.verify(&hash).expect("verify"),
            "o blob físico não está em B"
        );
        let bytes_em_b = b.store.read(&hash).expect("ler blob em B");
        assert_eq!(
            hash_dos_bytes(&bytes_em_b),
            hash,
            "o SHA do arquivo em B não confere"
        );
        assert_eq!(bytes_em_b, imagem);

        // Roster: cada um tem o outro, e só o outro.
        assert_eq!(b.roster(), vec![a.identidade.device_id().to_string()]);
        assert_eq!(a.roster(), vec![b.identidade.device_id().to_string()]);

        // ── 2. B edita → A, com uma imagem que A nunca viu ─────────────────
        //
        // Sem a imagem nova, a puxada de blobs do incremental passaria sem
        // nunca ser exercida: o único hash do cenário já estava nos dois lados.
        let foto: Vec<u8> = (0..2048_u32).map(|i| (i * 7 % 241) as u8).collect();
        let hash_foto = b.store.put(&foto).expect("B publica a foto");
        assert!(
            !a.store.has(&hash_foto).expect("has"),
            "A não podia ter a foto"
        );

        let de_b = format!(
            "<p>Era uma vez um mapa, redesenhado no celular.</p>\
             <img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\" alt=\"mapa\">\
             <img {ATTR_BLOB}=\"{hash_foto}\" {ATTR_MIME}=\"image/png\" alt=\"foto\">"
        );
        b.editar(&capitulo.id, &de_b);

        let (em_a, em_b) = std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta_a.accept().expect("A aceita de novo");
                atender_conexao(&mut fluxo, &mut codigos_a, &a.ctx())
            });
            let em_b = sincronizar_com(&endereco_a, &b.ctx());
            (servidor.join().expect("thread de A"), em_b)
        });
        let em_a = em_a.expect("A: sessão pareada");
        let em_b = em_b.expect("B: sessão pareada");
        assert_eq!(em_b.eventos_enviados, 1, "B tinha exatamente uma edição");
        assert_eq!(
            em_a.eventos_aplicados, 1,
            "A tinha que aplicar a edição de B"
        );
        assert_eq!(
            a.capitulo(&capitulo.id).expect("capítulo em A").content,
            de_b,
            "A não convergiu para a edição de B"
        );
        assert_eq!(em_a.blobs_recebidos, 1, "A tinha que puxar a foto nova");
        assert!(
            a.store.verify(&hash_foto).expect("verify"),
            "a foto referenciada pela edição de B não chegou a A"
        );
        assert_eq!(
            hash_dos_bytes(&a.store.read(&hash_foto).expect("ler")),
            hash_foto
        );

        // ── 3. A edita → B, e desta vez é A quem conecta ────────────────────
        let de_a = format!(
            "<p>Terceira versão, de volta ao desktop.</p><img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\" alt=\"mapa\">"
        );
        a.editar(&capitulo.id, &de_a);

        let escuta_b = escutar(0).expect("B escuta");
        let endereco_b = destino(&escuta_b);
        let mut codigos_b = Codigos::default();

        let (em_b, em_a) = std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta_b.accept().expect("B aceita");
                atender_conexao(&mut fluxo, &mut codigos_b, &b.ctx())
            });
            let em_a = sincronizar_com(&endereco_b, &a.ctx());
            (servidor.join().expect("thread de B"), em_a)
        });
        let em_b = em_b.expect("B: sessão pareada");
        let em_a = em_a.expect("A: sessão pareada");
        assert_eq!(em_a.eventos_enviados, 1);
        assert_eq!(
            em_b.eventos_aplicados, 1,
            "B tinha que aplicar a edição de A"
        );

        let final_a = a.capitulo(&capitulo.id).expect("A").content;
        let final_b = b.capitulo(&capitulo.id).expect("B").content;
        assert_eq!(final_a, de_a);
        assert_eq!(final_b, de_a, "B não convergiu para a edição de A");

        // E os dois concordam sobre o que cada um já viu.
        let vetor_a = vetor(&a.banco.database).expect("vetor A");
        let vetor_b = vetor(&b.banco.database).expect("vetor B");
        assert_eq!(
            vetor_a, vetor_b,
            "os vetores divergiram depois de convergir"
        );
    }

    /// **Sessão pareada com quem não está no roster é recusada.**
    ///
    /// Confiança não é transitiva: um aparelho que se autentica de verdade —
    /// chave válida, prova válida — mas nunca foi pareado não sincroniza. A
    /// sessão prova quem ele é; só o roster diz se é permitido.
    #[test]
    fn estranho_autenticado_nao_sincroniza() {
        let a = Aparelho::novo("Desktop");
        let estranho = Aparelho::novo("Estranho");

        let escuta = escutar(0).expect("escutar");
        let endereco = destino(&escuta);
        let mut codigos = Codigos::default();

        let (em_a, do_estranho) = std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta.accept().expect("aceitar");
                atender_conexao(&mut fluxo, &mut codigos, &a.ctx())
            });
            let do_estranho = sincronizar_com(&endereco, &estranho.ctx());
            (servidor.join().expect("thread"), do_estranho)
        });

        let erro_a = em_a.expect_err("A não podia sincronizar com estranho");
        assert!(
            erro_a.message.contains("não está pareado"),
            "recusou pelo motivo errado: {}",
            erro_a.message
        );
        assert!(
            do_estranho.is_err(),
            "o estranho não podia ter sincronizado"
        );
        assert!(
            a.roster().is_empty(),
            "o estranho não pode ter entrado no roster"
        );
    }
}
