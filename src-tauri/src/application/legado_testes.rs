//! **Gates da H-R3** — a caixa de recuperação do legado.
//!
//! O que eles protegem, em uma frase: a versão antiga que só existe numa linha de `sync_conflicts`
//! não pode sumir sem decisão do escritor, e a decisão dele não pode ficar pela metade.

use std::path::PathBuf;

use rusqlite::Connection;

use super::resolucao_testes::{sincronizar, Aparelho};
use crate::application::legado_recuperacao::{
    self, contar_pendentes, descartar, listar, preservar, PedidoDePreservacao,
};
use crate::application::{arranque, sync_bootstrap};
use crate::database::estado::{EstadoDoBanco, FaseDoBanco};
use crate::database::legado_v1::vigia::Vigia;
use crate::database::migrations::{sql_for_version, LATEST_SCHEMA_VERSION};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::connection::SqliteDatabase;

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

struct BancoAntigo {
    pasta: PathBuf,
    caminho: PathBuf,
}

impl Drop for BancoAntigo {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.pasta).ok();
    }
}

impl BancoAntigo {
    /// Um banco publicado (schema 15) com um conflito V1 aberto A/B, imagem inline dos dois lados.
    fn novo(conflitos: &[(&str, &str, &str)]) -> Self {
        let pasta = std::env::temp_dir().join(format!("narrahub-hr3-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&pasta).expect("pasta");
        let caminho = pasta.join("narrahub.db");
        let connection = Connection::open(&caminho).expect("criar");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("fk");
        let aplicar = |de: i64, ate: i64| {
            for versao in de..=ate {
                connection
                    .execute_batch(sql_for_version(versao).expect("migration"))
                    .unwrap_or_else(|e| panic!("v{versao}: {e}"));
            }
        };
        aplicar(1, 15);
        connection
            .execute_batch(
                "INSERT INTO universes (id, name) VALUES ('u1', 'Terra');
                 INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Saga');
                 INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro I');
                 INSERT INTO chapters (id, book_id, title, content)
                 VALUES ('cap-1', 'b1', 'Capítulo original', '<p>versão A, a materializada</p>');",
            )
            .expect("acervo");
        for (id, alvo, resolvido) in conflitos {
            connection
                .execute(
                    "INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value,
                         created_at, resolved_at)
                     VALUES (?1, 'chapter', ?2, 'content', ?3, ?4, '2025-02-02 10:00:00', ?5)",
                    rusqlite::params![
                        id,
                        alvo,
                        "<p>versão A, a materializada</p>",
                        format!("<p>versão B, que só existe aqui</p><img src=\"{PNG}\">"),
                        resolvido
                    ],
                )
                .expect("conflito V1");
        }
        aplicar(16, LATEST_SCHEMA_VERSION);
        connection
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
            .expect("versão");
        drop(connection);
        Self { pasta, caminho }
    }

    fn database(&self) -> SqliteDatabase {
        SqliteDatabase::new(&self.caminho)
    }

    /// Um arranque completo, como o do aplicativo: mídia, import do legado, adoção, `Ready`.
    fn arrancar(&self) -> usize {
        let database = self.database();
        let store = BlobStore::new(self.pasta.clone());
        let identidade = sync_bootstrap::prepare(&self.pasta, &database).expect("identidade");
        let estado = EstadoDoBanco::default();
        arranque::preparar_acervo(&database, &store, &identidade, &estado).expect("arranque");
        assert_eq!(estado.fase(), FaseDoBanco::Ready);
        let connection = database.read().expect("leitura");
        contar_pendentes(&connection).expect("contar") as usize
    }

    fn conexao(&self) -> Connection {
        let connection = Connection::open(&self.caminho).expect("abrir");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("fk");
        connection
    }
}

/// **H16 — a migration 29 sobe de 28 e o banco fica íntegro.**
#[test]
fn h16_migration_29_cria_a_caixa_e_mantem_a_integridade() {
    for inicio in [1, 28] {
        let pasta = std::env::temp_dir().join(format!("narrahub-h16-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&pasta).expect("pasta");
        let caminho = pasta.join("narrahub.db");
        let connection = Connection::open(&caminho).expect("criar");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("fk");
        for versao in 1..=inicio {
            connection
                .execute_batch(sql_for_version(versao).expect("migration"))
                .expect("aplicar");
        }
        if inicio == 28 {
            connection
                .execute_batch(sql_for_version(29).expect("migration 29"))
                .expect("28 → 29");
        } else {
            for versao in (inicio + 1)..=LATEST_SCHEMA_VERSION {
                connection
                    .execute_batch(sql_for_version(versao).expect("migration"))
                    .expect("aplicar");
            }
        }
        let existe: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'legacy_recovery_items'",
                [],
                |row| row.get(0),
            )
            .expect("consultar");
        assert_eq!(existe, 1, "a caixa não foi criada (início {inicio})");
        let integridade: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity_check");
        assert_eq!(integridade, "ok");
        let chaves: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("foreign_key_check");
        assert_eq!(chaves, 0);
        drop(connection);
        std::fs::remove_dir_all(&pasta).ok();
    }
}

/// **H17 — o import pega a versão B já convertida, e não toca na linha de origem.**
#[test]
fn h17_import_traz_b_convertido_e_preserva_a_origem() {
    let banco = BancoAntigo::novo(&[("c-1", "cap-1", "")]);
    assert_eq!(banco.arrancar(), 1);

    let connection = banco.conexao();
    let (antiga, origem_intacta): (String, String) = connection
        .query_row(
            "SELECT i.remote_value, c.remote_value
               FROM legacy_recovery_items i
               JOIN sync_conflicts c ON c.id = i.source_conflict_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("item e origem");
    assert!(
        antiga.contains("versão B"),
        "a versão antiga não veio: {antiga}"
    );
    assert!(
        !antiga.contains("data:image"),
        "o import copiou antes da conversão de mídia: {antiga}"
    );
    assert_eq!(
        antiga, origem_intacta,
        "o import alterou a linha histórica do V1"
    );
    let abertos: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE resolved_at = ''",
            [],
            |row| row.get(0),
        )
        .expect("contar");
    assert_eq!(abertos, 1, "o V1 foi alterado");
}

/// **H18 — dez arranques, uma pendência; e o que já foi decidido não volta a perguntar.**
#[test]
fn h18_import_e_idempotente_e_nao_reabre_decisao() {
    let banco = BancoAntigo::novo(&[("c-1", "cap-1", "")]);
    for _ in 0..5 {
        assert_eq!(banco.arrancar(), 1, "o import duplicou a pendência");
    }
    let connection = banco.conexao();
    let item: String = connection
        .query_row("SELECT id FROM legacy_recovery_items", [], |row| row.get(0))
        .expect("item");
    descartar(&banco.database(), &item).expect("descartar");

    for _ in 0..5 {
        assert_eq!(banco.arrancar(), 0, "a decisão voltou a ser pendência");
    }
    let (total, status): (i64, String) = banco
        .conexao()
        .query_row(
            "SELECT COUNT(*), MIN(status) FROM legacy_recovery_items",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("estado");
    assert_eq!((total, status.as_str()), (1, "discarded"));
}

/// **H19 — conflito V1 já resolvido no passado não vira pendência.**
#[test]
fn h19_conflito_v1_resolvido_nao_vira_pendencia() {
    let banco = BancoAntigo::novo(&[("c-1", "cap-1", "2025-03-03 10:00:00")]);
    assert_eq!(banco.arrancar(), 0);
    let total: i64 = banco
        .conexao()
        .query_row("SELECT COUNT(*) FROM legacy_recovery_items", [], |row| {
            row.get(0)
        })
        .expect("contar");
    assert_eq!(total, 0, "decisão antiga virou pergunta nova");
}

/// **H26 — banco sem legado V1 não ganha caixa nenhuma.**
#[test]
fn h26_banco_sem_legado_nao_tem_recuperacao() {
    let banco = BancoAntigo::novo(&[]);
    assert_eq!(banco.arrancar(), 0);
    let a = Aparelho::novo();
    assert_eq!(
        contar_pendentes(&a.banco.connection()).expect("contar"),
        0,
        "um aparelho novo não tem legado para recuperar"
    );
}

/// **H21 — preservar cria capítulo novo, que sincroniza como qualquer conteúdo.**
#[test]
fn h21_preservar_cria_capitulo_que_viaja_no_sync() {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);
    let item = semear_item(&a, &capitulo, "<p>versão antiga que precisa sobreviver</p>");
    let livro: String = a
        .banco
        .connection()
        .query_row(
            "SELECT book_id FROM chapters WHERE id = ?1",
            [&capitulo],
            |row| row.get(0),
        )
        .expect("livro");

    let novo = preservar(
        &a.banco.database,
        &a.eu,
        &PedidoDePreservacao {
            id: item.clone(),
            book_id: livro,
            titulo: "Capítulo original — versão recuperada".into(),
        },
    )
    .expect("preservar");
    assert_ne!(novo, capitulo, "o capítulo recuperado reusou o id original");

    let (status, preservado): (String, String) = a
        .banco
        .connection()
        .query_row(
            "SELECT status, preserved_chapter_id FROM legacy_recovery_items WHERE id = ?1",
            [&item],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("item");
    assert_eq!(
        (status.as_str(), preservado.as_str()),
        ("preserved", novo.as_str())
    );

    sincronizar(&a, &b);
    assert_eq!(
        b.conteudo(&novo).as_deref(),
        Some("<p>versão antiga que precisa sobreviver</p>"),
        "o capítulo recuperado não chegou ao outro aparelho"
    );
}

/// **H22 — queda no meio da preservação: nem capítulo órfão, nem status falso.**
#[test]
fn h22_queda_na_preservacao_nao_deixa_nada_pela_metade() {
    let a = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    let item = semear_item(&a, &capitulo, "<p>versão antiga</p>");
    let livro: String = a
        .banco
        .connection()
        .query_row(
            "SELECT book_id FROM chapters WHERE id = ?1",
            [&capitulo],
            |row| row.get(0),
        )
        .expect("livro");
    let pedido = PedidoDePreservacao {
        id: item.clone(),
        book_id: livro,
        titulo: "Recuperado".into(),
    };

    for ponto in [
        crate::application::mutacao::falha::Ponto::AposAMutacao,
        crate::application::mutacao::falha::Ponto::DuranteOsEventos,
        crate::application::mutacao::falha::Ponto::AntesDoCommit,
    ] {
        crate::application::mutacao::falha::armar(Some(ponto));
        let erro = preservar(&a.banco.database, &a.eu, &pedido);
        crate::application::mutacao::falha::armar(None);
        assert!(erro.is_err(), "{ponto:?}: a queda não derrubou nada");
        let (capitulos, status): (i64, String) = a
            .banco
            .connection()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM chapters),
                        (SELECT status FROM legacy_recovery_items WHERE id = ?1)",
                [&item],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("estado");
        assert_eq!(capitulos, 1, "{ponto:?}: sobrou capítulo órfão");
        assert_eq!(
            status, "pending",
            "{ponto:?}: o item mentiu sobre a decisão"
        );
    }

    // Sem a queda, entra inteira — e uma só vez.
    preservar(&a.banco.database, &a.eu, &pedido).expect("preservar");
}

/// **H23 — preservar duas vezes não cria duas cópias.**
#[test]
fn h23_preservar_duas_vezes_nao_duplica() {
    let a = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    let item = semear_item(&a, &capitulo, "<p>versão antiga</p>");
    let livro: String = a
        .banco
        .connection()
        .query_row(
            "SELECT book_id FROM chapters WHERE id = ?1",
            [&capitulo],
            |row| row.get(0),
        )
        .expect("livro");
    let pedido = PedidoDePreservacao {
        id: item,
        book_id: livro,
        titulo: "Recuperado".into(),
    };
    preservar(&a.banco.database, &a.eu, &pedido).expect("primeira");
    let erro = preservar(&a.banco.database, &a.eu, &pedido).expect_err("a segunda tem de recusar");
    assert!(
        erro.message.contains("já foi resolvida"),
        "{}",
        erro.message
    );
    let capitulos: i64 = a
        .banco
        .connection()
        .query_row("SELECT COUNT(*) FROM chapters", [], |row| row.get(0))
        .expect("contar");
    assert_eq!(capitulos, 2, "a repetição criou uma segunda cópia");
}

/// **H24 — descartar tira a pendência e deixa a linha histórica intacta.**
#[test]
fn h24_descartar_nao_toca_no_legado_v1() {
    let banco = BancoAntigo::novo(&[("c-1", "cap-1", "")]);
    banco.arrancar();
    let connection = banco.conexao();
    let item: String = connection
        .query_row("SELECT id FROM legacy_recovery_items", [], |row| row.get(0))
        .expect("item");
    let antes: (String, String) = connection
        .query_row(
            "SELECT remote_value, resolved_at FROM sync_conflicts WHERE id = 'c-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("origem");
    drop(connection);

    descartar(&banco.database(), &item).expect("descartar");
    let connection = banco.conexao();
    let (status, resolvido): (String, String) = connection
        .query_row(
            "SELECT status, resolved_at FROM legacy_recovery_items WHERE id = ?1",
            [&item],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("item");
    assert_eq!(status, "discarded");
    assert!(!resolvido.is_empty(), "o descarte não datou a decisão");
    let depois: (String, String) = connection
        .query_row(
            "SELECT remote_value, resolved_at FROM sync_conflicts WHERE id = 'c-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("origem");
    assert_eq!(antes, depois, "o descarte mexeu no legado do V1");
    assert!(
        contar_pendentes(&connection).expect("contar") == 0,
        "a pendência continuou na caixa"
    );
}

/// **H25 — backup e restauração preservam a caixa inteira, com os estados.**
#[test]
fn h25_backup_e_restauracao_preservam_a_caixa() {
    let banco = BancoAntigo::novo(&[("c-1", "cap-1", ""), ("c-2", "cap-1", "")]);
    banco.arrancar();
    let connection = banco.conexao();
    let itens: Vec<String> = {
        let mut consulta = connection
            .prepare("SELECT id FROM legacy_recovery_items ORDER BY source_conflict_id")
            .expect("consulta");
        let linhas = consulta
            .query_map([], |row| row.get(0))
            .expect("linhas")
            .collect::<Result<Vec<String>, _>>()
            .expect("coletar");
        linhas
    };
    drop(connection);
    descartar(&banco.database(), &itens[0]).expect("descartar um");

    let backups = banco.pasta.join("backups");
    std::fs::create_dir_all(&backups).expect("pasta de backup");
    let manifesto = crate::database::backup::create_backup_at(
        &banco.caminho,
        None,
        &backups,
        "0.10.0-beta.2",
        crate::database::backup::BackupReason::Manual,
    )
    .expect("backup");

    // A restauração é a cópia do snapshot de volta — o que importa aqui é o conteúdo do arquivo.
    let restaurado = banco.pasta.join("restaurado.db");
    let no_backup = backups
        .join(&manifesto.backup_id)
        .join(&manifesto.database.file);
    std::fs::copy(&no_backup, &restaurado).expect("restaurar");
    let connection = Connection::open(&restaurado).expect("abrir restaurado");
    let mut consulta = connection
        .prepare("SELECT status FROM legacy_recovery_items ORDER BY source_conflict_id")
        .expect("consulta");
    let estados: Vec<String> = consulta
        .query_map([], |row| row.get(0))
        .expect("linhas")
        .collect::<Result<_, _>>()
        .expect("coletar");
    assert_eq!(
        estados,
        vec!["discarded".to_string(), "pending".to_string()],
        "o backup não levou a caixa com os estados"
    );
}

/// **H27 — depois de `Ready`, ninguém lê nem escreve `sync_conflicts`.**
///
/// O importador é a única leitura nova, e ela acontece ANTES de `Ready`. O vigia do autorizador do
/// SQLite (etapa G) é armado depois do arranque e acompanha listar, preservar e descartar.
#[test]
fn h27_depois_do_ready_a_caixa_nao_toca_no_legado() {
    let banco = BancoAntigo::novo(&[("c-1", "cap-1", "")]);
    assert_eq!(banco.arrancar(), 1);
    let database = banco.database();
    let identidade = sync_bootstrap::prepare(&banco.pasta, &database).expect("identidade");

    let vigia = Vigia::armar(&[banco.caminho.as_path()]);
    let connection = database.read().expect("leitura");
    let itens = listar(&connection).expect("listar");
    assert_eq!(itens.len(), 1);
    let destinos = legado_recuperacao::destinos(&connection, &itens[0].id).expect("destinos");
    assert!(
        destinos.iter().any(|destino| destino.e_o_livro_original),
        "o livro do capítulo original tinha de vir marcado"
    );
    drop(connection);

    preservar(
        &database,
        &identidade,
        &PedidoDePreservacao {
            id: itens[0].id.clone(),
            book_id: destinos[0].book_id.clone(),
            titulo: "Recuperado".into(),
        },
    )
    .expect("preservar");

    let acessos = vigia.acessos();
    drop(vigia);
    assert!(
        acessos.is_empty(),
        "a caixa tocou no legado do V1 depois de Ready: {acessos:?}"
    );
}

/// Um item de recuperação semeado direto, para os gates que não precisam do banco antigo inteiro.
fn semear_item(aparelho: &Aparelho, capitulo: &str, versao_antiga: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    aparelho
        .banco
        .connection()
        .execute(
            "INSERT INTO legacy_recovery_items
                (id, source_conflict_id, aggregate_type, aggregate_id, field,
                 local_value, remote_value, source_created_at)
             VALUES (?1, ?2, 'chapter', ?3, 'content', '<p>atual</p>', ?4, '2025-02-02 10:00:00')",
            rusqlite::params![&id, format!("c-{id}"), capitulo, versao_antiga],
        )
        .expect("semear item");
    id
}
