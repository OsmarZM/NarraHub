# NarraHub 0.7.1

Esta versão corrige a separação entre o ambiente de desenvolvimento e o aplicativo instalado, além de reforçar a troca de tema no desktop.

## Correções

- O modo de desenvolvimento agora usa `com.narrahub.app.dev` e um banco independente.
- O aplicativo instalado e as releases continuam usando `com.narrahub.app`.
- A validação de release impede que os dois perfis voltem a compartilhar o mesmo diretório.
- A janela Tauri recebeu a permissão necessária para sincronizar o tema nativo.
- Claro, escuro e sistema atualizam imediatamente a interface, persistem a preferência e informam o estado aos recursos de acessibilidade.

## Para quem está na versão 0.6.1

Se o NarraHub 0.6.1 foi executado depois de testes locais com uma versão mais nova, ele pode encerrar ao encontrar migrações futuras no banco. Nesse caso, o atualizador interno não consegue abrir.

Instale manualmente a versão 0.7.1 sobre a instalação atual. Não apague `%APPDATA%\com.narrahub.app\narrahub.db`, porque esse arquivo contém seus universos e histórias.

## Desenvolvimento

Ao iniciar por `.\iniciar-desktop.bat` ou `npm run desktop:dev`, o NarraHub usa `%APPDATA%\com.narrahub.app.dev`. Esse perfil começa vazio e pode receber migrações sem alterar o banco instalado.

Para testar com dados próximos da produção, copie o banco com o aplicativo fechado. Nunca aponte o modo de desenvolvimento diretamente para o arquivo de produção.

## Validação

- Alternância claro/escuro e persistência verificadas em navegador real.
- Inicialização Tauri confirmada com banco V1–V9 no diretório de desenvolvimento.
- Banco de produção comparado antes e depois por tamanho, data e SHA-256, sem alteração.
- Build Angular, 4 testes da API de compartilhamento e 10 testes Rust aprovados.
- Configuração de release verificada com identidade de produção e updater assinado.

Consulte também [Desenvolvimento seguro](https://github.com/OsmarZM/NarraHub/blob/app-v0.7.1/docs/DEVELOPMENT.md) e o [histórico de versões](https://github.com/OsmarZM/NarraHub/blob/app-v0.7.1/CHANGELOG.md).
