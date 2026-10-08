//! Escrita durável e troca de arquivo que funcionam no Windows e no Android.
//!
//! ## Durabilidade
//!
//! `fs::write` + `rename` torna o **nome** atômico, mas não garante que os bytes e a entrada de
//! diretório sobrevivam a uma queda de energia: o sistema pode confirmar o `rename` e perder o
//! conteúdo que ainda estava no cache. Por isso:
//!
//! ```text
//! sincronizar_arquivo     File::sync_all  (fsync / FlushFileBuffers) nos bytes
//! sincronizar_diretorio   fsync no diretório, para a entrada criada/renomeada persistir
//! ```
//!
//! No Windows não há como abrir diretório para `FlushFileBuffers` pela std; o NTFS registra a
//! alteração de nome no próprio journal de metadados, e o `sync_all` do arquivo é o que se consegue
//! garantir. No Android (ext4/f2fs) e em qualquer Unix o diretório é sincronizado.
//!
//! ## Troca de arquivo
//!
//! [`substituir_arquivo`] não depende de o `rename` do sistema aceitar destino existente. A
//! sequência é explícita, e cada passo que falha desfaz os anteriores:
//!
//! ```text
//! 1 sync do arquivo novo
//! 2 destino atual → "<destino>.substituindo-<id>"     falhou? nada mudou (ex.: SQLite ainda aberto)
//! 3 arquivo novo  → destino                           falhou? o antigo volta ao nome
//! 4 sync do destino e do diretório; remove o antigo
//! ```
//!
//! O passo 2 é o que pega o caso real do Windows: o SQLite abre o banco **sem**
//! `FILE_SHARE_DELETE`, e com uma conexão viva o arquivo não pode ser renomeado. A troca falha antes
//! de tocar em qualquer coisa, em vez de deixar o banco pela metade.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub fn sincronizar_arquivo(caminho: &Path) -> io::Result<()> {
    File::options()
        .read(true)
        .write(true)
        .open(caminho)?
        .sync_all()
}

#[cfg(unix)]
pub fn sincronizar_diretorio(diretorio: &Path) -> io::Result<()> {
    File::open(diretorio)?.sync_all()
}

#[cfg(not(unix))]
pub fn sincronizar_diretorio(_diretorio: &Path) -> io::Result<()> {
    Ok(())
}

/// Grava bytes num arquivo de forma atômica **e** durável: temporário sincronizado, `rename`,
/// diretório sincronizado.
pub fn gravar_atomico(destino: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporario = irmao(destino, "gravando");
    fs::write(&temporario, bytes)?;
    sincronizar_arquivo(&temporario)?;
    if let Err(erro) = substituir_arquivo(&temporario, destino) {
        let _ = fs::remove_file(&temporario);
        return Err(erro);
    }
    Ok(())
}

/// Remove um arquivo e torna a remoção durável.
pub fn remover_duravel(caminho: &Path) -> io::Result<()> {
    match fs::remove_file(caminho) {
        Ok(()) => {}
        Err(erro) if erro.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(erro) => return Err(erro),
    }
    if let Some(pai) = caminho.parent() {
        sincronizar_diretorio(pai)?;
    }
    Ok(())
}

/// Coloca `novo` no lugar de `destino`, exista ou não um `destino`.
pub fn substituir_arquivo(novo: &Path, destino: &Path) -> io::Result<()> {
    sincronizar_arquivo(novo)?;

    let afastado = if destino.exists() {
        let afastado = irmao(destino, "substituindo");
        // Falhar aqui não mudou nada: o destino continua sendo o antigo.
        fs::rename(destino, &afastado)?;
        Some(afastado)
    } else {
        None
    };

    if let Err(erro) = fs::rename(novo, destino) {
        if let Some(afastado) = &afastado {
            // Devolve o antigo. Se até isso falhar, o antigo continua inteiro em `afastado`, e o
            // erro diz onde.
            if let Err(volta) = fs::rename(afastado, destino) {
                return Err(io::Error::new(
                    volta.kind(),
                    format!(
                        "a troca falhou ({erro}) e o arquivo anterior não pôde voltar ao nome ({volta}); ele está em {}",
                        afastado.display()
                    ),
                ));
            }
        }
        return Err(erro);
    }

    sincronizar_arquivo(destino)?;
    if let Some(pai) = destino.parent() {
        sincronizar_diretorio(pai)?;
    }
    if let Some(afastado) = afastado {
        let _ = fs::remove_file(afastado);
    }
    Ok(())
}

fn irmao(caminho: &Path, sufixo: &str) -> PathBuf {
    let nome = caminho
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    caminho.with_file_name(format!(
        "{nome}.{sufixo}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Pasta(PathBuf);
    impl Pasta {
        fn nova() -> Self {
            let p = std::env::temp_dir().join(format!("narrahub-duravel-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&p).expect("pasta");
            Self(p)
        }
        fn restos(&self) -> Vec<String> {
            fs::read_dir(&self.0)
                .expect("ler")
                .map(|e| {
                    e.expect("entrada")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        }
    }
    impl Drop for Pasta {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn substitui_destino_que_ja_existe() {
        let pasta = Pasta::nova();
        let destino = pasta.0.join("narrahub.db");
        let novo = pasta.0.join("narrahub.db.restaurando");
        fs::write(&destino, b"banco migrado").expect("destino");
        fs::write(&novo, b"banco original").expect("novo");

        substituir_arquivo(&novo, &destino).expect("substituir");

        assert_eq!(fs::read(&destino).expect("ler"), b"banco original");
        assert!(!novo.exists());
        assert_eq!(
            pasta.restos(),
            vec!["narrahub.db".to_string()],
            "sobrou arquivo afastado"
        );
    }

    #[test]
    fn coloca_no_lugar_quando_nao_existe_destino() {
        let pasta = Pasta::nova();
        let destino = pasta.0.join("a.json");
        let novo = pasta.0.join("a.json.tmp");
        fs::write(&novo, b"{}").expect("novo");
        substituir_arquivo(&novo, &destino).expect("substituir");
        assert_eq!(fs::read(&destino).expect("ler"), b"{}");
    }

    /// O caso do Windows: o SQLite segura o arquivo aberto sem `FILE_SHARE_DELETE`. A troca tem que
    /// falhar sem tocar no banco — e funcionar depois que a conexão fecha.
    #[test]
    fn destino_aberto_pelo_sqlite_falha_sem_perder_nada_e_funciona_depois_de_fechar() {
        let pasta = Pasta::nova();
        let destino = pasta.0.join("narrahub.db");
        let novo = pasta.0.join("narrahub.db.restaurando");
        {
            let c = rusqlite::Connection::open(&destino).expect("criar");
            c.execute_batch("CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('migrado');")
                .expect("dados");
        }
        fs::copy(&destino, &novo).expect("copiar");
        rusqlite::Connection::open(&novo)
            .expect("abrir novo")
            .execute("UPDATE t SET v = 'original'", [])
            .expect("marcar");

        let aberta = rusqlite::Connection::open(&destino).expect("segurar aberto");
        aberta
            .query_row("SELECT v FROM t", [], |r| r.get::<_, String>(0))
            .expect("usar");
        let resultado = substituir_arquivo(&novo, &destino);

        if cfg!(windows) {
            assert!(
                resultado.is_err(),
                "no Windows o SQLite aberto impede a troca"
            );
            assert!(
                novo.exists(),
                "o arquivo novo continua disponível para tentar de novo"
            );
            let v: String = aberta
                .query_row("SELECT v FROM t", [], |r| r.get(0))
                .expect("ler");
            assert_eq!(v, "migrado", "o banco em uso não foi tocado");
            drop(aberta);
            substituir_arquivo(&novo, &destino).expect("depois de fechar, a troca funciona");
        } else {
            // Em Unix o rename de arquivo aberto é permitido; o conteúdo novo precisa estar lá.
            resultado.expect("substituir");
            drop(aberta);
        }
        let v: String = rusqlite::Connection::open(&destino)
            .expect("reabrir")
            .query_row("SELECT v FROM t", [], |r| r.get(0))
            .expect("ler");
        assert_eq!(v, "original");
        assert_eq!(pasta.restos(), vec!["narrahub.db".to_string()]);
    }

    #[test]
    fn gravacao_atomica_substitui_e_nao_deixa_temporario() {
        let pasta = Pasta::nova();
        let destino = pasta.0.join("marcador.json");
        gravar_atomico(&destino, b"um").expect("primeira");
        gravar_atomico(&destino, b"dois").expect("segunda");
        assert_eq!(fs::read(&destino).expect("ler"), b"dois");
        assert_eq!(pasta.restos(), vec!["marcador.json".to_string()]);
        remover_duravel(&destino).expect("remover");
        remover_duravel(&destino).expect("remover de novo é ok");
        assert!(pasta.restos().is_empty());
    }
}
