//! **Gates da etapa H** — o que casos extremos NÃO podem conseguir.
//!
//! As etapas A–G provaram que a sincronização funciona. Esta prova que o estranho não passa:
//!
//! ```text
//! ressuscitar dado apagado                          H1, H2, H4
//! sobrescrever edição concorrente                   H2, H8, H9, H11
//! usar `resolution` para escrever agregado alheio    H8, H12, H13, H14
//! ```
//!
//! A prioridade é uma só: **perda silenciosa é proibida**. Conflito a mais, espera e fail closed
//! são respostas aceitáveis; convergência que engole edição, não.

use std::collections::BTreeMap;

use super::resolucao_testes::{
    conflito_de_edicao, conflito_de_exclusao, convergiram, entregar, grupo_da_decisao, receber,
    sincronizar, Aparelho,
};
use crate::application::resolucao_divergencia::Acao;
use crate::domain::sync::{compute_revision, AggregateRef, EventEnvelope, Operation};
use crate::infrastructure::sqlite::sync_codec;
use crate::infrastructure::sqlite::sync_codec::resolucao::{Certificado, EfeitoCanonico};
use rusqlite::OptionalExtension;

/// Simula a coleta de um tombstone por um GC futuro: some **só** a linha do tombstone, e a
/// história do agregado fica onde está. É exatamente o estado que a etapa H tem de sobreviver.
fn coletar_tombstone(aparelho: &Aparelho, tipo: &str, id: &str) {
    let removidas = aparelho
        .banco
        .connection()
        .execute(
            "DELETE FROM sync_tombstones WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            [tipo, id],
        )
        .expect("coletar");
    assert_eq!(removidas, 1, "não havia tombstone para coletar");
}

fn historia(aparelho: &Aparelho, tipo: &str, id: &str) -> usize {
    let connection = aparelho.banco.connection();
    let mut consulta = connection
        .prepare(
            "SELECT rev FROM sync_revision_history WHERE aggregate_type = ?1 AND aggregate_id = ?2",
        )
        .expect("consulta");
    let linhas = consulta
        .query_map([tipo, id], |row| row.get::<_, String>(0))
        .expect("linhas");
    linhas.count()
}

/// Três aparelhos: A e B em conflito sobre um capítulo, e C — que conhece o capítulo e **só o
/// lado de A**. É em C que o caso extremo da H-R1 cabe: sem divergência aberta, ele pode apagar.
fn tres_aparelhos() -> (Aparelho, Aparelho, Aparelho, String, String) {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let c = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    a.escrever(&capitulo, "<p>original</p>");
    sincronizar(&a, &b);
    sincronizar(&a, &c);
    a.escrever(&capitulo, "<p>versão de A</p>");
    b.escrever(&capitulo, "<p>versão de B</p>");
    entregar(&a, &b);
    entregar(&b, &a);
    let chave = a.uma_aberta();
    assert_eq!(b.uma_aberta(), chave);
    // Só o que nasceu em A: C fica sabendo da edição de A, e não da de B. Sem os dois lados, não
    // há divergência em C — e é por isso que ele consegue apagar o capítulo.
    let so_de_a: Vec<EventEnvelope> = a
        .eventos_para(&c)
        .into_iter()
        .filter(|evento| evento.device_id == a.eu.device_id())
        .collect();
    receber(&c, &so_de_a).expect("C recebe o lado de A");
    assert!(c.abertas().is_empty(), "C não podia ter divergência aqui");
    (a, b, c, capitulo, chave)
}

/// **H1 — exclusão + tombstone coletado + resolução antiga = zero ressurreição.**
#[test]
fn h1_resolucao_antiga_depois_da_coleta_nao_ressuscita() {
    let (a, _b, c, capitulo, chave) = tres_aparelhos();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");

    // Em C o capítulo é apagado, e um GC futuro coleta o tombstone: sobra história sem cabeça.
    c.apagar_capitulo(&capitulo);
    assert!(c.tombstone("chapter", &capitulo).is_some());
    coletar_tombstone(&c, "chapter", &capitulo);
    assert!(
        historia(&c, "chapter", &capitulo) > 0,
        "o cenário exige história sobrevivendo à coleta"
    );

    entregar(&a, &c);
    assert_eq!(
        c.conteudo(&capitulo),
        None,
        "a resolução antiga ressuscitou um capítulo apagado"
    );
    assert!(
        c.pendentes() > 0 || !c.abertas().is_empty(),
        "não materializou, não esperou e não perguntou: a decisão sumiu no ar"
    );
}

/// **H2 — a cabeça já foi para uma revisão nova: a decisão antiga não passa por cima dela.**
#[test]
fn h2_resolucao_antiga_nao_sobrescreve_cabeca_nova() {
    let (a, _b, c, capitulo, chave) = tres_aparelhos();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");

    c.escrever(&capitulo, "<p>trabalho novo de C</p>");
    let x = c.conteudo(&capitulo);

    entregar(&a, &c);
    assert_eq!(
        c.conteudo(&capitulo),
        x,
        "a decisão antiga passou por cima da revisão nova de C"
    );
    assert!(
        !c.abertas().is_empty() || c.pendentes() > 0,
        "a decisão que não coube precisa virar pergunta ou espera"
    );
}

/// **H3 — o caso legítimo da R2 continua funcionando**: o agregado nunca materializou aqui e o
/// evento-base exato está pendente; a resolução o substitui no mesmo grupo.
#[test]
fn h3_r2_continua_valendo_para_pendente_nunca_materializado() {
    use crate::application::knowledge_service;
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);

    // Tags homônimas nos dois aparelhos, e B marca o capítulo com a dela. Em A, a marcação de B
    // fica PENDENTE: a tag dela não existe lá, e o `UNIQUE` do nome não deixa criar.
    knowledge_service::create_tag(&a.banco.database, &a.eu, &universo, "Mar", "#111111")
        .expect("tag de A");
    let tag_b =
        knowledge_service::create_tag(&b.banco.database, &b.eu, &universo, "mar", "#222222")
            .expect("tag de B")
            .id;
    knowledge_service::set_tag(&b.banco.database, &b.eu, "chapter", &capitulo, &tag_b, true)
        .expect("B marca");
    entregar(&a, &b);
    entregar(&b, &a);
    assert!(
        a.pendentes() > 0,
        "o cenário exige a marcação de B pendente em A"
    );

    // A mescla: a marcação pendente — agregado que NUNCA materializou aqui — é superada pela
    // decisão, no mesmo grupo atômico. É a R2 com prova positiva.
    let chave = a.uma_aberta();
    a.resolver(&chave, Acao::Mesclar).expect("mesclar");
    assert!(a.abertas().is_empty(), "o conflito de nome ficou aberto");
    sincronizar(&a, &b);

    for aparelho in [&a, &b] {
        assert!(
            aparelho.abertas().is_empty(),
            "sobrou conflito depois da mescla"
        );
        assert_eq!(
            aparelho.pendentes(),
            0,
            "sobrou evento pendente: a decisão não superou o que esperava a tag"
        );
        let (tags, marcadas): (i64, i64) = aparelho
            .banco
            .connection()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM content_tags),
                        (SELECT COUNT(*) FROM content_tag_assignments WHERE owner_id = ?1)",
                [&capitulo],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("contar");
        assert_eq!(
            (tags, marcadas),
            (1, 1),
            "a mescla não convergiu para uma tag e uma marcação"
        );
    }
}

/// **H4 — sem cabeça e sem tombstone, mas com história antiga: a R2 não entra.**
///
/// Aqui o efeito nem sequer tem `other_rev`: se a R2 aceitasse "ausência" como prova, ele entraria
/// sozinho e recriaria o capítulo.
#[test]
fn h4_r2_nao_entra_com_historia_antiga() {
    let (a, _b, c, capitulo, chave) = tres_aparelhos();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");
    let mut decisao = grupo_da_decisao(&a, &c);

    // O efeito perde o par: sobra só a base. É o formato de um efeito que só a R2 aplicaria.
    let sem_par = reassinar_sem_par(&a, &mut decisao);
    c.apagar_capitulo(&capitulo);
    coletar_tombstone(&c, "chapter", &capitulo);

    let _ = receber(&c, &sem_par);
    assert_eq!(
        c.conteudo(&capitulo),
        None,
        "a R2 aceitou ausência como prova de 'nunca existiu'"
    );
}

/// Reescreve a decisão sem `otherRev` nos efeitos e reassina o grupo inteiro.
fn reassinar_sem_par(a: &Aparelho, grupo: &mut [EventEnvelope]) -> Vec<EventEnvelope> {
    let mut certificado = Certificado::ler(&grupo[0].payload).expect("certificado");
    for resultado in certificado.results.iter_mut() {
        resultado.other_rev = String::new();
    }
    grupo[0].payload = certificado.canonico().expect("json");
    grupo[0].new_rev = compute_revision(
        "",
        &AggregateRef::new(&grupo[0].aggregate_type, &grupo[0].aggregate_id),
        Operation::Upsert,
        &grupo[0].payload,
    );
    for membro in grupo.iter_mut() {
        membro.signature = a.eu.sign(membro);
    }
    grupo.to_vec()
}

/// **H5 — a restauração legítima continua removendo o tombstone.**
#[test]
fn h5_restauracao_legitima_remove_o_tombstone() {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    a.escrever(&capitulo, "<p>texto de A</p>");
    sincronizar(&a, &b);
    b.apagar_capitulo(&capitulo);
    a.escrever(&capitulo, "<p>editado por A</p>");
    entregar(&a, &b);

    let chave = b.uma_aberta();
    b.resolver(&chave, Acao::Restaurar).expect("restaurar");
    assert_eq!(
        b.conteudo(&capitulo).as_deref(),
        Some("<p>editado por A</p>")
    );
    assert_eq!(
        b.tombstone("chapter", &capitulo),
        None,
        "a restauração decidida precisa poder tirar o tombstone"
    );
}

/// **H6 — nenhum caminho de produção faz coleta física de tombstone.**
///
/// `sync_gc::tombstones_coletaveis` continua sendo cálculo de coletabilidade, sem chamador de
/// produção, e toda remoção passa pelo helper com motivo declarado. Um GC futuro terá de
/// acrescentar a própria variante, de propósito, e passar por H1–H5.
#[test]
fn h6_nenhum_caminho_de_producao_coleta_tombstone() {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pilha = vec![raiz.clone()];
    let mut remocoes = Vec::new();
    let mut chamadas_de_gc = Vec::new();
    while let Some(pasta) = pilha.pop() {
        for entrada in std::fs::read_dir(&pasta).expect("ler src") {
            let caminho = entrada.expect("entrada").path();
            if caminho.is_dir() {
                pilha.push(caminho);
                continue;
            }
            if caminho.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let nome = caminho.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if nome.ends_with("_testes") || nome.ends_with("_tests") {
                continue;
            }
            let fonte = std::fs::read_to_string(&caminho).expect("ler");
            let codigo = fonte
                .split_once("#[cfg(test)]")
                .map(|(antes, _)| antes.to_string())
                .unwrap_or(fonte);
            let relativo = caminho
                .strip_prefix(&raiz)
                .expect("relativo")
                .to_string_lossy()
                .replace('\\', "/");
            if codigo.contains("DELETE FROM sync_tombstones")
                && relativo != "infrastructure/sqlite/sync_repository.rs"
            {
                remocoes.push(relativo.clone());
            }
            if codigo.contains("tombstones_coletaveis(")
                && relativo != "infrastructure/sqlite/sync_gc.rs"
            {
                chamadas_de_gc.push(relativo);
            }
        }
    }
    assert!(
        remocoes.is_empty(),
        "tombstone é removido fora do helper com motivo (H-R1): {remocoes:?}"
    );
    assert!(
        chamadas_de_gc.is_empty(),
        "alguém ligou a coleta de tombstones em produção: {chamadas_de_gc:?}"
    );

    let helper = include_str!("../infrastructure/sqlite/sync_repository.rs");
    let inicio = helper
        .find("pub enum RemocaoDeTombstone {")
        .expect("o enum de motivos sumiu");
    let corpo = &helper[inicio
        ..helper[inicio..]
            .find("\n}")
            .map(|f| inicio + f)
            .expect("fim")];
    let variantes: Vec<&str> = corpo
        .lines()
        .filter_map(|linha| linha.trim().strip_suffix(','))
        .filter(|linha| !linha.contains(' '))
        .collect();
    assert_eq!(
        variantes,
        vec![
            "SucessorCausalAplicado",
            "EfeitoDeResolucao",
            "RestauracaoDecidida"
        ],
        "o conjunto de motivos legítimos para remover um tombstone mudou sem passar por H1–H5"
    );
}

/// O certificado, com um efeito auxiliar a mais — assinado, coerente consigo mesmo, e sem nenhum
/// direito de unir duas cabeças. É o ataque que a H-R2 tem de neutralizar.
fn forjar_efeito_auxiliar(
    a: &Aparelho,
    grupo: &[EventEnvelope],
    alvo: &AggregateRef,
    base_rev: &str,
    outra: &str,
    payload: &str,
    universo: &str,
) -> Vec<EventEnvelope> {
    let mut grupo = grupo.to_vec();
    let ultimo = grupo.last().expect("grupo").clone();
    let mut extra = EventEnvelope {
        event_id: uuid::Uuid::new_v4().to_string(),
        device_id: ultimo.device_id.clone(),
        seq: ultimo.seq + 1,
        universe_id: universo.to_string(),
        aggregate_type: alvo.aggregate_type.clone(),
        aggregate_id: alvo.aggregate_id.clone(),
        operation: Operation::Upsert,
        payload: payload.to_string(),
        base_rev: base_rev.to_string(),
        new_rev: String::new(),
        signature: String::new(),
        grupo: ultimo.grupo.clone(),
    };
    extra.new_rev = compute_revision(base_rev, alvo, Operation::Upsert, payload);
    extra.grupo.index = ultimo.grupo.index + 1;
    grupo.push(extra.clone());

    let total = grupo.len() as i64;
    let mut certificado = Certificado::ler(&grupo[0].payload).expect("certificado");
    certificado.results.push(EfeitoCanonico {
        aggregate_type: alvo.aggregate_type.clone(),
        aggregate_id: alvo.aggregate_id.clone(),
        operation: "upsert".into(),
        base_rev: base_rev.to_string(),
        other_rev: outra.to_string(),
        result_rev: extra.new_rev.clone(),
    });
    grupo[0].payload = certificado.canonico().expect("json");
    grupo[0].new_rev = compute_revision(
        "",
        &AggregateRef::new(&grupo[0].aggregate_type, &grupo[0].aggregate_id),
        Operation::Upsert,
        &grupo[0].payload,
    );
    for membro in grupo.iter_mut() {
        membro.grupo.count = total;
        membro.signature = a.eu.sign(membro);
    }
    grupo
}

/// Um capítulo de A que B conhece, editado dos dois lados depois: a cabeça de B é concorrente.
fn alvo_alheio(
    a: &Aparelho,
    b: &Aparelho,
    universo: &str,
) -> (AggregateRef, String, String, String) {
    let capitulo = a.capitulo_novo(universo);
    a.escrever(&capitulo, "<p>original</p>");
    sincronizar(a, b);
    let agregado = AggregateRef::new("chapter", &capitulo);
    let base = a.revisao("chapter", &capitulo).expect("revisão comum");
    a.escrever(
        &capitulo,
        "<p>versão de A, que a decisão tentaria impor</p>",
    );
    let rev_de_a = a.revisao("chapter", &capitulo).expect("revisão de A");
    b.escrever(&capitulo, "<p>versão de B, que não pode sumir</p>");
    (agregado, base, rev_de_a, capitulo)
}

/// **H7 — o efeito sobre um participante continua ganhando a junção de dois pais.**
#[test]
fn h7_efeito_participante_continua_com_dois_pais() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");
    entregar(&a, &b);
    assert_eq!(b.conteudo(&capitulo), a.conteudo(&capitulo));
    assert!(b.abertas().is_empty(), "a decisão não fechou o conflito");
}

/// **H8 — efeito auxiliar sobre agregado alheio, com revisões válidas, não sobrescreve.**
#[test]
fn h8_efeito_auxiliar_alheio_nao_sobrescreve() {
    let (a, b, _capitulo, chave) = conflito_de_edicao();
    let universo = a.universo();
    let (alvo, base, _rev_de_a, id) = alvo_alheio(&a, &b, &universo);
    let conteudo_de_b = b.conteudo(&id);
    let cabeca_de_b = b.revisao("chapter", &id).expect("cabeça de B");

    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");
    let decisao = grupo_da_decisao(&a, &b);
    let payload = sync_codec::ler_canonico(&a.banco.connection(), &alvo)
        .expect("ler")
        .expect("estado de A")
        .payload;
    let forjado = forjar_efeito_auxiliar(
        &a,
        &decisao,
        &alvo,
        &base,
        &cabeca_de_b,
        &payload,
        &universo,
    );

    let _ = receber(&b, &forjado);
    assert_eq!(
        b.conteudo(&id),
        conteudo_de_b,
        "um efeito auxiliar alheio sobrescreveu a edição de B"
    );
}

/// **H9 — e o conflito que ele criou fica explícito.**
#[test]
fn h9_efeito_auxiliar_alheio_vira_divergencia() {
    let (a, b, _capitulo, chave) = conflito_de_edicao();
    let universo = a.universo();
    let (alvo, base, _rev_de_a, id) = alvo_alheio(&a, &b, &universo);
    let conteudo_de_b = b.conteudo(&id);
    let cabeca_de_b = b.revisao("chapter", &id).expect("cabeça de B");
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");
    let decisao = grupo_da_decisao(&a, &b);
    let payload = sync_codec::ler_canonico(&a.banco.connection(), &alvo)
        .expect("ler")
        .expect("estado de A")
        .payload;
    let forjado = forjar_efeito_auxiliar(
        &a,
        &decisao,
        &alvo,
        &base,
        &cabeca_de_b,
        &payload,
        &universo,
    );

    let _ = receber(&b, &forjado);
    assert_eq!(b.conteudo(&id), conteudo_de_b, "sobrescreveu");
    let abertas: BTreeMap<String, String> = b
        .abertas()
        .into_iter()
        .map(|(chave, kind, _)| (chave, kind))
        .collect();
    assert!(
        !abertas.is_empty(),
        "nem sobrescreveu nem perguntou: o efeito alheio sumiu em silêncio"
    );
}

/// **H10 — o auxiliar legitimamente ligado continua ganhando a junção.**
///
/// Restaurar um capítulo excluído traz junto a posição dele, que saiu na MESMA ação da exclusão.
/// A posição não é participante do conflito: se a junção dela fosse negada, o efeito divergiria no
/// outro aparelho e a decisão inteira viraria pergunta. Convergir sem conflito é a prova.
#[test]
fn h10_auxiliar_ligado_a_acao_original_mantem_a_juncao() {
    let (a, b, capitulo, chave) = conflito_de_exclusao();
    b.resolver(&chave, Acao::Restaurar).expect("restaurar");
    entregar(&b, &a);
    assert!(
        a.abertas().is_empty(),
        "a decisão virou pergunta: a posição perdeu a junção legítima"
    );
    convergiram(&a, &b, &capitulo);
    let posicao = sync_codec::posicao::posicao_do_item("chapter").expect("tipo da posição");
    assert_eq!(
        a.revisao(posicao, &capitulo),
        b.revisao(posicao, &capitulo),
        "a posição não convergiu"
    );
}

/// **H11 — num terceiro aparelho sem o índice local, nada se perde.**
///
/// Sem prova, sem junção especial: ou converge por ser sequencial, ou abre pergunta. O que não
/// pode é a edição local sumir.
#[test]
fn h11_terceiro_aparelho_sem_indice_nao_perde_edicao() {
    let (a, _b, c, capitulo, chave) = tres_aparelhos();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");
    c.escrever(&capitulo, "<p>edição própria de C</p>");
    let edicao_de_c = c.conteudo(&capitulo);

    entregar(&a, &c);
    assert_eq!(
        c.conteudo(&capitulo),
        edicao_de_c,
        "a edição de C foi sobrescrita por uma decisão que ele não tinha como conferir"
    );
    assert!(
        !c.abertas().is_empty() || c.pendentes() > 0,
        "a decisão que não coube precisa virar pergunta ou espera"
    );
}

/// **H12 — a mescla de tags não alcança a marcação de outra tag.**
#[test]
fn h12_mescla_nao_mexe_em_marcacao_de_outra_tag() {
    use crate::application::knowledge_service;
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);

    // Uma tag alheia ao conflito, marcada no capítulo e sincronizada.
    let alheia =
        knowledge_service::create_tag(&a.banco.database, &a.eu, &universo, "Sol", "#333333")
            .expect("tag alheia")
            .id;
    knowledge_service::set_tag(
        &a.banco.database,
        &a.eu,
        "chapter",
        &capitulo,
        &alheia,
        true,
    )
    .expect("marcar");
    sincronizar(&a, &b);
    let marcacao = AggregateRef::new(
        "tag_assignment",
        sync_codec::manuscrito::id_da_atribuicao(&alheia, "chapter", &capitulo),
    );
    let payload = sync_codec::ler_canonico(&a.banco.connection(), &marcacao)
        .expect("ler")
        .expect("marcação em A")
        .payload;
    let criacao = a
        .revisao(&marcacao.aggregate_type, &marcacao.aggregate_id)
        .expect("revisão da marcação");

    // B desmarca: a cabeça dele é o tombstone da marcação.
    knowledge_service::set_tag(
        &b.banco.database,
        &b.eu,
        "chapter",
        &capitulo,
        &alheia,
        false,
    )
    .expect("desmarcar");
    let tombstone = b
        .tombstone(&marcacao.aggregate_type, &marcacao.aggregate_id)
        .expect("tombstone em B");

    // E agora o conflito de nome de tag, com uma decisão forjada que tenta ressuscitar a marcação
    // alheia pela regra de dois pais.
    knowledge_service::create_tag(&a.banco.database, &a.eu, &universo, "Mar", "#111111")
        .expect("tag de A");
    knowledge_service::create_tag(&b.banco.database, &b.eu, &universo, "mar", "#222222")
        .expect("tag de B");
    entregar(&a, &b);
    entregar(&b, &a);
    let chave = a.uma_aberta();
    a.resolver(&chave, Acao::Mesclar).expect("mesclar");
    let decisao = grupo_da_decisao(&a, &b);
    let forjado = forjar_efeito_auxiliar(
        &a, &decisao, &marcacao, &criacao, &tombstone, &payload, &universo,
    );

    let _ = receber(&b, &forjado);
    assert_eq!(
        b.tombstone(&marcacao.aggregate_type, &marcacao.aggregate_id)
            .as_deref(),
        Some(tombstone.as_str()),
        "a mescla ressuscitou a marcação de outra tag"
    );
}

/// **H13 — a resolução de exclusão bloqueada não alcança agregado fora da ação excluída.**
#[test]
fn h13_exclusao_bloqueada_nao_alcanca_agregado_de_fora() {
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);
    let (alvo, base, _rev, id) = alvo_alheio(&a, &b, &universo);
    let conteudo_de_b = b.conteudo(&id);
    let cabeca_de_b = b.revisao("chapter", &id).expect("cabeça de B");

    // A exclui o livro inteiro; B editou um capítulo dele: a exclusão fica bloqueada em B.
    let livro: String = a
        .banco
        .connection()
        .query_row(
            "SELECT book_id FROM chapters WHERE id = ?1",
            [&capitulo],
            |row| row.get(0),
        )
        .expect("livro do capítulo");
    b.escrever(&capitulo, "<p>B mexeu no capítulo do livro</p>");
    crate::application::manuscript_service::delete_book(&a.banco.database, &a.eu, &livro)
        .expect("A exclui o livro");
    entregar(&b, &a);
    entregar(&a, &b);
    // O cenário tem duas perguntas abertas em B (a do capítulo alheio e a do livro); a desta é a
    // da exclusão bloqueada.
    let (chave, _, _) = b
        .abertas()
        .into_iter()
        .find(|(_, kind, _)| kind == "parent_deletion_blocked")
        .expect("a exclusão do livro tinha de ficar bloqueada em B");
    b.resolver(&chave, Acao::ManterLocal)
        .expect("manter o local");
    let decisao = grupo_da_decisao(&b, &a);
    let payload = sync_codec::ler_canonico(&b.banco.connection(), &alvo)
        .expect("ler")
        .expect("estado")
        .payload;
    let forjado = forjar_efeito_auxiliar(
        &b,
        &decisao,
        &alvo,
        &base,
        &cabeca_de_b,
        &payload,
        &universo,
    );

    let _ = receber(&a, &forjado);
    assert_eq!(
        b.conteudo(&id),
        conteudo_de_b,
        "a decisão de exclusão bloqueada alcançou um capítulo de fora da ação"
    );
}

/// **H14 — a resolução de um grupo não aceita membro de fora da ação original.**
#[test]
fn h14_resolucao_de_grupo_nao_aceita_membro_externo() {
    let (a, b, capitulo, chave) = conflito_de_exclusao();
    let universo = a.universo();
    let (alvo, base, _rev, id) = alvo_alheio(&a, &b, &universo);
    let conteudo_de_a = a.conteudo(&id);
    let cabeca_de_a = a.revisao("chapter", &id).expect("cabeça de A");

    b.resolver(&chave, Acao::Restaurar).expect("restaurar");
    let decisao = grupo_da_decisao(&b, &a);
    let payload = sync_codec::ler_canonico(&b.banco.connection(), &alvo)
        .expect("ler")
        .expect("estado")
        .payload;
    let forjado = forjar_efeito_auxiliar(
        &b,
        &decisao,
        &alvo,
        &base,
        &cabeca_de_a,
        &payload,
        &universo,
    );

    let _ = receber(&a, &forjado);
    assert_eq!(
        a.conteudo(&id),
        conteudo_de_a,
        "um membro externo entrou na resolução do grupo e sobrescreveu A"
    );
    assert!(
        a.conteudo(&capitulo).is_some() || !a.abertas().is_empty(),
        "nem restaurou nem perguntou"
    );
}

/// **H15 — decisão concorrente continua resolvível recursivamente, e converge.**
#[test]
fn h15_decisao_concorrente_continua_resolvivel() {
    let (a, b, capitulo, chave) = conflito_de_edicao();
    let em_a = a.ficar_com(&chave, true);
    let em_b = b.ficar_com(&chave, true);
    a.resolver(&chave, em_a).expect("A decide");
    b.resolver(&chave, em_b).expect("B decide");
    sincronizar(&a, &b);

    let (sobre_a_decisao, kind, tipo) = a.abertas().remove(0);
    assert_eq!(
        (kind.as_str(), tipo.as_str()),
        ("concurrent", "conflict_resolution")
    );
    let acao = a.ficar_com(&sobre_a_decisao, false);
    a.resolver(&sobre_a_decisao, acao)
        .expect("resolver a decisão");
    sincronizar(&a, &b);
    convergiram(&a, &b, &capitulo);
    for aparelho in [&a, &b] {
        assert!(
            aparelho.abertas().is_empty(),
            "sobrou conflito depois da recursão"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Hardening transversal: a matriz dos estados perigosos, as permutações de
// entrega e as duas regressões (blob e bootstrap).
// ═══════════════════════════════════════════════════════════════════════════

/// O estado local quando o efeito de uma resolução chega.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Estado {
    /// O agregado nunca existiu aqui, e a história também está vazia.
    NuncaExistiu,
    /// Nunca materializou, mas as revisões dos dois lados são conhecidas (sem decisão que as
    /// explique). Ausência que NÃO é prova.
    SemCabecaComHistoria,
    /// A cabeça é a revisão que a decisão escolheu.
    CabecaEscolhida,
    /// A cabeça é o outro lado do conflito.
    CabecaDoOutroLado,
    /// A cabeça é uma terceira revisão, que a decisão não conhece.
    CabecaTerceira,
    /// Excluído aqui, com tombstone.
    Excluido,
    /// Excluído aqui, e o tombstone coletado por um GC futuro.
    ExcluidoSemTombstone,
}

/// **A matriz determinística dos estados perigosos.**
///
/// Para cada estado local, o mesmo efeito de resolução — com par e sem par. A invariante vale para
/// a linha inteira: **nenhum estado ambíguo produz sobrescrita silenciosa**. Materializar só é
/// aceitável quando a cabeça daqui está no par (junção de dois pais) ou quando o agregado
/// comprovadamente nunca materializou.
#[test]
fn matriz_de_estados_perigosos_nao_sobrescreve_em_silencio() {
    use crate::domain::sync::ROOT_REVISION;
    use crate::infrastructure::sqlite::sync_apply::{
        apply_efeito_de_resolucao, apply_remote_event, envelope_de_origem, Applied,
    };
    use crate::infrastructure::sqlite::sync_codec::resolucao::ParDoEfeito;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};
    use rusqlite::TransactionBehavior;

    const ORIGEM: &str = "dev-remoto";
    let capitulo = |titulo: &str| {
        format!(
            "{{\"id\":\"cap-1\",\"bookId\":\"b1\",\"title\":\"{titulo}\",\"content\":\"<p>{titulo}</p>\",\"summary\":\"\",\"sceneOrigin\":\"\",\"sceneDestination\":\"\",\"status\":\"rascunho\",\"canonStatus\":\"canon\",\"customFields\":[]}}"
        )
    };

    let casos: &[(Estado, bool, bool)] = &[
        // (estado local, o efeito traz par?, pode materializar?)
        (Estado::CabecaEscolhida, true, true),
        (Estado::CabecaDoOutroLado, true, true),
        (Estado::CabecaTerceira, true, false),
        (Estado::CabecaTerceira, false, false),
        // Excluído aqui, e o par da decisão não inclui o tombstone: ela não viu esta exclusão,
        // então não pode desfazê-la. Quem restaura é a resolução que parte do próprio tombstone
        // (H5, H10) — e aí a cabeça está no par.
        (Estado::Excluido, true, false),
        (Estado::ExcluidoSemTombstone, true, false),
        (Estado::ExcluidoSemTombstone, false, false),
        (Estado::SemCabecaComHistoria, true, false),
        (Estado::SemCabecaComHistoria, false, false),
        (Estado::NuncaExistiu, true, false),
        (Estado::NuncaExistiu, false, false),
    ];

    for (estado, com_par, pode_materializar) in casos.iter().copied() {
        let banco = TemporaryDatabase::new();
        let mut connection = banco.database.write().expect("escrita");
        seed_universe(&connection, "u1");
        connection
            .execute_batch(
                "INSERT INTO stories (id, universe_id, name) VALUES ('s1', 'u1', 'Historia');
                 INSERT INTO books (id, story_id, name) VALUES ('b1', 's1', 'Livro');
                 INSERT INTO sync_devices (device_id, name, ed25519_public, is_self)
                   VALUES ('dev-remoto', 'Remoto', 'CHAVE', 0);",
            )
            .expect("semear");
        let agregado = AggregateRef::new("chapter", "cap-1");

        let aplicar = |connection: &mut rusqlite::Connection, envelope: &EventEnvelope| {
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("transação");
            let resultado = apply_remote_event(&tx, envelope).expect("aplicar");
            tx.commit().expect("commit");
            resultado
        };

        // A história comum: criação, e depois os dois lados do conflito.
        let criacao = envelope_de_origem(
            ORIGEM,
            1,
            "u1",
            &agregado,
            Operation::Upsert,
            &capitulo("original"),
            ROOT_REVISION,
        );
        if estado != Estado::NuncaExistiu {
            assert_eq!(aplicar(&mut connection, &criacao), Applied::Aplicado);
        }
        let escolhida = envelope_de_origem(
            ORIGEM,
            2,
            "u1",
            &agregado,
            Operation::Upsert,
            &capitulo("escolhida"),
            &criacao.new_rev,
        );
        let outra = envelope_de_origem(
            ORIGEM,
            3,
            "u1",
            &agregado,
            Operation::Upsert,
            &capitulo("do outro lado"),
            &criacao.new_rev,
        );

        // O estado local de cada caso. As revisões que não materializam entram na história como
        // uma divergência as registraria.
        let registrar = |connection: &rusqlite::Connection, envelope: &EventEnvelope| {
            connection
                .execute(
                    "INSERT OR IGNORE INTO sync_revision_history
                        (aggregate_type, aggregate_id, rev, base_rev, event_id)
                     VALUES ('chapter', 'cap-1', ?1, ?2, ?3)",
                    rusqlite::params![&envelope.new_rev, &envelope.base_rev, &envelope.event_id],
                )
                .expect("registrar revisão");
        };
        match estado {
            Estado::NuncaExistiu => {}
            Estado::SemCabecaComHistoria => {
                registrar(&connection, &escolhida);
                registrar(&connection, &outra);
                connection
                    .execute("DELETE FROM sync_aggregate_state", [])
                    .expect("sem cabeça");
                connection
                    .execute("DELETE FROM chapters WHERE id = 'cap-1'", [])
                    .expect("sem domínio");
            }
            Estado::CabecaEscolhida => {
                assert_eq!(aplicar(&mut connection, &escolhida), Applied::Aplicado);
                registrar(&connection, &outra);
            }
            Estado::CabecaDoOutroLado => {
                assert_eq!(aplicar(&mut connection, &outra), Applied::Aplicado);
                registrar(&connection, &escolhida);
            }
            Estado::CabecaTerceira => {
                let terceira = envelope_de_origem(
                    ORIGEM,
                    4,
                    "u1",
                    &agregado,
                    Operation::Upsert,
                    &capitulo("terceira"),
                    &criacao.new_rev,
                );
                assert_eq!(aplicar(&mut connection, &terceira), Applied::Aplicado);
                registrar(&connection, &escolhida);
                registrar(&connection, &outra);
            }
            Estado::Excluido | Estado::ExcluidoSemTombstone => {
                let exclusao = envelope_de_origem(
                    ORIGEM,
                    5,
                    "u1",
                    &agregado,
                    Operation::Delete,
                    "",
                    &criacao.new_rev,
                );
                assert_eq!(aplicar(&mut connection, &exclusao), Applied::Aplicado);
                registrar(&connection, &escolhida);
                registrar(&connection, &outra);
                if estado == Estado::ExcluidoSemTombstone {
                    connection
                        .execute("DELETE FROM sync_tombstones", [])
                        .expect("simular a coleta");
                }
            }
        }

        let antes: Option<String> = connection
            .query_row(
                "SELECT content FROM chapters WHERE id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("estado antes");

        // O efeito da resolução: parte da revisão escolhida, e o par é o outro lado.
        let efeito = envelope_de_origem(
            "dev-remoto",
            6,
            "u1",
            &agregado,
            Operation::Upsert,
            &capitulo("resultado da decisão"),
            &escolhida.new_rev,
        );
        let par = ParDoEfeito {
            outra: if com_par {
                outra.new_rev.clone()
            } else {
                String::new()
            },
            de_participante: true,
        };
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transação");
        let resultado = apply_efeito_de_resolucao(&tx, &efeito, &par).expect("aplicar o efeito");
        tx.commit().expect("commit");

        let depois: Option<String> = connection
            .query_row(
                "SELECT content FROM chapters WHERE id = 'cap-1'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("estado depois");
        let caso = format!("{estado:?}, com_par={com_par}");
        if pode_materializar {
            assert_eq!(
                resultado,
                Applied::Aplicado,
                "{caso}: o efeito legítimo não entrou"
            );
            assert_eq!(
                depois.as_deref(),
                Some("<p>resultado da decisão</p>"),
                "{caso}: o efeito legítimo não materializou"
            );
        } else {
            assert_ne!(
                resultado,
                Applied::Aplicado,
                "{caso}: o efeito entrou num estado que não o autoriza"
            );
            assert_eq!(
                depois, antes,
                "{caso}: SOBRESCRITA SILENCIOSA — o estado local mudou"
            );
        }
    }
}

/// **Permutações de entrega.** Classes causais diferentes, o mesmo destino: ou converge, ou
/// pergunta. Nunca perde a edição de quem recebeu.
#[test]
fn permutacoes_de_entrega_nunca_perdem_edicao() {
    // Com os dois participantes já aqui, a decisão aplica; repetida, é idempotente.
    for ordem in ["tudo-de-uma-vez", "decisao-repetida"] {
        let (a, b, capitulo, chave) = conflito_de_edicao();
        let acao = a.ficar_com(&chave, true);
        a.resolver(&chave, acao).expect("A resolve");
        let tudo = a.eventos_para(&b);
        let decisao = grupo_da_decisao(&a, &b);
        receber(&b, &tudo).expect("tudo");
        if ordem == "decisao-repetida" {
            receber(&b, &tudo).expect("de novo, inteiro");
            receber(&b, &decisao).expect("e a decisão sozinha mais uma vez");
        }
        assert_eq!(
            b.conteudo(&capitulo),
            a.conteudo(&capitulo),
            "{ordem}: não convergiu"
        );
        assert!(b.abertas().is_empty(), "{ordem}: sobrou conflito");
        assert_eq!(
            b.decisoes().len(),
            1,
            "{ordem}: a decisão entrou mais de uma vez"
        );
    }

    // E num aparelho que ainda não tem os dois lados, a decisão ESPERA — sem tocar no conteúdo.
    let (a, _b, c, capitulo, chave) = tres_aparelhos();
    let antes = c.conteudo(&capitulo);
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao).expect("A resolve");
    let decisao = grupo_da_decisao(&a, &c);
    let _ = receber(&c, &decisao);
    assert_eq!(
        c.conteudo(&capitulo),
        antes,
        "a decisão mexeu no conteúdo antes de os participantes chegarem"
    );
    entregar(&a, &c);
    assert_eq!(
        c.conteudo(&capitulo),
        a.conteudo(&capitulo),
        "com tudo entregue, o conteúdo não convergiu"
    );
    // O lado perdedor chega DEPOIS da decisão, e C ainda não tem como ligar um ao outro: ele
    // pergunta em vez de engolir. Pergunta a mais é resposta aceitável; perda não é.
    for (_, kind, _) in c.abertas() {
        assert_eq!(
            kind, "concurrent",
            "o terceiro aparelho inventou outro tipo de conflito"
        );
    }
}

/// **Regressão da D12 com a etapa F**: um efeito que depende de blob não materializa nada
/// enquanto o arquivo não chega — e o grupo inteiro espera junto.
#[test]
fn resolucao_que_depende_de_blob_espera_o_grupo_inteiro() {
    use crate::infrastructure::blob_document::ATTR_BLOB;
    let a = Aparelho::novo();
    let b = Aparelho::novo();
    let universo = a.universo();
    let capitulo = a.capitulo_novo(&universo);
    sincronizar(&a, &b);

    // A edita com imagem; B edita sem. A imagem entra ANTES de o conflito existir.
    let bytes = b"imagem-da-decisao".to_vec();
    let hash = a.store.put(&bytes).expect("publicar em A");
    a.escrever(
        &capitulo,
        &format!("<p>com imagem</p><img {ATTR_BLOB}=\"{hash}\">"),
    );
    b.escrever(&capitulo, "<p>versão de B</p>");
    entregar(&b, &a);
    let chave = a.uma_aberta();
    let acao = a.ficar_com(&chave, true);
    a.resolver(&chave, acao)
        .expect("A resolve ficando com a dela");

    let conteudo_de_b = b.conteudo(&capitulo);
    let lote = a.eventos_para(&b);
    let relatorio = receber(&b, &lote).expect("receber sem o blob");
    assert!(
        !relatorio.esperando_blob.is_empty(),
        "o grupo não pediu o blob que falta"
    );
    assert_eq!(
        b.conteudo(&capitulo),
        conteudo_de_b,
        "materializou parte do grupo sem o blob"
    );
    assert!(
        b.decisoes().is_empty(),
        "a decisão entrou sem o blob do efeito"
    );

    // O blob chega, e aí sim o grupo inteiro entra.
    b.store.put(&bytes).expect("publicar em B");
    receber(&b, &lote).expect("receber com o blob");
    assert_eq!(b.conteudo(&capitulo), a.conteudo(&capitulo));
    assert_eq!(b.decisoes().len(), 1);
}
