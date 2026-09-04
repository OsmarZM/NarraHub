//! Sync V2 — troca de vetores e store-and-forward (ADR 0009 §13, etapa 6).
//!
//! Aqui a replicação deixa de ser "aplicar o que me deram" e passa a ser uma
//! conversa entre dois pares simétricos.
//!
//! ## Por que o vetor, e não um número
//!
//! A primeira redação do ADR dizia *"B pede a A: me mande tudo com
//! `seq > 1802` do seu `device_id`"*. Isso funciona para dois aparelhos e
//! **quebra em três**:
//!
//! ```text
//! Desktop edita  →  encontra o Notebook  →  Notebook tem o evento
//! Android encontra só o Notebook          →  nunca recebe o do Desktop
//! ```
//!
//! Cada peer guarda, por **origem**, até onde já viu. Numa sessão os dois
//! lados trocam esses vetores e cada um envia o que o outro não tem —
//! inclusive eventos originados por um terceiro aparelho ausente.
//!
//! ## Simetria de verdade
//!
//! Não existe um lado que pede e outro que responde. Os dois pedem e os dois
//! respondem, na mesma sessão. Quem abriu a porta é papel de **transporte**;
//! não faz dele servidor nem dono do dado (ADR 0009 §2).
//!
//! ## O relay não reassina
//!
//! O envelope é repassado **inteiro**, com a assinatura da origem intacta. Um
//! evento chega ao Android com a assinatura do Desktop mesmo tendo passado
//! pelo Notebook — e é isso que permite ao Android verificar a origem sem
//! nunca ter falado com o Desktop. Reassinar no relay destruiria a única
//! prova de quem escreveu.

use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use crate::domain::sync::{EventEnvelope, Operation};
use rusqlite::{Connection, OptionalExtension};
use std::collections::BTreeMap;

/// Até onde este aparelho viu, por origem. É o que os dois lados trocam no
/// início da sessão.
pub type VetorDeSequencias = BTreeMap<String, i64>;

/// Quantos eventos no máximo saem numa resposta.
///
/// Existe porque o primeiro encontro entre dois aparelhos antigos pode ter
/// milhares de eventos, e mandar tudo de uma vez trava a interface e estoura
/// a memória de um telefone. A sessão repete até o vetor parar de andar.
const LOTE_MAXIMO: usize = 500;

/// O vetor deste aparelho: por origem conhecida, a maior sequência contígua
/// aplicada.
///
/// Inclui a **própria** origem — é assim que o outro lado descobre o que este
/// aparelho escreveu.
pub fn vetor_local(connection: &Connection) -> DatabaseCommandResult<VetorDeSequencias> {
    let mut statement = connection
        .prepare("SELECT origin_device_id, last_seq_applied FROM sync_cursors")
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let linhas = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let mut vetor = VetorDeSequencias::new();
    for linha in linhas {
        let (origem, seq) =
            linha.map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        vetor.insert(origem, seq);
    }

    // Uma origem pode ter eventos no log sem ter cursor — é o caso do próprio
    // aparelho antes da primeira sessão. Sem isto, o primeiro encontro não
    // ofereceria nada do que foi escrito aqui.
    let mut origens = connection
        .prepare("SELECT device_id, MAX(seq) FROM sync_events GROUP BY device_id")
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    let linhas = origens
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    for linha in linhas {
        let (origem, maior) =
            linha.map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
        vetor.entry(origem).or_insert(maior);
    }

    Ok(vetor)
}

/// O que este aparelho tem e o outro não — de **todas** as origens.
///
/// É aqui que o store-and-forward acontece: o Notebook oferece ao Android
/// eventos que o Desktop originou, sem que os dois precisem se encontrar.
pub fn eventos_para(
    connection: &Connection,
    vetor_do_outro: &VetorDeSequencias,
) -> DatabaseCommandResult<Vec<EventEnvelope>> {
    let meu = vetor_local(connection)?;
    let mut saida = Vec::new();

    for (origem, meu_topo) in &meu {
        let topo_dele = vetor_do_outro.get(origem).copied().unwrap_or(0);
        if topo_dele >= *meu_topo {
            continue;
        }
        for envelope in eventos_da_origem(connection, origem, topo_dele, *meu_topo)? {
            saida.push(envelope);
            if saida.len() >= LOTE_MAXIMO {
                return Ok(saida);
            }
        }
    }

    Ok(saida)
}

/// Os eventos de uma origem no intervalo `(depois_de, ate]`.
///
/// Só entram os que **foram aplicados** aqui: repassar um pendente enviaria
/// adiante um evento cuja lacuna anterior este aparelho ainda não fechou, e o
/// outro lado o guardaria como pendente também — ruído em vez de progresso.
fn eventos_da_origem(
    connection: &Connection,
    origem: &str,
    depois_de: i64,
    ate: i64,
) -> DatabaseCommandResult<Vec<EventEnvelope>> {
    let mut statement = connection
        .prepare(
            "SELECT e.event_id, e.device_id, e.seq, e.universe_id, e.aggregate_type,
                    e.aggregate_id, e.operation, e.payload, e.base_rev, e.new_rev, e.signature
               FROM sync_events e
               JOIN sync_applied_events a ON a.event_id = e.event_id
              WHERE e.device_id = ?1 AND e.seq > ?2 AND e.seq <= ?3
           ORDER BY e.seq",
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    let linhas = statement
        .query_map(rusqlite::params![origem, depois_de, ate], |row| {
            let operacao: String = row.get(6)?;
            let operation = Operation::parse(&operacao).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    format!("operação desconhecida no log: {operacao:?}").into(),
                )
            })?;
            Ok(EventEnvelope {
                event_id: row.get(0)?,
                device_id: row.get(1)?,
                seq: row.get(2)?,
                universe_id: row.get(3)?,
                aggregate_type: row.get(4)?,
                aggregate_id: row.get(5)?,
                operation,
                payload: row.get(7)?,
                base_rev: row.get(8)?,
                new_rev: row.get(9)?,
                // Repassada intacta. O relay é carteiro, não autor.
                signature: row.get(10)?,
            })
        })
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;

    linhas
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))
}

/// Registra uma origem que apareceu no vetor do outro lado mas que este
/// aparelho ainda não conhecia.
///
/// Sem isto, a FK de `sync_events` recusaria os eventos dela e a propagação
/// transitiva pararia na primeira origem nova. A **decisão de confiar** é da
/// etapa 7 — aqui só se anota que ela existe, para o vetor poder citá-la.
pub fn registrar_origem_conhecida(
    connection: &Connection,
    device_id: &str,
    chave_publica: &str,
) -> DatabaseCommandResult<()> {
    let existe: bool = connection
        .query_row(
            "SELECT 1 FROM sync_devices WHERE device_id = ?1",
            [device_id],
            |_| Ok(true),
        )
        .optional()
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?
        .unwrap_or(false);
    if existe {
        return Ok(());
    }
    connection
        .execute(
            "INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
             VALUES (?1, '', ?2, 0)",
            rusqlite::params![device_id, chave_publica],
        )
        .map_err(|error| DatabaseCommandError::storage(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync_bootstrap;
    use crate::domain::identity::{verify, DeviceIdentity};
    use crate::domain::sync::AggregateRef;
    use crate::infrastructure::sqlite::sync_repository::{append_local_event, LocalChange};
    use crate::infrastructure::sqlite::sync_session::receber_eventos;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    /// Um aparelho de verdade: banco próprio, identidade própria, chave
    /// própria. Nada é compartilhado entre eles além dos eventos que trocam.
    struct Aparelho {
        nome: &'static str,
        banco: TemporaryDatabase,
        identidade: DeviceIdentity,
        _dados: std::path::PathBuf,
    }

    impl Drop for Aparelho {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self._dados);
        }
    }

    impl Aparelho {
        fn novo(nome: &'static str) -> Self {
            let banco = TemporaryDatabase::new();
            let dados =
                std::env::temp_dir().join(format!("narrahub-{nome}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dados).expect("criar diretório");
            let identidade = sync_bootstrap::prepare(&dados, &banco.database).expect("arranque");
            {
                let connection = banco.database.write().expect("abrir escrita");
                seed_universe(&connection, "u1");
                connection
                    .execute_batch(
                        "INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Historia');
                         INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');",
                    )
                    .expect("semear");
            }
            Self {
                nome,
                banco,
                identidade,
                _dados: dados,
            }
        }

        fn escrever(&self, id: &str, titulo: &str) -> EventEnvelope {
            let payload = format!(
                r#"{{"id":"{id}","book_id":"b1","title":"{titulo}","content":"texto","summary":"","scene_origin":"","scene_destination":"","word_count":1,"status":"rascunho","canon_status":"canon","sort_order":0,"created_at":"2026-01-01 00:00:00","updated_at":"2026-01-02 00:00:00"}}"#
            );
            let mut connection = self.banco.database.write().expect("abrir escrita");
            // A linha do agregado entra junto, como o serviço de domínio faz.
            // Sem isto o aparelho teria o evento e não teria o conteúdo — o
            // que denunciou, ao escrever este teste, que `create_chapter`
            // ainda não emite evento (ver NH-053).
            connection
                .execute(
                    "INSERT INTO chapters (id, book_id, title, content, word_count)
                     VALUES (?1, 'b1', ?2, 'texto', 1)",
                    rusqlite::params![id, titulo],
                )
                .expect("gravar capítulo local");
            append_local_event(
                &mut connection,
                &self.identidade,
                &LocalChange {
                    universe_id: "u1",
                    aggregate: AggregateRef::new("chapter", id),
                    operation: Operation::Upsert,
                    payload: &payload,
                },
            )
            .expect("escrever")
        }

        fn vetor(&self) -> VetorDeSequencias {
            let connection = self.banco.database.read().expect("abrir leitura");
            vetor_local(&connection).expect("vetor")
        }

        fn tem_capitulo(&self, id: &str) -> bool {
            let connection = self.banco.database.read().expect("abrir leitura");
            connection
                .query_row("SELECT COUNT(*) FROM chapters WHERE id = ?1", [id], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("contar")
                > 0
        }
    }

    /// Uma sessão simétrica: os dois trocam vetores e cada um manda o que o
    /// outro não tem. Nenhum lado é servidor.
    fn sincronizar(a: &Aparelho, b: &Aparelho) {
        // Cada lado precisa conhecer as origens que o outro cita, senão a FK
        // recusa os eventos. A confiança é da etapa 7; aqui só se anota.
        apresentar_origens(a, b);
        apresentar_origens(b, a);

        let vetor_a = a.vetor();
        let vetor_b = b.vetor();

        let para_b = {
            let connection = a.banco.database.read().expect("leitura");
            eventos_para(&connection, &vetor_b).expect("eventos de A para B")
        };
        let para_a = {
            let connection = b.banco.database.read().expect("leitura");
            eventos_para(&connection, &vetor_a).expect("eventos de B para A")
        };

        {
            let mut connection = b.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, &para_b).expect("B recebe");
        }
        {
            let mut connection = a.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, &para_a).expect("A recebe");
        }
    }

    /// Copia para `destino` as origens que `fonte` conhece.
    fn apresentar_origens(fonte: &Aparelho, destino: &Aparelho) {
        let conhecidas: Vec<(String, String)> = {
            let connection = fonte.banco.database.read().expect("leitura");
            let mut statement = connection
                .prepare("SELECT device_id, ed25519_public FROM sync_devices")
                .expect("preparar");
            let linhas = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .expect("consultar");
            linhas.collect::<Result<_, _>>().expect("ler")
        };
        let connection = destino.banco.database.write().expect("escrita");
        for (device_id, publica) in conhecidas {
            registrar_origem_conhecida(&connection, &device_id, &publica).expect("registrar");
        }
    }

    /// GATE DA ETAPA 6: propagação transitiva.
    ///
    /// O Desktop e o Android **nunca se encontram**. O conteúdo chega mesmo
    /// assim, carregado pelo Notebook — que é o que "store-and-forward"
    /// significa na prática.
    #[test]
    fn o_conteudo_atravessa_um_terceiro_que_so_serviu_de_ponte() {
        let desktop = Aparelho::novo("desktop");
        let notebook = Aparelho::novo("notebook");
        let android = Aparelho::novo("android");

        desktop.escrever("cap-1", "Escrito no Desktop");

        // Desktop encontra o Notebook.
        sincronizar(&desktop, &notebook);
        assert!(notebook.tem_capitulo("cap-1"), "o Notebook não recebeu");

        // Depois o Android encontra só o Notebook.
        sincronizar(&notebook, &android);
        assert!(
            android.tem_capitulo("cap-1"),
            "o Android não recebeu o que o Desktop escreveu — a ponte não funcionou"
        );

        // E o Android sabe que a ORIGEM é o Desktop, não o Notebook.
        let connection = android.banco.database.read().expect("leitura");
        let origem: String = connection
            .query_row(
                "SELECT device_id FROM sync_events WHERE aggregate_id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .expect("ler origem");
        assert_eq!(
            origem,
            desktop.identidade.device_id(),
            "o relay se sobrescreveu como origem"
        );
    }

    /// O relay repassa a assinatura ORIGINAL.
    ///
    /// Reassinar destruiria a única prova de quem escreveu, e o Android não
    /// teria como verificar um evento de um aparelho com quem nunca falou.
    #[test]
    fn o_relay_repassa_a_assinatura_da_origem_sem_reassinar() {
        let desktop = Aparelho::novo("desktop");
        let notebook = Aparelho::novo("notebook");
        let android = Aparelho::novo("android");

        let original = desktop.escrever("cap-1", "Do Desktop");

        sincronizar(&desktop, &notebook);
        sincronizar(&notebook, &android);

        let connection = android.banco.database.read().expect("leitura");
        let (assinatura, origem): (String, String) = connection
            .query_row(
                "SELECT signature, device_id FROM sync_events WHERE aggregate_id = 'cap-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ler evento");

        assert_eq!(
            assinatura, original.signature,
            "a assinatura mudou no caminho: o Notebook reassinou"
        );
        assert_eq!(origem, desktop.identidade.device_id());

        // E ela verifica contra a chave do DESKTOP, no aparelho que nunca
        // falou com ele.
        let recebido = EventEnvelope {
            signature: assinatura,
            ..original.clone()
        };
        assert!(
            verify(&recebido, &desktop.identidade.public_base32()),
            "o Android não consegue verificar a origem do evento que recebeu"
        );
    }

    /// Simetria: os dois lados trocam na mesma sessão.
    #[test]
    fn os_dois_lados_recebem_na_mesma_sessao() {
        let desktop = Aparelho::novo("desktop");
        let android = Aparelho::novo("android");

        desktop.escrever("do-desktop", "Escrito no PC");
        android.escrever("do-android", "Escrito no celular");

        sincronizar(&desktop, &android);

        assert!(
            desktop.tem_capitulo("do-android"),
            "o PC não recebeu do celular"
        );
        assert!(
            android.tem_capitulo("do-desktop"),
            "o celular não recebeu do PC"
        );
    }

    /// Agregados diferentes editados offline nos dois aparelhos convergem sem
    /// nenhum conflito — é o caso comum, e ele não pode incomodar ninguém.
    #[test]
    fn edicoes_em_agregados_diferentes_convergem_sem_conflito() {
        let desktop = Aparelho::novo("desktop");
        let android = Aparelho::novo("android");

        desktop.escrever("cap-a", "Capitulo A");
        desktop.escrever("cap-b", "Capitulo B");
        android.escrever("cap-c", "Capitulo C");

        sincronizar(&desktop, &android);

        for id in ["cap-a", "cap-b", "cap-c"] {
            assert!(desktop.tem_capitulo(id), "faltou {id} no Desktop");
            assert!(android.tem_capitulo(id), "faltou {id} no Android");
        }

        for aparelho in [&desktop, &android] {
            let connection = aparelho.banco.database.read().expect("leitura");
            let divergencias: i64 = connection
                .query_row("SELECT COUNT(*) FROM sync_divergences", [], |row| {
                    row.get(0)
                })
                .expect("contar");
            assert_eq!(
                divergencias, 0,
                "{} abriu divergência onde não havia conflito",
                aparelho.nome
            );
        }
    }

    /// Sincronizar de novo sem nada novo não move nada.
    #[test]
    fn sessao_repetida_sem_novidade_nao_faz_nada() {
        let desktop = Aparelho::novo("desktop");
        let android = Aparelho::novo("android");
        desktop.escrever("cap-1", "Um");

        sincronizar(&desktop, &android);
        let vetor_antes = android.vetor();

        sincronizar(&desktop, &android);
        assert_eq!(android.vetor(), vetor_antes, "o vetor andou sem novidade");

        let connection = android.banco.database.read().expect("leitura");
        let eventos: i64 = connection
            .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(eventos, 1, "o mesmo evento entrou duas vezes no log");
    }

    /// O que ainda está pendente **não** é repassado adiante.
    ///
    /// Repassar enviaria ao outro lado um evento cuja lacuna anterior este
    /// aparelho ainda não fechou; ele o guardaria como pendente também. Ruído
    /// em vez de progresso.
    #[test]
    fn pendente_nao_e_retransmitido() {
        let desktop = Aparelho::novo("desktop");
        let notebook = Aparelho::novo("notebook");
        let android = Aparelho::novo("android");

        desktop.escrever("cap-1", "Um");
        let segundo = desktop.escrever("cap-2", "Dois");
        desktop.escrever("cap-3", "Tres");

        apresentar_origens(&desktop, &notebook);

        // O Notebook recebe o 1 e o 3; o 2 se perdeu.
        let todos = {
            let connection = desktop.banco.database.read().expect("leitura");
            eventos_para(&connection, &VetorDeSequencias::new()).expect("todos")
        };
        let sem_o_meio: Vec<EventEnvelope> = todos
            .iter()
            .filter(|evento| evento.event_id != segundo.event_id)
            .cloned()
            .collect();
        {
            let mut connection = notebook.banco.database.write().expect("escrita");
            receber_eventos(&mut connection, &sem_o_meio).expect("receber");
        }

        // O Notebook agora fala com o Android.
        sincronizar(&notebook, &android);

        assert!(android.tem_capitulo("cap-1"));
        assert!(
            !android.tem_capitulo("cap-3"),
            "o Notebook repassou um evento que ele mesmo não conseguiu aplicar"
        );
    }
}
