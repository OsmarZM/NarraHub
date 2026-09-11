//! Onde os bytes de imagem moram: em arquivo endereçado pelo próprio
//! conteúdo, **fora do banco**.
//!
//! ADR 0010. O identificador de um asset é `SHA-256(bytes reais)` — não da
//! `data:` URL, não do base64, não de um caminho. A consequência prática é a
//! que a etapa 13 foi buscar:
//!
//! ```text
//! mesma imagem em 100 capítulos   →   um hash   →   um arquivo
//! ```
//!
//! ## Por baixo de `assets/`, e isso é decisão
//!
//! ```text
//! app_data/
//!   assets/
//!     blobs/
//!       sha256/
//!         ab/
//!           abcdef…                 ← publicado, nome = hash dos bytes
//!   blob-staging/
//!     <uuid>.part                   ← em construção, nunca visto como blob
//! ```
//!
//! `database/backup.rs` já varre `app_data/assets/` recursivamente, recusa
//! symlink e grava `path`/`sha256`/`size_bytes` num manifesto com hash
//! próprio. Pôr o store ali dentro o coloca nessa cobertura **sem uma linha
//! de código de backup nova**. Um store em outro lugar exigiria um segundo
//! mecanismo com um segundo manifesto para manter em dia, e dois mecanismos
//! de backup significam que um deles envelhece sem ninguém perceber.
//!
//! O staging fica **fora** de `assets/` pelo motivo simétrico: ele é varrido
//! recursivamente. Um `.part` abandonado por queda de energia entraria no
//! manifesto como arquivo real e voltaria pelo restore. Continua no mesmo
//! volume que o destino, que é o que `rename` atômico exige.
//!
//! ## O hash do chamador não é a identidade
//!
//! A identidade é derivada dos bytes, sempre:
//!
//! ```text
//! bytes  →  SHA-256  →  endereço
//! ```
//!
//! No recebimento remoto o protocolo pode **anunciar** o hash esperado, e aí
//! `put_esperando` confere. Mas o que é gravado é o que os bytes disseram, e
//! divergência descarta o temporário sem publicar nada. Aceitar o hash
//! anunciado seria deixar um peer escolher o endereço de um conteúdo: ele
//! anunciaria o hash da capa boa e mandaria outros bytes, e todo aparelho que
//! já tivesse aquele hash na referência passaria a servir o arquivo do
//! atacante como se fosse o original.

use crate::database::backup::hash_file;
use crate::database::error::{DatabaseCommandError, DatabaseCommandResult};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Onde os blobs publicados moram, relativo a `app_data`.
///
/// Em componentes, e não numa string com `/`, porque o caminho é construído
/// com `join` e a comparação com o manifesto de backup acontece em `Path`.
pub const DIRETORIO_DOS_BLOBS: [&str; 3] = ["assets", "blobs", "sha256"];

/// Onde os arquivos em construção moram. Irmão de `assets/`, não filho.
pub const DIRETORIO_DE_STAGING: &str = "blob-staging";

/// Um hash canônico tem 64 caracteres, porque `SHA-256` tem 32 bytes.
const TAMANHO_DO_HASH: usize = 64;

/// O guarda-chuva do disco.
///
/// Não tem estado além da raiz: duas instâncias apontando para o mesmo
/// `app_data` são a mesma coisa, o que é o que permite criar uma dentro de
/// cada comando sem coordenar nada.
#[derive(Debug, Clone)]
pub struct BlobStore {
    app_data: PathBuf,
}

impl BlobStore {
    pub fn new(app_data: impl Into<PathBuf>) -> Self {
        Self {
            app_data: app_data.into(),
        }
    }

    /// A raiz de `app_data`, para quem precisa falar com o backup.
    pub fn app_data(&self) -> &Path {
        &self.app_data
    }

    /// A raiz dos blobs publicados.
    pub fn raiz(&self) -> PathBuf {
        DIRETORIO_DOS_BLOBS
            .iter()
            .fold(self.app_data.clone(), |caminho, parte| caminho.join(parte))
    }

    /// O caminho de um hash, **depois** de validar o formato.
    ///
    /// A validação vem antes da construção do caminho, e não depois, porque
    /// depois não existe: o `join` com `..` já produziu o caminho de fuga. É o
    /// item 12 do contrato — um hash que chega pela rede ou de dentro de um
    /// documento Tiptap não pode virar caminho arbitrário.
    pub fn path_for(&self, hash: &str) -> DatabaseCommandResult<PathBuf> {
        let hash = validar_hash(hash)?;
        Ok(self.raiz().join(&hash[..2]).join(&hash))
    }

    /// O caminho relativo a `app_data`, como o manifesto de backup o escreve.
    pub fn caminho_relativo(&self, hash: &str) -> DatabaseCommandResult<String> {
        let hash = validar_hash(hash)?;
        Ok(format!(
            "{}/{}/{hash}",
            DIRETORIO_DOS_BLOBS.join("/"),
            &hash[..2]
        ))
    }

    /// **Presença**, não integridade.
    ///
    /// A distinção é deliberada e o contrato depende dela (item 10): no
    /// incremental uma linha pode existir com o blob ausente, e a tela mostra
    /// o asset como indisponível em vez de falhar. Quem precisa de garantia
    /// sobre os bytes chama [`BlobStore::verify`] ou [`BlobStore::read`], que
    /// releem o arquivo.
    pub fn has(&self, hash: &str) -> DatabaseCommandResult<bool> {
        Ok(self.path_for(hash)?.is_file())
    }

    /// O arquivo daquele endereço tem mesmo aqueles bytes?
    ///
    /// Blob corrompido no disco não conta como válido — é o que separa
    /// "existe um arquivo com esse nome" de "existe aquele conteúdo".
    pub fn verify(&self, hash: &str) -> DatabaseCommandResult<bool> {
        let caminho = self.path_for(hash)?;
        if !caminho.is_file() {
            return Ok(false);
        }
        let real = hash_file(&caminho).map_err(DatabaseCommandError::storage)?;
        Ok(real == hash)
    }

    /// Lê os bytes, **conferindo** que são os bytes daquele endereço.
    ///
    /// Devolver conteúdo diferente do endereço pedido é a única falha que o
    /// endereçamento por conteúdo existe para impedir, então a conferência não
    /// é opcional aqui. Custa uma passada de SHA-256 sobre um arquivo que já
    /// está sendo lido de qualquer forma.
    pub fn read(&self, hash: &str) -> DatabaseCommandResult<Vec<u8>> {
        let caminho = self.path_for(hash)?;
        let bytes = fs::read(&caminho).map_err(|error| {
            DatabaseCommandError::not_found(format!(
                "O arquivo do asset {hash} não está disponível: {error}"
            ))
        })?;
        let real = hash_dos_bytes(&bytes);
        if real != hash {
            return Err(DatabaseCommandError::storage(format!(
                "O arquivo guardado como {hash} tem conteúdo de {real}. O asset está corrompido \
                 no disco, e não vai ser servido como se estivesse íntegro."
            )));
        }
        Ok(bytes)
    }

    /// Publica bytes e devolve o endereço deles.
    ///
    /// Repetir a chamada com o mesmo conteúdo não cria duplicata: o endereço é
    /// o mesmo, e o arquivo íntegro que já está lá é aproveitado.
    pub fn put(&self, bytes: &[u8]) -> DatabaseCommandResult<String> {
        self.publicar(bytes, None)
    }

    /// Publica bytes recebidos de fora, conferindo contra o hash anunciado.
    ///
    /// Divergência descarta o temporário, não publica, e devolve erro
    /// controlado.
    pub fn put_esperando(&self, esperado: &str, bytes: &[u8]) -> DatabaseCommandResult<String> {
        self.publicar(bytes, Some(esperado))
    }

    fn publicar(&self, bytes: &[u8], esperado: Option<&str>) -> DatabaseCommandResult<String> {
        // Confere o formato do anúncio antes de escrever qualquer byte: um
        // hash malformado no protocolo não merece nem um arquivo temporário.
        let esperado = match esperado {
            Some(hash) => Some(validar_hash(hash)?),
            None => None,
        };

        if bytes.is_empty() {
            return Err(DatabaseCommandError::validation(
                "Um asset vazio não é um asset. Gravar zero byte faria a migração parecer \
                 concluída com o arquivo do escritor perdido no caminho.",
            ));
        }

        let temporario = self.escrever_temporario(bytes)?;

        // O hash sai do **disco**, não da memória. O que importa é o que ficou
        // gravado: uma escrita truncada por disco cheio produz um arquivo que
        // não é o conteúdo, e comparar contra o buffer que ainda está na RAM
        // não veria isso.
        let calculado = match hash_file(&temporario) {
            Ok(hash) => hash,
            Err(error) => {
                fs::remove_file(&temporario).ok();
                return Err(DatabaseCommandError::storage(error));
            }
        };

        if let Some(esperado) = esperado {
            if esperado != calculado {
                fs::remove_file(&temporario).ok();
                return Err(DatabaseCommandError::conflict(format!(
                    "O asset anunciado como {esperado} chegou com conteúdo de {calculado}. Nada \
                     foi publicado: o endereço de um blob é derivado dos bytes, e aceitar o hash \
                     anunciado deixaria quem manda escolher o endereço do que mandou."
                )));
            }
        }

        // Dedup: o arquivo íntegro que já está lá é o mesmo arquivo, por
        // definição. Reescrever seria trocar bytes idênticos por bytes
        // idênticos e abrir uma janela em que o blob não existe.
        if self.verify(&calculado)? {
            fs::remove_file(&temporario).ok();
            return Ok(calculado);
        }

        let destino = self.path_for(&calculado)?;
        if let Some(pai) = destino.parent() {
            if let Err(error) = fs::create_dir_all(pai) {
                fs::remove_file(&temporario).ok();
                return Err(DatabaseCommandError::storage(format!(
                    "Não foi possível preparar {}: {error}",
                    pai.display()
                )));
            }
        }

        // A publicação é um `rename`, e é por isso que ninguém nunca vê um
        // arquivo parcial com nome de hash válido. O destino pode já existir e
        // estar corrompido — o `rename` substitui, e aí o `put` conserta o
        // store em vez de recusar para sempre um asset que está em mão.
        if let Err(error) = fs::rename(&temporario, &destino) {
            fs::remove_file(&temporario).ok();
            return Err(DatabaseCommandError::storage(format!(
                "Não foi possível publicar o asset {calculado}: {error}"
            )));
        }
        Ok(calculado)
    }

    fn escrever_temporario(&self, bytes: &[u8]) -> DatabaseCommandResult<PathBuf> {
        let staging = self.app_data.join(DIRETORIO_DE_STAGING);
        fs::create_dir_all(&staging).map_err(|error| {
            DatabaseCommandError::storage(format!(
                "Não foi possível preparar {}: {error}",
                staging.display()
            ))
        })?;

        // Nome sorteado, e não derivado do hash: dois `put` do mesmo conteúdo
        // ao mesmo tempo escreveriam no mesmo temporário, e cada um veria meio
        // arquivo do outro.
        let caminho = staging.join(format!("{}.part", uuid::Uuid::new_v4()));
        let escrever = || -> std::io::Result<()> {
            let mut arquivo = fs::File::create(&caminho)?;
            arquivo.write_all(bytes)?;
            arquivo.sync_all()
        };
        if let Err(error) = escrever() {
            fs::remove_file(&caminho).ok();
            return Err(DatabaseCommandError::storage(format!(
                "Não foi possível gravar o asset em {}: {error}",
                caminho.display()
            )));
        }
        Ok(caminho)
    }
}

/// 64 hexadecimais minúsculos, e nada mais.
///
/// O minúsculo é exigência, não preferência. `AB…` e `ab…` são o mesmo hash e
/// dariam dois arquivos — ou, num sistema de arquivos que ignora caixa como o
/// do Windows, um arquivo com dois nomes na referência. As duas possibilidades
/// quebram a deduplicação que é o objetivo da etapa.
///
/// E como o alfabeto aceito é só `0-9a-f`, nenhum separador, `..`, byte nulo
/// ou letra de unidade sobrevive à validação. A defesa contra travessia de
/// caminho não é uma checagem à parte: é consequência do formato.
pub fn validar_hash(hash: &str) -> DatabaseCommandResult<String> {
    if hash.len() != TAMANHO_DO_HASH {
        return Err(DatabaseCommandError::validation(format!(
            "Referência de asset inválida: um SHA-256 canônico tem {TAMANHO_DO_HASH} caracteres, \
             e este tem {}.",
            hash.len()
        )));
    }
    if !hash
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DatabaseCommandError::validation(
            "Referência de asset inválida: só hexadecimal minúsculo é aceito. Um valor fora \
             desse alfabeto não é hash, e não vai virar caminho de arquivo.",
        ));
    }
    Ok(hash.to_string())
}

/// `true` quando a string já é uma referência canônica.
///
/// Existe para os guards de estrutura, que precisam perguntar sem tratar erro.
pub fn e_hash_canonico(hash: &str) -> bool {
    validar_hash(hash).is_ok()
}

pub fn hash_dos_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::error::DatabaseErrorKind;

    /// Vetor conhecido de `SHA-256("abc")`, para que o gate do endereço não
    /// dependa da mesma função que ele está conferindo.
    const SHA_DE_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    struct Loja {
        raiz: PathBuf,
        store: BlobStore,
    }

    impl Loja {
        fn nova() -> Self {
            let raiz = std::env::temp_dir().join(format!("narrahub-blob-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&raiz).expect("criar raiz de teste");
            Self {
                store: BlobStore::new(&raiz),
                raiz,
            }
        }

        /// Quantos arquivos existem sob os blobs publicados, recursivamente.
        fn arquivos_publicados(&self) -> Vec<PathBuf> {
            let mut encontrados = Vec::new();
            varrer(&self.store.raiz(), &mut encontrados);
            encontrados
        }

        fn arquivos_em_staging(&self) -> Vec<PathBuf> {
            let mut encontrados = Vec::new();
            varrer(&self.raiz.join(DIRETORIO_DE_STAGING), &mut encontrados);
            encontrados
        }
    }

    impl Drop for Loja {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.raiz).ok();
        }
    }

    fn varrer(diretorio: &Path, encontrados: &mut Vec<PathBuf>) {
        let Ok(filhos) = fs::read_dir(diretorio) else {
            return;
        };
        for filho in filhos.flatten() {
            let caminho = filho.path();
            if caminho.is_dir() {
                varrer(&caminho, encontrados);
            } else {
                encontrados.push(caminho);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // A identidade é dos bytes
    // ═══════════════════════════════════════════════════════════════════════

    /// **O endereço é `SHA-256` dos bytes reais.**
    ///
    /// Conferido contra um vetor conhecido, e não contra `hash_dos_bytes` — um
    /// gate que usa a mesma função que está testando concorda com qualquer
    /// erro que ela tenha.
    #[test]
    fn o_endereco_e_o_sha256_dos_bytes_reais() {
        let loja = Loja::nova();
        let hash = loja.store.put(b"abc").expect("publicar");
        assert_eq!(hash, SHA_DE_ABC);
        assert!(e_hash_canonico(&hash));
    }

    /// **O hash não é da representação textual.**
    ///
    /// O mesmo PNG dentro de duas `data:` URLs diferentes — MIME declarado
    /// diferente, quebra de linha diferente no base64 — precisa dar o mesmo
    /// endereço. Se o hash fosse da string, a deduplicação prometida pela
    /// etapa desapareceria sem ninguém perceber: a mesma capa gravada por dois
    /// caminhos do aplicativo viraria dois arquivos.
    #[test]
    fn o_hash_nao_e_da_representacao_textual() {
        let bytes = b"\x89PNG\r\n\x1a\n-conteudo-de-imagem";
        let uma = format!(
            "data:image/png;base64,{}",
            "aGFzaC1kYS1zdHJpbmc=" // o base64 nem precisa bater: o ponto é a moldura
        );
        let outra = format!("data:image/x-png;base64,\n{}\n", "aGFzaC1kYS1zdHJpbmc=");

        let dos_bytes = hash_dos_bytes(bytes);
        assert_ne!(
            dos_bytes,
            hash_dos_bytes(uma.as_bytes()),
            "o endereço não pode sair da string"
        );
        assert_ne!(dos_bytes, hash_dos_bytes(outra.as_bytes()));
        assert_ne!(
            hash_dos_bytes(uma.as_bytes()),
            hash_dos_bytes(outra.as_bytes()),
            "as duas molduras são strings diferentes — é justamente por isso que a moldura \
             não pode participar do endereço"
        );
    }

    /// **A mesma imagem em dois lugares é um arquivo só.**
    ///
    /// Este é o objetivo da etapa, e o gate mede o disco, não o valor de
    /// retorno: dois `put` do mesmo conteúdo, um arquivo publicado.
    #[test]
    fn a_mesma_imagem_publicada_duas_vezes_e_um_arquivo_so() {
        let loja = Loja::nova();
        let bytes = b"capa-do-livro-em-bytes";

        let primeiro = loja.store.put(bytes).expect("primeira publicação");
        let segundo = loja.store.put(bytes).expect("segunda publicação");

        assert_eq!(primeiro, segundo);
        assert_eq!(
            loja.arquivos_publicados().len(),
            1,
            "conteúdo igual não pode virar dois arquivos"
        );
    }

    /// E a deduplicação não sabe de qual superfície os bytes vieram.
    ///
    /// É o que permite a mesma imagem servir de capa de universo, capa de
    /// livro e node de um capítulo sem triplicar o disco. O store nem tem por
    /// onde saber a origem — e é essa ignorância que faz a dedup atravessar as
    /// dez superfícies de graça.
    #[test]
    fn a_deduplicacao_atravessa_superficies() {
        let loja = Loja::nova();
        let imagem = b"a-mesma-arte-usada-em-tudo";

        let da_capa_do_universo = loja.store.put(imagem).expect("capa de universo");
        let da_capa_do_livro = loja.store.put(imagem).expect("capa de livro");
        let de_dentro_do_capitulo = loja.store.put(imagem).expect("node do Tiptap");

        assert_eq!(da_capa_do_universo, da_capa_do_livro);
        assert_eq!(da_capa_do_livro, de_dentro_do_capitulo);
        assert_eq!(loja.arquivos_publicados().len(), 1);
    }

    /// Conteúdos diferentes não colidem no mesmo endereço.
    #[test]
    fn conteudos_diferentes_moram_em_enderecos_diferentes() {
        let loja = Loja::nova();
        let uma = loja.store.put(b"imagem-a").expect("publicar a");
        let outra = loja.store.put(b"imagem-b").expect("publicar b");
        assert_ne!(uma, outra);
        assert_eq!(loja.arquivos_publicados().len(), 2);
    }

    /// Os bytes voltam exatamente como entraram.
    #[test]
    fn read_devolve_exatamente_os_bytes_publicados() {
        let loja = Loja::nova();
        let bytes: Vec<u8> = (0..=255u8).collect();
        let hash = loja.store.put(&bytes).expect("publicar");
        assert_eq!(loja.store.read(&hash).expect("ler"), bytes);
    }

    /// Asset vazio é recusado.
    ///
    /// Zero byte não é um arquivo do escritor. Aceitar faria a migração de uma
    /// superfície parecer concluída com o asset perdido no caminho — e o
    /// inline seria limpo em seguida, porque há hash válido.
    #[test]
    fn asset_vazio_e_recusado() {
        let loja = Loja::nova();
        let erro = loja.store.put(b"").expect_err("zero byte não é asset");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);
        assert!(loja.arquivos_publicados().is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Temporário e publicação atômica
    // ═══════════════════════════════════════════════════════════════════════

    /// **Nenhum `.part` sobrevive a uma publicação bem-sucedida.**
    #[test]
    fn a_publicacao_nao_deixa_temporario_para_tras() {
        let loja = Loja::nova();
        loja.store.put(b"imagem").expect("publicar");
        assert!(
            loja.arquivos_em_staging().is_empty(),
            "sobrou temporário: {:?}",
            loja.arquivos_em_staging()
        );
    }

    /// **O temporário não mora dentro de `assets/`.**
    ///
    /// Gate estrutural, e não estético. O backup varre `assets/`
    /// recursivamente: um `.part` abandonado por queda de energia entraria no
    /// manifesto como arquivo real, com hash e tamanho próprios, e voltaria
    /// pelo restore. O staging tem que ficar irmão de `assets/`, no mesmo
    /// volume — o mesmo volume é o que `rename` atômico exige.
    #[test]
    fn o_temporario_fica_fora_do_que_o_backup_varre() {
        let loja = Loja::nova();
        let staging = loja.raiz.join(DIRETORIO_DE_STAGING);
        let assets = loja.raiz.join(DIRETORIO_DOS_BLOBS[0]);

        assert!(
            !staging.starts_with(&assets),
            "{} está sob {}, e o backup varreria o arquivo parcial",
            staging.display(),
            assets.display()
        );
        assert_eq!(
            staging.parent(),
            assets.parent(),
            "irmãos, para que o rename continue no mesmo volume"
        );
    }

    /// Publicação atômica: só existe arquivo com nome de hash depois de
    /// pronto, e o nome é sempre o hash — nunca um `.part`.
    #[test]
    fn o_arquivo_publicado_tem_o_nome_do_hash_e_nada_mais() {
        let loja = Loja::nova();
        let hash = loja.store.put(b"imagem").expect("publicar");

        let publicados = loja.arquivos_publicados();
        assert_eq!(publicados.len(), 1);
        let nome = publicados[0]
            .file_name()
            .expect("nome")
            .to_string_lossy()
            .to_string();
        assert_eq!(nome, hash);
        assert_eq!(publicados[0], loja.store.path_for(&hash).expect("caminho"));
    }

    /// O endereço devolvido nunca é um caminho.
    ///
    /// Item 4 do contrato: caminho absoluto gravado quebra na restauração
    /// noutra máquina. O que o chamador recebe para gravar no banco tem que
    /// ser só o hash.
    #[test]
    fn o_que_vai_para_o_banco_e_hash_e_nao_caminho() {
        let loja = Loja::nova();
        let hash = loja.store.put(b"imagem").expect("publicar");
        for proibido in ['/', '\\', ':', '.'] {
            assert!(
                !hash.contains(proibido),
                "o endereço {hash} carrega {proibido}, e viraria caminho persistido"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Integridade: presença não é integridade
    // ═══════════════════════════════════════════════════════════════════════

    /// **Blob corrompido no disco não conta como válido.**
    ///
    /// E o gate registra a distinção deliberada do contrato: `has` responde
    /// presença, `verify` responde integridade, e `read` recusa servir bytes
    /// que não são os do endereço pedido. Sem essa separação, a tela mostraria
    /// a imagem errada como se fosse a certa.
    #[test]
    fn blob_corrompido_no_disco_nao_conta_como_valido() {
        let loja = Loja::nova();
        let hash = loja.store.put(b"imagem-original").expect("publicar");
        let caminho = loja.store.path_for(&hash).expect("caminho");
        fs::write(&caminho, b"outra-coisa-qualquer").expect("corromper");

        assert!(
            loja.store.has(&hash).expect("presença"),
            "o arquivo está lá"
        );
        assert!(
            !loja.store.verify(&hash).expect("integridade"),
            "mas o conteúdo não é o daquele endereço"
        );
        let erro = loja.store.read(&hash).expect_err("não serve corrompido");
        assert_eq!(erro.kind, DatabaseErrorKind::Storage);
    }

    /// Blob ausente: `verify` responde `false` em vez de estourar.
    ///
    /// Item 10 do contrato — referência e presença física são estados
    /// distintos. No incremental a linha chega antes do arquivo, e isso é
    /// normal, não erro.
    #[test]
    fn blob_ausente_nao_e_erro_para_has_e_verify() {
        let loja = Loja::nova();
        let hash = hash_dos_bytes(b"nunca-publicado");
        assert!(!loja.store.has(&hash).expect("presença"));
        assert!(!loja.store.verify(&hash).expect("integridade"));
        assert_eq!(
            loja.store.read(&hash).expect_err("ler o ausente").kind,
            DatabaseErrorKind::NotFound
        );
    }

    /// `put` conserta o store quando os bytes certos estão em mão.
    ///
    /// Recusar para sempre um asset cujo conteúdo correto está no argumento
    /// seria transformar corrupção de disco em perda permanente.
    #[test]
    fn put_conserta_blob_corrompido_quando_os_bytes_estao_em_mao() {
        let loja = Loja::nova();
        let bytes = b"imagem-original";
        let hash = loja.store.put(bytes).expect("publicar");
        fs::write(loja.store.path_for(&hash).expect("caminho"), b"corrompido").expect("corromper");

        let de_novo = loja.store.put(bytes).expect("republicar");
        assert_eq!(de_novo, hash);
        assert!(loja.store.verify(&hash).expect("integridade"));
        assert_eq!(loja.arquivos_publicados().len(), 1);
        assert!(loja.arquivos_em_staging().is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // O hash do chamador não é a identidade
    // ═══════════════════════════════════════════════════════════════════════

    /// **Hash anunciado que não bate com os bytes não publica nada.**
    ///
    /// O caso que isso fecha não é acidente de rede — é um peer anunciando o
    /// hash da capa boa e mandando outros bytes. Todo aparelho que já tivesse
    /// aquele hash na referência passaria a servir o arquivo dele como se
    /// fosse o original, e o endereçamento por conteúdo teria virado
    /// endereçamento por afirmação.
    #[test]
    fn hash_anunciado_que_nao_bate_com_os_bytes_e_recusado() {
        let loja = Loja::nova();
        let prometido = hash_dos_bytes(b"a-capa-boa");

        let erro = loja
            .store
            .put_esperando(&prometido, b"outros-bytes-quaisquer")
            .expect_err("os bytes não são os anunciados");

        assert_eq!(erro.kind, DatabaseErrorKind::Conflict);
        assert!(
            loja.arquivos_publicados().is_empty(),
            "nada podia ter sido publicado"
        );
        assert!(
            loja.arquivos_em_staging().is_empty(),
            "o temporário tinha que ser descartado"
        );
        assert!(!loja.store.has(&prometido).expect("presença"));
        assert!(
            !loja
                .store
                .has(&hash_dos_bytes(b"outros-bytes-quaisquer"))
                .expect("presença"),
            "nem sob o endereço verdadeiro dos bytes recebidos: a transferência foi recusada"
        );
    }

    /// E aceita quando bate, devolvendo o mesmo endereço.
    #[test]
    fn hash_anunciado_que_bate_publica_no_endereco_anunciado() {
        let loja = Loja::nova();
        let bytes = b"a-capa-boa";
        let prometido = hash_dos_bytes(bytes);

        let publicado = loja
            .store
            .put_esperando(&prometido, bytes)
            .expect("os bytes são os anunciados");

        assert_eq!(publicado, prometido);
        assert!(loja.store.verify(&prometido).expect("integridade"));
        assert!(loja.arquivos_em_staging().is_empty());
    }

    /// Hash anunciado malformado nem chega a criar temporário.
    #[test]
    fn hash_anunciado_malformado_nao_escreve_nada() {
        let loja = Loja::nova();
        let erro = loja
            .store
            .put_esperando("nao-sou-um-hash", b"bytes")
            .expect_err("formato inválido");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);
        assert!(loja.arquivos_em_staging().is_empty());
        assert!(!loja.raiz.join(DIRETORIO_DE_STAGING).exists());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Hash recebido nunca vira caminho
    // ═══════════════════════════════════════════════════════════════════════

    /// **Travessia de caminho é impossível por construção do formato.**
    ///
    /// Os candidatos aqui têm o tamanho certo de propósito: rejeitar só pelo
    /// comprimento deixaria passar uma travessia bem-medida. O que fecha é o
    /// alfabeto — `0-9a-f` não contém separador, ponto, dois-pontos nem nulo,
    /// então nenhuma dessas strings sobrevive à validação para virar `join`.
    #[test]
    fn hash_recebido_nunca_vira_caminho_arbitrario() {
        let loja = Loja::nova();
        let candidatos = [
            "../".repeat(21) + "e",                            // 64 caracteres de fuga
            "c:/".to_string() + &"a".repeat(61),               // caminho absoluto do Windows
            "..\\".repeat(21) + "e",                           // fuga com separador do Windows
            ".".repeat(64),                                    // só pontos
            "a".repeat(63) + "/",                              // um separador no fim
            "a/".to_string() + &"b".repeat(62),                // um separador no começo
            format!("{}\0{}", "a".repeat(31), "b".repeat(32)), // byte nulo no meio
        ];

        for candidato in candidatos {
            assert_eq!(candidato.len(), 64, "o cenário precisa ter o tamanho certo");
            let erro = loja
                .store
                .path_for(&candidato)
                .expect_err("isto não é um hash");
            assert_eq!(erro.kind, DatabaseErrorKind::Validation);
            assert!(loja.store.has(&candidato).is_err());
            assert!(loja.store.read(&candidato).is_err());
            assert!(loja.store.verify(&candidato).is_err());
        }
    }

    /// O caminho de um hash válido fica sob a raiz dos blobs, sempre.
    #[test]
    fn o_caminho_de_um_hash_valido_nao_sai_da_raiz() {
        let loja = Loja::nova();
        let hash = hash_dos_bytes(b"qualquer");
        let caminho = loja.store.path_for(&hash).expect("caminho");
        assert!(caminho.starts_with(loja.store.raiz()));
        assert_eq!(
            caminho,
            loja.store.raiz().join(&hash[..2]).join(&hash),
            "dois níveis: prefixo de dois caracteres e o hash inteiro"
        );
    }

    /// **Hash em maiúsculas é recusado.**
    ///
    /// Não é preferência de estilo. `AB…` e `ab…` são o mesmo hash: num
    /// sistema de arquivos que ignora caixa, como o do Windows, seriam um
    /// arquivo com dois nomes na referência; num que distingue, dois arquivos
    /// com o mesmo conteúdo. As duas coisas quebram a deduplicação que é o
    /// objetivo da etapa.
    #[test]
    fn hash_em_maiusculas_e_recusado() {
        let loja = Loja::nova();
        let bytes = b"imagem";
        let minusculo = loja.store.put(bytes).expect("publicar");
        let maiusculo = minusculo.to_ascii_uppercase();
        assert_ne!(minusculo, maiusculo, "o cenário precisa mudar a caixa");

        let erro = loja
            .store
            .path_for(&maiusculo)
            .expect_err("só minúsculo é canônico");
        assert_eq!(erro.kind, DatabaseErrorKind::Validation);
        assert!(!e_hash_canonico(&maiusculo));
        assert!(loja.store.verify(&maiusculo).is_err());
    }

    /// Tamanho errado é recusado, um caractere a mais ou a menos.
    #[test]
    fn hash_com_tamanho_errado_e_recusado() {
        let loja = Loja::nova();
        for tamanho in [0, 1, 32, 63, 65, 128] {
            let candidato = "a".repeat(tamanho);
            let erro = loja
                .store
                .path_for(&candidato)
                .expect_err("tamanho fora do canônico");
            assert_eq!(erro.kind, DatabaseErrorKind::Validation);
            assert!(
                erro.message.contains("64"),
                "a mensagem precisa dizer qual é o formato: {}",
                erro.message
            );
        }
    }

    /// O caminho relativo é o que o manifesto de backup escreve.
    ///
    /// O manifesto guarda o caminho relativo a `app_data` com `/`, e os gates
    /// de backup vão comparar contra esta função. Derivá-la do mesmo `join`
    /// que o `path_for` usa é o que impede as duas visões de divergirem.
    #[test]
    fn o_caminho_relativo_bate_com_o_absoluto() {
        let loja = Loja::nova();
        let hash = loja.store.put(b"imagem").expect("publicar");

        let relativo = loja.store.caminho_relativo(&hash).expect("relativo");
        assert_eq!(
            relativo,
            format!("assets/blobs/sha256/{}/{hash}", &hash[..2])
        );

        let absoluto = loja.store.path_for(&hash).expect("absoluto");
        let derivado = absoluto
            .strip_prefix(&loja.raiz)
            .expect("sob app_data")
            .components()
            .map(|parte| parte.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(derivado, relativo);
    }

    /// Dois `BlobStore` sobre o mesmo `app_data` são a mesma loja.
    ///
    /// É o que permite criar uma instância dentro de cada comando sem
    /// coordenar nada — e o que faz a dedup valer entre superfícies que não se
    /// conhecem.
    #[test]
    fn duas_instancias_sobre_a_mesma_raiz_veem_os_mesmos_blobs() {
        let loja = Loja::nova();
        let hash = loja.store.put(b"imagem").expect("publicar");

        let outra = BlobStore::new(&loja.raiz);
        assert!(outra.has(&hash).expect("presença"));
        assert!(outra.verify(&hash).expect("integridade"));
        assert_eq!(outra.read(&hash).expect("ler"), b"imagem");
    }
}
