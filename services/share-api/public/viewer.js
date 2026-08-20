const loading = document.querySelector('#loading');
const errorState = document.querySelector('#error');
const documentState = document.querySelector('#document');

openShare().catch((error) => {
  loading.hidden = true;
  errorState.hidden = false;
  document.querySelector('#error-message').textContent = error instanceof Error ? error.message : 'Link inválido.';
});

async function openShare() {
  const id = location.pathname.split('/').filter(Boolean).at(-1);
  const keyValue = new URLSearchParams(location.hash.slice(1)).get('k');
  if (!id || !keyValue) throw new Error('O link não contém a chave de leitura. Solicite um novo link ao autor.');
  const response = await fetch(`/v1/shares/${encodeURIComponent(id)}`, { cache: 'no-store' });
  if (!response.ok) throw new Error(response.status === 404 ? 'Este compartilhamento expirou ou foi revogado.' : 'O servidor não respondeu corretamente.');
  const envelope = await response.json();
  if (envelope.version !== 1 || envelope.algorithm !== 'A256GCM') throw new Error('Formato de compartilhamento não suportado.');
  const key = await crypto.subtle.importKey('raw', fromBase64Url(keyValue), { name: 'AES-GCM' }, false, ['decrypt']);
  let plaintext;
  try {
    plaintext = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: fromBase64Url(envelope.iv) }, key, fromBase64Url(envelope.ciphertext));
  } catch {
    throw new Error('A chave é inválida ou o conteúdo foi alterado.');
  }
  const payload = JSON.parse(new TextDecoder().decode(plaintext));
  if (payload.version !== 1 || payload.kind !== 'chapter' || typeof payload.title !== 'string' || typeof payload.content !== 'string') {
    throw new Error('Conteúdo descriptografado inválido.');
  }
  document.querySelector('#title').textContent = payload.title || 'Capítulo sem título';
  document.querySelector('#context').textContent = [payload.universeName, payload.storyName, payload.bookName].filter(Boolean).join(' · ');
  document.querySelector('#content').textContent = payload.content;
  document.querySelector('#expiry').textContent = `Disponível até ${new Date(envelope.expiresAt).toLocaleString('pt-BR')}`;
  loading.hidden = true;
  documentState.hidden = false;
}

function fromBase64Url(value) {
  const normalized = value.replace(/-/gu, '+').replace(/_/gu, '/');
  const binary = atob(normalized + '='.repeat((4 - normalized.length % 4) % 4));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
