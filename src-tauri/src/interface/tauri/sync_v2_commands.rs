//! A primeira porta do Sync V2 para o frontend (etapa 14, fatia 2).
//!
//! ## O que existia antes disto
//!
//! Nada. O `invoke_handler` registrava 108 comandos e nenhum era do V2 — treze
//! etapas de motor sem um único chamador de produto. O que o usuário alcançava
//! ao sincronizar era o **V1** (`src-tauri/src/sync.rs`), que copia 17 tabelas
//! inteiras sem passar pelo log de eventos.
//!
//! ## As duas regras desta fronteira
//!
//! ```text
//! 1  `device_id` NUNCA e parametro de entrada
//! 2  nenhum caminho de arquivo atravessa
//! ```
//!
//! A primeira é a invariante das etapas 2.5 e 8 chegando à fronteira: quem
//! responde "que aparelho é este" é a identidade Ed25519 em arquivo, via
//! [`super::sync_identity`]. Um `device_id` recebido de fora seria um `&str`
//! escolhido pelo chamador, e o roster autoriza por identidade — não por
//! afirmação. A etapa 8 existe inteira por causa disso.
//!
//! A segunda é a mesma do `blob_commands`: o mesmo acervo tem que funcionar no
//! Windows e no Android, e caminho local é exatamente o que não viaja.
//!
//! ## O V1 não é tocado
//!
//! Decisão registrada: congelar e substituir, **sem coexistir**. Estes comandos
//! não chamam nada do V1, não leem o estado dele e não constroem
//! interoperabilidade. Um acervo com parte das escritas vindas do snapshot do
//! V1 e parte da causalidade do V2 teria estado cuja origem o V2 não consegue
//! explicar.

use crate::application::sync_panorama::{panorama, Panorama};
use crate::database::error::DatabaseCommandResult;
use tauri::AppHandle;

/// O estado do Sync V2 neste aparelho.
///
/// Só leitura. A escuta e o pareamento entram na fatia 3, onde existe
/// transporte para elas conversarem — uma escuta que aceita conexão e fecha
/// seria porta pintada na parede.
///
/// Sem parâmetro nenhum, e isso é o contrato: não há o que um chamador possa
/// afirmar sobre quem ele é.
#[tauri::command]
pub fn sync_v2_panorama(app: AppHandle) -> DatabaseCommandResult<Panorama> {
    panorama(&super::database(&app)?, &super::sync_identity(&app)?)
}

#[cfg(test)]
mod tests {
    /// **Nenhum comando desta fronteira recebe `device_id`.**
    ///
    /// Gate textual, e é o tipo de gate que se justifica: a propriedade é
    /// sobre a *assinatura* dos comandos, e um teste de comportamento não
    /// alcança um parâmetro que não deveria existir. Ele falha no momento em
    /// que alguém acrescentar `device_id: String` a qualquer comando daqui —
    /// que é exatamente o atalho tentador quando a tela "já sabe" o id.
    ///
    /// O que a tela sabe é o que este módulo contou a ela. Deixar isso voltar
    /// como entrada transformaria identidade em parâmetro, e a etapa 8 existe
    /// para que não seja.
    #[test]
    fn nenhum_comando_recebe_device_id_como_parametro() {
        let fonte = include_str!("sync_v2_commands.rs");

        // Recorta só o que está antes do módulo de teste: as strings deste
        // próprio gate contêm "device_id" e passariam por ocorrência real.
        let codigo = fonte
            .split_once("#[cfg(test)]")
            .map(|(antes, _)| antes)
            .expect("o módulo de teste tinha que existir");

        assert!(
            codigo.contains("#[tauri::command]"),
            "a varredura não achou comando nenhum; o gate perdeu o alvo"
        );

        for proibido in [
            "device_id:",
            "deviceId:",
            "device_id :",
            "identity:",
            "public_key:",
        ] {
            assert!(
                !codigo.contains(proibido),
                "algum comando passou a receber `{proibido}` de fora. Identidade não é \
                 parâmetro: quem responde quem este aparelho é são a chave Ed25519 em \
                 arquivo e o roster, nunca o chamador."
            );
        }
    }

    /// E nenhum caminho de arquivo atravessa.
    ///
    /// Mesma regra do `blob_commands`, pelo mesmo motivo: caminho local não
    /// viaja entre Windows e Android.
    #[test]
    fn nenhum_comando_recebe_nem_devolve_caminho() {
        let fonte = include_str!("sync_v2_commands.rs");
        let codigo = fonte
            .split_once("#[cfg(test)]")
            .map(|(antes, _)| antes)
            .expect("o módulo de teste tinha que existir");

        for proibido in ["PathBuf", "path:", "caminho:", "&Path"] {
            assert!(
                !codigo.contains(proibido),
                "`{proibido}` apareceu na fronteira. O mesmo acervo tem que funcionar nos \
                 dois sistemas, e caminho local é o que não viaja."
            );
        }
    }

    /// **Esta fronteira não fala com o Sync V1.**
    ///
    /// A decisão é congelar e substituir, sem coexistência. O gate existe
    /// porque o atalho — "aproveita o `SyncState` que já está lá" — é o que
    /// produziria um acervo escrito pelos dois mecanismos, com estado que o V2
    /// não consegue explicar.
    #[test]
    fn a_fronteira_do_v2_nao_toca_no_v1() {
        let fonte = include_str!("sync_v2_commands.rs");
        let codigo = fonte
            .split_once("#[cfg(test)]")
            .map(|(antes, _)| antes)
            .expect("o módulo de teste tinha que existir");

        for proibido in ["crate::sync::", "SyncState", "sync_start", "sync_connect"] {
            assert!(
                !codigo.contains(proibido),
                "`{proibido}` é do Sync V1. O V1 está congelado: nada novo se apoia nele, e \
                 não existe interoperabilidade V1 <-> V2."
            );
        }
    }
}
