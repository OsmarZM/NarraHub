import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';
import { mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';

const SHARE_ID_PATTERN = /^[A-Za-z0-9_-]{16}$/u;
const BASE64_URL_PATTERN = /^[A-Za-z0-9_-]+$/u;
const MAX_CIPHERTEXT_LENGTH = 2_800_000;

export function validateEnvelope(value) {
  if (!value || typeof value !== 'object') throw new Error('Envelope inválido.');
  if (value.version !== 1 || value.algorithm !== 'A256GCM') throw new Error('Versão ou algoritmo não suportado.');
  if (typeof value.iv !== 'string' || value.iv.length !== 16 || !BASE64_URL_PATTERN.test(value.iv)) {
    throw new Error('IV inválido.');
  }
  if (typeof value.ciphertext !== 'string' || value.ciphertext.length < 24 || value.ciphertext.length > MAX_CIPHERTEXT_LENGTH || !BASE64_URL_PATTERN.test(value.ciphertext)) {
    throw new Error('Conteúdo cifrado inválido ou acima do limite.');
  }
  if (![1, 7, 30].includes(value.expiresInDays)) throw new Error('Expiração inválida.');
  return {
    version: 1,
    algorithm: 'A256GCM',
    iv: value.iv,
    ciphertext: value.ciphertext,
    expiresInDays: value.expiresInDays,
  };
}

export class FileShareStore {
  constructor(directory) {
    this.directory = path.resolve(directory);
  }

  async init() {
    await mkdir(this.directory, { recursive: true });
    await this.pruneExpired();
  }

  async create(envelope) {
    const normalized = validateEnvelope(envelope);
    const id = randomBytes(12).toString('base64url');
    const revokeToken = randomBytes(24).toString('base64url');
    const createdAt = new Date();
    const expiresAt = new Date(createdAt.getTime() + normalized.expiresInDays * 86_400_000);
    const record = {
      id,
      createdAt: createdAt.toISOString(),
      expiresAt: expiresAt.toISOString(),
      revokeHash: hashToken(revokeToken),
      envelope: {
        version: normalized.version,
        algorithm: normalized.algorithm,
        iv: normalized.iv,
        ciphertext: normalized.ciphertext,
      },
    };
    const target = this.recordPath(id);
    const temporary = `${target}.${randomBytes(6).toString('hex')}.tmp`;
    await writeFile(temporary, JSON.stringify(record), { encoding: 'utf8', flag: 'wx' });
    await rename(temporary, target);
    return { id, revokeToken, createdAt: record.createdAt, expiresAt: record.expiresAt };
  }

  async get(id) {
    this.assertId(id);
    try {
      const record = JSON.parse(await readFile(this.recordPath(id), 'utf8'));
      if (Date.parse(record.expiresAt) <= Date.now()) {
        await rm(this.recordPath(id), { force: true });
        return null;
      }
      return record;
    } catch (error) {
      if (error?.code === 'ENOENT' || error instanceof SyntaxError) return null;
      throw error;
    }
  }

  async revoke(id, token) {
    const record = await this.get(id);
    if (!record || typeof token !== 'string') return false;
    const expected = Buffer.from(record.revokeHash, 'hex');
    const received = Buffer.from(hashToken(token), 'hex');
    if (expected.length !== received.length || !timingSafeEqual(expected, received)) return false;
    await rm(this.recordPath(id), { force: true });
    return true;
  }

  async pruneExpired() {
    let names = [];
    try { names = await readdir(this.directory); } catch { return; }
    await Promise.all(names.filter((name) => name.endsWith('.json')).map(async (name) => {
      const id = name.slice(0, -5);
      if (!SHARE_ID_PATTERN.test(id)) return;
      await this.get(id);
    }));
  }

  recordPath(id) {
    this.assertId(id);
    return path.join(this.directory, `${id}.json`);
  }

  assertId(id) {
    if (!SHARE_ID_PATTERN.test(id)) throw new Error('Identificador inválido.');
  }
}

function hashToken(value) {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}
