//! Sync V2 — pareamento por PIN, via PAKE (ADR 0009 §6.2, etapa 10).
//!
//! ## O problema que o PAKE resolve, e que um hash não resolve
//!
//! Um PIN de 8 dígitos tem ~26 bits de entropia. O `psk` do Noise pressupõe 32
//! bytes de entropia forte, e **passar o PIN por um hash não cria entropia** —
//! só espalha os mesmos 26 bits por 32 bytes.
//!
//! ```text
//! psk = hash(PIN)      →  atacante grava o handshake
//!                      →  testa os 10⁸ candidatos EM CASA
//!                      →  sem limite de tentativas do nosso lado
//! ```
//!
//! Com PAKE, não existe verificador offline. Cada palpite exige uma
//! **interação** com o outro aparelho — e aí o limite de três tentativas volta
//! a valer.
//!
//! ### O que o PAKE NÃO garante
//!
//! Ele não protege contra o vazamento do próprio segredo. Quem **sabe** o PIN
//! enquanto ele ainda vale e consegue falar com o aparelho **se autentica** —
//! é exatamente para isso que o PIN serve.
//!
//! A propriedade é mais estreita, e escrevê-la torta faria alguém concluir daqui
//! a seis meses que vazar o PIN é inofensivo:
//!
//! > Uma **captura do tráfego** não permite verificar palpites de PIN
//! > localmente. Toda tentativa útil contra o aparelho precisa ser online, e
//! > por isso é contável e limitável.
//!
//! ## Nada de PAKE caseiro
//!
//! Usamos a implementação de SPAKE2 do crate [`spake2`], que é a mesma que o
//! `magic-wormhole` usa — e o caso de uso é literalmente o mesmo: dois pares
//! com um código humano curto, sem servidor no meio.
//!
//! Escrever a própria construção aqui seria o oposto do que esta etapa
//! precisa. Criptografia caseira falha em silêncio: passa nos testes felizes e
//! quebra contra quem sabe o que está fazendo.
//!
//! ## Como a chave forte nasce
//!
//! ```text
//! PIN (26 bits)  ──SPAKE2──▶  32 bytes fortes  ──▶  psk do Noise XXpsk0
//! ```
//!
//! Depois disso o pareamento é o mesmo da etapa 9: `XXpsk0`, prova Ed25519
//! sobre o hash do handshake, e a identidade saindo da `SessaoAutenticada`.
//! **O PIN autoriza o encontro; ele não diz quem chegou** — mesma regra do QR.

use crate::domain::identity::base32;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::time::{Duration, Instant};

/// Quantos dígitos o código digitado tem.
///
/// Oito é o que cabe numa tela sem virar tortura de digitação. A segurança não
/// vem daqui — vem de o PAKE tornar cada palpite uma interação.
pub const DIGITOS: usize = 8;

/// Quantas tentativas antes de o código morrer.
///
/// Três. É o número que transforma "10⁸ palpites offline" em "três palpites
/// online", e é o que o ADR §6 exige.
pub const MAX_TENTATIVAS: u32 = 3;

/// Validade do código. Igual à do QR, e pelo mesmo motivo.
pub const VALIDADE: Duration = Duration::from_secs(180);

/// Rótulos das duas pontas. O SPAKE2 assimétrico (`A`/`B`) precisa que cada
/// lado saiba qual papel ocupa, senão as chaves não batem.
const IDENTIDADE_ANFITRIAO: &[u8] = b"narrahub.pair.host";
const IDENTIDADE_VISITANTE: &[u8] = b"narrahub.pair.guest";

#[derive(Debug, PartialEq, Eq)]
pub enum FalhaDeCodigo {
    /// O código digitado não tem o formato esperado.
    Malformado,
    CodigoDesconhecido,
    CodigoExpirado,
    /// Três erros. O código morre e o escritor gera outro.
    TentativasEsgotadas,
    /// O outro lado não chegou à mesma chave. Com PAKE, o motivo quase certo é
    /// PIN diferente — e descobrir isso **exigiu esta interação**, que é
    /// exatamente o ponto.
    NaoCombinou,
}

impl std::fmt::Display for FalhaDeCodigo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let texto = match self {
            FalhaDeCodigo::Malformado => "O código precisa ter oito dígitos.",
            FalhaDeCodigo::CodigoDesconhecido => {
                "Este código não está mais disponível. Gere um novo."
            }
            FalhaDeCodigo::CodigoExpirado => "Este código expirou. Gere um novo.",
            FalhaDeCodigo::TentativasEsgotadas => {
                "O código foi digitado errado três vezes e não vale mais. Gere um novo."
            }
            FalhaDeCodigo::NaoCombinou => {
                "Os dois aparelhos não chegaram ao mesmo código. Confira os dígitos."
            }
        };
        f.write_str(texto)
    }
}

/// Um código de pareamento aberto neste aparelho.
///
/// Como os convites de QR, vive **só em memória**: é material de pareamento, e
/// o banco vai para backup.
pub struct Codigo {
    pin: String,
    criado_em: Instant,
    tentativas: u32,
}

impl Codigo {
    /// Gera um PIN novo, com CSPRNG.
    ///
    /// Oito dígitos sorteados de verdade, e não derivados de horário ou
    /// contador — um código previsível dispensaria o atacante de adivinhar.
    pub fn novo() -> Self {
        // Amostragem por rejeição, e não `byte % 10`.
        //
        // 256 não é divisível por 10: com o módulo direto, os dígitos 0 a 5
        // sairiam ~1,2% mais que os outros. O viés é pequeno e reduz o espaço
        // real de busca — e num código que já tem só 26 bits, não há de onde
        // tirar entropia para gastar. Descartar os bytes acima de 249 custa
        // nada e distribui uniforme.
        let mut pin = String::with_capacity(DIGITOS);
        while pin.len() < DIGITOS {
            let mut byte = [0_u8; 1];
            rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut byte);
            if byte[0] >= 250 {
                continue;
            }
            pin.push(char::from(b'0' + byte[0] % 10));
        }
        Self {
            pin,
            criado_em: Instant::now(),
            tentativas: 0,
        }
    }

    pub fn pin(&self) -> &str {
        &self.pin
    }

    /// Como o código aparece para o humano: `1234 5678`.
    pub fn legivel(&self) -> String {
        format!("{} {}", &self.pin[..4], &self.pin[4..])
    }

    fn utilizavel(&self) -> Result<(), FalhaDeCodigo> {
        if self.tentativas >= MAX_TENTATIVAS {
            return Err(FalhaDeCodigo::TentativasEsgotadas);
        }
        if self.criado_em.elapsed() > VALIDADE {
            return Err(FalhaDeCodigo::CodigoExpirado);
        }
        Ok(())
    }

    #[cfg(test)]
    fn envelhecer(&mut self, quanto: Duration) {
        self.criado_em -= quanto;
    }
}

pub fn normalizar(digitado: &str) -> Result<String, FalhaDeCodigo> {
    let so_digitos: String = digitado.chars().filter(|c| c.is_ascii_digit()).collect();
    if so_digitos.len() != DIGITOS {
        return Err(FalhaDeCodigo::Malformado);
    }
    Ok(so_digitos)
}

/// Um lado da troca SPAKE2, esperando a mensagem do outro.
pub struct TrocaPendente {
    estado: Spake2<Ed25519Group>,
    pub mensagem: Vec<u8>,
}

impl TrocaPendente {
    /// Começa a troca do lado de quem mostra o código.
    pub fn anfitriao(pin: &str) -> Self {
        let (estado, mensagem) = Spake2::<Ed25519Group>::start_a(
            &Password::new(pin.as_bytes()),
            &Identity::new(IDENTIDADE_ANFITRIAO),
            &Identity::new(IDENTIDADE_VISITANTE),
        );
        Self { estado, mensagem }
    }

    /// E do lado de quem digitou.
    pub fn visitante(pin: &str) -> Self {
        let (estado, mensagem) = Spake2::<Ed25519Group>::start_b(
            &Password::new(pin.as_bytes()),
            &Identity::new(IDENTIDADE_ANFITRIAO),
            &Identity::new(IDENTIDADE_VISITANTE),
        );
        Self { estado, mensagem }
    }

    /// Conclui com a mensagem do outro lado e devolve o **segredo mestre**.
    ///
    /// **Isto é o que o PAKE entrega**: um segredo que só existe se os dois
    /// lados tinham o mesmo PIN, e que não pode ser derivado de fora nem
    /// tendo o transcript inteiro.
    ///
    /// Repare que ele **não é uma chave de uso**. Ver [`SegredoMestre`].
    pub fn concluir(self, do_outro: &[u8]) -> Result<SegredoMestre, FalhaDeCodigo> {
        let material = self
            .estado
            .finish(do_outro)
            .map_err(|_| FalhaDeCodigo::NaoCombinou)?;
        let bytes =
            <[u8; 32]>::try_from(material.as_slice()).map_err(|_| FalhaDeCodigo::NaoCombinou)?;
        Ok(SegredoMestre(bytes))
    }
}

/// O que o SPAKE2 produz: material mestre, **não** uma chave de uso.
///
/// # Por que não usar isto direto como `psk`
///
/// Não é questão de entropia — o SPAKE2 já resolveu isso. É **separação de
/// domínio**. Hoje o segredo tem um uso só; amanhã alguém vai querer
/// confirmação de chave, identificador de sessão, um MAC. Se todos saírem do
/// mesmo material, a mesma chave passa a viver em protocolos diferentes, e é
/// assim que uma construção que valia num contexto passa a valer noutro.
///
/// ```text
/// segredo mestre do SPAKE2
///     ├─ HKDF("…noise-psk")      →  psk do Noise
///     ├─ HKDF("…confirmation")   →  confirmação de chave (quando existir)
///     └─ HKDF("…outra")          →  o que vier depois
/// ```
///
/// Cada finalidade fica criptograficamente separada, e nenhuma consegue
/// produzir a chave da outra. É o que o `magic-wormhole` faz no Dilation, e o
/// que a documentação do SPAKE2 recomenda: tratar a saída como material para
/// HKDF, não como chave pronta.
///
/// `Debug` escrito à mão, pelo mesmo motivo da `DeviceIdentity`: material
/// criptográfico não vaza em log de pânico.
pub struct SegredoMestre([u8; 32]);

impl std::fmt::Debug for SegredoMestre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SegredoMestre(<redigido>)")
    }
}

/// Rótulo do `psk` do Noise. Cada finalidade tem o seu, e eles não se repetem.
const INFO_NOISE_PSK: &[u8] = b"narrahub.sync.v2.pake.noise-psk";

/// Rótulo da confirmação de chave. Ainda sem uso — existe aqui para o teste
/// poder provar que dois rótulos produzem chaves diferentes, que é a
/// propriedade inteira.
const INFO_CONFIRMACAO: &[u8] = b"narrahub.sync.v2.pake.confirmation";

impl SegredoMestre {
    /// Deriva a chave de uma finalidade específica.
    fn derivar(&self, info: &[u8]) -> [u8; 32] {
        // Sem `salt`: os dois lados precisam chegar ao mesmo valor sem trocar
        // mais nada, e a separação vem do `info`. O segredo já é uniforme —
        // é saída de SPAKE2, não uma senha.
        let hkdf = hkdf::Hkdf::<sha2::Sha256>::new(None, &self.0);
        let mut saida = [0_u8; 32];
        hkdf.expand(info, &mut saida)
            .expect("32 bytes cabem numa expansão HKDF-SHA256");
        saida
    }

    /// O `psk` do `XXpsk0`.
    pub fn psk_do_noise(&self) -> [u8; 32] {
        self.derivar(INFO_NOISE_PSK)
    }

    /// Chave de confirmação. Sem uso ainda; ver `INFO_CONFIRMACAO`.
    pub fn chave_de_confirmacao(&self) -> [u8; 32] {
        self.derivar(INFO_CONFIRMACAO)
    }

    #[cfg(test)]
    fn bytes_do_mestre(&self) -> [u8; 32] {
        self.0
    }
}

/// Os códigos abertos deste aparelho.
#[derive(Default)]
pub struct Codigos {
    abertos: std::collections::HashMap<String, Codigo>,
}

impl Codigos {
    /// Emite um código e devolve o identificador dele e o PIN para a tela.
    pub fn emitir(&mut self) -> (String, String) {
        let codigo = Codigo::novo();
        let mut id = [0_u8; 8];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut id);
        let id = base32(&id);
        let legivel = codigo.legivel();
        self.abertos.insert(id.clone(), codigo);
        (id, legivel)
    }

    /// Começa a troca do lado do anfitrião, contando a tentativa.
    ///
    /// A tentativa é contada **na abertura**, não no fim. Contar só quando
    /// falha permitiria a um atacante abandonar a conexão antes do resultado e
    /// tentar de novo à vontade — o limite de três viraria decorativo, e com
    /// ele a única barreira que o PAKE deixa de pé.
    pub fn iniciar_tentativa(&mut self, id: &str) -> Result<TrocaPendente, FalhaDeCodigo> {
        let codigo = self
            .abertos
            .get_mut(id)
            .ok_or(FalhaDeCodigo::CodigoDesconhecido)?;
        codigo.utilizavel()?;
        codigo.tentativas += 1;
        Ok(TrocaPendente::anfitriao(&codigo.pin))
    }

    /// O único código aberto, quando há exatamente um.
    ///
    /// O pareamento por PIN precisa disto porque o visitante **não conhece o
    /// id** — ele digitou oito dígitos, e o id nunca apareceu em tela nenhuma.
    /// O anfitrião, então, resolve o código pelo que ele próprio tem aberto.
    ///
    /// Com dois códigos abertos a resposta seria um palpite: o SPAKE2 exige
    /// escolher o PIN **antes** de calcular, e tentar os dois transformaria o
    /// limite de três tentativas em seis. Recusar é o que mantém o limite
    /// significando o que diz.
    pub fn unico_aberto(&self) -> Result<String, FalhaDeCodigo> {
        let mut aberto = self.abertos.keys();
        match (aberto.next(), aberto.next()) {
            (Some(id), None) => Ok(id.clone()),
            (None, _) => Err(FalhaDeCodigo::CodigoDesconhecido),
            (Some(_), Some(_)) => Err(FalhaDeCodigo::CodigoDesconhecido),
        }
    }

    /// Encerra o código depois de um pareamento bem-sucedido.
    pub fn consumir(&mut self, id: &str) {
        self.abertos.remove(id);
    }

    #[cfg(test)]
    fn pin_de(&self, id: &str) -> String {
        self.abertos[id].pin.clone()
    }

    #[cfg(test)]
    fn envelhecer(&mut self, id: &str, quanto: Duration) {
        if let Some(codigo) = self.abertos.get_mut(id) {
            codigo.envelhecer(quanto);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O que um atacante na rede veria: as duas mensagens do SPAKE2.
    type Transcript = (Vec<u8>, Vec<u8>);

    /// As chaves a que os dois lados chegaram, quando chegaram.
    type Chaves = Option<([u8; 32], [u8; 32])>;

    /// Uma troca completa, devolvendo o transcript e a chave de cada lado.
    fn trocar(pin_anfitriao: &str, pin_visitante: &str) -> (Transcript, Chaves) {
        let anfitriao = TrocaPendente::anfitriao(pin_anfitriao);
        let visitante = TrocaPendente::visitante(pin_visitante);
        let msg_a = anfitriao.mensagem.clone();
        let msg_v = visitante.mensagem.clone();

        // O que se compara é o `psk` derivado, não o mestre: é ele que vai
        // para o Noise, e é nele que uma falha apareceria.
        let chave_a = anfitriao.concluir(&msg_v);
        let chave_v = visitante.concluir(&msg_a);

        let chaves = match (chave_a, chave_v) {
            (Ok(a), Ok(v)) => Some((a.psk_do_noise(), v.psk_do_noise())),
            _ => None,
        };
        ((msg_a, msg_v), chaves)
    }

    #[test]
    fn o_pin_tem_oito_digitos_e_e_sorteado() {
        let a = Codigo::novo();
        let b = Codigo::novo();
        assert_eq!(a.pin().len(), DIGITOS);
        assert!(a.pin().chars().all(|c| c.is_ascii_digit()));
        assert_ne!(a.pin(), b.pin(), "dois códigos seguidos saíram iguais");
        assert_eq!(
            a.legivel().len(),
            DIGITOS + 1,
            "espaço no meio, para leitura"
        );
    }

    /// Os dígitos saem uniformes.
    ///
    /// `byte % 10` faria 0 a 5 aparecerem ~1,2% mais que 6 a 9, porque 256 não
    /// divide por 10. O viés é pequeno e reduz o espaço real de busca — num
    /// código de 26 bits não há entropia sobrando para gastar.
    ///
    /// O teste é estatístico e a margem é folgada de propósito: ele existe para
    /// pegar um viés estrutural, não para julgar o gerador do sistema.
    #[test]
    fn os_digitos_do_pin_nao_tem_vies_de_modulo() {
        let mut contagem = [0_u32; 10];
        let amostras = 4000;
        for _ in 0..amostras {
            for digito in Codigo::novo().pin().chars() {
                contagem[digito.to_digit(10).expect("dígito") as usize] += 1;
            }
        }

        let total: u32 = contagem.iter().sum();
        let esperado = f64::from(total) / 10.0;
        for (digito, vezes) in contagem.iter().enumerate() {
            let desvio = (f64::from(*vezes) - esperado).abs() / esperado;
            assert!(
                desvio < 0.10,
                "o dígito {digito} apareceu {vezes} vezes, {:.1}% fora do esperado",
                desvio * 100.0
            );
        }
    }

    #[test]
    fn digitos_iguais_chegam_a_mesma_chave() {
        let ((_, _), chaves) = trocar("12345678", "12345678");
        let (a, v) = chaves.expect("com o mesmo PIN os dois lados fecham");
        assert_eq!(a, v);
        assert_ne!(a, [0_u8; 32]);
    }

    #[test]
    fn digitos_diferentes_nao_chegam_a_mesma_chave() {
        let ((_, _), chaves) = trocar("12345678", "12345679");
        // O SPAKE2 pode devolver chaves diferentes em vez de erro — o que
        // importa é que elas NÃO batem, e o Noise em seguida não fecha.
        if let Some((a, v)) = chaves {
            assert_ne!(a, v, "PINs diferentes produziram a mesma chave");
        }
    }

    // ── O QUE ESTA ETAPA EXISTE PARA PROVAR ────────────────────────────────

    /// GATE DA ETAPA 10: o transcript não permite testar PINs offline.
    ///
    /// Este é o motivo de o PAKE existir, e o teste que um `hash(PIN)` quebrado
    /// **não** passaria.
    ///
    /// O atacante recebe tudo que passou pela rede — as duas mensagens — e
    /// ainda por cima o **PIN correto**, que é mais do que ele teria.
    ///
    /// Cuidado com a leitura larga: isto **não** diz que vazar o PIN é
    /// inofensivo. Quem sabe o PIN e consegue falar com o aparelho se
    /// autentica, e é para isso que o PIN existe. O que o teste demonstra é
    /// que a **captura** não vira verificador — nem com o PIN na mão dá para
    /// reconstruir a sessão gravada:
    ///
    /// ```text
    /// transcript + PIN correto  ──▶  chave da sessão ?
    ///                                       ❌
    /// ```
    ///
    /// Porque a chave depende também do escalar efêmero de cada lado, que
    /// nunca vai para a rede. Sem uma função `(transcript, palpite) → resposta`,
    /// não existe verificador offline — e sem verificador, não existe ataque de
    /// dicionário em casa. Cada palpite precisa de uma interação, e aí os
    /// `MAX_TENTATIVAS` valem.
    #[test]
    fn o_transcript_nao_permite_testar_pins_offline() {
        let pin = "13572468";
        let ((msg_a, msg_v), chaves) = trocar(pin, pin);
        let (chave_real, _) = chaves.expect("a sessão legítima fecha");

        // O atacante gravou tudo E sabe o PIN. Ele tenta reproduzir a chave.
        let tentativa_como_anfitriao = TrocaPendente::anfitriao(pin)
            .concluir(&msg_v)
            .expect("o SPAKE2 do atacante roda")
            .psk_do_noise();
        let tentativa_como_visitante = TrocaPendente::visitante(pin)
            .concluir(&msg_a)
            .expect("o SPAKE2 do atacante roda")
            .psk_do_noise();

        assert_ne!(
            tentativa_como_anfitriao, chave_real,
            "o transcript mais o PIN correto reproduziram a chave da sessão: \
             existiria verificador offline, e o PAKE não estaria fazendo nada"
        );
        assert_ne!(tentativa_como_visitante, chave_real);
    }

    /// E o corolário: **nenhum** palpite se distingue de outro pelo transcript.
    ///
    /// Um verificador offline precisaria de um sinal que diferencie o palpite
    /// certo do errado. Aqui, tanto o certo quanto o errado produzem chaves que
    /// não batem com a real — ou seja, o atacante não tem como pontuar
    /// candidatos.
    #[test]
    fn o_palpite_certo_nao_se_distingue_do_errado_pelo_transcript() {
        let pin_real = "13572468";
        let ((_, msg_v), chaves) = trocar(pin_real, pin_real);
        let (chave_real, _) = chaves.expect("sessão legítima");

        let mut palpites = vec![pin_real.to_string()];
        palpites.extend((0..20).map(|indice| format!("{:08}", 10000000 + indice)));

        for palpite in palpites {
            let derivada = TrocaPendente::anfitriao(&palpite)
                .concluir(&msg_v)
                .expect("o SPAKE2 roda com qualquer palpite")
                .psk_do_noise();
            assert_ne!(
                derivada, chave_real,
                "o palpite {palpite} reproduziu a chave: o transcript virou verificador"
            );
        }
    }

    /// O contraste com o desenho errado, para deixar a diferença explícita.
    ///
    /// Se o `psk` fosse `hash(PIN)`, um atacante com o transcript **conseguiria**
    /// pontuar candidatos: derivar `hash(palpite)` e comparar com o material que
    /// protege a sessão. O teste demonstra que ali existe a função que aqui não
    /// existe.
    #[test]
    fn com_hash_do_pin_existiria_verificador_offline() {
        use sha2::{Digest, Sha256};

        let derivar = |pin: &str| -> [u8; 32] {
            let mut hasher = Sha256::new();
            hasher.update(pin.as_bytes());
            hasher.finalize().into()
        };

        let pin_real = "13572468";
        let psk_real = derivar(pin_real);

        // O atacante testa candidatos em casa, sem falar com ninguém.
        let mut encontrou = None;
        for candidato in 13572460..13572470_u32 {
            let tentativa = format!("{candidato:08}");
            if derivar(&tentativa) == psk_real {
                encontrou = Some(tentativa);
                break;
            }
        }

        assert_eq!(
            encontrou.as_deref(),
            Some(pin_real),
            "o ataque offline sobre hash(PIN) precisa funcionar — é o que ele demonstra"
        );

        // E o PAKE não tem esse caminho: não há função de (palpite) para um
        // valor comparável com o que trafegou. É a diferença inteira.
    }

    /// Duas sessões com o **mesmo** PIN produzem transcripts diferentes.
    ///
    /// Se as mensagens fossem função só do PIN, elas se repetiriam — e o
    /// atacante poderia montar uma tabela: transcript observado → PIN.
    #[test]
    fn o_mesmo_pin_nao_produz_o_mesmo_transcript() {
        let pin = "13572468";
        let ((a1, v1), _) = trocar(pin, pin);
        let ((a2, v2), _) = trocar(pin, pin);

        assert_ne!(a1, a2, "a mensagem do anfitrião é função só do PIN");
        assert_ne!(v1, v2, "a mensagem do visitante é função só do PIN");
    }

    /// E o PIN não aparece no que trafega.
    ///
    /// Necessário e longe de suficiente — os testes acima é que carregam a
    /// garantia. Este pega o erro grosseiro.
    #[test]
    fn o_pin_nao_aparece_no_transcript() {
        let pin = "13572468";
        let ((msg_a, msg_v), _) = trocar(pin, pin);
        for mensagem in [&msg_a, &msg_v] {
            let como_texto = String::from_utf8_lossy(mensagem);
            assert!(!como_texto.contains(pin), "o PIN vazou em claro");
            assert!(
                !mensagem
                    .windows(pin.len())
                    .any(|janela| janela == pin.as_bytes()),
                "os dígitos do PIN aparecem nos bytes que trafegam"
            );
        }
    }

    // ── separação de domínio (NH-061) ──────────────────────────────────────

    /// GATE DA NH-061: rótulos diferentes produzem chaves diferentes.
    ///
    /// É a propriedade inteira. Sem ela, a chave do Noise e a de confirmação
    /// seriam o mesmo valor, e uma construção que vale num contexto passaria a
    /// valer no outro.
    #[test]
    fn cada_finalidade_recebe_uma_chave_propria() {
        let ((_, msg_v), _) = trocar("13572468", "13572468");
        let mestre = TrocaPendente::anfitriao("13572468")
            .concluir(&msg_v)
            .expect("derivar");

        let psk = mestre.psk_do_noise();
        let confirmacao = mestre.chave_de_confirmacao();

        assert_ne!(
            psk, confirmacao,
            "duas finalidades receberam a mesma chave: a separação não existe"
        );
    }

    /// E nenhuma delas é o segredo mestre cru.
    ///
    /// Entregar o mestre direto como `psk` é o que a NH-061 corrigiu: o
    /// primeiro uso amarraria o material a um protocolo, e o segundo herdaria
    /// a chave do primeiro.
    #[test]
    fn nenhuma_chave_de_uso_e_o_segredo_mestre_cru() {
        let ((_, msg_v), _) = trocar("13572468", "13572468");
        let mestre = TrocaPendente::anfitriao("13572468")
            .concluir(&msg_v)
            .expect("derivar");

        let cru = mestre.bytes_do_mestre();
        assert_ne!(mestre.psk_do_noise(), cru);
        assert_ne!(mestre.chave_de_confirmacao(), cru);
    }

    /// A derivação é determinística: os dois lados chegam ao mesmo `psk` sem
    /// trocar mais nada.
    #[test]
    fn os_dois_lados_derivam_o_mesmo_psk() {
        let pin = "13572468";
        let anfitriao = TrocaPendente::anfitriao(pin);
        let visitante = TrocaPendente::visitante(pin);
        let msg_a = anfitriao.mensagem.clone();
        let msg_v = visitante.mensagem.clone();

        let mestre_a = anfitriao.concluir(&msg_v).expect("anfitrião");
        let mestre_v = visitante.concluir(&msg_a).expect("visitante");

        assert_eq!(mestre_a.psk_do_noise(), mestre_v.psk_do_noise());
        assert_eq!(
            mestre_a.chave_de_confirmacao(),
            mestre_v.chave_de_confirmacao()
        );
    }

    // ── limite de tentativas, que é a outra metade ─────────────────────────

    /// Três erros e o código morre.
    ///
    /// O PAKE torna cada palpite uma interação; **este limite é o que torna o
    /// número de interações finito**. Um sem o outro não protege.
    #[test]
    fn tres_tentativas_e_o_codigo_morre() {
        let mut codigos = Codigos::default();
        let (id, _) = codigos.emitir();

        for tentativa in 1..=MAX_TENTATIVAS {
            codigos
                .iniciar_tentativa(&id)
                .unwrap_or_else(|erro| panic!("tentativa {tentativa} recusada: {erro}"));
        }

        assert_eq!(
            codigos.iniciar_tentativa(&id).err(),
            Some(FalhaDeCodigo::TentativasEsgotadas)
        );
    }

    /// A tentativa é contada **ao abrir**, não ao falhar.
    ///
    /// Contar só quando falha permitiria abandonar a conexão antes do resultado
    /// e tentar de novo à vontade — o limite de três viraria decorativo, e com
    /// ele a única barreira que o PAKE deixa de pé.
    #[test]
    fn abandonar_a_conexao_no_meio_nao_devolve_a_tentativa() {
        let mut codigos = Codigos::default();
        let (id, _) = codigos.emitir();

        for _ in 0..MAX_TENTATIVAS {
            // Abre e larga, sem concluir.
            let _troca = codigos.iniciar_tentativa(&id).expect("abrir");
        }

        assert_eq!(
            codigos.iniciar_tentativa(&id).err(),
            Some(FalhaDeCodigo::TentativasEsgotadas),
            "abandonar no meio devolveu tentativas ao atacante"
        );
    }

    #[test]
    fn codigo_expirado_e_recusado() {
        let mut codigos = Codigos::default();
        let (id, _) = codigos.emitir();
        codigos.envelhecer(&id, VALIDADE + Duration::from_secs(1));
        assert_eq!(
            codigos.iniciar_tentativa(&id).err(),
            Some(FalhaDeCodigo::CodigoExpirado)
        );
    }

    #[test]
    fn codigo_consumido_some() {
        let mut codigos = Codigos::default();
        let (id, _) = codigos.emitir();
        codigos.consumir(&id);
        assert_eq!(
            codigos.iniciar_tentativa(&id).err(),
            Some(FalhaDeCodigo::CodigoDesconhecido)
        );
    }

    #[test]
    fn o_que_o_humano_digita_e_normalizado() {
        assert_eq!(normalizar("1357 2468").as_deref(), Ok("13572468"));
        assert_eq!(normalizar("1357-2468").as_deref(), Ok("13572468"));
        assert_eq!(normalizar("1234567").err(), Some(FalhaDeCodigo::Malformado));
        assert_eq!(
            normalizar("123456789").err(),
            Some(FalhaDeCodigo::Malformado)
        );
        assert_eq!(
            normalizar("abcdefgh").err(),
            Some(FalhaDeCodigo::Malformado)
        );
    }

    /// O caminho inteiro: PIN → SPAKE2 → chave forte → pareamento Noise.
    #[test]
    fn a_chave_do_pake_serve_de_psk_para_o_pareamento() {
        let mut codigos = Codigos::default();
        let (id, _) = codigos.emitir();
        let pin = codigos.pin_de(&id);

        let anfitriao = codigos.iniciar_tentativa(&id).expect("abrir tentativa");
        let visitante = TrocaPendente::visitante(&pin);
        let msg_a = anfitriao.mensagem.clone();
        let msg_v = visitante.mensagem.clone();

        let psk_a = anfitriao
            .concluir(&msg_v)
            .expect("anfitrião")
            .psk_do_noise();
        let psk_v = visitante
            .concluir(&msg_a)
            .expect("visitante")
            .psk_do_noise();
        assert_eq!(psk_a, psk_v);

        // E os 32 bytes têm a forma que o `psk` do Noise espera.
        assert_eq!(psk_a.len(), 32);
        assert_ne!(psk_a, [0_u8; 32]);
        codigos.consumir(&id);
    }
}
