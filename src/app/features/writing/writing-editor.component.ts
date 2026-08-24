import {
  AfterViewInit,
  ChangeDetectorRef,
  Component,
  ElementRef,
  EventEmitter,
  Input,
  OnChanges,
  OnDestroy,
  Output,
  SimpleChanges,
  ViewChild,
  ViewEncapsulation,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Editor, Extension, Node, mergeAttributes } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';

export interface WritingCharacter {
  id: string;
  name: string;
  image: string;
}

export interface AiWritingRequest {
  action: 'correct' | 'rewrite' | 'expand' | 'shorten' | 'custom' | 'chapter';
  instruction: string;
  selection: { from: number; to: number; text: string } | null;
  insertAt?: number;
  source?: 'selection' | 'slash' | 'toolbar';
}

interface PromptShortcut {
  id: string;
  command: string;
  label: string;
  prompt: string;
  icon: string;
  builtIn?: boolean;
}

interface CompletionSuggestion {
  value: string;
  kind: 'personagem' | 'frequente';
  image?: string;
}

const CHARACTER_AVATAR_KEY = new PluginKey<DecorationSet>('character-avatars');
const WRITER_VOCABULARY_KEY = 'narrahub.writerVocabulary';
const PROMPT_SHORTCUTS_KEY = 'narrahub.promptShortcuts';
const DEFAULT_PROMPT_SHORTCUTS: PromptShortcut[] = [
  { id: 'builtin-name', command: 'nome', label: 'Gerar um nome', icon: 'Aa', builtIn: true, prompt: 'Gere apenas um nome original e memorável, coerente com o gênero e o contexto deste capítulo. Não explique.' },
  { id: 'builtin-place', command: 'lugar', label: 'Criar um lugar', icon: '⌖', builtIn: true, prompt: 'Crie um lugar original adequado ao contexto. Retorne nome e uma descrição curta de até três frases, sem contradizer o capítulo.' },
  { id: 'builtin-character', command: 'personagem', label: 'Esboçar personagem', icon: '♙', builtIn: true, prompt: 'Crie um personagem coerente com o contexto. Retorne nome, papel narrativo, desejo e conflito em um parágrafo curto.' },
  { id: 'builtin-dialogue', command: 'dialogo', label: 'Sugerir diálogo', icon: '❝', builtIn: true, prompt: 'Sugira uma fala curta e natural para continuar a cena, preservando a personalidade dos personagens e sem inventar fatos importantes.' },
  { id: 'builtin-continue', command: 'continuar', label: 'Continuar a cena', icon: '→', builtIn: true, prompt: 'Continue a cena com um parágrafo curto e coerente, preservando estilo, ponto de vista e fatos já estabelecidos.' },
  { id: 'builtin-sensory', command: 'sensorial', label: 'Detalhe sensorial', icon: '✦', builtIn: true, prompt: 'Escreva um detalhe sensorial curto e coerente com a cena atual, sem alterar os acontecimentos.' },
];

const InlineImage = Node.create({
  name: 'image',
  group: 'block',
  atom: true,
  draggable: true,
  addAttributes() { return { src: { default: '' }, alt: { default: '' }, title: { default: '' } }; },
  parseHTML() { return [{ tag: 'img[src]' }]; },
  renderHTML({ HTMLAttributes }) { return ['img', mergeAttributes(HTMLAttributes, { loading: 'lazy' })]; },
});

function createCharacterAvatarExtension(getCharacters: () => WritingCharacter[]): Extension {
  const buildDecorations = (doc: ProseMirrorNode): DecorationSet => {
    const decorations: Decoration[] = [];
    const characters = [...getCharacters()]
      .filter((character) => character.name.trim().length > 1)
      .sort((a, b) => b.name.length - a.name.length);

    doc.descendants((node, position) => {
      if (!node.isText || !node.text || !characters.length) return;
      const source = node.text;
      const normalizedSource = source.toLocaleLowerCase('pt-BR');
      const occupied = new Set<number>();

      for (const character of characters) {
        const needle = character.name.toLocaleLowerCase('pt-BR');
        let start = 0;
        while ((start = normalizedSource.indexOf(needle, start)) >= 0) {
          const before = start > 0 ? source[start - 1] : '';
          const after = source[start + needle.length] ?? '';
          const hasWordBoundary = !/[\p{L}\p{N}_]/u.test(before) && !/[\p{L}\p{N}_]/u.test(after);
          if (hasWordBoundary && !occupied.has(start)) {
            occupied.add(start);
            decorations.push(Decoration.widget(position + start, () => {
              const avatar = document.createElement('span');
              avatar.className = 'nh-inline-character-avatar';
              avatar.contentEditable = 'false';
              avatar.title = character.name;
              avatar.setAttribute('aria-label', `Personagem ${character.name}`);
              if (character.image) avatar.style.backgroundImage = `url("${character.image.replace(/"/gu, '%22')}")`;
              else avatar.textContent = character.name.charAt(0).toLocaleUpperCase('pt-BR');
              return avatar;
            }, { key: `${character.id}-${position}-${start}`, side: -1 }));
          }
          start += Math.max(needle.length, 1);
        }
      }
    });
    return DecorationSet.create(doc, decorations);
  };

  return Extension.create({
    name: 'characterAvatars',
    addProseMirrorPlugins() {
      return [new Plugin<DecorationSet>({
        key: CHARACTER_AVATAR_KEY,
        state: {
          init: (_, state) => buildDecorations(state.doc),
          apply: (transaction, current) => transaction.docChanged || transaction.getMeta(CHARACTER_AVATAR_KEY)
            ? buildDecorations(transaction.doc)
            : current.map(transaction.mapping, transaction.doc),
        },
        props: { decorations: (state) => CHARACTER_AVATAR_KEY.getState(state) ?? null },
      })];
    },
  });
}

@Component({
  selector: 'app-writing-editor',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './writing-editor.component.html',
  styleUrl: './writing-editor.component.css',
  encapsulation: ViewEncapsulation.None,
})
export class WritingEditorComponent implements AfterViewInit, OnChanges, OnDestroy {
  @Input() title = '';
  @Input() content = '';
  @Input() saveMessage = '';
  @Input() focusMode = false;
  @Input() characters: WritingCharacter[] = [];
  @Input() aiEnabled = false;
  @Input() aiBusy = false;
  @Input() aiResponse = '';
  @Input() aiError = '';
  @Output() titleChange = new EventEmitter<string>();
  @Output() contentChange = new EventEmitter<string>();
  @Output() saveRequested = new EventEmitter<void>();
  @Output() focusModeChange = new EventEmitter<boolean>();
  @Output() fullscreenRequested = new EventEmitter<void>();
  @Output() assistantRequested = new EventEmitter<AiWritingRequest>();
  @Output() assistantApply = new EventEmitter<'replace' | 'insert'>();
  @Output() assistantDismissed = new EventEmitter<void>();
  @ViewChild('editorHost', { static: true }) private editorHost!: ElementRef<HTMLDivElement>;
  @ViewChild('imageInput', { static: true }) private imageInput!: ElementRef<HTMLInputElement>;
  @ViewChild('titleInput') private titleInput?: ElementRef<HTMLTextAreaElement>;

  editor: Editor | null = null;
  imageError = '';
  spellcheckEnabled = true;
  voicePanelOpen = false;
  voiceTranscript = '';
  voiceInterim = '';
  voiceError = '';
  isListening = false;
  completionSuggestions: CompletionSuggestion[] = [];
  activeCompletionIndex = 0;
  selectedText = '';
  aiPanelOpen = false;
  customPromptOpen = false;
  customPrompt = '';
  bubbleTop = 0;
  bubbleLeft = 0;
  bubbleBelow = false;
  slashMenuOpen = false;
  slashTop = 0;
  slashLeft = 0;
  slashQuery = '';
  slashActiveIndex = 0;
  shortcutEditorOpen = false;
  newShortcutCommand = '';
  newShortcutLabel = '';
  newShortcutPrompt = '';
  promptShortcuts: PromptShortcut[] = [...DEFAULT_PROMPT_SHORTCUTS, ...this.loadCustomShortcuts()];
  readonly speechSupported = typeof window !== 'undefined'
    && Boolean((window as any).SpeechRecognition || (window as any).webkitSpeechRecognition);

  private applyingExternalContent = false;
  private recognition: any = null;
  private completionFrom = 0;
  private slashFrom = 0;
  private vocabulary = new Map<string, number>();
  private readonly handleWindowResize = () => { this.resizeTitle(); this.positionAiBubble(); this.positionSlashMenu(); };
  private readonly handleDocumentScroll = () => { this.positionAiBubble(); this.positionSlashMenu(); };

  constructor(private readonly changeDetector: ChangeDetectorRef) { this.loadVocabulary(); }

  ngAfterViewInit(): void {
    this.editor = new Editor({
      element: this.editorHost.nativeElement,
      extensions: [StarterKit, InlineImage, createCharacterAvatarExtension(() => this.characters)],
      content: this.normalizeIncoming(this.content),
      editorProps: {
        attributes: {
          class: 'nh-prose',
          'aria-label': 'Conteúdo do capítulo',
          lang: 'pt-BR',
          spellcheck: 'true',
          autocapitalize: 'sentences',
        },
        handleKeyDown: (_, event) => this.handleSlashKey(event) || this.handleAutocompleteKey(event),
      },
      onUpdate: ({ editor }) => {
        if (!this.applyingExternalContent) this.contentChange.emit(editor.getHTML());
        this.refreshCompletions();
        this.refreshSlashCommands();
        this.changeDetector.detectChanges();
      },
      onSelectionUpdate: ({ editor }) => {
        this.refreshCompletions();
        const { from, to } = editor.state.selection;
        const nextSelectedText = from === to ? '' : editor.state.doc.textBetween(from, to, '\n').trim();
        if (nextSelectedText !== this.selectedText && (this.aiResponse || this.aiError)) this.assistantDismissed.emit();
        this.selectedText = nextSelectedText;
        if (this.selectedText) { this.aiPanelOpen = true; this.customPromptOpen = false; }
        else if (!this.aiBusy && !this.aiResponse && !this.customPromptOpen) this.aiPanelOpen = false;
        this.positionAiBubble();
        this.changeDetector.detectChanges();
      },
      onBlur: () => {
        this.learnVocabulary();
        this.saveRequested.emit();
      },
    });
    window.addEventListener('resize', this.handleWindowResize);
    document.addEventListener('scroll', this.handleDocumentScroll, true);
    queueMicrotask(() => this.resizeTitle());
  }

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['characters'] && this.editor) {
      this.editor.view.dispatch(this.editor.state.tr.setMeta(CHARACTER_AVATAR_KEY, true));
    }
    if (changes['aiBusy'] || changes['aiResponse'] || changes['aiError']) {
      if (this.aiBusy || this.aiResponse || this.aiError) this.aiPanelOpen = true;
      queueMicrotask(() => this.positionAiBubble());
    }
    if (!this.editor || !changes['content']) return;
    const incoming = this.normalizeIncoming(this.content);
    if (this.editor.getHTML() === incoming) return;
    this.applyingExternalContent = true;
    this.editor.commands.setContent(incoming, { emitUpdate: false });
    this.applyingExternalContent = false;
  }

  ngOnDestroy(): void {
    this.stopVoiceNote();
    this.learnVocabulary();
    window.removeEventListener('resize', this.handleWindowResize);
    document.removeEventListener('scroll', this.handleDocumentScroll, true);
    this.editor?.destroy();
  }

  command(action: 'bold' | 'italic' | 'strike' | 'bulletList' | 'orderedList' | 'blockquote' | 'horizontalRule' | 'undo' | 'redo'): void {
    if (!this.editor) return;
    const chain = this.editor.chain().focus();
    if (action === 'bold') chain.toggleBold().run();
    else if (action === 'italic') chain.toggleItalic().run();
    else if (action === 'strike') chain.toggleStrike().run();
    else if (action === 'bulletList') chain.toggleBulletList().run();
    else if (action === 'orderedList') chain.toggleOrderedList().run();
    else if (action === 'blockquote') chain.toggleBlockquote().run();
    else if (action === 'horizontalRule') chain.setHorizontalRule().run();
    else if (action === 'undo') chain.undo().run();
    else chain.redo().run();
  }

  setHeading(level: 1 | 2 | 3 | 0): void {
    if (!this.editor) return;
    if (level === 0) this.editor.chain().focus().setParagraph().run();
    else this.editor.chain().focus().toggleHeading({ level }).run();
  }

  setHeadingFromSelect(value: string): void {
    const level = Number(value);
    if (level === 0 || level === 1 || level === 2 || level === 3) this.setHeading(level);
  }

  onTitleInput(value: string): void {
    this.titleChange.emit(value.replace(/[\r\n]+/gu, ' '));
    queueMicrotask(() => this.resizeTitle());
  }

  requestImage(): void { this.imageError = ''; this.imageInput.nativeElement.click(); }

  requestAssistant(action: AiWritingRequest['action'] = 'chapter'): void {
    if (!this.editor) return;
    const { from, to } = this.editor.state.selection;
    const text = from === to ? '' : this.editor.state.doc.textBetween(from, to, '\n').trim();
    const instructions: Record<AiWritingRequest['action'], string> = {
      correct: 'Corrija ortografia, gramática e pontuação sem alterar a voz, o significado ou os fatos do trecho.',
      rewrite: 'Reescreva o trecho com mais fluidez e clareza, preservando a voz do escritor e os fatos.',
      expand: 'Detalhe um pouco mais este trecho com imagens sensoriais coerentes, sem inventar fatos importantes.',
      shorten: 'Encurte o trecho, removendo repetições e preservando significado, voz e fatos.',
      custom: '',
      chapter: '',
    };
    this.aiPanelOpen = true;
    this.positionAiBubble();
    if (action === 'custom' || action === 'chapter') {
      this.customPrompt = '';
      this.customPromptOpen = true;
      return;
    }
    this.customPromptOpen = false;
    this.assistantRequested.emit({ action, instruction: instructions[action], selection: text ? { from, to, text } : null, insertAt: from, source: text ? 'selection' : 'toolbar' });
  }

  submitCustomPrompt(): void {
    if (!this.editor || !this.customPrompt.trim()) return;
    const { from, to } = this.editor.state.selection;
    const text = from === to ? '' : this.editor.state.doc.textBetween(from, to, '\n').trim();
    this.customPromptOpen = false;
    this.assistantRequested.emit({ action: 'custom', instruction: this.customPrompt.trim(), selection: text ? { from, to, text } : null, insertAt: from, source: text ? 'selection' : 'toolbar' });
  }

  dismissAiPanel(): void {
    this.aiPanelOpen = false; this.customPromptOpen = false; this.customPrompt = '';
    this.assistantDismissed.emit();
  }

  applyAiResult(mode: 'replace' | 'insert'): void {
    this.aiPanelOpen = false; this.customPromptOpen = false; this.assistantApply.emit(mode);
  }

  retryCustomPrompt(): void {
    this.assistantDismissed.emit(); this.aiPanelOpen = true; this.customPromptOpen = true; this.customPrompt = '';
  }

  replaceRange(from: number, to: number, text: string): void {
    if (!this.editor) return;
    this.editor.chain().focus().insertContentAt({ from, to }, { type: 'text', text }).run();
    this.selectedText = '';
  }

  insertAtPosition(position: number, text: string): void {
    if (!this.editor || !text.trim()) return;
    this.editor.chain().focus().insertContentAt(position, text).run();
  }

  filteredPromptShortcuts(): PromptShortcut[] {
    const query = this.slashQuery.toLocaleLowerCase('pt-BR');
    return this.promptShortcuts.filter((shortcut) => !query
      || shortcut.command.includes(query)
      || shortcut.label.toLocaleLowerCase('pt-BR').includes(query));
  }

  executeSlashCommand(shortcut: PromptShortcut): void {
    if (!this.editor) return;
    const to = this.editor.state.selection.from;
    this.editor.chain().focus().deleteRange({ from: this.slashFrom, to }).run();
    this.slashMenuOpen = false; this.aiPanelOpen = true; this.customPromptOpen = false;
    this.positionAiBubble(this.slashFrom);
    this.assistantRequested.emit({ action: 'custom', instruction: shortcut.prompt, selection: null, insertAt: this.slashFrom, source: 'slash' });
  }

  savePromptShortcut(): void {
    const command = this.newShortcutCommand.trim().toLocaleLowerCase('pt-BR').replace(/^\/+|\s+/gu, '-').replace(/[^\p{L}\p{N}_-]/gu, '');
    const label = this.newShortcutLabel.trim(); const prompt = this.newShortcutPrompt.trim();
    if (!command || !label || !prompt || this.promptShortcuts.some((shortcut) => shortcut.command === command)) return;
    const shortcut: PromptShortcut = { id: crypto.randomUUID(), command, label, prompt, icon: '✦' };
    this.promptShortcuts = [...this.promptShortcuts, shortcut]; this.persistCustomShortcuts();
    this.newShortcutCommand = ''; this.newShortcutLabel = ''; this.newShortcutPrompt = ''; this.shortcutEditorOpen = false;
  }

  deletePromptShortcut(shortcut: PromptShortcut, event: Event): void {
    event.stopPropagation(); if (shortcut.builtIn) return;
    this.promptShortcuts = this.promptShortcuts.filter((item) => item.id !== shortcut.id); this.persistCustomShortcuts();
  }

  async importImage(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !this.editor) return;
    if (!file.type.startsWith('image/')) { this.imageError = 'Escolha um arquivo de imagem.'; return; }
    if (file.size > 8 * 1024 * 1024) { this.imageError = 'A imagem deve ter no máximo 8 MB.'; return; }
    const src = await this.fileToDataUrl(file);
    this.editor.chain().focus().insertContent({ type: 'image', attrs: { src, alt: file.name, title: file.name } }).run();
  }

  toggleSpellcheck(): void {
    this.spellcheckEnabled = !this.spellcheckEnabled;
    this.editor?.view.dom.setAttribute('spellcheck', String(this.spellcheckEnabled));
    this.editor?.view.focus();
  }

  toggleVoicePanel(): void {
    this.voicePanelOpen = !this.voicePanelOpen;
    if (!this.voicePanelOpen) this.stopVoiceNote();
  }

  startVoiceNote(): void {
    if (!this.speechSupported || this.isListening) return;
    this.voiceError = '';
    const Recognition = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
    this.recognition = new Recognition();
    this.recognition.lang = 'pt-BR';
    this.recognition.continuous = true;
    this.recognition.interimResults = true;
    this.recognition.onresult = (event: any) => {
      let interim = '';
      let finalText = '';
      for (let index = event.resultIndex; index < event.results.length; index++) {
        const transcript = String(event.results[index][0]?.transcript ?? '').trim();
        if (event.results[index].isFinal) finalText += `${transcript} `;
        else interim += transcript;
      }
      if (finalText) this.voiceTranscript = `${this.voiceTranscript} ${finalText}`.trim();
      this.voiceInterim = interim;
    };
    this.recognition.onerror = (event: any) => {
      this.voiceError = event.error === 'not-allowed'
        ? 'Permissão do microfone negada. Libere o acesso nas configurações do sistema.'
        : 'A transcrição foi interrompida. Tente novamente.';
      this.isListening = false;
    };
    this.recognition.onend = () => { this.isListening = false; this.voiceInterim = ''; };
    try { this.recognition.start(); this.isListening = true; }
    catch { this.voiceError = 'Não foi possível iniciar o microfone.'; }
  }

  stopVoiceNote(): void {
    if (this.recognition) {
      try { this.recognition.stop(); } catch { /* already stopped */ }
    }
    this.recognition = null;
    this.isListening = false;
    this.voiceInterim = '';
  }

  insertVoiceNote(): void {
    const text = this.voiceTranscript.trim();
    if (!text || !this.editor) return;
    this.editor.chain().focus().insertContent({ type: 'text', text: `${text} ` }).run();
    this.voiceTranscript = '';
    this.voiceInterim = '';
  }

  insertPlainText(text: string): void {
    if (!this.editor || !text.trim()) return;
    this.editor.chain().focus().insertContent({ type: 'text', text }).run();
  }

  applyCompletion(suggestion: CompletionSuggestion): void {
    if (!this.editor) return;
    const to = this.editor.state.selection.from;
    this.editor.chain().focus().insertContentAt({ from: this.completionFrom, to }, suggestion.value).run();
    this.completionSuggestions = [];
  }

  private handleSlashKey(event: KeyboardEvent): boolean {
    if (!this.slashMenuOpen) return false;
    const shortcuts = this.filteredPromptShortcuts();
    if (event.key === 'ArrowDown') {
      event.preventDefault(); this.slashActiveIndex = (this.slashActiveIndex + 1) % Math.max(1, shortcuts.length); return true;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault(); this.slashActiveIndex = (this.slashActiveIndex - 1 + Math.max(1, shortcuts.length)) % Math.max(1, shortcuts.length); return true;
    }
    if (event.key === 'Enter' && shortcuts.length) {
      event.preventDefault(); this.executeSlashCommand(shortcuts[this.slashActiveIndex] ?? shortcuts[0]); return true;
    }
    if (event.key === 'Escape') { event.preventDefault(); this.slashMenuOpen = false; return true; }
    return false;
  }

  private refreshSlashCommands(): void {
    if (!this.editor || !this.editor.isFocused || !this.aiEnabled || !this.editor.state.selection.empty) { this.slashMenuOpen = false; return; }
    const { $from } = this.editor.state.selection;
    if (!$from.parent.isTextblock) { this.slashMenuOpen = false; return; }
    const beforeCursor = $from.parent.textBetween(0, $from.parentOffset, undefined, '\ufffc');
    const match = beforeCursor.match(/(?:^|\s)\/([\p{L}\p{N}_-]*)$/u);
    if (!match) { this.slashMenuOpen = false; return; }
    this.slashQuery = match[1].toLocaleLowerCase('pt-BR');
    this.slashFrom = this.editor.state.selection.from - this.slashQuery.length - 1;
    this.slashActiveIndex = Math.min(this.slashActiveIndex, Math.max(0, this.filteredPromptShortcuts().length - 1));
    this.slashMenuOpen = true; this.completionSuggestions = []; this.positionSlashMenu();
  }

  private positionAiBubble(position?: number): void {
    if (!this.editor || !this.aiPanelOpen) return;
    try {
      const { from, to } = this.editor.state.selection;
      const anchor = position ?? from; const head = position ?? to;
      const start = this.editor.view.coordsAtPos(anchor); const end = this.editor.view.coordsAtPos(head);
      const top = Math.min(start.top, end.top); const bottom = Math.max(start.bottom, end.bottom);
      const offset = this.fixedContainingOffset();
      this.bubbleBelow = top < 190;
      this.bubbleTop = (this.bubbleBelow ? bottom + 10 : top - 10) - offset.top;
      this.bubbleLeft = Math.max(272, Math.min(window.innerWidth - 272, (start.left + end.right) / 2)) - offset.left;
    } catch { /* editor may be changing documents */ }
  }

  private positionSlashMenu(): void {
    if (!this.editor || !this.slashMenuOpen) return;
    try {
      const coords = this.editor.view.coordsAtPos(this.editor.state.selection.from);
      const offset = this.fixedContainingOffset();
      this.slashTop = Math.min(window.innerHeight - 360, coords.bottom + 8) - offset.top;
      this.slashLeft = Math.max(12, Math.min(window.innerWidth - 340, coords.left)) - offset.left;
    } catch { /* editor may be changing documents */ }
  }

  private fixedContainingOffset(): { left: number; top: number } {
    let element: HTMLElement | null = this.editorHost.nativeElement.parentElement;
    while (element) {
      if (getComputedStyle(element).transform !== 'none') {
        const rect = element.getBoundingClientRect(); return { left: rect.left, top: rect.top };
      }
      element = element.parentElement;
    }
    return { left: 0, top: 0 };
  }

  private handleAutocompleteKey(event: KeyboardEvent): boolean {
    if (!this.completionSuggestions.length) return false;
    if (event.key === 'Tab') {
      event.preventDefault();
      this.applyCompletion(this.completionSuggestions[this.activeCompletionIndex] ?? this.completionSuggestions[0]);
      return true;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      this.activeCompletionIndex = (this.activeCompletionIndex + 1) % this.completionSuggestions.length;
      return true;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      this.activeCompletionIndex = (this.activeCompletionIndex - 1 + this.completionSuggestions.length) % this.completionSuggestions.length;
      return true;
    }
    if (event.key === 'Escape') { this.completionSuggestions = []; return true; }
    return false;
  }

  private refreshCompletions(): void {
    if (!this.editor || !this.editor.isFocused) { this.completionSuggestions = []; return; }
    const { $from } = this.editor.state.selection;
    if (!$from.parent.isTextblock) { this.completionSuggestions = []; return; }
    const beforeCursor = $from.parent.textBetween(0, $from.parentOffset, undefined, '\ufffc');
    const match = beforeCursor.match(/([\p{L}\p{N}][\p{L}\p{N}'’-]{1,})$/u);
    if (!match) { this.completionSuggestions = []; return; }
    const fragment = match[1];
    const normalized = fragment.toLocaleLowerCase('pt-BR');
    this.completionFrom = this.editor.state.selection.from - fragment.length;

    const characterSuggestions: CompletionSuggestion[] = this.characters
      .filter((character) => character.name.toLocaleLowerCase('pt-BR').startsWith(normalized)
        && character.name.toLocaleLowerCase('pt-BR') !== normalized)
      .map((character) => ({ value: character.name, kind: 'personagem', image: character.image }));
    const frequentSuggestions: CompletionSuggestion[] = [...this.vocabulary.entries()]
      .filter(([value]) => value.toLocaleLowerCase('pt-BR').startsWith(normalized)
        && value.toLocaleLowerCase('pt-BR') !== normalized)
      .sort((a, b) => b[1] - a[1])
      .map(([value]) => ({ value, kind: 'frequente' as const }));
    const seen = new Set<string>();
    this.completionSuggestions = [...characterSuggestions, ...frequentSuggestions]
      .filter((item) => { const key = item.value.toLocaleLowerCase('pt-BR'); if (seen.has(key)) return false; seen.add(key); return true; })
      .slice(0, 5);
    this.activeCompletionIndex = 0;
  }

  private loadVocabulary(): void {
    try {
      const stored = JSON.parse(localStorage.getItem(WRITER_VOCABULARY_KEY) || '{}') as Record<string, number>;
      this.vocabulary = new Map(Object.entries(stored).filter(([, count]) => Number.isFinite(count)));
    } catch { this.vocabulary = new Map(); }
  }

  private loadCustomShortcuts(): PromptShortcut[] {
    try {
      const stored = JSON.parse(localStorage.getItem(PROMPT_SHORTCUTS_KEY) || '[]') as PromptShortcut[];
      return stored.filter((shortcut) => shortcut.id && shortcut.command && shortcut.label && shortcut.prompt)
        .map((shortcut) => ({ ...shortcut, builtIn: false, icon: shortcut.icon || '✦' }));
    } catch { return []; }
  }

  private persistCustomShortcuts(): void {
    const custom = this.promptShortcuts.filter((shortcut) => !shortcut.builtIn);
    try { localStorage.setItem(PROMPT_SHORTCUTS_KEY, JSON.stringify(custom)); } catch { /* storage unavailable */ }
  }

  private learnVocabulary(): void {
    if (!this.editor) return;
    const words = this.editor.getText().match(/[\p{L}][\p{L}'’-]{3,}/gu) ?? [];
    const chapterCounts = new Map<string, { value: string; count: number }>();
    for (const word of words) {
      const key = word.toLocaleLowerCase('pt-BR');
      const current = chapterCounts.get(key) ?? { value: word, count: 0 };
      current.count++;
      chapterCounts.set(key, current);
    }
    for (const { value, count } of chapterCounts.values()) {
      if (count >= 2) this.vocabulary.set(value, Math.min(500, (this.vocabulary.get(value) ?? 0) + 1));
    }
    const compact = [...this.vocabulary.entries()].sort((a, b) => b[1] - a[1]).slice(0, 300);
    try { localStorage.setItem(WRITER_VOCABULARY_KEY, JSON.stringify(Object.fromEntries(compact))); } catch { /* storage unavailable */ }
  }

  private resizeTitle(): void {
    const element = this.titleInput?.nativeElement;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${Math.max(element.scrollHeight, 52)}px`;
  }

  private fileToDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error ?? new Error('Falha ao ler a imagem.'));
      reader.readAsDataURL(file);
    });
  }

  private normalizeIncoming(content: string): string {
    const value = content.trim();
    if (!value) return '<p></p>';
    if (/<(?:p|h[1-6]|ul|ol|li|blockquote|img|hr|div|br)\b/iu.test(value)) return content;
    const escaped = value.replace(/&/gu, '&amp;').replace(/</gu, '&lt;').replace(/>/gu, '&gt;');
    return escaped.split(/\n{2,}/u).map((paragraph) => `<p>${paragraph.replace(/\n/gu, '<br>')}</p>`).join('');
  }
}
