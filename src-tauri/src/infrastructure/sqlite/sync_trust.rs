//! Sync V2 — confiança de origem: roster e verificação (ADR 0009 §5.2, etapa 7).
//!
//! Até a etapa 6 valia uma coisa desconfortável e deliberada:
//!
//! ```text
//! conhecida  ≠  confiável
//! ```
//!
//! O evento nascia assinado e a assinatura chegava intacta ao outro lado — mas
//! ninguém perguntava *"devo confiar nesta chave?"*. Esta etapa faz a
//! pergunta, e recusa quando a resposta é não.
//!
//! ## A cadeia, na ordem em que ela precisa acontecer
//!
//! ```text
//! Envelope chegou
//!       │
//!       ▼
//! device_id no roster?  ── não ──▶  QUARENTENA  (não entra no log)
//!       │
//!       ▼
//! estado do dispositivo ── revoked/retired ──▶  RECUSA
//!       │
//!       ▼
//! a chave pública bate com o device_id?  ── não ──▶  RECUSA
//!       │
//!       ▼
//! Ed25519.verify()      ── false ──▶  RECUSA
//!       │
//!       ▼
//! causalidade (etapas 4 e 5)
//! ```
//!
//! A ordem não é arbitrária. Verificar assinatura antes de saber de quem ela
//! é gastaria criptografia com qualquer lixo que chegasse pela rede; e checar
//! o estado antes da assinatura evita processar evento de aparelho revogado.
//!
//! ### Por que o terceiro passo existe
//!
//! O `device_id` **é** o fingerprint da chave pública. Se a linha do roster
//! tiver uma chave que não deriva daquele id, a verificação seguinte estaria
//! validando contra a chave errada — e um roster adulterado passaria a
//! aceitar eventos forjados de um `device_id` legítimo. É barato e fecha o
//! caminho inteiro.
//!
//! ## Entrar no roster não é automático
//!
//! Um dispositivo entra de dois jeitos, e nenhum acontece sozinho:
//!
//! - **pareamento direto** (etapa 9);
//! - **introdução autorizada**: um dispositivo `active` adiciona outro.
//!
//! Sem isso, parear com um aparelho passaria a significar aceitar tudo que ele
//! repassar, de qualquer origem — e um relay comprometido viraria porta de
//! entrada para conteúdo forjado.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::{decode_base32, fingerprint, verify};
use crate::domain::sync::EventEnvelope;
use crate::infrastructure::sync_transport::SessaoAutenticada;
use ed25519_dalek::VerifyingKey;
use rusqlite::{Connection, OptionalExtension};

/// Por que um envelope foi recusado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recusa {
    /// A origem não está no roster. **Não é erro de segurança**: é um
    /// aparelho pedindo para entrar no conjunto, e quem decide é o humano.
    OrigemDesconhecida { device_id: String },
    /// Origem aposentada ou revogada. Eventos novos dela não entram.
    OrigemInativa { device_id: String, estado: String },
    /// A chave do roster não deriva daquele `device_id`. Roster adulterado.
    RosterInconsistente { device_id: String },
    /// A assinatura não confere com a chave da origem.
    AssinaturaInvalida { device_id: String },
}

impl Recusa {
    /// Se isto merece aparecer para o escritor como pedido de entrada, e não
    /// como incidente. Distinguir importa: um aparelho novo é rotina, uma
    /// assinatura inválida não é.
    pub fn e_pedido_de_entrada(&self) -> bool {
        matches!(self, Recusa::OrigemDesconhecida { .. })
    }
}

impl std::fmt::Display for Recusa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Recusa::OrigemDesconhecida { device_id } => write!(
                f,
                "Dispositivo desconhecido quer entrar no conjunto: {device_id}"
            ),
            Recusa::OrigemInativa { device_id, estado } => {
                write!(f, "Dispositivo {device_id} está {estado}")
            }
            Recusa::RosterInconsistente { device_id } => write!(
                f,
                "A chave registrada para {device_id} não deriva desse identificador"
            ),
            Recusa::AssinaturaInvalida { device_id } => {
                write!(f, "Assinatura inválida em evento de {device_id}")
            }
        }
    }
}

/// A cadeia inteira, na ordem do cabeçalho.
pub fn verificar_origem(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Result<(), Recusa>> {
    let registro: Option<(String, String)> = connection
        .query_row(
            "SELECT ed25519_public, state FROM sync_devices WHERE device_id = ?1",
            [&envelope.device_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let Some((publica, estado)) = registro else {
        return Ok(Err(Recusa::OrigemDesconhecida {
            device_id: envelope.device_id.clone(),
        }));
    };

    if estado != "active" {
        return Ok(Err(Recusa::OrigemInativa {
            device_id: envelope.device_id.clone(),
            estado,
        }));
    }

    if !a_chave_deriva_o_id(&publica, &envelope.device_id) {
        return Ok(Err(Recusa::RosterInconsistente {
            device_id: envelope.device_id.clone(),
        }));
    }

    if !verify(envelope, &publica) {
        return Ok(Err(Recusa::AssinaturaInvalida {
            device_id: envelope.device_id.clone(),
        }));
    }

    Ok(Ok(()))
}

/// O `device_id` é o fingerprint da pública. Se não bate, a linha do roster
/// não descreve o aparelho que diz descrever.
pub fn a_chave_deriva_o_id(publica_base32: &str, device_id: &str) -> bool {
    let Some(bytes) = decode_base32(publica_base32) else {
        return false;
    };
    let Ok(bytes) = <[u8; 32]>::try_from(bytes.as_slice()) else {
        return false;
    };
    let Ok(chave) = VerifyingKey::from_bytes(&bytes) else {
        return false;
    };
    fingerprint(&chave) == device_id
}

/// Admite um dispositivo no roster por **introdução autorizada**.
///
/// # A autoridade vem da sessão, não de um argumento
///
/// A primeira versão desta função recebia `quem_introduz: &str` e consultava
/// o roster com ele. Local e em teste, correto. **Com rede, não:**
///
/// ```text
/// peer remoto:  "quem introduziu este aparelho foi DESKTOP-ABC"
///                           ↓
///         SELECT state FROM sync_devices WHERE device_id = 'DESKTOP-ABC'
///                           ↓
///                      active → aceita
/// ```
///
/// Qualquer peer pode **mencionar** o id de um aparelho ativo. Um `&str` não
/// prova nada sobre quem está falando.
///
/// Por isso o parâmetro é [`SessaoAutenticada`] — um tipo que só existe depois
/// de alguém ter assinado o hash do handshake com a Ed25519 correspondente.
/// Não há construtor a partir de um `device_id` recebido pela rede, e é essa
/// ausência que carrega a garantia.
///
/// Quem introduz ainda precisa estar `active`: senão parear com um aparelho
/// passaria a significar aceitar tudo que ele repassar, de qualquer origem.
///
/// A chave também precisa derivar o `device_id` — senão o roster passaria a
/// conter uma linha que a etapa de verificação usaria contra a chave errada.
pub fn introduzir_dispositivo(
    connection: &Connection,
    sessao: &SessaoAutenticada,
    device_id: &str,
    ed25519_public: &str,
) -> DatabaseCommandResult<()> {
    let quem_introduz = sessao.device_id();
    let estado_do_introdutor: Option<String> = connection
        .query_row(
            "SELECT state FROM sync_devices WHERE device_id = ?1",
            [quem_introduz],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    match estado_do_introdutor.as_deref() {
        Some("active") => {}
        Some(estado) => {
            return Err(DatabaseCommandError::validation(format!(
                "O dispositivo {quem_introduz} está {estado} e não pode apresentar outros."
            )))
        }
        None => {
            return Err(DatabaseCommandError::validation(format!(
                "O dispositivo {quem_introduz} não está no roster e não pode apresentar outros."
            )))
        }
    }

    if !a_chave_deriva_o_id(ed25519_public, device_id) {
        return Err(DatabaseCommandError::validation(format!(
            "A chave apresentada para {device_id} não deriva desse identificador. Aceitar \
             gravaria no roster uma linha que a verificação de assinatura usaria contra a \
             chave errada."
        )));
    }

    connection
        .execute(
            "INSERT INTO sync_devices (device_id, name, ed25519_public, introduced_by, is_self)
             VALUES (?1, '', ?2, ?3, 0)
             ON CONFLICT(device_id) DO NOTHING",
            rusqlite::params![device_id, ed25519_public, quem_introduz],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

/// Muda o estado de um dispositivo do roster.
///
/// `retired` é decisão administrativa sobre um aparelho que era confiável;
/// `revoked` diz que a chave caiu em mãos erradas. Nenhum dos dois apaga o que
/// veio antes: os eventos continuam no log, e o que foi aplicado continua
/// aplicado. Apagar automaticamente destruiria trabalho legítimo feito antes
/// do comprometimento (ADR 0009 §5.1).
pub fn mudar_estado(
    connection: &Connection,
    device_id: &str,
    estado: &str,
) -> DatabaseCommandResult<()> {
    if !matches!(estado, "active" | "retired" | "revoked") {
        return Err(DatabaseCommandError::validation(format!(
            "Estado de dispositivo desconhecido: {estado}"
        )));
    }
    let e_self: bool = connection
        .query_row(
            "SELECT is_self FROM sync_devices WHERE device_id = ?1",
            [device_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .map(|valor| valor == 1)
        .unwrap_or(false);

    if e_self && estado != "active" {
        return Err(DatabaseCommandError::validation(
            "Este aparelho não pode aposentar nem revogar a si mesmo: ele deixaria de conseguir \
             gravar as próprias alterações, e nenhuma sincronização consertaria isso depois.",
        ));
    }

    connection
        .execute(
            "UPDATE sync_devices
                SET state = ?2, state_changed_at = datetime('now')
              WHERE device_id = ?1",
            rusqlite::params![device_id, estado],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::DeviceIdentity;
    use crate::domain::sync::{AggregateRef, Operation};
    use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
    use crate::infrastructure::sqlite::sync_session::receber_eventos;
    use crate::infrastructure::sqlite::test_support::{
        origem_remota_confiavel, seed_universe, self_de_teste, TemporaryDatabase,
    };
    use crate::infrastructure::sync_transport::SessaoAutenticada;

    struct Cenario {
        fixture: TemporaryDatabase,
        eu: DeviceIdentity,
        remota: DeviceIdentity,
    }

    fn cenario() -> Cenario {
        let fixture = TemporaryDatabase::new();
        let (eu, remota) = {
            let connection = fixture.database.write().expect("escrita");
            seed_universe(&connection, "u1");
            connection
                .execute_batch(
                    "INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Historia');
                     INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');",
                )
                .expect("semear");
            let eu = self_de_teste(&connection);
            let remota = origem_remota_confiavel(&connection, &eu);
            (eu, remota)
        };
        Cenario {
            fixture,
            eu,
            remota,
        }
    }

    fn capitulo(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","book_id":"b1","title":"Titulo","content":"texto","summary":"","scene_origin":"","scene_destination":"","word_count":1,"status":"rascunho","canon_status":"canon","sort_order":0,"created_at":"2026-01-01 00:00:00","updated_at":"2026-01-02 00:00:00"}}"#
        )
    }

    fn evento_assinado(origem: &DeviceIdentity, id: &str) -> EventEnvelope {
        let mut envelope = envelope_de_origem(
            origem.device_id(),
            1,
            "u1",
            &AggregateRef::new("chapter", id),
            Operation::Upsert,
            &capitulo(id),
            "",
        );
        envelope.signature = origem.sign(&envelope);
        envelope
    }

    // ── ponto 1 da cadeia: device_id no roster ──────────────────────────────

    /// Origem fora do roster **não entra no log**, nem mesmo guardada.
    ///
    /// Guardar daria a ela aparência de legítima na sessão seguinte, e o relay
    /// a repassaria adiante — transformando este aparelho em porta de entrada
    /// para conteúdo de procedência desconhecida.
    #[test]
    fn origem_fora_do_roster_e_recusada_e_nao_entra_no_log() {
        let cenario = cenario();
        let forasteiro = DeviceIdentity::generate();
        let evento = evento_assinado(&forasteiro, "cap-1");

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[evento]).expect("receber");

        assert_eq!(relatorio.aplicados, 0);
        assert_eq!(relatorio.recusados.len(), 1);
        assert!(
            matches!(relatorio.recusados[0], Recusa::OrigemDesconhecida { .. }),
            "recusou pelo motivo errado: {:?}",
            relatorio.recusados[0]
        );

        let guardados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(guardados, 0, "um envelope não verificado entrou no log");
    }

    /// E aparece como **pedido de entrada**, não como incidente.
    ///
    /// Um aparelho novo é rotina; assinatura inválida não é. Misturar as duas
    /// coisas na tela treinaria o escritor a ignorar as duas.
    #[test]
    fn origem_desconhecida_e_pedido_de_entrada_e_nao_incidente() {
        let cenario = cenario();
        let forasteiro = DeviceIdentity::generate();
        let evento = evento_assinado(&forasteiro, "cap-1");

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[evento]).expect("receber");

        assert_eq!(relatorio.pedidos_de_entrada().len(), 1);
        assert_eq!(relatorio.incidentes().len(), 0);
    }

    // ── ponto 2 da cadeia: estado do dispositivo ────────────────────────────

    #[test]
    fn origem_revogada_e_recusada() {
        let cenario = cenario();
        let evento = evento_assinado(&cenario.remota, "cap-1");

        {
            let connection = cenario.fixture.database.write().expect("escrita");
            mudar_estado(&connection, cenario.remota.device_id(), "revoked").expect("revogar");
        }

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[evento]).expect("receber");

        assert_eq!(relatorio.aplicados, 0);
        assert!(matches!(
            relatorio.recusados[0],
            Recusa::OrigemInativa { .. }
        ));
        assert_eq!(relatorio.incidentes().len(), 1, "revogação é incidente");
    }

    #[test]
    fn origem_aposentada_nao_manda_evento_novo() {
        let cenario = cenario();
        let evento = evento_assinado(&cenario.remota, "cap-1");

        {
            let connection = cenario.fixture.database.write().expect("escrita");
            mudar_estado(&connection, cenario.remota.device_id(), "retired").expect("aposentar");
        }

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[evento]).expect("receber");
        assert_eq!(relatorio.aplicados, 0);
    }

    /// Revogar não apaga o que já foi aplicado.
    ///
    /// A chave caiu em mãos erradas **agora**; o trabalho legítimo feito antes
    /// continua sendo trabalho legítimo. Apagar automaticamente destruiria
    /// conteúdo do escritor por causa de um evento de segurança (ADR §5.1).
    #[test]
    fn revogar_nao_apaga_o_que_ja_tinha_sido_aplicado() {
        let cenario = cenario();
        let antes = evento_assinado(&cenario.remota, "cap-antigo");

        let mut connection = cenario.fixture.database.write().expect("escrita");
        receber_eventos(&mut connection, &[antes]).expect("receber");
        let existia: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM chapters WHERE id = 'cap-antigo'",
                [],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(existia, 1);

        mudar_estado(&connection, cenario.remota.device_id(), "revoked").expect("revogar");

        let continua: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM chapters WHERE id = 'cap-antigo'",
                [],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(
            continua, 1,
            "revogar apagou conteúdo que o escritor tinha de verdade"
        );
    }

    /// Este aparelho não pode se revogar.
    ///
    /// Ele deixaria de conseguir gravar as próprias alterações, e nenhuma
    /// sincronização consertaria isso depois — o `self` é quem assina.
    #[test]
    fn o_proprio_aparelho_nao_pode_se_revogar() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        mudar_estado(&connection, cenario.eu.device_id(), "revoked")
            .expect_err("o self não pode se revogar");
    }

    // ── ponto 3 da cadeia: a chave bate com o device_id ─────────────────────

    /// Roster adulterado: a linha existe, mas com uma chave que não deriva
    /// aquele `device_id`.
    ///
    /// Sem este passo, a verificação seguinte validaria contra a chave errada
    /// — e quem controlasse a chave plantada passaria a assinar eventos em
    /// nome de um `device_id` legítimo.
    #[test]
    fn chave_que_nao_deriva_o_device_id_e_recusada() {
        let cenario = cenario();
        let impostor = DeviceIdentity::generate();

        // O evento é assinado pelo impostor, e a chave dele é plantada na
        // linha da origem legítima.
        let mut envelope = envelope_de_origem(
            cenario.remota.device_id(),
            1,
            "u1",
            &AggregateRef::new("chapter", "cap-1"),
            Operation::Upsert,
            &capitulo("cap-1"),
            "",
        );
        envelope.signature = impostor.sign(&envelope);

        {
            let connection = cenario.fixture.database.write().expect("escrita");
            connection
                .execute(
                    "UPDATE sync_devices SET ed25519_public = ?2 WHERE device_id = ?1",
                    rusqlite::params![cenario.remota.device_id(), impostor.public_base32()],
                )
                .expect("plantar chave");
        }

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[envelope]).expect("receber");

        assert_eq!(relatorio.aplicados, 0);
        assert!(
            matches!(relatorio.recusados[0], Recusa::RosterInconsistente { .. }),
            "a assinatura do impostor foi validada contra a chave plantada: {:?}",
            relatorio.recusados[0]
        );
    }

    // ── ponto 4 da cadeia: Ed25519.verify ──────────────────────────────────

    #[test]
    fn assinatura_invalida_e_recusada_e_conta_como_incidente() {
        let cenario = cenario();
        let mut evento = evento_assinado(&cenario.remota, "cap-1");
        // Adultera o conteúdo depois de assinar — o caso do relay malicioso.
        evento.payload = capitulo("cap-1").replace("Titulo", "Adulterado");

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[evento]).expect("receber");

        assert_eq!(relatorio.aplicados, 0);
        assert!(matches!(
            relatorio.recusados[0],
            Recusa::AssinaturaInvalida { .. }
        ));
        assert_eq!(relatorio.incidentes().len(), 1);

        let guardados: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(guardados, 0);
    }

    #[test]
    fn evento_sem_assinatura_e_recusado() {
        let cenario = cenario();
        let mut evento = evento_assinado(&cenario.remota, "cap-1");
        evento.signature = String::new();

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[evento]).expect("receber");
        assert!(matches!(
            relatorio.recusados[0],
            Recusa::AssinaturaInvalida { .. }
        ));
    }

    /// E o caminho feliz: origem no roster, ativa, chave coerente, assinatura
    /// válida. Sem isto os testes acima poderiam estar recusando tudo.
    #[test]
    fn origem_legitima_atravessa_a_cadeia_inteira() {
        let cenario = cenario();
        let evento = evento_assinado(&cenario.remota, "cap-1");

        let mut connection = cenario.fixture.database.write().expect("escrita");
        let relatorio = receber_eventos(&mut connection, &[evento]).expect("receber");

        assert_eq!(relatorio.aplicados, 1);
        assert!(relatorio.recusados.is_empty());
    }

    // ── admissão no roster ─────────────────────────────────────────────────

    /// Quem introduz precisa estar `active`.
    ///
    /// É o que impede o pareamento de virar transitivo por acidente: um relay
    /// comprometido não pode se apresentar trazendo origens inventadas junto.
    /// GATE DA NH-056: a autoridade para introduzir vem da **sessão
    /// autenticada**, e um peer não consegue fabricá-la citando um id ativo.
    ///
    /// Antes desta etapa, `introduzir_dispositivo` recebia `quem_introduz:
    /// &str`. Com rede, qualquer peer poderia mandar o id de um aparelho ativo
    /// e a consulta ao roster diria "active → aceita".
    ///
    /// Agora o parâmetro é `SessaoAutenticada`, e o único construtor pede uma
    /// `DeviceIdentity` — que só existe com a chave privada em mãos. O teste
    /// demonstra a diferença: quem tem a chave consegue; quem só sabe o id,
    /// não tem por onde construir a autoridade.
    #[test]
    fn a_autoridade_para_introduzir_exige_a_chave_e_nao_o_id() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let novo = DeviceIdentity::generate();

        // Quem tem a chave do aparelho ativo consegue apresentar.
        let com_a_chave = SessaoAutenticada::deste_aparelho(&cenario.remota);
        introduzir_dispositivo(
            &connection,
            &com_a_chave,
            novo.device_id(),
            &novo.public_base32(),
        )
        .expect("quem controla a identidade ativa apresenta");

        // E o `device_id` da sessão é o da chave, não algo escolhido: não há
        // caminho para dizer "sou o DESKTOP-ABC" sem ter a privada dele.
        assert_eq!(com_a_chave.device_id(), cenario.remota.device_id());

        // Um impostor que conheça o id do aparelho ativo — informação pública,
        // que viaja no vetor de sequências — só consegue montar uma sessão com
        // a própria identidade.
        let impostor = DeviceIdentity::generate();
        let sessao_do_impostor = SessaoAutenticada::deste_aparelho(&impostor);
        assert_ne!(
            sessao_do_impostor.device_id(),
            cenario.remota.device_id(),
            "um impostor conseguiu montar uma sessão com o device_id alheio"
        );

        let outro = DeviceIdentity::generate();
        introduzir_dispositivo(
            &connection,
            &sessao_do_impostor,
            outro.device_id(),
            &outro.public_base32(),
        )
        .expect_err("o impostor não está no roster e não pode apresentar ninguém");
    }

    #[test]
    fn dispositivo_revogado_nao_apresenta_outros() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        mudar_estado(&connection, cenario.remota.device_id(), "revoked").expect("revogar");

        let novo = DeviceIdentity::generate();
        let sessao = SessaoAutenticada::deste_aparelho(&cenario.remota);
        introduzir_dispositivo(
            &connection,
            &sessao,
            novo.device_id(),
            &novo.public_base32(),
        )
        .expect_err("um dispositivo revogado não pode apresentar outros");
    }

    #[test]
    fn dispositivo_fora_do_roster_nao_apresenta_outros() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let estranho = DeviceIdentity::generate();
        let novo = DeviceIdentity::generate();

        let sessao = SessaoAutenticada::deste_aparelho(&estranho);
        introduzir_dispositivo(
            &connection,
            &sessao,
            novo.device_id(),
            &novo.public_base32(),
        )
        .expect_err("quem não está no roster não apresenta ninguém");
    }

    /// Introduzir uma chave que não deriva o `device_id` é recusado na porta.
    ///
    /// Aceitar gravaria no roster exatamente a linha que o ponto 3 da cadeia
    /// existe para pegar — melhor não deixar entrar.
    #[test]
    fn introducao_com_chave_incoerente_e_recusada() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let novo = DeviceIdentity::generate();
        let outra_chave = DeviceIdentity::generate();

        let sessao = SessaoAutenticada::deste_aparelho(&cenario.eu);
        introduzir_dispositivo(
            &connection,
            &sessao,
            novo.device_id(),
            &outra_chave.public_base32(),
        )
        .expect_err("a chave precisa derivar o device_id");
    }

    #[test]
    fn introducao_autorizada_registra_quem_apresentou() {
        let cenario = cenario();
        let connection = cenario.fixture.database.write().expect("escrita");
        let novo = DeviceIdentity::generate();

        let sessao = SessaoAutenticada::deste_aparelho(&cenario.remota);
        introduzir_dispositivo(
            &connection,
            &sessao,
            novo.device_id(),
            &novo.public_base32(),
        )
        .expect("introdução por dispositivo ativo");

        let quem: String = connection
            .query_row(
                "SELECT introduced_by FROM sync_devices WHERE device_id = ?1",
                [novo.device_id()],
                |row| row.get(0),
            )
            .expect("ler introduced_by");
        assert_eq!(
            quem,
            cenario.remota.device_id(),
            "quem apresentou precisa ficar registrado: é a trilha de como o \
             aparelho entrou no conjunto"
        );
    }
}
