//! **A ordem do arranque, num lugar só** (NH-079 etapa C, fatia 2).
//!
//! ```text
//! migrations            (database::upgrade)          → UpgradingBlobs
//!   → conversão de mídia (blob_upgrade)              → Adopting
//!   → adoção do acervo   (genese::adotar)            → confere que não sobrou órfão
//!   → Ready                                          → aplicação e sync liberados
//! ```
//!
//! Antes desta fatia, a sequência vivia no arranque do frontend: ele chamava a conversão de mídia
//! e seguia em frente. Duas coisas não podiam continuar assim. A adoção **precisa** rodar depois da
//! conversão — ela registra o estado como ele é, e a conversão ainda ia mudá-lo — e nenhum serviço
//! sincronizável pode ficar disponível antes dela, porque até lá o acervo não tem passado causal.
//!
//! Ordem em código, e não em convenção: quem chama isto chama uma função. Não há como inverter os
//! passos sem editar esta função, e há gate que prova a ordem pelo efeito, não pela intenção.
//!
//! ## Falha de adoção não é queda genérica
//!
//! ```text
//! erro na conversão de mídia   → RecoveryRequired, com a causa
//! erro na adoção                → RecoveryRequired, com a causa
//! órfão depois da adoção        → RecoveryRequired, com a causa
//! ```
//!
//! `RecoveryRequired` é o caminho que o ADR 0007 já usa: o banco fica preservado, todo comando de
//! domínio é recusado com mensagem, e a tela de recuperação é o que o escritor vê. Nada de
//! operação parcial, e nada de o aplicativo morrer sem dizer por quê.
//!
//! ## A exceção declarada: pendência de mídia
//!
//! Mídia legada que o aplicativo **não consegue** converter (URL externa, base64 quebrada) vira
//! pendência — e isso não é erro do arranque, é um item que continua preservado no banco. O ADR
//! 0010 já decidiu que isso não pode tirar o escritor do texto dele. Então:
//!
//! ```text
//! pendência de mídia aberta → aplicação LIBERADA (Ready), acervo NÃO adotado,
//!                             sincronização indisponível com motivo legível
//! ```
//!
//! É a mesma régua que o pareamento já aplicava desde a B1. O escritor continua escrevendo; o que
//! espera é a sincronização, e ela diz por quê.

use serde::Serialize;

use crate::application::{blob_upgrade, genese};
use crate::database::error::DatabaseCommandResult;
use crate::database::estado::{EstadoDoBanco, FaseDoBanco};
use crate::domain::identity::DeviceIdentity;
use crate::infrastructure::blob_store::BlobStore;
use crate::infrastructure::sqlite::SqliteDatabase;

/// O que o arranque fez com o acervo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumoDoAcervo {
    pub assets: blob_upgrade::ResumoDoArranque,
    pub adocao: genese::ResumoDaAdocao,
    /// A sincronização pode ser usada nesta sessão?
    pub sincronizacao_disponivel: bool,
    /// Vazio quando ela está disponível; o motivo legível quando não está.
    pub motivo_da_indisponibilidade: String,
}

/// **Prepara o acervo e libera o aplicativo — nesta ordem, ou não libera.**
pub fn preparar_acervo(
    database: &SqliteDatabase,
    store: &BlobStore,
    identidade: &DeviceIdentity,
    estado: &EstadoDoBanco,
) -> DatabaseCommandResult<ResumoDoAcervo> {
    estado.definir(FaseDoBanco::UpgradingBlobs);
    let assets = match blob_upgrade::preparar_assets(database, store) {
        Ok(assets) => assets,
        Err(erro) => {
            // Erro de verdade na conversão (disco, banco) — não é a pendência de uma imagem
            // ilegível, que `preparar_assets` devolve como resumo.
            estado.definir(FaseDoBanco::RecoveryRequired);
            return Err(erro);
        }
    };

    estado.definir(FaseDoBanco::Adopting);
    let adocao = match genese::adotar(database, identidade) {
        Ok(adocao) => adocao,
        Err(erro) if assets.pendencias_abertas > 0 => {
            // A adoção recusou porque a mídia ainda não converteu inteira. O texto continua
            // acessível; o que fica indisponível é a sincronização, e com motivo.
            estado.definir(FaseDoBanco::Ready);
            return Ok(ResumoDoAcervo {
                assets,
                adocao: genese::ResumoDaAdocao::default(),
                sincronizacao_disponivel: false,
                motivo_da_indisponibilidade: erro.message,
            });
        }
        Err(erro) => {
            estado.definir(FaseDoBanco::RecoveryRequired);
            return Err(erro);
        }
    };

    // **Não há conferência de órfão aqui, e é de propósito.** `genese::adotar` só devolve `Ok`
    // depois de garantir que não sobrou agregado sem revisão: na primeira adoção, dentro da
    // própria transação; num acervo já adotado, no caminho rápido, que falha fechado. Repetir a
    // checagem aqui seria código que nenhum teste distingue de código morto — e foi exatamente
    // isso que a mutação mostrou.

    estado.definir(FaseDoBanco::Ready);
    Ok(ResumoDoAcervo {
        assets,
        adocao,
        sincronizacao_disponivel: true,
        motivo_da_indisponibilidade: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::test_support::{seed_universe, TemporaryDatabase};

    /// Um aparelho no instante anterior ao arranque: banco migrado, identidade registrada, e o
    /// acervo como ele estiver.
    struct Aparelho {
        banco: TemporaryDatabase,
        store: BlobStore,
        identidade: DeviceIdentity,
        estado: EstadoDoBanco,
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
            let dados = std::env::temp_dir().join(format!("nh-arranque-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dados).expect("criar diretório");
            let identidade = crate::application::sync_bootstrap::prepare(&dados, &banco.database)
                .expect("identidade");
            let estado = EstadoDoBanco::default();
            // Onde o `database::upgrade` deixa o banco: migrations aplicadas, acervo por preparar.
            estado.definir(FaseDoBanco::UpgradingBlobs);
            Self {
                banco,
                store: BlobStore::new(&dados),
                identidade,
                estado,
                dados,
            }
        }

        /// Um acervo anterior ao Sync V2: linhas de domínio que nenhum evento explica.
        fn com_acervo_legado(self) -> Self {
            let connection = self.banco.connection();
            seed_universe(&connection, "u-legado");
            connection
                .execute_batch(
                    "INSERT INTO stories (id, universe_id, name) VALUES ('s1','u-legado','Saga');
                     INSERT INTO books (id, story_id, name) VALUES ('b1','s1','Livro');",
                )
                .expect("semear acervo legado");
            drop(connection);
            self
        }

        fn preparar(&self) -> DatabaseCommandResult<ResumoDoAcervo> {
            preparar_acervo(
                &self.banco.database,
                &self.store,
                &self.identidade,
                &self.estado,
            )
        }

        fn eventos(&self) -> i64 {
            self.banco
                .connection()
                .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
                .expect("contar")
        }
    }

    /// **Gate 1 — banco legado: abre, converte, adota sozinho, libera.**
    #[test]
    fn banco_legado_e_adotado_no_arranque_e_libera_o_aplicativo() {
        let aparelho = Aparelho::novo().com_acervo_legado();
        assert!(
            aparelho.estado.exigir_pronto().is_err(),
            "liberou antes da hora"
        );

        let resumo = aparelho.preparar().expect("arranque");

        assert!(resumo.adocao.adotados > 0, "{resumo:?}");
        assert!(resumo.sincronizacao_disponivel);
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::Ready);
        assert!(aparelho.estado.exigir_pronto().is_ok());
        let connection = aparelho.banco.connection();
        assert!(genese::primeiro_orfao(&connection)
            .expect("órfão")
            .is_none());
    }

    /// **Gate 2 — banco novo: a adoção roda, não emite nada, e registra a versão.**
    #[test]
    fn banco_novo_adota_zero_e_libera() {
        let aparelho = Aparelho::novo();

        let resumo = aparelho.preparar().expect("arranque");

        assert_eq!(resumo.adocao.adotados, 0, "{resumo:?}");
        assert_eq!(
            aparelho.eventos(),
            0,
            "o arranque emitiu evento num banco vazio"
        );
        assert!(resumo.sincronizacao_disponivel);
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::Ready);
        let adocoes: i64 = aparelho
            .banco
            .connection()
            .query_row("SELECT COUNT(*) FROM sync_adoptions", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(adocoes, 1, "a versão adotada precisa ficar registrada");
    }

    /// **Gate 3 — a adoção falha: o aplicativo não fica pronto, e a causa sobrevive.**
    ///
    /// Aqui a falha é de verdade (um agregado que não lê canonicamente), e não pendência de mídia.
    #[test]
    fn falha_na_adocao_nao_libera_e_cai_na_recuperacao() {
        let aparelho = Aparelho::novo().com_acervo_legado();
        {
            // Duas linhas do MESMO conjunto de modelos com a mesma chave: o codec recusa escolher
            // uma, e a leitura canônica falha (B6).
            let connection = aparelho.banco.connection();
            connection
                .execute_batch(
                    "INSERT INTO entity_templates
                        (id, universe_id, entity_type, attribute_key, default_value, sort_order)
                     VALUES ('t1','u-legado','character','Idade','10',0),
                            ('t2','u-legado','character','Idade','20',1);",
                )
                .expect("acervo inconsistente");
        }

        let erro = aparelho
            .preparar()
            .expect_err("acervo inconsistente não adota");

        // A causa precisa ser a REAL, e não "o acervo não foi adotado": esta segunda mensagem
        // apareceria mesmo se o erro da adoção tivesse sido engolido e a checagem seguinte
        // pegasse o rastro. São diagnósticos diferentes para quem vai consertar.
        assert!(
            erro.message.contains("Idade"),
            "a causa da falha se perdeu pelo caminho: {}",
            erro.message
        );
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::RecoveryRequired);
        assert!(
            aparelho.estado.exigir_pronto().is_err(),
            "o aplicativo foi liberado com o acervo pela metade"
        );
        assert_eq!(
            aparelho.eventos(),
            0,
            "sobrou evento de uma adoção que falhou"
        );
        assert_eq!(
            aparelho
                .banco
                .connection()
                .query_row("SELECT COUNT(*) FROM sync_adoptions", [], |row| row
                    .get::<_, i64>(0))
                .expect("contar"),
            0
        );
    }

    /// **Gate 4 — reinício depois de adotado: no-op, nada avança.**
    #[test]
    fn reinicio_depois_da_adocao_nao_emite_nada() {
        let aparelho = Aparelho::novo().com_acervo_legado();
        aparelho.preparar().expect("primeiro arranque");
        let eventos = aparelho.eventos();
        let cursor: i64 = aparelho
            .banco
            .connection()
            .query_row(
                "SELECT COALESCE(MAX(last_seq_applied), 0) FROM sync_cursors",
                [],
                |row| row.get(0),
            )
            .expect("cursor");

        let resumo = aparelho.preparar().expect("segundo arranque");

        assert!(resumo.adocao.ja_estava_adotado, "{resumo:?}");
        assert_eq!(aparelho.eventos(), eventos, "o reinício emitiu evento");
        assert_eq!(
            aparelho
                .banco
                .connection()
                .query_row(
                    "SELECT COALESCE(MAX(last_seq_applied), 0) FROM sync_cursors",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("cursor"),
            cursor,
            "o reinício mexeu no cursor"
        );
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::Ready);
    }

    /// **Gate 5 — acervo já adotado que ganhou um órfão: falha fechada, e não adoção de contrabando.**
    #[test]
    fn orfao_em_acervo_adotado_falha_o_arranque_sem_adotar() {
        let aparelho = Aparelho::novo().com_acervo_legado();
        aparelho.preparar().expect("primeiro arranque");
        let eventos = aparelho.eventos();

        // Um capítulo aparece sem passar pela fronteira — bug, adulteração ou cobertura nova.
        aparelho
            .banco
            .connection()
            .execute_batch(
                "INSERT INTO chapters (id, book_id, title, content, word_count)
                 VALUES ('cap-orfao','b1','Sem revisão','texto',1);",
            )
            .expect("órfão");

        let erro = aparelho
            .preparar()
            .expect_err("órfão tem de derrubar o arranque");

        assert!(erro.message.contains("cap-orfao"), "{}", erro.message);
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::RecoveryRequired);
        assert!(aparelho.estado.exigir_pronto().is_err());
        assert_eq!(
            aparelho.eventos(),
            eventos,
            "o arranque adotou o órfão de contrabando"
        );
    }

    /// **Gate de ordem — a conversão de mídia acontece ANTES da adoção.**
    ///
    /// Não é a ordem das chamadas que está sendo testada, e sim o efeito: a revisão de gênese do
    /// universo carrega o `coverBlobHash` que só existe **depois** da conversão. Se a adoção
    /// rodasse antes, a primeira revisão descreveria a capa legada, e o estado do banco deixaria de
    /// bater com ela no instante seguinte.
    #[test]
    fn a_midia_e_convertida_antes_de_a_genese_registrar_o_estado() {
        let aparelho = Aparelho::novo().com_acervo_legado();
        {
            // Capa legada, no formato que o ADR 0010 converte: inline que vira blob.
            let connection = aparelho.banco.connection();
            connection
                .execute(
                    "UPDATE universes SET cover_image = ?1 WHERE id = 'u-legado'",
                    ["data:image/png;base64,YQ=="],
                )
                .expect("capa legada");
        }

        let resumo = aparelho.preparar().expect("arranque");
        assert!(
            resumo.assets.migrados > 0,
            "a conversão não rodou: {resumo:?}"
        );

        let payload: String = aparelho
            .banco
            .connection()
            .query_row(
                "SELECT e.payload FROM sync_events e
                  WHERE e.aggregate_type = 'universe' AND e.aggregate_id = 'u-legado'
                  ORDER BY e.seq LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("a revisão de gênese do universo");
        assert!(
            payload.contains(r#""coverBlobHash":"#) && !payload.contains(r#""coverBlobHash":"""#),
            "a gênese registrou o universo antes de a capa virar blob: {payload}"
        );

        // E a ordem das fases é a que o contrato diz.
        assert_eq!(
            aparelho.estado.historico(),
            vec![
                FaseDoBanco::UpgradingBlobs, // o próprio setup, antes do arranque
                FaseDoBanco::UpgradingBlobs,
                FaseDoBanco::Adopting,
                FaseDoBanco::Ready,
            ]
        );
    }

    /// **A exceção declarada:** pendência de mídia não tira o escritor do texto, e tira o sync.
    #[test]
    fn pendencia_de_midia_libera_o_texto_e_segura_a_sincronizacao() {
        let aparelho = Aparelho::novo().com_acervo_legado();
        aparelho
            .banco
            .connection()
            .execute(
                "UPDATE universes SET cover_image = 'https://exemplo.invalido/capa.png'
                  WHERE id = 'u-legado'",
                [],
            )
            .expect("mídia que não converte");

        let resumo = aparelho
            .preparar()
            .expect("o arranque não pode falhar por mídia ilegível");

        assert!(resumo.assets.pendencias_abertas > 0, "{resumo:?}");
        assert!(!resumo.sincronizacao_disponivel);
        assert!(
            resumo.motivo_da_indisponibilidade.contains("pendência"),
            "{}",
            resumo.motivo_da_indisponibilidade
        );
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::Ready);
        assert!(
            aparelho.estado.exigir_pronto().is_ok(),
            "o texto ficou inacessível"
        );
        assert_eq!(aparelho.eventos(), 0, "adotou com a mídia por converter");
    }
}
