//! Sync V2 — o fio: enquadramento sobre TCP (etapa 14, fatia 1).
//!
//! ## O que faltava
//!
//! A etapa 8 entregou [`Transporte`], que cifra e decifra — e fala com
//! **buffers em memória**. Nenhum módulo do Sync V2 abria um socket: os "três
//! aparelhos" da etapa 6 são três bancos no mesmo processo. Este módulo é o
//! primeiro em que o V2 toca a rede.
//!
//! ## Por que enquadrar, e não só escrever no socket
//!
//! TCP é um fluxo de bytes. Ele não preserva a fronteira das escritas: dois
//! `write` de 10 bytes podem chegar como um `read` de 20, ou como cinco de 4.
//! O Noise, do outro lado, é orientado a **mensagem** — `decifrar` precisa
//! exatamente os bytes que `cifrar` produziu, nem um mais.
//!
//! Sem enquadramento, o primeiro `decifrar` de uma leitura parcial falha, e a
//! falha se parece com adulteração.
//!
//! ```text
//! QUADRO na rede      [u32 big-endian: tamanho][ciphertext]
//!
//! MENSAGEM lógica     1..n quadros; o PRIMEIRO BYTE do texto claro de cada
//!                     segmento diz se vem mais
//!                       0x00  vem mais
//!                       0x01  este é o último
//! ```
//!
//! O marcador de continuação vai **dentro** do texto cifrado, de propósito:
//! na rede, quem observa vê tamanhos, não estrutura. E vai no começo do
//! segmento, não num cabeçalho próprio, para não existir um segundo lugar
//! onde a mensagem possa se contradizer.
//!
//! ## O teto do Noise, que obriga a segmentar
//!
//! Uma mensagem Noise tem no máximo 65535 bytes, **incluindo** a etiqueta de
//! 16 bytes do AEAD. Com o byte de continuação, sobram [`MAIOR_SEGMENTO`] de
//! texto claro por segmento.
//!
//! Isso não é teórico para este projeto: o bundle de bootstrap da etapa 12
//! carrega acervo, e o contrato de blobs da etapa 13 admite asset de 8 MiB.
//! Mandar qualquer um dos dois num `cifrar` só falharia — e falharia tarde,
//! no aparelho de alguém.
//!
//! ## O teto nosso, que existe contra o outro lado
//!
//! [`MAIOR_MENSAGEM`] limita o que uma mensagem pode ocupar de memória. Um
//! peer hostil — ou com defeito — que anuncie tamanho enorme não consegue
//! fazer este lado alocar: o tamanho é conferido **antes** de qualquer
//! `vec![0; n]`, e é por isso que [`ler_mensagem`] recusa em vez de tentar.
//!
//! Num celular isso é a diferença entre uma sincronização que falha e um
//! processo morto pelo sistema.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::infrastructure::sync_transport::Transporte;

/// Maior ciphertext de um quadro. É o teto do Noise.
pub const MAIOR_QUADRO: usize = 65535;

/// Maior texto claro por segmento: o teto do Noise, menos a etiqueta do AEAD,
/// menos o byte de continuação.
pub const MAIOR_SEGMENTO: usize = MAIOR_QUADRO - 16 - 1;

/// Maior mensagem lógica que este lado aceita montar, em memória.
///
/// 64 MiB. O critério é o aparelho mais fraco dos dois: um bundle de bootstrap
/// com acervo e blobs passa de megabytes com facilidade, e 64 MiB ainda é
/// montável num celular modesto. Acima disso, o caminho certo é transferir por
/// blob (etapa 13), que já é feito peça por peça e verificado por hash — e não
/// aumentar este número.
pub const MAIOR_MENSAGEM: usize = 64 * 1024 * 1024;

/// Espera padrão de leitura e escrita.
///
/// Existe para o caso mais comum de rede local dando errado: o outro lado
/// abriu a conexão e parou de falar. Sem isso, a sincronização fica presa até
/// o TCP desistir sozinho, o que pode levar minutos.
pub const ESPERA_PADRAO: Duration = Duration::from_secs(10);

const VEM_MAIS: u8 = 0x00;
const ULTIMO: u8 = 0x01;

/// Por que o fio parou.
#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeFio {
    /// O quadro anunciou mais que [`MAIOR_QUADRO`]. Nada foi alocado.
    QuadroGrande { anunciado: usize },
    /// A mensagem passou de [`MAIOR_MENSAGEM`] somando segmentos.
    MensagemGrande { acumulado: usize },
    /// O fluxo terminou no meio de um quadro ou de uma mensagem.
    FimNoMeio { faltavam: usize },
    /// O outro lado abriu e não falou dentro da espera.
    Silencio,
    /// Erro de rede.
    Rede { motivo: String },
    /// O Noise recusou o segmento: etiqueta errada, ordem errada, adulteração.
    Cripto { motivo: String },
    /// Segmento sem nem o byte de continuação.
    SegmentoVazio,
}

impl std::fmt::Display for FalhaDeFio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDeFio::QuadroGrande { anunciado } => write!(
                f,
                "o outro aparelho anunciou um pacote de {anunciado} bytes, acima do limite do \
                 protocolo. A sincronização foi interrompida sem receber o pacote."
            ),
            FalhaDeFio::MensagemGrande { acumulado } => write!(
                f,
                "a transferência passou de {acumulado} bytes e foi interrompida. Um envio desse \
                 tamanho precisa vir por partes."
            ),
            FalhaDeFio::FimNoMeio { faltavam } => write!(
                f,
                "a conexão caiu com {faltavam} bytes ainda por receber. Nada foi aplicado — \
                 tente sincronizar de novo."
            ),
            FalhaDeFio::Silencio => write!(
                f,
                "o outro aparelho conectou e não respondeu no tempo esperado. Verifique se ele \
                 continua na mesma rede."
            ),
            FalhaDeFio::Rede { motivo } => {
                write!(f, "a conexão com o outro aparelho falhou: {motivo}.")
            }
            FalhaDeFio::Cripto { motivo } => write!(
                f,
                "os dados recebidos não passaram na verificação do canal seguro: {motivo}. Nada \
                 foi aplicado."
            ),
            FalhaDeFio::SegmentoVazio => write!(
                f,
                "o outro aparelho enviou um pacote vazio, que este protocolo não produz. Nada \
                 foi aplicado."
            ),
        }
    }
}

fn de_io(erro: std::io::Error) -> FalhaDeFio {
    match erro.kind() {
        // `WouldBlock` é o que o Windows devolve num `SO_RCVTIMEO` estourado;
        // `TimedOut` é o que os outros devolvem. Os dois são a mesma coisa
        // para quem está esperando o peer falar.
        ErrorKind::WouldBlock | ErrorKind::TimedOut => FalhaDeFio::Silencio,
        _ => FalhaDeFio::Rede {
            motivo: erro.to_string(),
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Quadro
// ═══════════════════════════════════════════════════════════════════════════

/// Escreve um quadro cru. É o que o **handshake** usa, porque ali ainda não
/// existe [`Transporte`] — as mensagens do `XX` são públicas por construção.
pub fn escrever_quadro(destino: &mut impl Write, bytes: &[u8]) -> Result<(), FalhaDeFio> {
    if bytes.len() > MAIOR_QUADRO {
        return Err(FalhaDeFio::QuadroGrande {
            anunciado: bytes.len(),
        });
    }
    let tamanho = u32::try_from(bytes.len()).expect("cabe, o teto é 65535");
    destino.write_all(&tamanho.to_be_bytes()).map_err(de_io)?;
    destino.write_all(bytes).map_err(de_io)?;
    // Sem `flush` a última escrita pode ficar no buffer de um `BufWriter`, e o
    // outro lado esperaria para sempre por bytes que já "foram enviados".
    destino.flush().map_err(de_io)
}

/// Lê um quadro cru.
///
/// **Confere o tamanho antes de alocar.** É o ponto em que um peer hostil
/// tentaria fazer este lado reservar memória que não tem.
pub fn ler_quadro(origem: &mut impl Read) -> Result<Vec<u8>, FalhaDeFio> {
    let mut cabecalho = [0_u8; 4];
    ler_exato(origem, &mut cabecalho)?;
    let anunciado = u32::from_be_bytes(cabecalho) as usize;

    if anunciado > MAIOR_QUADRO {
        return Err(FalhaDeFio::QuadroGrande { anunciado });
    }

    let mut bytes = vec![0_u8; anunciado];
    ler_exato(origem, &mut bytes)?;
    Ok(bytes)
}

/// `read_exact` com a distinção que importa: fluxo que acabou no meio não é a
/// mesma coisa que rede que falhou, e nenhum dos dois é adulteração.
fn ler_exato(origem: &mut impl Read, destino: &mut [u8]) -> Result<(), FalhaDeFio> {
    let mut lidos = 0;
    while lidos < destino.len() {
        match origem.read(&mut destino[lidos..]) {
            Ok(0) => {
                return Err(FalhaDeFio::FimNoMeio {
                    faltavam: destino.len() - lidos,
                })
            }
            Ok(n) => lidos += n,
            Err(erro) if erro.kind() == ErrorKind::Interrupted => continue,
            Err(erro) => return Err(de_io(erro)),
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Mensagem
// ═══════════════════════════════════════════════════════════════════════════

/// Cifra e envia uma mensagem lógica, segmentando o que passar do teto do
/// Noise.
///
/// Uma mensagem de um byte também vira um segmento — o caminho é o mesmo, e
/// não existe atalho para mensagem pequena que possa divergir do caminho da
/// grande.
pub fn escrever_mensagem(
    transporte: &mut Transporte,
    destino: &mut impl Write,
    carga: &[u8],
) -> Result<(), FalhaDeFio> {
    if carga.len() > MAIOR_MENSAGEM {
        return Err(FalhaDeFio::MensagemGrande {
            acumulado: carga.len(),
        });
    }

    let mut restante = carga;
    loop {
        let quanto = restante.len().min(MAIOR_SEGMENTO);
        let (agora, depois) = restante.split_at(quanto);
        let ultimo = depois.is_empty();

        let mut claro = Vec::with_capacity(agora.len() + 1);
        claro.push(if ultimo { ULTIMO } else { VEM_MAIS });
        claro.extend_from_slice(agora);

        let cifrado = transporte
            .cifrar(&claro)
            .map_err(|erro| FalhaDeFio::Cripto {
                motivo: erro.message,
            })?;
        escrever_quadro(destino, &cifrado)?;

        if ultimo {
            return Ok(());
        }
        restante = depois;
    }
}

/// Recebe e decifra uma mensagem lógica inteira.
///
/// **Ou devolve a mensagem completa, ou devolve erro.** Nunca devolve o que
/// chegou até agora: metade de um bundle aplicada é pior que nenhum bundle, e
/// quem chama não teria como saber a diferença.
pub fn ler_mensagem(
    transporte: &mut Transporte,
    origem: &mut impl Read,
) -> Result<Vec<u8>, FalhaDeFio> {
    let mut montada: Vec<u8> = Vec::new();
    loop {
        let cifrado = ler_quadro(origem)?;
        let claro = transporte
            .decifrar(&cifrado)
            .map_err(|erro| FalhaDeFio::Cripto {
                motivo: erro.message,
            })?;

        let (marcador, pedaco) = claro.split_first().ok_or(FalhaDeFio::SegmentoVazio)?;

        if montada.len() + pedaco.len() > MAIOR_MENSAGEM {
            return Err(FalhaDeFio::MensagemGrande {
                acumulado: montada.len() + pedaco.len(),
            });
        }
        montada.extend_from_slice(pedaco);

        match *marcador {
            ULTIMO => return Ok(montada),
            VEM_MAIS => continue,
            // Marcador desconhecido é versão futura ou dado corrompido que
            // passou pelo AEAD — o que só acontece se o outro lado tiver a
            // chave. Nos dois casos, não adivinhar.
            outro => {
                return Err(FalhaDeFio::Cripto {
                    motivo: format!("marcador de segmento desconhecido: {outro:#04x}"),
                })
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Socket
// ═══════════════════════════════════════════════════════════════════════════

/// Abre a escuta. Papel de **transporte** apenas: não faz deste lado servidor,
/// nem autoritativo, nem dono do dado (ADR 0009 §2).
pub fn ouvir(endereco: impl ToSocketAddrs) -> Result<TcpListener, FalhaDeFio> {
    TcpListener::bind(endereco).map_err(de_io)
}

/// Conecta com espera limitada.
///
/// `TcpStream::connect` sozinho não tem tempo limite, e num endereço errado da
/// rede local isso vira a diferença entre "não achei" e a tela parada.
pub fn conectar(endereco: impl ToSocketAddrs, espera: Duration) -> Result<TcpStream, FalhaDeFio> {
    let mut ultimo = None;
    for alvo in endereco.to_socket_addrs().map_err(de_io)? {
        match TcpStream::connect_timeout(&alvo, espera) {
            Ok(fluxo) => {
                ajustar_esperas(&fluxo, espera)?;
                return Ok(fluxo);
            }
            Err(erro) => ultimo = Some(erro),
        }
    }
    Err(ultimo.map(de_io).unwrap_or(FalhaDeFio::Rede {
        motivo: "nenhum endereço para tentar".into(),
    }))
}

/// Põe tempo limite nas duas pontas do socket.
///
/// Leitura **e** escrita: um peer que não lê enche a janela do TCP e prende
/// quem escreve, do mesmo jeito que um que não escreve prende quem lê.
pub fn ajustar_esperas(fluxo: &TcpStream, espera: Duration) -> Result<(), FalhaDeFio> {
    fluxo.set_read_timeout(Some(espera)).map_err(de_io)?;
    fluxo.set_write_timeout(Some(espera)).map_err(de_io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::DeviceIdentity;
    use crate::infrastructure::sync_transport::{
        autenticar, provar_identidade, Handshake, ProvaDeIdentidade, SessaoAutenticada,
    };
    use std::io::Cursor;
    use std::net::SocketAddr;

    fn estatica() -> [u8; 32] {
        let mut bytes = [0_u8; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut bytes);
        bytes
    }

    /// Dois `Transporte` prontos, sem socket — para os gates de enquadramento
    /// que não têm nada a ver com rede.
    fn dois_transportes() -> (Transporte, Transporte) {
        let (a, b) = (estatica(), estatica());
        let mut conector = Handshake::conector(&a).expect("conector");
        let mut ouvinte = Handshake::ouvinte(&b).expect("ouvinte");

        let m = conector.escrever(&[]).expect("m1");
        ouvinte.ler(&m).expect("m1");
        let m = ouvinte.escrever(&[]).expect("m2");
        conector.ler(&m).expect("m2");
        let m = conector.escrever(&[]).expect("m3");
        ouvinte.ler(&m).expect("m3");

        (
            conector.concluir().expect("transporte A"),
            ouvinte.concluir().expect("transporte B"),
        )
    }

    // ═══════════════════════════════════════════════════════════════════════
    // O teto do Noise
    // ═══════════════════════════════════════════════════════════════════════

    /// **Mensagem maior que o limite do Noise atravessa íntegra.**
    ///
    /// É o gate central da fatia: sem segmentação, `cifrar` falha acima de
    /// 65519 bytes, e o bundle da etapa 12 e o asset da etapa 13 passam disso.
    ///
    /// Os tamanhos não são redondos de propósito. Múltiplo exato do segmento é
    /// o caso em que um erro de fronteira se esconde — o último segmento sai
    /// vazio, e um enquadramento que não marque o fim direito devolve mensagem
    /// truncada sem erro nenhum.
    #[test]
    fn mensagem_maior_que_o_teto_do_noise_atravessa_integra() {
        for tamanho in [
            0,
            1,
            MAIOR_SEGMENTO - 1,
            MAIOR_SEGMENTO,
            MAIOR_SEGMENTO + 1,
            MAIOR_SEGMENTO * 2,
            MAIOR_SEGMENTO * 3 + 7,
            5 * 1024 * 1024,
        ] {
            let (mut envia, mut recebe) = dois_transportes();
            // Conteúdo dependente da posição: um enquadramento que repetisse
            // ou trocasse a ordem dos segmentos passaria por um buffer de
            // zeros e morre neste.
            let carga: Vec<u8> = (0..tamanho).map(|i| (i % 251) as u8).collect();

            let mut fio = Vec::new();
            escrever_mensagem(&mut envia, &mut fio, &carga).expect("escrever");
            let lida = ler_mensagem(&mut recebe, &mut Cursor::new(fio)).expect("ler");

            assert_eq!(lida.len(), carga.len(), "tamanho de {tamanho}");
            assert_eq!(lida, carga, "conteúdo de {tamanho}");
        }
    }

    /// Uma mensagem grande **é** mais de um quadro, e a segmentação não é
    /// decorativa.
    ///
    /// Sem este gate, `MAIOR_SEGMENTO` poderia estar errado por 17 bytes e o
    /// gate acima continuaria verde — porque o `snow` aceitaria o segmento e
    /// ninguém contaria os quadros.
    #[test]
    fn a_segmentacao_acontece_de_verdade() {
        let (mut envia, _) = dois_transportes();
        let mut fio = Vec::new();
        escrever_mensagem(&mut envia, &mut fio, &vec![7_u8; MAIOR_SEGMENTO * 2 + 1])
            .expect("escrever");

        let mut quantos = 0;
        let mut resto = &fio[..];
        while !resto.is_empty() {
            let tamanho = u32::from_be_bytes([resto[0], resto[1], resto[2], resto[3]]) as usize;
            assert!(tamanho <= MAIOR_QUADRO, "quadro acima do teto do Noise");
            resto = &resto[4 + tamanho..];
            quantos += 1;
        }
        assert_eq!(
            quantos, 3,
            "dois segmentos cheios e um com o byte que sobrou"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // O teto nosso
    // ═══════════════════════════════════════════════════════════════════════

    /// **Quadro grande demais é recusado sem alocar.**
    ///
    /// O cabeçalho anuncia 4 GiB e nada mais vem atrás. Se a leitura alocasse
    /// primeiro e conferisse depois, este teste não terminaria — e num celular
    /// o processo morreria.
    #[test]
    fn quadro_gigante_e_recusado_antes_de_alocar() {
        let mut fio = Cursor::new(u32::MAX.to_be_bytes().to_vec());
        assert_eq!(
            ler_quadro(&mut fio),
            Err(FalhaDeFio::QuadroGrande {
                anunciado: u32::MAX as usize
            })
        );

        // E o limite é exatamente o teto do Noise, não "algum número grande".
        let mut fio = Cursor::new(((MAIOR_QUADRO + 1) as u32).to_be_bytes().to_vec());
        assert_eq!(
            ler_quadro(&mut fio),
            Err(FalhaDeFio::QuadroGrande {
                anunciado: MAIOR_QUADRO + 1
            })
        );
    }

    /// Mensagem acima do teto é recusada na escrita, antes de qualquer byte ir
    /// para a rede.
    #[test]
    fn mensagem_acima_do_teto_nao_e_enviada() {
        let (mut envia, _) = dois_transportes();
        let mut fio = Vec::new();
        let erro = escrever_mensagem(&mut envia, &mut fio, &vec![0_u8; MAIOR_MENSAGEM + 1])
            .expect_err("tinha que recusar");
        assert_eq!(
            erro,
            FalhaDeFio::MensagemGrande {
                acumulado: MAIOR_MENSAGEM + 1
            }
        );
        assert!(fio.is_empty(), "nada podia ter sido escrito");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Fluxo que acaba, e dado que não confere
    // ═══════════════════════════════════════════════════════════════════════

    /// **Mensagem cortada no meio é erro, não mensagem parcial.**
    ///
    /// Três cortes: no cabeçalho, no meio do ciphertext, e entre segmentos de
    /// uma mensagem de vários. O terceiro é o que importa mais — é o caso em
    /// que já existe conteúdo montado, e devolver "o que deu" seria aplicar
    /// metade de um bundle.
    #[test]
    fn fluxo_cortado_no_meio_nunca_devolve_mensagem_parcial() {
        let (mut envia, _) = dois_transportes();
        let mut inteiro = Vec::new();
        escrever_mensagem(&mut envia, &mut inteiro, &vec![9_u8; MAIOR_SEGMENTO * 2])
            .expect("escrever");

        for corte in [2, 10, inteiro.len() / 2, inteiro.len() - 1] {
            let (_, mut recebe) = dois_transportes();
            let resultado = ler_mensagem(&mut recebe, &mut Cursor::new(inteiro[..corte].to_vec()));
            assert!(
                matches!(
                    resultado,
                    Err(FalhaDeFio::FimNoMeio { .. }) | Err(FalhaDeFio::Cripto { .. })
                ),
                "corte em {corte} devolveu {resultado:?}"
            );
        }
    }

    /// Um byte trocado no ciphertext não passa, e a mensagem não chega pela
    /// metade.
    ///
    /// Quem garante isso é o AEAD do Noise, não este módulo — e o gate existe
    /// para que o enquadramento não tenha inventado um caminho que escape
    /// dele.
    #[test]
    fn um_byte_adulterado_nao_atravessa() {
        let (mut envia, mut recebe) = dois_transportes();
        let mut fio = Vec::new();
        escrever_mensagem(&mut envia, &mut fio, b"evento do capitulo").expect("escrever");

        let ultimo = fio.len() - 1;
        fio[ultimo] ^= 0x01;

        assert!(matches!(
            ler_mensagem(&mut recebe, &mut Cursor::new(fio)),
            Err(FalhaDeFio::Cripto { .. })
        ));
    }

    /// Mensagens seguidas mantêm a ordem e a fronteira.
    ///
    /// O contador de nonce do Noise é por mensagem; se o enquadramento
    /// juntasse ou trocasse duas, a segunda não decifraria.
    #[test]
    fn mensagens_seguidas_mantem_a_ordem() {
        let (mut envia, mut recebe) = dois_transportes();
        let mut fio = Vec::new();
        for i in 0_u8..5 {
            escrever_mensagem(&mut envia, &mut fio, &[i; 3]).expect("escrever");
        }

        let mut origem = Cursor::new(fio);
        for i in 0_u8..5 {
            assert_eq!(
                ler_mensagem(&mut recebe, &mut origem).expect("ler"),
                vec![i; 3],
                "a {i}-ésima mensagem saiu fora de ordem"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TCP de verdade
    // ═══════════════════════════════════════════════════════════════════════

    /// **O handshake `XX` inteiro sobre `TcpListener`, e a identidade vinculada
    /// à conexão.**
    ///
    /// É a primeira vez que o Sync V2 atravessa um socket. O gate não termina
    /// no canal cifrado: ele confere que o `device_id` que cada lado obtém é o
    /// do **outro**, derivado da prova assinada sobre o hash daquela conexão —
    /// que é a invariante da etapa 8, agora sobre rede.
    ///
    /// Porta `0`: o sistema escolhe uma livre. Porta fixa em teste dá conflito
    /// quando dois testes rodam ao mesmo tempo, e o modo de falhar é
    /// intermitente.
    #[test]
    fn o_handshake_e_a_identidade_atravessam_um_socket_de_verdade() {
        let escuta = ouvir(("127.0.0.1", 0)).expect("ouvir");
        let porta: SocketAddr = escuta.local_addr().expect("endereço");

        let ident_ouvinte = DeviceIdentity::generate();
        let esperado_do_ouvinte = ident_ouvinte.public_base32();
        let estatica_ouvinte = estatica();

        // O lado que escuta, numa thread. Threads e não async: o projeto não
        // tem runtime assíncrono próprio, e duas pontas de um handshake não
        // justificam introduzir um.
        let servidor = std::thread::spawn(move || -> Result<SessaoAutenticada, FalhaDeFio> {
            let (mut fluxo, _) = escuta.accept().map_err(de_io)?;
            ajustar_esperas(&fluxo, ESPERA_PADRAO)?;

            let mut aperto = Handshake::ouvinte(&estatica_ouvinte).expect("ouvinte");
            let m1 = ler_quadro(&mut fluxo)?;
            aperto.ler(&m1).expect("m1");
            let m2 = aperto.escrever(&[]).expect("m2");
            escrever_quadro(&mut fluxo, &m2)?;
            let m3 = ler_quadro(&mut fluxo)?;
            aperto.ler(&m3).expect("m3");

            let hash = aperto.hash_do_handshake();
            let minha = provar_identidade(&ident_ouvinte, &hash);
            let mut transporte = aperto.concluir().expect("transporte");

            // A prova vai pelo canal já cifrado, com o enquadramento deste
            // módulo — é mensagem lógica como qualquer outra.
            escrever_mensagem(
                &mut transporte,
                &mut fluxo,
                format!("{}|{}", minha.ed25519_public, minha.assinatura).as_bytes(),
            )?;
            let recebida = ler_mensagem(&mut transporte, &mut fluxo)?;
            let texto = String::from_utf8(recebida).expect("prova em texto");
            let (publica, assinatura) = texto.split_once('|').expect("prova com dois campos");

            let sessao = autenticar(
                &ProvaDeIdentidade {
                    ed25519_public: publica.into(),
                    assinatura: assinatura.into(),
                },
                &hash,
            )
            .expect("a prova do conector tinha que verificar");

            // E o canal continua servindo depois da autenticação.
            let eco = ler_mensagem(&mut transporte, &mut fluxo)?;
            escrever_mensagem(&mut transporte, &mut fluxo, &eco)?;
            Ok(sessao)
        });

        let ident_conector = DeviceIdentity::generate();
        let esperado_do_conector = ident_conector.public_base32();
        let mut fluxo = conectar(porta, ESPERA_PADRAO).expect("conectar");

        let mut aperto = Handshake::conector(&estatica()).expect("conector");
        let m1 = aperto.escrever(&[]).expect("m1");
        escrever_quadro(&mut fluxo, &m1).expect("m1");
        let m2 = ler_quadro(&mut fluxo).expect("m2");
        aperto.ler(&m2).expect("m2");
        let m3 = aperto.escrever(&[]).expect("m3");
        escrever_quadro(&mut fluxo, &m3).expect("m3");

        let hash = aperto.hash_do_handshake();
        let minha = provar_identidade(&ident_conector, &hash);
        let mut transporte = aperto.concluir().expect("transporte");

        let recebida = ler_mensagem(&mut transporte, &mut fluxo).expect("prova do ouvinte");
        let texto = String::from_utf8(recebida).expect("prova em texto");
        let (publica, assinatura) = texto.split_once('|').expect("prova com dois campos");
        let sessao_do_ouvinte = autenticar(
            &ProvaDeIdentidade {
                ed25519_public: publica.into(),
                assinatura: assinatura.into(),
            },
            &hash,
        )
        .expect("a prova do ouvinte tinha que verificar");

        escrever_mensagem(
            &mut transporte,
            &mut fluxo,
            format!("{}|{}", minha.ed25519_public, minha.assinatura).as_bytes(),
        )
        .expect("minha prova");

        // Uma carga que passa do teto do Noise, para provar que a segmentação
        // funciona sobre TCP e não só sobre `Vec`. É aqui que TCP juntaria ou
        // partiria escritas se o enquadramento não existisse.
        let grande: Vec<u8> = (0..MAIOR_SEGMENTO * 2 + 13)
            .map(|i| (i % 251) as u8)
            .collect();
        escrever_mensagem(&mut transporte, &mut fluxo, &grande).expect("enviar grande");
        assert_eq!(
            ler_mensagem(&mut transporte, &mut fluxo).expect("eco"),
            grande,
            "a carga voltou diferente do que foi"
        );

        let sessao_do_conector = servidor.join().expect("thread do ouvinte").expect("sessão");

        assert_eq!(
            sessao_do_ouvinte.ed25519_public(),
            esperado_do_ouvinte,
            "o conector autenticou uma identidade que não é a do ouvinte"
        );
        assert_eq!(
            sessao_do_conector.ed25519_public(),
            esperado_do_conector,
            "o ouvinte autenticou uma identidade que não é a do conector"
        );
        assert_ne!(
            sessao_do_ouvinte.device_id(),
            sessao_do_conector.device_id(),
            "dois aparelhos diferentes não podem ter o mesmo device_id"
        );
    }

    /// **Peer que abre e não fala é derrubado pelo tempo limite.**
    ///
    /// Sem isso a sincronização fica presa até o TCP desistir sozinho, o que
    /// leva minutos — com a tela parada e sem nada a dizer ao usuário.
    ///
    /// A espera é curta de propósito: o que se prova é que existe uma, não
    /// quanto ela vale.
    #[test]
    fn peer_que_conecta_e_nao_fala_cai_no_tempo_limite() {
        let escuta = ouvir(("127.0.0.1", 0)).expect("ouvir");
        let porta: SocketAddr = escuta.local_addr().expect("endereço");

        // Conecta e não diz nada. O `_mudo` fica vivo até o fim do teste: se
        // fosse descartado, o outro lado veria fim de fluxo em vez de silêncio,
        // e o teste passaria pelo motivo errado.
        let _mudo = conectar(porta, ESPERA_PADRAO).expect("conectar");

        let (fluxo, _) = escuta.accept().expect("aceitar");
        ajustar_esperas(&fluxo, Duration::from_millis(150)).expect("esperas");

        let comeco = std::time::Instant::now();
        let mut leitor = &fluxo;
        assert_eq!(
            ler_quadro(&mut leitor),
            Err(FalhaDeFio::Silencio),
            "leitura sem dado tinha que estourar a espera"
        );
        assert!(
            comeco.elapsed() < Duration::from_secs(5),
            "a espera não foi respeitada: {:?}",
            comeco.elapsed()
        );
    }

    /// Conectar num endereço que não escuta falha rápido, e como erro de rede.
    #[test]
    fn conectar_em_porta_fechada_falha_como_rede() {
        // Uma porta que estava aberta e fechou: garante que ninguém escuta
        // nela, sem depender de "espero que 1 esteja livre".
        let escuta = ouvir(("127.0.0.1", 0)).expect("ouvir");
        let porta: SocketAddr = escuta.local_addr().expect("endereço");
        drop(escuta);

        match conectar(porta, Duration::from_millis(400)) {
            Err(FalhaDeFio::Rede { .. }) | Err(FalhaDeFio::Silencio) => {}
            outro => panic!("esperava falha de rede, veio {outro:?}"),
        }
    }
}
