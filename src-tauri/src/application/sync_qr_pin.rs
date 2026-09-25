//! Pareamento por PIN assistido por QR (Fase 4.5, NH-084, PR B).
//!
//! ## O que este QR é — e o que ele não é
//!
//! ```text
//! QR  →  endpoint + PIN  →  SPAKE2 existente  →  Noise XXpsk0  →  prova Ed25519  →  SessaoAutenticada
//! ```
//!
//! É **açúcar de UX sobre o PIN**: o celular lê pela câmera o que hoje se digita — endereço e os oito
//! dígitos — e segue exatamente o caminho do PIN que a Etapa I qualificou. Nada de modo novo no fio,
//! nada de Hello, Noise, roster ou confiança diferentes. A segurança é a do PIN/PAKE: três
//! tentativas, validade de três minutos e uso único, **controlados pela escuta**, nunca pelo relógio
//! de quem escaneou.
//!
//! O QR criptográfico da ADR 0009 §6.1 (`infrastructure/sync_pairing.rs`: segredo de 32 bytes,
//! `invitation_id`) **não é este**. Ele continua implementado e testado no core, sem ligação com o
//! fio nem qualificação, e nada daqui o reaproveita.
//!
//! ## Formato
//!
//! ```text
//! narrahub-pair-pin:1:<ipv4>:<porta>:<8 dígitos>
//! ```
//!
//! Mínimo de propósito: sem nome do aparelho, sem identidade. Descoberta não é confiança — quem chegou
//! é o que a sessão autenticada provar, nunca o que um texto de QR declarar.
//!
//! O formato mora **só aqui**. O frontend transforma o conteúdo opaco em imagem e devolve ao Rust a
//! string crua que o leitor de código de barras leu; ele não interpreta nada.
//!
//! ## Segredo fora de log
//!
//! O conteúdo carrega o PIN. Nenhuma mensagem de erro repete a entrada, o `Debug` do convite esconde
//! o PIN, e este módulo não escreve em log.

use crate::application::sync_sessao::{parear_por_pin, Contexto, ResultadoDaSessao};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use std::net::SocketAddrV4;

/// Prefixo do conteúdo. Distinto de `narrahub-pair` (o convite da §6.1) de propósito.
pub const PREFIXO: &str = "narrahub-pair-pin";

/// Versão do formato. Outro número é recusado — nunca interpretado "do jeito possível".
pub const VERSAO: u32 = 1;

/// O maior conteúdo aceito. `narrahub-pair-pin:1:255.255.255.255:65535:12345678` tem 51 caracteres;
/// um texto bem maior não é um convite nosso, e não merece ser lido até o fim.
const MAIOR_CONTEUDO: usize = 64;

const DIGITOS_DO_PIN: usize = 8;

/// O que o QR entrega depois de validado: onde conectar e o PIN a usar.
#[derive(Clone, PartialEq, Eq)]
pub struct ConvitePorPin {
    pub endpoint: String,
    pub pin: String,
}

impl std::fmt::Debug for ConvitePorPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvitePorPin")
            .field("endpoint", &self.endpoint)
            .field("pin", &"<oculto>")
            .finish()
    }
}

/// Por que um conteúdo foi recusado. As mensagens nunca repetem o que foi lido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalhaDoQr {
    /// Não é um QR de pareamento do NarraHub.
    NaoENarrahub,
    /// É do NarraHub, de outra versão do formato.
    VersaoIncompativel,
    /// O endereço não é um IPv4 com porta.
    EnderecoInvalido,
    /// O código não tem exatamente oito dígitos.
    CodigoInvalido,
}

impl std::fmt::Display for FalhaDoQr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FalhaDoQr::NaoENarrahub => {
                "Este QR não é de pareamento do NarraHub. Use o QR que aparece em \"Escutar nesta \
                 rede\" no outro aparelho, ou digite o endereço e o código."
            }
            FalhaDoQr::VersaoIncompativel => {
                "Este QR veio de outra versão do NarraHub. Atualize os dois aparelhos, ou digite o \
                 endereço e o código."
            }
            FalhaDoQr::EnderecoInvalido => {
                "O endereço deste QR não é válido. Gere um código novo no outro aparelho, ou digite o \
                 endereço à mão."
            }
            FalhaDoQr::CodigoInvalido => {
                "O código deste QR não é válido. Gere um código novo no outro aparelho e escaneie de \
                 novo."
            }
        })
    }
}

/// O conteúdo do QR para uma escuta aberta. Recusa o que não passaria em [`interpretar`].
pub fn montar(endpoint: &str, pin: &str) -> Result<String, FalhaDoQr> {
    validar_endpoint(endpoint)?;
    validar_pin(pin)?;
    Ok(format!("{PREFIXO}:{VERSAO}:{endpoint}:{pin}"))
}

/// Interpreta a string crua que o leitor de QR devolveu.
///
/// Estrito: prefixo e versão exatos, IPv4 canônico com porta, oito dígitos e nada mais. Nenhum espaço,
/// nenhum campo extra. Um conteúdo recusado aqui nunca chega à rede.
pub fn interpretar(texto: &str) -> Result<ConvitePorPin, FalhaDoQr> {
    if texto.len() > MAIOR_CONTEUDO {
        return Err(FalhaDoQr::NaoENarrahub);
    }
    let resto = texto
        .strip_prefix(PREFIXO)
        .and_then(|resto| resto.strip_prefix(':'))
        .ok_or(FalhaDoQr::NaoENarrahub)?;
    let (versao, resto) = resto.split_once(':').ok_or(FalhaDoQr::NaoENarrahub)?;
    if versao != VERSAO.to_string() {
        return Err(if versao.parse::<u32>().is_ok() {
            FalhaDoQr::VersaoIncompativel
        } else {
            FalhaDoQr::NaoENarrahub
        });
    }
    // O endpoint tem `:` (a porta); o PIN é o último campo.
    let (endpoint, pin) = resto.rsplit_once(':').ok_or(FalhaDoQr::CodigoInvalido)?;
    validar_endpoint(endpoint)?;
    validar_pin(pin)?;
    Ok(ConvitePorPin {
        endpoint: endpoint.to_string(),
        pin: pin.to_string(),
    })
}

/// Pareia com o aparelho do QR pelo caminho do PIN — o mesmo, sem atalho.
pub fn parear_por_qr(
    conteudo: &str,
    ctx: &Contexto<'_>,
) -> DatabaseCommandResult<ResultadoDaSessao> {
    let convite = interpretar(conteudo)
        .map_err(|falha| DatabaseCommandError::validation(falha.to_string()))?;
    parear_por_pin(&convite.endpoint, &convite.pin, ctx)
}

fn validar_endpoint(endpoint: &str) -> Result<(), FalhaDoQr> {
    let endereco: SocketAddrV4 = endpoint.parse().map_err(|_| FalhaDoQr::EnderecoInvalido)?;
    let ip = endereco.ip();
    // Canônico: `192.168.001.010` e parecidos não passam — o que se mostra é o que se lê.
    if endereco.to_string() != endpoint
        || endereco.port() == 0
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
    {
        return Err(FalhaDoQr::EnderecoInvalido);
    }
    Ok(())
}

fn validar_pin(pin: &str) -> Result<(), FalhaDoQr> {
    if pin.len() == DIGITOS_DO_PIN && pin.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(FalhaDoQr::CodigoInvalido)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync_bootstrap;
    use crate::application::sync_pin_pairing::escutar;
    use crate::application::sync_sessao::atender_conexao;
    use crate::domain::identity::DeviceIdentity;
    use crate::infrastructure::blob_store::BlobStore;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
    use crate::infrastructure::sync_pake::{Codigos, VALIDADE};
    use std::net::TcpListener;
    use std::time::Duration;

    const PIN: &str = "12345678";

    // ── Formato ──────────────────────────────────────────────────────────────────────────────────

    #[test]
    fn ida_e_volta_do_formato() {
        let conteudo = montar("192.168.1.145:45870", PIN).expect("montar");
        assert_eq!(conteudo, "narrahub-pair-pin:1:192.168.1.145:45870:12345678");
        let convite = interpretar(&conteudo).expect("interpretar");
        assert_eq!(convite.endpoint, "192.168.1.145:45870");
        assert_eq!(convite.pin, PIN);
    }

    /// Gate 1 e 9: conteúdo estranho é recusado **aqui**, com o motivo certo.
    #[test]
    fn conteudo_malformado_e_recusado_com_o_motivo_certo() {
        use FalhaDoQr::*;
        let casos: &[(&str, FalhaDoQr)] = &[
            ("", NaoENarrahub),
            ("https://exemplo.com", NaoENarrahub),
            (
                "narrahub-pair:1:ABC:SEGREDO:192.168.1.2:45870",
                NaoENarrahub,
            ),
            ("narrahub-pair-pin", NaoENarrahub),
            (
                "narrahub-pair-pinX:1:192.168.1.2:45870:12345678",
                NaoENarrahub,
            ),
            (
                "narrahub-pair-pin:x:192.168.1.2:45870:12345678",
                NaoENarrahub,
            ),
            (
                "narrahub-pair-pin:2:192.168.1.2:45870:12345678",
                VersaoIncompativel,
            ),
            (
                "narrahub-pair-pin:01:192.168.1.2:45870:12345678",
                VersaoIncompativel,
            ),
            ("narrahub-pair-pin:1:192.168.1.2:45870", EnderecoInvalido),
            (
                "narrahub-pair-pin:1:192.168.1.2:45870:1234567",
                CodigoInvalido,
            ),
            (
                "narrahub-pair-pin:1:192.168.1.2:45870:123456789",
                CodigoInvalido,
            ),
            (
                "narrahub-pair-pin:1:192.168.1.2:45870:1234 678",
                CodigoInvalido,
            ),
            (
                "narrahub-pair-pin:1:192.168.1.2:45870:1234567a",
                CodigoInvalido,
            ),
            (
                // O último campo é tomado como PIN; o que sobra à esquerda deixa de ser endereço.
                "narrahub-pair-pin:1:192.168.1.2:45870:12345678:extra",
                EnderecoInvalido,
            ),
            (
                "narrahub-pair-pin:1:192.168.1.2:45870:12345678 ",
                CodigoInvalido,
            ),
            (
                " narrahub-pair-pin:1:192.168.1.2:45870:12345678",
                NaoENarrahub,
            ),
            (
                "narrahub-pair-pin:1:pc-do-osmar:45870:12345678",
                EnderecoInvalido,
            ),
            ("narrahub-pair-pin:1:192.168.1.2:12345678", EnderecoInvalido),
            (
                "narrahub-pair-pin:1:192.168.1.2:0:12345678",
                EnderecoInvalido,
            ),
            (
                "narrahub-pair-pin:1:192.168.1.2:70000:12345678",
                EnderecoInvalido,
            ),
            (
                "narrahub-pair-pin:1:192.168.001.002:45870:12345678",
                EnderecoInvalido,
            ),
            (
                "narrahub-pair-pin:1:0.0.0.0:45870:12345678",
                EnderecoInvalido,
            ),
            (
                "narrahub-pair-pin:1:255.255.255.255:45870:12345678",
                EnderecoInvalido,
            ),
            (
                "narrahub-pair-pin:1:224.0.0.251:45870:12345678",
                EnderecoInvalido,
            ),
            ("narrahub-pair-pin:1:[::1]:45870:12345678", EnderecoInvalido),
        ];
        for (texto, esperada) in casos {
            assert_eq!(interpretar(texto), Err(*esperada), "conteúdo: {texto:?}");
        }
        let gigante = format!("{PREFIXO}:1:192.168.1.2:45870:{}", "1".repeat(80));
        assert_eq!(interpretar(&gigante), Err(NaoENarrahub));
    }

    #[test]
    fn montar_recusa_o_que_interpretar_recusaria() {
        assert_eq!(montar("pc:45870", PIN), Err(FalhaDoQr::EnderecoInvalido));
        assert_eq!(
            montar("192.168.1.2:45870", "1234 5678"),
            Err(FalhaDoQr::CodigoInvalido)
        );
    }

    /// Gate 10: o PIN não vaza por mensagem de erro nem por `Debug`.
    #[test]
    fn pin_nao_aparece_em_mensagem_nem_em_debug() {
        let segredo = "90817263";
        let conteudo = format!("{PREFIXO}:1:192.168.1.2:45870:{segredo}");
        let convite = interpretar(&conteudo).expect("válido");
        assert!(!format!("{convite:?}").contains(segredo));

        for estragado in [
            format!("{PREFIXO}:9:192.168.1.2:45870:{segredo}"),
            format!("{PREFIXO}:1:192.168.1.2:0:{segredo}"),
            format!("{PREFIXO}:1:192.168.1.2:45870:{segredo}9"),
            format!("outra-coisa:{segredo}"),
        ] {
            let falha = interpretar(&estragado).expect_err("recusado");
            assert!(!falha.to_string().contains(segredo), "{falha}");
            assert!(!format!("{falha:?}").contains(segredo));
        }
    }

    /// Gate 10, lado do código: este módulo não escreve em log.
    #[test]
    fn este_modulo_nao_escreve_em_log() {
        let fonte = include_str!("sync_qr_pin.rs");
        let producao = &fonte[..fonte.find("#[cfg(test)]").expect("módulo de teste")];
        for proibido in ["println!", "eprintln!", "dbg!", "log::", "tracing::"] {
            assert!(!producao.contains(proibido), "{proibido} em sync_qr_pin.rs");
        }
    }

    // ── Pela rede, com o caminho do PIN de verdade ────────────────────────────────────────────────

    struct Aparelho {
        banco: TemporaryDatabase,
        dados: std::path::PathBuf,
        identidade: DeviceIdentity,
        store: BlobStore,
        nome: &'static str,
    }

    impl Aparelho {
        fn novo(nome: &'static str) -> Self {
            let dados = std::env::temp_dir().join(format!("narrahub-qr-{}", uuid::Uuid::new_v4()));
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
                espera: Duration::from_secs(20),
            }
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

    /// O anfitrião escuta com os códigos dados; o visitante pareia pelo conteúdo de QR dado.
    /// `conteudo` recebe o endereço real da escuta e o PIN aberto, e devolve o texto escaneado.
    fn tentar(
        anfitriao: &Aparelho,
        visitante: &Aparelho,
        codigos: &mut Codigos,
        conteudo: impl FnOnce(&str, &str) -> String,
    ) -> (
        DatabaseCommandResult<ResultadoDaSessao>,
        DatabaseCommandResult<ResultadoDaSessao>,
    ) {
        let escuta = escutar(0).expect("escutar");
        let endpoint = format!(
            "127.0.0.1:{}",
            escuta.local_addr().expect("endereço").port()
        );
        let pin = codigos
            .unico_aberto()
            .ok()
            .map(|id| codigos.pin_de(&id))
            .unwrap_or_else(|| PIN.to_string());
        let texto = conteudo(&endpoint, &pin);
        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta.accept().expect("aceitar");
                atender_conexao(&mut fluxo, codigos, &anfitriao.ctx())
            });
            let do_visitante = parear_por_qr(&texto, &visitante.ctx());
            let do_anfitriao = servidor.join().expect("thread do anfitrião");
            (do_anfitriao, do_visitante)
        })
    }

    fn um_codigo() -> Codigos {
        let mut codigos = Codigos::default();
        codigos.emitir();
        codigos
    }

    /// O caminho feliz: nada digitado, e quem entra no roster é quem a sessão provou.
    #[test]
    fn qr_valido_pareia_e_o_roster_recebe_a_identidade_provada() {
        let (anfitriao, visitante) = (Aparelho::novo("PC"), Aparelho::novo("Celular"));
        let mut codigos = um_codigo();
        let (a, v) = tentar(&anfitriao, &visitante, &mut codigos, |endpoint, pin| {
            montar(endpoint, pin).expect("montar")
        });
        let v = v.expect("o visitante pareia pelo QR");
        a.expect("o anfitrião pareia");
        assert_eq!(v.parceiro.device_id, anfitriao.identidade.device_id());
        assert_eq!(anfitriao.roster(), vec![visitante.identidade.device_id()]);
        assert_eq!(visitante.roster(), vec![anfitriao.identidade.device_id()]);
    }

    /// Gate 2: expiração é da escuta. O QR não carrega prazo; o código aberto do anfitrião vence.
    #[test]
    fn qr_vencido_falha_pela_validade_da_escuta() {
        let (anfitriao, visitante) = (Aparelho::novo("PC"), Aparelho::novo("Celular"));
        let mut codigos = um_codigo();
        let id = codigos.unico_aberto().expect("aberto");
        codigos.envelhecer(&id, VALIDADE + Duration::from_secs(1));
        let (a, v) = tentar(&anfitriao, &visitante, &mut codigos, |endpoint, pin| {
            montar(endpoint, pin).expect("montar")
        });
        assert!(a.is_err());
        assert!(
            v.expect_err("QR vencido não pareia")
                .message
                .contains("não aceitou o código"),
            "a mensagem precisa dizer que o código não valeu"
        );
        assert!(anfitriao.roster().is_empty());
        assert!(visitante.roster().is_empty());
    }

    /// Uso único: o mesmo QR, depois de um pareamento que deu certo, não vale de novo.
    #[test]
    fn qr_ja_usado_nao_pareia_de_novo() {
        let (anfitriao, visitante) = (Aparelho::novo("PC"), Aparelho::novo("Celular"));
        let intruso = Aparelho::novo("Outro");
        let mut codigos = um_codigo();
        let mut guardado = String::new();
        let (_, v) = tentar(&anfitriao, &visitante, &mut codigos, |endpoint, pin| {
            guardado = montar(endpoint, pin).expect("montar");
            guardado.clone()
        });
        v.expect("primeiro uso pareia");
        let pin_usado = guardado.rsplit_once(':').expect("pin").1.to_string();
        let (a, v2) = tentar(&anfitriao, &intruso, &mut codigos, |endpoint, _| {
            montar(endpoint, &pin_usado).expect("montar")
        });
        assert!(a.is_err());
        assert!(v2
            .expect_err("segundo uso recusado")
            .message
            .contains("não aceitou o código"));
        assert_eq!(anfitriao.roster(), vec![visitante.identidade.device_id()]);
        assert!(intruso.roster().is_empty());
    }

    /// Gate 3: QR alterado (outro PIN) falha como PIN errado, e ninguém entra no roster.
    #[test]
    fn qr_com_pin_alterado_falha_e_ninguem_entra() {
        let (anfitriao, visitante) = (Aparelho::novo("PC"), Aparelho::novo("Celular"));
        let mut codigos = um_codigo();
        let (a, v) = tentar(&anfitriao, &visitante, &mut codigos, |endpoint, pin| {
            let alterado: String = pin
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    if i == 0 {
                        if c == '9' {
                            '0'
                        } else {
                            '9'
                        }
                    } else {
                        c
                    }
                })
                .collect();
            montar(endpoint, &alterado).expect("montar")
        });
        assert!(a.is_err());
        assert!(v.expect_err("PIN alterado").message.contains("não confere"));
        assert!(anfitriao.roster().is_empty());
        assert!(visitante.roster().is_empty());
    }

    /// Gate 9: conteúdo malformado é recusado antes de qualquer conexão.
    #[test]
    fn qr_malformado_nao_chega_a_rede() {
        let visitante = Aparelho::novo("Celular");
        let escuta = TcpListener::bind("127.0.0.1:0").expect("escuta");
        escuta.set_nonblocking(true).expect("não bloqueante");
        let porta = escuta.local_addr().expect("endereço").port();
        for texto in [
            format!("{PREFIXO}:2:127.0.0.1:{porta}:{PIN}"),
            format!("{PREFIXO}:1:127.0.0.1:{porta}:1234"),
            format!("{PREFIXO}:1:127.0.0.1:{porta}:{PIN}:extra"),
            format!("narrahub-pair:1:ID:SEGREDO:127.0.0.1:{porta}"),
        ] {
            assert!(parear_por_qr(&texto, &visitante.ctx()).is_err(), "{texto}");
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            matches!(escuta.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "um QR recusado abriu conexão"
        );
        assert!(visitante.roster().is_empty());
    }

    /// Gate 4: ler o QR não cria confiança. Interpretar é função pura, e um QR válido apontando para
    /// um anfitrião sem código aberto não põe ninguém no roster de ninguém.
    #[test]
    fn descoberta_nao_e_confianca() {
        let (anfitriao, visitante) = (Aparelho::novo("PC"), Aparelho::novo("Celular"));
        let conteudo = montar("127.0.0.1:45870", PIN).expect("montar");
        interpretar(&conteudo).expect("interpretar");
        assert!(visitante.roster().is_empty(), "interpretar tocou no banco");
        let mut sem_codigo = Codigos::default();
        let (_, v) = tentar(&anfitriao, &visitante, &mut sem_codigo, |endpoint, _| {
            montar(endpoint, PIN).expect("montar")
        });
        assert!(v.is_err());
        assert!(anfitriao.roster().is_empty());
        assert!(visitante.roster().is_empty());
    }
}
