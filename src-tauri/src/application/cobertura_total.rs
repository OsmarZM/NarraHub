//! **Gate de cobertura total do Sync V2** (NH-079 B6, item 7).
//!
//! A NH-079 nasceu de uma medição: 47 escritas públicas de domínio, 2 emitindo evento. O resto
//! mudava o arquivo do escritor e não chegava ao outro aparelho. As etapas B1–B6 fecharam isso
//! serviço a serviço, e este gate é o que impede a régua de voltar a escorregar.
//!
//! Ele cobra duas coisas, e as duas falham fechadas:
//!
//! ```text
//! 1  toda escrita pública de serviço passa pela Mutacao — ou está NESTA lista, com motivo
//! 2  todo efeito de exclusão do catálogo atinge agregado COBERTO por codec
//! ```
//!
//! ## Por que uma lista, e não uma exceção por atributo
//!
//! Estado local existe e é legítimo: uma sessão de convite não é acervo, e o índice de menções é
//! recalculado do texto. O que não pode existir é estado local **por esquecimento**. A lista
//! obriga a escrever o motivo no mesmo commit que cria a escrita, e o gate quebra tanto quando
//! aparece uma escrita nova fora dela quanto quando uma entrada dela deixa de existir — lista que
//! não é podada vira licença permanente.

/// Escrita pública de serviço que **não** passa pela `Mutacao`, com o motivo.
///
/// `(arquivo, função, por que não é acervo sincronizável)`
pub const ESCRITAS_LOCAIS_DECLARADAS: &[(&str, &str, &str)] = &[
    (
        "collaboration_service.rs",
        "save_session",
        "a sessão de convite é estado local: chave, token e validade deste aparelho, que não \
         viajam nem entram no bundle",
    ),
    (
        "collaboration_service.rs",
        "store_contribution",
        "a proposta do convidado ainda não é acervo — ela só toca o domínio quando aprovada, e \
         aprovar passa pela Mutacao (item 6)",
    ),
    (
        "collaboration_service.rs",
        "end_all_active",
        "encerrar sessões mexe no estado local do convite, não no acervo",
    ),
    (
        "collaboration_service.rs",
        "end_session",
        "idem: ciclo de vida do convite, local por definição",
    ),
    (
        "knowledge_service.rs",
        "sync_chapter_mentions",
        "índice derivado do texto do capítulo. O que viaja é o capítulo; cada aparelho recalcula \
         as menções ao aplicá-lo, e sincronizar o índice duplicaria a mesma informação em dois \
         históricos que poderiam divergir",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// As escritas públicas de cada serviço, fora dos testes: `(arquivo, função, usa Mutacao)`.
    fn escritas_publicas() -> Vec<(String, String, bool)> {
        let diretorio = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/application");
        let mut saida = Vec::new();
        for entrada in std::fs::read_dir(&diretorio).expect("ler src/application") {
            let caminho = entrada.expect("entrada").path();
            let nome_do_arquivo = caminho
                .file_name()
                .expect("nome")
                .to_string_lossy()
                .to_string();
            if !nome_do_arquivo.ends_with("_service.rs") {
                continue;
            }
            let fonte = std::fs::read_to_string(&caminho).expect("ler serviço");
            // Só o código de produção: um teste que abre a escrita direta é teste, não escrita de
            // serviço.
            let producao = fonte.split("#[cfg(test)]").next().unwrap_or("").to_string();
            for bloco in producao.split("\npub fn ").skip(1) {
                let funcao = bloco
                    .split('(')
                    .next()
                    .expect("nome da função")
                    .trim()
                    .to_string();
                if !bloco.contains("database.write()") {
                    continue;
                }
                saida.push((
                    nome_do_arquivo.clone(),
                    funcao,
                    bloco.contains("Mutacao::executar"),
                ));
            }
        }
        saida
    }

    /// **Escrita pública nova só entra pela `Mutacao` — ou declarando por que não é acervo.**
    #[test]
    fn toda_escrita_publica_passa_pela_mutacao_ou_esta_declarada() {
        let declaradas: Vec<(&str, &str)> = ESCRITAS_LOCAIS_DECLARADAS
            .iter()
            .map(|(arquivo, funcao, _)| (*arquivo, *funcao))
            .collect();

        let mut fora: Vec<String> = Vec::new();
        for (arquivo, funcao, usa_mutacao) in escritas_publicas() {
            if usa_mutacao {
                continue;
            }
            if !declaradas.contains(&(arquivo.as_str(), funcao.as_str())) {
                fora.push(format!("{arquivo}::{funcao}"));
            }
        }
        fora.sort();
        assert!(
            fora.is_empty(),
            "estas escritas públicas não passam pela Mutacao e não estão declaradas como estado \
             local: {fora:?}. Uma escrita de acervo fora da Mutacao não vira evento — o outro \
             aparelho do escritor nunca saberá dela."
        );
    }

    /// **A lista não vira licença permanente:** entrada que não corresponde mais a uma escrita
    /// direta tem de sair.
    #[test]
    fn nenhuma_declaracao_de_escrita_local_esta_obsoleta() {
        let escritas = escritas_publicas();
        for (arquivo, funcao, motivo) in ESCRITAS_LOCAIS_DECLARADAS {
            let existe = escritas
                .iter()
                .any(|(a, f, usa_mutacao)| a == arquivo && f == funcao && !usa_mutacao);
            assert!(
                existe,
                "{arquivo}::{funcao} está declarada como escrita local ({motivo}), e não é mais \
                 uma escrita direta. Tire a entrada da lista."
            );
            assert!(motivo.len() > 20, "{arquivo}::{funcao}: motivo vago");
        }
    }

    /// **Todo efeito de exclusão atinge agregado coberto.**
    ///
    /// Até a B6 o catálogo tinha efeitos apontando para agregados sem codec, cada um com uma ação
    /// declarada para enquanto isso durasse (bloquear a exclusão, recusar o universo). O último
    /// deles era `entity_templates`, fechado pelo `entity_template_set`. Daqui em diante, um
    /// efeito novo sobre agregado descoberto **para a etapa**, em vez de nascer com uma exceção.
    #[test]
    fn todo_efeito_de_exclusao_atinge_agregado_coberto() {
        use crate::infrastructure::sqlite::sync_codec::catalogo::{Efeito, EFEITOS};

        let descobertos: Vec<&str> = EFEITOS
            .iter()
            .filter(|efeito| matches!(efeito.efeito, Efeito::Delete | Efeito::Rewrite))
            .filter(|efeito| {
                !crate::infrastructure::sqlite::sync_codec::coberto(efeito.agregado_afetado)
            })
            .map(|efeito| efeito.agregado_afetado)
            .collect();

        assert!(
            descobertos.is_empty(),
            "estes agregados sofrem efeito de exclusão e não têm codec: {descobertos:?}. A \
             cobertura do Sync V2 está fechada desde a B6; um agregado novo precisa entrar com o \
             codec junto."
        );
    }
}
