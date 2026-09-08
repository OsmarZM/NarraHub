//! Sync V2 — pareamento por QR (ADR 0009 §6.1, etapa 9).
//!
//! ## A regra que organiza tudo aqui
//!
//! ```text
//! O QR prova que ESTA CONEXÃO FOI CONVIDADA.
//! A Ed25519 prova QUEM CHEGOU pela conexão.
//! ```
//!
//! São perguntas diferentes, e confundi-las desfaria a etapa 8 inteira. O
//! convite **não carrega `device_id`** — nem do lado que mostra, nem do lado
//! que lê. Se carregasse, a tentação seguinte seria "conectou e disse que é o
//! ABC do QR, então é o ABC", que é exatamente a confiança declarativa que a
//! etapa 8 gastou uma PR para eliminar.
//!
//! > **Divergência declarada do ADR:** a seção 6.1 lista `fingerprint` como
//! > campo do QR, para o leitor conferir com quem falou. Foi **removido** por
//! > decisão do autor. Com `XXpsk0` e segredo de 32 bytes, um intermediário
//! > não completa o handshake sem o segredo — o `fingerprint` acrescentava
//! > pouco e convidava ao erro declarativo. O ADR foi atualizado junto.
//!
//! ## Rede não cria intenção de usuário
//!
//! Esta é a outra metade da etapa, e é a que a NH-057 pediu:
//!
//! ```text
//! PACOTE ALEATÓRIO DA REDE          FLUXO DE PAREAMENTO
//! origem desconhecida               QR válido
//!        ↓                                ↓
//! descarta / limita taxa            Noise XXpsk0
//!        ↓                                ↓
//!    zero UI                        prova Ed25519
//!                                         ↓
//!                                  SessaoAutenticada
//!                                         ↓
//!                                  pedido explícito → UI
//! ```
//!
//! Sem essa separação, mandar `device-000001`, `device-000002`, … viraria
//! *"14.392 aparelhos querem entrar"* — uma tela que treina o escritor a
//! ignorar a tela.
//!
//! ## Convite: aleatório, curto e de uso único
//!
//! ```text
//! CSPRNG 256 bits  →  TTL de 3 minutos  →  primeiro pareamento  →  CONSUMIDO
//! ```
//!
//! **Os convites vivem só em memória**, e isso é decisão, não preguiça: o
//! segredo é material criptográfico, e o banco vai para backup — o mesmo
//! argumento que mantém a chave privada fora dele (ADR §5). O efeito colateral
//! é bom: fechar o aplicativo invalida os convites abertos, e errar para o
//! lado de invalidar é o lado certo de errar.

use crate::domain::identity::{base32, decode_base32, DeviceIdentity};
use crate::infrastructure::sync_transport::{
    autenticar, provar_identidade, Handshake, ProvaDeIdentidade, SessaoAutenticada,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Versão do formato do convite. Um aparelho novo lendo um QR antigo, ou o
/// contrário, precisa dizer "não entendo" em vez de interpretar errado.
pub const VERSAO_DO_CONVITE: u32 = 1;

/// Quanto tempo um convite vale. Curto de propósito: o QR está na tela, o
/// aparelho está na mão, e uma foto do código não pode servir daqui a dias.
pub const VALIDADE: Duration = Duration::from_secs(180);

const PADRAO_PAREAMENTO: &str = "Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s";

/// O que o QR carrega. Repare no que **não** está aqui: identidade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Convite {
    pub versao: u32,
    /// Identifica a tentativa, não o aparelho. Opaco e público.
    pub id: String,
    /// 32 bytes de CSPRNG, em base32. Vira o `psk` do `XXpsk0`.
    pub segredo: String,
    /// Onde encontrar quem convidou. Dica de transporte, sem valor de prova.
    pub endereco: String,
}

impl Convite {
    /// O texto que vai para dentro do QR.
    ///
    /// Formato simples e delimitado, em vez de JSON: o conteúdo é fixo, e um
    /// parser mais esperto só abriria caminho para ambiguidade num dado que
    /// chega de fora.
    pub fn para_qr(&self) -> String {
        format!(
            "narrahub-pair:{}:{}:{}:{}",
            self.versao, self.id, self.segredo, self.endereco
        )
    }

    pub fn ler_qr(texto: &str) -> Result<Self, FalhaDePareamento> {
        // `splitn` e não `split`: o endereço tem porta, e toda porta traz um
        // `:` junto. A primeira versão dividia em exatamente cinco partes e
        // recusava qualquer convite com endereço de verdade — o teste de ida e
        // volta pegou na primeira execução.
        let partes: Vec<&str> = texto.splitn(5, ':').collect();
        if partes.len() != 5 || partes[0] != "narrahub-pair" {
            return Err(FalhaDePareamento::ConviteIlegivel);
        }
        let versao: u32 = partes[1]
            .parse()
            .map_err(|_| FalhaDePareamento::ConviteIlegivel)?;
        if versao != VERSAO_DO_CONVITE {
            return Err(FalhaDePareamento::VersaoIncompativel { lida: versao });
        }
        // O segredo precisa ter os 32 bytes que o `psk` exige. Um convite com
        // menos entropia entraria no handshake como se fosse forte.
        let bytes = decode_base32(partes[3]).ok_or(FalhaDePareamento::ConviteIlegivel)?;
        if bytes.len() != 32 {
            return Err(FalhaDePareamento::ConviteIlegivel);
        }
        Ok(Self {
            versao,
            id: partes[2].to_string(),
            segredo: partes[3].to_string(),
            endereco: partes[4].to_string(),
        })
    }

    fn psk(&self) -> Result<[u8; 32], FalhaDePareamento> {
        decode_base32(&self.segredo)
            .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
            .ok_or(FalhaDePareamento::ConviteIlegivel)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDePareamento {
    ConviteIlegivel,
    VersaoIncompativel {
        lida: u32,
    },
    /// O convite não existe: nunca existiu, ou o aplicativo foi reaberto.
    ConviteDesconhecido,
    ConviteExpirado,
    /// Já foi usado. Uma foto do QR não vale duas vezes.
    ConviteConsumido,
    /// O handshake não fechou. Com `XXpsk0`, o motivo mais comum é segredo
    /// errado — e ele falha por criptografia, não por comparação.
    HandshakeRecusado,
    /// O handshake fechou, mas a prova de identidade não.
    IdentidadeNaoProvada,
}

impl std::fmt::Display for FalhaDePareamento {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FalhaDePareamento::ConviteIlegivel => {
                f.write_str("O código lido não é um convite do NarraHub.")
            }
            FalhaDePareamento::VersaoIncompativel { lida } => write!(
                f,
                "O convite é da versão {lida} e este aplicativo entende a {VERSAO_DO_CONVITE}. \
                 Atualize os dois aparelhos."
            ),
            FalhaDePareamento::ConviteDesconhecido => {
                f.write_str("Este convite não está mais disponível. Gere um código novo.")
            }
            FalhaDePareamento::ConviteExpirado => {
                f.write_str("Este convite expirou. Gere um código novo.")
            }
            FalhaDePareamento::ConviteConsumido => {
                f.write_str("Este convite já foi usado. Gere um código novo.")
            }
            FalhaDePareamento::HandshakeRecusado => {
                f.write_str("Não foi possível estabelecer a conexão segura com o outro aparelho.")
            }
            FalhaDePareamento::IdentidadeNaoProvada => {
                f.write_str("O outro aparelho não conseguiu provar a própria identidade.")
            }
        }
    }
}

/// Os convites abertos deste aparelho. Só em memória.
#[derive(Default)]
pub struct Convites {
    abertos: HashMap<String, EstadoDoConvite>,
}

struct EstadoDoConvite {
    segredo: String,
    criado_em: Instant,
    consumido: bool,
}

impl Convites {
    /// Emite um convite novo, com segredo de 32 bytes de CSPRNG.
    pub fn emitir(&mut self, endereco: &str) -> Convite {
        let mut segredo = [0_u8; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut segredo);
        let mut id = [0_u8; 8];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut id);

        let convite = Convite {
            versao: VERSAO_DO_CONVITE,
            id: base32(&id),
            segredo: base32(&segredo),
            endereco: endereco.to_string(),
        };
        self.abertos.insert(
            convite.id.clone(),
            EstadoDoConvite {
                segredo: convite.segredo.clone(),
                criado_em: Instant::now(),
                consumido: false,
            },
        );
        convite
    }

    /// O segredo de um convite ainda válido, **sem consumi-lo**.
    ///
    /// A checagem acontece antes de qualquer criptografia: gastar um handshake
    /// com um convite expirado seria trabalho para dar a mesma resposta.
    fn segredo_valido(&self, id: &str) -> Result<String, FalhaDePareamento> {
        let estado = self
            .abertos
            .get(id)
            .ok_or(FalhaDePareamento::ConviteDesconhecido)?;
        if estado.consumido {
            return Err(FalhaDePareamento::ConviteConsumido);
        }
        if estado.criado_em.elapsed() > VALIDADE {
            return Err(FalhaDePareamento::ConviteExpirado);
        }
        Ok(estado.segredo.clone())
    }

    /// Marca como consumido. Chamado só depois de o pareamento fechar.
    fn consumir(&mut self, id: &str) {
        if let Some(estado) = self.abertos.get_mut(id) {
            estado.consumido = true;
        }
    }

    #[cfg(test)]
    fn envelhecer(&mut self, id: &str, quanto: Duration) {
        if let Some(estado) = self.abertos.get_mut(id) {
            estado.criado_em -= quanto;
        }
    }
}

/// O resultado de um pareamento bem-sucedido.
///
/// **Este** é o tipo que produz interface. Um envelope solto vindo da rede não
/// produz — a diferença é a NH-057.
#[derive(Debug, Clone)]
pub struct PedidoDeEntrada {
    pub sessao: SessaoAutenticada,
    /// Qual convite autorizou o encontro. Serve para a tela dizer "o aparelho
    /// que leu o código que você mostrou", e não "um aparelho qualquer".
    pub convite_id: String,
}

/// As mensagens do pareamento, na ordem.
///
/// Deliberadamente explícito em vez de escondido atrás de um socket: a etapa 9
/// decide o **protocolo**; quem carrega os bytes é a camada de rede, que esta
/// fase não entrega.
pub struct Pareamento {
    pub(crate) handshake: Handshake,
    convite_id: String,
}

impl Pareamento {
    /// Lado de quem **mostra** o QR e espera alguém ler.
    pub fn anfitriao(
        convites: &Convites,
        convite_id: &str,
        estatica: &[u8; 32],
    ) -> Result<Self, FalhaDePareamento> {
        let segredo = convites.segredo_valido(convite_id)?;
        let psk = Convite {
            versao: VERSAO_DO_CONVITE,
            id: convite_id.to_string(),
            segredo,
            endereco: String::new(),
        }
        .psk()?;
        Ok(Self {
            handshake: construir(estatica, &psk, convite_id, false)?,
            convite_id: convite_id.to_string(),
        })
    }

    /// Lado de quem **leu** o QR.
    pub fn visitante(convite: &Convite, estatica: &[u8; 32]) -> Result<Self, FalhaDePareamento> {
        let psk = convite.psk()?;
        Ok(Self {
            handshake: construir(estatica, &psk, &convite.id, true)?,
            convite_id: convite.id.clone(),
        })
    }

    pub fn escrever(&mut self, carga: &[u8]) -> Result<Vec<u8>, FalhaDePareamento> {
        self.handshake
            .escrever(carga)
            .map_err(|_| FalhaDePareamento::HandshakeRecusado)
    }

    pub fn ler(&mut self, mensagem: &[u8]) -> Result<Vec<u8>, FalhaDePareamento> {
        self.handshake
            .ler(mensagem)
            .map_err(|_| FalhaDePareamento::HandshakeRecusado)
    }

    pub fn terminou(&self) -> bool {
        self.handshake.terminou()
    }

    /// A prova que este lado manda depois do handshake.
    pub fn prova(&self, identidade: &DeviceIdentity) -> ProvaDeIdentidade {
        provar_identidade(identidade, &self.handshake.hash_do_handshake())
    }

    /// Conclui: verifica a prova do outro lado e **consome** o convite.
    ///
    /// A ordem importa. Consumir antes de verificar deixaria um convite
    /// queimado por qualquer tentativa malfeita, e o escritor teria que gerar
    /// um código novo por causa de ruído na rede.
    pub fn concluir(
        self,
        convites: &mut Convites,
        prova_do_outro: &ProvaDeIdentidade,
    ) -> Result<PedidoDeEntrada, FalhaDePareamento> {
        let sessao = autenticar(prova_do_outro, &self.handshake.hash_do_handshake())
            .map_err(|_| FalhaDePareamento::IdentidadeNaoProvada)?;
        convites.consumir(&self.convite_id);
        Ok(PedidoDeEntrada {
            sessao,
            convite_id: self.convite_id,
        })
    }
}

/// O `invitation_id` entra como **prólogo** do Noise, além de identificar o
/// convite.
///
/// Isso amarra o handshake àquela tentativa: dois lados usando o segredo certo
/// mas ids diferentes não fecham. Sem o prólogo, um convite poderia ser
/// completado no contexto de outro.
fn construir(
    estatica: &[u8; 32],
    psk: &[u8; 32],
    convite_id: &str,
    inicia: bool,
) -> Result<Handshake, FalhaDePareamento> {
    Handshake::com_psk(
        PADRAO_PAREAMENTO,
        estatica,
        psk,
        convite_id.as_bytes(),
        inicia,
    )
    .map_err(|_| FalhaDePareamento::HandshakeRecusado)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ponta {
        identidade: DeviceIdentity,
        estatica: [u8; 32],
    }

    impl Ponta {
        fn nova() -> Self {
            let mut estatica = [0_u8; 32];
            rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut estatica);
            Self {
                identidade: DeviceIdentity::generate(),
                estatica,
            }
        }
    }

    /// Roda o pareamento inteiro e devolve o que os dois lados concluíram.
    fn parear(
        convites: &mut Convites,
        convite_do_visitante: &Convite,
        anfitriao: &Ponta,
        visitante: &Ponta,
    ) -> Result<(PedidoDeEntrada, SessaoAutenticada), FalhaDePareamento> {
        let mut lado_a =
            Pareamento::anfitriao(convites, &convite_do_visitante.id, &anfitriao.estatica)?;
        let mut lado_v = Pareamento::visitante(convite_do_visitante, &visitante.estatica)?;

        let m1 = lado_v.escrever(&[])?;
        lado_a.ler(&m1)?;
        let m2 = lado_a.escrever(&[])?;
        lado_v.ler(&m2)?;
        let m3 = lado_v.escrever(&[])?;
        lado_a.ler(&m3)?;

        assert!(lado_a.terminou() && lado_v.terminou());

        let prova_do_visitante = lado_v.prova(&visitante.identidade);
        let prova_do_anfitriao = lado_a.prova(&anfitriao.identidade);

        // O visitante também autentica o anfitrião — a sessão é simétrica.
        let sessao_do_anfitriao = crate::infrastructure::sync_transport::autenticar(
            &prova_do_anfitriao,
            &lado_v.handshake.hash_do_handshake(),
        )
        .map_err(|_| FalhaDePareamento::IdentidadeNaoProvada)?;

        let pedido = lado_a.concluir(convites, &prova_do_visitante)?;
        Ok((pedido, sessao_do_anfitriao))
    }

    #[test]
    fn o_qr_leva_e_traz_o_convite_sem_perder_nada() {
        let mut convites = Convites::default();
        let convite = convites.emitir("192.168.0.10:7777");
        let texto = convite.para_qr();
        assert_eq!(Convite::ler_qr(&texto).expect("ler"), convite);
    }

    /// GATE: o convite **não carrega identidade**.
    ///
    /// Se carregasse, a tentação seguinte seria "conectou e disse que é o ABC
    /// do QR, então é o ABC" — a confiança declarativa que a etapa 8 eliminou.
    #[test]
    fn o_convite_nao_carrega_device_id() {
        let anfitriao = Ponta::nova();
        let mut convites = Convites::default();
        let convite = convites.emitir("192.168.0.10:7777");
        let texto = convite.para_qr();

        assert!(
            !texto.contains(anfitriao.identidade.device_id()),
            "o QR vazou a identidade de quem convidou: {texto}"
        );
        assert!(
            !texto.contains(&anfitriao.identidade.public_base32()),
            "o QR vazou a chave pública de quem convidou"
        );
    }

    /// E a identidade que sai do pareamento vem da **sessão**, não do QR.
    #[test]
    fn a_identidade_admitida_vem_da_sessao_e_nao_do_qr() {
        let anfitriao = Ponta::nova();
        let visitante = Ponta::nova();
        let mut convites = Convites::default();
        let convite = convites.emitir("endereco");

        let (pedido, sessao_do_anfitriao) =
            parear(&mut convites, &convite, &anfitriao, &visitante).expect("parear");

        assert_eq!(pedido.sessao.device_id(), visitante.identidade.device_id());
        assert_eq!(
            sessao_do_anfitriao.device_id(),
            anfitriao.identidade.device_id(),
            "o visitante também precisa saber com quem falou"
        );
        assert_eq!(pedido.convite_id, convite.id);
    }

    // ── os seis casos que o autor pediu ────────────────────────────────────

    /// 1. Expirado.
    #[test]
    fn convite_expirado_e_recusado_antes_de_qualquer_criptografia() {
        let anfitriao = Ponta::nova();
        let visitante = Ponta::nova();
        let mut convites = Convites::default();
        let convite = convites.emitir("endereco");
        convites.envelhecer(&convite.id, VALIDADE + Duration::from_secs(1));

        assert_eq!(
            parear(&mut convites, &convite, &anfitriao, &visitante).err(),
            Some(FalhaDePareamento::ConviteExpirado)
        );
    }

    /// 2. Consumido — a foto do QU tirada três dias antes não vale.
    #[test]
    fn convite_so_vale_uma_vez() {
        let anfitriao = Ponta::nova();
        let primeiro = Ponta::nova();
        let segundo = Ponta::nova();
        let mut convites = Convites::default();
        let convite = convites.emitir("endereco");

        parear(&mut convites, &convite, &anfitriao, &primeiro).expect("primeiro pareamento");

        assert_eq!(
            parear(&mut convites, &convite, &anfitriao, &segundo).err(),
            Some(FalhaDePareamento::ConviteConsumido),
            "o mesmo convite pareou dois aparelhos"
        );
    }

    /// 3. Segredo errado — e ele falha por **criptografia**, não por
    ///    comparação de string: o segredo é o `psk` do `XXpsk0`.
    #[test]
    fn segredo_errado_nao_fecha_o_handshake() {
        let anfitriao = Ponta::nova();
        let visitante = Ponta::nova();
        let mut convites = Convites::default();
        let convite = convites.emitir("endereco");

        let mut outro_segredo = [0_u8; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut outro_segredo);
        let falsificado = Convite {
            segredo: base32(&outro_segredo),
            ..convite.clone()
        };

        assert_eq!(
            parear(&mut convites, &falsificado, &anfitriao, &visitante).err(),
            Some(FalhaDePareamento::HandshakeRecusado)
        );

        // E o convite legítimo **não** foi queimado por causa da tentativa.
        parear(&mut convites, &convite, &anfitriao, &visitante)
            .expect("uma tentativa malfeita não pode consumir o convite");
    }

    /// 4. Replay: as mensagens de um pareamento não valem em outro.
    #[test]
    fn mensagens_de_um_pareamento_nao_valem_em_outro() {
        let anfitriao = Ponta::nova();
        let visitante = Ponta::nova();
        let mut convites = Convites::default();

        let primeiro = convites.emitir("endereco");
        let mut v1 = Pareamento::visitante(&primeiro, &visitante.estatica).expect("visitante");
        let m1_gravada = v1.escrever(&[]).expect("primeira mensagem");

        // Outro convite, e a mensagem gravada é reapresentada.
        let segundo = convites.emitir("endereco");
        let mut a2 =
            Pareamento::anfitriao(&convites, &segundo.id, &anfitriao.estatica).expect("anfitriao");

        assert!(
            a2.ler(&m1_gravada).is_err(),
            "uma mensagem de outro pareamento foi aceita"
        );
    }

    /// 5. QR alterado.
    #[test]
    fn qr_alterado_e_recusado_na_leitura() {
        let mut convites = Convites::default();
        let convite = convites.emitir("endereco");
        let texto = convite.para_qr();

        for adulterado in [
            texto.replace("narrahub-pair", "outra-coisa"),
            texto.replace(&format!(":{}:", VERSAO_DO_CONVITE), ":99:"),
            texto.replace("narrahub-pair", "narrahub-pai"),
            // Segredo truncado: perde os 32 bytes que o `psk` exige.
            texto.replace(&convite.segredo, &convite.segredo[..10]),
        ] {
            assert!(
                Convite::ler_qr(&adulterado).is_err(),
                "aceitou um QR adulterado: {adulterado}"
            );
        }
    }

    /// Segredo curto demais é recusado na leitura.
    ///
    /// O `psk` do Noise pressupõe 32 bytes de entropia. Um convite com menos
    /// entraria no handshake com aparência de forte.
    #[test]
    fn segredo_com_entropia_insuficiente_e_recusado() {
        let curto = format!("narrahub-pair:{VERSAO_DO_CONVITE}:ID:AAAA:endereco");
        assert_eq!(
            Convite::ler_qr(&curto).err(),
            Some(FalhaDePareamento::ConviteIlegivel)
        );
    }

    /// 6. Sessão correta, convite diferente.
    ///
    /// O `invitation_id` entra como prólogo do Noise: dois lados com o segredo
    /// certo mas ids diferentes **não fecham**.
    #[test]
    fn segredo_certo_com_convite_diferente_nao_fecha() {
        let anfitriao = Ponta::nova();
        let visitante = Ponta::nova();
        let mut convites = Convites::default();

        let a = convites.emitir("endereco");
        let b = convites.emitir("endereco");

        // O visitante usa o segredo do convite A sob o id do convite B.
        let misturado = Convite {
            id: b.id.clone(),
            segredo: a.segredo.clone(),
            ..a.clone()
        };

        let mut lado_a =
            Pareamento::anfitriao(&convites, &b.id, &anfitriao.estatica).expect("anfitriao com B");
        let mut lado_v = Pareamento::visitante(&misturado, &visitante.estatica).expect("visitante");

        let m1 = lado_v.escrever(&[]).expect("primeira");
        assert!(
            lado_a.ler(&m1).is_err(),
            "o handshake fechou com segredo de um convite e id de outro"
        );
    }

    #[test]
    fn convite_desconhecido_e_recusado() {
        let anfitriao = Ponta::nova();
        let convites = Convites::default();
        assert_eq!(
            Pareamento::anfitriao(&convites, "ID-QUE-NAO-EXISTE", &anfitriao.estatica).err(),
            Some(FalhaDePareamento::ConviteDesconhecido)
        );
    }

    /// Dois convites emitidos em seguida não compartilham segredo.
    #[test]
    fn cada_convite_tem_segredo_proprio() {
        let mut convites = Convites::default();
        let a = convites.emitir("endereco");
        let b = convites.emitir("endereco");
        assert_ne!(a.segredo, b.segredo);
        assert_ne!(a.id, b.id);
    }
}
