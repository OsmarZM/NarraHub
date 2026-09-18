//! O estado do banco neste processo, checado no ponto comum de acesso.
//!
//! ## Por que existe
//!
//! A migration segura (`upgrade.rs`) só protege se quem abre o banco esperar por ela. Enquanto essa
//! espera dependesse do `AppBootstrapService` chamar as funções na ordem certa, qualquer comando novo
//! chamado cedo — ou uma tela que dispare antes do arranque terminar — poderia ler ou escrever num
//! banco ainda no schema antigo, ou no meio da atualização.
//!
//! ```text
//! Unprepared         o processo abriu; ninguém conferiu o schema          → comandos recusados
//! Migrating          backup feito, plugin aplicando migrations            → comandos recusados
//! UpgradingBlobs     schema pronto; o legado de mídia ainda vai converter → comandos recusados
//! Adopting           mídia pronta; o acervo ainda vai ganhar as revisões  → comandos recusados
//! Ready              schema, mídia e acervo prontos, conferidos            → liberado
//! RecoveryRequired   schema mais novo, marcador ilegível, rollback falhou  → recusados até recuperar
//! ```
//!
//! ## Por que duas fases novas na etapa C
//!
//! Até a C, `Ready` era declarado assim que as migrations terminavam — antes da conversão de mídia
//! e antes de o acervo ter passado causal. A ordem que a etapa C exige é outra, e ela precisa ser
//! visível no estado, não só na ordem em que alguém chama as funções:
//!
//! ```text
//! migrations → mídia → adoção → Ready → sync disponível
//! ```
//!
//! Enquanto o acervo não é adotado, as linhas de domínio existem sem nenhuma revisão que as
//! explique. Liberar comandos ali deixaria o escritor editar um acervo que a sincronização ainda
//! não sabe descrever — e a primeira edição inventaria uma criação por cima de texto que já
//! existia.
//!
//! Quem transita é só o `upgrade.rs` (e a restauração de backup, que devolve para `Unprepared`).
//! Quem checa é `interface::tauri::database` — o único caminho dos comandos de domínio ao banco — e
//! o Sync V1, que abre o arquivo por conta própria.

use std::sync::Mutex;

use serde::Serialize;

use super::error::{DatabaseCommandError, DatabaseCommandResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FaseDoBanco {
    Unprepared,
    Migrating,
    /// Migrations aplicadas; o legado de mídia (ADR 0010) ainda vai converter.
    UpgradingBlobs,
    /// Mídia pronta; o acervo ainda vai ganhar as primeiras revisões (etapa C).
    Adopting,
    Ready,
    RecoveryRequired,
}

#[derive(Debug)]
pub struct EstadoDoBanco {
    fase: Mutex<FaseDoBanco>,
    /// Por onde o arranque passou, em ordem. Existe para que a ORDEM seja verificável: um teste
    /// que só olha o estado final não distingue "adotou depois da mídia" de "adotou antes".
    historico: Mutex<Vec<FaseDoBanco>>,
}

impl Default for EstadoDoBanco {
    fn default() -> Self {
        Self {
            fase: Mutex::new(FaseDoBanco::Unprepared),
            historico: Mutex::new(Vec::new()),
        }
    }
}

impl EstadoDoBanco {
    pub fn fase(&self) -> FaseDoBanco {
        *self
            .fase
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
    }

    pub fn definir(&self, fase: FaseDoBanco) {
        *self
            .fase
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner()) = fase;
        self.historico
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
            .push(fase);
    }

    /// As fases por onde este arranque passou, na ordem.
    pub fn historico(&self) -> Vec<FaseDoBanco> {
        self.historico
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
            .clone()
    }

    /// Libera o acesso só com o banco pronto. Mensagem diferente para cada motivo: "ainda
    /// preparando" pede espera; "precisa de recuperação" pede ação.
    pub fn exigir_pronto(&self) -> DatabaseCommandResult<()> {
        match self.fase() {
            FaseDoBanco::Ready => Ok(()),
            FaseDoBanco::Unprepared
            | FaseDoBanco::Migrating
            | FaseDoBanco::UpgradingBlobs
            | FaseDoBanco::Adopting => Err(DatabaseCommandError::unavailable(
                "O banco ainda está sendo preparado. Aguarde a abertura terminar.",
            )),
            FaseDoBanco::RecoveryRequired => Err(DatabaseCommandError::unavailable(
                "O banco precisa de recuperação antes de ser usado. Abra a tela de recuperação.",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_pronto_libera_acesso() {
        let estado = EstadoDoBanco::default();
        for fase in [
            FaseDoBanco::Unprepared,
            FaseDoBanco::Migrating,
            FaseDoBanco::UpgradingBlobs,
            FaseDoBanco::Adopting,
            FaseDoBanco::RecoveryRequired,
        ] {
            estado.definir(fase);
            assert!(estado.exigir_pronto().is_err(), "{fase:?} liberou acesso");
        }
        estado.definir(FaseDoBanco::Ready);
        assert!(estado.exigir_pronto().is_ok());
    }

    /// **Todo caminho de comando até o banco passa pela guarda.**
    ///
    /// A guarda só vale se estiver no ponto comum. Um acesso novo que resolva o caminho do arquivo por
    /// conta própria abriria o banco antes do upgrade seguro, e nenhum teste de comportamento
    /// perceberia sem o Tauri rodando.
    #[test]
    fn os_pontos_de_acesso_ao_banco_exigem_banco_pronto() {
        // Checkout no Windows pode trazer CRLF, e o recorte procura o fim da função por `\n}\n`.
        let interface = include_str!("../interface/tauri/mod.rs").replace("\r\n", "\n");
        let inicio = interface
            .find("pub fn database(")
            .expect("database() sumiu");
        let corpo = &interface[inicio
            ..interface[inicio..]
                .find("\n}\n")
                .map(|f| inicio + f)
                .expect("fim")];
        assert!(
            corpo.contains("exigir_pronto()"),
            "interface::tauri::database não checa o estado do banco"
        );

        let v1 = include_str!("../sync.rs").replace("\r\n", "\n");
        let inicio = v1
            .find("fn database_path(")
            .expect("database_path do V1 sumiu");
        let corpo = &v1[inicio..v1[inicio..].find("\n}\n").map(|f| inicio + f).expect("fim")];
        assert!(
            corpo.contains("exigir_pronto()"),
            "o Sync V1 abre o banco sem checar o estado"
        );

        // Quem mais resolve o arquivo do banco pelo AppHandle? Varre o `src/` inteiro: um arquivo novo
        // que abra o banco por conta própria reprova aqui, sem precisar estar numa lista.
        let permitidos: &[(&str, usize)] = &[
            ("interface/tauri/mod.rs", 1), // o ponto comum, com a guarda
            ("sync.rs", 1),                // Sync V1, com a guarda
            ("database/health.rs", 2),     // compatibility e health: só leitura, antes do upgrade
            ("database/mod.rs", 1),        // a própria definição
            // O comando que PRODUZ o `Ready` (etapa C): ele roda a conversão de mídia e a adoção
            // antes de a fase existir, então não pode pedir a chave que ele mesmo entrega. A
            // guarda não desaparece — ela é o que este comando define no fim.
            ("interface/tauri/arranque_commands.rs", 1),
        ];
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut pilha = vec![raiz.clone()];
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
                let fonte = std::fs::read_to_string(&caminho).expect("ler arquivo");
                let codigo = fonte
                    .split_once("#[cfg(test)]")
                    .map(|(c, _)| c)
                    .unwrap_or(&fonte);
                let usos = codigo.matches("app_database_path(").count();
                let relativo = caminho
                    .strip_prefix(&raiz)
                    .expect("relativo")
                    .to_string_lossy()
                    .replace('\\', "/");
                let permitido = permitidos
                    .iter()
                    .find(|(nome, _)| *nome == relativo)
                    .map(|(_, n)| *n)
                    .unwrap_or(0);
                assert_eq!(
                    usos, permitido,
                    "{relativo} abre o arquivo do banco fora do ponto comum com guarda"
                );
            }
        }
    }
}
