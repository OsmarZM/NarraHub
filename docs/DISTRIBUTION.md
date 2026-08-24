# Distribuição

## Windows

O build oficial usa Rust MSVC e gera NSIS e MSI. A validação de uma release exige:

1. build Angular de produção;
2. `cargo check` e `cargo test`;
3. geração dos instaladores;
4. instalação em uma pasta limpa;
5. smoke test do executável instalado;
6. criação, edição, fechamento e reabertura de um capítulo;
7. teste dos controles da janela;
8. conferência de que nenhum dado foi criado automaticamente.

Assinatura de código deve ser adicionada antes de distribuição pública.

## Updater Windows

O pipeline `.github/workflows/release-windows.yml` publica a release e os artefatos assinados do updater. As chaves ficam nos Secrets/Variables do GitHub e nunca no repositório. Consulte [Atualizações](UPDATES.md).

Publicar código, executar o workflow, validar o instalador e publicar a release são quatro estados diferentes. O aplicativo instalado consulta somente a última release publicada.

## Android

O build Android deve gerar APK para teste interno e AAB para publicação. Antes de release:

- testar em pelo menos um aparelho físico;
- validar permissões de rede local nas versões Android suportadas;
- testar layout compacto, teclado e rotação;
- validar sincronização com Windows na mesma rede;
- configurar chave de assinatura fora do repositório;
- revisar política de backup e armazenamento.

Para teste interno ARM64, use `$env:CARGO_BUILD_JOBS = '1'; npm run android:apk`. A saída debug usa a chave de desenvolvimento do Android e pode ser instalada diretamente. `npm run android:release:apk` gera um APK de release sem assinatura; a chave de produção e suas senhas devem permanecer fora do repositório.

Em workspaces no disco D com o registro Cargo no C, mantenha `kotlin.incremental=false` no `gradle.properties` gerado. Essa configuração evita a falha `this and base files have different roots` do compilador incremental Kotlin. O build completo é mais previsível, embora o primeiro ciclo seja mais lento.

Se o linker Rust retornar `LNK1102` ou `out of memory`, reduza `CARGO_BUILD_JOBS` para `1`. Não remova plugins ou desative recursos para mascarar falta de memória.

Git push, geração de instalador e publicação em loja são entregas distintas e devem ser confirmadas separadamente.
