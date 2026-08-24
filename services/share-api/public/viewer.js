const loading = document.querySelector('#loading');
const errorState = document.querySelector('#error');
const workspace = document.querySelector('#workspace');
const main = document.querySelector('#main');
const universeNav = document.querySelector('#universe-nav');
const sectionNav = document.querySelector('#section-nav');
const conversation = document.querySelector('#conversation');
const conversationFeed = document.querySelector('#conversation-feed');
const contributorInput = document.querySelector('#contributor-name');
const entityDialog = document.querySelector('#entity-dialog');
const editorDialog = document.querySelector('#editor-dialog');

const state = { id:'', key:null, payload:null, universe:null, section:'overview', chapter:null, contributions:[], lastSequence:0 };
const sectionItems = [
  { id:'overview', icon:'⌂', label:'Visão geral' },
  { id:'chapters', icon:'≡', label:'Livros e capítulos' },
  { id:'entities', icon:'♧', label:'Fichas do universo' },
];

contributorInput.value = localStorage.getItem('narrahub.contributorName') || '';
contributorInput.addEventListener('change', () => localStorage.setItem('narrahub.contributorName', contributorInput.value.trim()));
document.querySelector('#conversation-button').addEventListener('click', () => conversation.classList.toggle('open'));
document.querySelector('#conversation-close').addEventListener('click', () => conversation.classList.remove('open'));
entityDialog.addEventListener('click', (event) => { if (event.target === entityDialog) entityDialog.close(); });
editorDialog.addEventListener('click', (event) => { if (event.target === editorDialog) editorDialog.close(); });

openShare().catch((error) => {
  loading.hidden = true; errorState.hidden = false;
  document.querySelector('#error-message').textContent = error instanceof Error ? error.message : 'Link inválido.';
});

async function openShare() {
  state.id = location.pathname.split('/').filter(Boolean).at(-1) || '';
  const keyValue = new URLSearchParams(location.hash.slice(1)).get('k');
  if (!state.id || !keyValue) throw new Error('O link não contém a chave da sessão. Solicite um novo link ao autor.');
  const response = await fetch(`/v1/shares/${encodeURIComponent(state.id)}`, { cache:'no-store' });
  if (!response.ok) throw new Error(response.status === 404 ? 'Esta sessão foi encerrada, expirou ou não existe.' : 'O computador do autor não respondeu corretamente.');
  const envelope = await response.json(); validateEnvelope(envelope);
  state.key = await crypto.subtle.importKey('raw', fromBase64Url(keyValue), { name:'AES-GCM' }, false, ['encrypt','decrypt']);
  const payload = await decrypt(envelope);
  if (payload.version === 3 && payload.kind === 'workspace') renderWorkspace(payload);
  else if (payload.version === 2 && payload.kind === 'bundle') renderLegacyBundle(payload);
  else if (payload.version === 1 && payload.kind === 'chapter') renderLegacyChapter(payload);
  else throw new Error('Conteúdo descriptografado inválido.');
  document.querySelector('#expiry').textContent = `Disponível até ${new Date(envelope.expiresAt).toLocaleString('pt-BR')}`;
  loading.hidden = true; workspace.hidden = false;
}

function renderWorkspace(payload) {
  if (!Array.isArray(payload.universes) || !payload.universes.length) throw new Error('A sessão não possui universos válidos.');
  state.payload = payload; state.universe = payload.universes[0];
  document.querySelector('#title').textContent = string(payload.title) || 'Sessão NarraHub';
  document.querySelector('#permission').textContent = permissionLabel(payload.permission);
  renderNavigation(); renderMain();
  if (payload.permission !== 'view') { void pollContributions(); window.setInterval(() => void pollContributions(), 2500); }
}

function renderNavigation() {
  replaceChildren(universeNav);
  for (const universe of state.payload.universes) {
    const button = element('button'); button.type = 'button'; button.classList.toggle('active', universe.id === state.universe.id);
    const avatar = element('span', '', string(universe.name).charAt(0).toUpperCase());
    if (safeDataImage(universe.coverImage)) { avatar.style.backgroundImage = `url(${universe.coverImage})`; avatar.textContent = ''; }
    const copy = element('span'); copy.append(element('b', '', string(universe.name)), element('small', '', `${array(universe.chapters).length} capítulos · ${array(universe.entities).length} fichas`));
    button.append(avatar, copy); button.addEventListener('click', () => { state.universe = universe; state.section = 'overview'; state.chapter = null; renderNavigation(); renderMain(); }); universeNav.append(button);
  }
  replaceChildren(sectionNav);
  for (const item of sectionItems) {
    if (item.id === 'chapters' && !array(state.universe.chapters).length) continue;
    if (item.id === 'entities' && !array(state.universe.entities).length) continue;
    const button = element('button'); button.type = 'button'; button.classList.toggle('active', state.section === item.id && !state.chapter);
    button.append(element('span', '', item.icon), element('b', '', item.label));
    button.addEventListener('click', () => { state.section = item.id; state.chapter = null; renderNavigation(); renderMain(); }); sectionNav.append(button);
  }
}

function renderMain() {
  replaceChildren(main);
  if (state.chapter) return renderChapter(state.chapter);
  if (state.section === 'chapters') return renderChapterList();
  if (state.section === 'entities') return renderEntityGrid();
  renderOverview();
}

function renderOverview() {
  const universe = state.universe; const hero = element('section', 'hero');
  if (safeDataImage(universe.coverImage)) { const image = element('img'); image.src = universe.coverImage; image.alt = ''; hero.append(image); }
  const copy = element('div', 'hero-copy'); copy.append(element('p', 'eyebrow', 'UNIVERSO'), element('h2', '', string(universe.name)), element('p', '', string(universe.description) || 'Sem descrição.'));
  copy.append(actionRow(universeTarget(), true)); hero.append(copy); main.append(hero);
  const stats = element('div', 'page-head'); const text = element('div'); text.append(element('p', 'eyebrow', 'CONTEÚDO COMPARTILHADO'), element('h1', '', 'Explore este universo'), element('p', '', `${array(universe.chapters).length} capítulo(s) e ${array(universe.entities).length} ficha(s) disponíveis nesta sessão.`)); stats.append(text); main.append(stats);
}

function renderChapterList() {
  main.append(pageHead('BIBLIOTECA', 'Livros e capítulos', 'Abra um capítulo para ler, anotar ou propor uma revisão sem alterar diretamente o texto do autor.'));
  const list = element('div', 'list');
  for (const chapter of array(state.universe.chapters)) {
    const button = element('button', 'list-item'); button.type = 'button'; const copy = element('span', 'list-copy');
    copy.append(element('b', '', string(chapter.title) || 'Capítulo sem título'), element('small', '', [chapter.storyName, chapter.bookName].filter(Boolean).join(' / ')));
    button.append(element('span', 'list-icon', '≡'), copy, element('span', 'list-meta', `${wordCount(chapter.content)} palavras`));
    button.addEventListener('click', () => { state.chapter = chapter; renderNavigation(); renderMain(); }); list.append(button);
  }
  main.append(list);
}

function renderChapter(chapter) {
  const article = element('article', 'chapter-page'); const back = element('button', 'secondary', '← Voltar aos capítulos'); back.type = 'button';
  back.addEventListener('click', () => { state.chapter = null; renderNavigation(); renderMain(); });
  article.append(back, element('p', 'chapter-context', [chapter.storyName, chapter.bookName].filter(Boolean).join(' / ')), element('h1', '', string(chapter.title) || 'Capítulo sem título'));
  if (string(chapter.summary)) article.append(element('p', 'chapter-summary', chapter.summary));
  article.append(renderRichText(chapter.content), actionRow(chapterTarget(chapter), true)); main.append(article);
}

function renderEntityGrid() {
  main.append(pageHead('FICHAS', 'Personagens, lugares e elementos', 'As fichas abrem em uma janela dedicada para preservar espaço e legibilidade.'));
  const grid = element('div', 'entity-grid');
  for (const entity of array(state.universe.entities)) {
    const card = element('button', 'entity-card'); card.type = 'button'; const copy = element('div');
    copy.append(element('small', '', string(entity.type)), element('b', '', string(entity.name) || 'Sem nome'), element('p', '', string(entity.summary) || string(entity.description) || 'Sem resumo.'));
    card.append(entityAvatar(entity), copy); card.addEventListener('click', () => openEntity(entity)); grid.append(card);
  }
  main.append(grid);
}

function openEntity(entity) {
  const root = document.querySelector('#entity-dialog-content'); replaceChildren(root); const content = element('div', 'sheet-content');
  content.append(sheetHead(string(entity.type).toUpperCase(), string(entity.name) || 'Sem nome', entityDialog));
  const profile = element('div', 'entity-profile'); const copy = element('div'); copy.append(element('p', 'entity-description', string(entity.summary) || string(entity.description) || 'Sem descrição.'), actionRow(entityTarget(entity), true)); profile.append(entityAvatar(entity), copy); content.append(profile);
  if (array(entity.attributes).length) { const attributes = element('div', 'attributes'); for (const attribute of array(entity.attributes)) { const item = element('div', 'attribute'); item.append(element('small', '', string(attribute.key)), element('p', '', string(attribute.value))); attributes.append(item); } content.append(attributes); }
  root.append(content); entityDialog.showModal();
}

function actionRow(target, includeEdit) {
  const row = element('div', 'action-row');
  if (state.payload.permission !== 'view') { const note = element('button', 'secondary', '＋ Anotar nesta seção'); note.type = 'button'; note.addEventListener('click', () => openNoteComposer(target)); row.append(note); }
  if (includeEdit && state.payload.permission === 'edit') { const edit = element('button', 'primary', 'Propor edição'); edit.type = 'button'; edit.addEventListener('click', () => openEditor(target)); row.append(edit); }
  return row;
}

function openNoteComposer(target) {
  const root = document.querySelector('#editor-dialog-content'); replaceChildren(root); const content = element('div', 'sheet-content'); content.append(sheetHead('ANOTAÇÃO', target.label, editorDialog));
  const form = element('form', 'edit-form'); const label = element('label', '', 'Anotação para o autor'); const textarea = element('textarea'); textarea.maxLength = 12000; textarea.placeholder = 'Escreva sua observação, dúvida ou sugestão sobre esta seção…'; label.append(textarea); form.append(label);
  const actions = sheetActions(editorDialog, 'Enviar anotação'); form.append(actions.row); form.addEventListener('submit', async (event) => { event.preventDefault(); if (!textarea.value.trim()) return; actions.submit.disabled = true; await submitContribution(target, 'note', { message:textarea.value.trim() }); editorDialog.close(); conversation.classList.add('open'); }); content.append(form); root.append(content); editorDialog.showModal(); textarea.focus();
}

function openEditor(target) {
  const root = document.querySelector('#editor-dialog-content'); replaceChildren(root); const content = element('div', 'sheet-content'); content.append(sheetHead('PROPOSTA DE EDIÇÃO', target.label, editorDialog));
  const form = element('form', 'edit-form'); const controls = [];
  for (const fieldData of editableFields(target)) {
    const label = element('label', '', fieldData.label); let input;
    if (fieldData.rich) { input = element('div', 'rich-editor'); input.contentEditable = 'true'; input.append(...renderRichText(fieldData.value).childNodes); }
    else { input = fieldData.multiline ? element('textarea') : element('input'); input.value = fieldData.value; if (fieldData.multiline) input.rows = 5; }
    label.append(input); form.append(label); controls.push({ ...fieldData, input });
  }
  const actions = sheetActions(editorDialog, 'Enviar proposta'); form.append(actions.row);
  form.addEventListener('submit', async (event) => {
    event.preventDefault(); actions.submit.disabled = true; let sent = 0;
    for (const control of controls) { const value = control.rich ? serializeRichEditor(control.input) : control.input.value; if (value !== control.value) { await submitContribution(target, 'edit', { field:control.field, originalValue:control.value, proposedValue:value }); control.apply(value); sent += 1; } }
    if (!sent) { actions.submit.disabled = false; actions.submit.textContent = 'Nenhuma mudança'; return; }
    editorDialog.close(); if (target.type === 'entity' && entityDialog.open) entityDialog.close(); renderMain(); conversation.classList.add('open');
  }); content.append(form); root.append(content); editorDialog.showModal();
}

function editableFields(target) {
  if (target.type === 'universe') return [field('name','Nome',target.data.name,false,false,(value)=>{target.data.name=value;renderNavigation();}),field('description','Descrição',target.data.description,true,false,(value)=>{target.data.description=value;})];
  if (target.type === 'chapter') return [field('title','Título',target.data.title,false,false,(value)=>{target.data.title=value;}),field('summary','Resumo',target.data.summary,true,false,(value)=>{target.data.summary=value;}),field('content','Texto do capítulo',target.data.content,true,true,(value)=>{target.data.content=value;})];
  const fields = [field('name','Nome',target.data.name,false,false,(value)=>{target.data.name=value;}),field('summary','Resumo',target.data.summary,true,false,(value)=>{target.data.summary=value;}),field('description','Descrição',target.data.description,true,false,(value)=>{target.data.description=value;})];
  for (const attribute of array(target.data.attributes)) fields.push(field(`attribute:${attribute.key}`,attribute.key,attribute.value,true,false,(value)=>{attribute.value=value;})); return fields;
}

function field(fieldName,label,value,multiline,rich,apply) { return { field:fieldName,label,value:string(value),multiline,rich,apply }; }
function universeTarget() { return { type:'universe',id:state.universe.id,label:state.universe.name,universeId:state.universe.id,data:state.universe }; }
function chapterTarget(chapter) { return { type:'chapter',id:chapter.id,label:chapter.title,universeId:state.universe.id,data:chapter }; }
function entityTarget(entity) { return { type:'entity',id:entity.id,label:entity.name,universeId:state.universe.id,data:entity }; }

async function submitContribution(target, contributionKind, values) {
  if (!state.payload.collaborationToken) throw new Error('Esta sessão não aceita contribuições.');
  const payload = { version:1,kind:'contribution',id:crypto.randomUUID(),contributor:contributorInput.value.trim() || 'Convidado',contributionKind,universeId:target.universeId,targetType:target.type,targetId:target.id,targetLabel:target.label,createdAt:new Date().toISOString(),...values };
  const envelope = await encrypt(payload); const response = await fetch(`/v1/shares/${encodeURIComponent(state.id)}/contributions`, { method:'POST',headers:{'Content-Type':'application/json','X-NarraHub-Contribution-Token':state.payload.collaborationToken},body:JSON.stringify(envelope) });
  if (!response.ok) { const error = await response.json().catch(() => ({})); throw new Error(error.error || 'Não foi possível enviar a contribuição.'); } await pollContributions();
}

async function pollContributions() {
  try { const response = await fetch(`/v1/shares/${encodeURIComponent(state.id)}/contributions?after=${state.lastSequence}`, { cache:'no-store' }); if (!response.ok) return; const data = await response.json();
    for (const item of array(data.items)) { state.lastSequence = Math.max(state.lastSequence,Number(item.sequence)||0); try { const payload = await decrypt(item.envelope); if (payload.kind === 'contribution') state.contributions.push(payload); } catch { /* item corrompido */ } } renderConversation();
  } catch { /* sessão encerrada entre ciclos */ }
}

function renderConversation() { replaceChildren(conversationFeed); const notes = state.contributions.filter((item)=>item.contributionKind==='note'); document.querySelector('#conversation-count').textContent=String(notes.length); for (const note of notes.slice().reverse()) { const card=element('article','message'); const head=element('header'); head.append(element('b','',string(note.contributor)||'Convidado'),element('small','',string(note.targetLabel))); card.append(head,element('p','',string(note.message))); conversationFeed.append(card); } if (!notes.length) conversationFeed.append(element('div','empty','Ainda não há anotações nesta sessão.')); }
async function encrypt(payload) { const iv=crypto.getRandomValues(new Uint8Array(12)); const plaintext=new TextEncoder().encode(JSON.stringify(payload)); const ciphertext=await crypto.subtle.encrypt({name:'AES-GCM',iv},state.key,plaintext); return {version:1,algorithm:'A256GCM',iv:toBase64Url(iv),ciphertext:toBase64Url(new Uint8Array(ciphertext))}; }
async function decrypt(envelope) { validateEnvelope(envelope); try { const plaintext=await crypto.subtle.decrypt({name:'AES-GCM',iv:fromBase64Url(envelope.iv)},state.key,fromBase64Url(envelope.ciphertext)); return JSON.parse(new TextDecoder().decode(plaintext)); } catch { throw new Error('A chave é inválida ou o conteúdo foi alterado.'); } }
function validateEnvelope(envelope) { if (envelope?.version!==1 || envelope?.algorithm!=='A256GCM') throw new Error('Formato de compartilhamento não suportado.'); }
function renderLegacyBundle(payload) { state.payload={version:3,kind:'workspace',title:payload.title,permission:'view',universes:[{id:'legacy',name:payload.universe?.name||payload.title,description:payload.universe?.description||'',coverImage:payload.universe?.coverImage||'',chapters:array(payload.chapters),entities:array(payload.entities)}]}; state.universe=state.payload.universes[0]; document.querySelector('#title').textContent=state.payload.title||'Seleção NarraHub'; document.querySelector('#permission').textContent=permissionLabel('view'); renderNavigation(); renderMain(); }
function renderLegacyChapter(payload) { renderLegacyBundle({title:payload.title,universe:{name:payload.universeName},chapters:[payload],entities:[]}); }
function permissionLabel(value) { return value==='edit'?'Pode propor edições':value==='comment'?'Pode fazer anotações':'Somente leitura'; }
function pageHead(eyebrow,title,description) { const head=element('header','page-head'); const copy=element('div'); copy.append(element('p','eyebrow',eyebrow),element('h1','',title),element('p','',description)); head.append(copy); return head; }
function entityAvatar(entity) { const avatar=element('span','entity-avatar',string(entity.name).charAt(0).toUpperCase()||'?'); if (safeDataImage(entity.image)) { avatar.style.backgroundImage=`url(${entity.image})`; avatar.textContent=''; } return avatar; }
function sheetHead(eyebrow,title,dialog) { const head=element('header','sheet-head'); const copy=element('div'); copy.append(element('p','eyebrow',eyebrow),element('h2','',title)); const close=element('button','sheet-close','×'); close.type='button'; close.addEventListener('click',()=>dialog.close()); head.append(copy,close); return head; }
function sheetActions(dialog,label) { const row=element('div','sheet-actions'); const cancel=element('button','secondary','Cancelar'); cancel.type='button'; cancel.addEventListener('click',()=>dialog.close()); const submit=element('button','primary',label); submit.type='submit'; row.append(cancel,submit); return {row,submit}; }
function renderRichText(value) { const output=element('div','rich-text'); const parsed=new DOMParser().parseFromString(string(value),'text/html'); const allowed=new Set(['P','BR','BLOCKQUOTE','H1','H2','H3','H4','UL','OL','LI','STRONG','B','EM','I','U','S','HR']); const copy=(node,parent)=>{ if(node.nodeType===Node.TEXT_NODE){parent.append(document.createTextNode(node.textContent||''));return;} if(node.nodeType!==Node.ELEMENT_NODE)return; if(!allowed.has(node.tagName)){for(const child of node.childNodes)copy(child,parent);return;} const clean=document.createElement(node.tagName.toLowerCase());parent.append(clean);for(const child of node.childNodes)copy(child,clean);}; for(const node of parsed.body.childNodes)copy(node,output);if(!output.textContent?.trim()&&string(value).trim())output.textContent=string(value);return output; }
function serializeRichEditor(editor) { const container=element('div');for(const child of editor.childNodes)container.append(child.cloneNode(true));return new XMLSerializer().serializeToString(container).replace(/^<div xmlns="http:\/\/www\.w3\.org\/1999\/xhtml">|<\/div>$/gu,''); }
function wordCount(value) { const text=new DOMParser().parseFromString(string(value),'text/html').body.textContent?.trim()||'';return text?text.split(/\s+/u).length:0; }
function replaceChildren(node,...children) { node.replaceChildren(...children); }
function element(tag,className='',text='') { const node=document.createElement(tag);if(className)node.className=className;if(text)node.textContent=text;return node; }
function string(value) { return typeof value==='string'?value.slice(0,2_000_000):''; }
function array(value) { return Array.isArray(value)?value:[]; }
function safeDataImage(value) { return typeof value==='string'&&/^data:image\/(?:png|jpe?g|webp|gif);base64,[A-Za-z0-9+/=]+$/u.test(value); }
function toBase64Url(bytes) { let binary='';for(let index=0;index<bytes.length;index+=0x8000)binary+=String.fromCharCode(...bytes.subarray(index,index+0x8000));return btoa(binary).replace(/\+/gu,'-').replace(/\//gu,'_').replace(/=+$/gu,''); }
function fromBase64Url(value) { const normalized=value.replace(/-/gu,'+').replace(/_/gu,'/');const binary=atob(normalized+'='.repeat((4-normalized.length%4)%4));return Uint8Array.from(binary,(character)=>character.charCodeAt(0)); }
