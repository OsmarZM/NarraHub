const loading = document.querySelector('#loading');
const errorState = document.querySelector('#error');
const documentState = document.querySelector('#document');
const content = document.querySelector('#content');

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
  if (!response.ok) throw new Error(response.status === 404 ? 'Este compartilhamento foi encerrado, expirou ou não existe.' : 'O computador do autor não respondeu corretamente.');
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
  if (payload.version === 1 && payload.kind === 'chapter') renderLegacyChapter(payload);
  else if (payload.version === 2 && payload.kind === 'bundle') renderBundle(payload);
  else throw new Error('Conteúdo descriptografado inválido.');
  document.querySelector('#expiry').textContent = `Disponível no máximo até ${new Date(envelope.expiresAt).toLocaleString('pt-BR')}`;
  loading.hidden = true;
  documentState.hidden = false;
}

function renderLegacyChapter(payload) {
  document.querySelector('#title').textContent = payload.title || 'Capítulo sem título';
  document.querySelector('#context').textContent = [payload.universeName, payload.storyName, payload.bookName].filter(Boolean).join(' · ');
  content.append(renderRichText(payload.content));
}

function renderBundle(payload) {
  if (!Array.isArray(payload.chapters) || !Array.isArray(payload.entities)) throw new Error('Seleção compartilhada inválida.');
  document.querySelector('#title').textContent = payload.title || payload.universe?.name || 'Seleção NarraHub';
  document.querySelector('#context').textContent = `${payload.chapters.length} capítulo(s) · ${payload.entities.length} ficha(s)`;

  if (payload.universe) {
    const hero = element('section', 'shared-universe');
    if (safeDataImage(payload.universe.coverImage)) {
      const image = element('img'); image.src = payload.universe.coverImage; image.alt = '';
      hero.append(image);
    }
    const copy = element('div');
    copy.append(element('small', '', 'UNIVERSO'), element('h2', '', string(payload.universe.name)), element('p', '', string(payload.universe.description) || 'Sem descrição.'));
    hero.append(copy); content.append(hero);
  }

  if (payload.chapters.length) {
    content.append(sectionTitle('Capítulos'));
    for (const chapter of payload.chapters) {
      const section = element('section', 'shared-chapter');
      section.append(element('small', '', [chapter.storyName, chapter.bookName].filter(Boolean).join(' / ')), element('h2', '', string(chapter.title) || 'Capítulo sem título'), renderRichText(chapter.content));
      content.append(section);
    }
  }

  if (payload.entities.length) {
    content.append(sectionTitle('Fichas do universo'));
    const grid = element('div', 'entity-grid');
    for (const entity of payload.entities) grid.append(renderEntity(entity));
    content.append(grid);
  }
}

function renderEntity(entity) {
  const card = element('section', 'entity-card');
  const heading = element('div', 'entity-heading');
  if (safeDataImage(entity.image)) {
    const image = element('img'); image.src = entity.image; image.alt = '';
    heading.append(image);
  } else heading.append(element('span', 'entity-initial', string(entity.name).charAt(0).toUpperCase() || '?'));
  const copy = element('div'); copy.append(element('small', '', string(entity.type)), element('h3', '', string(entity.name) || 'Sem nome'));
  heading.append(copy); card.append(heading, element('p', '', string(entity.description) || 'Sem descrição.'));
  if (Array.isArray(entity.attributes) && entity.attributes.length) {
    const attributes = element('dl');
    for (const attribute of entity.attributes.slice(0, 40)) attributes.append(element('dt', '', string(attribute.key)), element('dd', '', string(attribute.value)));
    card.append(attributes);
  }
  return card;
}

function renderRichText(value) {
  const output = element('div', 'rich-text');
  const parsed = new DOMParser().parseFromString(string(value), 'text/html');
  const allowed = new Set(['P', 'BR', 'BLOCKQUOTE', 'H1', 'H2', 'H3', 'H4', 'UL', 'OL', 'LI', 'STRONG', 'B', 'EM', 'I', 'U', 'S', 'HR']);
  const copy = (node, parent) => {
    if (node.nodeType === Node.TEXT_NODE) { parent.append(document.createTextNode(node.textContent || '')); return; }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    if (!allowed.has(node.tagName)) { for (const child of node.childNodes) copy(child, parent); return; }
    const clean = document.createElement(node.tagName.toLowerCase());
    parent.append(clean);
    for (const child of node.childNodes) copy(child, clean);
  };
  for (const node of parsed.body.childNodes) copy(node, output);
  if (!output.textContent?.trim() && string(value).trim()) output.textContent = string(value);
  return output;
}

function sectionTitle(title) { const heading = element('h2', 'section-title', title); return heading; }
function element(tag, className = '', text = '') { const node = document.createElement(tag); if (className) node.className = className; if (text) node.textContent = text; return node; }
function string(value) { return typeof value === 'string' ? value.slice(0, 2_000_000) : ''; }
function safeDataImage(value) { return typeof value === 'string' && /^data:image\/(?:png|jpe?g|webp|gif);base64,[A-Za-z0-9+/=]+$/u.test(value); }

function fromBase64Url(value) {
  const normalized = value.replace(/-/gu, '+').replace(/_/gu, '/');
  const binary = atob(normalized + '='.repeat((4 - normalized.length % 4) % 4));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
