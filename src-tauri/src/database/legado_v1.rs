//! **O legado do Sync V1** (etapa G).
//!
//! O V1 — snapshot de 17 tabelas por TCP, com código de seis dígitos e LWW por `updated_at` —
//! saiu do runtime na etapa G. O Sync V2 é o único protocolo alcançável.
//!
//! O que sobrou dele é **schema**, não código:
//!
//! ```text
//! tabela           por que continua no banco
//! sync_conflicts   conflitos por campo que o V1 gravou e nunca teve como resolver; a versão
//!                  do outro aparelho só existe nessa linha, então ela é preservada para
//!                  auditoria. A conversão de mídia do ADR 0010 (superfície 9) ainda a migra
//!                  no arranque de um banco antigo.
//! sync_peers       criada pela migration 1; nenhum código jamais escreveu nela
//! devices          idem
//! ```
//!
//! Nenhuma das três é lida ou escrita por caminho de produção: nem sessão, nem bootstrap, nem
//! panorama, nem resolução de conflito. O único acesso restante é o upgrade histórico — as
//! migrations e a conversão de mídia no arranque, antes de o banco ficar `Ready`.
//!
//! **Não há `DROP`**: remover as tabelas exigiria uma migration nova só para apagar bytes
//! históricos, e `sync_conflicts` é a única cópia da versão perdedora de um conflito V1. Os
//! gates G10/G11 (abaixo e em `sync_sessao`) provam que o runtime não as toca; o G12 impede que
//! o código de produção volte a citá-las fora do upgrade.

/// As tabelas do Sync V1 que ficam no schema como legado histórico.
pub const TABELAS_DO_SYNC_V1: &[(&str, &str)] = &[
    (
        "sync_conflicts",
        "conflitos do V1, preservados para auditoria; só a conversão de mídia do arranque os toca",
    ),
    ("sync_peers", "endereços do V1; nunca escrita"),
    ("devices", "aparelhos do V1; nunca escrita"),
];

/// **O vigia do legado** (só em teste): registra toda leitura ou escrita de uma tabela do V1
/// feita por uma conexão aberta sobre um banco vigiado.
///
/// Usa o autorizador do SQLite, então pega o que nenhuma busca textual pega: SQL montado em
/// tempo de execução, gatilho, `COUNT(*)` sem coluna. Toda conexão de produção passa por
/// [`crate::infrastructure::sqlite::connection::apply_pragmas`], que o instala.
#[cfg(test)]
pub(crate) mod vigia {
    use super::TABELAS_DO_SYNC_V1;
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    use rusqlite::Connection;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, OnceLock};

    type Registro = Arc<Mutex<Vec<String>>>;

    fn armados() -> &'static Mutex<Vec<(PathBuf, Registro)>> {
        static ARMADOS: OnceLock<Mutex<Vec<(PathBuf, Registro)>>> = OnceLock::new();
        ARMADOS.get_or_init(Default::default)
    }

    fn normalizar(caminho: &Path) -> PathBuf {
        std::fs::canonicalize(caminho).unwrap_or_else(|_| caminho.to_path_buf())
    }

    /// Enquanto vivo, vigia os bancos dados. Por caminho, e não por thread: a sessão real atende
    /// o outro aparelho numa thread própria, e é justamente lá que um acesso escondido estaria.
    pub struct Vigia {
        caminhos: Vec<PathBuf>,
        registro: Registro,
    }

    impl Vigia {
        pub fn armar(caminhos: &[&Path]) -> Self {
            let registro: Registro = Arc::default();
            let caminhos: Vec<PathBuf> = caminhos.iter().map(|c| normalizar(c)).collect();
            let mut lista = armados().lock().expect("vigia");
            for caminho in &caminhos {
                lista.push((caminho.clone(), registro.clone()));
            }
            Self { caminhos, registro }
        }

        pub fn acessos(&self) -> Vec<String> {
            self.registro.lock().expect("registro").clone()
        }
    }

    impl Drop for Vigia {
        fn drop(&mut self) {
            if let Ok(mut lista) = armados().lock() {
                lista.retain(|(caminho, registro)| {
                    !(self.caminhos.contains(caminho) && Arc::ptr_eq(registro, &self.registro))
                });
            }
        }
    }

    pub fn instalar(connection: &Connection) {
        let Some(caminho) = connection.path().filter(|c| !c.is_empty()) else {
            return;
        };
        let caminho = normalizar(Path::new(caminho));
        let registro = armados()
            .lock()
            .expect("vigia")
            .iter()
            .find(|(armado, _)| *armado == caminho)
            .map(|(_, registro)| registro.clone());
        let Some(registro) = registro else {
            return;
        };
        connection.authorizer(Some(move |contexto: AuthContext<'_>| {
            let alvo = match contexto.action {
                AuthAction::Read { table_name, .. } => Some(("leitura", table_name)),
                AuthAction::Insert { table_name } => Some(("escrita", table_name)),
                AuthAction::Update { table_name, .. } => Some(("escrita", table_name)),
                AuthAction::Delete { table_name } => Some(("escrita", table_name)),
                _ => None,
            };
            if let Some((tipo, tabela)) = alvo {
                if TABELAS_DO_SYNC_V1.iter().any(|(t, _)| *t == tabela) {
                    registro
                        .lock()
                        .expect("registro")
                        .push(format!("{tipo} de {tabela}"));
                }
            }
            Authorization::Allow
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::TABELAS_DO_SYNC_V1;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// Quem ainda pode citar uma tabela do V1 no código de produção, e com que papel.
    ///
    /// `com_sql = false`: a citação é classificação ou lista (catálogo, preservação), nunca uma
    /// consulta. Só as migrations escrevem SQL sobre elas.
    const QUEM_PODE_CITAR: &[(&str, bool, &str)] = &[
        ("database/migrations.rs", true, "migrations históricas"),
        ("database/legado_v1.rs", false, "este catálogo"),
        (
            "infrastructure/sqlite/blob_surfaces.rs",
            false,
            "superfície 9 do ADR 0010: conversão de mídia de banco antigo",
        ),
        (
            "infrastructure/sqlite/blob_backfill.rs",
            false,
            "ordem da conversão de mídia; o SQL é genérico sobre o catálogo",
        ),
        (
            "infrastructure/sqlite/sync_snapshot.rs",
            false,
            "classificação no catálogo do bootstrap: legado local, não viaja, não bloqueia",
        ),
        (
            "application/epoca.rs",
            false,
            "tabelas que a rotação de época preserva",
        ),
        (
            "infrastructure/sqlite/sync_codec/midia.rs",
            false,
            "superfícies de mídia que não viajam em evento",
        ),
    ];

    /// Símbolos do V1 que nenhum código de produção pode voltar a ter.
    const SIMBOLOS_DO_V1: &[&str] = &[
        "crate::sync::",
        "SyncState",
        "SyncServerStatus",
        "sync_status",
        "sync_start",
        "sync_stop",
        "sync_connect",
    ];

    fn raiz() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    fn arquivos() -> Vec<PathBuf> {
        let mut pilha = vec![raiz()];
        let mut todos = Vec::new();
        while let Some(pasta) = pilha.pop() {
            for entrada in std::fs::read_dir(&pasta).expect("ler src") {
                let caminho = entrada.expect("entrada").path();
                if caminho.is_dir() {
                    pilha.push(caminho);
                } else if caminho.extension().is_some_and(|e| e == "rs") {
                    todos.push(caminho);
                }
            }
        }
        todos.sort();
        todos
    }

    /// Módulos declarados atrás de `#[cfg(test)]` — os arquivos deles são só teste.
    fn modulos_de_teste(fontes: &[(PathBuf, String)]) -> BTreeSet<String> {
        let mut nomes = BTreeSet::new();
        for (_, fonte) in fontes {
            let linhas: Vec<&str> = fonte.lines().map(str::trim).collect();
            for (i, linha) in linhas.iter().enumerate() {
                if *linha != "#[cfg(test)]" {
                    continue;
                }
                let Some(seguinte) = linhas[i + 1..].iter().find(|l| !l.is_empty()) else {
                    continue;
                };
                let declaracao = seguinte
                    .trim_start_matches("pub(crate) ")
                    .trim_start_matches("pub ");
                if let Some(nome) = declaracao
                    .strip_prefix("mod ")
                    .and_then(|r| r.strip_suffix(';'))
                {
                    nomes.insert(nome.trim().to_string());
                }
            }
        }
        nomes
    }

    /// O código de produção de um arquivo: sem o módulo de teste e sem comentário.
    fn producao(fonte: &str) -> String {
        let fonte = fonte.replace("\r\n", "\n");
        let codigo = fonte
            .split_once("#[cfg(test)]")
            .map(|(c, _)| c.to_string())
            .unwrap_or(fonte);
        codigo
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn cita(linha: &str, termo: &str) -> bool {
        let bytes = linha.as_bytes();
        let mut inicio = 0;
        while let Some(pos) = linha[inicio..].find(termo) {
            let a = inicio + pos;
            let b = a + termo.len();
            let borda = |i: usize| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_';
            if (a == 0 || !borda(a - 1)) && (b >= bytes.len() || !borda(b)) {
                return true;
            }
            inicio = b;
        }
        false
    }

    fn fontes_de_producao() -> Vec<(String, String)> {
        let fontes: Vec<(PathBuf, String)> = arquivos()
            .into_iter()
            .map(|c| {
                let fonte = std::fs::read_to_string(&c).expect("ler");
                (c, fonte)
            })
            .collect();
        let de_teste = modulos_de_teste(&fontes);
        let raiz = raiz();
        fontes
            .into_iter()
            .filter(|(caminho, _)| {
                let nome = caminho.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                !de_teste.contains(nome)
            })
            .map(|(caminho, fonte)| {
                let relativo = caminho
                    .strip_prefix(&raiz)
                    .expect("relativo")
                    .to_string_lossy()
                    .replace('\\', "/");
                (relativo, producao(&fonte))
            })
            .collect()
    }

    /// **G1 — nenhum comando do V1 existe nem está registrado.**
    #[test]
    fn g1_nenhum_comando_do_sync_v1_registrado() {
        assert!(
            !raiz().join("sync.rs").exists(),
            "src/sync.rs voltou: o Sync V1 saiu do runtime na etapa G"
        );
        let lib = std::fs::read_to_string(raiz().join("lib.rs")).expect("lib.rs");
        let inicio = lib.find("invoke_handler").expect("invoke_handler");
        let lista = &lib[inicio..];
        for comando in ["sync_status", "sync_start", "sync_stop", "sync_connect"] {
            assert!(
                !cita(lista, comando),
                "`{comando}` é comando do Sync V1 e voltou ao invoke_handler"
            );
        }
        let codigo = producao(&lib);
        assert!(!codigo.contains("mod sync;"), "o módulo do Sync V1 voltou");
        assert!(
            !codigo.contains("SyncState"),
            "o estado do Sync V1 voltou a ser gerenciado"
        );
    }

    /// **G12 — busca arquitetural**: o código de produção não cita símbolo do V1, e só o
    /// upgrade histórico cita as tabelas dele — sem SQL, fora das migrations.
    #[test]
    fn g12_producao_nao_depende_do_sync_v1() {
        let mut infracoes = Vec::new();
        let mut citacoes_permitidas = 0;
        for (relativo, codigo) in fontes_de_producao() {
            let permitido = QUEM_PODE_CITAR
                .iter()
                .find(|(nome, _, _)| *nome == relativo);
            for (n, linha) in codigo.lines().enumerate() {
                for simbolo in SIMBOLOS_DO_V1 {
                    let achou = if simbolo.contains("::") {
                        linha.contains(simbolo)
                    } else {
                        cita(linha, simbolo)
                    };
                    if achou {
                        infracoes.push(format!("{relativo}:{}: símbolo do V1 `{simbolo}`", n + 1));
                    }
                }
                for (tabela, _) in TABELAS_DO_SYNC_V1 {
                    if !cita(linha, tabela) {
                        continue;
                    }
                    match permitido {
                        None => infracoes.push(format!(
                            "{relativo}:{}: cita a tabela do V1 `{tabela}` fora do upgrade",
                            n + 1
                        )),
                        Some((_, false, _))
                            if [
                                "SELECT", "INSERT", "UPDATE", "DELETE", "FROM", "INTO", "JOIN",
                            ]
                            .iter()
                            .any(|sql| cita(linha, sql)) =>
                        {
                            infracoes.push(format!(
                                "{relativo}:{}: SQL sobre a tabela do V1 `{tabela}`",
                                n + 1
                            ))
                        }
                        Some(_) => citacoes_permitidas += 1,
                    }
                }
            }
        }
        assert!(
            infracoes.is_empty(),
            "o código de produção voltou a depender do Sync V1:\n{}",
            infracoes.join("\n")
        );
        assert!(
            citacoes_permitidas > 0,
            "a varredura não achou nem as citações permitidas; ela quebrou"
        );
    }

    /// **O vigia morde**: sem esta prova, G10/G11 passariam com um autorizador que não olha nada.
    /// `COUNT(*)` não cita coluna nenhuma — é o jeito mais discreto de ler uma tabela.
    #[test]
    fn o_vigia_pega_leitura_e_escrita_do_legado() {
        use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
        let banco = TemporaryDatabase::new();
        let vigia = super::vigia::Vigia::armar(&[banco.database.path()]);
        let leitura = banco.database.read().expect("leitura");
        let _: i64 = leitura
            .query_row("SELECT COUNT(*) FROM sync_conflicts", [], |row| row.get(0))
            .expect("contar");
        let _: i64 = leitura
            .query_row("SELECT COUNT(*) FROM universes", [], |row| row.get(0))
            .expect("contar domínio");
        drop(leitura);
        banco
            .database
            .write()
            .expect("escrita")
            .execute_batch(
                "INSERT INTO sync_peers (id, name, trusted_at) VALUES ('p','x','y');
                 DELETE FROM devices;",
            )
            .expect("escrever");
        let acessos = vigia.acessos();
        assert!(
            acessos.iter().any(|a| a == "leitura de sync_conflicts"),
            "{acessos:?}"
        );
        assert!(
            acessos.iter().any(|a| a == "escrita de sync_peers"),
            "{acessos:?}"
        );
        assert!(
            acessos.iter().any(|a| a == "escrita de devices"),
            "{acessos:?}"
        );
        assert!(
            acessos.iter().all(|a| !a.contains("universes")),
            "o vigia registrou tabela que não é do V1: {acessos:?}"
        );
    }

    /// **G7 — um banco publicado com legado do V1 sobe até o schema atual e fica utilizável.**
    ///
    /// Nasce no schema 15 (0.9.2, a última publicada estável) com o que o V1 deixava num banco
    /// de verdade: um conflito aberto com imagem inline nos dois lados, um peer e um aparelho.
    /// Sobe por todas as migrations, passa pelo arranque real (conversão de mídia e adoção) e
    /// termina `Ready`, íntegro, com o legado preservado e o acervo capturável como doador. O
    /// arranque é o único que ainda toca `sync_conflicts` — é migração —; depois dele, o vigia
    /// prova que a captura não o toca mais.
    #[test]
    fn g7_banco_publicado_com_legado_v1_sobe_integro_e_utilizavel() {
        use crate::application::{arranque, sync_bootstrap};
        use crate::database::estado::{EstadoDoBanco, FaseDoBanco};
        use crate::database::migrations::{sql_for_version, LATEST_SCHEMA_VERSION};
        use crate::infrastructure::blob_store::BlobStore;
        use crate::infrastructure::sqlite::connection::SqliteDatabase;
        use rusqlite::Connection;

        const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let pasta = std::env::temp_dir().join(format!("narrahub-g7-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&pasta).expect("pasta");
        let caminho = pasta.join("narrahub.db");
        {
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
                .execute(
                    "INSERT INTO universes (id, name, description, cover_image, created_at, updated_at)
                     VALUES ('u1', 'Terra', '', '', '2025-01-01', '2025-01-01');",
                    [],
                )
                .expect("universo");
            connection
                .execute(
                    "INSERT INTO sync_conflicts
                        (id, aggregate_type, aggregate_id, field, local_value, remote_value)
                     VALUES ('c-v1', 'chapter', 'cap-sumido', 'content', ?1, ?2)",
                    [
                        format!("<p>meu</p><img src=\"{PNG}\">"),
                        format!("<p>dele</p><img src=\"{PNG}\">"),
                    ],
                )
                .expect("conflito V1 aberto");
            connection
                .execute_batch(
                    "INSERT INTO sync_peers (id, name, trusted_at) VALUES ('p1', 'Velho', '2025-01-01');
                     INSERT INTO devices (id, name, created_at, last_seen_at)
                     VALUES ('d1', 'Velho', '2025-01-01', '2025-01-01');",
                )
                .expect("peer e aparelho do V1");
            aplicar(16, LATEST_SCHEMA_VERSION);
            connection
                .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION)
                .expect("versão");
        }

        let banco = SqliteDatabase::new(&caminho);
        let store = BlobStore::new(pasta.clone());
        let identidade = sync_bootstrap::prepare(&pasta, &banco).expect("identidade");
        let estado = EstadoDoBanco::default();
        let resumo = arranque::preparar_acervo(&banco, &store, &identidade, &estado)
            .expect("o arranque de um banco publicado com legado do V1");
        assert_eq!(estado.fase(), FaseDoBanco::Ready);
        assert!(
            resumo.sincronizacao_disponivel,
            "{}",
            resumo.motivo_da_indisponibilidade
        );

        let connection = banco.read().expect("leitura");
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
        let (local, remoto, aberto): (String, String, String) = connection
            .query_row(
                "SELECT local_value, remote_value, resolved_at FROM sync_conflicts WHERE id = 'c-v1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("o conflito V1 foi preservado");
        assert!(local.contains("meu") && remoto.contains("dele") && aberto.is_empty());
        assert!(
            !remoto.contains("data:image"),
            "a conversão de mídia do arranque deixou de migrar o legado"
        );
        drop(connection);

        let vigia = super::vigia::Vigia::armar(&[caminho.as_path()]);
        let mut escrita = banco.write().expect("escrita");
        crate::infrastructure::sqlite::sync_snapshot::capturar(&mut escrita)
            .expect("consultar")
            .expect("o legado do V1 não impede o banco de doar o acervo");
        drop(escrita);
        assert!(vigia.acessos().is_empty(), "{:?}", vigia.acessos());
        drop(vigia);
        std::fs::remove_dir_all(&pasta).ok();
    }

    /// A lista de quem pode citar não guarda arquivo que já não cita: permissão sem uso é porta
    /// aberta para a próxima dependência.
    #[test]
    fn g12_permissao_sem_uso_nao_fica_na_lista() {
        let fontes = fontes_de_producao();
        for (nome, _, motivo) in QUEM_PODE_CITAR {
            let codigo = fontes
                .iter()
                .find(|(relativo, _)| relativo == nome)
                .map(|(_, c)| c.as_str())
                .unwrap_or_else(|| panic!("{nome} não existe mais; tire da lista"));
            assert!(
                TABELAS_DO_SYNC_V1
                    .iter()
                    .any(|(tabela, _)| codigo.lines().any(|l| cita(l, tabela))),
                "{nome} ({motivo}) não cita mais o V1; tire da lista"
            );
        }
    }
}
