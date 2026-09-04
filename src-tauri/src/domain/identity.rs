//! Sync V2 — identidade do dispositivo e assinatura de eventos.
//!
//! ADR 0009, seções 5 e 10; etapa 2.5 da seção 23. Camada pura: não conhece
//! `rusqlite`, `tauri` nem sistema de arquivos. Onde a chave **mora** é
//! assunto da infraestrutura; o que ela **significa** é assunto daqui.
//!
//! ## Por que esta etapa vem antes da 3
//!
//! A etapa 2 grava `signature = ''` porque não havia chave. A etapa 3 liga o
//! log às escritas reais, e a partir dela nascem eventos de verdade.
//! `sync_events` é append-only por trigger:
//!
//! ```text
//! evento nascido sem assinatura  →  não pode ser assinado depois
//!                                →  começo do log inverificável para sempre
//!                                →  exceção eterna no caminho de verificação
//! ```
//!
//! Uma exceção no caminho que decide se um evento é autêntico é o pior lugar
//! possível para ter uma.

use crate::domain::sync::EventEnvelope;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Separador de domínio da assinatura. Diferente do usado em `new_rev`: os
/// dois hashes cobrem coisas diferentes, e um prefixo comum permitiria que
/// bytes preparados para um valessem no outro.
const SIGNATURE_DOMAIN: &[u8] = b"narrahub.sync.v2.envelope\x00";

/// Quantos bytes do SHA-256 da chave pública viram o `device_id`.
///
/// 20 bytes = 160 bits, que em base32 dão 32 caracteres legíveis. Truncar
/// hash é seguro para identificador; o que autentica é a assinatura, não o
/// tamanho do apelido.
const FINGERPRINT_BYTES: usize = 20;

const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// A identidade deste dispositivo. A parte privada nunca sai daqui para o
/// banco (ADR 0009 §5): o banco vai para backup, e backup vai para pendrive.
pub struct DeviceIdentity {
    signing_key: SigningKey,
    device_id: String,
}

/// `Debug` escrito à mão, e não derivado.
///
/// `#[derive(Debug)]` imprimiria a chave privada em qualquer mensagem de
/// pânico, log ou `dbg!` — e uma chave que vaza num log deixa de ser privada
/// mesmo estando fora do backup e fora do banco.
impl std::fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceIdentity")
            .field("device_id", &self.device_id)
            .field("signing_key", &"<redigida>")
            .finish()
    }
}

impl DeviceIdentity {
    /// Gera uma identidade nova. Chamado uma vez, no primeiro boot.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        Self::from_signing_key(signing_key)
    }

    pub fn from_secret_bytes(secret: [u8; 32]) -> Self {
        Self::from_signing_key(SigningKey::from_bytes(&secret))
    }

    fn from_signing_key(signing_key: SigningKey) -> Self {
        let device_id = fingerprint(&signing_key.verifying_key());
        Self {
            signing_key,
            device_id,
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// A pública em base32, do jeito que vai para `sync_devices`.
    pub fn public_base32(&self) -> String {
        base32(&self.public_bytes())
    }

    pub fn sign(&self, envelope: &EventEnvelope) -> String {
        base32(&self.signing_key.sign(&canonical_bytes(envelope)).to_bytes())
    }
}

/// `device_id` a partir da chave pública: SHA-256 truncado, em base32.
pub fn fingerprint(public: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public.as_bytes());
    base32(&hasher.finalize()[..FINGERPRINT_BYTES])
}

/// Representação canônica do envelope, **menos a assinatura**.
///
/// Cada campo entra precedido do seu tamanho, na mesma ordem sempre. Sem
/// isso, dois serializadores produzem bytes diferentes para o mesmo evento e
/// a assinatura falha por motivo errado — o que é pior que não assinar,
/// porque parece ataque.
///
/// **O `payload` entra como bytes opacos.** Ele não é reserializado aqui nem
/// em lugar nenhum: reordenar chaves ou normalizar espaço mudaria os bytes do
/// mesmo conteúdo, e o outro aparelho recusaria o evento como adulterado.
pub fn canonical_bytes(envelope: &EventEnvelope) -> Vec<u8> {
    let mut saida = Vec::with_capacity(256);
    saida.extend_from_slice(SIGNATURE_DOMAIN);
    let seq = envelope.seq.to_string();
    for campo in [
        envelope.event_id.as_str(),
        envelope.device_id.as_str(),
        seq.as_str(),
        envelope.universe_id.as_str(),
        envelope.aggregate_type.as_str(),
        envelope.aggregate_id.as_str(),
        envelope.operation.as_str(),
        envelope.payload.as_str(),
        envelope.base_rev.as_str(),
        envelope.new_rev.as_str(),
    ] {
        saida.extend_from_slice(&(campo.len() as u64).to_be_bytes());
        saida.extend_from_slice(campo.as_bytes());
    }
    saida
}

/// Verifica a assinatura de um envelope contra a pública da origem.
///
/// Etapa 7 vai decidir **de quem** aceitar; esta função só responde se os
/// bytes conferem. Separar as duas perguntas é o que permite testar a
/// criptografia sem o roster e o roster sem a criptografia.
pub fn verify(envelope: &EventEnvelope, public_base32: &str) -> bool {
    let Some(publica) = decode_base32(public_base32) else {
        return false;
    };
    let Ok(publica) = <[u8; 32]>::try_from(publica.as_slice()) else {
        return false;
    };
    let Ok(publica) = VerifyingKey::from_bytes(&publica) else {
        return false;
    };
    let Some(assinatura) = decode_base32(&envelope.signature) else {
        return false;
    };
    let Ok(assinatura) = <[u8; 64]>::try_from(assinatura.as_slice()) else {
        return false;
    };
    publica
        .verify(
            &canonical_bytes(envelope),
            &Signature::from_bytes(&assinatura),
        )
        .is_ok()
}

/// Base32 RFC 4648 sem preenchimento.
///
/// Escrito aqui em vez de virar dependência: são vinte linhas, o formato é
/// fixo, e o alfabeto sem `0`, `1` e `8` é o que faz um fingerprint sobreviver
/// a alguém lendo em voz alta.
pub fn base32(bytes: &[u8]) -> String {
    let mut saida = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut acumulador: u32 = 0;
    let mut bits: u32 = 0;
    for byte in bytes {
        acumulador = (acumulador << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            saida.push(BASE32[((acumulador >> bits) & 0b1_1111) as usize] as char);
        }
    }
    if bits > 0 {
        saida.push(BASE32[((acumulador << (5 - bits)) & 0b1_1111) as usize] as char);
    }
    saida
}

pub fn decode_base32(texto: &str) -> Option<Vec<u8>> {
    let mut saida = Vec::with_capacity(texto.len() * 5 / 8);
    let mut acumulador: u32 = 0;
    let mut bits: u32 = 0;
    for caractere in texto.bytes() {
        let valor = BASE32.iter().position(|alvo| *alvo == caractere)? as u32;
        acumulador = (acumulador << 5) | valor;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            saida.push(((acumulador >> bits) & 0xFF) as u8);
        }
    }
    Some(saida)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sync::Operation;

    /// Nome para o par "campo, como adulterá-lo" — sem ele o tipo aparece por
    /// extenso e não se lê.
    type Adulteracao = (&'static str, Box<dyn Fn(&mut EventEnvelope)>);

    fn envelope() -> EventEnvelope {
        EventEnvelope {
            event_id: "ev-1".into(),
            device_id: "dev-1".into(),
            seq: 7,
            universe_id: "u1".into(),
            aggregate_type: "chapter".into(),
            aggregate_id: "cap-1".into(),
            operation: Operation::Upsert,
            payload: r#"{"zeta":1,  "alfa" : 2.50}"#.into(),
            base_rev: "r0".into(),
            new_rev: "r1".into(),
            signature: String::new(),
        }
    }

    #[test]
    fn o_device_id_deriva_da_chave_publica() {
        let identidade = DeviceIdentity::generate();
        let outra = DeviceIdentity::generate();
        assert_ne!(identidade.device_id(), outra.device_id());
        assert_eq!(identidade.device_id().len(), 32);

        // Determinístico: a mesma chave produz sempre o mesmo id. É o que
        // permite reconhecer o aparelho depois de reiniciar.
        let mesma = DeviceIdentity::from_secret_bytes(identidade.secret_bytes());
        assert_eq!(mesma.device_id(), identidade.device_id());
    }

    #[test]
    fn assinatura_valida_confere() {
        let identidade = DeviceIdentity::generate();
        let mut envelope = envelope();
        envelope.signature = identidade.sign(&envelope);
        assert!(verify(&envelope, &identidade.public_base32()));
    }

    /// Cada campo do envelope está coberto. Um campo de fora da assinatura
    /// seria um campo que um relay pode alterar sem que ninguém perceba — e
    /// mudar `device_id` ou `seq` em trânsito reescreve a origem do evento.
    #[test]
    fn alterar_qualquer_campo_invalida_a_assinatura() {
        let identidade = DeviceIdentity::generate();
        let publica = identidade.public_base32();
        let original = {
            let mut envelope = envelope();
            envelope.signature = identidade.sign(&envelope);
            envelope
        };

        let mutacoes: Vec<Adulteracao> = vec![
            (
                "event_id",
                Box::new(|e: &mut EventEnvelope| e.event_id = "ev-2".into()),
            ),
            (
                "device_id",
                Box::new(|e: &mut EventEnvelope| e.device_id = "dev-2".into()),
            ),
            ("seq", Box::new(|e: &mut EventEnvelope| e.seq = 8)),
            (
                "universe_id",
                Box::new(|e: &mut EventEnvelope| e.universe_id = "u2".into()),
            ),
            (
                "aggregate_type",
                Box::new(|e: &mut EventEnvelope| e.aggregate_type = "entity".into()),
            ),
            (
                "aggregate_id",
                Box::new(|e: &mut EventEnvelope| e.aggregate_id = "cap-2".into()),
            ),
            (
                "operation",
                Box::new(|e: &mut EventEnvelope| e.operation = Operation::Delete),
            ),
            (
                "payload",
                Box::new(|e: &mut EventEnvelope| e.payload = "{}".into()),
            ),
            (
                "base_rev",
                Box::new(|e: &mut EventEnvelope| e.base_rev = "rX".into()),
            ),
            (
                "new_rev",
                Box::new(|e: &mut EventEnvelope| e.new_rev = "rY".into()),
            ),
        ];

        for (campo, mutar) in mutacoes {
            let mut adulterado = original.clone();
            mutar(&mut adulterado);
            assert!(
                !verify(&adulterado, &publica),
                "mexer em {campo} passou pela verificação: o campo está fora da assinatura"
            );
        }
    }

    #[test]
    fn assinatura_de_outra_chave_nao_confere() {
        let dono = DeviceIdentity::generate();
        let impostor = DeviceIdentity::generate();
        let mut envelope = envelope();
        envelope.signature = impostor.sign(&envelope);
        assert!(!verify(&envelope, &dono.public_base32()));
    }

    /// A canonicalização não pode ambiguar por concatenação: `"ab" + "c"` não
    /// pode produzir os mesmos bytes que `"a" + "bc"`.
    #[test]
    fn campos_vizinhos_nao_se_confundem() {
        let mut um = envelope();
        um.aggregate_type = "ab".into();
        um.aggregate_id = "c".into();
        let mut outro = envelope();
        outro.aggregate_type = "a".into();
        outro.aggregate_id = "bc".into();
        assert_ne!(canonical_bytes(&um), canonical_bytes(&outro));
    }

    /// Assinatura mal formada é recusa, não pânico. Ela chega pela rede.
    #[test]
    fn entrada_invalida_e_recusada_sem_panico() {
        let identidade = DeviceIdentity::generate();
        let publica = identidade.public_base32();

        for lixo in ["", "!!!!", "AAAA", "aaaa"] {
            let mut envelope = envelope();
            envelope.signature = lixo.into();
            assert!(!verify(&envelope, &publica));
        }

        let mut envelope = envelope();
        envelope.signature = identidade.sign(&envelope);
        for chave_ruim in ["", "ZZZZ", "0189"] {
            assert!(!verify(&envelope, chave_ruim));
        }
    }

    #[test]
    fn base32_fecha_o_ciclo() {
        for tamanho in [0_usize, 1, 5, 20, 32, 64] {
            let bytes: Vec<u8> = (0..tamanho)
                .map(|indice| (indice * 7 % 256) as u8)
                .collect();
            assert_eq!(
                decode_base32(&base32(&bytes)).as_deref(),
                Some(bytes.as_slice())
            );
        }
    }
}
