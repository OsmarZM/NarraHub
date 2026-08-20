// ============================================
// NarraHub — TypeScript Models
// ============================================

// ── Universe ────────────────────────────────────

export interface Universe {
  id: string;
  name: string;
  description: string;
  cover_image: string;
  created_at: string;
  updated_at: string;
}

export interface UniverseStats {
  total_words: number;
  total_chapters: number;
  total_stories: number;
  total_books: number;
  total_entities: number;
  entity_counts: Record<string, number>;
}

export interface UniverseWithStats extends Universe {
  stats: UniverseStats;
}

// ── Story ───────────────────────────────────────

export interface Story {
  id: string;
  universe_id: string;
  name: string;
  description: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

// ── Book ────────────────────────────────────────

export interface Book {
  id: string;
  story_id: string;
  name: string;
  description: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

// ── Chapter ─────────────────────────────────────

export type ChapterStatus = 'IDEIA' | 'PLANEJADO' | 'ESCREVENDO' | 'REVISAO' | 'FINALIZADO';
export type CanonStatus = 'CANON' | 'IDEIA' | 'DESCARTADO' | 'ALTERNATIVO';

export interface Chapter {
  id: string;
  book_id: string;
  title: string;
  content: string; // JSON from Tiptap
  word_count: number;
  status: ChapterStatus;
  canon_status: CanonStatus;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

// ── Entity ──────────────────────────────────────

export type EntityType =
  | 'Personagem'
  | 'Lugar'
  | 'Evento'
  | 'Objeto'
  | 'Organização'
  | 'Raça'
  | 'Espécie'
  | 'Poder'
  | 'Religião'
  | 'Cultura'
  | 'Documento'
  | 'Nota'
  | string; // Extensible — user can create custom types

export interface Entity {
  id: string;
  universe_id: string;
  type: EntityType;
  name: string;
  description: string;
  image: string;
  canon_status: CanonStatus;
  created_at: string;
  updated_at: string;
}

export interface EntityAttribute {
  id: string;
  entity_id: string;
  key: string;
  value: string;
  sort_order: number;
}

export interface EntityTemplate {
  id: string;
  universe_id: string;
  entity_type: string;
  attribute_key: string;
  default_value: string;
  sort_order: number;
}

export interface EntityWithDetails extends Entity {
  attributes: EntityAttribute[];
  relations: RelationWithEntity[];
  mentions: MentionWithChapter[];
}

// ── Relation ────────────────────────────────────

export type RelationImportance = 'normal' | 'alta' | 'critica';

export interface Relation {
  id: string;
  universe_id: string;
  source_id: string;
  target_id: string;
  type: string;
  label: string;
  bidirectional: boolean;
  importance: RelationImportance;
  created_at: string;
}

export interface RelationWithEntity extends Relation {
  source: Entity;
  target: Entity;
}

// ── Mention ─────────────────────────────────────

export interface Mention {
  id: string;
  chapter_id: string;
  entity_id: string;
  created_at: string;
}

export interface MentionWithChapter extends Mention {
  chapter_title: string;
  book_name: string;
}

// ── Change Log ──────────────────────────────────

export interface ChangeLog {
  id: string;
  entity_type: string;
  entity_id: string;
  action: 'create' | 'update' | 'delete';
  field: string;
  old_value: string;
  new_value: string;
  created_at: string;
}

export interface TimelineEvent {
  id: string;
  universe_id: string;
  title: string;
  description: string;
  event_type: string;
  start_date: string;
  end_date: string;
  created_at: string;
  updated_at: string;
}

export type PlanningStatus = 'IDEIAS' | 'PLANEJADO' | 'ESCREVENDO' | 'REVISAO' | 'FINALIZADO';

export interface PlanningItem {
  id: string;
  universe_id: string;
  chapter_id: string | null;
  title: string;
  description: string;
  status: PlanningStatus;
  target_words: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface RelationCard extends Relation {
  source_name: string;
  source_type: string;
  target_name: string;
  target_type: string;
}

export interface HistoryEntry extends ChangeLog {
  display_name: string;
}

export interface SyncServerStatus {
  running: boolean;
  address: string | null;
  pairing_code: string | null;
  device_name: string;
}

export interface SyncResult {
  received: number;
  sent: number;
  conflicts: number;
  peer_name: string;
}

// ── Graph ───────────────────────────────────────

export type GraphViewMode = 'diagrama' | 'visual' | 'misto';

export interface GraphNode {
  id: string;
  name: string;
  type: EntityType;
  image: string;
  canon_status: CanonStatus;
  description: string;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  label: string;
  bidirectional: boolean;
  importance: RelationImportance;
}

export interface GraphFilters {
  entityTypes: Set<string>;
  focalEntityId: string | null;
  depth: number;
  showCanonOnly: boolean;
}

// ── Entity Shape Map ────────────────────────────

export const ENTITY_SHAPES: Record<string, { shape: string; symbol: string; color: string }> = {
  'Personagem': { shape: 'ellipse', symbol: '●', color: '#8b5cf6' },
  'Lugar': { shape: 'hexagon', symbol: '⬟', color: '#06b6d4' },
  'Evento': { shape: 'rectangle', symbol: '■', color: '#f59e0b' },
  'Objeto': { shape: 'triangle', symbol: '▲', color: '#10b981' },
  'Organização': { shape: 'diamond', symbol: '◆', color: '#ef4444' },
  'Raça': { shape: 'octagon', symbol: '⬢', color: '#ec4899' },
  'Espécie': { shape: 'octagon', symbol: '⬢', color: '#ec4899' },
  'Poder': { shape: 'octagon', symbol: '⬢', color: '#ec4899' },
  'Religião': { shape: 'octagon', symbol: '⬢', color: '#ec4899' },
  'Cultura': { shape: 'octagon', symbol: '⬢', color: '#ec4899' },
  'Documento': { shape: 'round-rectangle', symbol: '▭', color: '#64748b' },
  'Nota': { shape: 'round-rectangle', symbol: '▭', color: '#94a3b8' },
};

export const DEFAULT_ENTITY_SHAPE = { shape: 'ellipse', symbol: '●', color: '#94a3b8' };

// ── Predefined Relations ────────────────────────

export const PREDEFINED_RELATIONS = {
  familia: [
    { label: 'Irmão(ã) de', bidirectional: true },
    { label: 'Pai/Mãe de', bidirectional: false },
    { label: 'Filho(a) de', bidirectional: false },
    { label: 'Cônjuge de', bidirectional: true },
  ],
  social: [
    { label: 'Amigo(a) de', bidirectional: true },
    { label: 'Inimigo(a) de', bidirectional: true },
    { label: 'Aliado(a) de', bidirectional: true },
    { label: 'Mestre de', bidirectional: false },
    { label: 'Aprendiz de', bidirectional: false },
  ],
  romance: [
    { label: 'Ama', bidirectional: false },
    { label: 'Amou', bidirectional: false },
    { label: 'Casado(a) com', bidirectional: true },
  ],
  geografico: [
    { label: 'Nasceu em', bidirectional: false },
    { label: 'Vive em', bidirectional: false },
    { label: 'Capital de', bidirectional: false },
    { label: 'Contém', bidirectional: false },
  ],
  narrativo: [
    { label: 'Participou de', bidirectional: false },
    { label: 'Iniciou', bidirectional: false },
    { label: 'Destruiu', bidirectional: false },
    { label: 'Fundou', bidirectional: false },
  ],
  posse: [
    { label: 'Possui', bidirectional: false },
    { label: 'Controla', bidirectional: false },
    { label: 'Governa', bidirectional: false },
  ],
};

// ── Default Entity Attributes ───────────────────

export const DEFAULT_ATTRIBUTES: Record<string, string[]> = {
  'Personagem': [
    'Idade',
    'Nascimento',
    'Local de nascimento',
    'Descrição física',
    'Personalidade',
    'Objetivos',
    'Medos',
    'Segredos',
  ],
  'Lugar': [
    'Tipo',
    'População',
    'Governo',
    'Governante',
    'Clima',
  ],
  'Evento': [
    'Data',
    'Duração',
    'Local',
    'Consequências',
  ],
  'Objeto': [
    'Tipo',
    'Material',
    'Propriedades',
    'Dono atual',
    'Origem',
  ],
  'Organização': [
    'Tipo',
    'Líder',
    'Sede',
    'Membros',
    'Objetivo',
  ],
};
