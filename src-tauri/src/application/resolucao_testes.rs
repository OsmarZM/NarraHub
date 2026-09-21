//! **Gates da etapa F** — resolução de conflito como fato causal, em aparelhos independentes.
//!
//! Cada aparelho tem banco, identidade e blob store próprios. A "sessão" aqui é o mesmo que a
//! sessão real faz depois do `Hello`: `eventos_para` → `receber_eventos` (com a conferência de blob)
//! → resoluções automáticas. O fio não acrescenta nada à causalidade, e os gates de fio estão em
//! `sync_sessao`.

use std::collections::BTreeMap;

use crate::application::resolucao_divergencia::{
    ler_aberta, ler_por_chave, resolver_automaticas, resolver_conflito, Acao,
};
use crate::application::{knowledge_service, manuscript_service, sync_bootstrap, universe_service};
use crate::database::error::DatabaseCommandResult;
use crate::domain::identity::DeviceIdentity;
use crate::domain::manuscript::ChapterUpdate;
use crate::domain::sync::{compute_revision, AggregateRef, EventEnvelope, Operation};
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::sync_codec;
use crate::infrastructure::sqlite::sync_codec::resolucao::Certificado;
use crate::infrastructure::sqlite::sync_exchange::{eventos_para, vetor_local};
use crate::infrastructure::sqlite::sync_session::{receber_eventos, Relatorio};
use crate::infrastructure::sqlite::test_support::TemporaryDatabase;

struct Aparelho {
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
    fn novo() -> Self {
        let banco = TemporaryDatabase::new();
        let dados = std::env::temp_dir().join(format!("narrahub-f-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dados).expect("dados");
        let eu = sync_bootstrap::prepare(&dados, &banco.database).expect("arranque");
        Self {
            banco,
            eu,
            store: BlobStore::new(dados.clone()),
            dados,
        }
    }

    fn universo(&self) -> String {
        universe_service::create(&self.banco.database, &self.store, &self.eu, "Terra", "", "")
            .expect("universo")
            .id
    }

    fn capitulo_novo(&self, universo: &str) -> String {
        let historia =
            manuscript_service::create_story(&self.banco.database, &self.eu, universo, "Saga")
                .expect("história");
        let livro =
            manuscript_service::create_book(&self.banco.database, &self.eu, &historia.id, "Livro")
                .expect("livro");
        manuscript_service::create_chapter(&self.banco.database, &self.eu, &livro.id, "Um")
            .expect("capítulo")
            .id
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

    fn apagar_capitulo(&self, capitulo: &str) {
        manuscript_service::delete_chapter(&self.banco.database, &self.eu, capitulo)
            .expect("apagar capítulo");
    }

    fn conteudo(&self, capitulo: &str) -> Option<String> {
        self.banco
            .connection()
            .query_row(
                "SELECT content FROM chapters WHERE id = ?1",
                [capitulo],
                |row| row.get(0),
            )
            .ok()
    }

    fn revisao(&self, tipo: &str, id: &str) -> Option<String> {
        sync_codec::revisao_corrente(&self.banco.connection(), &AggregateRef::new(tipo, id))
            .expect("revisão")
    }

    fn tombstone(&self, tipo: &str, id: &str) -> Option<String> {
        self.banco
            .connection()
            .query_row(
                "SELECT deleted_rev FROM sync_tombstones WHERE aggregate_type = ?1 AND aggregate_id = ?2",
                [tipo, id],
                |row| row.get(0),
            )
            .ok()
    }

    /// `(conflict_key, kind, aggregate_type)` das divergências abertas.
    fn abertas(&self) -> Vec<(String, String, String)> {
        let connection = self.banco.connection();
        let mut consulta = connection
            .prepare(
                "SELECT conflict_key, kind, aggregate_type FROM sync_divergences
                  WHERE resolved_at = '' ORDER BY conflict_key",
            )
            .expect("consulta");
        let linhas = consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("linhas");
        linhas.collect::<Result<_, _>>().expect("ler")
    }

    fn uma_aberta(&self) -> String {
        let abertas = self.abertas();
        assert_eq!(abertas.len(), 1, "{abertas:?}");
        abertas[0].0.clone()
    }

    fn decisoes(&self) -> BTreeMap<String, String> {
        let connection = self.banco.connection();
        let mut consulta = connection
            .prepare("SELECT conflict_key, certificate FROM conflict_resolutions")
            .expect("consulta");
        let linhas = consulta
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("linhas");
        linhas.collect::<Result<_, _>>().expect("ler")
    }

    fn pendentes(&self) -> i64 {
        self.banco
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sync_events e
                  WHERE NOT EXISTS (SELECT 1 FROM sync_applied_events a WHERE a.event_id = e.event_id)",
                [],
                |row| row.get(0),
            )
            .expect("pendentes")
    }

    /// A ação que fica com a versão DESTE aparelho (`local = true`) ou com a do outro.
    fn ficar_com(&self, chave: &str, local: bool) -> Acao {
        let divergencia = ler_aberta(&self.banco.connection(), chave).expect("aberta");
        let a_e_daqui = divergencia.local() == &divergencia.a;
        if a_e_daqui == local {
            Acao::FicarComA
        } else {
            Acao::FicarComB
        }
    }

    fn resolver(&self, chave: &str, acao: Acao) -> DatabaseCommandResult<String> {
        resolver_conflito(&self.banco.database, &self.eu, chave, &acao)
            .map(|resultado| resultado.resolution_rev)
    }

    fn eventos_para(&self, outro: &Aparelho) -> Vec<EventEnvelope> {
        let vetor = vetor_local(&outro.banco.connection()).expect("vetor");
        eventos_para(&self.banco.connection(), &vetor).expect("eventos")
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

/// Entrega a `para` o lote dado, como a sessão faz: drenagem com a conferência de blob, e depois as
/// resoluções automáticas.
fn receber(para: &Aparelho, lote: &[EventEnvelope]) -> DatabaseCommandResult<Relatorio> {
    let relatorio = receber_eventos(&mut para.banco.connection(), lote, &para.store)?;
    resolver_automaticas(&para.banco.database, &para.eu)?;
    Ok(relatorio)
}

/// `para` recebe tudo o que `de` tem e ele não.
fn entregar(de: &Aparelho, para: &Aparelho) -> Relatorio {
    apresentar(de, para);
    apresentar(para, de);
    receber(para, &de.eventos_para(para)).expect("receber")
}

/// Duas voltas nos dois sentidos: a segunda leva o que a primeira produziu (inclusive resoluções
/// automáticas).
fn sincronizar(a: &Aparelho, b: &Aparelho) {
    for _ in 0..2 {
        entregar(a, b);
        entregar(b, a);
    }
}

/// Dois aparelhos com o mesmo capítulo, e cada um o editou do seu jeito: um conflito aberto nos dois.
fn conflito_de_edicao() -> (Aparelho, Aparelho, String, String) {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);
    a.escrever(&capitulo, "<p>versão de A</p>");
    b.escrever(&capitulo, "<p>versão de B</p>");
    entregar(&a, &b);
    entregar(&b, &a);
    let chave = a.uma_aberta();
    assert_eq!(
        b.uma_aberta(),
        chave,
        "o mesmo conflito tem a mesma chave nos dois"
    );
    (a, b, capitulo, chave)
}

/// Os dois convergiram: mesmo conteúdo, mesma cabeça causal, nenhum conflito aberto, as mesmas
/// decisões.
fn convergiram(a: &Aparelho, b: &Aparelho, capitulo: &str) {
    assert_eq!(
        a.conteudo(capitulo),
        b.conteudo(capitulo),
        "conteúdo diferente"
    );
    assert_eq!(
        a.revisao("chapter", capitulo),
        b.revisao("chapter", capitulo),
        "cabeça causal diferente"
    );
    assert!(a.abertas().is_empty(), "A: {:?}", a.abertas());
    assert!(b.abertas().is_empty(), "B: {:?}", b.abertas());
    assert_eq!(a.decisoes(), b.decisoes(), "decisões diferentes");
}

/// **F1 — upsert×upsert, fica a versão local de quem decide.** Os dois convergem para ela, numa
/// cabeça só, que é o resultado declarado pela decisão.
#[test]
fn f1_upsert_upsert_fica_a_local() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("resolver");
    assert_eq!(a.conteudo(&capitulo).as_deref(), Some("<p>versão de A</p>"));
    assert!(a.abertas().is_empty());
    sincronizar(&a, &b);
    convergiram(&a, &b, &capitulo);
    assert_eq!(b.conteudo(&capitulo).as_deref(), Some("<p>versão de A</p>"));
    let certificado = Certificado::ler(&a.decisoes()[&chave]).expect("certificado");
    assert_eq!(
        Some(certificado.results[0].result_rev.clone()),
        a.revisao("chapter", &capitulo),
        "a cabeça não é o resultado declarado"
    );
}

/// **F2 — o mesmo conflito, fica a versão do outro aparelho.**
#[test]
fn f2_upsert_upsert_fica_a_remota() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let acao = a.ficar_com(&chave, false);
    a.resolver(&chave, acao).expect("resolver");
    assert_eq!(a.conteudo(&capitulo).as_deref(), Some("<p>versão de B</p>"));
    sincronizar(&a, &b);
    convergiram(&a, &b, &capitulo);
    assert_eq!(b.conteudo(&capitulo).as_deref(), Some("<p>versão de B</p>"));
}

/// Edição num aparelho, exclusão no outro: o conflito aberto nos dois.
fn conflito_de_exclusao() -> (Aparelho, Aparelho, String, String) {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);
    a.escrever(&capitulo, "<p>editado em A</p>");
    b.apagar_capitulo(&capitulo);
    entregar(&a, &b);
    entregar(&b, &a);
    let chave = a.uma_aberta();
    assert_eq!(b.uma_aberta(), chave);
    (a, b, capitulo, chave)
}

/// **F3 — upsert×delete, restaurar.** Decidido pelo lado que excluiu: o conteúdo volta nos dois.
#[test]
fn f3_upsert_delete_restaurar() {
    let (a, b, capitulo, chave) = conflito_de_exclusao();
    assert!(b.conteudo(&capitulo).is_none());
    b.resolver(&chave, Acao::Restaurar).expect("restaurar");
    assert_eq!(
        b.conteudo(&capitulo).as_deref(),
        Some("<p>editado em A</p>")
    );
    sincronizar(&a, &b);
    convergiram(&a, &b, &capitulo);
    assert_eq!(
        a.conteudo(&capitulo).as_deref(),
        Some("<p>editado em A</p>")
    );
}

/// **F4 — upsert×delete, manter a exclusão.** Decidido pelo lado que editou (a ação de lá é um
/// grupo: a exclusão da posição e a do capítulo), com o preflight rodando de novo.
#[test]
fn f4_upsert_delete_manter_exclusao() {
    let (a, b, capitulo, chave) = conflito_de_exclusao();
    a.resolver(&chave, Acao::ManterExclusao)
        .expect("manter a exclusão");
    assert!(a.conteudo(&capitulo).is_none());
    sincronizar(&a, &b);
    assert!(a.conteudo(&capitulo).is_none() && b.conteudo(&capitulo).is_none());
    assert!(a.abertas().is_empty() && b.abertas().is_empty());
    assert!(a.tombstone("chapter", &capitulo).is_some());
    assert_eq!(
        a.tombstone("chapter", &capitulo),
        b.tombstone("chapter", &capitulo),
        "a exclusão final não é uma só"
    );
    assert_eq!(a.decisoes(), b.decisoes());
}

/// **F5 — a decisão chega a um terceiro aparelho** que tinha o mesmo conflito, e fecha lá também.
#[test]
fn f5_resolucao_chega_ao_terceiro() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let c = Aparelho::novo();
    entregar(&a, &c);
    entregar(&b, &c);
    assert_eq!(
        c.uma_aberta(),
        chave,
        "o terceiro tem o mesmo conflito, com a mesma chave"
    );
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("resolver");
    entregar(&a, &c);
    assert!(c.abertas().is_empty(), "{:?}", c.abertas());
    assert_eq!(c.conteudo(&capitulo), a.conteudo(&capitulo));
    assert_eq!(
        c.revisao("chapter", &capitulo),
        a.revisao("chapter", &capitulo)
    );
    sincronizar(&b, &c);
    convergiram(&a, &b, &capitulo);
    convergiram(&b, &c, &capitulo);
}

/// **F6 — a mesma decisão, tomada nos dois aparelhos sem se falarem, é uma decisão só.**
#[test]
fn f6_mesma_resolucao_repetida_e_idempotente() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let em_a = a.ficar_com(&chave, true);
    let em_b = b.ficar_com(&chave, false);
    assert_eq!(
        em_a, em_b,
        "ficar com a versão de A é a mesma letra nos dois"
    );
    let rev_a = a.resolver(&chave, em_a).expect("A decide");
    let rev_b = b.resolver(&chave, em_b).expect("B decide o mesmo");
    assert_eq!(rev_a, rev_b, "a mesma decisão teve revisões diferentes");
    sincronizar(&a, &b);
    convergiram(&a, &b, &capitulo);
    // E reenviar tudo de novo não muda nada.
    let tudo = eventos_para(&a.banco.connection(), &BTreeMap::new()).expect("log");
    let relatorio = receber(&b, &tudo).expect("reenvio");
    assert_eq!(
        (relatorio.aplicados, relatorio.divergencias),
        (0, 0),
        "{relatorio:?}"
    );
}

/// **F7 — a decisão chega antes de um dos participantes:** espera, sem perder a decisão; quando o
/// participante chega, a decisão entra.
#[test]
fn f7_resolucao_antes_do_participante_espera() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let c = Aparelho::novo();
    // C conhece só o lado de A.
    apresentar(&a, &c);
    apresentar(&b, &c);
    let so_de_a: Vec<EventEnvelope> = a
        .eventos_para(&c)
        .into_iter()
        .filter(|e| e.device_id == a.eu.device_id())
        .collect();
    receber(&c, &so_de_a).expect("C recebe o lado de A");
    let acao = a.ficar_com(&chave, false);
    a.resolver(&chave, acao).expect("A fica com a de B");
    let decisao: Vec<EventEnvelope> = a
        .eventos_para(&c)
        .into_iter()
        .filter(|e| e.device_id == a.eu.device_id())
        .collect();
    let relatorio = receber(&c, &decisao).expect("C recebe a decisão");
    assert!(
        relatorio.pendentes > 0,
        "a decisão não esperou: {relatorio:?}"
    );
    assert_eq!(c.conteudo(&capitulo).as_deref(), Some("<p>versão de A</p>"));
    assert!(
        c.decisoes().is_empty(),
        "a decisão materializou sem o participante"
    );

    // Chega o participante que faltava.
    receber(&c, &b.eventos_para(&c)).expect("C recebe o lado de B");
    assert_eq!(c.conteudo(&capitulo).as_deref(), Some("<p>versão de B</p>"));
    assert!(c.abertas().is_empty(), "{:?}", c.abertas());
    assert_eq!(c.pendentes(), 0);
    assert_eq!(
        c.revisao("chapter", &capitulo),
        a.revisao("chapter", &capitulo)
    );
}

/// **F8 — duas decisões diferentes: nenhuma vence em silêncio.** Cada aparelho continua com a sua,
/// e os dois passam a ter o mesmo conflito sobre a decisão.
#[test]
fn f8_duas_decisoes_diferentes_nao_tem_vencedor() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let em_a = a.ficar_com(&chave, true);
    let em_b = b.ficar_com(&chave, true);
    assert_ne!(em_a, em_b);
    a.resolver(&chave, em_a).expect("A fica com a sua");
    b.resolver(&chave, em_b).expect("B fica com a sua");
    sincronizar(&a, &b);
    assert_eq!(a.conteudo(&capitulo).as_deref(), Some("<p>versão de A</p>"));
    assert_eq!(b.conteudo(&capitulo).as_deref(), Some("<p>versão de B</p>"));
    let abertas_a = a.abertas();
    assert_eq!(abertas_a.len(), 1, "{abertas_a:?}");
    assert_eq!(
        abertas_a,
        b.abertas(),
        "o conflito sobre a decisão não é o mesmo nos dois"
    );
}

/// **F20 — as duas decisões viram `concurrent` sobre `conflict_resolution/K`**, e esse conflito se
/// resolve pelo mesmo mecanismo: os dois convergem.
#[test]
fn f20_decisoes_concorrentes_sao_concurrent_sobre_a_decisao() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let em_a = a.ficar_com(&chave, true);
    let em_b = b.ficar_com(&chave, true);
    a.resolver(&chave, em_a).expect("A");
    b.resolver(&chave, em_b).expect("B");
    sincronizar(&a, &b);
    let (sobre_a_decisao, kind, tipo) = a.abertas().remove(0);
    assert_eq!(kind, "concurrent", "não se cria kind novo");
    assert_eq!(tipo, "conflict_resolution");
    let divergencia = ler_aberta(&a.banco.connection(), &sobre_a_decisao).expect("aberta");
    assert_eq!(
        divergencia.agregado.aggregate_id, chave,
        "o agregado é conflict_resolution/K"
    );

    // A escolhe a decisão de B: os dois ficam com a versão de B.
    let acao = a.ficar_com(&sobre_a_decisao, false);
    a.resolver(&sobre_a_decisao, acao)
        .expect("resolver a decisão");
    sincronizar(&a, &b);
    convergiram(&a, &b, &capitulo);
    assert_eq!(a.conteudo(&capitulo).as_deref(), Some("<p>versão de B</p>"));
}

/// **F9 — a decisão de um grupo é atômica no receptor:** uma queda no meio dos membros não deixa
/// nenhum materializado.
#[test]
fn f9_resolucao_de_grupo_e_atomica() {
    let (a, b, capitulo, chave) = conflito_de_exclusao();
    b.resolver(&chave, Acao::Restaurar).expect("restaurar");
    let lote = b.eventos_para(&a);
    let conteudo_antes = a.conteudo(&capitulo);
    crate::infrastructure::sqlite::sync_session::falha_de_grupo::armar(Some(1));
    let erro = receber_eventos(&mut a.banco.connection(), &lote, &a.store);
    crate::infrastructure::sqlite::sync_session::falha_de_grupo::armar(None);
    assert!(erro.is_err(), "a queda no meio do grupo não derrubou nada");
    assert_eq!(a.conteudo(&capitulo), conteudo_antes);
    assert!(
        a.decisoes().is_empty(),
        "a decisão materializou pela metade"
    );
    assert_eq!(a.uma_aberta(), chave);
    // Sem a queda, entra inteira.
    receber(&a, &lote).expect("receber");
    convergiram(&a, &b, &capitulo);
}

/// **F10 — conflito fechado não reaparece**, nem com todo o log reenviado nos dois sentidos.
#[test]
fn f10_conflito_fechado_nao_reaparece() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("resolver");
    sincronizar(&a, &b);
    let tudo_de_a = eventos_para(&a.banco.connection(), &BTreeMap::new()).expect("a");
    let tudo_de_b = eventos_para(&b.banco.connection(), &BTreeMap::new()).expect("b");
    receber(&b, &tudo_de_a).expect("b");
    receber(&a, &tudo_de_b).expect("a");
    convergiram(&a, &b, &capitulo);
    let total: i64 = a
        .banco
        .connection()
        .query_row("SELECT COUNT(*) FROM sync_divergences", [], |row| {
            row.get(0)
        })
        .expect("contar");
    assert_eq!(total, 1, "nasceu divergência nova");
}

/// **F11 — o aparelho semeado depois da decisão nasce resolvido.**
#[test]
fn f11_bootstrap_depois_da_decisao_nasce_resolvido() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let acao = a.ficar_com(&chave, false);
    a.resolver(&chave, acao).expect("resolver");
    sincronizar(&a, &b);
    let bundle = {
        let mut connection = a.banco.connection();
        crate::infrastructure::sqlite::sync_snapshot::capturar(&mut connection)
            .expect("capturar")
            .expect("sem pendência")
    };
    let novo = Aparelho::novo();
    crate::infrastructure::sqlite::sync_snapshot::semear(
        &mut novo.banco.connection(),
        &novo.store,
        &novo.eu,
        &bundle,
    )
    .expect("semear")
    .expect("semeado");
    assert_eq!(
        novo.conteudo(&capitulo).as_deref(),
        Some("<p>versão de B</p>")
    );
    assert_eq!(
        novo.decisoes(),
        a.decisoes(),
        "a decisão não viajou no bundle"
    );
    assert!(novo.abertas().is_empty());
    assert_eq!(
        novo.revisao("chapter", &capitulo),
        a.revisao("chapter", &capitulo)
    );
}

/// **F12 — conflito aberto continua bloqueando o bootstrap, até ser resolvido.**
#[test]
fn f12_conflito_aberto_bloqueia_bootstrap() {
    let (a, _b, _capitulo, chave) = conflito_de_edicao();
    let captura = {
        let mut connection = a.banco.connection();
        crate::infrastructure::sqlite::sync_snapshot::capturar(&mut connection).expect("capturar")
    };
    assert!(
        matches!(
            captura,
            Err(
                crate::infrastructure::sqlite::sync_snapshot::FalhaDeCaptura::DivergenciaAberta { .. }
            )
        ),
        "{captura:?}"
    );
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("resolver");
    let captura = {
        let mut connection = a.banco.connection();
        crate::infrastructure::sqlite::sync_snapshot::capturar(&mut connection).expect("capturar")
    };
    assert!(captura.is_ok(), "{captura:?}");
}

/// Tag homônima: A cria "Mar", B cria "mar" e marca o capítulo com ela.
fn conflito_de_tag() -> (Aparelho, Aparelho, String, String, String, String) {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);
    let tag_a =
        knowledge_service::create_tag(&a.banco.database, &a.eu, &universo, "Mar", "#111111")
            .expect("tag A")
            .id;
    let tag_b =
        knowledge_service::create_tag(&b.banco.database, &b.eu, &universo, "mar", "#222222")
            .expect("tag B")
            .id;
    knowledge_service::set_tag(&b.banco.database, &b.eu, "chapter", &capitulo, &tag_b, true)
        .expect("B marca");
    entregar(&a, &b);
    entregar(&b, &a);
    assert!(
        a.pendentes() > 0,
        "a marcação de B esperava a tag dela existir em A"
    );
    let chave = a.uma_aberta();
    assert_eq!(b.uma_aberta(), chave);
    (a, b, capitulo, tag_a, tag_b, chave)
}

fn tags(aparelho: &Aparelho) -> Vec<(String, String)> {
    let connection = aparelho.banco.connection();
    let mut consulta = connection
        .prepare("SELECT id, name FROM content_tags ORDER BY id")
        .expect("tags");
    let linhas = consulta
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("linhas");
    linhas.collect::<Result<_, _>>().expect("ler")
}

fn marcacoes(aparelho: &Aparelho) -> Vec<(String, String)> {
    let connection = aparelho.banco.connection();
    let mut consulta = connection
        .prepare("SELECT tag_id, owner_id FROM content_tag_assignments ORDER BY tag_id")
        .expect("marcações");
    let linhas = consulta
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("linhas");
    linhas.collect::<Result<_, _>>().expect("ler")
}

/// **F13 — conflito estrutural mantém a semântica própria: mesclar tags.** Uma tag só (a de
/// `participant_a`), com as marcações das duas — inclusive a que esperava a tag existir — e a
/// origem de B destravada.
#[test]
fn f13_tag_homonima_mesclar() {
    let (a, b, capitulo, tag_a, tag_b, chave) = conflito_de_tag();
    let divergencia = ler_aberta(&a.banco.connection(), &chave).expect("aberta");
    let sobrevivente = divergencia.a.aggregate_id.clone();
    assert!(sobrevivente == tag_a || sobrevivente == tag_b);
    a.resolver(&chave, Acao::Mesclar).expect("mesclar");
    sincronizar(&a, &b);
    for (quem, aparelho) in [("A", &a), ("B", &b)] {
        let ids: Vec<String> = tags(aparelho).into_iter().map(|(id, _)| id).collect();
        assert_eq!(
            ids,
            vec![sobrevivente.clone()],
            "{quem}: não sobrou uma tag só"
        );
        assert_eq!(
            marcacoes(aparelho),
            vec![(sobrevivente.clone(), capitulo.clone())],
            "{quem}: a marcação não passou para a sobrevivente"
        );
        assert!(
            aparelho.abertas().is_empty(),
            "{quem}: {:?}",
            aparelho.abertas()
        );
        assert_eq!(
            aparelho.pendentes(),
            0,
            "{quem}: ficou evento esperando para sempre"
        );
    }
    // A origem de B continua andando depois da mescla.
    b.escrever(&capitulo, "<p>depois da mescla</p>");
    entregar(&b, &a);
    assert_eq!(
        a.conteudo(&capitulo).as_deref(),
        Some("<p>depois da mescla</p>")
    );
}

/// **F13 — renomear:** as duas tags continuam existindo, com nomes diferentes, nos dois aparelhos.
#[test]
fn f13_tag_homonima_renomear() {
    let (a, b, capitulo, tag_a, tag_b, chave) = conflito_de_tag();
    a.resolver(
        &chave,
        Acao::Renomear {
            tag_id: tag_a.clone(),
            nome: "Mar aberto".into(),
        },
    )
    .expect("renomear");
    sincronizar(&a, &b);
    let esperado = {
        let mut t = vec![
            (tag_a.clone(), "Mar aberto".to_string()),
            (tag_b.clone(), "mar".to_string()),
        ];
        t.sort();
        t
    };
    assert_eq!(tags(&a), esperado);
    assert_eq!(tags(&b), esperado);
    assert_eq!(marcacoes(&a), vec![(tag_b.clone(), capitulo.clone())]);
    assert!(a.abertas().is_empty() && b.abertas().is_empty());
    assert_eq!(a.pendentes(), 0);
}

/// **F14 — queda no meio da resolução: tudo ou nada.** Nada muda com a queda; repetir conclui.
#[test]
fn f14_queda_durante_a_resolucao_nao_deixa_nada() {
    let (a, _b, capitulo, chave) = conflito_de_edicao();
    let eventos_antes = eventos_para(&a.banco.connection(), &BTreeMap::new())
        .expect("log")
        .len();
    let acao = a.ficar_com(&chave, false);
    for ponto in [
        crate::application::mutacao::falha::Ponto::DuranteOsEventos,
        crate::application::mutacao::falha::Ponto::AntesDoCommit,
    ] {
        crate::application::mutacao::falha::armar(Some(ponto));
        let erro = a.resolver(&chave, acao.clone());
        crate::application::mutacao::falha::armar(None);
        assert!(erro.is_err(), "{ponto:?}: a queda não derrubou a resolução");
        assert_eq!(a.conteudo(&capitulo).as_deref(), Some("<p>versão de A</p>"));
        assert_eq!(
            a.uma_aberta(),
            chave,
            "{ponto:?}: o conflito fechou sem decisão"
        );
        assert!(
            a.decisoes().is_empty(),
            "{ponto:?}: a decisão ficou pela metade"
        );
        assert_eq!(
            eventos_para(&a.banco.connection(), &BTreeMap::new())
                .expect("log")
                .len(),
            eventos_antes
        );
    }
    a.resolver(&chave, acao).expect("repetir conclui");
    assert_eq!(a.conteudo(&capitulo).as_deref(), Some("<p>versão de B</p>"));
}

/// A decisão de `a`, como ela viaja: o grupo `resolution` inteiro.
fn grupo_da_decisao(a: &Aparelho, para: &Aparelho) -> Vec<EventEnvelope> {
    let lote = a.eventos_para(para);
    let grupo: Vec<EventEnvelope> = lote
        .into_iter()
        .filter(|e| e.grupo.kind == sync_codec::resolucao::KIND_DO_GRUPO)
        .collect();
    assert!(!grupo.is_empty(), "a decisão não está no lote");
    grupo
}

/// Reassina o membro 0 depois de mudar o certificado — a assinatura continua válida; o que precisa
/// recusar é o CONTEÚDO.
fn reassinar_certificado(
    a: &Aparelho,
    grupo: &mut [EventEnvelope],
    mudar: impl Fn(&mut Certificado),
) {
    let mut certificado = Certificado::ler(&grupo[0].payload).expect("certificado");
    mudar(&mut certificado);
    // Na forma canônica: a adulteração é de CONTEÚDO, não de formatação.
    let payload = certificado.canonico().expect("json");
    grupo[0].payload = payload;
    grupo[0].new_rev = compute_revision(
        "",
        &AggregateRef::new(&grupo[0].aggregate_type, &grupo[0].aggregate_id),
        Operation::Upsert,
        &grupo[0].payload,
    );
    grupo[0].signature = a.eu.sign(&grupo[0]);
}

fn recusa_sem_materializar(b: &Aparelho, grupo: &[EventEnvelope], capitulo: &str, caso: &str) {
    let conteudo = b.conteudo(capitulo);
    let abertas = b.abertas();
    let erro = receber_eventos(&mut b.banco.connection(), grupo, &b.store)
        .expect_err(&format!("{caso}: um certificado adulterado foi aceito"));
    assert!(
        erro.message.contains("não se sustenta"),
        "{caso}: {}",
        erro.message
    );
    assert_eq!(b.conteudo(capitulo), conteudo, "{caso}: materializou");
    assert_eq!(b.abertas(), abertas, "{caso}: o conflito mudou");
    assert!(b.decisoes().is_empty(), "{caso}: a decisão entrou");
}

/// **F16 — certificado adulterado (com assinatura válida) é recusado, sem materializar nada.**
#[test]
fn f16_certificado_adulterado_e_recusado() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("resolver");
    let original = grupo_da_decisao(&a, &b);

    type Adulteracao = Box<dyn Fn(&mut Certificado)>;
    let casos: Vec<(&str, Adulteracao)> = vec![
        (
            "escolha trocada",
            Box::new(|c| {
                c.choice = if c.choice == "a" {
                    "b".into()
                } else {
                    "a".into()
                }
            }),
        ),
        (
            "participantes trocados",
            Box::new(|c| std::mem::swap(&mut c.participant_a, &mut c.participant_b)),
        ),
        (
            "chave de outro conflito",
            Box::new(|c| c.participant_a.revision = "rev-inventada".into()),
        ),
        (
            "resultado forjado",
            Box::new(|c| c.results[0].result_rev = "rev-forjada".into()),
        ),
        (
            "par forjado",
            Box::new(|c| c.results[0].other_rev = "rev-inventada".into()),
        ),
        (
            "kind forjado",
            Box::new(|c| c.kind = "tag_name_conflict".into()),
        ),
    ];
    for (caso, mudar) in casos {
        let mut grupo = original.clone();
        reassinar_certificado(&a, &mut grupo, mudar);
        recusa_sem_materializar(&b, &grupo, &capitulo, caso);
    }
    // Chave coerente com o agregado e com o grupo, mas que NÃO é o hash do tipo e dos
    // participantes: só a recomputação da chave recusa.
    let mut inventada = original.clone();
    let chave_falsa = "f".repeat(64);
    for membro in inventada.iter_mut() {
        membro.grupo.root_id = chave_falsa.clone();
    }
    inventada[0].aggregate_id = chave_falsa.clone();
    reassinar_certificado(&a, &mut inventada, |c| c.conflict_key = chave_falsa.clone());
    for membro in inventada.iter_mut().skip(1) {
        membro.signature = a.eu.sign(membro);
    }
    recusa_sem_materializar(&b, &inventada, &capitulo, "chave que não é recomputável");
    // O original entra.
    receber(&b, &original).expect("o certificado íntegro entra");
    assert!(b.abertas().is_empty());
}

/// **F17 — `results[]` de uma decisão com vários efeitos corresponde EXATAMENTE ao grupo.**
#[test]
fn f17_resultados_multiagregado_correspondem_ao_grupo() {
    let (a, b, capitulo, chave) = conflito_de_exclusao();
    a.resolver(&chave, Acao::ManterExclusao)
        .expect("manter a exclusão");
    let original = grupo_da_decisao(&a, &b);
    assert!(
        original.len() >= 3,
        "o cenário exige vários efeitos: {}",
        original.len()
    );

    let mut a_mais = original.clone();
    reassinar_certificado(&a, &mut a_mais, |c| {
        let mut extra = c.results[0].clone();
        extra.aggregate_id = "agregado-que-nao-esta-no-grupo".into();
        c.results.push(extra);
    });
    recusa_sem_materializar(&b, &a_mais, &capitulo, "efeito a mais no certificado");

    let mut a_menos = original.clone();
    reassinar_certificado(&a, &mut a_menos, |c| {
        c.results.pop();
    });
    recusa_sem_materializar(&b, &a_menos, &capitulo, "efeito a menos no certificado");

    let mut trocado = original.clone();
    reassinar_certificado(&a, &mut trocado, |c| {
        let n = c.results.len();
        let (primeiro, ultimo) = (
            c.results[0].aggregate_type.clone(),
            c.results[n - 1].aggregate_type.clone(),
        );
        assert_ne!(
            primeiro, ultimo,
            "o cenário exige efeitos sobre tipos diferentes"
        );
        c.results[0].aggregate_type = ultimo;
        c.results[n - 1].aggregate_type = primeiro;
    });
    recusa_sem_materializar(&b, &trocado, &capitulo, "efeitos trocados de agregado");

    receber(&b, &original).expect("o certificado íntegro entra");
    assert!(b.abertas().is_empty());
}

/// **F18 — exclusão contra exclusão converge sozinha para UMA revisão final**, pela resolução
/// causal automática.
#[test]
fn f18_delete_delete_converge_automaticamente() {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);
    // A edita antes de excluir: as duas exclusões partem de bases diferentes. (Da MESMA base, as
    // duas teriam a mesma revisão — e nem chegariam a ser conflito.)
    a.escrever(&capitulo, "<p>última edição de A</p>");
    a.apagar_capitulo(&capitulo);
    b.apagar_capitulo(&capitulo);
    let (rev_a, rev_b) = (
        a.tombstone("chapter", &capitulo),
        b.tombstone("chapter", &capitulo),
    );
    assert_ne!(rev_a, rev_b, "o cenário exige duas exclusões diferentes");
    sincronizar(&a, &b);
    assert!(
        a.abertas().is_empty() && b.abertas().is_empty(),
        "{:?}",
        a.abertas()
    );
    let final_a = a.tombstone("chapter", &capitulo);
    assert_eq!(
        final_a,
        b.tombstone("chapter", &capitulo),
        "não há uma exclusão final só"
    );
    assert_ne!(
        final_a, rev_a,
        "a exclusão final não é nova: nenhuma das duas pode ganhar"
    );
    assert_ne!(final_a, rev_b);
    assert_eq!(a.decisoes().len(), 1);
    assert_eq!(a.decisoes(), b.decisoes());
    let certificado = Certificado::ler(a.decisoes().values().next().expect("decisão")).expect("c");
    assert_eq!(certificado.choice, "auto");
}

/// **F19 — duas edições que chegaram ao mesmo conteúdo convergem sozinhas para UMA revisão final.**
#[test]
fn f19_estados_identicos_convergem_automaticamente() {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);
    a.escrever(&capitulo, "<p>o mesmo texto</p>");
    b.escrever(&capitulo, "<p>rascunho</p>");
    b.escrever(&capitulo, "<p>o mesmo texto</p>");
    let (rev_a, rev_b) = (
        a.revisao("chapter", &capitulo),
        b.revisao("chapter", &capitulo),
    );
    assert_ne!(
        rev_a, rev_b,
        "o cenário exige revisões diferentes do mesmo conteúdo"
    );
    sincronizar(&a, &b);
    convergiram(&a, &b, &capitulo);
    let final_a = a.revisao("chapter", &capitulo);
    assert_ne!(final_a, rev_a);
    assert_ne!(final_a, rev_b);
    assert_eq!(a.decisoes().len(), 1);
}

/// **F21 — decisão antiga não sobrescreve edição nova.** A1×B1; B edita para B2; chega a decisão de
/// A1×B1. B2 não é participante: fica, e nasce um conflito novo entre B2 e a decisão.
#[test]
fn f21_decisao_antiga_nao_sobrescreve_edicao_nova() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    b.escrever(&capitulo, "<p>B2, escrito depois</p>");
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A decide A1×B1");
    entregar(&a, &b);
    assert_eq!(
        b.conteudo(&capitulo).as_deref(),
        Some("<p>B2, escrito depois</p>"),
        "a decisão antiga sobrescreveu B2"
    );
    let abertas = b.abertas();
    assert_eq!(abertas.len(), 1, "{abertas:?}");
    assert_ne!(
        abertas[0].0, chave,
        "a pergunta velha continuou aberta no lugar da nova"
    );
    let nova = ler_por_chave(&b.banco.connection(), &abertas[0].0, true)
        .expect("ler")
        .expect("nova");
    assert_eq!(
        Some(nova.remote_rev.clone()),
        a.revisao("chapter", &capitulo),
        "o outro lado do conflito novo é a decisão de A"
    );
    // A chega à mesma pergunta, com a mesma chave.
    entregar(&b, &a);
    assert_eq!(
        a.abertas()
            .iter()
            .map(|(k, _, _)| k.clone())
            .collect::<Vec<_>>(),
        vec![abertas[0].0.clone()]
    );
}

/// **O `Hello` recusa o formato canônico 1 contra 2**: a etapa F subiu o formato.
#[test]
fn formato_canonico_da_etapa_f_e_2() {
    assert_eq!(sync_codec::FORMATO_CANONICO_ATUAL, 2);
    assert_eq!(crate::application::genese::VERSAO_DA_ADOCAO, 2);
    assert_eq!(crate::application::sync_sessao::PROTOCOLO_DO_SYNC, 1);
}
