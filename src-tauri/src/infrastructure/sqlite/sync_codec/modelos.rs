//! Codec de `entity_template_set` (B6, item 1) — os atributos padrão de um tipo de ficha.
//!
//! ```text
//! identity = (universeId, entityType)   codificada com o tamanho de cada parte na frente
//! payload  = { universeId, entityType, attributes: [ { key, defaultValue }, … ] }
//! ```
//!
//! ## Por que a identidade não é o `id` da linha
//!
//! `entity_templates` tem uma linha por atributo, com `id` aleatório. Esse `id` é local: dois
//! aparelhos que tenham o mesmo modelo de ficha têm ids diferentes para as mesmas linhas. O que
//! existe de verdade, para o escritor, é **o conjunto** — "todo Personagem deste universo nasce
//! com Idade e Origem". Por isso o agregado é o conjunto inteiro, e a identidade é a dupla.
//!
//! E a dupla é codificada com o tamanho de cada parte ([`identidade_composta`]), não concatenada
//! com delimitador: `entityType` é texto que o escritor digitou e pode conter qualquer caractere.
//!
//! ## Este agregado não tem escritor no app — e nem por isso é somente-leitura no protocolo
//!
//! Nenhum comando cria template hoje; o acervo veio de versões antigas e de snapshot, e o app só
//! lê na criação de ficha. Mas o codec **precisa** saber aplicar: quando a etapa F resolver um
//! conflito com "usar A", "usar B" ou "combinar", é o core que materializa o resultado. Ausência
//! de tela de edição não é ausência de escrita.
//!
//! ## Empate e duplicata: fail closed
//!
//! A leitura legada ordenava por `sort_order` e nada mais. Empate resolvia pela ordem que o SQLite
//! devolvesse — que não é garantida. Duas gêneses do mesmo acervo poderiam produzir ordens
//! diferentes, e cada aparelho adotaria um payload distinto para o mesmo conjunto. Aqui a ordem é
//! `(sort_order, attributeKey)`, determinística por construção.
//!
//! Duplicata de `attributeKey` no mesmo conjunto é outra coisa, e **não** se resolve escolhendo
//! uma: as duas linhas dizem valores padrão diferentes para o mesmo atributo, e escolher seria
//! decidir pelo escritor qual delas ele quis. A leitura canônica **recusa**, e a adoção para ali
//! até alguém sanear.

use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};

use super::{de_json, erro, existe, para_json, EstadoDoAgregado};
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::conflito::{identidade_composta, partes_da_identidade};
use crate::domain::ids::new_id;
use crate::domain::sync::{EventEnvelope, Operation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtributoPadrao {
    pub key: String,
    pub default_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConjuntoDeModelos {
    pub universe_id: String,
    pub entity_type: String,
    /// Em ordem determinística: `(sort_order, key)`. A posição na lista É a ordem — o
    /// `sort_order` físico não viaja.
    pub attributes: Vec<AtributoPadrao>,
}

/// A identidade do conjunto, a partir das duas partes.
pub fn identidade(universe_id: &str, entity_type: &str) -> String {
    identidade_composta(&[universe_id, entity_type])
}

/// As duas partes, a partir da identidade.
pub fn partes(id: &str) -> DatabaseCommandResult<(String, String)> {
    match partes_da_identidade(id).as_deref() {
        Some([universo, tipo]) => Ok((universo.clone(), tipo.clone())),
        _ => Err(DatabaseCommandError::storage(format!(
            "'{id}' não é uma identidade de conjunto de modelos (universeId, entityType)."
        ))),
    }
}

// ── Auditoria ────────────────────────────────────────────────────────────

/// O que impede um conjunto de ser adotado como está.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicata {
    pub universe_id: String,
    pub entity_type: String,
    pub attribute_key: String,
    pub quantas: i64,
}

/// **Todos os conjuntos com a mesma `attributeKey` repetida.**
///
/// É a auditoria que roda antes de qualquer decisão sobre adotar o acervo. Devolver a lista, em vez
/// de só um `bool`, é o que permite ao saneamento ser explícito: quem for corrigir precisa saber
/// quais chaves, em quais tipos, de quais universos.
pub fn duplicatas(connection: &Connection) -> DatabaseCommandResult<Vec<Duplicata>> {
    let mut consulta = connection
        .prepare(
            "SELECT universe_id, entity_type, attribute_key, COUNT(*) AS quantas
               FROM entity_templates
              GROUP BY universe_id, entity_type, attribute_key
             HAVING COUNT(*) > 1
              ORDER BY universe_id, entity_type, attribute_key",
        )
        .map_err(erro)?;
    let linhas = consulta
        .query_map([], |row| {
            Ok(Duplicata {
                universe_id: row.get(0)?,
                entity_type: row.get(1)?,
                attribute_key: row.get(2)?,
                quantas: row.get(3)?,
            })
        })
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

/// Os conjuntos existentes no banco, como identidades de agregado.
pub fn conjuntos(connection: &Connection) -> DatabaseCommandResult<Vec<String>> {
    let mut consulta = connection
        .prepare(
            "SELECT DISTINCT universe_id, entity_type FROM entity_templates
              ORDER BY universe_id, entity_type",
        )
        .map_err(erro)?;
    let linhas = consulta
        .query_map([], |row| {
            Ok(identidade(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
            ))
        })
        .map_err(erro)?;
    linhas.collect::<Result<_, _>>().map_err(erro)
}

// ── Leitura canônica ─────────────────────────────────────────────────────

pub fn ler(connection: &Connection, id: &str) -> DatabaseCommandResult<Option<EstadoDoAgregado>> {
    let (universe_id, entity_type) = partes(id)?;
    // `(sort_order, attribute_key)`: o desempate faz parte do contrato. Sem ele, dois aparelhos
    // com o mesmo acervo poderiam ler o mesmo conjunto em ordens diferentes.
    let mut consulta = connection
        .prepare(
            "SELECT attribute_key, default_value FROM entity_templates
              WHERE universe_id = ?1 AND entity_type = ?2
              ORDER BY sort_order, attribute_key",
        )
        .map_err(erro)?;
    let atributos: Vec<AtributoPadrao> = consulta
        .query_map([&universe_id, &entity_type], |row| {
            Ok(AtributoPadrao {
                key: row.get(0)?,
                default_value: row.get(1)?,
            })
        })
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?;

    if atributos.is_empty() {
        return Ok(None);
    }

    // Duplicata não vira escolha silenciosa. Duas linhas com a mesma chave dizem valores padrão
    // diferentes para o mesmo atributo, e decidir qual vale seria decidir pelo escritor.
    let mut vistas: Vec<&str> = Vec::with_capacity(atributos.len());
    for atributo in &atributos {
        if vistas.contains(&atributo.key.as_str()) {
            return Err(DatabaseCommandError::storage(format!(
                "O conjunto de modelos de '{entity_type}' no universo {universe_id} tem \
                 '{}' repetido. Duas linhas dizem o valor padrão do mesmo atributo, e escolher \
                 uma seria decidir pelo escritor. Saneie antes de adotar.",
                atributo.key
            )));
        }
        vistas.push(&atributo.key);
    }

    let payload = para_json(&ConjuntoDeModelos {
        universe_id: universe_id.clone(),
        entity_type,
        attributes: atributos,
    })?;
    Ok(Some(EstadoDoAgregado {
        universe_id,
        payload,
    }))
}

// ── Validação (a mesma no local e no remoto) ─────────────────────────────

pub(super) fn validar(
    connection: &Connection,
    payload: &str,
) -> DatabaseCommandResult<Option<String>> {
    let conjunto: ConjuntoDeModelos = serde_json::from_str(payload).map_err(|error| {
        DatabaseCommandError::storage(format!("Conjunto de modelos ilegível: {error}"))
    })?;
    if conjunto.entity_type.trim().is_empty() {
        return Err(DatabaseCommandError::storage(
            "Conjunto de modelos sem tipo de ficha. Estado incompatível.",
        ));
    }
    let mut vistas: Vec<&str> = Vec::new();
    for atributo in &conjunto.attributes {
        if atributo.key.trim().is_empty() {
            return Err(DatabaseCommandError::storage(
                "Atributo padrão sem chave. Estado incompatível.",
            ));
        }
        if vistas.contains(&atributo.key.as_str()) {
            return Err(DatabaseCommandError::storage(format!(
                "O conjunto de modelos traz '{}' duas vezes. Estado incompatível.",
                atributo.key
            )));
        }
        vistas.push(&atributo.key);
    }
    if !existe(connection, "universes", &conjunto.universe_id)? {
        return Ok(Some(format!("universe {}", conjunto.universe_id)));
    }
    Ok(None)
}

pub fn dependencias(
    connection: &Connection,
    envelope: &EventEnvelope,
) -> DatabaseCommandResult<Option<String>> {
    validar(connection, &envelope.payload)
}

// ── Aplicação ────────────────────────────────────────────────────────────

/// Materializa o conjunto inteiro: apaga as linhas daquele `(universo, tipo)` e regrava na ordem
/// do payload.
///
/// Substituir tudo, em vez de casar linha a linha, é o que faz a materialização ser **exata**: o
/// `sort_order` de cada linha passa a ser a posição na lista, e o estado relido volta byte a byte
/// igual ao payload. Casar por chave deixaria `sort_order` antigo sobrevivendo e a releitura
/// poderia sair em outra ordem.
pub fn aplicar(tx: &Transaction<'_>, envelope: &EventEnvelope) -> DatabaseCommandResult<()> {
    let (universe_id, entity_type) = partes(&envelope.aggregate_id)?;
    tx.execute(
        "DELETE FROM entity_templates WHERE universe_id = ?1 AND entity_type = ?2",
        [&universe_id, &entity_type],
    )
    .map_err(erro)?;
    if envelope.operation == Operation::Delete {
        return Ok(());
    }

    let conjunto: ConjuntoDeModelos = de_json(envelope)?;
    if conjunto.universe_id != universe_id || conjunto.entity_type != entity_type {
        return Err(DatabaseCommandError::storage(format!(
            "O payload descreve o conjunto de '{}' em {}, e o envelope é de '{entity_type}' em \
             {universe_id}.",
            conjunto.entity_type, conjunto.universe_id
        )));
    }
    for (posicao, atributo) in conjunto.attributes.iter().enumerate() {
        tx.execute(
            "INSERT INTO entity_templates
               (id, universe_id, entity_type, attribute_key, default_value, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                new_id(),
                &universe_id,
                &entity_type,
                &atributo.key,
                &atributo.default_value,
                posicao as i64,
            ],
        )
        .map_err(erro)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    fn semear(connection: &Connection, linhas: &[(&str, &str, &str, i64)]) {
        seed_universe(connection, "u1");
        for (tipo, chave, valor, ordem) in linhas {
            connection
                .execute(
                    "INSERT INTO entity_templates
                       (id, universe_id, entity_type, attribute_key, default_value, sort_order)
                     VALUES (?1, 'u1', ?2, ?3, ?4, ?5)",
                    rusqlite::params![new_id(), tipo, chave, valor, ordem],
                )
                .expect("semear");
        }
    }

    /// **Empate de `sort_order` não decide a ordem por sorte.**
    ///
    /// A leitura legada ordenava só por `sort_order`. Com empate, a ordem era a que o SQLite
    /// devolvesse — e duas gêneses do mesmo acervo poderiam produzir payloads diferentes.
    #[test]
    fn empate_de_ordem_e_desempatado_pela_chave() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        semear(
            &connection,
            &[
                ("Personagem", "Origem", "", 0),
                ("Personagem", "Idade", "", 0),
                ("Personagem", "Altura", "", 0),
            ],
        );

        let estado = ler(&connection, &identidade("u1", "Personagem"))
            .expect("ler")
            .expect("existe");
        let conjunto: ConjuntoDeModelos = serde_json::from_str(&estado.payload).expect("payload");
        assert_eq!(
            conjunto
                .attributes
                .iter()
                .map(|a| a.key.as_str())
                .collect::<Vec<_>>(),
            vec!["Altura", "Idade", "Origem"]
        );
    }

    /// **Duplicata recusa a adoção em vez de escolher uma linha.**
    #[test]
    fn chave_repetida_recusa_a_leitura_canonica() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        semear(
            &connection,
            &[
                ("Personagem", "Idade", "20", 0),
                ("Personagem", "Idade", "30", 1),
            ],
        );

        let erro = ler(&connection, &identidade("u1", "Personagem")).expect_err("duplicata");
        assert!(erro.message.contains("Idade"), "{}", erro.message);
        assert!(erro.message.contains("Saneie"), "{}", erro.message);

        let achadas = duplicatas(&connection).expect("auditar");
        assert_eq!(achadas.len(), 1);
        assert_eq!(achadas[0].attribute_key, "Idade");
        assert_eq!(achadas[0].quantas, 2);
    }

    /// Conjunto sem linha nenhuma não existe — não é conjunto vazio.
    #[test]
    fn conjunto_sem_linhas_nao_existe() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        seed_universe(&connection, "u1");
        assert!(ler(&connection, &identidade("u1", "Personagem"))
            .expect("ler")
            .is_none());
    }

    /// A identidade sobrevive a um `entityType` com o caractere que seria delimitador.
    #[test]
    fn tipo_de_ficha_com_dois_pontos_nao_confunde_a_identidade() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        semear(&connection, &[("Lugar:Sagrado", "Clima", "ameno", 0)]);

        let id = identidade("u1", "Lugar:Sagrado");
        assert_eq!(
            partes(&id).expect("partes"),
            ("u1".to_string(), "Lugar:Sagrado".to_string())
        );
        assert!(ler(&connection, &id).expect("ler").is_some());
    }

    /// A auditoria enxerga o acervo inteiro, e não um universo de cada vez.
    #[test]
    fn a_auditoria_lista_os_conjuntos_existentes() {
        let banco = TemporaryDatabase::new();
        let connection = banco.connection();
        semear(
            &connection,
            &[
                ("Personagem", "Idade", "", 0),
                ("Lugar", "Clima", "", 0),
                ("Lugar", "Moeda", "", 1),
            ],
        );

        assert_eq!(
            conjuntos(&connection).expect("listar"),
            vec![identidade("u1", "Lugar"), identidade("u1", "Personagem")]
        );
        assert!(duplicatas(&connection).expect("auditar").is_empty());
    }
}
