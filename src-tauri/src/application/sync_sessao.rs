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
//!    nao         sim       DOADOR    confere; captura, envia, serve blobs;
//!                                    admite SO depois do `Semeado { ok }`
//!    sim         sim       PAR       os dois admitem; nada a semear
//!    nao         nao       PAR       os dois admitem; incremental
//! ```
//!
//! **O receptor fresco não admite** — decisão registrada. A etapa 12 exige
//! receptor com só o `self` no roster, e o `semear` traz o doador pelo merge
//! do roster do bundle. Admitir antes tornaria o bootstrap impossível, e isso
//! foi medido.
//!
//! **E o doador só admite depois da prova** (etapa D, item 13). Até lá ele
//! apenas confere que admitiria. Uma semeadura que falha termina com os dois
//! roster como começaram — nenhum lado pareado pela metade — e o incremental
//! só começa com os dois lados já confiando um no outro.
//!
//! ## Lock-step, e por quê
//!
//! Toda troca é "um fala, o outro responde". Com os dois escrevendo ao mesmo
//! tempo, duas mensagens grandes podem encher a janela do TCP nos dois sentidos
//! e travar a sessão com os dois lados esperando. O visitante sempre fala
//! primeiro.
//!
//! ## O `Hello`: compatibilidade antes de qualquer estado (etapa E)
//!
//! ```text
//! modo em claro → PAKE/Noise → Apresentacao → SessaoAutenticada
//!   → Hello ⇄ Hello        protocolo, formato canônico, modo       ← aqui
//!   → Estado / Autorizacao
//!   → qualquer persistência
//! ```
//!
//! O `Hello` é a primeira mensagem de aplicação dos dois modos, e a única cujo
//! formato é permanente: uma versão futura pode mudar qualquer outra, mas
//! continua abrindo com ele. A decisão é em duas camadas:
//!
//! ```text
//! no Hello        protocolo diferente           → aborta
//!                 formato canônico diferente    → aborta
//!                 modo diferente do quadro      → aborta
//!                 impressão do bundle diferente → só anota
//! depois do       Par                           → impressão irrelevante
//! Estado          Doador / Receptor             → impressão precisa bater;
//!                                                 senão aborta ANTES de capturar
//! ```
//!
//! Schema diferente não entra na conta: o incremental não depende dele, e o
//! bootstrap depende só das colunas das tabelas transferidas — que é o que a
//! impressão do bundle mede.
//!
//! **Aparelho com o Sync V2 anterior ao `Hello`** (0.10.0-beta.1 e beta.2) não é
//! suportado: ele não interopera com o protocolo 1, sem decodificador antigo nem
//! downgrade. O que se garante é detectar cedo — ele abre com `Estado` ou
//! `Autorizacao` onde se espera `Hello` — e parar com zero escrita.
//!
//! **O PIN já foi consumido** quando o `Hello` chega: o pareamento por PIN
//! concluiu a autenticação. Incompatibilidade descoberta depois disso não
//! devolve a validade do código — gera-se outro depois de atualizar.
//!
//! ## Blob que falta segura o evento, e a sessão diz que ficou incompleta
//!
//! A drenagem (`sync_session`) não materializa evento que cite imagem ausente
//! aqui: ele fica no log, pendente. Depois dos eventos, cada lado pede ao outro
//! os blobs do seu domínio e os dos seus pendentes — inclusive de sessões
//! antigas —, grava só o que conferir o SHA e drena de novo. Se ainda sobrar
//! evento esperando blob (o outro não tinha, ou mandou bytes que não conferem),
//! a sessão termina com erro explícito, **depois** de servir os blobs que o
//! outro lado pediu. O evento não é apagado e não vira conflito: a próxima
//! sessão tenta de novo.
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
use crate::infrastructure::sqlite::sync_session::BlobsLocais;
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

/// **A versão do contrato da sessão**: sequência de mensagens, `Mensagem`, envelope e assinatura.
///
/// Começa em 1 na etapa E. O Sync V2 anterior a este número (0.10.0-beta.1/beta.2) é "legado
/// pré-negociação" e não interopera.
pub const PROTOCOLO_DO_SYNC: u32 = 1;

/// O que cada lado declara antes de qualquer outra coisa.
///
/// Sem `deny_unknown_fields`, de propósito: é aqui que uma versão futura acrescenta o que precisar,
/// e um campo a mais não pode derrubar a mensagem que existe para explicar a diferença.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocolo: u32,
    pub formato_canonico: i64,
    /// [`sync_snapshot::formato_do_bundle`]. Só decide quando há bootstrap.
    pub formato_do_bundle: String,
    /// `"pin"` ou `"pareado"`: o quadro em claro, repetido dentro do canal cifrado.
    pub modo: String,
    /// Só para a mensagem ao escritor. Nunca decide nada.
    pub app: String,
}

const MODO_HELLO_PIN: &str = "pin";
const MODO_HELLO_PAREADO: &str = "pareado";

/// Por que dois aparelhos autenticados não podem sincronizar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incompatibilidade {
    /// O outro abriu com `Estado`/`Autorizacao`: é o Sync V2 anterior ao `Hello`.
    PeerLegado { abriu_com: &'static str },
    Protocolo {
        meu: u32,
        dele: u32,
        app_dele: String,
    },
    FormatoCanonico {
        meu: i64,
        dele: i64,
        app_dele: String,
    },
    /// O `Hello` diz um modo e o quadro em claro disse outro.
    Modo { esperado: String, veio: String },
    /// Só quando o papel exige bootstrap.
    FormatoDoBundle { app_dele: String },
}

impl std::fmt::Display for Incompatibilidade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Incompatibilidade::PeerLegado { .. } => f.write_str(
                "Este aparelho usa uma versão beta antiga do Sync V2. Atualize o NarraHub nos \
                 dois aparelhos antes de sincronizar novamente.",
            ),
            Incompatibilidade::Protocolo {
                meu,
                dele,
                app_dele,
            } => write!(
                f,
                "Os dois aparelhos falam versões diferentes da sincronização (este: {meu}, o \
                 outro: {dele}, NarraHub {app_dele}). Atualize os dois para a mesma versão. Nada \
                 foi alterado."
            ),
            Incompatibilidade::FormatoCanonico {
                meu,
                dele,
                app_dele,
            } => write!(
                f,
                "Os dois aparelhos descrevem o acervo em formatos diferentes (este: {meu}, o \
                 outro: {dele}, NarraHub {app_dele}). Atualize os dois para a mesma versão. Nada \
                 foi alterado."
            ),
            Incompatibilidade::Modo { esperado, veio } => write!(
                f,
                "A conexão começou como '{esperado}' e o outro aparelho se apresentou como \
                 '{veio}'. A sessão foi encerrada sem alterar nada."
            ),
            Incompatibilidade::FormatoDoBundle { app_dele } => write!(
                f,
                "Os dois aparelhos sincronizam, mas não conseguem copiar o acervo inteiro de um \
                 para o outro nesta combinação de versões (o outro: NarraHub {app_dele}). \
                 Atualize os dois para a mesma versão antes do primeiro pareamento. Nada foi \
                 alterado."
            ),
        }
    }
}

/// Como uma sessão termina mal: incompatibilidade classificada, ou qualquer outro erro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaDaSessao {
    Incompativel(Incompatibilidade),
    Erro(DatabaseCommandError),
}

impl From<DatabaseCommandError> for FalhaDaSessao {
    fn from(erro: DatabaseCommandError) -> Self {
        FalhaDaSessao::Erro(erro)
    }
}

impl From<Incompatibilidade> for FalhaDaSessao {
    fn from(motivo: Incompatibilidade) -> Self {
        FalhaDaSessao::Incompativel(motivo)
    }
}

/// Para a fronteira Tauri, cujo contrato de erro é `kind` + `message`.
impl From<FalhaDaSessao> for DatabaseCommandError {
    fn from(falha: FalhaDaSessao) -> Self {
        match falha {
            FalhaDaSessao::Incompativel(motivo) => {
                DatabaseCommandError::conflict(motivo.to_string())
            }
            FalhaDaSessao::Erro(erro) => erro,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "dados")]
enum Mensagem {
    /// Sempre a primeira. Ver o cabeçalho.
    Hello(Hello),
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
/// **A ordem da etapa C, cobrada onde o fluxo do produto começa.**
///
/// ```text
/// estado legado → backfills → gênese → baseline / pareamento
/// ```
///
/// Um acervo com item sem revisão não foi adotado: parear agora mandaria para o outro aparelho
/// conteúdo que nenhum evento explica, e o incremental não teria de onde partir. A checagem é por
/// ÓRFÃO, e não pela linha de `sync_adoptions`: um aparelho recém-instalado não tem o que adotar e
/// passa direto, como deve.
fn exigir_acervo_adotado(ctx: &Contexto<'_>) -> DatabaseCommandResult<()> {
    let connection = ctx.database.read()?;
    // A época do protocolo 1 primeiro (etapa E): um banco que ainda carrega o passado pré-Hello
    // tem no log envelopes que o relay mandaria adiante como se fossem do protocolo 1.
    crate::application::epoca::exigir_epoca(&connection)?;
    crate::application::genese::exigir_acervo_sem_orfaos(&connection)
}

pub fn atender_conexao(
    fluxo: &mut TcpStream,
    codigos: &mut Codigos,
    ctx: &Contexto<'_>,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    atender_conexao_detalhada(fluxo, codigos, ctx).map_err(Into::into)
}

/// [`atender_conexao`], com a incompatibilidade classificada.
pub fn atender_conexao_detalhada(
    fluxo: &mut TcpStream,
    codigos: &mut Codigos,
    ctx: &Contexto<'_>,
) -> Result<ResultadoDaSessao, FalhaDaSessao> {
    exigir_acervo_adotado(ctx)?;
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
        Err(falha("Conexão com modo desconhecido. Nada foi trocado.").into())
    }
}

/// Conecta no endereço digitado e pareia por PIN; faz bootstrap se couber.
pub fn parear_por_pin(
    endereco: &str,
    pin: &str,
    ctx: &Contexto<'_>,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    parear_por_pin_detalhado(endereco, pin, ctx).map_err(Into::into)
}

/// [`parear_por_pin`], com a incompatibilidade classificada.
pub fn parear_por_pin_detalhado(
    endereco: &str,
    pin: &str,
    ctx: &Contexto<'_>,
) -> Result<ResultadoDaSessao, FalhaDaSessao> {
    exigir_acervo_adotado(ctx)?;
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
    sincronizar_com_detalhado(endereco, ctx).map_err(Into::into)
}

/// [`sincronizar_com`], com a incompatibilidade classificada.
pub fn sincronizar_com_detalhado(
    endereco: &str,
    ctx: &Contexto<'_>,
) -> Result<ResultadoDaSessao, FalhaDaSessao> {
    exigir_acervo_adotado(ctx)?;
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
) -> Result<ResultadoDaSessao, FalhaDaSessao> {
    let parceiro = pareada.parceiro();
    let SessaoPareada {
        transporte,
        sessao,
        nome_do_outro,
    } = pareada;
    let mut canal = Canal { transporte, fluxo };

    let (meu_hello, hello_dele) = trocar_hello(&mut canal, ctx, MODO_HELLO_PIN, falo_primeiro)?;

    let eu_fresco = fresco(ctx.database)?;
    let ele_fresco = match canal.trocar(&Mensagem::Estado { fresco: eu_fresco }, falo_primeiro)? {
        Mensagem::Estado { fresco } => fresco,
        outra => return Err(inesperada("Estado", &outra).into()),
    };
    let papel = papel_de(eu_fresco, ele_fresco);

    // A segunda camada do Hello: a impressão do bundle só decide quando há bundle. Os dois lados
    // chegam ao mesmo papel e às mesmas duas impressões, então abortam juntos, sem mensagem a mais
    // — e antes de capturar, enviar ou receber qualquer coisa.
    if papel != Papel::Par && meu_hello.formato_do_bundle != hello_dele.formato_do_bundle {
        return Err(Incompatibilidade::FormatoDoBundle {
            app_dele: hello_dele.app.clone(),
        }
        .into());
    }

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
            // **Confere antes, grava depois do `Semeado { ok: true }`** (etapa D, item 13).
            //
            // Admitir antes de capturar deixava, numa semeadura que falha, o doador pareado com
            // um aparelho que não recebeu nada — e o receptor sem o doador. O próximo encontro
            // dos dois seria uma sessão entre pareados que um dos lados nunca concluiu.
            //
            // As recusas da admissão (o próprio aparelho, aparelho que saiu do conjunto) são
            // conhecidas ANTES de o acervo sair daqui: descobrir depois seria ter entregado tudo
            // a quem não podia entrar. A escrita no roster é que espera a prova.
            {
                let connection = ctx.database.read()?;
                crate::infrastructure::sqlite::sync_trust::conferir_admissao_por_pareamento(
                    &connection,
                    &sessao,
                )?;
            }
            doar_bootstrap(&mut canal, ctx)?;
            para_admitir(ctx.database)?;
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
                    #[cfg(test)]
                    let bytes = blob_adulterado::talvez(bytes);
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
) -> Result<ResultadoDaSessao, FalhaDaSessao> {
    let parceiro = pareada.parceiro();
    let mut canal = Canal {
        transporte: pareada.transporte,
        fluxo,
    };

    // Entre pareados não há bundle: a impressão dele não decide nada aqui.
    trocar_hello(&mut canal, ctx, MODO_HELLO_PAREADO, falo_primeiro)?;

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
        ))
        .into());
    }
    match resposta {
        Mensagem::Autorizacao { aceito: true, .. } => {}
        Mensagem::Autorizacao {
            aceito: false,
            motivo,
        } => {
            return Err(falha(format!(
                "O outro aparelho recusou a sincronização: {motivo}."
            ))
            .into())
        }
        outra => return Err(inesperada("Autorizacao", &outra).into()),
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
// Hello
// ═══════════════════════════════════════════════════════════════════════════

/// O `Hello` deste aparelho. Só lê.
fn hello_local(ctx: &Contexto<'_>, modo: &str) -> DatabaseCommandResult<Hello> {
    let formato_do_bundle = {
        let connection = ctx.database.read()?;
        sync_snapshot::formato_do_bundle(&connection)?
    };
    let hello = Hello {
        protocolo: PROTOCOLO_DO_SYNC,
        formato_canonico: crate::infrastructure::sqlite::sync_codec::FORMATO_CANONICO_ATUAL,
        formato_do_bundle,
        modo: modo.to_string(),
        app: env!("CARGO_PKG_VERSION").to_string(),
    };
    #[cfg(test)]
    let hello = hello_de_teste::ajustar(hello);
    Ok(hello)
}

/// Troca os `Hello` e aplica a primeira camada da decisão. Devolve (o meu, o dele).
///
/// Lock-step como tudo o mais, com uma diferença deliberada no lado que fala depois: se o que
/// chega não é `Hello`, ele **não responde** — um aparelho legado não tem o que fazer com o nosso.
fn trocar_hello(
    canal: &mut Canal<'_>,
    ctx: &Contexto<'_>,
    modo: &str,
    falo_primeiro: bool,
) -> Result<(Hello, Hello), FalhaDaSessao> {
    let meu = hello_local(ctx, modo)?;

    #[cfg(test)]
    if hello_de_teste::como_legado() {
        // O Sync V2 pré-negociação não manda Hello: segue direto para Estado/Autorizacao.
        return Ok((meu.clone(), meu));
    }

    let veio = if falo_primeiro {
        canal.enviar(&Mensagem::Hello(meu.clone()))?;
        canal.receber().map_err(|erro| {
            falha(format!(
                "O outro aparelho encerrou a conexão ao receber a apresentação de versão. Se ele \
                 estiver numa versão beta antiga do Sync V2, atualize o NarraHub nos dois \
                 aparelhos. Detalhe: {}",
                erro.message
            ))
        })?
    } else {
        let veio = canal.receber()?;
        if let Some(abriu_com) = legado(&veio) {
            return Err(Incompatibilidade::PeerLegado { abriu_com }.into());
        }
        canal.enviar(&Mensagem::Hello(meu.clone()))?;
        veio
    };

    let dele = match veio {
        Mensagem::Hello(hello) => hello,
        outra => {
            if let Some(abriu_com) = legado(&outra) {
                return Err(Incompatibilidade::PeerLegado { abriu_com }.into());
            }
            return Err(inesperada("Hello", &outra).into());
        }
    };

    if dele.protocolo != meu.protocolo {
        return Err(Incompatibilidade::Protocolo {
            meu: meu.protocolo,
            dele: dele.protocolo,
            app_dele: dele.app,
        }
        .into());
    }
    if dele.formato_canonico != meu.formato_canonico {
        return Err(Incompatibilidade::FormatoCanonico {
            meu: meu.formato_canonico,
            dele: dele.formato_canonico,
            app_dele: dele.app,
        }
        .into());
    }
    if dele.modo != modo {
        return Err(Incompatibilidade::Modo {
            esperado: modo.to_string(),
            veio: dele.modo,
        }
        .into());
    }
    Ok((meu, dele))
}

/// A primeira mensagem do Sync V2 pré-negociação, reconhecida pelo que ela é.
fn legado(mensagem: &Mensagem) -> Option<&'static str> {
    match mensagem {
        Mensagem::Estado { .. } => Some("Estado"),
        Mensagem::Autorizacao { .. } => Some("Autorizacao"),
        _ => None,
    }
}

/// Portas de teste do `Hello`, por thread: só o lado armado muda.
#[cfg(test)]
pub(crate) mod hello_de_teste {
    use super::Hello;

    thread_local! {
        static AJUSTE: std::cell::Cell<Option<fn(&mut Hello)>> = const { std::cell::Cell::new(None) };
        static LEGADO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Altera o `Hello` que este lado manda.
    pub fn alterar(ajuste: Option<fn(&mut Hello)>) {
        AJUSTE.with(|celula| celula.set(ajuste));
    }

    /// Faz este lado se comportar como o Sync V2 anterior ao `Hello`.
    pub fn fingir_legado(legado: bool) {
        LEGADO.with(|celula| celula.set(legado));
    }

    pub(super) fn ajustar(mut hello: Hello) -> Hello {
        if let Some(ajuste) = AJUSTE.with(|celula| celula.get()) {
            ajuste(&mut hello);
        }
        hello
    }

    pub(super) fn como_legado() -> bool {
        LEGADO.with(|celula| celula.get())
    }

    pub fn limpar() {
        alterar(None);
        fingir_legado(false);
    }
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
/// envia antes, depois recebe. Blobs vêm depois porque só então cada lado sabe
/// de quais precisa: o evento que cita imagem ausente ficou pendente na
/// drenagem, e a puxada busca o arquivo e drena de novo (etapa D, item 12).
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
    let esperando_blob = if falo_primeiro {
        resultado.eventos_enviados += enviar_eventos(canal, ctx, dele)?;
        receber_eventos(canal, ctx, resultado)?;
        let esperando = puxar_blobs(canal, ctx, resultado)?;
        servir_blobs(canal, ctx)?;
        esperando
    } else {
        receber_eventos(canal, ctx, resultado)?;
        resultado.eventos_enviados += enviar_eventos(canal, ctx, dele)?;
        servir_blobs(canal, ctx)?;
        puxar_blobs(canal, ctx, resultado)?
    };

    // Só agora, com o protocolo inteiro cumprido dos dois lados: o outro aparelho recebeu o que
    // pediu deste, e a falha daqui não vira falha de rede do lado de lá.
    if !esperando_blob.is_empty() {
        return Err(falha(format!(
            "Sincronização incompleta: {} imagem(ns) de que as alterações recebidas dependem não \
             chegaram — o outro aparelho não as tem, ou mandou um arquivo que não confere. As \
             alterações ficaram guardadas, sem aplicar, e entram quando a imagem chegar. \
             Exemplo: {}.",
            esperando_blob.len(),
            esperando_blob.iter().next().cloned().unwrap_or_default()
        )));
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
            sync_session::receber_eventos(&mut connection, &lote, ctx.store)?
        };
        resultado.eventos_aplicados += relatorio.aplicados;
        resultado.eventos_pendentes = relatorio.pendentes;
        canal.enviar(&Mensagem::Vetor(vetor(ctx.database)?))?;
    }
}

/// Pede os blobs que faltam, grava o que conferir e drena os pendentes de novo.
///
/// O que falta é a soma de duas perguntas: o domínio cita e o disco não tem (acervo antigo), e um
/// evento pendente cita e o disco não tem — este segundo é o que segura a fila. A drenagem de
/// depois é a mesma função da recepção, com a mesma checagem: um blob que não chegou continua
/// segurando o evento.
///
/// Devolve os blobs que ainda seguram eventos. Vazio = nada ficou para trás por falta de arquivo.
fn puxar_blobs(
    canal: &mut Canal<'_>,
    ctx: &Contexto<'_>,
    resultado: &mut ResultadoDaSessao,
) -> DatabaseCommandResult<BTreeSet<String>> {
    let mut faltam = blobs_que_faltam(ctx)?;
    let dos_pendentes = {
        let connection = ctx.database.read()?;
        sync_session::blobs_dos_pendentes(&connection)?
    };
    for hash in dos_pendentes {
        if !faltam.contains(&hash) && !ctx.store.verificado(&hash)? {
            faltam.insert(hash);
        }
    }
    resultado.blobs_recebidos += pedir_blobs(canal, ctx.store, &faltam)?;

    let relatorio = {
        let mut connection = ctx.database.write()?;
        sync_session::receber_eventos(&mut connection, &[], ctx.store)?
    };
    resultado.eventos_aplicados += relatorio.aplicados;
    resultado.eventos_pendentes = relatorio.pendentes;
    Ok(relatorio.esperando_blob)
}

/// Bytes adulterados na saída, só em teste: o outro lado precisa recusar pelo SHA.
///
/// O `servir_blobs` real não consegue mentir — `BlobStore::read` confere antes de devolver —, e é
/// exatamente por isso que o gate do "peer mandou outros bytes" precisa de uma porta aqui.
#[cfg(test)]
pub(crate) mod blob_adulterado {
    thread_local! {
        static ARMADO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    pub fn armar(armado: bool) {
        ARMADO.with(|celula| celula.set(armado));
    }

    pub fn talvez(mut bytes: Vec<u8>) -> Vec<u8> {
        if ARMADO.with(|celula| celula.get()) {
            match bytes.first_mut() {
                Some(primeiro) => *primeiro ^= 0xFF,
                None => bytes.push(0),
            }
        }
        bytes
    }
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
    /// **Acervo não adotado não entra em sessão nenhuma** — nem como quem liga, nem como quem
    /// atende. A recusa vem antes de qualquer byte na rede: o endereço abaixo não existe, e o erro
    /// ainda assim é o da adoção.
    #[test]
    fn sessao_recusa_acervo_nao_adotado() {
        let aparelho = Aparelho::novo("legado");
        {
            // Um acervo anterior ao Sync V2: linha de domínio sem revisão que a explique.
            let connection = aparelho.banco.database.write().expect("escrita");
            crate::infrastructure::sqlite::test_support::seed_universe(&connection, "u-legado");
        }

        let erro = sincronizar_com("127.0.0.1:1", &aparelho.ctx())
            .expect_err("acervo não adotado não sincroniza");
        assert!(erro.message.contains("não foi adotado"), "{}", erro.message);

        let erro = parear_por_pin("127.0.0.1:1", "000000", &aparelho.ctx())
            .expect_err("acervo não adotado não pareia");
        assert!(erro.message.contains("não foi adotado"), "{}", erro.message);

        // Depois da adoção, a recusa some (e o erro passa a ser o da conexão que não existe).
        crate::application::genese::adotar(&aparelho.banco.database, &aparelho.identidade)
            .expect("adotar");
        let erro = sincronizar_com("127.0.0.1:1", &aparelho.ctx()).expect_err("sem servidor");
        assert!(
            !erro.message.contains("não foi adotado"),
            "{}",
            erro.message
        );
    }
    /// **D0 — o receptor que já abriu o aplicativo continua sendo receptor.**
    ///
    /// O arranque da etapa C adota o acervo (sobre nada, num aparelho novo) e registra a versão em
    /// `sync_adoptions`. Se essa marca contar como "conteúdo", o aparelho recém-instalado deixa de
    /// ser elegível — e o pareamento perde o caminho de semeadura para sempre, porque todo
    /// aparelho real abre o aplicativo antes de parear.
    ///
    /// Este gate passa pelo fluxo de verdade: arranque no receptor, pareamento por PIN, papéis.
    #[test]
    fn receptor_que_ja_passou_pelo_arranque_ainda_recebe_bootstrap() {
        let a = Aparelho::novo("Desktop");
        let b = Aparelho::novo("Celular");

        // A tem acervo.
        universe_service::create(
            &a.banco.database,
            &a.store,
            &a.identidade,
            "Terra Média",
            "",
            "",
        )
        .expect("universo");

        // B é novo — e abriu o aplicativo uma vez, como qualquer aparelho real.
        let estado_de_b = crate::database::estado::EstadoDoBanco::default();
        estado_de_b.definir(crate::database::estado::FaseDoBanco::UpgradingBlobs);
        crate::application::arranque::preparar_acervo(
            &b.banco.database,
            &b.store,
            &b.identidade,
            &estado_de_b,
        )
        .expect("arranque de B");
        assert_eq!(
            estado_de_b.fase(),
            crate::database::estado::FaseDoBanco::Ready,
            "o arranque de um aparelho novo tem de terminar pronto"
        );

        let escuta_a = TcpListener::bind("127.0.0.1:0").expect("porta de A");
        let endereco_a = escuta_a.local_addr().expect("endereço").to_string();
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

        assert_eq!(
            em_b.papel,
            Papel::Receptor,
            "B abriu o aplicativo uma vez e deixou de ser elegível a receber o acervo"
        );
        assert_eq!(em_a.papel, Papel::Doador);
        assert!(em_a.houve_bootstrap && em_b.houve_bootstrap);
        assert!(
            b.banco
                .database
                .read()
                .expect("leitura")
                .query_row("SELECT COUNT(*) FROM universes", [], |row| row
                    .get::<_, i64>(0))
                .expect("contar")
                > 0,
            "o acervo de A não chegou em B"
        );
    }
    /// Um par de aparelhos com acervo, já pareados e convergidos.
    fn dois_pareados_com_acervo() -> (Aparelho, Aparelho, String) {
        let a = Aparelho::novo("Desktop");
        let b = Aparelho::novo("Celular");
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
            "Livro",
        )
        .expect("livro");
        let capitulo =
            manuscript_service::create_chapter(&a.banco.database, &a.identidade, &livro.id, "Um")
                .expect("capítulo");

        let escuta = TcpListener::bind("127.0.0.1:0").expect("porta");
        let endereco = escuta.local_addr().expect("endereço").to_string();
        let mut codigos = Codigos::default();
        let (_, legivel) = codigos.emitir();
        let pin: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();
        let (em_a, em_b) = std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta.accept().expect("aceita");
                atender_conexao(&mut fluxo, &mut codigos, &a.ctx())
            });
            let em_b = parear_por_pin(&endereco, &pin, &b.ctx());
            (servidor.join().expect("thread"), em_b)
        });
        em_a.expect("A: pareamento");
        em_b.expect("B: pareamento");
        (a, b, capitulo.id)
    }

    /// Sincroniza dois aparelhos já pareados e devolve os dois resultados.
    fn sincronizar_pareados(
        a: &Aparelho,
        b: &Aparelho,
    ) -> (
        DatabaseCommandResult<ResultadoDaSessao>,
        DatabaseCommandResult<ResultadoDaSessao>,
    ) {
        sincronizar_pareados_adulterando(a, b, false)
    }

    /// Idem, com A adulterando os bytes de todo blob que servir.
    fn sincronizar_pareados_adulterando(
        a: &Aparelho,
        b: &Aparelho,
        adulterar: bool,
    ) -> (
        DatabaseCommandResult<ResultadoDaSessao>,
        DatabaseCommandResult<ResultadoDaSessao>,
    ) {
        let escuta = TcpListener::bind("127.0.0.1:0").expect("porta");
        let endereco = escuta.local_addr().expect("endereço").to_string();
        let mut codigos = Codigos::default();
        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                // A porta é por thread: só o lado de A mente.
                blob_adulterado::armar(adulterar);
                let (mut fluxo, _) = escuta.accept().expect("aceita");
                let resultado = atender_conexao(&mut fluxo, &mut codigos, &a.ctx());
                blob_adulterado::armar(false);
                resultado
            });
            let em_b = sincronizar_com(&endereco, &b.ctx());
            (servidor.join().expect("thread"), em_b)
        })
    }

    /// Eventos no log de B que não foram aplicados.
    fn pendentes(aparelho: &Aparelho) -> i64 {
        aparelho
            .banco
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sync_events e
              LEFT JOIN sync_applied_events a ON a.event_id = e.event_id
                  WHERE a.event_id IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("contar pendentes")
    }

    fn cursor_de(aparelho: &Aparelho, origem: &str) -> i64 {
        aparelho
            .banco
            .connection()
            .query_row(
                "SELECT last_seq_applied FROM sync_cursors WHERE origin_device_id = ?1",
                [origem],
                |row| row.get(0),
            )
            .unwrap_or(0)
    }

    /// **D12 — blob que o outro lado não tem: o estado não pode ser materializado.**
    ///
    /// O evento referencia um blob; o blob é dependência de materialização, não um extra que
    /// chega depois. Se ele não vem, o agregado não pode avançar — senão o aparelho fica com uma
    /// referência que nenhuma sessão futura vai buscar, porque o cursor já passou.
    #[test]
    fn d_blob_que_o_outro_lado_nao_tem_nao_materializa_o_evento() {
        let (a, b, capitulo) = dois_pareados_com_acervo();

        // A edita o capítulo com uma imagem — e o arquivo dela some do store de A antes da
        // sessão. Do ponto de vista de B, é um blob que o outro lado não consegue entregar.
        let imagem: Vec<u8> = (0..1024_u32).map(|i| (i * 13 % 251) as u8).collect();
        let hash = a.store.put(&imagem).expect("publicar");
        let com_imagem =
            format!("<p>com imagem</p><img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\">");
        a.editar(&capitulo, &com_imagem);
        std::fs::remove_file(a.store.path_for(&hash).expect("caminho"))
            .expect("sumir com o arquivo em A");

        let cursor_antes = cursor_de(&b, a.identidade.device_id());
        let (em_a, em_b) = sincronizar_pareados(&a, &b);
        em_a.expect("A cumpriu o protocolo: serviu o que tinha e puxou o que quis");

        let erro = em_b.expect_err("a sessão terminou como sucesso sem o blob que o evento cita");
        assert!(erro.message.contains("incompleta"), "{}", erro.message);
        let no_b = b.capitulo(&capitulo).expect("capítulo em B");
        assert!(
            !no_b.content.contains(&hash),
            "B materializou uma referência a um blob que ele nunca recebeu"
        );
        // O evento não foi apagado nem virou conflito: está no log, esperando.
        assert_eq!(
            pendentes(&b),
            1,
            "o evento da edição tinha de ficar pendente em B"
        );
        assert_eq!(
            cursor_de(&b, a.identidade.device_id()),
            cursor_antes,
            "o cursor de B passou por cima do evento que espera o blob"
        );
        let divergencias: i64 = b
            .banco
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_divergences", [], |row| {
                row.get(0)
            })
            .expect("contar");
        assert_eq!(divergencias, 0, "blob ausente não é divergência");

        // ── retry posterior: A recupera o arquivo, e a próxima sessão aplica uma vez ──
        assert_eq!(a.store.put(&imagem).expect("A recupera a imagem"), hash);
        let (em_a, em_b) = sincronizar_pareados(&a, &b);
        em_a.expect("A: retry");
        let em_b = em_b.expect("B: com o blob disponível, a sessão conclui");
        assert_eq!(
            em_b.eventos_aplicados, 1,
            "o pendente entra exatamente uma vez"
        );
        assert_eq!(em_b.blobs_recebidos, 1);
        assert!(b.store.verify(&hash).expect("verify"));
        assert_eq!(b.capitulo(&capitulo).expect("B").content, com_imagem);
        assert_eq!(pendentes(&b), 0);

        // E uma terceira sessão não reaplica nada.
        let (em_a, em_b) = sincronizar_pareados(&a, &b);
        em_a.expect("A: terceira");
        let em_b = em_b.expect("B: terceira");
        assert_eq!(em_b.eventos_aplicados, 0);
        assert_eq!(em_b.blobs_recebidos, 0);
    }

    /// **D12 — o outro lado manda bytes que não conferem o SHA.**
    ///
    /// Nada é publicado sob aquele endereço, o evento continua pendente, e a sessão termina como
    /// incompleta. Na sessão seguinte, com bytes honestos, entra.
    #[test]
    fn d_blob_com_sha_invalido_na_rede_nao_materializa_o_evento() {
        let (a, b, capitulo) = dois_pareados_com_acervo();

        let imagem: Vec<u8> = (0..900_u32).map(|i| (i * 29 % 251) as u8).collect();
        let hash = a.store.put(&imagem).expect("publicar");
        let com_imagem =
            format!("<p>com imagem</p><img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\">");
        a.editar(&capitulo, &com_imagem);

        let (em_a, em_b) = sincronizar_pareados_adulterando(&a, &b, true);
        em_a.expect("A cumpriu o protocolo");
        let erro = em_b.expect_err("bytes adulterados não podiam concluir a sessão");
        assert!(erro.message.contains("incompleta"), "{}", erro.message);
        assert!(
            !b.store.has(&hash).expect("has"),
            "B publicou sob um endereço que os bytes não sustentam"
        );
        assert!(!b.capitulo(&capitulo).expect("B").content.contains(&hash));
        assert_eq!(pendentes(&b), 1);

        let (em_a, em_b) = sincronizar_pareados(&a, &b);
        em_a.expect("A: honesto");
        let em_b = em_b.expect("B: honesto");
        assert_eq!(em_b.eventos_aplicados, 1);
        assert_eq!(b.capitulo(&capitulo).expect("B").content, com_imagem);
    }

    /// **D13 — bootstrap que falha não deixa ninguém pareado pela metade.**
    ///
    /// Hoje o doador admite o receptor no roster ANTES de capturar. Se a semeadura falhar, o
    /// doador fica achando que pareou e o receptor não — e o próximo encontro dos dois é uma
    /// sessão entre pareados que nunca existiu.
    #[test]
    fn d_bootstrap_que_falha_nao_deixa_pareamento_pela_metade() {
        let a = Aparelho::novo("Desktop");
        let b = Aparelho::novo("Celular");
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
            "Livro",
        )
        .expect("livro");
        let capitulo =
            manuscript_service::create_chapter(&a.banco.database, &a.identidade, &livro.id, "Um")
                .expect("capítulo");
        // Uma imagem que o doador não consegue entregar: a semeadura do receptor precisa dela.
        let imagem: Vec<u8> = (0..512_u32).map(|i| (i * 17 % 251) as u8).collect();
        let hash = a.store.put(&imagem).expect("publicar");
        a.editar(
            &capitulo.id,
            &format!("<p>x</p><img {ATTR_BLOB}=\"{hash}\" {ATTR_MIME}=\"image/png\">"),
        );
        std::fs::remove_file(a.store.path_for(&hash).expect("caminho"))
            .expect("sumir com o arquivo");

        let escuta = TcpListener::bind("127.0.0.1:0").expect("porta");
        let endereco = escuta.local_addr().expect("endereço").to_string();
        let mut codigos = Codigos::default();
        let (_, legivel) = codigos.emitir();
        let pin: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();
        let (em_a, em_b) = std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta.accept().expect("aceita");
                atender_conexao(&mut fluxo, &mut codigos, &a.ctx())
            });
            let em_b = parear_por_pin(&endereco, &pin, &b.ctx());
            (servidor.join().expect("thread"), em_b)
        });

        assert!(
            em_b.is_err(),
            "a semeadura sem o blob obrigatório tinha de falhar"
        );
        let _ = em_a;
        assert_eq!(
            b.roster(),
            Vec::<String>::new(),
            "o receptor ficou com o doador no roster depois de uma semeadura que falhou"
        );
        assert_eq!(
            a.roster(),
            Vec::<String>::new(),
            "o doador ficou pareado com um aparelho que não recebeu nada"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Etapa E — Hello: compatibilidade antes de qualquer estado
    // ═══════════════════════════════════════════════════════════════════════

    type Detalhado = Result<ResultadoDaSessao, FalhaDaSessao>;

    /// Tudo o que um aparelho persiste: cada linha de cada tabela, e cada arquivo do diretório de
    /// dados (blobs, identidade). Duas fotos iguais = nada foi escrito.
    fn retrato(aparelho: &Aparelho) -> Vec<String> {
        let connection = aparelho.banco.connection();
        let tabelas: Vec<String> = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("tabelas")
            .query_map([], |row| row.get(0))
            .expect("listar")
            .collect::<Result<_, _>>()
            .expect("ler");
        let mut foto = Vec::new();
        for tabela in tabelas {
            let mut consulta = connection
                .prepare(&format!("SELECT * FROM \"{tabela}\""))
                .expect("select");
            let colunas = consulta.column_count();
            let mut linhas: Vec<String> = consulta
                .query_map([], |row| {
                    let mut linha = Vec::with_capacity(colunas);
                    for i in 0..colunas {
                        linha.push(format!("{:?}", row.get::<_, rusqlite::types::Value>(i)?));
                    }
                    Ok(linha.join("|"))
                })
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("ler linhas");
            linhas.sort();
            foto.push(format!("## {tabela} ({})", linhas.len()));
            foto.extend(linhas);
        }
        fn arquivos(dir: &std::path::Path, foto: &mut Vec<String>) {
            let Ok(entradas) = std::fs::read_dir(dir) else {
                return;
            };
            let mut entradas: Vec<_> = entradas.flatten().collect();
            entradas.sort_by_key(|e| e.path());
            for entrada in entradas {
                let caminho = entrada.path();
                if caminho.is_dir() {
                    arquivos(&caminho, foto);
                } else {
                    let bytes = std::fs::read(&caminho).unwrap_or_default();
                    foto.push(format!(
                        "arquivo {} {}",
                        caminho.display(),
                        hash_dos_bytes(&bytes)
                    ));
                }
            }
        }
        arquivos(&aparelho.dados, &mut foto);
        foto
    }

    fn pin_de(codigos: &mut Codigos) -> String {
        let (_, legivel) = codigos.emitir();
        legivel.chars().filter(|c| c.is_ascii_digit()).collect()
    }

    /// Pareamento por PIN com uma porta de teste armada em cada lado. (resultado de A, de B)
    fn parear_armado(
        a: &Aparelho,
        b: &Aparelho,
        codigos: &mut Codigos,
        pin: &str,
        armar_a: fn(),
        armar_b: fn(),
    ) -> (Detalhado, Detalhado) {
        let escuta = TcpListener::bind("127.0.0.1:0").expect("porta");
        let endereco = escuta.local_addr().expect("endereço").to_string();
        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                armar_a();
                let (mut fluxo, _) = escuta.accept().expect("aceita");
                let r = atender_conexao_detalhada(&mut fluxo, codigos, &a.ctx());
                hello_de_teste::limpar();
                r
            });
            armar_b();
            let em_b = parear_por_pin_detalhado(&endereco, pin, &b.ctx());
            hello_de_teste::limpar();
            (servidor.join().expect("thread de A"), em_b)
        })
    }

    /// Sessão entre pareados com uma porta armada em cada lado. (resultado de A, de B)
    fn sincronizar_armado(
        a: &Aparelho,
        b: &Aparelho,
        armar_a: fn(),
        armar_b: fn(),
    ) -> (Detalhado, Detalhado) {
        let escuta = TcpListener::bind("127.0.0.1:0").expect("porta");
        let endereco = escuta.local_addr().expect("endereço").to_string();
        let mut codigos = Codigos::default();
        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                armar_a();
                let (mut fluxo, _) = escuta.accept().expect("aceita");
                let r = atender_conexao_detalhada(&mut fluxo, &mut codigos, &a.ctx());
                hello_de_teste::limpar();
                r
            });
            armar_b();
            let em_b = sincronizar_com_detalhado(&endereco, &b.ctx());
            hello_de_teste::limpar();
            (servidor.join().expect("thread de A"), em_b)
        })
    }

    /// Um par já pareado em que A tem uma edição que B ainda não recebeu: a sessão seguinte
    /// ESCREVERIA em B. É o que dá sentido a "zero escrita" entre pareados.
    fn pareados_com_edicao_pendente() -> (Aparelho, Aparelho) {
        let (a, b, capitulo) = dois_pareados_com_acervo();
        a.editar(&capitulo, "<p>edição que B ainda não viu</p>");
        (a, b)
    }

    fn nada() {}

    fn incompativel(r: &Detalhado) -> Option<Incompatibilidade> {
        match r {
            Err(FalhaDaSessao::Incompativel(motivo)) => Some(motivo.clone()),
            _ => None,
        }
    }

    fn protocolo_2() {
        hello_de_teste::alterar(Some(|h| h.protocolo = 2));
    }
    fn canonico_2() {
        hello_de_teste::alterar(Some(|h| h.formato_canonico = 2));
    }
    fn modo_trocado() {
        hello_de_teste::alterar(Some(|h| {
            h.modo = if h.modo == "pin" { "pareado" } else { "pin" }.into()
        }));
    }
    fn outro_bundle() {
        hello_de_teste::alterar(Some(|h| h.formato_do_bundle = "outro".into()));
    }
    fn como_beta() {
        hello_de_teste::fingir_legado(true);
    }

    /// Um par com acervo em A e B virgem — o par que faria bootstrap.
    fn a_com_acervo() -> (Aparelho, Aparelho) {
        let a = Aparelho::novo("Desktop");
        let b = Aparelho::novo("Celular");
        universe_service::create(&a.banco.database, &a.store, &a.identidade, "Terra", "", "")
            .expect("universo");
        (a, b)
    }

    /// **E2 — protocolo e formato canônico diferentes abortam, nos dois modos, nos dois sentidos,
    /// com zero escrita dos dois lados.**
    #[test]
    fn e_protocolo_ou_canonico_diferente_aborta_sem_escrita() {
        type Esperado = fn(&Incompatibilidade) -> bool;
        let casos: [(&str, fn(), Esperado); 2] = [
            ("protocolo", protocolo_2, |m| {
                matches!(m, Incompatibilidade::Protocolo { .. })
            }),
            ("canônico", canonico_2, |m| {
                matches!(m, Incompatibilidade::FormatoCanonico { .. })
            }),
        ];
        for (nome, armar, esperado) in casos {
            // Nos dois sentidos: o diferente é quem atende, depois quem conecta.
            for diferente_em_a in [true, false] {
                let (armar_a, armar_b) = if diferente_em_a {
                    (armar, nada as fn())
                } else {
                    (nada as fn(), armar)
                };

                // PIN, num par que faria bootstrap.
                let (a, b) = a_com_acervo();
                let (antes_a, antes_b) = (retrato(&a), retrato(&b));
                let mut codigos = Codigos::default();
                let pin = pin_de(&mut codigos);
                let (em_a, em_b) = parear_armado(&a, &b, &mut codigos, &pin, armar_a, armar_b);
                for (lado, r) in [("A", &em_a), ("B", &em_b)] {
                    let motivo = incompativel(r)
                        .unwrap_or_else(|| panic!("{nome}/PIN/{lado}: não classificou: {r:?}"));
                    assert!(esperado(&motivo), "{nome}/PIN/{lado}: {motivo:?}");
                }
                assert_eq!(retrato(&a), antes_a, "{nome}/PIN: A escreveu");
                assert_eq!(retrato(&b), antes_b, "{nome}/PIN: B escreveu");

                // Entre pareados.
                let (a, b) = pareados_com_edicao_pendente();
                let (antes_a, antes_b) = (retrato(&a), retrato(&b));
                let (em_a, em_b) = sincronizar_armado(&a, &b, armar_a, armar_b);
                for (lado, r) in [("A", &em_a), ("B", &em_b)] {
                    let motivo = incompativel(r)
                        .unwrap_or_else(|| panic!("{nome}/pareado/{lado}: não classificou: {r:?}"));
                    assert!(esperado(&motivo), "{nome}/pareado/{lado}: {motivo:?}");
                }
                assert_eq!(retrato(&a), antes_a, "{nome}/pareado: A escreveu");
                assert_eq!(retrato(&b), antes_b, "{nome}/pareado: B escreveu");
            }
        }
    }

    /// **E2 — o modo do Hello tem de ser o do quadro em claro.** Quem recebe o modo trocado aborta
    /// classificando; o outro lado perde a conexão. Nenhum dos dois escreve.
    #[test]
    fn e_modo_divergente_aborta_sem_escrita() {
        for mentiroso_em_a in [true, false] {
            let (armar_a, armar_b) = if mentiroso_em_a {
                (modo_trocado as fn(), nada as fn())
            } else {
                (nada as fn(), modo_trocado as fn())
            };

            let (a, b) = a_com_acervo();
            let (antes_a, antes_b) = (retrato(&a), retrato(&b));
            let mut codigos = Codigos::default();
            let pin = pin_de(&mut codigos);
            let (em_a, em_b) = parear_armado(&a, &b, &mut codigos, &pin, armar_a, armar_b);
            let honesto = if mentiroso_em_a { &em_b } else { &em_a };
            assert!(
                matches!(incompativel(honesto), Some(Incompatibilidade::Modo { .. })),
                "PIN: {honesto:?}"
            );
            assert!(em_a.is_err() && em_b.is_err());
            assert_eq!(retrato(&a), antes_a);
            assert_eq!(retrato(&b), antes_b);

            let (a, b) = pareados_com_edicao_pendente();
            let (antes_a, antes_b) = (retrato(&a), retrato(&b));
            let (em_a, em_b) = sincronizar_armado(&a, &b, armar_a, armar_b);
            let honesto = if mentiroso_em_a { &em_b } else { &em_a };
            assert!(
                matches!(incompativel(honesto), Some(Incompatibilidade::Modo { .. })),
                "pareado: {honesto:?}"
            );
            assert!(em_a.is_err() && em_b.is_err());
            assert_eq!(retrato(&a), antes_a);
            assert_eq!(retrato(&b), antes_b);
        }
    }

    /// **E3 — o Sync V2 anterior ao Hello é PeerLegado, não "JSON inválido", e nada é escrito.**
    ///
    /// O aparelho beta abre com `Estado` (PIN) ou `Autorizacao` (pareado) onde o protocolo 1 espera
    /// `Hello`. Quando é ele quem conecta, o lado novo o reconhece pelo que ele é.
    #[test]
    fn e_peer_beta_e_classificado_como_legado_sem_escrita() {
        // PIN: a beta digita o código.
        let (a, b) = a_com_acervo();
        let (antes_a, antes_b) = (retrato(&a), retrato(&b));
        let mut codigos = Codigos::default();
        let pin = pin_de(&mut codigos);
        let (em_a, em_b) = parear_armado(&a, &b, &mut codigos, &pin, nada, como_beta);
        assert_eq!(
            incompativel(&em_a),
            Some(Incompatibilidade::PeerLegado {
                abriu_com: "Estado"
            }),
            "{em_a:?}"
        );
        let mensagem = DatabaseCommandError::from(em_a.expect_err("A")).message;
        assert!(
            mensagem.contains("versão beta antiga do Sync V2"),
            "{mensagem}"
        );
        assert!(em_b.is_err(), "a beta não podia concluir");
        assert_eq!(retrato(&a), antes_a, "o lado novo escreveu");
        assert_eq!(retrato(&b), antes_b, "o lado beta escreveu");

        // Pareados: a beta conecta para sincronizar.
        let (a, b) = pareados_com_edicao_pendente();
        let (antes_a, antes_b) = (retrato(&a), retrato(&b));
        let (em_a, em_b) = sincronizar_armado(&a, &b, nada, como_beta);
        assert_eq!(
            incompativel(&em_a),
            Some(Incompatibilidade::PeerLegado {
                abriu_com: "Autorizacao"
            }),
            "{em_a:?}"
        );
        assert!(em_b.is_err());
        assert_eq!(retrato(&a), antes_a);
        assert_eq!(retrato(&b), antes_b);

        // E quando é o lado novo quem conecta numa beta: a beta morre no Hello, e o novo diz por quê.
        let (a, b) = pareados_com_edicao_pendente();
        let (antes_a, antes_b) = (retrato(&a), retrato(&b));
        let (em_a, em_b) = sincronizar_armado(&a, &b, como_beta, nada);
        assert!(em_a.is_err(), "a beta não podia concluir");
        let erro = DatabaseCommandError::from(em_b.expect_err("o novo não podia concluir"));
        assert!(
            erro.message.contains("versão beta antiga"),
            "{}",
            erro.message
        );
        assert_eq!(retrato(&a), antes_a);
        assert_eq!(retrato(&b), antes_b);
    }

    /// **PIN autenticado + Hello incompatível = PIN consumido.** O mesmo código não vale de novo;
    /// um código novo, com versões compatíveis, pareia.
    #[test]
    fn e_pin_de_sessao_incompativel_fica_consumido() {
        let (a, b) = a_com_acervo();
        let mut codigos = Codigos::default();
        let pin = pin_de(&mut codigos);
        let (em_a, _) = parear_armado(&a, &b, &mut codigos, &pin, protocolo_2, nada);
        assert!(incompativel(&em_a).is_some());

        let (em_a, em_b) = parear_armado(&a, &b, &mut codigos, &pin, nada, nada);
        assert!(
            em_a.is_err() && em_b.is_err(),
            "o PIN reviveu: {em_a:?} {em_b:?}"
        );
        assert!(a.roster().is_empty() && b.roster().is_empty());

        let novo = pin_de(&mut codigos);
        let (em_a, em_b) = parear_armado(&a, &b, &mut codigos, &novo, nada, nada);
        assert_eq!(em_a.expect("A").papel, Papel::Doador);
        assert_eq!(em_b.expect("B").papel, Papel::Receptor);
    }

    /// **E4 — a impressão do bundle só decide quando há bundle.**
    ///
    /// Com bootstrap (Doador/Receptor), impressões diferentes abortam os dois lados antes de
    /// capturar ou receber qualquer coisa. Entre Pares, a mesma diferença não impede nada.
    #[test]
    fn e_bundle_diferente_so_impede_bootstrap() {
        for diferente_em_a in [true, false] {
            let (armar_a, armar_b) = if diferente_em_a {
                (outro_bundle as fn(), nada as fn())
            } else {
                (nada as fn(), outro_bundle as fn())
            };
            let (a, b) = a_com_acervo();
            let (antes_a, antes_b) = (retrato(&a), retrato(&b));
            let mut codigos = Codigos::default();
            let pin = pin_de(&mut codigos);
            let (em_a, em_b) = parear_armado(&a, &b, &mut codigos, &pin, armar_a, armar_b);
            for r in [&em_a, &em_b] {
                assert!(
                    matches!(
                        incompativel(r),
                        Some(Incompatibilidade::FormatoDoBundle { .. })
                    ),
                    "{r:?}"
                );
            }
            assert_eq!(retrato(&a), antes_a, "o doador escreveu");
            assert_eq!(retrato(&b), antes_b, "o receptor escreveu");

            // Par (os dois virgens): a diferença é irrelevante.
            let a = Aparelho::novo("Desktop");
            let b = Aparelho::novo("Celular");
            let mut codigos = Codigos::default();
            let pin = pin_de(&mut codigos);
            let (em_a, em_b) = parear_armado(&a, &b, &mut codigos, &pin, armar_a, armar_b);
            assert_eq!(em_a.expect("A: Par").papel, Papel::Par);
            assert_eq!(em_b.expect("B: Par").papel, Papel::Par);

            // E entre pareados com acervo, também.
            let (a, b) = pareados_com_edicao_pendente();
            let (em_a, em_b) = sincronizar_armado(&a, &b, armar_a, armar_b);
            em_a.expect("A: incremental com bundle diferente");
            em_b.expect("B: incremental com bundle diferente");
        }
    }

    /// O `Hello` aceita campo que ainda não existe: é por ele que uma versão futura explica a
    /// diferença.
    #[test]
    fn e_hello_tolera_campo_novo() {
        let json = r#"{"tipo":"Hello","dados":{"protocolo":1,"formato_canonico":1,
            "formato_do_bundle":"x","modo":"pin","app":"9.9.9","capacidades":["futuro"]}}"#;
        match serde_json::from_str::<Mensagem>(json).expect("campo novo não pode derrubar") {
            Mensagem::Hello(hello) => assert_eq!(hello.protocolo, 1),
            outra => panic!("{outra:?}"),
        }
    }

    /// **O número canônico do fio e o da adoção são o mesmo, por construção.**
    #[test]
    fn e_formato_canonico_e_a_versao_da_adocao() {
        assert_eq!(
            crate::application::genese::VERSAO_DA_ADOCAO,
            crate::infrastructure::sqlite::sync_codec::FORMATO_CANONICO_ATUAL
        );
    }
}
