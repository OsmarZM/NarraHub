//! **Gates da E0-beta** sobre os bancos REAIS da 0.10.0-beta.2 (`fixtures/beta2`).
//!
//! Os bancos foram gerados pelo código da beta.2 (dois aparelhos pareados, create/edit/delete,
//! evento recebido) e são carregados aqui como a instalação de alguém que atualiza: schema 20,
//! identidade e blobs da beta, migrations 21..atual como o plugin as aplica, e o arranque.

use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

use crate::application::arranque::{preparar_acervo, ResumoDoAcervo};
use crate::application::epoca::{self, falha, TABELAS_DO_PASSADO};
use crate::application::sync_bootstrap;
use crate::application::sync_sessao::{
    atender_conexao, parear_por_pin, sincronizar_com, Contexto, Papel,
};
use crate::database::estado::{EstadoDoBanco, FaseDoBanco};
use crate::database::migrations::{sql_for_version, LATEST_SCHEMA_VERSION};
use crate::domain::identity::DeviceIdentity;
use crate::domain::sync::{AggregateRef, EventEnvelope};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::identity_store;
use crate::infrastructure::sqlite::sync_codec;
use crate::infrastructure::sqlite::sync_exchange::{eventos_para, vetor_local};
use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
use crate::infrastructure::sqlite::SqliteDatabase;
use crate::infrastructure::sync_pake::Codigos;

const DUMP_A: &str = include_str!("../../fixtures/beta2/a.sql");
const DUMP_B: &str = include_str!("../../fixtures/beta2/b.sql");
const IDS: &str = include_str!("../../fixtures/beta2/ids.txt");

fn ids() -> BTreeMap<String, String> {
    IDS.lines()
        .filter_map(|linha| linha.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn id(chave: &str) -> String {
    ids()
        .get(chave)
        .cloned()
        .unwrap_or_else(|| panic!("ids.txt sem {chave}"))
}

fn copiar_dir(de: &Path, para: &Path) {
    std::fs::create_dir_all(para).expect("dir");
    for entrada in std::fs::read_dir(de).expect("ler dir").flatten() {
        let destino = para.join(entrada.file_name());
        if entrada.path().is_dir() {
            copiar_dir(&entrada.path(), &destino);
        } else {
            std::fs::copy(entrada.path(), destino).expect("copiar");
        }
    }
}

/// Uma instalação da beta.2 no instante em que o aplicativo novo abre pela primeira vez.
struct Instalacao {
    dir: PathBuf,
    database: SqliteDatabase,
    store: BlobStore,
    nome: &'static str,
}

impl Drop for Instalacao {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Instalacao {
    /// A beta, migrada para o schema atual como o plugin faz — e ainda sem arranque.
    fn da_beta(nome: &'static str) -> Self {
        let dir = std::env::temp_dir().join(format!("narrahub-e0-{nome}-{}", uuid::Uuid::new_v4()));
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/beta2");
        copiar_dir(&fixtures.join(format!("dados-{nome}")), &dir);
        let caminho = dir.join("narrahub.db");
        {
            let connection = Connection::open(&caminho).expect("abrir");
            // O dump do `iterdump` cria as tabelas em ordem alfabética e não desliga as FKs; o
            // SQLite embutido as liga por padrão. Só durante a carga.
            connection
                .execute_batch("PRAGMA foreign_keys = OFF;")
                .expect("fk off");
            connection
                .execute_batch(if nome == "a" { DUMP_A } else { DUMP_B })
                .expect("carregar o banco da beta.2");
            connection
                .pragma_update(None, "user_version", 20)
                .expect("versão da beta");
            connection
                .execute_batch("PRAGMA foreign_keys = ON;")
                .expect("fk");
            for versao in 21..=LATEST_SCHEMA_VERSION {
                connection
                    .execute_batch(sql_for_version(versao).expect("migration"))
                    .unwrap_or_else(|erro| panic!("migration {versao}: {erro}"));
                connection
                    .pragma_update(None, "user_version", versao)
                    .expect("versão");
            }
        }
        Self {
            database: SqliteDatabase::new(caminho),
            store: BlobStore::new(dir.clone()),
            dir,
            nome,
        }
    }

    /// O arranque do aplicativo: identidade e acervo, nesta ordem.
    fn arrancar(&self) -> (EstadoDoBanco, Result<ResumoDoAcervo, String>) {
        let estado = EstadoDoBanco::default();
        let resultado = sync_bootstrap::prepare(&self.dir, &self.database)
            .and_then(|identidade| {
                preparar_acervo(&self.database, &self.store, &identidade, &estado)
            })
            .map_err(|erro| erro.message);
        (estado, resultado)
    }

    fn identidade(&self) -> DeviceIdentity {
        sync_bootstrap::prepare(&self.dir, &self.database).expect("identidade")
    }

    fn arquivo_de_identidade(&self) -> String {
        identity_store::load_or_create(&self.dir)
            .expect("arquivo")
            .device_id()
            .to_string()
    }

    fn conn(&self) -> Connection {
        self.database.write().expect("conexão")
    }

    fn contar(&self, sql: &str) -> i64 {
        self.conn()
            .query_row(sql, [], |row| row.get(0))
            .unwrap_or_else(|erro| panic!("{sql}: {erro}"))
    }

    fn ctx<'a>(&'a self, identidade: &'a DeviceIdentity) -> Contexto<'a> {
        Contexto {
            database: &self.database,
            store: &self.store,
            identidade,
            nome_local: self.nome,
            espera: Duration::from_secs(30),
        }
    }

    fn eventos(&self) -> Vec<EventEnvelope> {
        let connection = self.conn();
        eventos_para(&connection, &BTreeMap::new()).expect("log")
    }

    fn capitulos(&self) -> BTreeMap<String, String> {
        let connection = self.conn();
        let mut consulta = connection
            .prepare("SELECT id, content FROM chapters")
            .expect("capítulos");
        let linhas = consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("linhas");
        linhas.collect::<Result<_, _>>().expect("ler")
    }

    /// Cada linha de cada tabela e cada arquivo do diretório de dados.
    fn retrato(&self) -> Vec<String> {
        let connection = self.conn();
        let tabelas: Vec<String> = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("tabelas")
            .query_map([], |row| row.get(0))
            .expect("listar")
            .collect::<Result<_, _>>()
            .expect("ler");
        let mut foto = Vec::new();
        for tabela in tabelas {
            let mut consulta = connection
                .prepare(&format!("SELECT * FROM \"{tabela}\""))
                .expect("select");
            let colunas = consulta.column_count();
            let mut linhas: Vec<String> = consulta
                .query_map([], |row| {
                    let mut linha = Vec::new();
                    for i in 0..colunas {
                        linha.push(format!("{:?}", row.get::<_, rusqlite::types::Value>(i)?));
                    }
                    Ok(linha.join("|"))
                })
                .expect("linhas")
                .collect::<Result<_, _>>()
                .expect("ler");
            linhas.sort();
            foto.push(format!("## {tabela}"));
            foto.extend(linhas);
        }
        fn arquivos(dir: &Path, foto: &mut Vec<String>) {
            let mut entradas: Vec<_> = std::fs::read_dir(dir).expect("dir").flatten().collect();
            entradas.sort_by_key(|e| e.path());
            for entrada in entradas {
                let caminho = entrada.path();
                if caminho.is_dir() {
                    arquivos(&caminho, foto);
                } else if caminho.extension().and_then(|e| e.to_str()) != Some("db")
                    && !caminho.to_string_lossy().ends_with("-wal")
                    && !caminho.to_string_lossy().ends_with("-shm")
                {
                    let bytes = std::fs::read(&caminho).unwrap_or_default();
                    foto.push(format!(
                        "arquivo {} {}",
                        caminho.display(),
                        crate::infrastructure::blob_store::hash_dos_bytes(&bytes)
                    ));
                }
            }
        }
        arquivos(&self.dir, &mut foto);
        foto
    }
}

/// Um aparelho novo do protocolo 1, sem passado.
struct Novo {
    banco: TemporaryDatabase,
    dir: PathBuf,
    store: BlobStore,
    eu: DeviceIdentity,
}

impl Drop for Novo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Novo {
    fn criar() -> Self {
        let dir = std::env::temp_dir().join(format!("narrahub-e0-novo-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("dir");
        let banco = TemporaryDatabase::new();
        let eu = sync_bootstrap::prepare(&dir, &banco.database).expect("arranque");
        Self {
            store: BlobStore::new(dir.clone()),
            banco,
            dir,
            eu,
        }
    }
}

/// Apresenta `quem` ao roster de `a` — o que o pareamento faria, sem a rede.
fn apresentar(a: &Connection, a_eu: &DeviceIdentity, quem: &DeviceIdentity) {
    let sessao = crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(a_eu);
    crate::infrastructure::sqlite::sync_trust::introduzir_dispositivo(
        a,
        &sessao,
        quem.device_id(),
        &quem.public_base32(),
    )
    .expect("introduzir");
}

fn rotacionada(instalacao: &Instalacao) -> DeviceIdentity {
    let (estado, resultado) = instalacao.arrancar();
    let resumo = resultado.expect("o arranque da beta atualizada");
    assert_eq!(estado.fase(), FaseDoBanco::Ready, "{resumo:?}");
    assert!(resumo.sincronizacao_disponivel, "{resumo:?}");
    assert!(
        resumo.pareamentos_invalidados,
        "a atualização da beta não girou a época"
    );
    instalacao.identidade()
}

/// **E0-beta — a atualização gira a identidade, arquiva o passado inteiro e refaz a gênese.**
#[test]
fn beta_atualizada_gira_a_epoca_e_arquiva_o_passado() {
    let a = Instalacao::da_beta("a");
    let antiga_a = id("a");
    let antiga_b = id("b");
    assert_eq!(
        a.arquivo_de_identidade(),
        antiga_a,
        "a fixture não é a beta de A"
    );

    let mut antes: BTreeMap<&str, i64> = BTreeMap::new();
    for tabela in TABELAS_DO_PASSADO {
        antes.insert(tabela, a.contar(&format!("SELECT COUNT(*) FROM {tabela}")));
    }
    assert_eq!(
        antes["sync_events"], 8,
        "a fixture tem os 8 envelopes da beta"
    );
    let dominio = [
        "universes",
        "stories",
        "books",
        "chapters",
        "attachments",
        "chapter_revisions",
    ];
    let dominio_antes: Vec<i64> = dominio
        .iter()
        .map(|t| a.contar(&format!("SELECT COUNT(*) FROM {t}")))
        .collect();

    let nova = rotacionada(&a);

    // O acervo do escritor não perdeu nem ganhou uma linha, e o banco está íntegro.
    let dominio_depois: Vec<i64> = dominio
        .iter()
        .map(|t| a.contar(&format!("SELECT COUNT(*) FROM {t}")))
        .collect();
    assert_eq!(
        dominio_depois, dominio_antes,
        "a rotação mexeu no acervo: {dominio:?}"
    );
    let integridade: String = a
        .conn()
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity_check");
    assert_eq!(integridade, "ok");
    assert_eq!(
        a.conn()
            .prepare("PRAGMA foreign_key_check")
            .expect("fk")
            .query_map([], |_| Ok(()))
            .expect("fk")
            .count(),
        0,
        "foreign_key_check sujo depois da rotação"
    );

    // Identidade: nova, no arquivo e no banco, e sem sobra do arquivo da próxima.
    assert_ne!(nova.device_id(), antiga_a);
    assert_eq!(a.arquivo_de_identidade(), nova.device_id());
    assert!(!identity_store::next_identity_path(&a.dir).exists());
    let (origem, anterior, dono): (String, String, String) = a
        .conn()
        .query_row(
            "SELECT origem, identidade_anterior, device_id FROM sync_epoca WHERE protocolo = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("época marcada");
    assert_eq!(
        (origem.as_str(), anterior.as_str()),
        ("rotacao", antiga_a.as_str())
    );
    assert_eq!(dono, nova.device_id());

    // Roster: só o self novo. Os pareamentos da beta deixaram de existir.
    assert_eq!(
        a.contar("SELECT COUNT(*) FROM sync_devices"),
        1,
        "sobrou roster da beta"
    );
    assert_eq!(
        a.contar("SELECT COUNT(*) FROM sync_devices WHERE is_self = 1"),
        1
    );

    // O passado inteiro está em sync_legado, linha por linha — inclusive o relay de B.
    for tabela in TABELAS_DO_PASSADO {
        assert_eq!(
            a.contar(&format!(
                "SELECT COUNT(*) FROM sync_legado WHERE tabela = '{tabela}'"
            )),
            antes[tabela],
            "{tabela}: o arquivo não tem o passado inteiro"
        );
    }

    // Nenhum envelope do passado no log vivo.
    for envelope in a.eventos() {
        assert_eq!(
            envelope.device_id,
            nova.device_id(),
            "origem do passado no log"
        );
        assert!(
            !envelope.grupo.mutation_id.is_empty(),
            "envelope sem grupo no log"
        );
        assert_ne!(envelope.device_id, antiga_b);
    }

    // Toda revisão corrente descreve o banco.
    let connection = a.conn();
    let mut consulta = connection
        .prepare("SELECT aggregate_type, aggregate_id FROM sync_aggregate_state")
        .expect("estados");
    let estados: Vec<(String, String)> = consulta
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("estados")
        .collect::<Result<_, _>>()
        .expect("ler");
    assert!(!estados.is_empty());
    for (tipo, id) in &estados {
        let agregado = AggregateRef::new(tipo, id);
        let canonico = sync_codec::ler_canonico(&connection, &agregado)
            .expect("ler")
            .unwrap_or_else(|| panic!("{tipo} {id}: estado sem domínio"))
            .payload;
        assert_eq!(
            sync_codec::payload_da_revisao_corrente(&connection, &agregado).expect("payload"),
            Some(canonico),
            "{tipo} {id}: a revisão corrente não descreve o banco"
        );
    }
    drop(consulta);
    drop(connection);

    // As exclusões com prova viraram delete-genesis — e só elas.
    let deletes: BTreeSet<(String, String)> = a
        .eventos()
        .into_iter()
        .filter(|e| e.operation == crate::domain::sync::Operation::Delete)
        .map(|e| {
            assert_eq!(e.base_rev, "", "delete-genesis parte da raiz");
            (e.aggregate_type, e.aggregate_id)
        })
        .collect();
    assert_eq!(
        deletes,
        BTreeSet::from([
            ("attachment".to_string(), id("x2_apagado")),
            ("chapter".to_string(), id("c3_apagado")),
        ]),
        "tombstone de X2 e estado sem domínio de C3 — e nenhuma exclusão sem prova"
    );
    assert_eq!(a.contar("SELECT exclusoes_preservadas FROM sync_epoca"), 2);
}

/// **Idempotente:** abrir de novo não muda um byte, nem gira de novo.
#[test]
fn a_rotacao_e_idempotente() {
    let a = Instalacao::da_beta("a");
    let nova = rotacionada(&a);
    let depois = a.retrato();

    let (estado, resultado) = a.arrancar();
    let resumo = resultado.expect("segundo arranque");
    assert_eq!(estado.fase(), FaseDoBanco::Ready);
    assert!(!resumo.pareamentos_invalidados, "girou de novo");
    assert_eq!(a.identidade().device_id(), nova.device_id());
    assert_eq!(a.retrato(), depois, "o segundo arranque mudou o estado");
}

/// **A regra de saída:** nenhum envelope da beta sai pelo protocolo 1, e o que sai aplica inteiro
/// num aparelho novo em papel Par — sem lacuna, sem base desconhecida, sem decisão.
#[test]
fn nenhum_envelope_do_passado_sai_pelo_protocolo_1() {
    let a = Instalacao::da_beta("a");
    let nova = rotacionada(&a);
    let z = Novo::criar();
    apresentar(&z.banco.connection(), &z.eu, &nova);

    let vetor_z = vetor_local(&z.banco.connection()).expect("vetor");
    let lote = eventos_para(&a.conn(), &vetor_z).expect("lote");
    assert!(!lote.is_empty());
    for envelope in &lote {
        assert_eq!(
            envelope.device_id,
            nova.device_id(),
            "saiu envelope de outra origem"
        );
        assert!(
            !envelope.grupo.mutation_id.is_empty(),
            "saiu envelope pré-Hello"
        );
    }

    // Os blobs que o lote cita, pelo caminho verificado.
    let citados: BTreeSet<String> = lote
        .iter()
        .flat_map(sync_codec::midia::blobs_do_evento)
        .collect();
    crate::infrastructure::sqlite::blob_backfill::transferir_blobs(
        crate::infrastructure::sqlite::blob_backfill::origem_local(&a.store),
        &z.store,
        &citados,
    )
    .expect("blobs");

    let relatorio = crate::infrastructure::sqlite::sync_session::receber_eventos(
        &mut z.banco.connection(),
        &lote,
        &z.store,
    )
    .expect("Z recebe");
    assert_eq!(relatorio.aplicados, lote.len(), "{relatorio:?}");
    assert_eq!(relatorio.pendentes, 0, "{relatorio:?}");
    assert_eq!(relatorio.divergencias, 0, "{relatorio:?}");
    assert!(relatorio.precisam_reconciliar.is_empty(), "{relatorio:?}");

    let capitulos_z: BTreeMap<String, String> = {
        let connection = z.banco.connection();
        let mut consulta = connection
            .prepare("SELECT id, content FROM chapters")
            .expect("capítulos");
        let linhas = consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("linhas");
        linhas.collect::<Result<_, _>>().expect("ler")
    };
    assert_eq!(capitulos_z, a.capitulos(), "Z não convergiu com A");
}

/// **Os dois aparelhos da beta, atualizados:** o pareamento antigo não vale; pareados de novo por
/// PIN (os dois com acervo, papel Par), convergem sem nenhuma decisão.
#[test]
fn a_e_b_da_beta_atualizados_pareiam_de_novo_e_convergem_sem_decisao() {
    let a = Instalacao::da_beta("a");
    let b = Instalacao::da_beta("b");
    let nova_a = rotacionada(&a);
    let nova_b = rotacionada(&b);

    // O pareamento da beta deixou de existir dos dois lados.
    let escuta = TcpListener::bind("127.0.0.1:0").expect("porta");
    let endereco = escuta.local_addr().expect("endereço").to_string();
    let mut codigos = Codigos::default();
    let (em_a, em_b) = std::thread::scope(|escopo| {
        let servidor = escopo.spawn(|| {
            let (mut fluxo, _) = escuta.accept().expect("aceita");
            atender_conexao(&mut fluxo, &mut codigos, &a.ctx(&nova_a))
        });
        let em_b = sincronizar_com(&endereco, &b.ctx(&nova_b));
        (servidor.join().expect("thread"), em_b)
    });
    assert!(
        em_a.is_err() && em_b.is_err(),
        "o pareamento da beta sobreviveu"
    );

    // Pareia de novo, por PIN.
    let (_, legivel) = codigos.emitir();
    let pin: String = legivel.chars().filter(|c| c.is_ascii_digit()).collect();
    let (em_a, em_b) = std::thread::scope(|escopo| {
        let servidor = escopo.spawn(|| {
            let (mut fluxo, _) = escuta.accept().expect("aceita");
            atender_conexao(&mut fluxo, &mut codigos, &a.ctx(&nova_a))
        });
        let em_b = parear_por_pin(&endereco, &pin, &b.ctx(&nova_b));
        (servidor.join().expect("thread"), em_b)
    });
    let em_a = em_a.expect("A: pareamento");
    let em_b = em_b.expect("B: pareamento");
    assert_eq!((em_a.papel, em_b.papel), (Papel::Par, Papel::Par));

    for (quem, instalacao) in [("A", &a), ("B", &b)] {
        assert_eq!(
            instalacao.contar("SELECT COUNT(*) FROM sync_divergences"),
            0,
            "{quem}: o mesmo passado virou decisão"
        );
        assert_eq!(
            instalacao.contar(
                "SELECT COUNT(*) FROM sync_events e
                  LEFT JOIN sync_applied_events x ON x.event_id = e.event_id
                  WHERE x.event_id IS NULL"
            ),
            0,
            "{quem}: ficou pendente"
        );
        for envelope in instalacao.eventos() {
            assert!(
                envelope.device_id == nova_a.device_id()
                    || envelope.device_id == nova_b.device_id(),
                "{quem}: envelope de origem da beta no log"
            );
        }
    }
    assert_eq!(a.capitulos(), b.capitulos(), "A e B não convergiram");
}

/// **Capítulo apagado na beta não ressuscita.** Um aparelho do protocolo 1 que ainda o tenha
/// recebe a exclusão como decisão; o aparelho que apagou recebe a gênese dele como decisão. Nenhum
/// dos dois perde nem ganha o capítulo em silêncio.
#[test]
fn capitulo_apagado_na_beta_nao_ressuscita_em_silencio() {
    let a = Instalacao::da_beta("a");
    let nova_a = rotacionada(&a);
    let c3 = id("c3_apagado");

    // Z tem o mesmo universo, história e livro de A — e ainda tem C3.
    let z = Novo::criar();
    {
        let origem = a.conn();
        let destino = z.banco.connection();
        for (tabela, chave) in [
            ("universes", id("universe")),
            ("stories", id("story")),
            ("books", id("book")),
        ] {
            copiar_linha(&origem, &destino, tabela, &chave);
        }
        destino
            .execute(
                "INSERT INTO chapters (id, book_id, title, content, sort_order)
                 VALUES (?1, ?2, 'C3', '<p>C3 que Z ainda tem</p>', 9)",
                [&c3, &id("book")],
            )
            .expect("C3 em Z");
    }
    crate::application::genese::adotar(&z.banco.database, &z.eu).expect("Z adota");
    apresentar(&z.banco.connection(), &z.eu, &nova_a);
    apresentar(&a.conn(), &nova_a, &z.eu);

    // A → Z: o delete-genesis de C3 chega a quem ainda o tem.
    let para_z =
        eventos_para(&a.conn(), &vetor_local(&z.banco.connection()).expect("v")).expect("a→z");
    crate::infrastructure::sqlite::sync_session::receber_eventos_sem_conferir_blobs(
        &mut z.banco.connection(),
        &para_z,
    )
    .expect("Z recebe");
    // Z → A: a gênese de C3 chega a quem o apagou.
    let para_a =
        eventos_para(&z.banco.connection(), &vetor_local(&a.conn()).expect("v")).expect("z→a");
    crate::infrastructure::sqlite::sync_session::receber_eventos_sem_conferir_blobs(
        &mut a.conn(),
        &para_a,
    )
    .expect("A recebe");

    assert!(
        !a.capitulos().contains_key(&c3),
        "C3, apagado na beta, voltou em A sem ninguém decidir"
    );
    let em_z: i64 = z
        .banco
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM chapters WHERE id = ?1",
            [&c3],
            |row| row.get(0),
        )
        .expect("C3 em Z");
    assert_eq!(em_z, 1, "Z perdeu C3 sem ninguém decidir");
    let decisao = |connection: &Connection| -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM sync_divergences
                  WHERE aggregate_type = 'chapter' AND aggregate_id = ?1 AND resolved_at = ''",
                [&c3],
                |row| row.get(0),
            )
            .expect("decisão")
    };
    assert_eq!(decisao(&a.conn()), 1, "A não registrou a decisão sobre C3");
    assert_eq!(
        decisao(&z.banco.connection()),
        1,
        "Z não registrou a decisão sobre C3"
    );
}

fn copiar_linha(origem: &Connection, destino: &Connection, tabela: &str, id: &str) {
    let mut consulta = origem
        .prepare(&format!("SELECT * FROM {tabela} WHERE id = ?1"))
        .expect("select");
    let colunas: Vec<String> = consulta
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let valores: Vec<rusqlite::types::Value> = consulta
        .query_row([id], |row| {
            (0..colunas.len())
                .map(|i| row.get::<_, rusqlite::types::Value>(i))
                .collect()
        })
        .expect("linha");
    let marcadores: Vec<String> = (1..=colunas.len()).map(|i| format!("?{i}")).collect();
    destino
        .execute(
            &format!(
                "INSERT INTO {tabela} ({}) VALUES ({})",
                colunas.join(", "),
                marcadores.join(", ")
            ),
            rusqlite::params_from_iter(valores),
        )
        .expect("inserir");
}

/// **Queda com a próxima identidade no disco e o banco intocado:** nada mudou no banco, e a
/// passada seguinte conclui com a MESMA identidade preparada.
#[test]
fn queda_antes_do_banco_nao_muda_nada_e_a_proxima_passada_conclui() {
    for ponto in [
        falha::Ponto::DepoisDaIdentidade,
        falha::Ponto::AntesDoCommit,
    ] {
        let a = Instalacao::da_beta("a");
        let antes_do_banco = {
            // O retrato do banco sem os arquivos: a próxima identidade vai aparecer no disco.
            let mut foto = a.retrato();
            foto.retain(|linha| !linha.starts_with("arquivo "));
            foto
        };

        falha::armar(Some(ponto));
        let (estado, resultado) = a.arrancar();
        falha::armar(None);
        assert!(
            resultado.is_err(),
            "{ponto:?}: a queda não derrubou o arranque"
        );
        assert_eq!(estado.fase(), FaseDoBanco::RecoveryRequired);
        let mut depois = a.retrato();
        depois.retain(|linha| !linha.starts_with("arquivo "));
        // O único efeito no banco permitido é o do `prepare`, que não escreve num banco beta.
        assert_eq!(depois, antes_do_banco, "{ponto:?}: o banco mudou");
        assert_eq!(
            a.arquivo_de_identidade(),
            id("a"),
            "{ponto:?}: a identidade girou"
        );
        let preparada = identity_store::proxima(&a.dir)
            .expect("ler")
            .expect("a próxima identidade ficou no disco")
            .device_id()
            .to_string();

        let nova = rotacionada(&a);
        assert_eq!(
            nova.device_id(),
            preparada,
            "{ponto:?}: a passada seguinte gerou outra"
        );
    }
}

/// **Queda com o banco já na época nova e o arquivo ainda antigo:** o próximo `prepare` promove o
/// arquivo antes de reconciliar — e a identidade nova não é rebaixada pela antiga.
#[test]
fn queda_depois_do_commit_e_concluida_pelo_proximo_arranque() {
    let a = Instalacao::da_beta("a");
    falha::armar(Some(falha::Ponto::DepoisDoCommit));
    let (_, resultado) = a.arrancar();
    falha::armar(None);
    assert!(resultado.is_err());

    let marcada = a
        .conn()
        .query_row("SELECT device_id FROM sync_epoca", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("o banco comitou a época");
    assert_eq!(
        identity_store::load_or_create(&a.dir)
            .expect("atual")
            .device_id(),
        id("a"),
        "o cenário exige o arquivo ainda antigo"
    );

    let identidade = a.identidade();
    assert_eq!(
        identidade.device_id(),
        marcada,
        "o prepare não concluiu a troca"
    );
    assert!(!identity_store::next_identity_path(&a.dir).exists());
    let self_do_banco: String = a
        .conn()
        .query_row(
            "SELECT device_id FROM sync_devices WHERE is_self = 1",
            [],
            |row| row.get(0),
        )
        .expect("self");
    assert_eq!(
        self_do_banco, marcada,
        "a reconciliação rebaixou a identidade nova"
    );
    assert_eq!(a.contar("SELECT COUNT(*) FROM sync_devices"), 1);

    let (estado, resultado) = a.arrancar();
    assert!(
        !resultado.expect("arranque").pareamentos_invalidados,
        "girou duas vezes"
    );
    assert_eq!(estado.fase(), FaseDoBanco::Ready);
    assert_eq!(a.contar("SELECT COUNT(*) FROM sync_epoca"), 1);
}

/// **Sem época, sem sessão.** Um banco que ainda carrega o passado pré-Hello não entra em sessão
/// nenhuma — a recusa vem antes de qualquer byte na rede.
#[test]
fn banco_com_passado_nao_girado_nao_sincroniza() {
    let a = Instalacao::da_beta("a");
    // Só o `prepare`, sem o arranque que gira.
    let antiga = a.identidade();
    let erro = sincronizar_com("127.0.0.1:1", &a.ctx(&antiga)).expect_err("sem época");
    assert!(
        erro.message.contains("versão beta antiga"),
        "{}",
        erro.message
    );
    assert!(epoca::rotacao_pendente(&a.conn()).expect("pendente"));
}

/// **Aparelho novo nasce na época atual — e continua virgem para bootstrap** (a D0 não volta).
#[test]
fn aparelho_novo_nasce_na_epoca_e_continua_elegivel_a_bootstrap() {
    let z = Novo::criar();
    let origem: String = z
        .banco
        .connection()
        .query_row(
            "SELECT origem FROM sync_epoca WHERE protocolo = 1",
            [],
            |row| row.get(0),
        )
        .expect("marcado no arranque");
    assert_eq!(origem, "instalacao");
    assert!(
        crate::infrastructure::sqlite::sync_snapshot::receptor_elegivel(&mut z.banco.connection())
            .expect("consultar"),
        "o marcador de época tirou a virgindade de um aparelho novo"
    );
}

/// **A porta dos gatilhos só abre dentro da rotação**, e o arquivo do passado não se edita.
#[test]
fn a_porta_do_log_so_abre_com_a_trava_e_o_legado_e_imutavel() {
    let z = Novo::criar();
    crate::application::universe_service::create(&z.banco.database, &z.store, &z.eu, "U", "", "")
        .expect("um evento no log");
    let connection = z.banco.connection();
    assert!(connection.execute("DELETE FROM sync_events", []).is_err());
    assert!(connection.execute("DELETE FROM sync_cursors", []).is_err());

    connection
        .execute_batch(
            "BEGIN;
             INSERT INTO sync_rotacao_em_curso (unica) VALUES (1);
             DELETE FROM sync_applied_events; DELETE FROM sync_revision_history;
             DELETE FROM sync_aggregate_state; DELETE FROM sync_events; DELETE FROM sync_cursors;
             ROLLBACK;",
        )
        .expect("com a trava, a porta abre");
    assert!(
        connection.execute("DELETE FROM sync_events", []).is_err(),
        "a trava ficou aberta"
    );

    connection
        .execute(
            "INSERT INTO sync_legado (protocolo, tabela, linha, arquivada_em) VALUES (0, 't', '{}', 'x')",
            [],
        )
        .expect("arquivar");
    assert!(connection
        .execute("UPDATE sync_legado SET linha = '[]'", [])
        .is_err());
    assert!(connection.execute("DELETE FROM sync_legado", []).is_err());
}

/// **Toda tabela `sync_*` tem destino declarado na rotação.**
#[test]
fn todo_estado_do_protocolo_tem_destino_na_rotacao() {
    let z = Novo::criar();
    let tabelas: Vec<String> = z
        .banco
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'sync\\_%' ESCAPE '\\'")
        .expect("tabelas")
        .query_map([], |row| row.get(0))
        .expect("listar")
        .collect::<Result<_, _>>()
        .expect("ler");
    assert!(tabelas.len() >= TABELAS_DO_PASSADO.len());
    for tabela in &tabelas {
        let arquivada = TABELAS_DO_PASSADO.contains(&tabela.as_str());
        let preservada = epoca::PRESERVADAS_NA_ROTACAO
            .iter()
            .any(|(nome, _)| nome == tabela);
        assert!(
            arquivada ^ preservada,
            "{tabela}: precisa estar em exatamente um dos dois lados da rotação"
        );
    }
}
