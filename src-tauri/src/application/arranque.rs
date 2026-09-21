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
            // legível — e **só** legível: escrever agora criaria a revisão do agregado antes da
            // conversão, a conversão mudaria o estado logo depois, e a gênese pularia o agregado
            // por ele já ter revisão corrente. A revisão deixaria de descrever o banco.
            estado.definir(FaseDoBanco::ReadyReadOnly);
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

        /// Mídia legada que o aplicativo **não** consegue converter: vira pendência, não erro.
        fn com_midia_impossivel(self) -> Self {
            self.banco
                .connection()
                .execute(
                    "UPDATE universes SET cover_image = 'https://exemplo.invalido/capa.png'
                      WHERE id = 'u-legado'",
                    [],
                )
                .expect("mídia que não converte");
            self
        }

        /// O banco como os comandos o recebem: o handle nasce da fase, e é ele que recusa escrita.
        fn banco_conforme_a_fase(&self) -> SqliteDatabase {
            SqliteDatabase::conforme_a_fase(self.estado.fase(), self.banco.database.path())
        }

        fn nome_do_universo(&self) -> String {
            self.banco
                .connection()
                .query_row(
                    "SELECT name FROM universes WHERE id = 'u-legado'",
                    [],
                    |row| row.get(0),
                )
                .expect("nome")
        }

        /// A revisão corrente de cada agregado coberto descreve o estado do banco.
        fn invariante_de_materializacao(&self) {
            use crate::infrastructure::sqlite::sync_codec;
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
                let agregado = crate::domain::sync::AggregateRef::new(&tipo, &id);
                let registrado = sync_codec::payload_da_revisao_corrente(&connection, &agregado)
                    .expect("payload da revisão");
                let materializado = sync_codec::ler_canonico(&connection, &agregado)
                    .expect("estado")
                    .map(|estado| estado.payload);
                assert_eq!(
                    registrado, materializado,
                    "{tipo} {id}: a revisão corrente não é o estado do banco"
                );
            }
        }

        fn eventos(&self) -> i64 {
            self.banco
                .connection()
                .query_row("SELECT COUNT(*) FROM sync_events", [], |row| row.get(0))
                .expect("contar")
        }
    }

    /// **D0 — o arranque da C não pode tornar um aparelho novo "usado".**
    ///
    /// A adoção grava uma linha em `sync_adoptions`, e essa tabela é `ProtocoloNaoTransferido` —
    /// categoria que `tabelas_que_bloqueiam` usa inteira para decidir se um aparelho está virgem.
    /// Se ela contar, um aparelho recém-instalado deixa de ser elegível a receber bootstrap
    /// **por ter aberto o aplicativo uma vez**, e o pareamento perde o caminho de semeadura.
    ///
    /// Virgem é não ter acervo nem passado causal. A marca de que a adoção rodou (sobre nada) não
    /// é acervo.
    #[test]
    fn aparelho_novo_continua_elegivel_a_bootstrap_depois_do_arranque() {
        use crate::infrastructure::sqlite::sync_snapshot::receptor_elegivel;

        let aparelho = Aparelho::novo();
        aparelho.preparar().expect("arranque de um aparelho novo");
        assert_eq!(
            aparelho
                .banco
                .connection()
                .query_row("SELECT COUNT(*) FROM sync_adoptions", [], |row| row
                    .get::<_, i64>(0))
                .expect("contar"),
            1,
            "o cenário exige que a adoção tenha registrado a versão"
        );

        let mut connection = aparelho.banco.connection();
        assert!(
            receptor_elegivel(&mut connection).expect("consultar"),
            "o arranque tornou um aparelho novo inelegível a bootstrap"
        );
        drop(connection);

        // E escrita de domínio de verdade continua tirando a virgindade, como sempre.
        let banco = aparelho.banco_conforme_a_fase();
        crate::application::universe_service::create(
            &banco,
            &aparelho.store,
            &aparelho.identidade,
            "Primeiro universo",
            "",
            "",
        )
        .expect("criar universo");
        let mut connection = aparelho.banco.connection();
        assert!(
            !receptor_elegivel(&mut connection).expect("consultar"),
            "um aparelho com acervo continuou elegível a ser semeado por cima"
        );
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

    /// **Pendência de mídia: o texto abre, e mais nada.**
    ///
    /// O ADR 0010 diz que uma imagem inválida não pode tirar o escritor do texto dele. Não diz que
    /// ela autoriza escrever — e escrever aqui seria pior do que travar:
    ///
    /// ```text
    /// escrita antes da conversão  →  Mutacao cria a revisão de U
    ///   → blob_upgrade muda o coverBlobHash de U
    ///   → a gênese PULA U, que já tem revisão corrente
    ///   → a revisão corrente deixa de descrever o banco
    /// ```
    #[test]
    fn pendencia_de_midia_deixa_ler_o_texto_e_recusa_escrever() {
        let aparelho = Aparelho::novo().com_acervo_legado().com_midia_impossivel();

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
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::ReadyReadOnly);

        // 1. o texto pode ser lido — e pela guarda dos comandos, que é quem decide
        aparelho
            .estado
            .exigir_leitura()
            .expect("a leitura precisa continuar liberada no degradado");
        let banco = aparelho.banco_conforme_a_fase();
        let nome: String = banco
            .read()
            .expect("leitura tem de continuar liberada")
            .query_row(
                "SELECT name FROM universes WHERE id = 'u-legado'",
                [],
                |row| row.get(0),
            )
            .expect("ler o universo");
        assert_eq!(nome, "u-legado", "o seed usa o id como nome");

        // 2. a escrita é recusada — e é o mesmo caminho que a Mutacao usa
        let erro = banco.write().expect_err("escrita não pode ser liberada");
        assert!(erro.message.contains("imagens antigas"), "{}", erro.message);

        // 3. um comando de domínio de verdade recusa, e não deixa evento nenhum
        let antes = aparelho.eventos();
        let erro = crate::application::universe_service::update(
            &banco,
            &aparelho.store,
            &aparelho.identidade,
            "u-legado",
            crate::domain::universe::UniverseUpdate {
                name: Some("Nome novo".into()),
                description: None,
                cover_image: None,
            },
        )
        .expect_err("o escritor não pode alterar o universo antes da conversão");
        assert!(erro.message.contains("imagens antigas"), "{}", erro.message);
        assert_eq!(
            aparelho.eventos(),
            antes,
            "a escrita recusada deixou evento"
        );
        assert_eq!(
            aparelho.nome_do_universo(),
            "u-legado",
            "o nome mudou apesar da recusa"
        );

        // 4. e a sincronização recusa, pelo motivo dela: acervo sem adoção
        let connection = aparelho.banco.connection();
        let erro = genese::exigir_acervo_sem_orfaos(&connection)
            .expect_err("sync não pode rodar sobre acervo não adotado");
        assert!(erro.message.contains("não foi adotado"), "{}", erro.message);
    }

    /// **5 e 6 — resolvida a pendência, o fluxo completo libera a escrita, e a revisão bate.**
    #[test]
    fn resolvida_a_pendencia_o_novo_arranque_adota_e_libera_a_escrita() {
        let aparelho = Aparelho::novo().com_acervo_legado().com_midia_impossivel();
        aparelho.preparar().expect("primeiro arranque");
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::ReadyReadOnly);
        assert_eq!(aparelho.eventos(), 0, "adotou com mídia pendente");

        // O escritor resolve a pendência: troca a imagem impossível por uma convertível.
        aparelho
            .banco
            .connection()
            .execute(
                "UPDATE universes SET cover_image = 'data:image/png;base64,YQ=='
                  WHERE id = 'u-legado'",
                [],
            )
            .expect("substituir a mídia");
        aparelho
            .banco
            .connection()
            .execute("DELETE FROM blob_migration_issues", [])
            .expect("a pendência sai quando a mídia é resolvida");

        let resumo = aparelho.preparar().expect("segundo arranque");

        assert!(resumo.sincronizacao_disponivel, "{resumo:?}");
        assert!(resumo.adocao.adotados > 0, "{resumo:?}");
        assert_eq!(aparelho.estado.fase(), FaseDoBanco::Ready);

        // A escrita volta a ser aceita.
        let banco = aparelho.banco_conforme_a_fase();
        crate::application::universe_service::update(
            &banco,
            &aparelho.store,
            &aparelho.identidade,
            "u-legado",
            crate::domain::universe::UniverseUpdate {
                name: Some("Nome novo".into()),
                description: None,
                cover_image: None,
            },
        )
        .expect("com o acervo adotado, o escritor escreve");

        // 6. a revisão corrente do agregado afetado pela mídia descreve o estado canônico.
        aparelho.invariante_de_materializacao();
    }
}
