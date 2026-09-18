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

    fn entidade(&self, universo: &str, nome: &str) -> String {
        crate::application::entity_service::create(
            &self.banco.database,
            &self.store,
            &self.eu,
            crate::domain::entity::NewEntity {
                universe_id: universo.to_string(),
                entity_type: "Personagem".into(),
                name: nome.to_string(),
                description: String::new(),
                image: String::new(),
                attributes: vec![crate::domain::entity::NewEntityAttribute {
                    key: "Idade".into(),
                    value: "50".into(),
                }],
            },
        )
        .expect("entidade")
        .id
    }

    fn relacao(&self, universo: &str, origem: &str, destino: &str, rotulo: &str) -> String {
        crate::application::workspace_service::create_relation(
            &self.banco.database,
            &self.eu,
            universo,
            origem,
            destino,
            rotulo,
        )
        .expect("relação")
    }

    fn evento(&self, universo: &str, titulo: &str, entidade: Option<&str>, ordem: f64) -> String {
        crate::application::workspace_service::create_timeline_event(
            &self.banco.database,
            &self.eu,
            universo,
            crate::domain::workspace::NewTimelineEvent {
                title: titulo.to_string(),
                date: "1400-01-01".into(),
                description: String::new(),
                entity_id: entidade.map(|e| e.to_string()),
                display_date: String::new(),
                sort_key: ordem,
            },
        )
        .expect("evento")
    }

    fn posicao(&self, universo: &str, entidade: &str, x: f64, y: f64) {
        canvas_service::save_entity_position(
            &self.banco.database,
            &self.eu,
            universo,
            entidade,
            x,
            y,
        )
        .expect("posição");
    }

    fn tag(&self, universo: &str, nome: &str) -> String {
        crate::application::knowledge_service::create_tag(
            &self.banco.database,
            &self.eu,
            universo,
            nome,
            "#7d3650",
        )
        .expect("tag")
        .id
    }

    fn marcar(&self, tag: &str, dono_tipo: &str, dono: &str, marcada: bool) {
        crate::application::knowledge_service::set_tag(
            &self.banco.database,
            &self.eu,
            dono_tipo,
            dono,
            tag,
            marcada,
        )
        .expect("marcar");
    }

    fn no(&self, universo: &str, texto: &str, x: f64, y: f64) -> String {
        canvas_service::create_node(
            &self.banco.database,
            &self.store,
            &self.eu,
            universo,
            "note",
            texto,
            "",
            x,
            y,
        )
        .expect("elemento")
        .id
    }

    fn mover_no(&self, no: &str, x: f64, y: f64) {
        canvas_service::save_node_position(&self.banco.database, &self.eu, no, x, y)
            .expect("mover");
    }

    fn escrever_no(&self, no: &str, texto: &str) {
        canvas_service::update_node(
            &self.banco.database,
            &self.store,
            &self.eu,
            no,
            crate::domain::canvas::CanvasNodePatch {
                text: Some(texto.to_string()),
                ..Default::default()
            },
        )
        .expect("editar elemento");
    }

    fn aresta(
        &self,
        universo: &str,
        origem: (&str, &str),
        destino: (&str, &str),
        rotulo: &str,
    ) -> String {
        canvas_service::create_edge(
            &self.banco.database,
            &self.eu,
            universo,
            &crate::domain::canvas::CanvasEndpoint {
                kind: origem.0.into(),
                id: origem.1.into(),
            },
            &crate::domain::canvas::CanvasEndpoint {
                kind: destino.0.into(),
                id: destino.1.into(),
            },
            rotulo,
        )
        .expect("ligação")
        .id
    }

    fn card(&self, universo: &str, titulo: &str, capitulo: Option<&str>) -> String {
        crate::application::planning_service::create(
            &self.banco.database,
            &self.store,
            &self.eu,
            crate::application::planning_service::NovoCard {
                universe_id: universo,
                title: titulo,
                description: "",
                chapter_id: capitulo,
                image: "",
            },
        )
        .expect("card")
    }

    fn campo(&self, universo: &str, nome: &str, tipo: &str, dono: Option<&str>) -> String {
        let escopo = if dono.is_some() { "card" } else { "universal" };
        crate::application::planning_service::create_field_definition(
            &self.banco.database,
            &self.eu,
            crate::application::planning_service::NovaPropriedade {
                universe_id: universo,
                name: nome,
                field_type: tipo,
                options: &[],
                scope: escopo,
                card_id: dono,
            },
        )
        .expect("campo")
        .id
    }

    /// Grava a ficha do card com os valores informados (escalares e relações, como a tela manda).
    fn salvar_card(
        &self,
        universo: &str,
        card: &str,
        titulo: &str,
        capitulo: Option<&str>,
        valores: serde_json::Value,
    ) {
        crate::application::planning_service::save_card(
            &self.banco.database,
            &self.store,
            &self.eu,
            crate::application::planning_service::PlanningCardSaveRequest {
                id: card.to_string(),
                universe_id: universo.to_string(),
                title: titulo.to_string(),
                description: String::new(),
                image: String::new(),
                status: "IDEIAS".into(),
                chapter_id: capitulo.map(|c| c.to_string()),
                field_values: valores,
            },
        )
        .expect("salvar ficha");
    }

    fn mover_card(&self, universo: &str, card: &str, coluna: &str, posicao: i64) {
        crate::application::planning_service::save_order(
            &self.banco.database,
            &self.eu,
            universo,
            &[crate::domain::planning::PlanningCardPlacement {
                id: card.to_string(),
                status: coluna.to_string(),
                sort_order: posicao,
            }],
        )
        .expect("mover");
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

    /// Os eventos deste aparelho, na ordem de emissão: (tipo, id, operação).
    fn eventos(&self) -> Vec<(String, String, String)> {
        let connection = self.banco.connection();
        let mut consulta = connection
            .prepare(
                "SELECT aggregate_type, aggregate_id, operation FROM sync_events
                  WHERE device_id = ?1 ORDER BY seq",
            )
            .expect("consulta");
        consulta
            .query_map([self.eu.device_id()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("eventos")
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
    a.invariante_de_materializacao();
    b.invariante_de_materializacao();
    (em_b, em_a)
}

/// **A lista de tipos vem do codec, não de uma lista à mão aqui.**
///
/// A primeira versão desta asserção tinha os tipos da B2 escritos à mão, e por isso não cobria nada
/// da B3 nem da B4 — um agregado novo entrava sem ninguém conferir a materialização dele. Derivar de
/// `TIPOS_COBERTOS` faz cada etapa nova cair automaticamente dentro do gate.
fn tipos_conferidos() -> &'static [&'static str] {
    sync_codec::TIPOS_COBERTOS
}

impl Aparelho {
    /// **Asserção geral:** para TODO agregado coberto com revisão corrente, o estado canônico no
    /// banco é o payload dessa revisão.
    ///
    /// Vale em repouso — depois de uma sessão inteira. Ficam de fora só os agregados com decisão
    /// aberta ou evento guardado sem aplicar: neles o banco ainda não é, por definição, uma revisão
    /// única. Entre dois eventos de uma mesma mutação (capítulo excluído, ordem ainda por chegar) o
    /// estado do OUTRO agregado pode estar no meio do caminho; por isso a checagem evento a evento
    /// é sobre o agregado aplicado (ver `cada_aplicado_materializa_o_proprio_evento`).
    fn invariante_de_materializacao(&self) {
        let connection = self.banco.connection();
        let mut consulta = connection
            .prepare("SELECT aggregate_type, aggregate_id FROM sync_aggregate_state")
            .expect("consulta");
        let agregados: Vec<(String, String)> = consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("agregados");
        for (tipo, id) in agregados {
            if !tipos_conferidos().contains(&tipo.as_str()) {
                continue;
            }
            let agregado = AggregateRef::new(&tipo, &id);
            if sync_codec::estado_concorrente(&connection, &agregado)
                .expect("concorrente")
                .is_some()
            {
                continue;
            }
            let Some(registrado) =
                sync_codec::payload_da_revisao_corrente(&connection, &agregado).expect("revisão")
            else {
                continue;
            };
            let canonico = sync_codec::ler_canonico(&connection, &agregado)
                .expect("ler")
                .map(|estado| estado.payload);
            assert_eq!(
                canonico.as_deref(),
                Some(registrado.as_str()),
                "{}: {tipo} {id} no banco não é o payload da revisão corrente",
                self.nome
            );
        }
    }
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
    a.convergiu_com(b, "story_position", &arvore.historia);
    a.convergiu_com(b, "book", &arvore.livro);
    a.convergiu_com(b, "book_position", &arvore.livro);
    for capitulo in &arvore.capitulos {
        a.convergiu_com(b, "chapter", capitulo);
        a.convergiu_com(b, "chapter_position", capitulo);
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
fn reordenar_muda_so_as_posicoes_e_chega_igual() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let mut arvore = arvore(&pc, &android);
    let c3 = pc.capitulo(&arvore.livro, "Três").id;
    arvore.capitulos.push(c3.clone());
    sincronizar(&pc, &android);

    let capitulos_antes = pc.eventos_do_tipo("chapter");
    let posicoes_antes = pc.eventos_do_tipo("chapter_position");
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
    // [c1, c2, c3] → [c3, c1, c2]: os três números mudam, então três posições. Nenhuma lista.
    assert_eq!(pc.eventos_do_tipo("chapter_position"), posicoes_antes + 3);

    sincronizar(&pc, &android);
    convergencia_da_arvore(&pc, &android, &arvore);
    assert_eq!(android.ids_dos_capitulos(&arvore.livro), nova);
}

#[test]
fn excluir_capitulo_leva_anexo_marcacao_e_posicoes_sem_mexer_no_irmao() {
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
    pc.convergiu_com(&android, "attachment_position", &anexo.id);
    pc.convergiu_com(&android, "chapter_position", &alvo);
    pc.convergiu_com(&android, "chapter", &arvore.capitulos[1]);
    pc.convergiu_com(&android, "chapter_position", &arvore.capitulos[1]);
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
    pc.convergiu_com(&android, "book_position", &arvore.livro);
    for capitulo in &arvore.capitulos {
        pc.convergiu_com(&android, "chapter", capitulo);
        pc.convergiu_com(&android, "chapter_position", capitulo);
    }
    // O livro irmão não é revisado: posição por item, nada compactado.
    pc.convergiu_com(&android, "book", &segundo);
    pc.convergiu_com(&android, "book_position", &segundo);

    manuscript_service::delete_story(&pc.banco.database, &pc.eu, &arvore.historia)
        .expect("história");
    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0);
    for (tipo, id) in [
        ("story", &arvore.historia),
        ("story_position", &arvore.historia),
        ("book", &segundo),
        ("book_position", &segundo),
        ("chapter", &solto),
        ("chapter_position", &solto),
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

/// Capítulo ligado a card do planejamento: o `SET NULL` **reescreve** o card (B4), nos dois lados.
#[test]
fn excluir_capitulo_ligado_a_card_reescreve_o_card_nos_dois() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);
    let alvo = arvore.capitulos[0].clone();
    let card = pc.card(&arvore.universo, "Cena do porto", Some(&alvo));
    sincronizar(&pc, &android);
    pc.convergiu_com(&android, "planning_item", &card);

    manuscript_service::delete_chapter(&pc.banco.database, &pc.eu, &alvo).expect("excluir");
    assert!(pc
        .canonico("planning_item", &card)
        .expect("card")
        .contains(r#""chapterId":null"#));

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0, "{no_android:?}");
    pc.convergiu_com(&android, "chapter", &alvo);
    pc.convergiu_com(&android, "planning_item", &card);
    assert!(android
        .canonico("planning_item", &card)
        .expect("card")
        .contains(r#""chapterId":null"#));
}

/// O preflight vale para a posição do item: com a posição de um capítulo em decisão aberta, excluir o
/// capítulo apagaria uma das duas versões em silêncio, e é recusado.
#[test]
fn posicao_em_divergencia_recusa_a_exclusao_do_item() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let arvore = arvore(&pc, &android);
    let [c1, c2] = [arvore.capitulos[0].clone(), arvore.capitulos[1].clone()];

    // Os dois trocam os capítulos de lugar, cada um do seu jeito: c1 muda nos dois lados.
    manuscript_service::reorder_chapters(
        &pc.banco.database,
        &pc.eu,
        &arvore.livro,
        &[c2.clone(), c1.clone()],
    )
    .expect("pc reordena");
    let c3 = android.capitulo(&arvore.livro, "Três").id;
    manuscript_service::reorder_chapters(
        &android.banco.database,
        &android.eu,
        &arvore.livro,
        &[c3, c2, c1.clone()],
    )
    .expect("android reordena");
    let (no_android, _) = sincronizar(&pc, &android);
    assert!(
        android
            .divergencias_abertas("chapter_position")
            .contains(&(c1.clone(), "concurrent".to_string())),
        "{no_android:?} {:?}",
        android.divergencias_abertas("chapter_position")
    );

    let erro = manuscript_service::delete_chapter(&android.banco.database, &android.eu, &c1)
        .expect_err("posição em divergência");
    assert!(
        erro.message.contains("chapter_position"),
        "{}",
        erro.message
    );
    assert!(android.canonico("chapter", &c1).is_some());
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

/// **Após todo `Aplicado`, o agregado aplicado é exatamente o evento.** As ações de uma árvore
/// inteira (criação, edição, reorder, exclusão) chegam ao Android uma por vez — cada ação com todos
/// os seus membros, que é a menor unidade que o receptor aplica (B2.2).
#[test]
fn cada_aplicado_materializa_o_proprio_evento() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    let historia = pc.historia(&universo, "Saga").id;
    let livro = pc.livro(&historia, "Livro").id;
    let c1 = pc.capitulo(&livro, "Um").id;
    let c2 = pc.capitulo(&livro, "Dois").id;
    pc.escrever(&c2, "<p>texto</p>");
    manuscript_service::reorder_chapters(
        &pc.banco.database,
        &pc.eu,
        &livro,
        &[c2.clone(), c1.clone()],
    )
    .expect("reordenar");
    manuscript_service::delete_chapter(&pc.banco.database, &pc.eu, &c1).expect("excluir");

    apresentar(&pc, &android);
    apresentar(&android, &pc);
    let vetor = vetor_local(&android.banco.connection()).expect("vetor");
    let eventos = eventos_para(&pc.banco.connection(), &vetor).expect("eventos");
    assert!(eventos.len() >= 10, "{}", eventos.len());
    let mut acoes: Vec<&[crate::domain::sync::EventEnvelope]> = Vec::new();
    let mut inicio = 0;
    for (indice, evento) in eventos.iter().enumerate() {
        if evento.grupo.e_isolado() || evento.grupo.index + 1 == evento.grupo.count {
            acoes.push(&eventos[inicio..=indice]);
            inicio = indice + 1;
        }
    }
    assert_eq!(inicio, eventos.len(), "sobrou ação pela metade no log");
    assert!(acoes.iter().any(|acao| acao.len() > 1));
    for acao in acoes {
        let relatorio = receber_eventos(&mut android.banco.connection(), acao).expect("receber");
        assert_eq!(
            relatorio.aplicados,
            acao.len(),
            "ação {:?} não entrou inteira",
            acao[0].grupo
        );
        for evento in acao {
            let materializado = android.canonico(&evento.aggregate_type, &evento.aggregate_id);
            match evento.operation {
                Operation::Upsert => assert_eq!(
                    materializado.as_deref(),
                    Some(evento.payload.as_str()),
                    "{} {}",
                    evento.aggregate_type,
                    evento.aggregate_id
                ),
                Operation::Delete => {
                    if !sync_codec::existencia_derivada(&evento.aggregate_type) {
                        assert!(materializado.is_none());
                    }
                }
            }
        }
    }
    android.invariante_de_materializacao();
    pc.convergiu_com(&android, "chapter_position", &c2);
    pc.convergiu_com(&android, "chapter_position", &c1);
}

/// **Causalidade cruzada entre origens.**
///
/// ```text
/// C cria c3 (capítulo + posição)
/// A recebe de C, conhece c3, move c3
/// B recebe a posição de c3 que A emitiu ANTES do create de c3, que é de C
///   → a posição fica pendente, não conta como aplicada
/// B recebe os eventos de C
///   → c3 entra, a posição de A é reaplicada, e o estado final é o payload dela
/// ```
#[test]
fn posicao_que_cita_capitulo_de_outra_origem_espera_o_capitulo_chegar() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let c = Aparelho::novo("c");
    let arvore = arvore(&a, &b);
    sincronizar(&a, &c);

    let c3 = c.capitulo(&arvore.livro, "Três").id;
    sincronizar(&a, &c);
    let nova = vec![
        c3.clone(),
        arvore.capitulos[0].clone(),
        arvore.capitulos[1].clone(),
    ];
    manuscript_service::reorder_chapters(&a.banco.database, &a.eu, &arvore.livro, &nova)
        .expect("A reordena");

    apresentar(&a, &b);
    apresentar(&c, &b);
    let vetor_b = vetor_local(&b.banco.connection()).expect("vetor");
    let todos = eventos_para(&a.banco.connection(), &vetor_b).expect("eventos");
    let (de_a, de_c): (Vec<_>, Vec<_>) = todos
        .into_iter()
        .partition(|evento| evento.device_id == a.eu.device_id());
    let posicao_de_a = de_a
        .iter()
        .find(|evento| evento.aggregate_type == "chapter_position" && evento.aggregate_id == c3)
        .expect("a posição de c3 emitida por A")
        .clone();
    assert!(
        !de_c.is_empty(),
        "A precisa ter os eventos de C para repassar"
    );

    let relatorio = receber_eventos(&mut b.banco.connection(), &de_a).expect("B recebe de A");
    assert!(relatorio.pendentes >= 1, "{relatorio:?}");
    let aplicado: bool = b
        .banco
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_applied_events WHERE event_id = ?1)",
            [&posicao_de_a.event_id],
            |row| row.get(0),
        )
        .expect("aplicado");
    assert!(!aplicado, "a posição de c3 foi dada como aplicada sem c3");
    assert!(b.canonico("chapter", &c3).is_none());

    let relatorio = receber_eventos(&mut b.banco.connection(), &de_c).expect("B recebe de C");
    assert!(relatorio.precisam_reconciliar.is_empty(), "{relatorio:?}");
    assert!(b.canonico("chapter", &c3).is_some());
    assert_eq!(
        b.canonico("chapter_position", &c3).as_deref(),
        Some(posicao_de_a.payload.as_str()),
        "o estado final da posição em B é o payload de A"
    );
    b.invariante_de_materializacao();
    a.convergiu_com(&b, "chapter_position", &c3);
    a.convergiu_com(&b, "chapter", &c3);
    assert_eq!(b.ids_dos_capitulos(&arvore.livro), nova);
}

// ═══════════════════════════════════════════════════════════════════════════
// B3: entidade, relação, linha do tempo e posição no grafo
// ═══════════════════════════════════════════════════════════════════════════

/// Universo com duas entidades, uma relação entre elas, um evento ligado à primeira e a posição
/// dela no grafo — criado no `autor` e já sincronizado.
struct Elenco {
    universo: String,
    e1: String,
    e2: String,
    relacao: String,
    evento: String,
}

fn elenco(autor: &Aparelho, outro: &Aparelho) -> Elenco {
    let universo = autor.universo("Terra");
    let e1 = autor.entidade(&universo, "Frodo");
    let e2 = autor.entidade(&universo, "Sam");
    let relacao = autor.relacao(&universo, &e1, &e2, "amigo");
    let evento = autor.evento(&universo, "Partida", Some(&e1), 1.0);
    autor.posicao(&universo, &e1, 10.0, -20.5);
    let (recebido, _) = sincronizar(autor, outro);
    assert_eq!(recebido.divergencias, 0);
    assert!(
        recebido.precisam_reconciliar.is_empty(),
        "{:?}",
        recebido.precisam_reconciliar
    );
    Elenco {
        universo,
        e1,
        e2,
        relacao,
        evento,
    }
}

fn convergencia_do_elenco(a: &Aparelho, b: &Aparelho, elenco: &Elenco) {
    for (tipo, id) in [
        ("entity", elenco.e1.as_str()),
        ("entity", elenco.e2.as_str()),
        ("relation", elenco.relacao.as_str()),
        ("timeline_event", elenco.evento.as_str()),
        ("canvas_entity_position", elenco.e1.as_str()),
    ] {
        a.convergiu_com(b, tipo, id);
    }
}

#[test]
fn entidade_relacao_evento_e_posicao_criados_no_pc_chegam_ao_android() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let elenco = elenco(&pc, &android);
    convergencia_do_elenco(&pc, &android, &elenco);

    // Os atributos são estado interno da ficha: chegaram com ela, sem evento próprio.
    let atributos: i64 = android
        .banco
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM entity_attributes WHERE entity_id = ?1",
            [&elenco.e1],
            |row| row.get(0),
        )
        .expect("atributos");
    assert!(atributos >= 14, "{atributos}");
    assert_eq!(
        android.eventos_do_tipo("entity"),
        0,
        "o Android não emitiu nada"
    );
}

#[test]
fn criado_no_android_e_editado_no_pc_converge() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let elenco = elenco(&android, &pc);
    convergencia_do_elenco(&android, &pc, &elenco);

    crate::application::entity_service::update(
        &pc.banco.database,
        &pc.store,
        &pc.eu,
        &elenco.e1,
        crate::domain::entity::EntityUpdate {
            summary: Some("resumo do PC".into()),
            ..Default::default()
        },
    )
    .expect("editar");
    crate::application::workspace_service::rename_timeline_event(
        &pc.banco.database,
        &pc.eu,
        &elenco.evento,
        "Partida (revisada)",
    )
    .expect("renomear");
    pc.posicao(&elenco.universo, &elenco.e1, 33.0, 44.0);

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0);
    convergencia_do_elenco(&pc, &android, &elenco);
    assert!(android
        .canonico("entity", &elenco.e1)
        .expect("ficha")
        .contains("resumo do PC"));
}

#[test]
fn atributo_da_ficha_e_uma_revisao_da_entidade_e_nao_de_um_agregado_proprio() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let elenco = elenco(&pc, &android);
    let antes = pc.eventos_do_tipo("entity");

    crate::application::entity_service::save_attribute(
        &pc.banco.database,
        &pc.eu,
        crate::domain::entity::EntityAttribute {
            id: "temp_novo".into(),
            entity_id: elenco.e1.clone(),
            key: "Apelido".into(),
            value: "Portador".into(),
            sort_order: 0,
        },
    )
    .expect("atributo");
    assert_eq!(
        pc.eventos_do_tipo("entity"),
        antes + 1,
        "uma revisão da entidade"
    );

    sincronizar(&pc, &android);
    pc.convergiu_com(&android, "entity", &elenco.e1);
    assert!(android
        .canonico("entity", &elenco.e1)
        .expect("ficha")
        .contains("Portador"));
}

#[test]
fn entidade_editada_nos_dois_lados_vira_decisao_sem_perder_nada() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let elenco = elenco(&pc, &android);
    let editar = |aparelho: &Aparelho, texto: &str| {
        crate::application::entity_service::update(
            &aparelho.banco.database,
            &aparelho.store,
            &aparelho.eu,
            &elenco.e1,
            crate::domain::entity::EntityUpdate {
                summary: Some(texto.into()),
                ..Default::default()
            },
        )
        .expect("editar");
    };
    editar(&pc, "versão do PC");
    editar(&android, "versão do Android");

    let (no_android, no_pc) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 1, "{no_android:?}");
    assert_eq!(no_pc.divergencias, 1, "{no_pc:?}");
    assert!(pc
        .canonico("entity", &elenco.e1)
        .expect("ficha")
        .contains("versão do PC"));
    assert!(android
        .canonico("entity", &elenco.e1)
        .expect("ficha")
        .contains("versão do Android"));
    // As duas revisões existem nos dois lados: nada foi sobrescrito.
    for aparelho in [&pc, &android] {
        let revisoes: i64 = aparelho
            .banco
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sync_revision_history WHERE aggregate_type = 'entity' AND aggregate_id = ?1",
                [&elenco.e1],
                |row| row.get(0),
            )
            .expect("revisões");
        assert!(revisoes >= 3, "{revisoes}");
    }
}

#[test]
fn entidade_apagada_em_a_com_relacao_criada_em_b_nao_perde_a_relacao() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let elenco = elenco(&a, &b);
    let terceira = b.entidade(&elenco.universo, "Merry");
    let nova = b.relacao(&elenco.universo, &elenco.e1, &terceira, "primo");
    crate::application::entity_service::delete(&a.banco.database, &a.eu, &elenco.e1)
        .expect("apagar entidade");

    let (em_b, em_a) = sincronizar(&a, &b);

    // Em B a exclusão vira decisão: a relação nova continua lá.
    assert!(em_b.divergencias >= 1, "{em_b:?}");
    assert_eq!(
        b.divergencias_abertas("entity"),
        vec![(elenco.e1.clone(), "parent_deletion_blocked".to_string())]
    );
    assert!(b.canonico("relation", &nova).is_some());
    assert!(b.canonico("entity", &elenco.e1).is_some());
    // Em A a relação nova não tem onde se apoiar: fica pendente, não é descartada.
    assert!(
        em_a.precisam_reconciliar.contains(&nova) || em_a.pendentes >= 1,
        "{em_a:?}"
    );
    assert!(a.canonico("relation", &nova).is_none());
    let guardado: bool = a
        .banco
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_events WHERE aggregate_id = ?1)",
            [&nova],
            |row| row.get(0),
        )
        .expect("guardado");
    assert!(guardado);
}

#[test]
fn entidade_apagada_em_a_com_evento_editado_em_b_nao_perde_a_edicao() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let elenco = elenco(&a, &b);
    crate::application::workspace_service::rename_timeline_event(
        &b.banco.database,
        &b.eu,
        &elenco.evento,
        "Editado no B",
    )
    .expect("renomear");
    crate::application::entity_service::delete(&a.banco.database, &a.eu, &elenco.e1)
        .expect("apagar");

    let (em_b, _) = sincronizar(&a, &b);
    assert!(em_b.divergencias >= 1, "{em_b:?}");
    // A edição de B continua no banco de B, e o evento não sumiu em nenhum dos dois.
    assert!(b
        .canonico("timeline_event", &elenco.evento)
        .expect("evento")
        .contains("Editado no B"));
    assert!(a.canonico("timeline_event", &elenco.evento).is_some());
}

/// O `SET NULL` sem concorrência: o evento sobrevive nos dois lados, sem a entidade.
#[test]
fn entidade_apagada_deixa_o_evento_vivo_com_entidade_nula_nos_dois() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let elenco = elenco(&a, &b);
    crate::application::entity_service::delete(&a.banco.database, &a.eu, &elenco.e1)
        .expect("apagar");

    let (em_b, _) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    for (tipo, id) in [
        ("entity", elenco.e1.as_str()),
        ("relation", elenco.relacao.as_str()),
        ("canvas_entity_position", elenco.e1.as_str()),
        ("timeline_event", elenco.evento.as_str()),
    ] {
        a.convergiu_com(&b, tipo, id);
    }
    let evento = b
        .canonico("timeline_event", &elenco.evento)
        .expect("evento");
    assert!(evento.contains(r#""entityId":null"#), "{evento}");
    assert!(b.canonico("entity", &elenco.e1).is_none());
    assert!(b.canonico("relation", &elenco.relacao).is_none());
    assert!(b.canonico("canvas_entity_position", &elenco.e1).is_none());
}

/// Relação, evento e posição que chegam antes das entidades de que dependem: esperam, não são
/// dadas como aplicadas, e entram quando as entidades chegam.
///
/// Os dependentes vêm de **outra origem** (C), contíguos na sequência dela: o que os segura aqui é a
/// dependência de domínio, não a lacuna de `seq`.
#[test]
fn dependencia_que_chega_depois_espera_e_converge() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let c = Aparelho::novo("c");
    let universo = a.universo("Terra");
    // B já tem o universo: o que vai segurar os eventos de C é a dependência das ENTIDADES.
    sincronizar(&a, &b);
    let e1 = a.entidade(&universo, "Frodo");
    let e2 = a.entidade(&universo, "Sam");
    sincronizar(&a, &c);
    let relacao = c.relacao(&universo, &e1, &e2, "amigo");
    let evento = c.evento(&universo, "Partida", Some(&e1), 1.0);
    c.posicao(&universo, &e1, 1.0, 2.0);

    apresentar(&a, &b);
    apresentar(&c, &b);
    let vetor = vetor_local(&b.banco.connection()).expect("vetor");
    let todos = eventos_para(&c.banco.connection(), &vetor).expect("eventos");
    let (de_c, de_a): (Vec<_>, Vec<_>) = todos
        .into_iter()
        .partition(|evento| evento.device_id == c.eu.device_id());
    assert_eq!(
        de_c.len(),
        3,
        "relação, evento e posição, contíguos na origem C"
    );

    let relatorio = receber_eventos(&mut b.banco.connection(), &de_c).expect("de C");
    assert_eq!(relatorio.aplicados, 0, "{relatorio:?}");
    assert_eq!(relatorio.pendentes, 3, "{relatorio:?}");
    for evento in &de_c {
        let aplicado: bool = b
            .banco
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_applied_events WHERE event_id = ?1)",
                [&evento.event_id],
                |row| row.get(0),
            )
            .expect("aplicado");
        assert!(
            !aplicado,
            "{} avançou sem a entidade",
            evento.aggregate_type
        );
    }
    assert_eq!(b.contar("SELECT COUNT(*) FROM relations"), 0);
    assert_eq!(b.contar("SELECT COUNT(*) FROM timeline_events"), 0);
    assert_eq!(b.contar("SELECT COUNT(*) FROM canvas_entity_positions"), 0);

    let relatorio = receber_eventos(&mut b.banco.connection(), &de_a).expect("de A");
    assert!(relatorio.precisam_reconciliar.is_empty(), "{relatorio:?}");
    assert_eq!(relatorio.pendentes, 0, "{relatorio:?}");
    for (tipo, id) in [
        ("entity", e1.as_str()),
        ("entity", e2.as_str()),
        ("relation", relacao.as_str()),
        ("timeline_event", evento.as_str()),
        ("canvas_entity_position", e1.as_str()),
    ] {
        c.convergiu_com(&b, tipo, id);
    }
    b.invariante_de_materializacao();
}

/// Três origens: a relação de A depende de uma entidade de C. Chegando em qualquer ordem, converge.
#[test]
fn relacao_de_a_com_entidade_de_c_converge_em_qualquer_ordem() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let c = Aparelho::novo("c");
    let universo = a.universo("Terra");
    let e1 = a.entidade(&universo, "Frodo");
    sincronizar(&a, &b);
    sincronizar(&a, &c);

    let de_c = c.entidade(&universo, "Gandalf");
    sincronizar(&a, &c);
    let relacao = a.relacao(&universo, &e1, &de_c, "guia");

    apresentar(&a, &b);
    apresentar(&c, &b);
    let vetor = vetor_local(&b.banco.connection()).expect("vetor");
    let todos = eventos_para(&a.banco.connection(), &vetor).expect("eventos");
    let (eventos_de_a, eventos_de_c): (Vec<_>, Vec<_>) = todos
        .into_iter()
        .partition(|evento| evento.device_id == a.eu.device_id());

    // Primeiro o que é de A (a relação cita uma entidade de C que ainda não chegou).
    let relatorio = receber_eventos(&mut b.banco.connection(), &eventos_de_a).expect("de A");
    assert!(relatorio.pendentes >= 1, "{relatorio:?}");
    assert!(b.canonico("relation", &relacao).is_none());

    let relatorio = receber_eventos(&mut b.banco.connection(), &eventos_de_c).expect("de C");
    assert!(relatorio.precisam_reconciliar.is_empty(), "{relatorio:?}");
    assert_eq!(relatorio.pendentes, 0, "{relatorio:?}");
    a.convergiu_com(&b, "relation", &relacao);
    a.convergiu_com(&b, "entity", &de_c);
    b.invariante_de_materializacao();
}

/// Posição do grafo é autoral e sincroniza; `clear_layout` exclui cada posição, com evento.
#[test]
fn limpar_layout_exclui_as_posicoes_nos_dois_aparelhos() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let elenco = elenco(&pc, &android);
    pc.convergiu_com(&android, "canvas_entity_position", &elenco.e1);

    canvas_service::clear_layout(&pc.banco.database, &pc.eu, &elenco.universo).expect("limpar");
    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0, "{no_android:?}");
    pc.convergiu_com(&android, "canvas_entity_position", &elenco.e1);
    assert_eq!(
        android.contar("SELECT COUNT(*) FROM canvas_entity_positions"),
        0
    );
}

/// Trocar a entidade de um evento não é operação do app: um payload assim é inconsistência.
#[test]
fn evento_que_troca_de_entidade_e_recusado() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let elenco = elenco(&pc, &android);
    let payload = android
        .canonico("timeline_event", &elenco.evento)
        .expect("evento")
        .replace(&elenco.e1, &elenco.e2);
    let base = {
        let connection = android.banco.connection();
        sync_codec::revisao_corrente(
            &connection,
            &AggregateRef::new("timeline_event", &elenco.evento),
        )
        .expect("rev")
        .expect("tem revisão")
    };
    let seq = pc.contar(
        "SELECT COALESCE(MAX(seq), 0) FROM sync_events
          WHERE device_id IN (SELECT device_id FROM sync_devices WHERE is_self = 1)",
    ) + 1;
    let mut envelope = envelope_de_origem(
        pc.eu.device_id(),
        seq,
        &elenco.universo,
        &AggregateRef::new("timeline_event", &elenco.evento),
        Operation::Upsert,
        &payload,
        &base,
    );
    envelope.signature = pc.eu.sign(&envelope);

    let erro = receber_eventos(&mut android.banco.connection(), &[envelope])
        .expect_err("trocar a entidade do evento não é operação do app");
    assert!(
        erro.message.contains("não é uma operação do app"),
        "{}",
        erro.message
    );
    assert!(android
        .canonico("timeline_event", &elenco.evento)
        .expect("evento")
        .contains(&elenco.e1));
}

// ═══════════════════════════════════════════════════════════════════════════
// B3, revisão: simetria local ↔ remoto, identidade da posição, destacamento
// ═══════════════════════════════════════════════════════════════════════════

/// **Nenhum caminho local grava o que o apply remoto recusaria.** A validação é a mesma função nos
/// dois lados; falhar acontece dentro da `Mutacao`, então domínio e evento voltam juntos.
#[test]
fn escrita_local_entre_universos_diferentes_nao_grava_nem_emite() {
    let pc = Aparelho::novo("pc");
    let u1 = pc.universo("Primeiro");
    let u2 = pc.universo("Segundo");
    let daqui = pc.entidade(&u1, "Frodo");
    let de_la = pc.entidade(&u2, "Estranho");
    let eventos_antes = pc.contar("SELECT COUNT(*) FROM sync_events");

    // Relação declarada em u2 com uma ponta de u1.
    let erro = crate::application::workspace_service::create_relation(
        &pc.banco.database,
        &pc.eu,
        &u2,
        &daqui,
        &de_la,
        "impossível",
    )
    .expect_err("relação entre universos");
    assert!(erro.message.contains("incompatível"), "{}", erro.message);
    assert_eq!(pc.contar("SELECT COUNT(*) FROM relations"), 0);

    // Evento em u2 apontando para entidade de u1.
    let erro = crate::application::workspace_service::create_timeline_event(
        &pc.banco.database,
        &pc.eu,
        &u2,
        crate::domain::workspace::NewTimelineEvent {
            title: "Impossível".into(),
            date: "1400-01-01".into(),
            description: String::new(),
            entity_id: Some(daqui.clone()),
            display_date: String::new(),
            sort_key: 1.0,
        },
    )
    .expect_err("evento entre universos");
    assert!(erro.message.contains("incompatível"), "{}", erro.message);
    assert_eq!(pc.contar("SELECT COUNT(*) FROM timeline_events"), 0);

    // Posição declarada em u2 para entidade de u1.
    let erro =
        canvas_service::save_entity_position(&pc.banco.database, &pc.eu, &u2, &daqui, 1.0, 2.0)
            .expect_err("posição entre universos");
    assert!(erro.message.contains("incompatível"), "{}", erro.message);
    assert_eq!(pc.contar("SELECT COUNT(*) FROM canvas_entity_positions"), 0);

    assert_eq!(
        pc.contar("SELECT COUNT(*) FROM sync_events"),
        eventos_antes,
        "nenhum evento pode ter sobrado de uma escrita recusada"
    );
}

/// Uma entidade tem UMA posição. Duas linhas (o schema deixaria, a PK é composta) não têm estado
/// canônico, e um evento que criaria a segunda é recusado sem gravar.
#[test]
fn posicao_da_entidade_e_unica() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let u1 = pc.universo("Primeiro");
    let u2 = pc.universo("Segundo");
    let entidade = pc.entidade(&u1, "Frodo");
    pc.posicao(&u1, &entidade, 5.0, 6.0);
    sincronizar(&pc, &android);
    pc.convergiu_com(&android, "canvas_entity_position", &entidade);

    // Um evento remoto que declara a mesma entidade em OUTRO universo: recusado, e nada gravado.
    let payload = format!(
        r#"{{"entityId":"{entidade}","universeId":"{u2}","positionX":9.0,"positionY":9.0}}"#
    );
    let base = {
        let connection = android.banco.connection();
        sync_codec::revisao_corrente(
            &connection,
            &AggregateRef::new("canvas_entity_position", &entidade),
        )
        .expect("rev")
        .expect("tem revisão")
    };
    let seq = pc.contar(
        "SELECT COALESCE(MAX(seq), 0) FROM sync_events
          WHERE device_id IN (SELECT device_id FROM sync_devices WHERE is_self = 1)",
    ) + 1;
    // Montado pelo helper canônico de ação remota (B6, item 8).
    let acao = crate::infrastructure::sqlite::test_support::AcaoRemota::da(&pc.eu)
        .a_partir_do_seq(seq)
        .no_universo(&u2)
        .upsert(
            AggregateRef::new("canvas_entity_position", &entidade),
            &payload,
            &base,
        )
        .assinada();
    let erro = receber_eventos(&mut android.banco.connection(), &acao)
        .expect_err("segunda posição para a mesma entidade");
    assert!(
        erro.message.contains("uma entidade tem uma posição"),
        "{}",
        erro.message
    );
    assert_eq!(
        android.contar("SELECT COUNT(*) FROM canvas_entity_positions"),
        1,
        "a segunda linha não pode ter sido criada"
    );

    // Desde a B6 (migration 25) a unicidade é do BANCO, e não só do código que lê: nem uma linha
    // escrita por fora do app consegue criar a segunda posição da mesma entidade. A leitura
    // canônica mantém a recusa por dentro — ela é a rede embaixo, para um banco que chegue aqui
    // sem o índice (restaurado de versão antiga, por exemplo).
    let erro = android
        .banco
        .connection()
        .execute(
            "INSERT INTO canvas_entity_positions
               (universe_id, entity_id, position_x, position_y, updated_at)
             VALUES (?1, ?2, 1.0, 1.0, '2026-01-01')",
            [&u2, &entidade],
        )
        .expect_err("o banco tem de recusar a segunda linha");
    assert!(
        erro.to_string().contains("UNIQUE"),
        "a recusa não veio do índice: {erro}"
    );
    assert_eq!(
        android.contar("SELECT COUNT(*) FROM canvas_entity_positions"),
        1
    );
}

/// A entidade de um evento se destaca (`SET NULL`) e **não** se reancora.
#[test]
fn evento_destacado_nao_pode_ser_reancorado() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let elenco = elenco(&pc, &android);

    // A exclusão da entidade destaca o evento nos dois aparelhos.
    crate::application::entity_service::delete(&pc.banco.database, &pc.eu, &elenco.e1)
        .expect("apagar entidade");
    let (em_android, _) = sincronizar(&pc, &android);
    assert_eq!(em_android.divergencias, 0, "{em_android:?}");
    let evento_agora = android
        .canonico("timeline_event", &elenco.evento)
        .expect("evento");
    assert!(
        evento_agora.contains(r#""entityId":null"#),
        "{evento_agora}"
    );

    // Um evento posterior tentando reancorar a entidade: recusado.
    let payload = evento_agora.replace(
        r#""entityId":null"#,
        &format!(r#""entityId":"{}""#, elenco.e2),
    );
    let base = {
        let connection = android.banco.connection();
        sync_codec::revisao_corrente(
            &connection,
            &AggregateRef::new("timeline_event", &elenco.evento),
        )
        .expect("rev")
        .expect("tem revisão")
    };
    let seq = pc.contar(
        "SELECT COALESCE(MAX(seq), 0) FROM sync_events
          WHERE device_id IN (SELECT device_id FROM sync_devices WHERE is_self = 1)",
    ) + 1;
    let mut envelope = envelope_de_origem(
        pc.eu.device_id(),
        seq,
        &elenco.universo,
        &AggregateRef::new("timeline_event", &elenco.evento),
        Operation::Upsert,
        &payload,
        &base,
    );
    envelope.signature = pc.eu.sign(&envelope);
    let erro = receber_eventos(&mut android.banco.connection(), &[envelope.clone()])
        .expect_err("reancorar não é operação do app");
    assert!(erro.message.contains("Reancorar"), "{}", erro.message);

    // O estado continua nulo, e o cursor não finge que aplicou.
    assert!(android
        .canonico("timeline_event", &elenco.evento)
        .expect("evento")
        .contains(r#""entityId":null"#));
    let aplicado: bool = android
        .banco
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_applied_events WHERE event_id = ?1)",
            [&envelope.event_id],
            |row| row.get(0),
        )
        .expect("aplicado");
    assert!(!aplicado);
    android.invariante_de_materializacao();
}

// ═══════════════════════════════════════════════════════════════════════════
// B4: card, quadro e propriedades do planejamento
// ═══════════════════════════════════════════════════════════════════════════

/// Universo com capítulo, entidade, história, um campo universal e dois cards — no `autor`, já
/// sincronizado.
struct Quadro {
    universo: String,
    historia: String,
    capitulo: String,
    entidade: String,
    campo_texto: String,
    campo_entidade: String,
    campo_historia: String,
    card_a: String,
    card_b: String,
}

fn quadro(autor: &Aparelho, outro: &Aparelho) -> Quadro {
    let universo = autor.universo("Terra");
    let historia = autor.historia(&universo, "Saga").id;
    let livro = autor.livro(&historia, "Livro I").id;
    let capitulo = autor.capitulo(&livro, "Um").id;
    let entidade = autor.entidade(&universo, "Frodo");
    let campo_texto = autor.campo(&universo, "Tom", "text", None);
    let campo_entidade = autor.campo(&universo, "Personagens", "character", None);
    let campo_historia = autor.campo(&universo, "Histórias", "story", None);
    let card_a = autor.card(&universo, "Cena do porto", Some(&capitulo));
    let card_b = autor.card(&universo, "Cena da ponte", None);
    autor.salvar_card(
        &universo,
        &card_a,
        "Cena do porto",
        Some(&capitulo),
        serde_json::json!({
            campo_texto.clone(): "tenso",
            campo_entidade.clone(): [entidade.clone()],
            campo_historia.clone(): [historia.clone()],
        }),
    );
    autor.salvar_card(
        &universo,
        &card_b,
        "Cena da ponte",
        None,
        serde_json::json!({ campo_texto.clone(): "calmo" }),
    );
    let (recebido, _) = sincronizar(autor, outro);
    assert_eq!(recebido.divergencias, 0);
    assert!(
        recebido.precisam_reconciliar.is_empty(),
        "{:?}",
        recebido.precisam_reconciliar
    );
    Quadro {
        universo,
        historia,
        capitulo,
        entidade,
        campo_texto,
        campo_entidade,
        campo_historia,
        card_a,
        card_b,
    }
}

fn convergencia_do_quadro(a: &Aparelho, b: &Aparelho, quadro: &Quadro) {
    for (tipo, id) in [
        ("planning_item", quadro.card_a.as_str()),
        ("planning_item_position", quadro.card_a.as_str()),
        ("planning_item", quadro.card_b.as_str()),
        ("planning_item_position", quadro.card_b.as_str()),
        ("planning_field_definition", quadro.campo_texto.as_str()),
        ("planning_field_position", quadro.campo_texto.as_str()),
        ("planning_field_definition", quadro.campo_entidade.as_str()),
        ("planning_field_position", quadro.campo_entidade.as_str()),
        ("planning_field_definition", quadro.campo_historia.as_str()),
        ("planning_field_position", quadro.campo_historia.as_str()),
    ] {
        a.convergiu_com(b, tipo, id);
    }
}

#[test]
fn card_quadro_e_propriedades_criados_no_pc_chegam_ao_android() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let quadro = quadro(&pc, &android);
    convergencia_do_quadro(&pc, &android, &quadro);

    // Escalares no JSON, relações nas linhas — do jeito que este banco representa as duas coisas.
    let card = android
        .canonico("planning_item", &quadro.card_a)
        .expect("card");
    assert!(card.contains("\"value\":\"tenso\""), "{card}");
    assert!(
        card.contains(&quadro.entidade) && card.contains(&quadro.historia),
        "{card}"
    );
    assert_eq!(
        android.contar("SELECT COUNT(*) FROM planning_field_links"),
        2,
        "as duas relações do card A"
    );
}

/// Mover card mexe **só** na posição dele; editar a ficha mexe no conteúdo (e na posição, se a etapa
/// mudar).
#[test]
fn mover_card_nao_revisa_o_conteudo_do_card() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let quadro = quadro(&pc, &android);
    let cards_antes = pc.eventos_do_tipo("planning_item");
    let posicoes_antes = pc.eventos_do_tipo("planning_item_position");

    pc.mover_card(&quadro.universo, &quadro.card_b, "ESCREVENDO", 0);
    assert_eq!(
        pc.eventos_do_tipo("planning_item"),
        cards_antes,
        "mover revisou o conteúdo do card"
    );
    assert_eq!(
        pc.eventos_do_tipo("planning_item_position"),
        posicoes_antes + 1,
        "mover um card revisou outra posição além da dele"
    );

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0, "{no_android:?}");
    convergencia_do_quadro(&pc, &android, &quadro);
    let coluna: String = android
        .banco
        .connection()
        .query_row(
            "SELECT status FROM planning_items WHERE id = ?1",
            [&quadro.card_b],
            |row| row.get(0),
        )
        .expect("coluna");
    assert_eq!(coluna, "ESCREVENDO");
}

#[test]
fn ficha_do_card_editada_no_android_chega_ao_pc() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let quadro = quadro(&pc, &android);
    android.salvar_card(
        &quadro.universo,
        &quadro.card_b,
        "Cena da ponte (revisada)",
        Some(&quadro.capitulo),
        serde_json::json!({ quadro.campo_texto.clone(): "urgente" }),
    );
    let (no_pc, _) = sincronizar(&android, &pc);
    assert_eq!(no_pc.divergencias, 0, "{no_pc:?}");
    convergencia_do_quadro(&android, &pc, &quadro);
    assert!(pc
        .canonico("planning_item", &quadro.card_b)
        .expect("card")
        .contains("urgente"));
}

#[test]
fn excluir_card_leva_os_campos_exclusivos_dele_e_as_posicoes() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let quadro = quadro(&pc, &android);
    let exclusivo = pc.campo(
        &quadro.universo,
        "Só deste card",
        "text",
        Some(&quadro.card_b),
    );
    sincronizar(&pc, &android);
    pc.convergiu_com(&android, "planning_field_definition", &exclusivo);

    crate::application::planning_service::delete(
        &pc.banco.database,
        &pc.eu,
        &quadro.card_b,
        &quadro.universo,
    )
    .expect("excluir card");

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0, "{no_android:?}");
    pc.convergiu_com(&android, "planning_item", &quadro.card_b);
    pc.convergiu_com(&android, "planning_field_definition", &exclusivo);
    pc.convergiu_com(&android, "planning_field_position", &exclusivo);
    pc.convergiu_com(&android, "planning_item_position", &quadro.card_b);
    // O card irmão não é revisado.
    pc.convergiu_com(&android, "planning_item_position", &quadro.card_a);
    assert!(android.canonico("planning_item", &quadro.card_b).is_none());
    assert!(android
        .canonico("planning_field_definition", &exclusivo)
        .is_none());
}

/// **O gatilho que reescreve vários cards.** Excluir a propriedade tira o valor dela de TODOS os
/// cards; cada card afetado ganha revisão própria, e o outro aparelho converge.
#[test]
fn excluir_propriedade_reescreve_todos_os_cards_afetados() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let quadro = quadro(&pc, &android);
    let cards_antes = pc.eventos_do_tipo("planning_item");

    crate::application::planning_service::delete_field_definition(
        &pc.banco.database,
        &pc.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("excluir propriedade");

    // Os dois cards tinham valor no campo: duas revisões, uma por card.
    assert_eq!(
        pc.eventos_do_tipo("planning_item"),
        cards_antes + 2,
        "o gatilho reescreveu cards sem evento"
    );
    for card in [&quadro.card_a, &quadro.card_b] {
        assert!(
            !pc.canonico("planning_item", card)
                .expect("card")
                .contains(&quadro.campo_texto),
            "o valor do campo continua no card"
        );
    }

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0, "{no_android:?}");
    pc.convergiu_com(&android, "planning_field_definition", &quadro.campo_texto);
    pc.convergiu_com(&android, "planning_item", &quadro.card_a);
    pc.convergiu_com(&android, "planning_item", &quadro.card_b);
}

#[test]
fn excluir_historia_e_entidade_tira_a_ligacao_do_card_com_revisao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let quadro = quadro(&pc, &android);

    crate::application::entity_service::delete(&pc.banco.database, &pc.eu, &quadro.entidade)
        .expect("excluir entidade");
    assert!(!pc
        .canonico("planning_item", &quadro.card_a)
        .expect("card")
        .contains(&quadro.entidade));
    // Antes de seguir: a revisão corrente do card já tem de ser este estado, sem a ligação.
    pc.invariante_de_materializacao();

    manuscript_service::delete_story(&pc.banco.database, &pc.eu, &quadro.historia)
        .expect("excluir história");
    let card = pc.canonico("planning_item", &quadro.card_a).expect("card");
    assert!(!card.contains(&quadro.historia), "{card}");
    assert!(
        card.contains(r#""chapterId":null"#),
        "o capítulo foi com a história: {card}"
    );

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0, "{no_android:?}");
    pc.convergiu_com(&android, "planning_item", &quadro.card_a);
    pc.convergiu_com(&android, "entity", &quadro.entidade);
    pc.convergiu_com(&android, "story", &quadro.historia);
    assert_eq!(
        android.contar("SELECT COUNT(*) FROM planning_field_links"),
        0,
        "as ligações do card tinham de ter saído"
    );
}

/// **Concorrência não altera o estado vivo antes da decisão.**
///
/// ```text
/// A edita o valor do campo F no card
/// B apaga o campo F  — UMA ação: reescrita do card (sem F) + posição de F + F
/// A recebe a ação:  a reescrita do card é concorrente → a ação inteira vira UMA decisão, sobre a
///                   raiz (a exclusão de F); nenhum membro toca o domínio (B2.2)
/// ```
#[test]
fn propriedade_apagada_no_outro_lado_nao_muda_o_card_antes_da_decisao() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let quadro = quadro(&a, &b);

    a.salvar_card(
        &quadro.universo,
        &quadro.card_a,
        "Cena do porto",
        Some(&quadro.capitulo),
        serde_json::json!({ quadro.campo_texto.clone(): "mudado em A" }),
    );
    crate::application::planning_service::delete_field_definition(
        &b.banco.database,
        &b.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("B apaga a propriedade");

    sincronizar(&a, &b);

    // Em A nada foi executado por conta própria: o campo continua, com o valor que A escreveu.
    let card = a.canonico("planning_item", &quadro.card_a).expect("card");
    assert!(
        card.contains("mudado em A"),
        "o valor de A foi apagado antes da decisão: {card}"
    );
    assert!(
        a.canonico("planning_field_definition", &quadro.campo_texto)
            .is_some(),
        "a propriedade foi apagada em A antes da decisão"
    );
    assert_eq!(
        a.divergencias_abertas("planning_field_definition"),
        vec![(
            quadro.campo_texto.clone(),
            "parent_deletion_blocked".to_string()
        )]
    );
    assert_eq!(
        a.divergencias_abertas("planning_item"),
        vec![],
        "o card é efeito da ação: a decisão é da exclusão de F, não uma segunda decisão"
    );
    assert!(
        crate::infrastructure::sqlite::sync_codec::estado_concorrente(
            &a.banco.connection(),
            &crate::domain::sync::AggregateRef::new("planning_item", &quadro.card_a),
        )
        .expect("estado")
        .is_some(),
        "o card faz parte da ação em decisão"
    );

    // A sessão seguinte não duplica decisão nenhuma.
    let abertas_antes = a.divergencias_abertas("planning_item").len()
        + a.divergencias_abertas("planning_field_definition").len();
    let (_, em_a) = sincronizar(&a, &b);
    assert_eq!(em_a.divergencias, 0, "{em_a:?}");
    assert_eq!(
        a.divergencias_abertas("planning_item").len()
            + a.divergencias_abertas("planning_field_definition").len(),
        abertas_antes
    );
    a.invariante_de_materializacao();
}

/// Entrega num sentido só: `para` recebe o que `de` tem.
fn entregar(de: &Aparelho, para: &Aparelho) -> Relatorio {
    apresentar(de, para);
    apresentar(para, de);
    let vetor = vetor_local(&para.banco.connection()).expect("vetor");
    let eventos = eventos_para(&de.banco.connection(), &vetor).expect("eventos");
    let relatorio = receber_eventos(&mut para.banco.connection(), &eventos).expect("receber");
    para.invariante_de_materializacao();
    relatorio
}

fn revisao(aparelho: &Aparelho, tipo: &str, id: &str) -> Option<String> {
    sync_codec::revisao_corrente(&aparelho.banco.connection(), &AggregateRef::new(tipo, id))
        .expect("revisão")
}

/// **Manter o local fecha a causalidade de todo membro — inclusive do que já estava igual.**
///
/// ```text
/// A muda F no card_b e depois o tira; edita F no card_a  card_b de A: sem F, revisão de A
/// B apaga F: UMA ação (card_a e card_b sem F, posição, F) card_b de B: o mesmo payload, revisão de B
/// A recebe: card_a concorrente e diferente → uma decisão; A mantém o local
/// C, que recebeu a ação de B, edita card_b a partir da revisão de B
/// A recebe C: sequencial, nenhuma decisão nova
/// ```
///
/// Ignorar o card_b por já estar igual deixaria A na revisão dele, e a edição de C — que partiu da
/// revisão de B, que A conhece — viraria uma decisão entre dois estados que nunca divergiram.
#[test]
fn manter_local_adota_a_revisao_da_origem_quando_o_estado_ja_e_igual() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let c = Aparelho::novo("c");
    let quadro = quadro(&a, &b);
    let (em_c, _) = sincronizar(&a, &c);
    assert_eq!(em_c.divergencias, 0, "{em_c:?}");

    // A chega ao mesmo card_b de B por outro caminho (duas edições): mesmo payload, outra revisão.
    a.salvar_card(
        &quadro.universo,
        &quadro.card_b,
        "Cena da ponte",
        None,
        serde_json::json!({ quadro.campo_texto.clone(): "agitado" }),
    );
    a.salvar_card(
        &quadro.universo,
        &quadro.card_b,
        "Cena da ponte",
        None,
        serde_json::json!({}),
    );
    a.salvar_card(
        &quadro.universo,
        &quadro.card_a,
        "Cena do porto",
        Some(&quadro.capitulo),
        serde_json::json!({ quadro.campo_texto.clone(): "mudado em A" }),
    );
    crate::application::planning_service::delete_field_definition(
        &b.banco.database,
        &b.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("B apaga a propriedade");

    let rev_de_b = revisao(&b, "planning_item", &quadro.card_b).expect("revisão de B");
    assert_eq!(
        a.canonico("planning_item", &quadro.card_b),
        b.canonico("planning_item", &quadro.card_b),
        "o cenário exige o mesmo card_b nos dois"
    );
    assert_ne!(
        revisao(&a, "planning_item", &quadro.card_b).as_deref(),
        Some(rev_de_b.as_str()),
        "o cenário exige revisões diferentes"
    );

    let em_c = entregar(&b, &c);
    assert_eq!(em_c.divergencias, 0, "{em_c:?}");
    let em_a = entregar(&b, &a);
    assert_eq!(em_a.divergencias, 1, "{em_a:?}");
    let decisao = a
        .divergencias_abertas("planning_field_definition")
        .first()
        .map(|(id, _)| id.clone())
        .expect("a decisão da ação");
    let id_da_divergencia: String = a
        .banco
        .connection()
        .query_row(
            "SELECT id FROM sync_divergences
              WHERE aggregate_type = 'planning_field_definition' AND aggregate_id = ?1
                AND resolved_at = ''",
            [&decisao],
            |row| row.get(0),
        )
        .expect("divergência");
    crate::application::resolucao_divergencia::resolver(
        &a.banco.database,
        &a.eu,
        &id_da_divergencia,
        crate::application::resolucao_divergencia::Escolha::ManterLocal,
    )
    .expect("manter o local");
    assert_eq!(
        revisao(&a, "planning_item", &quadro.card_b).as_deref(),
        Some(rev_de_b.as_str()),
        "A adotou a revisão de B para o estado que já era igual"
    );
    a.invariante_de_materializacao();

    assert_eq!(
        revisao(&c, "planning_item", &quadro.card_b).as_deref(),
        Some(rev_de_b.as_str())
    );
    c.salvar_card(
        &quadro.universo,
        &quadro.card_b,
        "Cena da ponte ao luar",
        None,
        serde_json::json!({}),
    );
    let em_a = entregar(&c, &a);
    assert_eq!(em_a.divergencias, 0, "{em_a:?}");
    assert!(a.divergencias().is_empty(), "{:?}", a.divergencias());
    assert_eq!(
        a.canonico("planning_item", &quadro.card_b),
        c.canonico("planning_item", &quadro.card_b),
        "a edição de C entrou em A como sequencial"
    );
}

/// **Manter o local quando o local é a ausência.** A apagou o card_b; B apagou a propriedade, e a
/// ação de B reescreve o card_b. Manter o local não pode ignorar o card: nasce aqui uma exclusão que
/// descende da reescrita de B.
#[test]
fn manter_local_com_o_card_apagado_aqui_exclui_sobre_a_revisao_da_origem() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let quadro = quadro(&a, &b);

    crate::application::planning_service::delete(
        &a.banco.database,
        &a.eu,
        &quadro.card_b,
        &quadro.universo,
    )
    .expect("A apaga o card");
    crate::application::planning_service::delete_field_definition(
        &b.banco.database,
        &b.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("B apaga a propriedade");
    let rev_de_b = revisao(&b, "planning_item", &quadro.card_b).expect("revisão de B");

    let em_a = entregar(&b, &a);
    assert_eq!(em_a.divergencias, 1, "{em_a:?}");
    let (campo, tipo) = a
        .divergencias_abertas("planning_field_definition")
        .first()
        .cloned()
        .expect("a decisão da ação");
    assert_eq!(tipo, "parent_deletion_blocked");
    let id_da_divergencia: String = a
        .banco
        .connection()
        .query_row(
            "SELECT id FROM sync_divergences
              WHERE aggregate_type = 'planning_field_definition' AND aggregate_id = ?1
                AND resolved_at = ''",
            [&campo],
            |row| row.get(0),
        )
        .expect("divergência");
    crate::application::resolucao_divergencia::resolver(
        &a.banco.database,
        &a.eu,
        &id_da_divergencia,
        crate::application::resolucao_divergencia::Escolha::ManterLocal,
    )
    .expect("manter o local");

    assert!(a.canonico("planning_item", &quadro.card_b).is_none());
    let (operacao, base): (String, String) = a
        .banco
        .connection()
        .query_row(
            "SELECT operation, base_rev FROM sync_events
              WHERE device_id = ?1 AND aggregate_type = 'planning_item' AND aggregate_id = ?2
              ORDER BY seq DESC LIMIT 1",
            [a.eu.device_id(), quadro.card_b.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("o último evento de A sobre o card_b");
    assert_eq!(
        (operacao.as_str(), base.as_str()),
        ("delete", rev_de_b.as_str()),
        "a ausência escolhida tem de descender da reescrita de B"
    );
    a.invariante_de_materializacao();
}

/// **Manter o local quando os dois lados excluíram, por caminhos diferentes.** A renomeou F e
/// depois o apagou; B apagou F direto. As exclusões têm revisões diferentes. Manter o local não
/// emite nada — a ausência já é o estado dos dois —, mas o tombstone daqui passa a ser o da origem:
/// um evento futuro que parta da exclusão de B é sequencial aqui.
#[test]
fn manter_local_com_os_dois_lados_excluindo_adota_o_tombstone_da_origem() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let quadro = quadro(&a, &b);

    crate::application::planning_service::rename_field_definition(
        &a.banco.database,
        &a.eu,
        &quadro.campo_texto,
        &quadro.universo,
        "Clima",
    )
    .expect("A renomeia");
    a.salvar_card(
        &quadro.universo,
        &quadro.card_a,
        "Cena do porto",
        Some(&quadro.capitulo),
        serde_json::json!({ quadro.campo_texto.clone(): "mudado em A" }),
    );
    crate::application::planning_service::delete_field_definition(
        &a.banco.database,
        &a.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("A apaga");
    crate::application::planning_service::delete_field_definition(
        &b.banco.database,
        &b.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("B apaga");
    let tombstone_de = |aparelho: &Aparelho| -> String {
        aparelho
            .banco
            .connection()
            .query_row(
                "SELECT deleted_rev FROM sync_tombstones
                  WHERE aggregate_type = 'planning_field_definition' AND aggregate_id = ?1",
                [&quadro.campo_texto],
                |row| row.get(0),
            )
            .expect("tombstone")
    };
    let exclusao_de_b = tombstone_de(&b);
    assert_ne!(
        tombstone_de(&a),
        exclusao_de_b,
        "o cenário exige exclusões diferentes"
    );

    let em_a = entregar(&b, &a);
    assert_eq!(em_a.divergencias, 1, "{em_a:?}");
    let id_da_divergencia: String = a
        .banco
        .connection()
        .query_row(
            "SELECT id FROM sync_divergences
              WHERE kind = 'parent_deletion_blocked' AND resolved_at = ''",
            [],
            |row| row.get(0),
        )
        .expect("a decisão da ação");
    let eventos_antes = a.eventos();
    crate::application::resolucao_divergencia::resolver(
        &a.banco.database,
        &a.eu,
        &id_da_divergencia,
        crate::application::resolucao_divergencia::Escolha::ManterLocal,
    )
    .expect("manter o local");

    assert_eq!(tombstone_de(&a), exclusao_de_b);
    assert!(revisao(&a, "planning_field_definition", &quadro.campo_texto).is_none());
    // A ausência igual não gera evento. O card_a de A é outro estado (perdeu as ligações ao ser
    // salvo) e ganha revisão sobre a de B, pela regra de estado diferente.
    let novos: Vec<_> = a.eventos()[eventos_antes.len()..].to_vec();
    assert_eq!(
        novos,
        vec![(
            "planning_item".to_string(),
            quadro.card_a.clone(),
            "upsert".to_string()
        )],
        "a ausência igual gerou evento"
    );
    assert!(a.divergencias().is_empty(), "{:?}", a.divergencias());
    a.invariante_de_materializacao();
}

/// Resolver mantendo o local: o campo e o valor continuam.
#[test]
fn manter_local_preserva_a_propriedade_e_o_valor() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let quadro = quadro(&a, &b);
    a.salvar_card(
        &quadro.universo,
        &quadro.card_a,
        "Cena do porto",
        Some(&quadro.capitulo),
        serde_json::json!({ quadro.campo_texto.clone(): "mudado em A" }),
    );
    crate::application::planning_service::delete_field_definition(
        &b.banco.database,
        &b.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("B apaga");
    sincronizar(&a, &b);

    let bloqueada = a
        .divergencias_abertas("planning_field_definition")
        .first()
        .map(|(id, _)| id.clone())
        .expect("a exclusão bloqueada");
    let id_da_divergencia: String = a
        .banco
        .connection()
        .query_row(
            "SELECT id FROM sync_divergences
              WHERE aggregate_type = 'planning_field_definition' AND aggregate_id = ?1
                AND resolved_at = ''",
            [&bloqueada],
            |row| row.get(0),
        )
        .expect("divergência");

    crate::application::resolucao_divergencia::resolver(
        &a.banco.database,
        &a.eu,
        &id_da_divergencia,
        crate::application::resolucao_divergencia::Escolha::ManterLocal,
    )
    .expect("manter o local");

    assert!(a
        .canonico("planning_field_definition", &quadro.campo_texto)
        .is_some());
    assert!(a
        .canonico("planning_item", &quadro.card_a)
        .expect("card")
        .contains("mudado em A"));
    a.invariante_de_materializacao();
}

/// Resolver aceitando o remoto: o campo e o valor somem — e o card ganha revisão por isso.
#[test]
fn aceitar_a_exclusao_da_propriedade_tira_o_campo_e_o_valor_com_revisao() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let quadro = quadro(&a, &b);
    a.salvar_card(
        &quadro.universo,
        &quadro.card_a,
        "Cena do porto",
        Some(&quadro.capitulo),
        serde_json::json!({ quadro.campo_texto.clone(): "mudado em A" }),
    );
    crate::application::planning_service::delete_field_definition(
        &b.banco.database,
        &b.eu,
        &quadro.campo_texto,
        &quadro.universo,
    )
    .expect("B apaga");
    sincronizar(&a, &b);

    let id_da_divergencia: String = a
        .banco
        .connection()
        .query_row(
            "SELECT id FROM sync_divergences
              WHERE aggregate_type = 'planning_field_definition' AND resolved_at = ''",
            [],
            |row| row.get(0),
        )
        .expect("divergência da propriedade");

    // Uma decisão só, da ação: aceitar tira F e recalcula o card de A sem ele — a edição de A no
    // resto do card não é trocada pelo card de B.
    crate::application::resolucao_divergencia::resolver(
        &a.banco.database,
        &a.eu,
        &id_da_divergencia,
        crate::application::resolucao_divergencia::Escolha::AceitarRemoto,
    )
    .expect("aceitar a exclusão");

    assert!(a
        .canonico("planning_field_definition", &quadro.campo_texto)
        .is_none());
    let card = a.canonico("planning_item", &quadro.card_a).expect("card");
    assert!(
        !card.contains("mudado em A"),
        "o valor do campo apagado continuou: {card}"
    );
    // O card mudou por causa da exclusão: isso tem de ser uma revisão dele, não uma alteração muda.
    a.invariante_de_materializacao();

    // E a revisão nova descende da reescrita de B: B a recebe como sequencial, sem nova decisão.
    let (em_b, em_a) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    assert_eq!(em_a.divergencias, 0, "{em_a:?}");
    a.convergiu_com(&b, "planning_item", &quadro.card_a);
    b.invariante_de_materializacao();
}

/// **`Delete` domina `Rewrite` do mesmo agregado.** O card que possui um campo exclusivo, com valor
/// dele dentro do próprio card: apagar o card apaga o campo, e o efeito do campo sobre o card é
/// absorvido — nenhuma revisão intermediária do card.
#[test]
fn excluir_card_com_campo_exclusivo_preenchido_nao_revisa_o_card() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let quadro = quadro(&pc, &android);
    let exclusivo = pc.campo(
        &quadro.universo,
        "Só deste card",
        "text",
        Some(&quadro.card_b),
    );
    pc.salvar_card(
        &quadro.universo,
        &quadro.card_b,
        "Cena da ponte",
        None,
        serde_json::json!({
            quadro.campo_texto.clone(): "calmo",
            exclusivo.clone(): "valor do campo exclusivo",
        }),
    );
    sincronizar(&pc, &android);
    assert!(pc
        .canonico("planning_item", &quadro.card_b)
        .expect("card")
        .contains("valor do campo exclusivo"));

    let antes = pc.eventos().len();
    crate::application::planning_service::delete(
        &pc.banco.database,
        &pc.eu,
        &quadro.card_b,
        &quadro.universo,
    )
    .expect("excluir card");

    let eventos = pc.eventos();
    let novos = &eventos[antes..];
    assert_eq!(
        novos,
        &[
            (
                "planning_item_position".to_string(),
                quadro.card_b.clone(),
                "delete".to_string()
            ),
            (
                "planning_field_position".to_string(),
                exclusivo.clone(),
                "delete".to_string()
            ),
            (
                "planning_field_definition".to_string(),
                exclusivo.clone(),
                "delete".to_string()
            ),
            (
                "planning_item".to_string(),
                quadro.card_b.clone(),
                "delete".to_string()
            ),
        ],
        "o card condenado não pode ganhar revisão pela cascata do campo dele"
    );

    let (no_android, _) = sincronizar(&pc, &android);
    assert_eq!(no_android.divergencias, 0, "{no_android:?}");
    pc.convergiu_com(&android, "planning_item", &quadro.card_b);
    pc.convergiu_com(&android, "planning_field_definition", &exclusivo);
    pc.convergiu_com(&android, "planning_item_position", &quadro.card_b);
    pc.convergiu_com(&android, "planning_field_position", &exclusivo);
}

/// Card movido num lado e ficha editada no outro: agregados diferentes, nenhuma decisão.
#[test]
fn card_movido_em_a_e_conteudo_editado_em_b_convergem_sem_conflito() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let quadro = quadro(&a, &b);

    a.mover_card(&quadro.universo, &quadro.card_a, "REVISAO", 0);
    b.salvar_card(
        &quadro.universo,
        &quadro.card_a,
        "Cena do porto (editada em B)",
        Some(&quadro.capitulo),
        serde_json::json!({ quadro.campo_texto.clone(): "tenso" }),
    );

    let (em_b, em_a) = sincronizar(&a, &b);
    assert_eq!(
        em_b.divergencias + em_a.divergencias,
        0,
        "mover e editar não podem conflitar: {em_b:?} {em_a:?}"
    );
    convergencia_do_quadro(&a, &b, &quadro);
    assert!(a
        .canonico("planning_item", &quadro.card_a)
        .expect("card")
        .contains("editada em B"));
    let coluna: String = a
        .banco
        .connection()
        .query_row(
            "SELECT status FROM planning_items WHERE id = ?1",
            [&quadro.card_a],
            |row| row.get(0),
        )
        .expect("coluna");
    assert_eq!(coluna, "REVISAO");
}

/// Card de outra origem que cita entidade, história e propriedade que ainda não chegaram: espera,
/// não é dado como aplicado, e converge quando o resto chega.
#[test]
fn card_que_depende_de_outra_origem_espera_e_converge() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let c = Aparelho::novo("c");
    let universo = a.universo("Terra");
    // B já tem o universo: o que segura os eventos de C é a dependência do card.
    sincronizar(&a, &b);
    let historia = a.historia(&universo, "Saga").id;
    let entidade = a.entidade(&universo, "Frodo");
    let campo_entidade = a.campo(&universo, "Personagens", "character", None);
    let campo_historia = a.campo(&universo, "Histórias", "story", None);
    sincronizar(&a, &c);

    let card = c.card(&universo, "Cena de C", None);
    c.salvar_card(
        &universo,
        &card,
        "Cena de C",
        None,
        serde_json::json!({
            campo_entidade.clone(): [entidade.clone()],
            campo_historia.clone(): [historia.clone()],
        }),
    );

    apresentar(&a, &b);
    apresentar(&c, &b);
    let vetor = vetor_local(&b.banco.connection()).expect("vetor");
    let todos = eventos_para(&c.banco.connection(), &vetor).expect("eventos");
    let (de_c, de_a): (Vec<_>, Vec<_>) = todos
        .into_iter()
        .partition(|evento| evento.device_id == c.eu.device_id());

    let relatorio = receber_eventos(&mut b.banco.connection(), &de_c).expect("de C");
    // A criação do card (sem ligações) entra; a ficha que cita a entidade e a história de A espera.
    assert!(relatorio.pendentes >= 1, "{relatorio:?}");
    assert_eq!(
        b.contar("SELECT COUNT(*) FROM planning_field_links"),
        0,
        "nenhuma ligação pode ter sido gravada sem os alvos"
    );
    assert_ne!(
        b.canonico("planning_item", &card),
        c.canonico("planning_item", &card),
        "o card ainda não pode ter convergido"
    );

    let relatorio = receber_eventos(&mut b.banco.connection(), &de_a).expect("de A");
    assert!(relatorio.precisam_reconciliar.is_empty(), "{relatorio:?}");
    assert_eq!(relatorio.pendentes, 0, "{relatorio:?}");
    c.convergiu_com(&b, "planning_item", &card);
    c.convergiu_com(&b, "planning_item_position", &card);
    b.invariante_de_materializacao();
}

// ═══════════════════════════════════════════════════════════════════════════
// B5 — conhecimento e canvas
// ═══════════════════════════════════════════════════════════════════════════

/// **Mover não é editar.** Conteúdo e posição do elemento livre são agregados diferentes, então
/// arrastar num aparelho e escrever no outro não colide com nada.
#[test]
fn mover_o_elemento_num_lado_e_escrever_nele_no_outro_nao_conflita() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let no = a.no(&universo, "rascunho", 0.0, 0.0);
    sincronizar(&a, &b);

    a.mover_no(&no, 120.0, -40.0);
    b.escrever_no(&no, "rota comercial antiga");

    let (em_b, em_a) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    assert_eq!(em_a.divergencias, 0, "{em_a:?}");
    a.convergiu_com(&b, "canvas_node", &no);
    a.convergiu_com(&b, "canvas_node_position", &no);
    assert!(a
        .canonico("canvas_node", &no)
        .expect("nó")
        .contains("rota comercial antiga"));
    assert!(b
        .canonico("canvas_node_position", &no)
        .expect("posição")
        .contains("120"));
}

/// Excluir o elemento emite a árvore inteira, e o cursor da origem continua contíguo.
#[test]
fn excluir_o_elemento_leva_a_ligacao_e_a_posicao_com_eventos_proprios() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let entidade = a.entidade(&universo, "Frodo");
    let no = a.no(&universo, "rascunho", 0.0, 0.0);
    let ligacao = a.aresta(&universo, ("canvas", &no), ("entity", &entidade), "cita");
    sincronizar(&a, &b);

    let antes = a.eventos().len();
    canvas_service::delete_node(&a.banco.database, &a.eu, &no).expect("excluir");
    let eventos = a.eventos();
    assert_eq!(
        &eventos[antes..],
        &[
            (
                "canvas_edge".to_string(),
                ligacao.clone(),
                "delete".to_string()
            ),
            (
                "canvas_node_position".to_string(),
                no.clone(),
                "delete".to_string()
            ),
            ("canvas_node".to_string(), no.clone(), "delete".to_string()),
        ],
        "descendentes antes do pai, cada um com o seu evento"
    );

    let (em_b, _) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    a.convergiu_com(&b, "canvas_node", &no);
    a.convergiu_com(&b, "canvas_edge", &ligacao);
    assert_eq!(b.contar("SELECT COUNT(*) FROM canvas_edges"), 0);
}

/// **Apagar a entidade leva a ligação do canvas.** Sem o gatilho da migration 22, ela ficaria no
/// arquivo do outro aparelho para sempre — invisível na tela e viva no banco.
#[test]
fn excluir_a_entidade_leva_a_ligacao_do_canvas_nos_dois_aparelhos() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let entidade = a.entidade(&universo, "Frodo");
    let no = a.no(&universo, "rascunho", 0.0, 0.0);
    let ligacao = a.aresta(&universo, ("canvas", &no), ("entity", &entidade), "cita");
    sincronizar(&a, &b);
    assert_eq!(b.contar("SELECT COUNT(*) FROM canvas_edges"), 1);

    crate::application::entity_service::delete(&a.banco.database, &a.eu, &entidade)
        .expect("excluir entidade");

    let (em_b, _) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    a.convergiu_com(&b, "canvas_edge", &ligacao);
    assert_eq!(
        b.contar("SELECT COUNT(*) FROM canvas_edges"),
        0,
        "a ligação invisível continuou no arquivo do outro aparelho"
    );
}

/// Ligação criada num lado para um elemento que o outro apagou: a exclusão chega e é bloqueada,
/// porque apagar o elemento levaria junto uma ligação que a origem não conhecia.
#[test]
fn elemento_apagado_num_lado_com_ligacao_criada_no_outro_vira_decisao() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let entidade = a.entidade(&universo, "Frodo");
    let no = a.no(&universo, "rascunho", 0.0, 0.0);
    sincronizar(&a, &b);

    canvas_service::delete_node(&a.banco.database, &a.eu, &no).expect("A apaga o elemento");
    let ligacao = b.aresta(&universo, ("canvas", &no), ("entity", &entidade), "cita");

    let (em_b, _) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 1, "{em_b:?}");
    assert_eq!(
        b.divergencias_abertas("canvas_node"),
        vec![(no.clone(), "parent_deletion_blocked".to_string())]
    );
    assert!(
        b.canonico("canvas_edge", &ligacao).is_some(),
        "a ligação de B foi apagada antes da decisão"
    );
    assert!(b.canonico("canvas_node", &no).is_some());
    b.invariante_de_materializacao();
}

/// **A ligação chega antes de uma das pontas.** Dependência, não erro.
///
/// O elemento é de B, a ligação é de A: a origem de A é contígua e completa, e mesmo assim a
/// ligação não pode materializar até o elemento de B chegar. O que segura é a **ponta que falta**,
/// não uma lacuna de `seq`.
#[test]
fn ligacao_que_chega_antes_da_ponta_de_outra_origem_espera_por_ela() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let c = Aparelho::novo("c");
    let universo = a.universo("Terra");
    let entidade = a.entidade(&universo, "Frodo");
    sincronizar(&a, &b);

    let no = b.no(&universo, "rascunho", 0.0, 0.0);
    sincronizar(&a, &b);
    let ligacao = a.aresta(&universo, ("canvas", &no), ("entity", &entidade), "cita");

    apresentar(&a, &c);
    apresentar(&c, &a);
    apresentar(&b, &c);
    apresentar(&c, &b);

    // C recebe só o que A tem de si mesma... e A já replicou o nó de B, então para provar a
    // espera de verdade C recebe primeiro apenas os eventos DA ORIGEM A.
    let para_c = eventos_para(
        &a.banco.connection(),
        &vetor_local(&c.banco.connection()).expect("vetor c"),
    )
    .expect("a → c");
    let so_de_a: Vec<_> = para_c
        .into_iter()
        .filter(|envelope| envelope.device_id == a.eu.device_id())
        .collect();
    let em_c = receber_eventos(&mut c.banco.connection(), &so_de_a).expect("c recebe");
    assert!(
        c.canonico("canvas_edge", &ligacao).is_none(),
        "a ligação materializou sem a ponta: {em_c:?}"
    );
    assert!(c.canonico("entity", &entidade).is_some());

    // O nó de B chega, e a ligação entra.
    sincronizar(&b, &c);
    sincronizar(&a, &c);
    a.convergiu_com(&c, "canvas_edge", &ligacao);
    a.convergiu_com(&c, "canvas_node", &no);
}

/// Marcar a mesma tag no mesmo dono nos dois aparelhos é **a mesma marcação**: a identidade é
/// `tagId:ownerType:ownerId`, não o `id` aleatório da linha. Converge sem divergência.
#[test]
fn a_mesma_marcacao_criada_dos_dois_lados_converge_sem_divergencia() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let historia = a.historia(&universo, "Saga").id;
    let livro = a.livro(&historia, "Livro I").id;
    let capitulo = a.capitulo(&livro, "Um").id;
    let tag = a.tag(&universo, "Reescrever");
    sincronizar(&a, &b);

    a.marcar(&tag, "chapter", &capitulo, true);
    b.marcar(&tag, "chapter", &capitulo, true);

    let (em_b, em_a) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    assert_eq!(em_a.divergencias, 0, "{em_a:?}");
    let marcacao = sync_codec::manuscrito::id_da_atribuicao(&tag, "chapter", &capitulo);
    a.convergiu_com(&b, "tag_assignment", &marcacao);
    assert_eq!(
        a.contar("SELECT COUNT(*) FROM content_tag_assignments"),
        1,
        "dois ids aleatórios viraram duas marcações"
    );
}

/// Tag renomeada num lado e marcada no outro: agregados diferentes, sem conflito.
#[test]
fn tag_renomeada_num_lado_e_marcada_no_outro_nao_conflita() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let entidade = a.entidade(&universo, "Frodo");
    let tag = a.tag(&universo, "Rever");
    sincronizar(&a, &b);

    crate::application::knowledge_service::update_tag(
        &a.banco.database,
        &a.eu,
        &tag,
        "Reescrever",
        "#2f6f7d",
    )
    .expect("renomear");
    b.marcar(&tag, "entity", &entidade, true);

    let (em_b, em_a) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    assert_eq!(em_a.divergencias, 0, "{em_a:?}");
    a.convergiu_com(&b, "content_tag", &tag);
    let marcacao = sync_codec::manuscrito::id_da_atribuicao(&tag, "entity", &entidade);
    a.convergiu_com(&b, "tag_assignment", &marcacao);
    assert!(a
        .canonico("content_tag", &tag)
        .expect("tag")
        .contains("Reescrever"));
}

/// **Tag homônima criada dos dois lados.** O schema não deixa as duas existirem, e nenhuma está
/// errada. Nada é aplicado, nada é alterado: vira decisão do escritor.
#[test]
fn tag_homonima_criada_dos_dois_lados_vira_decisao_em_vez_de_travar() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    sincronizar(&a, &b);

    let tag_de_a = a.tag(&universo, "Mar");
    let tag_de_b = b.tag(&universo, "mar"); // COLLATE NOCASE: é o mesmo nome

    let (em_b, em_a) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 1, "{em_b:?}");
    assert_eq!(em_a.divergencias, 1, "{em_a:?}");
    assert_eq!(
        a.divergencias_abertas("content_tag"),
        vec![(tag_de_b.clone(), "tag_name_conflict".to_string())]
    );
    assert_eq!(
        b.divergencias_abertas("content_tag"),
        vec![(tag_de_a.clone(), "tag_name_conflict".to_string())]
    );
    // Cada um continua com a sua, intacta, e nenhuma revisão corrente foi inventada.
    assert!(a.canonico("content_tag", &tag_de_b).is_none());
    assert!(a.canonico("content_tag", &tag_de_a).is_some());
    a.invariante_de_materializacao();
    b.invariante_de_materializacao();

    // Renomear a tag daqui libera o nome. A tag que estava esperando NÃO entra sozinha: o evento
    // dela já está marcado como aplicado, e reaplicá-lo é a resolução da divergência — trabalho da
    // etapa F, com a tela de decisão. O que a B5 garante é que nada foi perdido nem alterado.
    crate::application::knowledge_service::update_tag(
        &a.banco.database,
        &a.eu,
        &tag_de_a,
        "Mar aberto",
        "#7d3650",
    )
    .expect("renomear");
    assert!(a.canonico("content_tag", &tag_de_b).is_none());
    assert_eq!(
        a.divergencias_abertas("content_tag"),
        vec![(tag_de_b.clone(), "tag_name_conflict".to_string())],
        "a decisão continua aberta até alguém tomá-la"
    );
    a.invariante_de_materializacao();
}

/// Tag apagada num lado, card editado no outro: a exclusão é bloqueada, e o card de B não muda
/// antes da decisão. Mesmo padrão da propriedade de card na B4.
#[test]
fn tag_apagada_num_lado_com_card_editado_no_outro_bloqueia_a_exclusao() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let tag = a.tag(&universo, "Mar");
    let campo_tag = a.campo(&universo, "Tags", "tags", None);
    let card = a.card(&universo, "Cena do porto", None);
    a.salvar_card(
        &universo,
        &card,
        "Cena do porto",
        None,
        serde_json::json!({ campo_tag.clone(): [tag.clone()] }),
    );
    let (recebido, _) = sincronizar(&a, &b);
    assert_eq!(recebido.divergencias, 0, "{recebido:?}");

    b.salvar_card(
        &universo,
        &card,
        "Cena do porto ao amanhecer",
        None,
        serde_json::json!({ campo_tag.clone(): [tag.clone()] }),
    );
    crate::application::knowledge_service::delete_tag(&a.banco.database, &a.eu, &tag)
        .expect("A apaga a tag");

    let (em_b, _) = sincronizar(&a, &b);
    assert!(em_b.divergencias >= 1, "{em_b:?}");
    assert_eq!(
        b.divergencias_abertas("content_tag"),
        vec![(tag.clone(), "parent_deletion_blocked".to_string())]
    );
    assert!(
        b.canonico("planning_item", &card)
            .expect("card")
            .contains(&tag),
        "a ligação do card de B sumiu antes da decisão"
    );
    assert!(b.canonico("content_tag", &tag).is_some());
    b.invariante_de_materializacao();
}

// ═══════════════════════════════════════════════════════════════════════════
// B5 — revisão: um agregado não atravessa a fronteira entre dois universos
// ═══════════════════════════════════════════════════════════════════════════

/// Dois universos no mesmo aparelho, com um dono de cada tipo no segundo.
struct DoisUniversos {
    tag_u1: String,
    entidade_u2: String,
    capitulo_u2: String,
    card_u2: String,
    universo_u2: String,
}

fn dois_universos(a: &Aparelho) -> DoisUniversos {
    let u1 = a.universo("Primeiro");
    let u2 = a.universo("Segundo");
    let historia = a.historia(&u2, "Saga").id;
    let livro = a.livro(&historia, "Livro I").id;
    DoisUniversos {
        tag_u1: a.tag(&u1, "Mar"),
        entidade_u2: a.entidade(&u2, "Frodo"),
        capitulo_u2: a.capitulo(&livro, "Um").id,
        card_u2: a.card(&u2, "Cena", None),
        universo_u2: u2,
    }
}

/// **Uma tag não marca conteúdo de outro universo.** Nem localmente, nem por evento recebido.
///
/// A regra é uma função só, cobrada nos dois lados. Se ela existisse só no remoto, este aparelho
/// emitiria um evento que o outro recusaria — e a marcação ficaria aqui, inválida, para sempre.
#[test]
fn marcar_tag_de_um_universo_em_dono_de_outro_e_recusado_localmente() {
    let a = Aparelho::novo("a");
    let cenario = dois_universos(&a);

    let antes_das_linhas = a.contar("SELECT COUNT(*) FROM content_tag_assignments");
    let antes_dos_eventos = a.eventos().len();

    for (tipo, dono) in [
        ("entity", cenario.entidade_u2.as_str()),
        ("chapter", cenario.capitulo_u2.as_str()),
        ("planning", cenario.card_u2.as_str()),
        ("universe", cenario.universo_u2.as_str()),
    ] {
        let erro = crate::application::knowledge_service::set_tag(
            &a.banco.database,
            &a.eu,
            tipo,
            dono,
            &cenario.tag_u1,
            true,
        )
        .expect_err("dono de outro universo tinha que ser recusado");
        assert!(
            erro.message.contains("outro universo"),
            "{tipo}: {}",
            erro.message
        );
    }

    assert_eq!(
        a.contar("SELECT COUNT(*) FROM content_tag_assignments"),
        antes_das_linhas,
        "a marcação entre universos gravou linha"
    );
    assert_eq!(
        a.eventos().len(),
        antes_dos_eventos,
        "a marcação entre universos gerou evento"
    );
}

/// O mesmo pelo lado remoto: o evento **não** é aplicado, e o cursor não avança por cima dele.
#[test]
fn marcacao_entre_universos_que_chega_de_fora_nao_e_aplicada() {
    let a = Aparelho::novo("a");
    let cenario = dois_universos(&a);

    let agregado = AggregateRef::new(
        "tag_assignment",
        sync_codec::manuscrito::id_da_atribuicao(&cenario.tag_u1, "entity", &cenario.entidade_u2),
    );
    // Montado À MÃO, na ordem dos campos da struct canônica. O macro `json!` ordena as chaves
    // alfabeticamente, e um payload fora da ordem canônica seria pego pela invariante de
    // materialização — o teste passaria sem nunca exercer a regra que ele diz testar.
    let payload = format!(
        r#"{{"tagId":"{}","ownerType":"entity","ownerId":"{}"}}"#,
        cenario.tag_u1, cenario.entidade_u2
    );
    let b = Aparelho::novo("b");
    apresentar(&b, &a);
    let mut envelope = envelope_de_origem(
        b.eu.device_id(),
        1,
        &cenario.universo_u2,
        &agregado,
        Operation::Upsert,
        &payload,
        "",
    );
    envelope.signature = b.eu.sign(&envelope);
    let event_id = envelope.event_id.clone();

    let erro = receber_eventos(&mut a.banco.connection(), &[envelope])
        .expect_err("marcação entre universos");
    assert!(
        erro.message.contains("outro universo"),
        "recusou pelo motivo errado: {}",
        erro.message
    );

    assert_eq!(a.contar("SELECT COUNT(*) FROM content_tag_assignments"), 0);
    let aplicado: i64 = a
        .banco
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sync_applied_events WHERE event_id = ?1",
            [&event_id],
            |row| row.get(0),
        )
        .expect("contar");
    assert_eq!(aplicado, 0, "o evento inválido foi marcado como aplicado");
}

/// **Um anexo não pertence a conteúdo de outro universo.**
#[test]
fn anexo_com_dono_de_outro_universo_e_recusado_sem_deixar_linha_nem_evento() {
    let a = Aparelho::novo("a");
    let u1 = a.universo("Primeiro");
    let u2 = a.universo("Segundo");
    let entidade_u2 = a.entidade(&u2, "Frodo");

    let antes = a.eventos().len();
    let erro = canvas_service::create_attachment(
        &a.banco.database,
        &a.store,
        &a.eu,
        &u1,
        "entity",
        &entidade_u2,
        "data:image/png;base64,YQ==",
        "",
    )
    .expect_err("anexo entre universos");
    assert!(
        erro.message.contains("outro universo"),
        "recusou pelo motivo errado: {}",
        erro.message
    );

    assert_eq!(a.contar("SELECT COUNT(*) FROM attachments"), 0);
    assert_eq!(a.eventos().len(), antes, "o anexo inválido gerou evento");
}

/// **O dono do anexo é imutável.** Um evento com o mesmo id e outro dono não move a imagem.
#[test]
fn anexo_que_chega_com_outro_dono_e_recusado_e_o_original_fica_intacto() {
    let a = Aparelho::novo("a");
    let universo = a.universo("Terra");
    let dona = a.entidade(&universo, "Frodo");
    let outra = a.entidade(&universo, "Sam");
    let anexo = canvas_service::create_attachment(
        &a.banco.database,
        &a.store,
        &a.eu,
        &universo,
        "entity",
        &dona,
        "data:image/png;base64,YQ==",
        "retrato",
    )
    .expect("anexo");

    // Parte do payload CANÔNICO deste anexo e troca só o dono: assim a única diferença é a que
    // está sendo testada. Montar o JSON à parte arriscaria diferir na ordem das chaves, e a
    // invariante de materialização recusaria o evento antes de a regra do dono ser consultada.
    let payload = a
        .canonico("attachment", &anexo.id)
        .expect("o anexo tem estado canônico")
        .replace(&dona, &outra);
    let b = Aparelho::novo("b");
    apresentar(&b, &a);
    // A partir da revisão corrente daqui: assim o evento é SEQUENCIAL e chega à validação. Com
    // base vazia ele seria concorrente, viraria divergência e a regra de imutabilidade nem seria
    // consultada — nada se moveria, mas também nada seria provado.
    let base = {
        let connection = a.banco.connection();
        sync_codec::revisao_corrente(&connection, &AggregateRef::new("attachment", &anexo.id))
            .expect("rev")
            .expect("o anexo tem revisão")
    };
    let mut envelope = envelope_de_origem(
        b.eu.device_id(),
        1,
        &universo,
        &AggregateRef::new("attachment", &anexo.id),
        Operation::Upsert,
        &payload,
        &base,
    );
    envelope.signature = b.eu.sign(&envelope);

    let erro = receber_eventos(&mut a.banco.connection(), &[envelope])
        .expect_err("o dono do anexo é imutável");
    assert!(
        erro.message.contains("imutável"),
        "recusou pelo motivo errado: {}",
        erro.message
    );

    let dono_agora: String = a
        .banco
        .connection()
        .query_row(
            "SELECT owner_id FROM attachments WHERE id = ?1",
            [&anexo.id],
            |row| row.get(0),
        )
        .expect("ler o anexo");
    assert_eq!(dono_agora, dona, "o anexo mudou de dono");
}

/// **O conflito de nome guarda as DUAS identidades que colidiram.**
///
/// Renomear a tag daqui depois não pode apagar o rastro: a etapa F precisa saber quem colidiu com
/// quem para poder oferecer "são a mesma tag", decisão que tem de juntar as marcações das duas.
#[test]
fn o_conflito_de_nome_guarda_a_tag_daqui_e_sobrevive_ao_rename() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    sincronizar(&a, &b);

    let t1 = a.tag(&universo, "Mar");
    let t2 = b.tag(&universo, "mar");
    sincronizar(&a, &b);

    let guardado = |aparelho: &Aparelho| -> (String, String) {
        aparelho
            .banco
            .connection()
            .query_row(
                "SELECT aggregate_id, related_aggregate_id FROM sync_divergences
                  WHERE kind = 'tag_name_conflict' AND resolved_at = ''",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("o conflito")
    };
    assert_eq!(
        guardado(&a),
        (t2.clone(), t1.clone()),
        "em A: a tag que chegou é T2, e a daqui é T1"
    );
    assert_eq!(
        guardado(&b),
        (t1.clone(), t2.clone()),
        "em B é o espelho disso"
    );

    // Renomear T1 não apaga com quem T2 colidiu.
    crate::application::knowledge_service::update_tag(
        &a.banco.database,
        &a.eu,
        &t1,
        "Mar aberto",
        "#7d3650",
    )
    .expect("renomear");
    assert_eq!(
        guardado(&a),
        (t2, t1),
        "o rename apagou a identidade com que o conflito aconteceu"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// B2.2 — posição por item: a unidade de conflito da ordem é o item, não a lista
// ═══════════════════════════════════════════════════════════════════════════

impl Aparelho {
    /// Toda divergência aberta, de qualquer tipo. Os testes de criação concorrente exigem ZERO, e não
    /// só zero de posição: uma lista inteira que ainda existisse colidiria aqui.
    fn divergencias(&self) -> Vec<(String, String, String)> {
        let connection = self.banco.connection();
        let mut consulta = connection
            .prepare(
                "SELECT aggregate_type, aggregate_id, kind FROM sync_divergences
                  WHERE resolved_at = '' ORDER BY aggregate_type, aggregate_id",
            )
            .expect("consulta");
        consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("divergências")
    }

    fn reordenar_capitulos(&self, livro: &str, ordem: &[&str]) {
        let ids: Vec<String> = ordem.iter().map(|id| id.to_string()).collect();
        manuscript_service::reorder_chapters(&self.banco.database, &self.eu, livro, &ids)
            .expect("reordenar");
    }

    fn ids_dos_capitulos(&self, livro: &str) -> Vec<String> {
        manuscript_service::list_chapters_by_book(&self.banco.database, livro)
            .expect("capítulos")
            .into_iter()
            .map(|capitulo| capitulo.id)
            .collect()
    }

    fn anexar_em(&self, universo: &str, dono_tipo: &str, dono: &str, bytes: &str) -> String {
        canvas_service::create_attachment(
            &self.banco.database,
            &self.store,
            &self.eu,
            universo,
            dono_tipo,
            dono,
            bytes,
            "",
        )
        .expect("anexo")
        .id
    }
}

/// Criação concorrente converge sem decisão, e as duas telas mostram a MESMA ordem.
fn converge_sem_divergencia(
    a: &Aparelho,
    b: &Aparelho,
    tipo_da_posicao: &str,
    criados: &[&str],
    ordem_em: impl Fn(&Aparelho) -> Vec<String>,
) {
    let (em_b, em_a) = sincronizar(a, b);
    assert!(
        a.divergencias().is_empty(),
        "divergências em {}: {:?}",
        a.nome,
        a.divergencias()
    );
    assert!(
        b.divergencias().is_empty(),
        "divergências em {}: {:?}",
        b.nome,
        b.divergencias()
    );
    assert_eq!(em_b.divergencias, 0, "{em_b:?}");
    assert_eq!(em_a.divergencias, 0, "{em_a:?}");
    for id in criados {
        a.convergiu_com(b, tipo_da_posicao, id);
    }
    assert_eq!(
        ordem_em(a),
        ordem_em(b),
        "convergiram causalmente e mostram ordens diferentes"
    );
}

/// 1. Duas histórias criadas ao mesmo tempo.
#[test]
fn b22_historias_criadas_ao_mesmo_tempo_convergem_sem_decisao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    sincronizar(&pc, &android);

    let do_pc = pc.historia(&universo, "Saga do PC").id;
    let do_android = android.historia(&universo, "Saga do Android").id;

    converge_sem_divergencia(
        &pc,
        &android,
        "story_position",
        &[&do_pc, &do_android],
        |ap| {
            manuscript_service::list_stories(&ap.banco.database, &universo)
                .expect("histórias")
                .into_iter()
                .map(|h| h.id)
                .collect()
        },
    );
}

/// 2. Dois livros criados ao mesmo tempo na mesma história.
#[test]
fn b22_livros_criados_ao_mesmo_tempo_convergem_sem_decisao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    let historia = pc.historia(&universo, "Saga").id;
    sincronizar(&pc, &android);

    let do_pc = pc.livro(&historia, "Livro do PC").id;
    let do_android = android.livro(&historia, "Livro do Android").id;

    converge_sem_divergencia(
        &pc,
        &android,
        "book_position",
        &[&do_pc, &do_android],
        |ap| {
            manuscript_service::list_books_by_story(&ap.banco.database, &ap.store, &historia)
                .expect("livros")
                .into_iter()
                .map(|l| l.id)
                .collect()
        },
    );
}

/// 3. Dois capítulos criados ao mesmo tempo no mesmo livro.
#[test]
fn b22_capitulos_criados_ao_mesmo_tempo_convergem_sem_decisao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    let historia = pc.historia(&universo, "Saga").id;
    let livro = pc.livro(&historia, "Livro").id;
    sincronizar(&pc, &android);

    let do_pc = pc.capitulo(&livro, "Cap do PC").id;
    let do_android = android.capitulo(&livro, "Cap do Android").id;

    converge_sem_divergencia(
        &pc,
        &android,
        "chapter_position",
        &[&do_pc, &do_android],
        |ap| ap.ids_dos_capitulos(&livro),
    );
}

/// 4. Duas propriedades do planejamento criadas ao mesmo tempo.
#[test]
fn b22_propriedades_criadas_ao_mesmo_tempo_convergem_sem_decisao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    sincronizar(&pc, &android);

    let do_pc = pc.campo(&universo, "Tom", "text", None);
    let do_android = android.campo(&universo, "Clima", "text", None);

    converge_sem_divergencia(
        &pc,
        &android,
        "planning_field_position",
        &[&do_pc, &do_android],
        |ap| {
            crate::application::planning_service::list_field_definitions(
                &ap.banco.database,
                &universo,
                None,
            )
            .expect("propriedades")
            .into_iter()
            .map(|campo| campo.id)
            .collect()
        },
    );
}

/// 5. Dois anexos criados ao mesmo tempo na mesma galeria: os dois existem, mesma ordem, nenhuma
///    decisão. É o caso que a B6 mostrou abrindo divergência e deixando as galerias em ordens opostas.
#[test]
fn b22_anexos_criados_ao_mesmo_tempo_convergem_na_mesma_ordem() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    let entidade = pc.entidade(&universo, "Frodo");
    sincronizar(&pc, &android);

    let do_pc = pc.anexar_em(&universo, "entity", &entidade, "data:image/png;base64,YQ==");
    let do_android =
        android.anexar_em(&universo, "entity", &entidade, "data:image/png;base64,Yg==");

    converge_sem_divergencia(
        &pc,
        &android,
        "attachment_position",
        &[&do_pc, &do_android],
        |ap| {
            canvas_service::list_attachments(
                &ap.banco.database,
                &ap.store,
                &universo,
                "entity",
                &entidade,
            )
            .expect("galeria")
            .into_iter()
            .map(|anexo| anexo.id)
            .collect()
        },
    );
    for anexo in [&do_pc, &do_android] {
        pc.convergiu_com(&android, "attachment", anexo);
    }
}

/// 6. Dois cards criados ao mesmo tempo no mesmo quadro.
#[test]
fn b22_cards_criados_ao_mesmo_tempo_convergem_sem_decisao() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    sincronizar(&pc, &android);

    let do_pc = pc.card(&universo, "Cena do PC", None);
    let do_android = android.card(&universo, "Cena do Android", None);

    converge_sem_divergencia(
        &pc,
        &android,
        "planning_item_position",
        &[&do_pc, &do_android],
        |ap| {
            crate::application::planning_service::list(&ap.banco.database, &ap.store, &universo)
                .expect("quadro")
                .into_iter()
                .map(|card| card.id)
                .collect()
        },
    );
}

/// Livro com três capítulos, sincronizado nos dois aparelhos.
fn livro_com_tres_capitulos(pc: &Aparelho, android: &Aparelho) -> (String, [String; 3]) {
    let universo = pc.universo("Terra");
    let historia = pc.historia(&universo, "Saga").id;
    let livro = pc.livro(&historia, "Livro").id;
    let c1 = pc.capitulo(&livro, "Um").id;
    let c2 = pc.capitulo(&livro, "Dois").id;
    let c3 = pc.capitulo(&livro, "Três").id;
    let (recebido, _) = sincronizar(pc, android);
    assert_eq!(recebido.divergencias, 0, "{recebido:?}");
    (livro, [c1, c2, c3])
}

/// 7. **O mesmo capítulo movido nos dois lados: conflito na unidade certa, decidido pela ação.**
///
/// ```text
/// PC       troca c1 e c2   → uma ação: posições de c1 e c2
/// Android  troca c2 e c3   → uma ação: posições de c2 e c3
/// ```
///
/// A colisão real é só em c2, e é nela que a decisão fica ancorada — uma por aparelho, nenhuma em c1
/// ou c3. Mas a ação é atômica (B2.2): a reordenação do outro lado não entra pela metade. Aplicar só c1
/// do PC sobre c2 e c3 do Android comporia uma terceira ordem que nenhum dos dois escritores escolheu;
/// cada aparelho continua com a ordem que o seu escritor fez, até a decisão.
#[test]
fn b22_o_mesmo_capitulo_movido_nos_dois_lados_diverge_so_nele() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let (livro, [c1, c2, c3]) = livro_com_tres_capitulos(&pc, &android);

    pc.reordenar_capitulos(&livro, &[&c2, &c1, &c3]);
    android.reordenar_capitulos(&livro, &[&c1, &c3, &c2]);
    sincronizar(&pc, &android);

    let esperado = vec![(
        "chapter_position".to_string(),
        c2.clone(),
        "concurrent".to_string(),
    )];
    assert_eq!(pc.divergencias(), esperado, "no PC");
    assert_eq!(android.divergencias(), esperado, "no Android");
    // Nenhuma ação entrou pela metade: cada lado mostra a ordem que o seu escritor fez.
    assert_eq!(
        pc.ids_dos_capitulos(&livro),
        vec![c2.clone(), c1.clone(), c3.clone()]
    );
    assert_eq!(
        android.ids_dos_capitulos(&livro),
        vec![c1.clone(), c3.clone(), c2.clone()]
    );
    for capitulo in [&c1, &c2, &c3] {
        pc.convergiu_com(&android, "chapter", capitulo);
    }
}

/// 8. **Um move o capítulo, o outro o exclui: conflito, sem perda silenciosa.**
#[test]
fn b22_capitulo_movido_num_lado_e_excluido_no_outro_nao_perde_nada_em_silencio() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let (livro, [c1, c2, _c3]) = livro_com_tres_capitulos(&pc, &android);

    pc.reordenar_capitulos(&livro, &[&c2, &c1, &_c3]);
    manuscript_service::delete_chapter(&android.banco.database, &android.eu, &c2)
        .expect("Android exclui");
    let revisao_do_pc = pc
        .payload_corrente("chapter_position", &c2)
        .expect("revisão do movimento no PC");
    sincronizar(&pc, &android);

    // Cada lado decide sobre a AÇÃO que recebeu, pela raiz dela: no PC, a exclusão de c2 (e não o
    // efeito sobre a posição); no Android, o movimento de c2.
    assert_eq!(
        pc.divergencias(),
        vec![(
            "chapter".to_string(),
            c2.clone(),
            "parent_deletion_blocked".to_string()
        )],
        "no PC"
    );
    assert_eq!(
        android.divergencias(),
        vec![(
            "chapter_position".to_string(),
            c2.clone(),
            "concurrent".to_string()
        )],
        "no Android"
    );
    // No PC o capítulo não some sem decisão.
    assert!(
        pc.canonico("chapter", &c2).is_some(),
        "o PC perdeu o capítulo que tinha movido, sem decisão"
    );
    // O movimento do PC não se perdeu no Android: a revisão dele está no log de lá.
    let guardada: bool = android
        .banco
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_events WHERE aggregate_type = 'chapter_position'
                             AND aggregate_id = ?1 AND payload = ?2)",
            [&c2, &revisao_do_pc],
            |row| row.get(0),
        )
        .expect("consulta");
    assert!(guardada, "o movimento do PC não está guardado no Android");
    // A reordenação do PC é uma ação só: com c2 em conflito, c1 também não se move no Android.
    assert_eq!(
        android.ids_dos_capitulos(&livro),
        vec![c1.clone(), _c3.clone()],
        "a ação do PC entrou pela metade no Android"
    );
}

/// 9. **Mover o card e editar o conteúdo dele não conflitam.**
#[test]
fn b22_mover_o_card_num_lado_e_editar_o_conteudo_no_outro_nao_conflitam() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    let card = pc.card(&universo, "Cena do porto", None);
    sincronizar(&pc, &android);

    crate::application::planning_service::save_order(
        &pc.banco.database,
        &pc.eu,
        &universo,
        &[crate::domain::planning::PlanningCardPlacement {
            id: card.clone(),
            status: "ESCREVENDO".into(),
            sort_order: 0,
        }],
    )
    .expect("PC move o card");
    android.salvar_card(
        &universo,
        &card,
        "Cena do porto ao amanhecer",
        None,
        serde_json::json!({}),
    );

    let (em_android, em_pc) = sincronizar(&pc, &android);
    assert!(pc.divergencias().is_empty(), "{:?}", pc.divergencias());
    assert!(
        android.divergencias().is_empty(),
        "{:?}",
        android.divergencias()
    );
    assert_eq!(em_android.divergencias, 0, "{em_android:?}");
    assert_eq!(em_pc.divergencias, 0, "{em_pc:?}");
    pc.convergiu_com(&android, "planning_item", &card);
    pc.convergiu_com(&android, "planning_item_position", &card);
    assert!(pc
        .canonico("planning_item", &card)
        .expect("card")
        .contains("ao amanhecer"));
    assert!(android
        .canonico("planning_item_position", &card)
        .expect("posição")
        .contains("ESCREVENDO"));
}

/// 10. **Excluir um filho não revisa os irmãos.** Nada é compactado, nenhuma lista é reescrita.
#[test]
fn b22_excluir_um_capitulo_nao_gera_revisao_nos_irmaos() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let (_livro, [c1, c2, c3]) = livro_com_tres_capitulos(&pc, &android);

    let antes = pc.eventos().len();
    manuscript_service::delete_chapter(&pc.banco.database, &pc.eu, &c2).expect("excluir");
    let novos = pc.eventos()[antes..].to_vec();
    assert_eq!(
        novos,
        vec![
            (
                "chapter_position".to_string(),
                c2.clone(),
                "delete".to_string()
            ),
            ("chapter".to_string(), c2.clone(), "delete".to_string()),
        ],
        "a exclusão de um capítulo mexeu em algo além dele e da posição dele"
    );

    let (em_android, _) = sincronizar(&pc, &android);
    assert_eq!(em_android.divergencias, 0, "{em_android:?}");
    for irmao in [&c1, &c3] {
        pc.convergiu_com(&android, "chapter_position", irmao);
    }
}

/// **Uma ação que chega pela metade não toca o domínio.**
///
/// O lote é cortado no meio do grupo, como numa queda de Wi-Fi. Até o resto chegar — em outra sessão —
/// nenhum membro entra, nem os que já estão no log.
#[test]
fn b22_acao_que_chega_pela_metade_nao_toca_o_dominio_ate_completar() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let (livro, [c1, c2, c3]) = livro_com_tres_capitulos(&pc, &android);

    manuscript_service::delete_book(&pc.banco.database, &pc.eu, &livro).expect("PC apaga o livro");
    apresentar(&pc, &android);
    let vetor = vetor_local(&android.banco.connection()).expect("vetor");
    let eventos = eventos_para(&pc.banco.connection(), &vetor).expect("eventos");
    let grupo = eventos[0].grupo.clone();
    assert!(
        grupo.count >= 4,
        "a exclusão do livro é uma ação de vários eventos: {grupo:?}"
    );
    assert_eq!(eventos.len() as i64, grupo.count);
    assert!(eventos
        .iter()
        .all(|e| e.grupo.mutation_id == grupo.mutation_id));
    let cursor = |aparelho: &Aparelho| -> i64 {
        aparelho
            .banco
            .connection()
            .query_row(
                "SELECT COALESCE(MAX(last_seq_applied), 0) FROM sync_cursors WHERE origin_device_id = ?1",
                [pc.eu.device_id()],
                |row| row.get(0),
            )
            .expect("cursor")
    };
    let cursor_antes = cursor(&android);

    let (primeira, segunda) = eventos.split_at(eventos.len() / 2);
    let relatorio = receber_eventos(&mut android.banco.connection(), primeira).expect("sessão 1");
    assert_eq!(relatorio.aplicados, 0, "{relatorio:?}");
    assert_eq!(relatorio.pendentes, primeira.len(), "{relatorio:?}");
    assert_eq!(
        cursor(&android),
        cursor_antes,
        "o cursor andou com o grupo pela metade"
    );
    assert!(android.canonico("book", &livro).is_some());
    for capitulo in [&c1, &c2, &c3] {
        assert!(
            android.canonico("chapter", capitulo).is_some(),
            "{capitulo} saiu antes de a ação inteira chegar"
        );
        assert!(android.canonico("chapter_position", capitulo).is_some());
    }

    let relatorio = receber_eventos(&mut android.banco.connection(), segunda).expect("sessão 2");
    assert_eq!(relatorio.aplicados as i64, grupo.count, "{relatorio:?}");
    assert_eq!(relatorio.pendentes, 0, "{relatorio:?}");
    assert_eq!(relatorio.divergencias, 0, "{relatorio:?}");
    assert!(android.canonico("book", &livro).is_none());
    for capitulo in [&c1, &c2, &c3] {
        pc.convergiu_com(&android, "chapter", capitulo);
        pc.convergiu_com(&android, "chapter_position", capitulo);
    }
    android.invariante_de_materializacao();
}

/// **Falha no meio da ação: nenhum membro fica aplicado.**
#[test]
fn b22_falha_no_meio_da_acao_desfaz_todos_os_membros() {
    use crate::infrastructure::sqlite::sync_session::falha_de_grupo;

    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let (livro, [c1, c2, c3]) = livro_com_tres_capitulos(&pc, &android);

    manuscript_service::delete_book(&pc.banco.database, &pc.eu, &livro).expect("PC apaga o livro");
    apresentar(&pc, &android);
    let vetor = vetor_local(&android.banco.connection()).expect("vetor");
    let eventos = eventos_para(&pc.banco.connection(), &vetor).expect("eventos");
    assert!(eventos[0].grupo.count > 3, "{:?}", eventos[0].grupo);

    falha_de_grupo::armar(Some(2));
    let erro = receber_eventos(&mut android.banco.connection(), &eventos)
        .expect_err("a falha injetada depois do membro 2 tinha que derrubar a sessão");
    falha_de_grupo::armar(None);
    assert!(erro.message.contains("membro 2"), "{}", erro.message);

    assert!(android.canonico("book", &livro).is_some());
    for capitulo in [&c1, &c2, &c3] {
        assert!(
            android.canonico("chapter", capitulo).is_some(),
            "{capitulo} ficou apagado depois da falha"
        );
        assert!(
            android.canonico("chapter_position", capitulo).is_some(),
            "a posição de {capitulo} ficou apagada depois da falha"
        );
    }
    android.invariante_de_materializacao();

    // Sem a falha, a mesma ação entra inteira.
    let relatorio = receber_eventos(&mut android.banco.connection(), &eventos).expect("de novo");
    assert_eq!(relatorio.divergencias, 0, "{relatorio:?}");
    assert!(android.canonico("book", &livro).is_none());
    for capitulo in [&c1, &c2, &c3] {
        pc.convergiu_com(&android, "chapter", capitulo);
    }
}

/// **O lote da troca não corta uma ação ao meio.**
///
/// Sem isto, uma ação maior que o lote nunca terminaria de chegar: o receptor não aplica grupo
/// incompleto, o vetor dele não anda, e a sessão seguinte mandaria o mesmo começo para sempre.
#[test]
fn b22_lote_menor_que_a_acao_entrega_a_acao_inteira() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let (livro, [c1, c2, c3]) = livro_com_tres_capitulos(&pc, &android);
    manuscript_service::delete_book(&pc.banco.database, &pc.eu, &livro).expect("PC apaga o livro");
    apresentar(&pc, &android);

    let total_da_acao = {
        let vetor = vetor_local(&android.banco.connection()).expect("vetor");
        eventos_para(&pc.banco.connection(), &vetor).expect("eventos")[0]
            .grupo
            .count
    };
    assert!(total_da_acao > 3, "{total_da_acao}");

    // Lote de 2, bem menor que a ação: a troca repete até o vetor parar de andar, como a sessão faz.
    let mut rodadas = 0;
    loop {
        rodadas += 1;
        assert!(
            rodadas < 20,
            "a troca não terminou: a ação ficou presa no corte do lote"
        );
        let vetor = vetor_local(&android.banco.connection()).expect("vetor");
        let lote = crate::infrastructure::sqlite::sync_exchange::eventos_para_com_limite(
            &pc.banco.connection(),
            &vetor,
            2,
        )
        .expect("lote");
        if lote.is_empty() {
            break;
        }
        let ultimo = &lote[lote.len() - 1].grupo;
        assert!(
            ultimo.e_isolado() || ultimo.index == ultimo.count - 1,
            "o lote terminou no meio de uma ação: {ultimo:?}"
        );
        receber_eventos(&mut android.banco.connection(), &lote).expect("receber");
    }
    assert!(android.canonico("book", &livro).is_none());
    for capitulo in [&c1, &c2, &c3] {
        pc.convergiu_com(&android, "chapter", capitulo);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// B6 item 9 — a identidade do conflito atravessa aparelhos
// ═══════════════════════════════════════════════════════════════════════════

impl Aparelho {
    /// A identidade PORTÁTIL inteira do conflito aberto daquele tipo: chave e os dois
    /// participantes. Os três têm de bater entre aparelhos — só a chave bater esconderia uma
    /// canonicalização divergente que só apareceria quando a resolução precisasse dos
    /// participantes.
    fn identidade_do_conflito(&self, kind: &str) -> (String, String, String) {
        self.banco
            .connection()
            .query_row(
                "SELECT conflict_key, participant_a, participant_b FROM sync_divergences
                  WHERE kind = ?1 AND resolved_at = ''",
                [kind],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("o conflito")
    }
}

/// **Edição contra edição: os dois aparelhos calculam a MESMA chave.**
///
/// Cada um vê uma das revisões como "sua". A ordenação dos participantes apaga essa diferença —
/// sem isso, a resolução detectada num lado não teria como ser reconhecida no outro.
#[test]
fn o_mesmo_conflito_concorrente_tem_a_mesma_chave_nos_dois_aparelhos() {
    let a = Aparelho::novo("a");
    let b = Aparelho::novo("b");
    let universo = a.universo("Terra");
    let historia = a.historia(&universo, "Saga").id;
    let livro = a.livro(&historia, "Livro I").id;
    let capitulo = a.capitulo(&livro, "Um").id;
    sincronizar(&a, &b);

    a.escrever(&capitulo, "a versão de A");
    b.escrever(&capitulo, "a versão de B");
    let (em_b, em_a) = sincronizar(&a, &b);
    assert_eq!(em_b.divergencias, 1, "{em_b:?}");
    assert_eq!(em_a.divergencias, 1, "{em_a:?}");

    let aqui = a.identidade_do_conflito("concurrent");
    let la = b.identidade_do_conflito("concurrent");
    assert_eq!(aqui, la, "o mesmo conflito produziu duas identidades");
    assert!(!aqui.0.is_empty() && !aqui.1.is_empty() && !aqui.2.is_empty());
    assert!(
        aqui.1.as_bytes() < aqui.2.as_bytes(),
        "participantes fora de ordem"
    );
}

/// **Tag homônima: o caso em que os papéis se invertem.**
///
/// No PC, `aggregate_id` é a tag que chegou (T2) e `related_aggregate_id` é a daqui (T1). No
/// Android é o contrário. Uma chave feita de `(aggregate_type, aggregate_id, revisões)` daria duas
/// chaves para a mesma colisão.
#[test]
fn a_tag_homonima_tem_a_mesma_chave_mesmo_com_os_papeis_invertidos() {
    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    sincronizar(&pc, &android);

    let t1 = pc.tag(&universo, "Mar");
    let t2 = android.tag(&universo, "mar");
    sincronizar(&pc, &android);

    // Os papéis estão de fato invertidos: é isso que torna o teste honesto.
    let papeis = |aparelho: &Aparelho| -> (String, String) {
        aparelho
            .banco
            .connection()
            .query_row(
                "SELECT aggregate_id, related_aggregate_id FROM sync_divergences
                  WHERE kind = 'tag_name_conflict' AND resolved_at = ''",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("conflito")
    };
    assert_eq!(papeis(&pc), (t2.clone(), t1.clone()));
    assert_eq!(papeis(&android), (t1, t2));

    let no_pc = pc.identidade_do_conflito("tag_name_conflict");
    let no_android = android.identidade_do_conflito("tag_name_conflict");
    assert_eq!(
        no_pc, no_android,
        "os papéis invertidos produziram identidades diferentes"
    );
    assert!(
        no_pc.1.as_bytes() < no_pc.2.as_bytes(),
        "participantes fora de ordem"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// B6 item 6 — o conteúdo aprovado na colaboração é uma escrita como as outras
// ═══════════════════════════════════════════════════════════════════════════

/// **Aprovar uma proposta vira revisão assinada e atravessa a sincronização.**
///
/// Antes da B6 a aprovação escrevia direto no domínio: o universo mudava no aparelho do anfitrião e
/// nenhum evento nascia. O outro aparelho do próprio escritor nunca via o que ele tinha aprovado —
/// e, pior, um `baseline` posterior levaria o texto novo sem nenhuma revisão que o explicasse.
#[test]
fn contribuicao_aprovada_vira_revisao_e_chega_no_outro_aparelho() {
    use crate::application::collaboration_service;
    use crate::domain::collaboration::{IncomingContribution, NewCollaborationSession};

    let pc = Aparelho::novo("pc");
    let android = Aparelho::novo("android");
    let universo = pc.universo("Terra");
    let (recebido, _) = sincronizar(&pc, &android);
    assert_eq!(recebido.divergencias, 0, "{recebido:?}");

    collaboration_service::save_session(
        &pc.banco.database,
        NewCollaborationSession {
            id: "sess".into(),
            title: "Leitura".into(),
            permission: "edit".into(),
            universe_ids: vec![universo.clone()],
            encryption_key: "chave".into(),
            revoke_token: "token".into(),
            expires_at: "2026-12-31 00:00:00".into(),
        },
    )
    .expect("sessão de colaboração");
    collaboration_service::store_contribution(
        &pc.banco.database,
        &pc.store,
        "sess",
        1,
        IncomingContribution {
            id: "c1".into(),
            contributor: "Convidado".into(),
            kind: "edit".into(),
            universe_id: universo.clone(),
            target_type: "universe".into(),
            target_id: universo.clone(),
            target_label: "Terra".into(),
            field: "name".into(),
            original_value: "Terra".into(),
            proposed_value: "Terra Nova".into(),
            message: String::new(),
            created_at: String::new(),
        },
    )
    .expect("proposta do convidado");

    let eventos_antes = pc.eventos().len();
    collaboration_service::review(&pc.banco.database, &pc.eu, "c1", "approved").expect("aprovar");
    assert_eq!(
        pc.eventos().len(),
        eventos_antes + 1,
        "a aprovação tem de virar UMA revisão do universo"
    );
    pc.invariante_de_materializacao();

    let (em_android, _) = sincronizar(&pc, &android);
    assert_eq!(em_android.divergencias, 0, "{em_android:?}");
    assert!(
        android
            .canonico("universe", &universo)
            .expect("universo")
            .contains("Terra Nova"),
        "o texto aprovado não chegou ao outro aparelho"
    );
    pc.convergiu_com(&android, "universe", &universo);

    // Recusar não emite nada: o domínio não muda, e não há o que sincronizar.
    collaboration_service::store_contribution(
        &pc.banco.database,
        &pc.store,
        "sess",
        2,
        IncomingContribution {
            id: "c2".into(),
            contributor: "Convidado".into(),
            kind: "edit".into(),
            universe_id: universo.clone(),
            target_type: "universe".into(),
            target_id: universo.clone(),
            target_label: "Terra Nova".into(),
            field: "name".into(),
            original_value: "Terra Nova".into(),
            proposed_value: "Terra Velha".into(),
            message: String::new(),
            created_at: String::new(),
        },
    )
    .expect("segunda proposta");
    let antes_da_recusa = pc.eventos().len();
    collaboration_service::review(&pc.banco.database, &pc.eu, "c2", "rejected").expect("recusar");
    assert_eq!(pc.eventos().len(), antes_da_recusa);
    assert!(pc
        .canonico("universe", &universo)
        .expect("universo")
        .contains("Terra Nova"));
}
