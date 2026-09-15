//! **Gates da B2 em dois aparelhos.** PC e Android simulados: cada um com banco, identidade em
//! arquivo e blob store próprios. Nada é compartilhado além dos eventos que trocam — pelo mesmo
//! caminho da sessão real (`eventos_para` → `receber_eventos`).
//!
//! Convergência aqui é **estado**, não fila vazia: depois de sincronizar, o payload canônico de cada
//! agregado é igual nos dois, e é o payload da revisão corrente dos dois.

use crate::application::{canvas_service, manuscript_service, sync_bootstrap, universe_service};
use crate::domain::identity::DeviceIdentity;
use crate::domain::manuscript::{Book, Chapter, ChapterUpdate, Story};
use crate::domain::sync::{AggregateRef, Operation};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::sync_apply::envelope_de_origem;
use crate::infrastructure::sqlite::sync_codec;
use crate::infrastructure::sqlite::sync_exchange::{eventos_para, vetor_local};
use crate::infrastructure::sqlite::sync_session::{receber_eventos, Relatorio};
use crate::infrastructure::sqlite::test_support::TemporaryDatabase;
use rusqlite::OptionalExtension;

struct Aparelho {
    nome: &'static str,
    banco: TemporaryDatabase,
    eu: DeviceIdentity,
    store: BlobStore,
    dados: std::path::PathBuf,
}

impl Drop for Aparelho {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dados);
    }
}

impl Aparelho {
    fn novo(nome: &'static str) -> Self {
        let banco = TemporaryDatabase::new();
        let dados =
            std::env::temp_dir().join(format!("narrahub-b2-{nome}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dados).expect("dados");
        let eu = sync_bootstrap::prepare(&dados, &banco.database).expect("arranque");
        Self {
            nome,
            banco,
            eu,
            store: BlobStore::new(dados.clone()),
            dados,
        }
    }

    fn universo(&self, nome: &str) -> String {
        universe_service::create(&self.banco.database, &self.store, &self.eu, nome, "", "")
            .expect("universo")
            .id
    }

    fn historia(&self, universo: &str, nome: &str) -> Story {
        manuscript_service::create_story(&self.banco.database, &self.eu, universo, nome)
            .expect("história")
    }

    fn livro(&self, historia: &str, nome: &str) -> Book {
        manuscript_service::create_book(&self.banco.database, &self.eu, historia, nome)
            .expect("livro")
    }

    fn capitulo(&self, livro: &str, titulo: &str) -> Chapter {
        manuscript_service::create_chapter(&self.banco.database, &self.eu, livro, titulo)
            .expect("capítulo")
    }

    fn escrever(&self, capitulo: &str, texto: &str) {
        manuscript_service::update_chapter(
            &self.banco.database,
            &self.eu,
            capitulo,
            ChapterUpdate {
                content: Some(texto.into()),
                word_count: Some(sync_codec::palavras::contar(texto)),
                ..ChapterUpdate::default()
            },
        )
        .expect("escrever");
    }

    fn canonico(&self, tipo: &str, id: &str) -> Option<String> {
        let connection = self.banco.connection();
        sync_codec::ler_canonico(&connection, &AggregateRef::new(tipo, id))
            .expect("ler")
            .map(|estado| estado.payload)
    }

    fn payload_corrente(&self, tipo: &str, id: &str) -> Option<String> {
        let connection = self.banco.connection();
        sync_codec::payload_da_revisao_corrente(&connection, &AggregateRef::new(tipo, id))
            .expect("revisão")
    }

    fn tombstone(&self, tipo: &str, id: &str) -> bool {
        self.banco
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_tombstones WHERE aggregate_type = ?1 AND aggregate_id = ?2)",
                [tipo, id],
                |row| row.get(0),
            )
            .expect("tombstone")
    }

    fn contar(&self, sql: &str) -> i64 {
        self.banco
            .connection()
            .query_row(sql, [], |row| row.get(0))
            .expect("contar")
    }

    fn eventos_do_tipo(&self, tipo: &str) -> i64 {
        self.banco
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sync_events WHERE device_id = ?1 AND aggregate_type = ?2",
                [self.eu.device_id(), tipo],
                |row| row.get(0),
            )
            .expect("contar")
    }

    fn divergencias_abertas(&self, tipo: &str) -> Vec<(String, String)> {
        let connection = self.banco.connection();
        let mut consulta = connection
            .prepare(
                "SELECT aggregate_id, kind FROM sync_divergences
                  WHERE aggregate_type = ?1 AND resolved_at = '' ORDER BY aggregate_id",
            )
            .expect("consulta");
        consulta
            .query_map([tipo], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("divergências")
    }

    /// O agregado existe aqui e lá com o mesmo estado canônico, e esse estado é o da revisão
    /// corrente dos dois — ou não existe em nenhum, com tombstone nos dois.
    fn convergiu_com(&self, outro: &Aparelho, tipo: &str, id: &str) {
        let aqui = self.canonico(tipo, id);
        let la = outro.canonico(tipo, id);
        assert_eq!(
            aqui, la,
            "{tipo} {id}: {} e {} divergem",
            self.nome, outro.nome
        );
        match aqui {
            Some(payload) => {
                assert_eq!(
                    self.payload_corrente(tipo, id).as_deref(),
                    Some(payload.as_str()),
                    "{tipo} {id}: em {} o estado não é o da revisão corrente",
                    self.nome
                );
                assert_eq!(
                    outro.payload_corrente(tipo, id).as_deref(),
                    Some(payload.as_str()),
                    "{tipo} {id}: em {} o estado não é o da revisão corrente",
                    outro.nome
                );
            }
            None => {
                assert!(
                    self.tombstone(tipo, id),
                    "{tipo} {id} sem tombstone em {}",
                    self.nome
                );
                assert!(
                    outro.tombstone(tipo, id),
                    "{tipo} {id} sem tombstone em {}",
                    outro.nome
                );
            }
        }
    }
}

fn apresentar(fonte: &Aparelho, destino: &Aparelho) {
    let conhecidas: Vec<(String, String)> = {
        let connection = fonte.banco.connection();
        let mut consulta = connection
            .prepare("SELECT device_id, ed25519_public FROM sync_devices")
            .expect("preparar");
        let linhas = consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("consultar");
        linhas.collect::<Result<_, _>>().expect("ler")
    };
    let connection = destino.banco.connection();
    for (device_id, publica) in conhecidas {
        let ja: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_devices WHERE device_id = ?1)",
                [&device_id],
                |row| row.get(0),
            )
            .expect("existe");
        if ja {
            continue;
        }
        let sessao =
            crate::infrastructure::sync_transport::SessaoAutenticada::deste_aparelho(&destino.eu);
        crate::infrastructure::sqlite::sync_trust::introduzir_dispositivo(
            &connection,
            &sessao,
            &device_id,
            &publica,
        )
        .expect("introduzir");
    }
}

/// Uma sessão simétrica. Devolve (o que `b` recebeu, o que `a` recebeu).
fn sincronizar(a: &Aparelho, b: &Aparelho) -> (Relatorio, Relatorio) {
    apresentar(a, b);
    apresentar(b, a);
    let vetor_a = vetor_local(&a.banco.connection()).expect("vetor a");
    let vetor_b = vetor_local(&b.banco.connection()).expect("vetor b");
    let para_b = eventos_para(&a.banco.connection(), &vetor_b).expect("a → b");
    let para_a = eventos_para(&b.banco.connection(), &vetor_a).expect("b → a");
    let em_b = receber_eventos(&mut b.banco.connection(), &para_b).expect("b recebe");
    let em_a = receber_eventos(&mut a.banco.connection(), &para_a).expect("a recebe");
    (em_b, em_a)
}

/// Universo → história → livro → dois capítulos com texto, criados no `autor`, já sincronizados.
struct Arvore {
    universo: String,
    historia: String,
    livro: String,
    capitulos: Vec<String>,
}

fn arvore(autor: &Aparelho, outro: &Aparelho) -> Arvore {
    let universo = autor.universo("Terra");
    let historia = autor.historia(&universo, "Saga").id;
    let livro = autor.livro(&historia, "Livro I").id;
    let c1 = autor.capitulo(&livro, "Um").id;
    let c2 = autor.capitulo(&livro, "Dois").id;
    autor.escrever(&c1, "<p>Era uma vez</p>");
    let (recebido, _) = sincronizar(autor, outro);
    assert_eq!(recebido.divergencias, 0);
    assert!(
        recebido.precisam_reconciliar.is_empty(),
        "{:?}",
        recebido.precisam_reconciliar
    );
    Arvore {
        universo,
        historia,
        livro,
        capitulos: vec![c1, c2],
    }
}

fn convergencia_da_arvore(a: &Aparelho, b: &Aparelho, arvore: &Arvore) {
    a.convergiu_com(b, "universe", &arvore.universo);
    a.convergiu_com(b, "story", &arvore.historia);
    a.convergiu_com(b, "book", &arvore.livro);
    a.convergiu_com(b, "chapter_order", &arvore.livro);
    for capitulo in &arvore.capitulos {
        a.convergiu_com(b, "chapter", capitulo);
    }
}

#[test]
fn criado_no_pc_chega_ao_android() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);
    convergencia_da_arvore(&pc, &android, &arvore);

    // word_count fora do payload: o Android recalculou igual ao PC.
    let contagem = |aparelho: &Aparelho| -> i64 {
        aparelho
            .banco
            .connection()
            .query_row(
                "SELECT word_count FROM chapters WHERE id = ?1",
                [&arvore.capitulos[0]],
                |row| row.get(0),
            )
            .expect("contagem")
    };
    assert_eq!(contagem(&pc), 3);
    assert_eq!(contagem(&android), 3);
}

#[test]
fn criado_no_android_chega_ao_pc_e_edicao_segue_o_mesmo_caminho() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&android, &pc);
    convergencia_da_arvore(&android, &pc, &arvore);

    pc.escrever(&arvore.capitulos[1], "<p>escrito no PC</p>");
    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.aplicados, 1);
    convergencia_da_arvore(&pc, &android, &arvore);
}

#[test]
fn conteudo_independente_dos_dois_lados_vira_uniao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let do_pc = pc.universo("Do PC");
    let do_android = android.universo("Do Android");
    let (em_android, em_pc) = sincronizar(&pc, &android);
    assert_eq!(em_android.divergencias + em_pc.divergencias, 0);
    pc.convergiu_com(&android, "universe", &do_pc);
    pc.convergiu_com(&android, "universe", &do_android);
}

#[test]
fn reordenar_muda_so_a_ordem_e_chega_igual() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let mut arvore = arvore(&pc, &android);
    let c3 = pc.capitulo(&arvore.livro, "Três").id;
    arvore.capitulos.push(c3.clone());
    sincronizar(&pc, &android);

    let capitulos_antes = pc.eventos_do_tipo("chapter");
    let ordens_antes = pc.eventos_do_tipo("chapter_order");
    let nova = vec![
        c3.clone(),
        arvore.capitulos[0].clone(),
        arvore.capitulos[1].clone(),
    ];
    manuscript_service::reorder_chapters(&pc.banco.database, &pc.eu, &arvore.livro, &nova)
        .expect("reordenar");
    assert_eq!(
        pc.eventos_do_tipo("chapter"),
        capitulos_antes,
        "reordenar revisou capítulo"
    );
    assert_eq!(pc.eventos_do_tipo("chapter_order"), ordens_antes + 1);

    sincronizar(&pc, &android);
    convergencia_da_arvore(&pc, &android, &arvore);
    let ordem = android
        .canonico("chapter_order", &arvore.livro)
        .expect("ordem");
    assert!(
        ordem.contains(&format!("\"chapterIds\":[\"{c3}\",")),
        "{ordem}"
    );
}

#[test]
fn excluir_capitulo_leva_anexo_e_marcacao_e_reescreve_a_ordem() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);
    let alvo = arvore.capitulos[0].clone();

    let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    let anexo = canvas_service::create_attachment(
        &pc.banco.database,
        &pc.store,
        &pc.eu,
        &arvore.universo,
        "chapter",
        &alvo,
        png,
        "",
    )
    .expect("anexo");
    // Marcação de tag ainda sem evento (B5): existe só no PC, e a exclusão precisa emitir o fim dela.
    pc.banco
        .connection()
        .execute_batch(&format!(
            "INSERT INTO content_tags (id, universe_id, name, created_at) VALUES ('t1', '{u}', 'Tom', '2026-01-01');
             INSERT INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at)
               VALUES ('ta1', 't1', 'chapter', '{alvo}', '2026-01-01');",
            u = arvore.universo
        ))
        .expect("tag");
    sincronizar(&pc, &android);
    pc.convergiu_com(&android, "attachment", &anexo.id);

    manuscript_service::delete_chapter(&pc.banco.database, &pc.eu, &alvo).expect("excluir");
    let marcacao = sync_codec::manuscrito::id_da_atribuicao("t1", "chapter", &alvo);
    assert!(pc.tombstone("tag_assignment", &marcacao));

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0);
    pc.convergiu_com(&android, "chapter", &alvo);
    pc.convergiu_com(&android, "attachment", &anexo.id);
    pc.convergiu_com(&android, "chapter_order", &arvore.livro);
    pc.convergiu_com(&android, "chapter", &arvore.capitulos[1]);
}

#[test]
fn excluir_livro_e_historia_converge_com_tombstone_de_toda_a_arvore() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);
    let segundo = pc.livro(&arvore.historia, "Livro II").id;
    let solto = pc.capitulo(&segundo, "Solto").id;
    sincronizar(&pc, &android);

    manuscript_service::delete_book(&pc.banco.database, &pc.eu, &arvore.livro).expect("livro");
    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(
        no_android.divergencias,
        0,
        "{:?}",
        android.divergencias_abertas("chapter")
    );
    pc.convergiu_com(&android, "book", &arvore.livro);
    pc.convergiu_com(&android, "chapter_order", &arvore.livro);
    for capitulo in &arvore.capitulos {
        pc.convergiu_com(&android, "chapter", capitulo);
    }
    pc.convergiu_com(&android, "book", &segundo);

    manuscript_service::delete_story(&pc.banco.database, &pc.eu, &arvore.historia)
        .expect("história");
    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0);
    for (tipo, id) in [
        ("story", &arvore.historia),
        ("book", &segundo),
        ("chapter_order", &segundo),
        ("chapter", &solto),
    ] {
        pc.convergiu_com(&android, tipo, id);
    }
    assert_eq!(android.contar("SELECT COUNT(*) FROM chapters"), 0);
    pc.convergiu_com(&android, "universe", &arvore.universo);
}

/// Pai excluído num lado, filho criado no outro ao mesmo tempo: nada some em nenhum dos dois.
#[test]
fn exclusao_do_pai_com_filho_concorrente_nao_perde_o_filho() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);

    let novo = android.capitulo(&arvore.livro, "Escrito no ônibus").id;
    android.escrever(&novo, "<p>trabalho novo</p>");
    manuscript_service::delete_book(&pc.banco.database, &pc.eu, &arvore.livro).expect("excluir");

    let (no_android, no_pc) = sincronizar(&pc, &android);

    // No Android: o livro, os capítulos antigos e o novo continuam; a exclusão virou decisão.
    assert!(no_android.divergencias >= 1);
    assert_eq!(
        android.divergencias_abertas("book"),
        vec![(arvore.livro.clone(), "parent_deletion_blocked".to_string())]
    );
    assert!(android.canonico("chapter", &novo).is_some());
    assert!(android.canonico("book", &arvore.livro).is_some());
    for antigo in &arvore.capitulos {
        assert!(
            android.canonico("chapter", antigo).is_some(),
            "capítulo {antigo} saiu com a exclusão do livro bloqueada: livro pela metade"
        );
    }
    assert_eq!(
        android
            .canonico("chapter", &novo)
            .map(|p| p.contains("trabalho novo")),
        Some(true)
    );

    // No PC: o capítulo novo não tem onde morar; fica pendente, não é descartado nem aplicado.
    assert!(
        no_pc.precisam_reconciliar.contains(&novo),
        "{:?}",
        no_pc.precisam_reconciliar
    );
    let guardado: bool = pc
        .banco
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_events WHERE aggregate_id = ?1)",
            [&novo],
            |row| row.get(0),
        )
        .expect("guardado");
    assert!(
        guardado,
        "o evento do filho concorrente precisa ficar guardado no PC"
    );
}

/// Capítulo ligado a card do planejamento: o `SET NULL` reescreveria um agregado da B4.
/// Local: a exclusão é recusada inteira. Remoto: a exclusão vira decisão, e o card continua ligado.
#[test]
fn exclusao_que_reescreveria_card_do_planejamento_e_recusada_nos_dois_lados() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);
    let alvo = arvore.capitulos[0].clone();
    let ligar = |aparelho: &Aparelho| {
        aparelho
            .banco
            .connection()
            .execute(
                "INSERT INTO planning_items (id, universe_id, chapter_id, title, created_at, updated_at)
                 VALUES ('card-1', ?1, ?2, 'Cena', '2026-01-01', '2026-01-01')",
                [&arvore.universo, &alvo],
            )
            .expect("card");
    };

    // Local.
    ligar(&pc);
    let eventos = pc.contar("SELECT COUNT(*) FROM sync_events");
    let erro = manuscript_service::delete_chapter(&pc.banco.database, &pc.eu, &alvo)
        .expect_err("card ligado");
    assert!(erro.message.contains("planejamento"), "{}", erro.message);
    assert!(pc.canonico("chapter", &alvo).is_some());
    assert_eq!(pc.contar("SELECT COUNT(*) FROM sync_events"), eventos);

    // Remoto: o card só existe no Android; o PC exclui.
    pc.banco
        .connection()
        .execute("DELETE FROM planning_items WHERE id = 'card-1'", [])
        .expect("desligar no pc");
    ligar(&android);
    manuscript_service::delete_chapter(&pc.banco.database, &pc.eu, &alvo).expect("excluir no pc");
    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 1);
    assert_eq!(
        android.divergencias_abertas("chapter"),
        vec![(alvo.clone(), "parent_deletion_blocked".to_string())]
    );
    let ligado: Option<String> = android
        .banco
        .connection()
        .query_row(
            "SELECT chapter_id FROM planning_items WHERE id = 'card-1'",
            [],
            |row| row.get(0),
        )
        .optional()
        .expect("card")
        .flatten();
    assert_eq!(
        ligado.as_deref(),
        Some(alvo.as_str()),
        "o SET NULL aconteceu em silêncio"
    );
}

/// Reescrita também passa pelo preflight: com a ordem do livro em divergência, excluir capítulo
/// alteraria a ordem em silêncio, e é recusado.
#[test]
fn reescrita_de_agregado_em_divergencia_recusa_a_exclusao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);

    let nova = vec![arvore.capitulos[1].clone(), arvore.capitulos[0].clone()];
    manuscript_service::reorder_chapters(&pc.banco.database, &pc.eu, &arvore.livro, &nova)
        .expect("pc reordena");
    android.capitulo(&arvore.livro, "Três");
    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(
        android.divergencias_abertas("chapter_order"),
        vec![(arvore.livro.clone(), "concurrent".to_string())],
        "{no_android:?}"
    );

    let erro = manuscript_service::delete_chapter(
        &android.banco.database,
        &android.eu,
        &arvore.capitulos[0],
    )
    .expect_err("ordem em divergência");
    assert!(erro.message.contains("chapter_order"), "{}", erro.message);
    assert!(android.canonico("chapter", &arvore.capitulos[0]).is_some());
}

/// `delete_universe` recusa com a mensagem combinada; uma exclusão de universo que chegue de fora
/// vira decisão pendente e não apaga nada.
#[test]
fn exclusao_de_universo_e_recusada_local_e_remotamente() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);

    let erro = universe_service::delete(&pc.banco.database, &arvore.universo).expect_err("recusa");
    assert_eq!(
        erro.message,
        "Esta operação ainda depende de tipos que estão sendo migrados para o Sync V2."
    );
    assert!(pc.canonico("universe", &arvore.universo).is_some());

    let base = {
        let connection = android.banco.connection();
        sync_codec::revisao_corrente(
            &connection,
            &AggregateRef::new("universe", &arvore.universo),
        )
        .expect("rev")
        .expect("tem revisão")
    };
    let seq = pc.contar("SELECT COALESCE(MAX(seq), 0) FROM sync_events WHERE device_id IN (SELECT device_id FROM sync_devices WHERE is_self = 1)") + 1;
    let mut exclusao = envelope_de_origem(
        pc.eu.device_id(),
        seq,
        &arvore.universo,
        &AggregateRef::new("universe", &arvore.universo),
        Operation::Delete,
        "",
        &base,
    );
    exclusao.signature = pc.eu.sign(&exclusao);
    let relatorio = receber_eventos(&mut android.banco.connection(), &[exclusao]).expect("receber");
    assert_eq!(relatorio.divergencias, 1);
    assert_eq!(
        android.divergencias_abertas("universe"),
        vec![(
            arvore.universo.clone(),
            "parent_deletion_blocked".to_string()
        )]
    );
    convergencia_da_arvore_sem_universo(&android, &arvore);
}

fn convergencia_da_arvore_sem_universo(aparelho: &Aparelho, arvore: &Arvore) {
    assert!(aparelho.canonico("universe", &arvore.universo).is_some());
    assert!(aparelho.canonico("story", &arvore.historia).is_some());
    assert!(aparelho.canonico("book", &arvore.livro).is_some());
    for capitulo in &arvore.capitulos {
        assert!(aparelho.canonico("chapter", capitulo).is_some());
    }
}

/// Mesmo estado, mesmo payload, nenhuma revisão: salvar sem mudar nada autoral não gera evento.
#[test]
fn salvar_o_mesmo_estado_nao_gera_revisao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);
    let antes = pc.eventos_do_tipo("chapter");
    pc.escrever(&arvore.capitulos[0], "<p>Era uma vez</p>");
    assert_eq!(pc.eventos_do_tipo("chapter"), antes);
}
