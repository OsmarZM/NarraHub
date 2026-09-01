import { Injectable, inject } from '@angular/core';
import { EntityWithDetails, UniverseWithStats } from '../core/models';
import { OnlineShareDocument, SharedUniverse } from '../core/native/online-share.service';
import { EntityStore } from '../features/entities/state/entity.store';
import { ManuscriptStore } from '../features/manuscript/state/manuscript.store';

/** O que o autor escolheu compartilhar. Vem da tela; a montagem é daqui. */
export interface ShareDocumentRequest {
  universes: UniverseWithStats[];
  includeChapters: boolean;
  includeEntities: boolean;
  permission: OnlineShareDocument['permission'];
}

/**
 * Monta o documento que será cifrado e compartilhado.
 *
 * Existe porque o `WorkspaceLayout` fazia isso: escolher quais capítulos entram, achatar
 * entidade com atributos, e **redimensionar imagem em canvas para WebP**. Compressão de imagem
 * não é responsabilidade de um componente de layout — e enquanto morava lá, qualquer mudança
 * no formato do documento passava pelo arquivo mais movimentado do frontend.
 *
 * O serviço não conhece a sessão de compartilhamento nem a interface: ele recebe o que foi
 * escolhido e devolve o documento. Quem abre a sessão, cifra, copia o link e avisa o usuário
 * continua sendo quem chamou.
 */
@Injectable({ providedIn: 'root' })
export class WorkspaceShareService {
  private readonly manuscript = inject(ManuscriptStore);
  private readonly entities = inject(EntityStore);

  /**
   * Acima disto a imagem é recomprimida. O limite existe porque o documento inteiro viaja
   * cifrado dentro de um link: capa original de câmera estoura o que é razoável trafegar.
   */
  private static readonly MAX_INLINE_IMAGE_BYTES = 180_000;
  private static readonly MAX_IMAGE_EDGE = 720;
  private static readonly WEBP_QUALITY = 0.76;

  async buildDocument(request: ShareDocumentRequest): Promise<OnlineShareDocument> {
    const universes = await Promise.all(
      request.universes.map((universe) =>
        this.buildSharedUniverse(universe, request.includeChapters, request.includeEntities),
      ),
    );
    return {
      version: 3,
      kind: 'workspace',
      title: this.titleFor(request.universes),
      permission: request.permission,
      universes,
      sharedAt: new Date().toISOString(),
    };
  }

  private titleFor(universes: UniverseWithStats[]): string {
    return universes.length === 1 ? universes[0].name : `${universes.length} universos literários`;
  }

  private async buildSharedUniverse(
    universe: UniverseWithStats,
    includeChapters: boolean,
    includeEntities: boolean,
  ): Promise<SharedUniverse> {
    const [chapters, entities] = await Promise.all([
      includeChapters ? this.manuscript.listChaptersSnapshot(universe.id) : Promise.resolve([]),
      includeEntities ? this.entities.listSnapshot(universe.id) : Promise.resolve([]),
    ]);
    const details = includeEntities
      ? (await Promise.all(entities.map((entity) => this.entities.getDetailsSnapshot(entity.id)))).filter(
          (entity): entity is EntityWithDetails => !!entity,
        )
      : [];

    return {
      id: universe.id,
      name: universe.name,
      description: universe.description,
      coverImage: await this.prepareImage(universe.cover_image),
      chapters: chapters.map((chapter) => ({
        id: chapter.id,
        title: chapter.title,
        content: chapter.content,
        summary: chapter.summary,
        storyName: chapter.story_name,
        bookName: chapter.book_name,
      })),
      entities: await Promise.all(
        details.map(async (entity) => ({
          id: entity.id,
          type: entity.type,
          name: entity.name,
          summary: entity.summary,
          description: entity.description,
          image: await this.prepareImage(entity.image),
          canonStatus: entity.canon_status,
          // Atributo em branco não vira linha vazia na ficha de quem recebe.
          attributes: entity.attributes
            .filter((attribute) => attribute.value.trim())
            .map((attribute) => ({ key: attribute.key, value: attribute.value })),
        })),
      ),
    };
  }

  /**
   * Recomprime imagem grande para WebP. Falha de decodificação devolve string vazia em vez de
   * rejeitar: uma capa que não carrega não pode impedir o compartilhamento do texto.
   */
  private async prepareImage(dataUrl: string): Promise<string> {
    if (!dataUrl || dataUrl.length <= WorkspaceShareService.MAX_INLINE_IMAGE_BYTES) return dataUrl;
    return new Promise((resolve) => {
      const image = new Image();
      image.onload = () => {
        const scale = Math.min(
          1,
          WorkspaceShareService.MAX_IMAGE_EDGE / Math.max(image.naturalWidth, image.naturalHeight),
        );
        const canvas = document.createElement('canvas');
        canvas.width = Math.max(1, Math.round(image.naturalWidth * scale));
        canvas.height = Math.max(1, Math.round(image.naturalHeight * scale));
        canvas.getContext('2d')?.drawImage(image, 0, 0, canvas.width, canvas.height);
        resolve(canvas.toDataURL('image/webp', WorkspaceShareService.WEBP_QUALITY));
      };
      image.onerror = () => resolve('');
      image.src = dataUrl;
    });
  }
}
