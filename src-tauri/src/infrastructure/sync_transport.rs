//! Sync V2 — transporte Noise e vínculo com a identidade (ADR 0009 §7, etapa 8).
//!
//! ## O que esta etapa precisa provar, e não é "o canal é cifrado"
//!
//! Cifrar é o fácil. O difícil, e o que decide se o resto do Sync V2 vale
//! alguma coisa, é isto:
//!
//! > A identidade autenticada pelo Noise tem que ser **a mesma** que o Sync V2
//! > usa para autorizar. Nenhum `device_id` declarado pela camada de aplicação
//! > vale como prova de identidade.
//!
//! Sem esse vínculo, todo o trabalho das etapas 2.5 e 7 fica pendurado num
//! `&str` que qualquer peer pode escrever no pacote.
//!
//! ## Por que o Noise sozinho não basta
//!
//! O Noise autentica a **chave estática X25519** do outro lado. O Sync V2
//! autoriza pela identidade **Ed25519**, que é de onde o `device_id` deriva.
//! São duas chaves diferentes, de propósito (ADR §5): uma protege o canal
//! desta conexão, a outra prova quem originou um evento — inclusive um evento
//! retransmitido por terceiros, semanas depois.
//!
//! ```text
//! Noise diz:      "quem está do outro lado controla a X25519 estática K"
//! Sync V2 quer:   "quem está do outro lado É o device_id D"
//!
//! e nada, sozinho, liga K a D.
//! ```
//!
//! Um peer com uma X25519 legítima poderia anunciar qualquer `device_id` — e
//! o `introduzir_dispositivo` da etapa 7, que hoje recebe um `&str`, aceitaria.
//!
//! ## O vínculo: assinar o hash do handshake
//!
//! Depois do handshake, cada lado tem o mesmo `h` — o hash do transcript, que
//! resume tudo que foi trocado, inclusive as chaves estáticas. Cada lado
//! assina esse `h` com a **Ed25519** e manda a assinatura junto com a pública:
//!
//! ```text
//! prova = Ed25519.sign(h)
//!
//! quem verifica sabe:
//!   1. a assinatura é válida para aquela Ed25519      → controla a identidade
//!   2. o `h` é o desta conexão e de nenhuma outra     → não é replay
//!   3. o `h` inclui a X25519 estática do Noise        → liga as duas chaves
//! ```
//!
//! O ponto 2 é o que impede pegar uma prova capturada de outra sessão: `h`
//! depende dos efêmeros, que mudam a cada handshake.
//!
//! ## O que sai daqui
//!
//! Uma [`SessaoAutenticada`], cujo `device_id` **não pode ser escolhido por
//! quem fala**. É o único tipo que a etapa 9 vai aceitar como autoridade para
//! admitir alguém no roster.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::identity::{base32, decode_base32, fingerprint, DeviceIdentity};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// O padrão Noise do pareamento e das sessões seguintes.
///
/// `XX` porque os dois lados descobrem a estática do outro durante o
/// handshake — que é o que permite a primeira conexão entre aparelhos que
/// ainda não se conhecem. A autenticação de quem é quem **não** vem daqui:
/// vem da prova Ed25519 sobre o `h`.
const PADRAO: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Uma sessão cuja identidade do outro lado foi **provada**, não declarada.
///
/// O `device_id` sai do fingerprint da Ed25519 que assinou o hash do
/// handshake desta conexão. Não existe caminho para construí-la a partir de um
/// `device_id` recebido pela rede — é essa ausência que faz o tipo valer.
#[derive(Debug, Clone)]
pub struct SessaoAutenticada {
    device_id: String,
    ed25519_public: String,
}

impl SessaoAutenticada {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn ed25519_public(&self) -> &str {
        &self.ed25519_public
    }
}

impl SessaoAutenticada {
    /// A sessão que representa **este** aparelho agindo por si.
    ///
    /// Não passa por handshake porque não há rede no meio: a autoridade vem de
    /// ter a chave privada em mãos, que é o mesmo que a prova do handshake
    /// demonstra remotamente. É o caminho do pareamento local e do arranque.
    ///
    /// Deliberadamente **não** aceita um `device_id` solto — só uma
    /// `DeviceIdentity`, que só existe se a privada foi carregada do disco.
    pub fn deste_aparelho(identidade: &DeviceIdentity) -> Self {
        Self {
            device_id: identidade.device_id().to_string(),
            ed25519_public: identidade.public_base32(),
        }
    }
}

/// A prova que cada lado manda depois do handshake.
#[derive(Debug, Clone)]
pub struct ProvaDeIdentidade {
    pub ed25519_public: String,
    /// Ed25519 sobre o hash do handshake desta conexão.
    pub assinatura: String,
}

/// Produz a prova para esta conexão.
pub fn provar_identidade(
    identidade: &DeviceIdentity,
    hash_do_handshake: &[u8],
) -> ProvaDeIdentidade {
    ProvaDeIdentidade {
        ed25519_public: identidade.public_base32(),
        assinatura: base32(&identidade.assinar_bytes(hash_do_handshake)),
    }
}

/// Verifica a prova do outro lado e devolve a sessão autenticada.
///
/// Falha fechada em tudo: chave ilegível, assinatura ilegível, assinatura que
/// não confere. Nenhum desses casos produz uma sessão "meio autenticada".
pub fn autenticar(
    prova: &ProvaDeIdentidade,
    hash_do_handshake: &[u8],
) -> Result<SessaoAutenticada, FalhaDeAutenticacao> {
    let bytes = decode_base32(&prova.ed25519_public)
        .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
        .ok_or(FalhaDeAutenticacao::ChaveIlegivel)?;
    let publica =
        VerifyingKey::from_bytes(&bytes).map_err(|_| FalhaDeAutenticacao::ChaveIlegivel)?;

    let assinatura = decode_base32(&prova.assinatura)
        .and_then(|bytes| <[u8; 64]>::try_from(bytes.as_slice()).ok())
        .ok_or(FalhaDeAutenticacao::AssinaturaIlegivel)?;

    publica
        .verify(
            &crate::domain::identity::bytes_do_handshake(hash_do_handshake),
            &Signature::from_bytes(&assinatura),
        )
        .map_err(|_| FalhaDeAutenticacao::ProvaInvalida)?;

    Ok(SessaoAutenticada {
        device_id: fingerprint(&publica),
        ed25519_public: prova.ed25519_public.clone(),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeAutenticacao {
    ChaveIlegivel,
    AssinaturaIlegivel,
    /// A assinatura não cobre o hash **desta** conexão. Cobre uma prova
    /// capturada de outra sessão, ou não cobre nada.
    ProvaInvalida,
}

impl std::fmt::Display for FalhaDeAutenticacao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let texto = match self {
            FalhaDeAutenticacao::ChaveIlegivel => "a chave de identidade do peer está ilegível",
            FalhaDeAutenticacao::AssinaturaIlegivel => "a prova de identidade está ilegível",
            FalhaDeAutenticacao::ProvaInvalida => {
                "a prova de identidade não corresponde a esta conexão"
            }
        };
        f.write_str(texto)
    }
}

/// Um lado do handshake Noise.
pub struct Handshake {
    estado: snow::HandshakeState,
}

/// Uma sessão de transporte pronta, depois do handshake.
pub struct Transporte {
    estado: snow::TransportState,
}

impl Handshake {
    /// Quem abre a porta. Papel de **transporte** apenas — não faz dele
    /// servidor, nem autoritativo, nem dono do dado (ADR §2).
    pub fn ouvinte(estatica: &[u8; 32]) -> DatabaseCommandResult<Self> {
        Self::novo(estatica, false)
    }

    /// Quem conecta. Mesma coisa, do outro lado.
    pub fn conector(estatica: &[u8; 32]) -> DatabaseCommandResult<Self> {
        Self::novo(estatica, true)
    }

    fn novo(estatica: &[u8; 32], inicia: bool) -> DatabaseCommandResult<Self> {
        let construtor = snow::Builder::new(
            PADRAO
                .parse()
                .map_err(|_| DatabaseCommandError::storage("padrão Noise inválido"))?,
        )
        .local_private_key(estatica);

        let estado = if inicia {
            construtor.build_initiator()
        } else {
            construtor.build_responder()
        }
        .map_err(|error| {
            DatabaseCommandError::storage(format!("não foi possível iniciar o Noise: {error}"))
        })?;

        Ok(Self { estado })
    }

    pub fn escrever(&mut self, carga: &[u8]) -> DatabaseCommandResult<Vec<u8>> {
        let mut saida = vec![0_u8; 65535];
        let tamanho = self
            .estado
            .write_message(carga, &mut saida)
            .map_err(|error| DatabaseCommandError::storage(format!("handshake: {error}")))?;
        saida.truncate(tamanho);
        Ok(saida)
    }

    pub fn ler(&mut self, mensagem: &[u8]) -> DatabaseCommandResult<Vec<u8>> {
        let mut saida = vec![0_u8; 65535];
        let tamanho = self
            .estado
            .read_message(mensagem, &mut saida)
            .map_err(|error| DatabaseCommandError::storage(format!("handshake: {error}")))?;
        saida.truncate(tamanho);
        Ok(saida)
    }

    pub fn terminou(&self) -> bool {
        self.estado.is_handshake_finished()
    }

    /// O hash do transcript. É o que a prova de identidade assina.
    pub fn hash_do_handshake(&self) -> Vec<u8> {
        self.estado.get_handshake_hash().to_vec()
    }

    pub fn concluir(self) -> DatabaseCommandResult<Transporte> {
        let estado = self.estado.into_transport_mode().map_err(|error| {
            DatabaseCommandError::storage(format!("não foi possível concluir o handshake: {error}"))
        })?;
        Ok(Transporte { estado })
    }
}

impl Transporte {
    pub fn cifrar(&mut self, carga: &[u8]) -> DatabaseCommandResult<Vec<u8>> {
        let mut saida = vec![0_u8; carga.len() + 64];
        let tamanho = self
            .estado
            .write_message(carga, &mut saida)
            .map_err(|error| DatabaseCommandError::storage(format!("cifrar: {error}")))?;
        saida.truncate(tamanho);
        Ok(saida)
    }

    pub fn decifrar(&mut self, mensagem: &[u8]) -> DatabaseCommandResult<Vec<u8>> {
        let mut saida = vec![0_u8; mensagem.len()];
        let tamanho = self
            .estado
            .read_message(mensagem, &mut saida)
            .map_err(|error| DatabaseCommandError::storage(format!("decifrar: {error}")))?;
        saida.truncate(tamanho);
        Ok(saida)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um aparelho no transporte: identidade Ed25519 e estática X25519.
    ///
    /// As duas são geradas independentemente, como no produto — é justamente
    /// por serem independentes que o vínculo entre elas precisa ser provado.
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

    /// Roda o `XX` inteiro e devolve os dois lados prontos, com o hash.
    fn apertar_maos(a: &Ponta, b: &Ponta) -> (Transporte, Transporte, Vec<u8>, Vec<u8>) {
        let mut conector = Handshake::conector(&a.estatica).expect("conector");
        let mut ouvinte = Handshake::ouvinte(&b.estatica).expect("ouvinte");

        let m1 = conector.escrever(&[]).expect("e -> ");
        ouvinte.ler(&m1).expect("ler e");
        let m2 = ouvinte.escrever(&[]).expect("e, ee, s, es");
        conector.ler(&m2).expect("ler resposta");
        let m3 = conector.escrever(&[]).expect("s, se");
        ouvinte.ler(&m3).expect("ler final");

        assert!(conector.terminou() && ouvinte.terminou());
        let hash_a = conector.hash_do_handshake();
        let hash_b = ouvinte.hash_do_handshake();
        (
            conector.concluir().expect("transporte A"),
            ouvinte.concluir().expect("transporte B"),
            hash_a,
            hash_b,
        )
    }

    #[test]
    fn o_canal_cifra_e_decifra_nos_dois_sentidos() {
        let a = Ponta::nova();
        let b = Ponta::nova();
        let (mut ta, mut tb, _, _) = apertar_maos(&a, &b);

        let cifrado = ta.cifrar(b"evento do capitulo").expect("cifrar");
        assert_ne!(&cifrado[..], b"evento do capitulo", "saiu em claro");
        assert_eq!(
            tb.decifrar(&cifrado).expect("decifrar"),
            b"evento do capitulo"
        );

        let volta = tb.cifrar(b"resposta").expect("cifrar");
        assert_eq!(ta.decifrar(&volta).expect("decifrar"), b"resposta");
    }

    /// Os dois lados chegam ao **mesmo** hash de handshake.
    ///
    /// É o que torna possível assinar "esta conexão": se cada lado tivesse um
    /// hash diferente, a prova de um nunca verificaria no outro.
    #[test]
    fn os_dois_lados_veem_o_mesmo_hash_de_handshake() {
        let a = Ponta::nova();
        let b = Ponta::nova();
        let (_, _, hash_a, hash_b) = apertar_maos(&a, &b);
        assert_eq!(hash_a, hash_b);
        assert!(!hash_a.is_empty());
    }

    /// GATE DA ETAPA 8: a identidade que o Noise autentica é a que o Sync V2
    /// usa para autorizar.
    ///
    /// O `device_id` da sessão **não é declarado por ninguém** — ele sai do
    /// fingerprint da chave que assinou o hash desta conexão.
    #[test]
    fn a_identidade_da_sessao_e_provada_e_bate_com_o_device_id() {
        let a = Ponta::nova();
        let b = Ponta::nova();
        let (_, _, hash_a, hash_b) = apertar_maos(&a, &b);

        let prova_de_a = provar_identidade(&a.identidade, &hash_a);
        let sessao_vista_por_b = autenticar(&prova_de_a, &hash_b).expect("autenticar A");

        assert_eq!(
            sessao_vista_por_b.device_id(),
            a.identidade.device_id(),
            "a sessão autenticou um device_id diferente do dono da chave"
        );
        assert_eq!(
            sessao_vista_por_b.ed25519_public(),
            a.identidade.public_base32()
        );
    }

    /// O caso central: **anunciar um `device_id` não é provar identidade**.
    ///
    /// Um peer com uma X25519 legítima — o Noise dele funciona — tenta se
    /// passar por outro aparelho. Sem a prova sobre o `h`, o
    /// `introduzir_dispositivo` da etapa 7 aceitaria o `&str` que ele mandasse.
    #[test]
    fn nao_da_para_se_passar_por_outro_device_id() {
        let impostor = Ponta::nova();
        let alvo = DeviceIdentity::generate();
        let b = Ponta::nova();

        let (_, _, hash_impostor, hash_b) = apertar_maos(&impostor, &b);

        // O impostor manda a chave do alvo, mas assina com a própria — é o
        // melhor que ele consegue fazer sem ter a privada do alvo.
        let prova_forjada = ProvaDeIdentidade {
            ed25519_public: alvo.public_base32(),
            assinatura: base32(&impostor.identidade.assinar_bytes(&hash_impostor)),
        };

        let resultado = autenticar(&prova_forjada, &hash_b);
        assert_eq!(
            resultado.err(),
            Some(FalhaDeAutenticacao::ProvaInvalida),
            "um peer conseguiu se apresentar como outro device_id"
        );
    }

    /// Prova capturada de outra sessão não vale nesta.
    ///
    /// O `h` depende dos efêmeros, que mudam a cada handshake. Sem isso, quem
    /// gravasse o tráfego de uma sessão poderia se autenticar como aquele
    /// aparelho para sempre.
    #[test]
    fn prova_de_outra_sessao_nao_vale_aqui() {
        let a = Ponta::nova();
        let b = Ponta::nova();

        let (_, _, hash_primeira, _) = apertar_maos(&a, &b);
        let prova_antiga = provar_identidade(&a.identidade, &hash_primeira);

        // Segunda conexão, entre os mesmos aparelhos.
        let (_, _, _, hash_segunda) = apertar_maos(&a, &b);
        assert_ne!(
            hash_primeira, hash_segunda,
            "o hash repetiu entre sessões: o replay passaria a ser possível"
        );

        assert_eq!(
            autenticar(&prova_antiga, &hash_segunda).err(),
            Some(FalhaDeAutenticacao::ProvaInvalida)
        );
    }

    /// E a assinatura do handshake não é aceita como assinatura de evento.
    ///
    /// Separadores de domínio distintos. Sem eles, uma prova de handshake
    /// capturada poderia ser oferecida como assinatura de um evento forjado.
    #[test]
    fn a_prova_do_handshake_nao_serve_como_assinatura_de_evento() {
        use crate::domain::identity::verify;
        use crate::domain::sync::{AggregateRef, EventEnvelope, Operation};

        let a = Ponta::nova();
        let b = Ponta::nova();
        let (_, _, hash, _) = apertar_maos(&a, &b);
        let prova = provar_identidade(&a.identidade, &hash);

        let envelope = EventEnvelope {
            event_id: "ev-1".into(),
            device_id: a.identidade.device_id().into(),
            seq: 1,
            universe_id: "u1".into(),
            aggregate_type: "chapter".into(),
            aggregate_id: "cap-1".into(),
            operation: Operation::Upsert,
            payload: "{}".into(),
            base_rev: String::new(),
            new_rev: "rev".into(),
            // A assinatura do handshake, apresentada como se fosse do evento.
            signature: prova.assinatura.clone(),
        };
        let _ = AggregateRef::new("chapter", "cap-1");

        assert!(
            !verify(&envelope, &a.identidade.public_base32()),
            "uma prova de handshake foi aceita como assinatura de evento"
        );
    }

    #[test]
    fn prova_ilegivel_falha_fechada_sem_panico() {
        let a = Ponta::nova();
        let b = Ponta::nova();
        let (_, _, hash, _) = apertar_maos(&a, &b);

        for chave in ["", "!!!", "AAAA"] {
            let prova = ProvaDeIdentidade {
                ed25519_public: chave.into(),
                assinatura: base32(&a.identidade.assinar_bytes(&hash)),
            };
            assert!(autenticar(&prova, &hash).is_err());
        }

        for assinatura in ["", "!!!", "AAAA"] {
            let prova = ProvaDeIdentidade {
                ed25519_public: a.identidade.public_base32(),
                assinatura: assinatura.into(),
            };
            assert!(autenticar(&prova, &hash).is_err());
        }
    }

    /// Mensagem adulterada em trânsito não decifra.
    #[test]
    fn mensagem_adulterada_nao_decifra() {
        let a = Ponta::nova();
        let b = Ponta::nova();
        let (mut ta, mut tb, _, _) = apertar_maos(&a, &b);

        let mut cifrado = ta.cifrar(b"conteudo importante").expect("cifrar");
        let ultimo = cifrado.len() - 1;
        cifrado[ultimo] ^= 0xFF;

        assert!(
            tb.decifrar(&cifrado).is_err(),
            "o ChaChaPoly aceitou uma mensagem adulterada"
        );
    }
}
