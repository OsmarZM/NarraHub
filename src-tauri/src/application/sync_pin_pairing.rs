//! Pareamento por PIN sobre a rede (etapa 14, fatia 3).
//!
//! ## O que cada etapa já tinha entregado, e o que faltava
//!
//! ```text
//! etapa  8   Noise XX + prova Ed25519 sobre o hash   sobre BUFFERS
//! etapa  9   convite de QR, uso unico, expiravel     sem rede
//! etapa 10   PIN por SPAKE2 -> segredo de 32 bytes   sem rede
//! etapa 14/1 enquadramento sobre TCP                 ← o fio
//! etapa 14/3 os tres juntos, sobre o fio             ← isto
//! ```
//!
//! Nenhuma das três precisou mudar. Esta fatia as costura.
//!
//! ## A ordem, e por que cada passo está onde está
//!
//! ```text
//! VISITANTE (digitou o PIN)                 ANFITRIAO (mostra o PIN)
//!
//!   conecta  ──────────────────────────────▶  aceita
//!   msg SPAKE2 do visitante  ──────────────▶  conta a tentativa, entao
//!                                             comeca a sua troca
//!                            ◀──────────────  msg SPAKE2 do anfitriao
//!   (os dois derivam o mesmo psk de 32 bytes, ou nenhum deriva nada)
//!
//!   Noise XXpsk0, prologo = as duas mensagens do SPAKE2
//!   e          ──────────────────────────────▶
//!                            ◀──────────────  e, ee, s, es
//!   s, se      ──────────────────────────────▶
//!
//!   (canal cifrado)
//!   minha identidade + nome  ──────────────▶  verifica, admite
//!                            ◀──────────────  identidade + nome dele
//!   verifica, admite
//! ```
//!
//! **A tentativa é contada quando a mensagem do visitante chega**, antes de
//! qualquer cripto — é o que a etapa 10 decidiu, e o motivo é que contar só no
//! fim permitiria abandonar a conexão antes do resultado e tentar de novo à
//! vontade. O limite de três seria decorativo, e ele é a única barreira que o
//! PAKE deixa de pé para um PIN de 26 bits.
//!
//! **O prólogo do Noise são as duas mensagens do SPAKE2.** Os dois lados as
//! têm, e nenhum terceiro as tem juntas antes de falar com os dois. Amarra
//! aquele handshake àquela troca de PIN: um `psk` capturado de outra tentativa
//! não fecha.
//!
//! **A identidade vai depois do handshake, pelo canal já cifrado.** Antes dele
//! não existe canal; e o que se assina é o hash daquele handshake, que só
//! existe no fim.
//!
//! ## O que este módulo não faz
//!
//! Não descobre ninguém na rede. Endereço e porta são digitados — decisão
//! registrada, para provar o transporte antes de envolver mDNS, multicast do
//! Android e diferença entre roteadores. E não sincroniza nada: pareamento é
//! entrar no conjunto, não trocar eventos. A troca é a fatia 4.
//!
//! Não toca no Sync V1.

use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::DeviceIdentity;
use crate::infrastructure::sqlite::{sync_trust, SqliteDatabase};
use crate::infrastructure::sync_pake::{normalizar, Codigos, TrocaPendente};
use crate::infrastructure::sync_transport::{
    autenticar, provar_identidade, Handshake, ProvaDeIdentidade, SessaoAutenticada,
};
use crate::infrastructure::sync_wire::{
    ajustar_esperas, escrever_mensagem, escrever_quadro, ler_mensagem, ler_quadro, ESPERA_PADRAO,
};
use serde::{Deserialize, Serialize};

/// O padrão do Noise para pareamento, igual ao da etapa 9.
const PADRAO: &str = "Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s";

/// Rótulo do prólogo. Separa este pareamento de qualquer outro uso futuro do
/// mesmo `psk`.
const ROTULO_DO_PROLOGO: &[u8] = b"narrahub.sync.v2.pin";

/// Quem entrou no conjunto, para a tela dizer o nome e não o identificador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parceiro {
    pub device_id: String,
    pub nome: String,
}

/// O que cada lado manda pelo canal cifrado.
///
/// O nome é do aparelho, não da pessoa, e é o que aparece na lista. A chave e
/// a assinatura são a prova — o `device_id` **não** viaja: ele é derivado da
/// chave por quem recebe, em [`autenticar`]. Um `device_id` no fio seria um
/// campo que o outro lado escolhe.
#[derive(Debug, Serialize, Deserialize)]
struct Apresentacao {
    ed25519_public: String,
    assinatura: String,
    nome: String,
}

fn falha(motivo: impl std::fmt::Display) -> DatabaseCommandError {
    DatabaseCommandError::validation(motivo.to_string())
}

/// O prólogo: rótulo mais as duas mensagens do SPAKE2, sempre na mesma ordem.
fn prologo(do_visitante: &[u8], do_anfitriao: &[u8]) -> Vec<u8> {
    let mut bytes =
        Vec::with_capacity(ROTULO_DO_PROLOGO.len() + do_visitante.len() + do_anfitriao.len());
    bytes.extend_from_slice(ROTULO_DO_PROLOGO);
    bytes.extend_from_slice(do_visitante);
    bytes.extend_from_slice(do_anfitriao);
    bytes
}

/// Gera uma estática X25519 nova para esta conexão.
///
/// Efêmera de propósito, e isso **não** enfraquece nada: o que autoriza é a
/// Ed25519 assinando o hash do handshake, não a estática. Persistir a estática
/// exigiria um segundo segredo em disco com o mesmo cuidado do primeiro, para
/// ganhar nada — o roster autoriza por Ed25519.
fn estatica_da_conexao() -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut bytes);
    bytes
}

/// Lado de quem **mostra** o PIN: atende uma conexão e pareia.
///
/// Recebe o `TcpStream` já aceito, e não o `TcpListener`: quem decide aceitar,
/// em que thread, e quantas vezes é a camada de cima. Assim este corpo é
/// testável com um socket de teste e não precisa de um servidor inteiro.
pub fn anfitriao_atende(
    fluxo: &mut TcpStream,
    codigos: &mut Codigos,
    identidade: &DeviceIdentity,
    nome_local: &str,
    database: &SqliteDatabase,
    espera: Duration,
) -> DatabaseCommandResult<Parceiro> {
    ajustar_esperas(fluxo, espera).map_err(falha)?;

    // A tentativa é contada aqui, na chegada. Ver o cabeçalho.
    let id_do_codigo = codigos.unico_aberto().map_err(falha)?;
    let do_visitante = ler_quadro(fluxo).map_err(falha)?;
    let troca = codigos.iniciar_tentativa(&id_do_codigo).map_err(falha)?;

    let do_anfitriao = troca.mensagem.clone();
    escrever_quadro(fluxo, &do_anfitriao).map_err(falha)?;

    let psk = troca.concluir(&do_visitante).map_err(falha)?.psk_do_noise();

    let mut aperto = Handshake::com_psk(
        PADRAO,
        &estatica_da_conexao(),
        &psk,
        &prologo(&do_visitante, &do_anfitriao),
        false,
    )?;

    let m1 = ler_quadro(fluxo).map_err(falha)?;
    aperto.ler(&m1)?;
    let m2 = aperto.escrever(&[])?;
    escrever_quadro(fluxo, &m2).map_err(falha)?;
    let m3 = ler_quadro(fluxo).map_err(falha)?;
    aperto.ler(&m3)?;

    let parceiro = trocar_identidades(fluxo, aperto, identidade, nome_local, database, false)?;

    // O código só é consumido depois de o pareamento dar certo. Consumir antes
    // queimaria o PIN por ruído de rede, e o escritor teria que gerar outro.
    codigos.consumir(&id_do_codigo);
    Ok(parceiro)
}

/// Lado de quem **digitou** o PIN: conecta e pareia.
pub fn visitante_pareia(
    fluxo: &mut TcpStream,
    pin_digitado: &str,
    identidade: &DeviceIdentity,
    nome_local: &str,
    database: &SqliteDatabase,
    espera: Duration,
) -> DatabaseCommandResult<Parceiro> {
    ajustar_esperas(fluxo, espera).map_err(falha)?;
    let pin = normalizar(pin_digitado).map_err(falha)?;

    let troca = TrocaPendente::visitante(&pin);
    let do_visitante = troca.mensagem.clone();
    escrever_quadro(fluxo, &do_visitante).map_err(falha)?;
    let do_anfitriao = ler_quadro(fluxo).map_err(falha)?;

    let psk = troca.concluir(&do_anfitriao).map_err(falha)?.psk_do_noise();

    let mut aperto = Handshake::com_psk(
        PADRAO,
        &estatica_da_conexao(),
        &psk,
        &prologo(&do_visitante, &do_anfitriao),
        true,
    )?;

    let m1 = aperto.escrever(&[])?;
    escrever_quadro(fluxo, &m1).map_err(falha)?;
    let m2 = ler_quadro(fluxo).map_err(falha)?;
    aperto.ler(&m2)?;
    let m3 = aperto.escrever(&[])?;
    escrever_quadro(fluxo, &m3).map_err(falha)?;

    trocar_identidades(fluxo, aperto, identidade, nome_local, database, true)
}

/// A metade final, idêntica dos dois lados menos a ordem de fala.
///
/// Quem fala primeiro é decidido por parâmetro e não por papel: com os dois
/// escrevendo antes de ler, duas mensagens grandes poderiam encher a janela do
/// TCP nos dois sentidos ao mesmo tempo. O `visitante` escreve primeiro.
fn trocar_identidades(
    fluxo: &mut TcpStream,
    aperto: Handshake,
    identidade: &DeviceIdentity,
    nome_local: &str,
    database: &SqliteDatabase,
    escrevo_primeiro: bool,
) -> DatabaseCommandResult<Parceiro> {
    let hash = aperto.hash_do_handshake();
    let minha = provar_identidade(identidade, &hash);
    let mut transporte = aperto.concluir()?;

    let apresentacao = Apresentacao {
        ed25519_public: minha.ed25519_public,
        assinatura: minha.assinatura,
        nome: nome_local.to_string(),
    };
    let minha_carga = serde_json::to_vec(&apresentacao)
        .map_err(|erro| DatabaseCommandError::storage(erro.to_string()))?;

    let dele: Apresentacao = if escrevo_primeiro {
        escrever_mensagem(&mut transporte, fluxo, &minha_carga).map_err(falha)?;
        ler_apresentacao(&mut transporte, fluxo)?
    } else {
        let dele = ler_apresentacao(&mut transporte, fluxo)?;
        escrever_mensagem(&mut transporte, fluxo, &minha_carga).map_err(falha)?;
        dele
    };

    let sessao: SessaoAutenticada = autenticar(
        &ProvaDeIdentidade {
            ed25519_public: dele.ed25519_public,
            assinatura: dele.assinatura,
        },
        &hash,
    )
    .map_err(falha)?;

    let nome = nome_utilizavel(&dele.nome);
    let connection = database.write()?;
    sync_trust::admitir_por_pareamento(&connection, &sessao, &nome)?;

    Ok(Parceiro {
        device_id: sessao.device_id().to_string(),
        nome,
    })
}

fn ler_apresentacao(
    transporte: &mut crate::infrastructure::sync_transport::Transporte,
    fluxo: &mut TcpStream,
) -> DatabaseCommandResult<Apresentacao> {
    let bytes = ler_mensagem(transporte, fluxo).map_err(falha)?;
    serde_json::from_slice(&bytes)
        .map_err(|_| falha("O outro aparelho respondeu num formato que este não reconhece."))
}

/// O nome que vai para a lista, contido.
///
/// Vem do outro aparelho, então é texto de fora: sem controle, sem quebra de
/// linha, e curto. Um nome de 2 MB não é ataque interessante, mas é uma linha
/// de roster que ninguém consegue ler.
fn nome_utilizavel(bruto: &str) -> String {
    let limpo: String = bruto
        .chars()
        .filter(|c| !c.is_control())
        .take(60)
        .collect::<String>()
        .trim()
        .to_string();
    if limpo.is_empty() {
        "Aparelho sem nome".to_string()
    } else {
        limpo
    }
}

/// Abre a escuta de pareamento e devolve o endereço para a tela mostrar.
///
/// O endereço é o que o humano digita no outro aparelho. Sem descoberta
/// automática nesta fatia — decisão registrada.
pub fn escutar(porta: u16) -> DatabaseCommandResult<TcpListener> {
    crate::infrastructure::sync_wire::ouvir(("0.0.0.0", porta)).map_err(falha)
}

/// Conecta no endereço digitado.
pub fn conectar(endereco: &str) -> DatabaseCommandResult<TcpStream> {
    crate::infrastructure::sync_wire::conectar(endereco, ESPERA_PADRAO).map_err(falha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync_bootstrap;
    use crate::infrastructure::sqlite::test_support::TemporaryDatabase;

    struct Aparelho {
        banco: TemporaryDatabase,
        dados: std::path::PathBuf,
        identidade: DeviceIdentity,
    }

    impl Aparelho {
        fn novo() -> Self {
            let dados = std::env::temp_dir().join(format!("narrahub-pin-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dados).expect("criar diretório");
            let banco = TemporaryDatabase::new();
            let identidade = sync_bootstrap::prepare(&dados, &banco.database).expect("arranque");
            Self {
                banco,
                dados,
                identidade,
            }
        }

        fn roster(&self) -> Vec<(String, String, String)> {
            let connection = self.banco.connection();
            let mut consulta = connection
                .prepare(
                    "SELECT device_id, name, introduced_by FROM sync_devices
                      WHERE is_self = 0 ORDER BY device_id",
                )
                .expect("consulta");
            let linhas = consulta
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .expect("linhas")
                .collect::<Result<Vec<_>, _>>()
                .expect("coletar");
            linhas
        }
    }

    impl Drop for Aparelho {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dados);
        }
    }

    /// O endereço para o visitante conectar, a partir de quem escuta.
    ///
    /// `escutar` liga em `0.0.0.0` — que é o certo em produção: o outro
    /// aparelho vem pela rede local, não pelo loopback. Mas `local_addr()`
    /// então devolve `0.0.0.0:porta`, e **`0.0.0.0` é endereço de escuta, não
    /// de destino**. Conectar nele foi o que travou este teste na primeira
    /// execução.
    fn destino(escuta: &TcpListener) -> String {
        let porta = escuta.local_addr().expect("endereço").port();
        format!("127.0.0.1:{porta}")
    }

    /// Roda um pareamento inteiro entre dois aparelhos, sobre TCP de verdade.
    ///
    /// `pin_digitado` é parâmetro para o mesmo arranjo servir ao caminho certo
    /// e ao PIN errado — é a diferença entre provar que funciona e provar que
    /// **só** funciona com o código certo.
    fn parear(
        anfitriao: &Aparelho,
        visitante: &Aparelho,
        errar_o_pin: bool,
    ) -> (
        DatabaseCommandResult<Parceiro>,
        DatabaseCommandResult<Parceiro>,
    ) {
        let escuta = escutar(0).expect("escutar");
        let endereco = destino(&escuta);

        let mut codigos = Codigos::default();
        let (_, legivel) = codigos.emitir();
        let pin_certo: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();
        let pin_digitado = if errar_o_pin {
            // Um dígito diferente, mantendo o formato.
            let mut errado: Vec<char> = pin_certo.chars().collect();
            errado[0] = if errado[0] == '0' { '1' } else { '0' };
            errado.into_iter().collect()
        } else {
            pin_certo
        };

        // `thread::scope` e nao `spawn`: a `DeviceIdentity` NAO e `Clone`, de
        // proposito -- ela guarda a chave privada, e um `clone` dela seria uma
        // segunda copia de material criptografico viva em outro lugar. Com
        // escopo, a thread empresta a identidade e nada e duplicado.
        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta.accept().expect("aceitar");
                anfitriao_atende(
                    &mut fluxo,
                    &mut codigos,
                    &anfitriao.identidade,
                    "Desktop da Ana",
                    &anfitriao.banco.database,
                    Duration::from_secs(20),
                )
            });

            let mut fluxo = conectar(&endereco).expect("conectar");
            let do_visitante = visitante_pareia(
                &mut fluxo,
                &pin_digitado,
                &visitante.identidade,
                "Celular da Ana",
                &visitante.banco.database,
                Duration::from_secs(20),
            );
            let do_anfitriao = servidor.join().expect("thread do anfitrião");
            (do_anfitriao, do_visitante)
        })
    }

    /// **O pareamento por PIN funciona sobre a rede, e os dois lados se
    /// admitem.**
    ///
    /// É o gate central da fatia. O que ele prova não é "conectou": é que cada
    /// lado terminou com o `device_id` do **outro** no roster, derivado da
    /// chave que assinou o hash daquele handshake, e com `introduced_by`
    /// vazio — que é como o schema registra pareamento direto.
    #[test]
    fn o_pareamento_por_pin_atravessa_a_rede_e_os_dois_se_admitem() {
        let anfitriao = Aparelho::novo();
        let visitante = Aparelho::novo();

        let (do_anfitriao, do_visitante) = parear(&anfitriao, &visitante, false);
        let visto_pelo_anfitriao = do_anfitriao.expect("o anfitrião tinha que parear");
        let visto_pelo_visitante = do_visitante.expect("o visitante tinha que parear");

        assert_eq!(
            visto_pelo_anfitriao.device_id,
            visitante.identidade.device_id(),
            "o anfitrião admitiu um aparelho que não é o visitante"
        );
        assert_eq!(
            visto_pelo_visitante.device_id,
            anfitriao.identidade.device_id(),
            "o visitante admitiu um aparelho que não é o anfitrião"
        );
        assert_eq!(visto_pelo_anfitriao.nome, "Celular da Ana");
        assert_eq!(visto_pelo_visitante.nome, "Desktop da Ana");

        let no_anfitriao = anfitriao.roster();
        assert_eq!(
            no_anfitriao.len(),
            1,
            "o roster do anfitrião: {no_anfitriao:?}"
        );
        assert_eq!(no_anfitriao[0].0, visitante.identidade.device_id());
        assert_eq!(no_anfitriao[0].1, "Celular da Ana");
        assert_eq!(
            no_anfitriao[0].2, "",
            "pareamento direto não tem padrinho (ADR 0009 §5)"
        );

        let no_visitante = visitante.roster();
        assert_eq!(no_visitante.len(), 1);
        assert_eq!(no_visitante[0].0, anfitriao.identidade.device_id());
    }

    /// **PIN errado não pareia, e ninguém entra em roster nenhum.**
    ///
    /// O que reprova é o SPAKE2: sem o mesmo PIN não existe segredo comum, e
    /// sem ele o `XXpsk0` não fecha. O gate confere o roster dos **dois**
    /// lados — um pareamento que falhasse pela metade deixaria um aparelho
    /// aceitando eventos de quem não conseguiu se autenticar.
    #[test]
    fn pin_errado_nao_pareia_e_ninguem_entra_no_roster() {
        let anfitriao = Aparelho::novo();
        let visitante = Aparelho::novo();

        let (do_anfitriao, do_visitante) = parear(&anfitriao, &visitante, true);
        assert!(do_anfitriao.is_err(), "o anfitrião não podia ter pareado");
        assert!(do_visitante.is_err(), "o visitante não podia ter pareado");

        assert!(
            anfitriao.roster().is_empty(),
            "o roster do anfitrião ficou com {:?}",
            anfitriao.roster()
        );
        assert!(
            visitante.roster().is_empty(),
            "o roster do visitante ficou com {:?}",
            visitante.roster()
        );
    }

    /// **Três tentativas, e o código morre.**
    ///
    /// O limite é a única barreira que sobra para um PIN de 26 bits, e a etapa
    /// 10 decidiu contar na abertura. Este gate prova isso pela rede: três
    /// visitantes com PIN errado esgotam o código, e o quarto — **mesmo com o
    /// PIN certo** — não passa.
    ///
    /// Contar só no fim deixaria um atacante abandonar a conexão antes do
    /// resultado e tentar à vontade.
    #[test]
    fn tres_tentativas_erradas_matam_o_codigo_mesmo_para_quem_acerta() {
        let anfitriao = Aparelho::novo();
        let escuta = escutar(0).expect("escutar");
        let endereco = destino(&escuta);

        let mut codigos = Codigos::default();
        let (_, legivel) = codigos.emitir();
        let pin_certo: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();
        let pin_errado = pin_certo
            .chars()
            .map(|c| if c == '7' { '8' } else { '7' })
            .collect::<String>();

        let certo_para_o_fim = pin_certo.clone();

        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let mut resultados = Vec::new();
                for _ in 0..4 {
                    let (mut fluxo, _) = escuta.accept().expect("aceitar");
                    resultados.push(anfitriao_atende(
                        &mut fluxo,
                        &mut codigos,
                        &anfitriao.identidade,
                        "Desktop",
                        &anfitriao.banco.database,
                        Duration::from_secs(20),
                    ));
                }
                resultados
            });

            for tentativa in 0..3 {
                let visitante = Aparelho::novo();
                let mut fluxo = conectar(&endereco).expect("conectar");
                let resultado = visitante_pareia(
                    &mut fluxo,
                    &pin_errado,
                    &visitante.identidade,
                    "Intruso",
                    &visitante.banco.database,
                    Duration::from_secs(20),
                );
                assert!(resultado.is_err(), "tentativa {tentativa} não podia passar");
            }

            // A quarta, com o PIN CERTO.
            let legitimo = Aparelho::novo();
            let mut fluxo = conectar(&endereco).expect("conectar");
            let resultado = visitante_pareia(
                &mut fluxo,
                &certo_para_o_fim,
                &legitimo.identidade,
                "Celular",
                &legitimo.banco.database,
                Duration::from_secs(20),
            );
            assert!(
                resultado.is_err(),
                "o código tinha que estar esgotado; o PIN certo não ressuscita"
            );

            let resultados = servidor.join().expect("thread do anfitrião");
            assert_eq!(resultados.len(), 4);
            for (indice, resultado) in resultados.iter().enumerate() {
                assert!(
                    resultado.is_err(),
                    "a tentativa {indice} passou no anfitrião"
                );
            }
            assert!(
                anfitriao.roster().is_empty(),
                "ninguém podia ter entrado: {:?}",
                anfitriao.roster()
            );
        });
    }

    /// **Conexão interrompida no meio não queima o código.**
    ///
    /// O visitante manda a mensagem do SPAKE2 e some — queda de Wi-Fi, app
    /// fechado, cabo. A tentativa é contada, e tem que ser; o que **não** pode
    /// acontecer é o código deixar de existir, porque aí o escritor teria que
    /// gerar outro PIN por causa de ruído de rede.
    ///
    /// Este gate nasceu de uma mutação sobrevivente: mover `consumir` para
    /// antes do pareamento não reprovava em nada, e eu tinha escrito no código
    /// que a ordem importava.
    #[test]
    fn conexao_interrompida_no_meio_nao_queima_o_codigo() {
        let anfitriao = Aparelho::novo();
        let visitante = Aparelho::novo();
        let escuta = escutar(0).expect("escutar");
        let endereco = destino(&escuta);

        let mut codigos = Codigos::default();
        let (_, legivel) = codigos.emitir();
        let pin: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();
        let pin_para_depois = pin.clone();

        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                // Primeiro a conexão que morre no meio, depois a legítima.
                let (mut abandonada, _) = escuta.accept().expect("aceitar a primeira");
                let primeira = anfitriao_atende(
                    &mut abandonada,
                    &mut codigos,
                    &anfitriao.identidade,
                    "Desktop",
                    &anfitriao.banco.database,
                    Duration::from_millis(400),
                );

                let (mut boa, _) = escuta.accept().expect("aceitar a segunda");
                let segunda = anfitriao_atende(
                    &mut boa,
                    &mut codigos,
                    &anfitriao.identidade,
                    "Desktop",
                    &anfitriao.banco.database,
                    Duration::from_secs(20),
                );
                (primeira, segunda)
            });

            // A conexão que abre, fala uma vez e desaparece.
            {
                let mut fluxo = conectar(&endereco).expect("conectar");
                let troca = TrocaPendente::visitante(&pin);
                escrever_quadro(&mut fluxo, &troca.mensagem).expect("mandar o spake");
                // `fluxo` morre aqui, no fim do bloco.
            }

            let mut fluxo = conectar(&endereco).expect("conectar de novo");
            let boa = visitante_pareia(
                &mut fluxo,
                &pin_para_depois,
                &visitante.identidade,
                "Celular",
                &visitante.banco.database,
                Duration::from_secs(20),
            );

            let (primeira, segunda) = servidor.join().expect("thread do anfitrião");
            assert!(primeira.is_err(), "a conexão abandonada não podia parear");
            assert!(
                segunda.is_ok(),
                "o mesmo código tinha que continuar valendo: {segunda:?}"
            );
            assert!(boa.is_ok(), "o visitante legítimo falhou: {boa:?}");
        });

        assert_eq!(
            anfitriao.roster().len(),
            1,
            "só o aparelho legítimo entrou: {:?}",
            anfitriao.roster()
        );
    }

    /// **Quem some DEPOIS do handshake também não queima o código.**
    ///
    /// Este gate existe porque o anterior não alcançava a janela certa, e a
    /// mutação sobreviveu duas vezes: mover `consumir` para antes do
    /// pareamento só muda alguma coisa quando o Noise **fecha** e a troca de
    /// identidade não acontece. É estreita, e é real — a conexão cai entre o
    /// último quadro do handshake e a apresentação.
    ///
    /// Aqui o visitante é dirigido à mão até o fim do `XXpsk0` e então some.
    /// O anfitrião falha, e o código tem que continuar valendo para o
    /// escritor, que não tem como saber que a rede piscou.
    #[test]
    fn visitante_que_some_depois_do_handshake_nao_queima_o_codigo() {
        let anfitriao = Aparelho::novo();
        let visitante = Aparelho::novo();
        let escuta = escutar(0).expect("escutar");
        let endereco = destino(&escuta);

        let mut codigos = Codigos::default();
        let (_, legivel) = codigos.emitir();
        let pin: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();
        let pin_depois = pin.clone();

        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut morta, _) = escuta.accept().expect("aceitar a primeira");
                let primeira = anfitriao_atende(
                    &mut morta,
                    &mut codigos,
                    &anfitriao.identidade,
                    "Desktop",
                    &anfitriao.banco.database,
                    Duration::from_millis(600),
                );

                let (mut boa, _) = escuta.accept().expect("aceitar a segunda");
                let segunda = anfitriao_atende(
                    &mut boa,
                    &mut codigos,
                    &anfitriao.identidade,
                    "Desktop",
                    &anfitriao.banco.database,
                    Duration::from_secs(20),
                );
                (primeira, segunda)
            });

            // O visitante, passo a passo, até o handshake fechar — e então some.
            {
                let mut fluxo = conectar(&endereco).expect("conectar");
                let troca = TrocaPendente::visitante(&pin);
                let meu = troca.mensagem.clone();
                escrever_quadro(&mut fluxo, &meu).expect("spake do visitante");
                let dele = ler_quadro(&mut fluxo).expect("spake do anfitrião");
                let psk = troca.concluir(&dele).expect("mesmo pin").psk_do_noise();

                let mut aperto = Handshake::com_psk(
                    PADRAO,
                    &estatica_da_conexao(),
                    &psk,
                    &prologo(&meu, &dele),
                    true,
                )
                .expect("handshake");

                let m1 = aperto.escrever(&[]).expect("m1");
                escrever_quadro(&mut fluxo, &m1).expect("m1");
                let m2 = ler_quadro(&mut fluxo).expect("m2");
                aperto.ler(&m2).expect("m2");
                let m3 = aperto.escrever(&[]).expect("m3");
                escrever_quadro(&mut fluxo, &m3).expect("m3");

                assert!(aperto.terminou(), "o handshake tinha que ter fechado");
                // E some, sem apresentar identidade nenhuma.
            }

            let mut fluxo = conectar(&endereco).expect("conectar de novo");
            let boa = visitante_pareia(
                &mut fluxo,
                &pin_depois,
                &visitante.identidade,
                "Celular",
                &visitante.banco.database,
                Duration::from_secs(20),
            );

            let (primeira, segunda) = servidor.join().expect("thread do anfitrião");
            assert!(primeira.is_err(), "sem apresentação não há pareamento");
            assert!(
                segunda.is_ok(),
                "o código foi queimado por uma conexão que caiu: {segunda:?}"
            );
            assert!(boa.is_ok(), "o visitante legítimo falhou: {boa:?}");
        });
    }

    /// **O nome é contido no caminho de verdade, não só na função.**
    ///
    /// Outra mutação sobrevivente: tirar a chamada de `nome_utilizavel` do
    /// fluxo não reprovava nada, porque o gate testava a função isolada. Uma
    /// função correta que ninguém chama é o defeito que esta etapa inteira
    /// começou encontrando.
    #[test]
    fn o_nome_que_chega_pela_rede_e_contido_antes_do_roster() {
        let anfitriao = Aparelho::novo();
        let visitante = Aparelho::novo();
        let escuta = escutar(0).expect("escutar");
        let endereco = destino(&escuta);

        let mut codigos = Codigos::default();
        let (_, legivel) = codigos.emitir();
        let pin: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();

        let hostil = format!("  Celular\nquebrado\t{}  ", "x".repeat(200));

        std::thread::scope(|escopo| {
            let servidor = escopo.spawn(|| {
                let (mut fluxo, _) = escuta.accept().expect("aceitar");
                anfitriao_atende(
                    &mut fluxo,
                    &mut codigos,
                    &anfitriao.identidade,
                    "Desktop",
                    &anfitriao.banco.database,
                    Duration::from_secs(20),
                )
            });

            let mut fluxo = conectar(&endereco).expect("conectar");
            visitante_pareia(
                &mut fluxo,
                &pin,
                &visitante.identidade,
                &hostil,
                &visitante.banco.database,
                Duration::from_secs(20),
            )
            .expect("pareamento");
            servidor.join().expect("thread").expect("pareamento");
        });

        let roster = anfitriao.roster();
        assert_eq!(roster.len(), 1);
        let nome = &roster[0].1;
        assert!(
            !nome.contains('\n') && !nome.contains('\t'),
            "controle entrou no roster: {nome:?}"
        );
        assert!(
            nome.chars().count() <= 60,
            "nome de {} caracteres entrou inteiro",
            nome.chars().count()
        );
        assert!(
            nome.starts_with("Celular"),
            "o nome foi contido mas ficou irreconhecível: {nome:?}"
        );
    }

    /// **O `device_id` não viaja no fio.**
    ///
    /// Gate estrutural, e é o formato certo para esta propriedade: ela é sobre
    /// o que a mensagem **não tem**. Quem recebe deriva o id da chave, em
    /// `autenticar`; um campo `device_id` na apresentação seria um valor que o
    /// outro lado escolhe, e o roster passaria a gravar o que lhe disseram.
    ///
    /// A mutação que eu tentei primeiro — fazer o id vir do fio — nem
    /// compilou, o que não prova nada. Este gate reprova no momento em que o
    /// campo aparecer.
    #[test]
    fn a_apresentacao_no_fio_nao_carrega_device_id() {
        let fonte = include_str!("sync_pin_pairing.rs");
        let inicio = fonte
            .find("struct Apresentacao {")
            .expect("a varredura perdeu o struct");
        let corpo = &fonte[inicio..inicio + fonte[inicio..].find('}').expect("fim do struct")];

        assert!(
            corpo.contains("ed25519_public") && corpo.contains("assinatura"),
            "a varredura pegou o struct errado: {corpo}"
        );
        for proibido in ["device_id", "deviceId", "fingerprint"] {
            assert!(
                !corpo.contains(proibido),
                "`{proibido}` entrou na mensagem do fio. O identificador é DERIVADO da chave \
                 por quem recebe; recebê-lo pronto é aceitar o que o outro lado afirma."
            );
        }
    }

    /// Nome vindo de fora é contido antes de ir para o roster.
    #[test]
    fn nome_do_outro_aparelho_e_contido() {
        assert_eq!(nome_utilizavel("  Celular da Ana  "), "Celular da Ana");
        assert_eq!(nome_utilizavel(""), "Aparelho sem nome");
        assert_eq!(nome_utilizavel("   "), "Aparelho sem nome");
        assert_eq!(
            nome_utilizavel("linha\nquebrada\ttabulada"),
            "linhaquebradatabulada",
            "controle não entra na lista"
        );
        assert_eq!(
            nome_utilizavel(&"a".repeat(500)).chars().count(),
            60,
            "nome longo é cortado"
        );
    }

    /// O prólogo depende das duas mensagens, e da ordem delas.
    ///
    /// Se ele ignorasse uma das pontas, um `psk` capturado de outra tentativa
    /// de PIN poderia ser reusado noutro handshake.
    #[test]
    fn o_prologo_amarra_as_duas_mensagens_do_spake2() {
        let a = prologo(b"visitante", b"anfitriao");
        assert_ne!(a, prologo(b"anfitriao", b"visitante"), "ordem importa");
        assert_ne!(a, prologo(b"visitante", b"outro"), "as duas entram");
        assert_ne!(a, prologo(b"outro", b"anfitriao"), "as duas entram");
        assert!(
            a.starts_with(ROTULO_DO_PROLOGO),
            "o rótulo separa este uso de qualquer outro"
        );
    }
}
