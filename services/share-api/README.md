# NarraHub Share API

Serviço opcional e autohospedável para links de leitura. A criptografia acontece no aplicativo antes do envio. O serviço grava somente `ciphertext`, IV, expiração e o hash do token de revogação.

## Desenvolvimento

```powershell
$env:NARRAHUB_SHARE_PUBLIC_URL = 'http://localhost:8787'
node services/share-api/src/server.mjs
```

No NarraHub, configure `http://localhost:8787` em **Configurações > Compartilhamento online**. HTTP é aceito apenas para `localhost`; produção exige HTTPS.

## Produção

Variáveis:

- `NARRAHUB_SHARE_PUBLIC_URL`: URL HTTPS pública, sem barra final;
- `NARRAHUB_SHARE_PORT`: porta interna, padrão `8787`;
- `NARRAHUB_SHARE_DATA_DIR`: volume persistente, padrão `/data` no container;
- `NARRAHUB_SHARE_ALLOWED_ORIGINS`: origens permitidas separadas por vírgula;
- `NARRAHUB_SHARE_TRUST_PROXY=1`: respeita `X-Forwarded-Proto` quando executado atrás de proxy confiável.

O proxy reverso deve limitar requisições, usar TLS e preservar o volume `/data`. O limite interno é 20 criações por IP/hora e cada blob cifrado pode ter aproximadamente 2 MB.

Quem possui a URL completa, incluindo o fragmento `#k=`, consegue ler o conteúdo. A chave não aparece nos logs HTTP porque fragmentos não são enviados ao servidor.
