import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { FileShareStore, validateEnvelope } from './share-store.mjs';

const moduleDirectory = path.dirname(fileURLToPath(import.meta.url));
const publicDirectory = path.resolve(moduleDirectory, '../public');
const port = Number.parseInt(process.env.NARRAHUB_SHARE_PORT || '8787', 10);
const dataDirectory = process.env.NARRAHUB_SHARE_DATA_DIR || path.resolve(moduleDirectory, '../data');
const configuredPublicUrl = (process.env.NARRAHUB_SHARE_PUBLIC_URL || '').replace(/\/+$/u, '');
const allowedOrigins = new Set((process.env.NARRAHUB_SHARE_ALLOWED_ORIGINS || 'http://localhost:4200,http://127.0.0.1:4200,http://tauri.localhost,https://tauri.localhost,tauri://localhost')
  .split(',').map((value) => value.trim()).filter(Boolean));
const store = new FileShareStore(dataDirectory);
const rateBuckets = new Map();

if (process.env.NODE_ENV === 'production' && !configuredPublicUrl) {
  throw new Error('NARRAHUB_SHARE_PUBLIC_URL é obrigatória em produção.');
}

await store.init();

const server = createServer(async (request, response) => {
  try {
    applySecurityHeaders(request, response);
    if (request.method === 'OPTIONS') { response.writeHead(204); response.end(); return; }
    const url = new URL(request.url || '/', 'http://localhost');

    if (request.method === 'GET' && url.pathname === '/health') {
      sendJson(response, 200, { ok: true, service: 'narrahub-share', encryption: 'client-side' });
      return;
    }
    if (request.method === 'POST' && url.pathname === '/v1/shares') {
      enforceRateLimit(request);
      const body = await readJsonBody(request);
      const envelope = validateEnvelope(body);
      const contributionToken = typeof body.contributionToken === 'string' ? body.contributionToken : '';
      const created = await store.create(envelope, contributionToken);
      const publicUrl = configuredPublicUrl || inferredPublicUrl(request);
      sendJson(response, 201, {
        id: created.id,
        url: `${publicUrl}/s/${created.id}`,
        expiresAt: created.expiresAt,
        revokeToken: created.revokeToken,
      });
      return;
    }

    const shareMatch = url.pathname.match(/^\/v1\/shares\/([A-Za-z0-9_-]{16})$/u);
    if (shareMatch && request.method === 'GET') {
      const record = await store.get(shareMatch[1]);
      if (!record) { sendJson(response, 404, { error: 'Compartilhamento inexistente ou expirado.' }); return; }
      response.setHeader('cache-control', 'no-store');
      sendJson(response, 200, { ...record.envelope, expiresAt: record.expiresAt });
      return;
    }
    if (shareMatch && request.method === 'DELETE') {
      const token = (request.headers.authorization || '').replace(/^Bearer\s+/iu, '');
      const revoked = await store.revoke(shareMatch[1], token);
      if (!revoked) { sendJson(response, 403, { error: 'Token de revogação inválido.' }); return; }
      response.writeHead(204); response.end(); return;
    }

    const contributionMatch = url.pathname.match(/^\/v1\/shares\/([A-Za-z0-9_-]{16})\/contributions$/u);
    if (contributionMatch && request.method === 'GET') {
      const items = await store.listContributions(contributionMatch[1], Number.parseInt(url.searchParams.get('after') || '0', 10) || 0);
      if (!items) { sendJson(response, 404, { error: 'Compartilhamento inexistente ou expirado.' }); return; }
      sendJson(response, 200, { items }); return;
    }
    if (contributionMatch && request.method === 'POST') {
      const token = request.headers['x-narrahub-contribution-token'] || '';
      const item = await store.appendContribution(contributionMatch[1], token, await readJsonBody(request));
      if (!item) { sendJson(response, 403, { error: 'Token de colaboração inválido.' }); return; }
      sendJson(response, 201, { sequence:item.sequence, receivedAt:item.receivedAt }); return;
    }

    if (request.method === 'GET' && /^\/s\/[A-Za-z0-9_-]{16}$/u.test(url.pathname)) {
      await sendStatic(response, 'viewer.html', 'text/html; charset=utf-8'); return;
    }
    if (request.method === 'GET' && url.pathname === '/viewer.js') {
      await sendStatic(response, 'viewer.js', 'text/javascript; charset=utf-8'); return;
    }
    if (request.method === 'GET' && url.pathname === '/viewer.css') {
      await sendStatic(response, 'viewer.css', 'text/css; charset=utf-8'); return;
    }
    if (request.method === 'GET' && url.pathname === '/favicon.ico') {
      response.writeHead(204); response.end(); return;
    }
    sendJson(response, 404, { error: 'Rota não encontrada.' });
  } catch (error) {
    const status = error?.statusCode || 400;
    sendJson(response, status, { error: error instanceof Error ? error.message : 'Requisição inválida.' });
  }
});

server.listen(port, '0.0.0.0', () => {
  console.log(`[NarraHub Share] ouvindo na porta ${port}; dados cifrados em ${path.resolve(dataDirectory)}`);
});

function applySecurityHeaders(request, response) {
  const origin = request.headers.origin;
  if (origin && allowedOrigins.has(origin)) response.setHeader('access-control-allow-origin', origin);
  response.setHeader('vary', 'origin');
  response.setHeader('access-control-allow-methods', 'GET,POST,DELETE,OPTIONS');
  response.setHeader('access-control-allow-headers', 'content-type,authorization,x-narrahub-contribution-token');
  response.setHeader('x-content-type-options', 'nosniff');
  response.setHeader('referrer-policy', 'no-referrer');
  response.setHeader('permissions-policy', 'camera=(), microphone=(), geolocation=()');
  response.setHeader('content-security-policy', "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'self'; frame-ancestors 'none'");
}

function enforceRateLimit(request) {
  const key = request.socket.remoteAddress || 'unknown';
  const now = Date.now();
  const bucket = rateBuckets.get(key);
  if (!bucket || now - bucket.startedAt >= 3_600_000) {
    rateBuckets.set(key, { startedAt: now, count: 1 });
    return;
  }
  bucket.count += 1;
  if (bucket.count > 20) {
    const error = new Error('Limite de compartilhamentos por hora excedido.');
    error.statusCode = 429;
    throw error;
  }
}

async function readJsonBody(request) {
  if (!(request.headers['content-type'] || '').toLowerCase().startsWith('application/json')) {
    const error = new Error('Use content-type application/json.'); error.statusCode = 415; throw error;
  }
  const chunks = [];
  let length = 0;
  for await (const chunk of request) {
    length += chunk.length;
    if (length > 3_000_000) { const error = new Error('Corpo acima do limite.'); error.statusCode = 413; throw error; }
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks).toString('utf8'));
}

function inferredPublicUrl(request) {
  const host = request.headers.host || `localhost:${port}`;
  const forwarded = process.env.NARRAHUB_SHARE_TRUST_PROXY === '1' ? request.headers['x-forwarded-proto'] : null;
  const protocol = forwarded === 'https' ? 'https' : 'http';
  return `${protocol}://${host}`;
}

async function sendStatic(response, fileName, contentType) {
  const content = await readFile(path.join(publicDirectory, fileName));
  response.writeHead(200, { 'content-type': contentType, 'cache-control': 'public, max-age=300' });
  response.end(content);
}

function sendJson(response, status, body) {
  if (response.writableEnded) return;
  response.writeHead(status, { 'content-type': 'application/json; charset=utf-8' });
  response.end(JSON.stringify(body));
}
