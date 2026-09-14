//! O bundle de bootstrap no fio (etapa 14, fatia 4).
//!
//! ## Por que existe um formato de fio separado
//!
//! [`BootstrapBundle`] foi desenhado na etapa 12 para ser capturado e semeado
//! **no mesmo processo**: não deriva `serde`, e as linhas das tabelas são
//! `rusqlite::types::Value`. Agora ele precisa atravessar um socket.
//!
//! A saída conservadora é **não tocar no tipo da etapa 12**. Este módulo é uma
//! representação de transporte com ida e volta, e o contrato é um só:
//!
//! ```text
//! de_fio(para_fio(bundle)) == bundle        byte a byte, valor a valor
//! ```
//!
//! O gate `ida_e_volta_preserva_cada_tipo_de_valor` cobra isso sobre os cinco
//! tipos do SQLite, incluindo os que um formato ingênuo perderia.
//!
//! ## Os dois valores que um JSON ingênuo corromperia
//!
//! - **`REAL` não finito.** O `serde_json` serializa `inf` como `null`, e na
//!   volta `null` não é número. O real viaja como o **padrão de bits** do
//!   `f64`, em inteiro sem sinal — exato para qualquer valor.
//! - **`BLOB`.** Bytes em JSON como lista de números ocupariam quatro vezes o
//!   tamanho. Viajam em base64 estrito, com o decodificador da etapa 13, que
//!   recusa entrada malformada em vez de adivinhar.
//!
//! Nenhum dos dois é hipótese: são exatamente as colunas que um acervo real
//! tem, e perder uma delas no bootstrap produziria um receptor que parece
//! certo e diverge do doador.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

use crate::domain::data_url::{codificar_base64, decodificar_base64};
use crate::infrastructure::sqlite::sync_snapshot::{BootstrapBundle, MembroDoRoster, Tabela};

/// Um valor do SQLite, com o tipo explícito.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
enum ValorNoFio {
    Nulo,
    Inteiro(i64),
    /// `f64::to_bits`. Ver o cabeçalho.
    Real(u64),
    Texto(String),
    /// Base64 estrito.
    Bytes(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct TabelaNoFio {
    nome: String,
    colunas: Vec<String>,
    linhas: Vec<Vec<ValorNoFio>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MembroNoFio {
    device_id: String,
    name: String,
    ed25519_public: String,
    x25519_public: String,
    state: String,
    introduced_by: String,
    exit_reason: String,
}

/// O bundle, pronto para `serde_json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct BundleNoFio {
    tabelas: Vec<TabelaNoFio>,
    roster: Vec<MembroNoFio>,
    vetor: BTreeMap<String, i64>,
    blobs: BTreeSet<String>,
}

fn valor_para_fio(valor: &Value) -> ValorNoFio {
    match valor {
        Value::Null => ValorNoFio::Nulo,
        Value::Integer(n) => ValorNoFio::Inteiro(*n),
        Value::Real(r) => ValorNoFio::Real(r.to_bits()),
        Value::Text(t) => ValorNoFio::Texto(t.clone()),
        Value::Blob(b) => ValorNoFio::Bytes(codificar_base64(b)),
    }
}

fn valor_de_fio(valor: ValorNoFio) -> Result<Value, String> {
    Ok(match valor {
        ValorNoFio::Nulo => Value::Null,
        ValorNoFio::Inteiro(n) => Value::Integer(n),
        ValorNoFio::Real(bits) => Value::Real(f64::from_bits(bits)),
        ValorNoFio::Texto(t) => Value::Text(t),
        ValorNoFio::Bytes(b64) => Value::Blob(
            decodificar_base64(&b64)
                .map_err(|motivo| format!("coluna binária ilegível: {motivo}"))?,
        ),
    })
}

/// Prepara o bundle para atravessar a rede.
pub fn para_fio(bundle: &BootstrapBundle) -> BundleNoFio {
    BundleNoFio {
        tabelas: bundle
            .tabelas
            .iter()
            .map(|tabela| TabelaNoFio {
                nome: tabela.nome.clone(),
                colunas: tabela.colunas.clone(),
                linhas: tabela
                    .linhas
                    .iter()
                    .map(|linha| linha.iter().map(valor_para_fio).collect())
                    .collect(),
            })
            .collect(),
        roster: bundle
            .roster
            .iter()
            .map(|membro| MembroNoFio {
                device_id: membro.device_id.clone(),
                name: membro.name.clone(),
                ed25519_public: membro.ed25519_public.clone(),
                x25519_public: membro.x25519_public.clone(),
                state: membro.state.clone(),
                introduced_by: membro.introduced_by.clone(),
                exit_reason: membro.exit_reason.clone(),
            })
            .collect(),
        vetor: bundle.vetor.clone(),
        blobs: bundle.blobs.clone(),
    }
}

/// Reconstrói o bundle que chegou.
///
/// **Não valida o conteúdo** — e não deve. Quem decide se um bundle é
/// coerente, se o receptor está vazio e se a identidade bate é o `semear` da
/// etapa 12, dentro da transação dele. Validar aqui criaria uma segunda
/// definição de "bundle aceitável", e duas definições divergem.
///
/// O que este passo recusa é só o que não é bundle nenhum: uma coluna binária
/// com base64 quebrado, ou uma linha com número de valores diferente do número
/// de colunas — que no `semear` viraria um `INSERT` com parâmetros deslocados.
pub fn de_fio(fio: BundleNoFio) -> Result<BootstrapBundle, String> {
    let mut tabelas = Vec::with_capacity(fio.tabelas.len());
    for tabela in fio.tabelas {
        let largura = tabela.colunas.len();
        let mut linhas = Vec::with_capacity(tabela.linhas.len());
        for (indice, linha) in tabela.linhas.into_iter().enumerate() {
            if linha.len() != largura {
                return Err(format!(
                    "a linha {indice} da tabela {} tem {} valores para {largura} colunas",
                    tabela.nome,
                    linha.len()
                ));
            }
            linhas.push(
                linha
                    .into_iter()
                    .map(valor_de_fio)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        tabelas.push(Tabela {
            nome: tabela.nome,
            colunas: tabela.colunas,
            linhas,
        });
    }

    Ok(BootstrapBundle {
        tabelas,
        roster: fio
            .roster
            .into_iter()
            .map(|membro| MembroDoRoster {
                device_id: membro.device_id,
                name: membro.name,
                ed25519_public: membro.ed25519_public,
                x25519_public: membro.x25519_public,
                state: membro.state,
                introduced_by: membro.introduced_by,
                exit_reason: membro.exit_reason,
            })
            .collect(),
        vetor: fio.vetor,
        blobs: fio.blobs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle_com_todos_os_tipos() -> BootstrapBundle {
        BootstrapBundle {
            tabelas: vec![Tabela {
                nome: "chapters".into(),
                colunas: vec![
                    "nulo".into(),
                    "inteiro".into(),
                    "real".into(),
                    "texto".into(),
                    "bytes".into(),
                ],
                linhas: vec![
                    vec![
                        Value::Null,
                        Value::Integer(i64::MIN),
                        Value::Real(f64::INFINITY),
                        Value::Text("<p>acentuação e \"aspas\" e \\ barra</p>".into()),
                        Value::Blob((0..=255).collect()),
                    ],
                    vec![
                        Value::Null,
                        Value::Integer(i64::MAX),
                        Value::Real(-0.0),
                        Value::Text(String::new()),
                        Value::Blob(Vec::new()),
                    ],
                    vec![
                        Value::Null,
                        Value::Integer(0),
                        Value::Real(0.1 + 0.2),
                        Value::Text("😀".into()),
                        Value::Blob(vec![0]),
                    ],
                ],
            }],
            roster: vec![MembroDoRoster {
                device_id: "DEV".into(),
                name: "Desktop".into(),
                ed25519_public: "CHAVE".into(),
                x25519_public: String::new(),
                state: "active".into(),
                introduced_by: String::new(),
                exit_reason: String::new(),
            }],
            vetor: BTreeMap::from([("DEV".to_string(), 42)]),
            blobs: BTreeSet::from(["a".repeat(64)]),
        }
    }

    /// **Ida e volta preserva cada tipo de valor do SQLite.**
    ///
    /// Os casos não são decorativos: `i64::MIN` e `MAX` pegam um formato que
    /// passasse inteiro por `f64`; `inf` pega o `null` do `serde_json`; `-0.0`
    /// pega uma comparação que só olhasse o valor numérico; `0.1 + 0.2` pega
    /// arredondamento decimal; os 256 bytes pegam base64 que perca a borda.
    ///
    /// E o caminho inclui o `serde_json` de verdade, em bytes — que é o que
    /// atravessa a rede —, não só a conversão entre structs.
    #[test]
    fn ida_e_volta_preserva_cada_tipo_de_valor() {
        let original = bundle_com_todos_os_tipos();
        let bytes = serde_json::to_vec(&para_fio(&original)).expect("serializar");
        let volta: BundleNoFio = serde_json::from_slice(&bytes).expect("desserializar");
        let reconstruido = de_fio(volta).expect("reconstruir");

        assert_eq!(reconstruido.roster, original.roster);
        assert_eq!(reconstruido.vetor, original.vetor);
        assert_eq!(reconstruido.blobs, original.blobs);
        assert_eq!(reconstruido.tabelas.len(), 1);

        let (antes, depois) = (&original.tabelas[0], &reconstruido.tabelas[0]);
        assert_eq!(depois.colunas, antes.colunas);
        for (l, (linha_a, linha_d)) in antes.linhas.iter().zip(&depois.linhas).enumerate() {
            for (c, (va, vd)) in linha_a.iter().zip(linha_d).enumerate() {
                let igual = match (va, vd) {
                    // `PartialEq` de f64 diz 0.0 == -0.0; o bit diz que não.
                    (Value::Real(a), Value::Real(d)) => a.to_bits() == d.to_bits(),
                    _ => va == vd,
                };
                assert!(igual, "linha {l}, coluna {c}: {va:?} virou {vd:?}");
            }
        }
    }

    /// Linha com largura errada é recusada, e não vira `INSERT` deslocado.
    #[test]
    fn linha_com_largura_errada_e_recusada() {
        let mut fio = para_fio(&bundle_com_todos_os_tipos());
        fio.tabelas[0].linhas[0].pop();
        let erro = de_fio(fio).expect_err("tinha que recusar");
        assert!(erro.contains("4 valores para 5 colunas"), "motivo: {erro}");
    }

    /// Base64 quebrado numa coluna binária é recusado, não adivinhado.
    #[test]
    fn coluna_binaria_ilegivel_e_recusada() {
        let mut fio = para_fio(&bundle_com_todos_os_tipos());
        fio.tabelas[0].linhas[0][4] = ValorNoFio::Bytes("!!!nao-e-base64".into());
        assert!(de_fio(fio).is_err());
    }
}
