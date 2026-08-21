import { AfterViewInit, Component, ElementRef, EventEmitter, Input, OnChanges, OnDestroy, Output, SimpleChanges, ViewChild, ViewEncapsulation } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Editor, Node, mergeAttributes } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';

const InlineImage = Node.create({
  name: 'image',
  group: 'block',
  atom: true,
  draggable: true,
  addAttributes() { return { src: { default: '' }, alt: { default: '' }, title: { default: '' } }; },
  parseHTML() { return [{ tag: 'img[src]' }]; },
  renderHTML({ HTMLAttributes }) { return ['img', mergeAttributes(HTMLAttributes, { loading: 'lazy' })]; },
});

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
  @Output() titleChange = new EventEmitter<string>();
  @Output() contentChange = new EventEmitter<string>();
  @Output() saveRequested = new EventEmitter<void>();
  @Output() focusModeChange = new EventEmitter<boolean>();
  @Output() fullscreenRequested = new EventEmitter<void>();
  @ViewChild('editorHost', { static: true }) private editorHost!: ElementRef<HTMLDivElement>;
  @ViewChild('imageInput', { static: true }) private imageInput!: ElementRef<HTMLInputElement>;

  editor: Editor | null = null;
  imageError = '';
  private applyingExternalContent = false;

  ngAfterViewInit(): void {
    this.editor = new Editor({
      element: this.editorHost.nativeElement,
      extensions: [StarterKit, InlineImage],
      content: this.normalizeIncoming(this.content),
      editorProps: { attributes: { class: 'nh-prose', 'aria-label': 'Conteúdo do capítulo' } },
      onUpdate: ({ editor }) => {
        if (!this.applyingExternalContent) this.contentChange.emit(editor.getHTML());
      },
      onBlur: () => this.saveRequested.emit(),
    });
  }

  ngOnChanges(changes: SimpleChanges): void {
    if (!this.editor || !changes['content']) return;
    const incoming = this.normalizeIncoming(this.content);
    if (this.editor.getHTML() === incoming) return;
    this.applyingExternalContent = true;
    this.editor.commands.setContent(incoming, { emitUpdate: false });
    this.applyingExternalContent = false;
  }

  ngOnDestroy(): void { this.editor?.destroy(); }

  command(action: 'bold' | 'italic' | 'strike' | 'bulletList' | 'orderedList' | 'blockquote' | 'horizontalRule'): void {
    if (!this.editor) return;
    const chain = this.editor.chain().focus();
    if (action === 'bold') chain.toggleBold().run();
    else if (action === 'italic') chain.toggleItalic().run();
    else if (action === 'strike') chain.toggleStrike().run();
    else if (action === 'bulletList') chain.toggleBulletList().run();
    else if (action === 'orderedList') chain.toggleOrderedList().run();
    else if (action === 'blockquote') chain.toggleBlockquote().run();
    else chain.setHorizontalRule().run();
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

  requestImage(): void { this.imageError = ''; this.imageInput.nativeElement.click(); }

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
