//! Onde um `#[tauri::command]` pode nascer.
//!
//! A Fase 3 do roadmap dizia "remover `src-tauri/src/commands/` legado". Uma revisão
//! arquitetural mostrou que essa descrição era fraca em duas direções ao mesmo tempo:
//!
//! - **alarmista demais** sobre o legado: `commands/` tinha 35 linhas, oito arquivos eram
//!   só comentários de placeholder, e o único comando registrado (`get_app_info`) não era
//!   chamado por ninguém no frontend;
//! - **frouxa demais** sobre o que importa: `database/planning.rs` continha um
//!   `#[tauri::command]` de domínio com validação, transação e SQL no mesmo arquivo — um
//!   caminho paralelo que contradizia `interface → application → domain → repository` e que
//!   um gate contra `commands/` não pegaria.
//!
//! Por isso a regra é sobre **colocação**, não sobre um diretório específico:
//!
//! > Comando de domínio só nasce em `interface/tauri`.
//!
//! A exceção é infraestrutura genuína — o que opera o dispositivo, e não o conteúdo do
//! escritor. Cada uma está listada abaixo com o motivo; uma exceção sem justificativa é uma
//! violação com permissão.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Arquivos que podem expor `#[tauri::command]` fora de `interface/tauri`.
    ///
    /// O critério é o mesmo do [ADR 0008] no frontend: **isto opera a plataforma, não o
    /// domínio.** Nenhum deles grava conteúdo do escritor por um caminho próprio.
    const INFRAESTRUTURA: &[(&str, &str)] = &[
        (
            "lib.rs",
            "pergunta se o updater está configurado neste build",
        ),
        (
            "database/health.rs",
            "diagnóstico do arquivo, em somente leitura",
        ),
        (
            "database/backup.rs",
            "cópia consistente do banco, opera arquivos",
        ),
        (
            "database/recovery.rs",
            "restauração e rollback, opera arquivos",
        ),
        (
            "database/production_replica.rs",
            "réplica de leitura para diagnóstico",
        ),
        ("local_ai.rs", "ciclo de vida do runtime de IA local"),
        ("online_share.rs", "servidor efêmero e túnel"),
        ("sync.rs", "descoberta e transporte entre dispositivos"),
    ];

    fn arquivos_com_comando(dir: &Path, raiz: &Path, achados: &mut Vec<String>) {
        let Ok(entradas) = std::fs::read_dir(dir) else {
            return;
        };
        for entrada in entradas.flatten() {
            let caminho = entrada.path();
            if caminho.is_dir() {
                arquivos_com_comando(&caminho, raiz, achados);
                continue;
            }
            if caminho.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let Ok(fonte) = std::fs::read_to_string(&caminho) else {
                continue;
            };
            // Linha que **começa** com o atributo, e não o texto solto: este próprio
            // arquivo e a documentação do serviço citam `#[tauri::command]` em comentário, e
            // a primeira versão do gate acusou os dois.
            let declara_comando = fonte
                .lines()
                .any(|linha| linha.trim_start().starts_with("#[tauri::command]"));
            if !declara_comando {
                continue;
            }
            let relativo = caminho
                .strip_prefix(raiz)
                .unwrap_or(&caminho)
                .to_string_lossy()
                .replace('\\', "/");
            achados.push(relativo);
        }
    }

    /// O gate da Fase 3.
    #[test]
    fn comando_de_dominio_so_nasce_em_interface_tauri() {
        let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut achados = Vec::new();
        arquivos_com_comando(&raiz, &raiz, &mut achados);

        assert!(
            achados.len() > 5,
            "a varredura encontrou só {} arquivos com comando; ela quebrou e este gate \
             passaria a aprovar qualquer coisa",
            achados.len()
        );

        let permitidos: Vec<&str> = INFRAESTRUTURA.iter().map(|(arquivo, _)| *arquivo).collect();
        let fora_do_lugar: Vec<&String> = achados
            .iter()
            .filter(|arquivo| !arquivo.starts_with("interface/tauri/"))
            .filter(|arquivo| !permitidos.contains(&arquivo.as_str()))
            .collect();

        assert!(
            fora_do_lugar.is_empty(),
            "comando de domínio fora de interface/tauri: {fora_do_lugar:?}.\n\n\
             Um comando que toca conteúdo do escritor precisa passar por \
             interface/tauri → application → domain → repository. Se o comando novo é \
             infraestrutura — se ele opera o dispositivo e não o conteúdo —, acrescente-o à \
             lista INFRAESTRUTURA **com o motivo**. Exceção sem justificativa é violação com \
             permissão."
        );
    }

    /// O caminho antigo não pode voltar por um atalho de nome diferente.
    #[test]
    fn o_diretorio_commands_legado_nao_volta() {
        let legado = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
        assert!(
            !legado.exists(),
            "src/commands voltou. Ele era o caminho anterior ao core de aplicação, e oito dos \
             seus dez arquivos já eram só comentários de placeholder quando foi removido."
        );
    }

    /// Toda exceção precisa de motivo escrito, e o motivo precisa dizer alguma coisa.
    #[test]
    fn toda_excecao_de_infraestrutura_tem_justificativa() {
        for (arquivo, motivo) in INFRAESTRUTURA {
            assert!(
                motivo.len() > 15,
                "a exceção de {arquivo} precisa de um motivo que explique por que ela é \
                 infraestrutura e não domínio"
            );
        }
    }
}
