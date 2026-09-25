//! Atualização do NarraHub no Android, a partir das GitHub Releases.
//!
//! ## O caminho
//!
//! ```text
//! GET /repos/OsmarZM/NarraHub/releases
//!   -> a release mais nova que ESTA instalação deve receber      (versao::deve_oferecer)
//!   -> os dois assets de nome exato: NarraHub-Android.apk e .sha256
//! baixa o .sha256, depois o APK, calculando o SHA durante o download
//!   -> não conferiu? apaga e recusa
//! antes de abrir o instalador: existe, tamanho > 0, SHA de novo
//!   -> instalador do Android, e o usuário confirma
//! ```
//!
//! ## O que este módulo recusa, e por quê
//!
//! - **Asset arbitrário.** Só os dois nomes exatos. Uma release com um `.apk` a mais, de nome
//!   parecido, não faz o app baixar o arquivo errado.
//! - **URL fora deste repositório.** O endereço do asset precisa começar com
//!   `https://github.com/OsmarZM/NarraHub/releases/download/<tag>/`. O JSON da API é dado de fora;
//!   ele não escolhe de onde o app baixa.
//! - **O frontend escolhendo URL.** A tela pede "verificar", "baixar" e "instalar", e só. A URL
//!   fica no estado do Rust, preenchida pela verificação.
//! - **Rascunho.** Release em rascunho não é release.
//!
//! O SHA-256 publicado prova que o arquivo é o que a pipeline gerou, e o Android, na instalação,
//! prova que foi assinado com a mesma chave da versão instalada. Nenhum dos dois substitui o outro:
//! o hash pega arquivo corrompido ou trocado no caminho, e a assinatura pega um APK de outra origem
//! que alguém tenha conseguido publicar com hash coerente.
//!
//! Não toca banco, BlobStore nem identidade. A atualização instala por cima, e o Android preserva
//! o diretório de dados do app.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::domain::versao::{deve_oferecer, Versao};

pub const REPOSITORIO: &str = "OsmarZM/NarraHub";
pub const ASSET_APK: &str = "NarraHub-Android.apk";
pub const ASSET_SHA: &str = "NarraHub-Android.apk.sha256";

/// Maior APK que o app aceita baixar. O de release fica bem abaixo; o teto existe contra uma
/// resposta que anuncie gigabytes e encha o armazenamento do aparelho.
pub const MAIOR_APK: u64 = 400 * 1024 * 1024;

/// A versão como o usuário lê: a tag sem o prefixo `app-v` e sem metadado de build.
///
/// A primeira versão partia a tag no primeiro `-` para achar o pré-release, e o primeiro `-` de
/// `app-v0.9.10` é o do prefixo: saía "0.9.10-v0.9.10". O teste de escolha pegou.
fn texto_da_versao(tag: &str) -> String {
    let sem_prefixo = tag
        .strip_prefix("app-v")
        .or_else(|| tag.strip_prefix('v'))
        .unwrap_or(tag);
    sem_prefixo
        .split('+')
        .next()
        .unwrap_or(sem_prefixo)
        .to_string()
}

fn prefixo_de_download(tag: &str) -> String {
    format!("https://github.com/{REPOSITORIO}/releases/download/{tag}/")
}

/// Uma versão mais nova, pronta para baixar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Novidade {
    pub versao: String,
    pub titulo: String,
    pub notas: String,
    pub tamanho_bytes: u64,
    pub pre_release: bool,
    /// Não vão para o frontend: a tela não escolhe de onde baixar.
    #[serde(skip)]
    pub url_apk: String,
    #[serde(skip)]
    pub url_sha: String,
}

/// Escolhe, entre as releases da API, a mais nova que esta instalação deve receber.
///
/// Releases sem os dois assets, com tag que não é versão, ou com URL fora do repositório são
/// ignoradas uma a uma — uma release malformada não impede de achar a certa.
pub fn escolher(
    releases: &serde_json::Value,
    instalada: &Versao,
) -> Result<Option<Novidade>, String> {
    let lista = releases
        .as_array()
        .ok_or("a resposta das releases não é uma lista")?;

    let mut melhor: Option<(Versao, Novidade)> = None;
    for release in lista {
        if release["draft"].as_bool().unwrap_or(true) {
            continue;
        }
        let Some(tag) = release["tag_name"].as_str() else {
            continue;
        };
        let Ok(versao) = Versao::ler(tag) else {
            continue;
        };
        if !deve_oferecer(instalada, &versao) {
            continue;
        }

        let assets = release["assets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let asset = |nome: &str| assets.iter().find(|a| a["name"].as_str() == Some(nome));
        let (Some(apk), Some(sha)) = (asset(ASSET_APK), asset(ASSET_SHA)) else {
            continue;
        };

        let prefixo = prefixo_de_download(tag);
        let url_apk = apk["browser_download_url"].as_str().unwrap_or_default();
        let url_sha = sha["browser_download_url"].as_str().unwrap_or_default();
        if url_apk != format!("{prefixo}{ASSET_APK}") || url_sha != format!("{prefixo}{ASSET_SHA}")
        {
            continue;
        }
        let tamanho = apk["size"].as_u64().unwrap_or(0);
        if tamanho == 0 || tamanho > MAIOR_APK {
            continue;
        }

        let novidade = Novidade {
            versao: texto_da_versao(tag),
            titulo: release["name"].as_str().unwrap_or(tag).to_string(),
            notas: release["body"].as_str().unwrap_or_default().to_string(),
            tamanho_bytes: tamanho,
            pre_release: release["prerelease"].as_bool().unwrap_or(false),
            url_apk: url_apk.to_string(),
            url_sha: url_sha.to_string(),
        };
        if melhor.as_ref().is_none_or(|(atual, _)| versao > *atual) {
            melhor = Some((versao, novidade));
        }
    }
    Ok(melhor.map(|(_, novidade)| novidade))
}

/// Lê o arquivo `.sha256` no formato do `sha256sum`: `<64 hex>  NarraHub-Android.apk`.
///
/// O nome, quando presente, precisa ser o do APK: um `.sha256` de outro arquivo, anexado por
/// engano, reprovaria todo download com uma mensagem enganosa de "arquivo corrompido".
pub fn ler_sha256(conteudo: &str) -> Result<String, String> {
    let mut partes = conteudo.split_whitespace();
    let hash = partes.next().ok_or("o arquivo de SHA-256 está vazio")?;
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("o arquivo de SHA-256 não contém um hash válido".into());
    }
    if let Some(nome) = partes.next() {
        if nome.trim_start_matches('*') != ASSET_APK {
            return Err(format!(
                "o arquivo de SHA-256 descreve \"{nome}\", não o APK"
            ));
        }
    }
    Ok(hash.to_ascii_lowercase())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 de um arquivo em disco, lido em blocos.
pub fn sha256_do_arquivo(caminho: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut arquivo =
        std::fs::File::open(caminho).map_err(|e| format!("não foi possível ler o APK: {e}"))?;
    let mut hasher = Sha256::new();
    let mut bloco = vec![0_u8; 1 << 16];
    loop {
        let lidos = arquivo
            .read(&mut bloco)
            .map_err(|e| format!("não foi possível ler o APK: {e}"))?;
        if lidos == 0 {
            break;
        }
        hasher.update(&bloco[..lidos]);
    }
    Ok(hex(&hasher.finalize()))
}

/// A última conferência antes de entregar o arquivo ao instalador.
///
/// O arquivo passou pelo SHA no download, e passa de novo aqui: entre um e outro ele ficou em
/// disco, e é o conteúdo que existe **agora** que o instalador vai ler.
pub fn conferir_antes_de_instalar(caminho: &Path, sha_esperado: &str) -> Result<u64, String> {
    let meta =
        std::fs::metadata(caminho).map_err(|_| "o APK baixado não existe mais".to_string())?;
    if !meta.is_file() || meta.len() == 0 {
        return Err("o APK baixado está vazio".into());
    }
    let calculado = sha256_do_arquivo(caminho)?;
    if calculado != sha_esperado {
        return Err("o APK baixado não confere com o SHA-256 publicado".into());
    }
    Ok(meta.len())
}

/// Um APK baixado e conferido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacoteBaixado {
    pub versao: String,
    pub caminho: PathBuf,
    pub sha256: String,
}

fn cliente(versao_instalada: &str) -> Result<reqwest::Client, String> {
    let construtor = reqwest::Client::builder()
        // A API do GitHub recusa requisição sem User-Agent.
        .user_agent(format!("NarraHub-Android/{versao_instalada}"))
        .connect_timeout(std::time::Duration::from_secs(15));
    // I-BUG-09: sem isto, a primeira requisição HTTPS no Android derruba a tarefa com
    // "Expect rustls-platform-verifier to be initialized" — e a atualização nunca aparece.
    #[cfg(target_os = "android")]
    let construtor = construtor.tls_backend_preconfigured(tls_com_raizes_embutidas()?);
    construtor
        .build()
        .map_err(|e| format!("não foi possível preparar a conexão: {e}"))
}

/// TLS com as raízes da Mozilla embutidas no binário (`webpki-roots`), em vez do verificador da
/// plataforma. No Android este é o único caminho que não depende de inicialização por JNI.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn tls_com_raizes_embutidas() -> Result<rustls::ClientConfig, String> {
    let mut raizes = rustls::RootCertStore::empty();
    raizes.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provedor = std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    Ok(rustls::ClientConfig::builder_with_provider(provedor)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("não foi possível preparar a conexão segura: {e}"))?
        .with_root_certificates(raizes)
        .with_no_client_auth())
}

/// Consulta as releases e devolve a novidade, se houver.
pub async fn verificar(versao_instalada: &str) -> Result<Option<Novidade>, String> {
    let instalada = Versao::ler(versao_instalada)?;
    let resposta = cliente(versao_instalada)?
        .get(format!(
            "https://api.github.com/repos/{REPOSITORIO}/releases?per_page=30"
        ))
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("não foi possível consultar as versões publicadas: {e}"))?;
    if !resposta.status().is_success() {
        return Err(format!(
            "o GitHub respondeu {} ao consultar as versões",
            resposta.status()
        ));
    }
    let json: serde_json::Value = resposta
        .json()
        .await
        .map_err(|e| format!("a lista de versões veio ilegível: {e}"))?;
    escolher(&json, &instalada)
}

/// Baixa o SHA e o APK, conferindo durante o download.
///
/// O arquivo nasce como `.part` e só ganha o nome final depois de conferir. Um download
/// interrompido nunca deixa para trás um arquivo com cara de pronto.
pub async fn baixar(
    novidade: &Novidade,
    versao_instalada: &str,
    pasta: &Path,
    mut progresso: impl FnMut(u64, u64),
) -> Result<PacoteBaixado, String> {
    let cliente = cliente(versao_instalada)?;

    let sha_texto = cliente
        .get(&novidade.url_sha)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("não foi possível baixar o SHA-256 da versão: {e}"))?
        .text()
        .await
        .map_err(|e| format!("o SHA-256 da versão veio ilegível: {e}"))?;
    let sha_esperado = ler_sha256(&sha_texto)?;

    std::fs::create_dir_all(pasta)
        .map_err(|e| format!("não foi possível preparar a pasta de download: {e}"))?;
    let parcial = pasta.join(format!("{ASSET_APK}.part"));
    let final_ = pasta.join(format!("NarraHub-Android-{}.apk", novidade.versao));

    let mut resposta = cliente
        .get(&novidade.url_apk)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("não foi possível baixar o APK: {e}"))?;

    let total = resposta.content_length().unwrap_or(novidade.tamanho_bytes);
    if total > MAIOR_APK {
        return Err("o APK anunciado passa do limite aceito".into());
    }

    let mut arquivo = std::fs::File::create(&parcial)
        .map_err(|e| format!("não foi possível gravar o APK: {e}"))?;
    let mut hasher = Sha256::new();
    let mut baixados: u64 = 0;
    let resultado: Result<(), String> = async {
        use std::io::Write;
        while let Some(pedaco) = resposta
            .chunk()
            .await
            .map_err(|e| format!("o download foi interrompido: {e}"))?
        {
            baixados += pedaco.len() as u64;
            if baixados > MAIOR_APK {
                return Err("o APK passou do limite aceito durante o download".into());
            }
            hasher.update(&pedaco);
            arquivo
                .write_all(&pedaco)
                .map_err(|e| format!("não foi possível gravar o APK: {e}"))?;
            progresso(baixados, total);
        }
        arquivo
            .flush()
            .map_err(|e| format!("não foi possível gravar o APK: {e}"))?;
        Ok(())
    }
    .await;
    drop(arquivo);

    let calculado = hex(&hasher.finalize());
    if let Err(erro) = resultado {
        let _ = std::fs::remove_file(&parcial);
        return Err(erro);
    }
    if baixados == 0 {
        let _ = std::fs::remove_file(&parcial);
        return Err("o APK veio vazio".into());
    }
    if calculado != sha_esperado {
        let _ = std::fs::remove_file(&parcial);
        return Err(
            "o APK baixado não confere com o SHA-256 publicado; o arquivo foi descartado".into(),
        );
    }

    std::fs::rename(&parcial, &final_)
        .map_err(|e| format!("não foi possível finalizar o APK: {e}"))?;
    Ok(PacoteBaixado {
        versao: novidade.versao.clone(),
        caminho: final_,
        sha256: sha_esperado,
    })
}

#[cfg(test)]
mod tests {
    /// **I-BUG-09 — o cliente do atualizador não depende do verificador da plataforma.** Achado em
    /// aparelho físico (Android 16): a verificação entrava em pânico com "Expect
    /// rustls-platform-verifier to be initialized". O reqwest precisa aceitar a configuração embutida
    /// — se as versões do rustls divergirem, o downcast falha e o build do cliente dá erro, que é o
    /// que este gate pega no desktop antes de chegar ao celular.
    #[test]
    fn ibug09_tls_embutido_e_aceito_pelo_reqwest() {
        let tls = super::tls_com_raizes_embutidas().expect("tls");
        let cliente = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .build();
        assert!(
            cliente.is_ok(),
            "o reqwest recusou o TLS embutido: {:?}",
            cliente.err()
        );
        assert!(
            webpki_roots::TLS_SERVER_ROOTS.len() > 100,
            "as raízes embutidas vieram vazias"
        );
    }

    use super::*;
    use serde_json::json;

    fn release(tag: &str, draft: bool, pre: bool, assets: serde_json::Value) -> serde_json::Value {
        json!({ "tag_name": tag, "name": format!("NarraHub {tag}"), "body": "notas", "draft": draft, "prerelease": pre, "assets": assets })
    }

    fn assets_de(tag: &str, tamanho: u64) -> serde_json::Value {
        let base = prefixo_de_download(tag);
        json!([
            { "name": ASSET_APK, "size": tamanho, "browser_download_url": format!("{base}{ASSET_APK}") },
            { "name": ASSET_SHA, "size": 90, "browser_download_url": format!("{base}{ASSET_SHA}") },
        ])
    }

    fn instalada(texto: &str) -> Versao {
        Versao::ler(texto).expect("versão")
    }

    /// **A mais nova oferecível, e só ela.** `0.9.10` vence `0.9.2`, rascunho não conta.
    #[test]
    fn escolhe_a_mais_nova_por_semver_e_ignora_rascunho() {
        let lista = json!([
            release("app-v0.9.2", false, false, assets_de("app-v0.9.2", 1000)),
            release("app-v0.9.10", false, false, assets_de("app-v0.9.10", 1000)),
            release("app-v0.9.3", false, false, assets_de("app-v0.9.3", 1000)),
            release("app-v0.9.99", true, false, assets_de("app-v0.9.99", 1000)),
        ]);
        let novidade = escolher(&lista, &instalada("0.9.2"))
            .expect("escolher")
            .expect("novidade");
        assert_eq!(novidade.versao, "0.9.10");
        assert_eq!(
            novidade.url_apk,
            format!("{}{ASSET_APK}", prefixo_de_download("app-v0.9.10"))
        );
    }

    #[test]
    fn nada_novo_quando_a_instalada_ja_e_a_maior() {
        let lista = json!([release(
            "app-v0.9.2",
            false,
            false,
            assets_de("app-v0.9.2", 1000)
        )]);
        assert_eq!(
            escolher(&lista, &instalada("0.9.2")).expect("escolher"),
            None
        );
    }

    /// O canal é a versão instalada: estável não recebe beta, beta recebe beta.
    #[test]
    fn estavel_nao_ve_beta_e_beta_ve() {
        let lista = json!([release(
            "app-v0.10.0-beta.2",
            false,
            true,
            assets_de("app-v0.10.0-beta.2", 1000)
        )]);
        assert_eq!(
            escolher(&lista, &instalada("0.9.2")).expect("escolher"),
            None
        );
        let novidade = escolher(&lista, &instalada("0.10.0-beta.1"))
            .expect("escolher")
            .expect("novidade");
        assert_eq!(novidade.versao, "0.10.0-beta.2");
        assert!(novidade.pre_release);
    }

    /// **Asset arbitrário e URL de fora não passam.**
    #[test]
    fn recusa_asset_de_nome_parecido_e_url_de_fora_do_repositorio() {
        let tag = "app-v0.9.3";
        let base = prefixo_de_download(tag);
        let casos = [
            // Nome parecido, sem o exato.
            json!([
                { "name": "NarraHub-Android-debug.apk", "size": 1000, "browser_download_url": format!("{base}NarraHub-Android-debug.apk") },
                { "name": ASSET_SHA, "size": 90, "browser_download_url": format!("{base}{ASSET_SHA}") },
            ]),
            // APK sem SHA.
            json!([{ "name": ASSET_APK, "size": 1000, "browser_download_url": format!("{base}{ASSET_APK}") }]),
            // URL de outro domínio.
            json!([
                { "name": ASSET_APK, "size": 1000, "browser_download_url": "https://exemplo.com/NarraHub-Android.apk" },
                { "name": ASSET_SHA, "size": 90, "browser_download_url": format!("{base}{ASSET_SHA}") },
            ]),
            // URL de outra tag do mesmo repositório.
            json!([
                { "name": ASSET_APK, "size": 1000, "browser_download_url": format!("{}{ASSET_APK}", prefixo_de_download("app-v0.1.0")) },
                { "name": ASSET_SHA, "size": 90, "browser_download_url": format!("{base}{ASSET_SHA}") },
            ]),
            // Tamanho zero e acima do teto.
            assets_de(tag, 0),
            assets_de(tag, MAIOR_APK + 1),
        ];
        for (indice, assets) in casos.into_iter().enumerate() {
            let lista = json!([release(tag, false, false, assets)]);
            assert_eq!(
                escolher(&lista, &instalada("0.9.2")).expect("escolher"),
                None,
                "o caso {indice} não podia virar novidade"
            );
        }
    }

    /// A URL escolhida não sai no JSON que vai para a tela.
    #[test]
    fn a_url_nao_vai_para_o_frontend() {
        let lista = json!([release(
            "app-v0.9.3",
            false,
            false,
            assets_de("app-v0.9.3", 1000)
        )]);
        let novidade = escolher(&lista, &instalada("0.9.2"))
            .expect("escolher")
            .expect("novidade");
        let serializado = serde_json::to_string(&novidade).expect("json");
        assert!(
            !serializado.contains("http"),
            "URL vazou para o frontend: {serializado}"
        );
    }

    #[test]
    fn le_o_formato_do_sha256sum() {
        let hash = "a".repeat(64);
        assert_eq!(
            ler_sha256(&format!("{hash}  {ASSET_APK}\n")).expect("ok"),
            hash
        );
        assert_eq!(
            ler_sha256(&format!("{hash} *{ASSET_APK}")).expect("ok"),
            hash
        );
        assert_eq!(
            ler_sha256(&hash.to_uppercase()).expect("ok"),
            hash,
            "maiúsculas viram minúsculas"
        );
        assert!(ler_sha256("").is_err());
        assert!(ler_sha256("abc  NarraHub-Android.apk").is_err());
        assert!(
            ler_sha256(&format!("{hash}  outro.apk")).is_err(),
            "SHA de outro arquivo"
        );
    }

    /// Antes de instalar: existe, não está vazio, e o SHA confere **agora**.
    #[test]
    fn a_conferencia_antes_de_instalar_pega_arquivo_trocado() {
        let pasta =
            std::env::temp_dir().join(format!("narrahub-atualiza-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&pasta).expect("pasta");
        let caminho = pasta.join("NarraHub-Android-0.9.3.apk");

        assert!(
            conferir_antes_de_instalar(&caminho, "x").is_err(),
            "arquivo inexistente"
        );

        std::fs::write(&caminho, b"").expect("vazio");
        assert!(
            conferir_antes_de_instalar(&caminho, "x").is_err(),
            "arquivo vazio"
        );

        std::fs::write(&caminho, b"apk verdadeiro").expect("gravar");
        let certo = sha256_do_arquivo(&caminho).expect("sha");
        assert_eq!(
            conferir_antes_de_instalar(&caminho, &certo).expect("confere"),
            14
        );

        std::fs::write(&caminho, b"apk trocado!!!").expect("trocar");
        assert!(
            conferir_antes_de_instalar(&caminho, &certo).is_err(),
            "arquivo trocado depois do download não pode ir ao instalador"
        );
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
