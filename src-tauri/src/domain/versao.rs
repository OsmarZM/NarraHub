//! Versão do aplicativo: leitura e comparação SemVer.
//!
//! Existe para a atualização do Android decidir se uma release do GitHub é mais nova que a
//! instalada. Comparar como texto erra justamente os casos que aparecem: `"0.9.10" < "0.9.2"`
//! lexicograficamente, e `"1.0.0-beta.10" < "1.0.0-beta.9"` também. A precedência aqui é a da
//! especificação SemVer 2.0.0, seção 11.
//!
//! Metadado de build (`+abc`) é aceito e ignorado na comparação, como a especificação manda.

use std::cmp::Ordering;

/// Uma versão `MAIOR.MENOR.PATCH`, com pré-release opcional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versao {
    pub maior: u64,
    pub menor: u64,
    pub patch: u64,
    /// Identificadores do pré-release, na ordem. Vazio numa versão estável.
    pub pre: Vec<Identificador>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identificador {
    Numerico(u64),
    Texto(String),
}

impl Versao {
    /// Lê `1.2.3`, `1.2.3-beta.4`, e aceita o prefixo das tags do projeto: `app-v1.2.3`, `v1.2.3`.
    pub fn ler(texto: &str) -> Result<Self, String> {
        let limpo = texto.trim();
        let limpo = limpo
            .strip_prefix("app-v")
            .or_else(|| limpo.strip_prefix('v'))
            .unwrap_or(limpo);
        let sem_build = limpo.split('+').next().unwrap_or(limpo);
        let (nucleo, pre) = match sem_build.split_once('-') {
            Some((nucleo, pre)) => (nucleo, Some(pre)),
            None => (sem_build, None),
        };

        let partes: Vec<&str> = nucleo.split('.').collect();
        if partes.len() != 3 {
            return Err(format!("\"{texto}\" não é uma versão MAIOR.MENOR.PATCH"));
        }
        let numero = |parte: &str| -> Result<u64, String> {
            // SemVer proíbe zero à esquerda: "01" não é "1".
            if parte.is_empty() || (parte.len() > 1 && parte.starts_with('0')) {
                return Err(format!("\"{texto}\" tem um número de versão inválido"));
            }
            parte
                .parse::<u64>()
                .map_err(|_| format!("\"{texto}\" tem um número de versão inválido"))
        };

        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre
                .split('.')
                .map(|parte| {
                    if parte.is_empty()
                        || !parte.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                    {
                        return Err(format!("\"{texto}\" tem um pré-release inválido"));
                    }
                    Ok(if parte.chars().all(|c| c.is_ascii_digit()) {
                        Identificador::Numerico(numero(parte)?)
                    } else {
                        Identificador::Texto(parte.to_string())
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };

        Ok(Self {
            maior: numero(partes[0])?,
            menor: numero(partes[1])?,
            patch: numero(partes[2])?,
            pre,
        })
    }

    pub fn e_pre_release(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl Ord for Versao {
    fn cmp(&self, outra: &Self) -> Ordering {
        (self.maior, self.menor, self.patch)
            .cmp(&(outra.maior, outra.menor, outra.patch))
            .then_with(|| match (self.pre.is_empty(), outra.pre.is_empty()) {
                // Estável vem DEPOIS de qualquer pré-release da mesma versão: 1.0.0-rc.9 < 1.0.0.
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => comparar_pre(&self.pre, &outra.pre),
            })
    }
}

impl PartialOrd for Versao {
    fn partial_cmp(&self, outra: &Self) -> Option<Ordering> {
        Some(self.cmp(outra))
    }
}

/// SemVer 11.4: numérico < texto; numéricos por valor; textos por ASCII; mais campos vence.
fn comparar_pre(a: &[Identificador], b: &[Identificador]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let ordem = match (x, y) {
            (Identificador::Numerico(x), Identificador::Numerico(y)) => x.cmp(y),
            (Identificador::Numerico(_), Identificador::Texto(_)) => Ordering::Less,
            (Identificador::Texto(_), Identificador::Numerico(_)) => Ordering::Greater,
            (Identificador::Texto(x), Identificador::Texto(y)) => x.cmp(y),
        };
        if ordem != Ordering::Equal {
            return ordem;
        }
    }
    a.len().cmp(&b.len())
}

/// Esta release é oferecida a quem tem esta versão instalada?
///
/// **O canal é a própria versão instalada.** Quem tem uma versão estável nunca recebe
/// pré-release; quem instalou uma beta recebe betas mais novas e, depois, a estável. Não existe
/// configuração de canal para errar: um testador que volta para a estável sai do canal de teste
/// sozinho.
pub fn deve_oferecer(instalada: &Versao, candidata: &Versao) -> bool {
    if candidata.e_pre_release() && !instalada.e_pre_release() {
        return false;
    }
    candidata > instalada
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(texto: &str) -> Versao {
        Versao::ler(texto).expect(texto)
    }

    /// **`0.9.10` é mais nova que `0.9.2`.** É o caso que comparação de texto erra, e foi o
    /// exemplo pedido.
    #[test]
    fn compara_por_numero_e_nao_por_texto() {
        assert!(v("0.9.10") > v("0.9.2"));
        assert!(v("0.10.0") > v("0.9.99"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(
            "0.9.10" < "0.9.2",
            "a premissa: como texto, a ordem sai errada"
        );
    }

    /// A ordem de pré-release da especificação, inclusive `beta.10` depois de `beta.9`.
    #[test]
    fn ordena_pre_releases_como_a_especificacao() {
        let ordem = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.9",
            "1.0.0-beta.10",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for par in ordem.windows(2) {
            assert!(
                v(par[0]) < v(par[1]),
                "{} devia vir antes de {}",
                par[0],
                par[1]
            );
        }
    }

    #[test]
    fn aceita_o_prefixo_das_tags_e_ignora_metadado_de_build() {
        assert_eq!(v("app-v0.9.3"), v("0.9.3"));
        assert_eq!(v("v0.9.3"), v("0.9.3"));
        assert_eq!(v("0.9.3+build.7").cmp(&v("0.9.3")), Ordering::Equal);
    }

    #[test]
    fn recusa_o_que_nao_e_versao() {
        for errada in [
            "",
            "0.9",
            "0.9.2.1",
            "01.9.2",
            "0.9.x",
            "0.9.2-",
            "0.9.2-beta..1",
            "latest",
        ] {
            assert!(Versao::ler(errada).is_err(), "devia recusar {errada:?}");
        }
    }

    /// **O canal é a versão instalada.**
    #[test]
    fn estavel_nunca_recebe_beta_e_beta_recebe_beta_e_estavel() {
        // Estável instalada.
        assert!(deve_oferecer(&v("0.9.2"), &v("0.9.3")));
        assert!(
            !deve_oferecer(&v("0.9.2"), &v("0.10.0-beta.1")),
            "estável não recebe beta"
        );
        assert!(
            !deve_oferecer(&v("0.9.3"), &v("0.9.3")),
            "mesma versão não é atualização"
        );
        assert!(
            !deve_oferecer(&v("0.9.3"), &v("0.9.2")),
            "versão mais velha não é atualização"
        );

        // Beta instalada.
        assert!(deve_oferecer(&v("0.10.0-beta.1"), &v("0.10.0-beta.2")));
        assert!(
            deve_oferecer(&v("0.10.0-beta.2"), &v("0.10.0")),
            "a estável encerra o teste"
        );
        assert!(!deve_oferecer(&v("0.10.0-beta.2"), &v("0.10.0-beta.1")));
    }
}
