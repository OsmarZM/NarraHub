# Compartilhamento online

## Objetivo

Permitir que o autor entregue uma cópia de leitura de um capítulo sem criar conta, colaboração em nuvem ou acesso remoto ao banco local.

## Fluxo

1. O autor abre um capítulo e escolhe **Compartilhar**.
2. O aplicativo serializa título, texto e contexto do capítulo.
3. Uma chave aleatória AES-256-GCM e um IV de 96 bits são criados no dispositivo.
4. Somente o envelope cifrado é enviado por HTTPS ao NarraHub Share.
5. O servidor devolve um identificador, expiração e token de revogação.
6. A chave é adicionada ao fragmento `#k=` da URL. Fragmentos não são enviados na requisição HTTP.
7. O visualizador busca o blob, descriptografa no navegador e renderiza o texto com `textContent`.

## Limites de segurança

- Qualquer pessoa com a URL completa consegue ler o conteúdo.
- O servidor não recebe a chave, mas pode observar IP, horário, tamanho do blob e identificador.
- O servidor precisa operar atrás de HTTPS em produção.
- O serviço limita criações a 20 por IP/hora e blobs cifrados a aproximadamente 2 MB.
- O conteúdo expira em 1, 7 ou 30 dias.
- Os tokens de revogação ficam somente no dispositivo do autor, em armazenamento local.
- A tela de Configurações lista os links criados nesse dispositivo e permite revogá-los; a chave de leitura não é persistida nessa lista.
- Não há edição colaborativa, conta, descoberta pública ou indexação.

## Execução

```powershell
$env:NARRAHUB_SHARE_PUBLIC_URL = 'http://localhost:8787'
npm run share-api:dev
```

Configure `http://localhost:8787` na tela de Configurações. Para produção, publique `services/share-api/Dockerfile` com volume persistente em `/data`, proxy reverso e TLS. Nesse ambiente, `NARRAHUB_SHARE_PUBLIC_URL` é obrigatória para que o serviço não construa links a partir de um cabeçalho `Host` não confiável.

## URL temporária gratuita sem conta

Para testes e compartilhamentos ocasionais, o Cloudflare Quick Tunnel cria uma URL aleatória `*.trycloudflare.com` sem conta e sem configurar DNS. Instale `cloudflared.exe` em `D:\DevTools\Cloudflared` ou no `PATH` e execute:

```powershell
npm run share-api:temporary
```

O inicializador abre o túnel em segundo plano, obtém a URL HTTPS, configura `NARRAHUB_SHARE_PUBLIC_URL` e inicia o NarraHub Share. Copie a URL exibida para **Configurações > Compartilhamento online**.

Esse modo é somente temporário: o computador precisa permanecer ligado, a URL muda ao reiniciar e o Quick Tunnel não oferece SLA. Para uma comunidade pública estável, use um domínio e hospedagem persistente.

## API

### `POST /v1/shares`

Entrada:

```json
{
  "version": 1,
  "algorithm": "A256GCM",
  "iv": "base64url",
  "ciphertext": "base64url",
  "expiresInDays": 7
}
```

O servidor nunca aceita título ou texto aberto. A resposta contém `id`, `url`, `expiresAt` e `revokeToken`.

### `GET /v1/shares/:id`

Retorna o envelope cifrado enquanto não estiver expirado.

### `DELETE /v1/shares/:id`

Exige `Authorization: Bearer <revokeToken>` e remove o compartilhamento.
