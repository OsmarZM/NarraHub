export type NativeCommandErrorKind = 'validation' | 'not_found' | 'conflict' | 'storage' | 'unavailable';

export interface NativeCommandErrorPayload {
  kind: NativeCommandErrorKind;
  message: string;
}

export class NativeCommandError extends Error {
  readonly kind: NativeCommandErrorKind;

  constructor(kind: NativeCommandErrorKind, message: string) {
    super(message);
    this.name = 'NativeCommandError';
    this.kind = kind;
  }
}

export function normalizeNativeCommandError(error: unknown, fallback: string): NativeCommandError {
  if (isNativeCommandErrorPayload(error)) return new NativeCommandError(error.kind, error.message);
  if (error instanceof Error) return new NativeCommandError('unavailable', error.message || fallback);
  if (typeof error === 'string' && error.trim()) return new NativeCommandError('unavailable', error);
  return new NativeCommandError('unavailable', fallback);
}

function isNativeCommandErrorPayload(error: unknown): error is NativeCommandErrorPayload {
  if (!error || typeof error !== 'object') return false;
  const candidate = error as Partial<NativeCommandErrorPayload>;
  return typeof candidate.message === 'string'
    && ['validation', 'not_found', 'conflict', 'storage', 'unavailable'].includes(candidate.kind || '');
}
