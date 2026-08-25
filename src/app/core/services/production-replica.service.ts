import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';

export interface ReplicaCounts {
  universes: number;
  stories: number;
  books: number;
  chapters: number;
  entities: number;
}

export interface ReplicaChange {
  kind: 'universe' | 'story' | 'book' | 'chapter' | 'entity';
  id: string;
  name: string;
}

export interface ReplicaChanges {
  addedCount: number;
  removedCount: number;
  added: ReplicaChange[];
  removed: ReplicaChange[];
}

export interface ProductionReplicaStatus {
  enabled: boolean;
  sourceExists: boolean;
  sourceModifiedAt: string | null;
  snapshotId: string | null;
  capturedAt: string | null;
  previousSnapshotId: string | null;
  schemaVersion: number | null;
  counts: ReplicaCounts;
  changes: ReplicaChanges;
}

export interface ReplicaCatalogItem {
  id: string;
  parentId: string | null;
  name: string;
  detail: string;
  wordCount: number;
  updatedAt: string;
}

export interface ProductionReplicaCatalog {
  snapshotId: string;
  capturedAt: string;
  universes: ReplicaCatalogItem[];
  stories: ReplicaCatalogItem[];
  books: ReplicaCatalogItem[];
  chapters: ReplicaCatalogItem[];
  entities: ReplicaCatalogItem[];
}

export interface ProductionReplicaChapter {
  id: string;
  title: string;
  content: string;
  wordCount: number;
  updatedAt: string;
  bookName: string;
  storyName: string;
  universeName: string;
  snapshotId: string;
  capturedAt: string;
}

@Injectable({ providedIn: 'root' })
export class ProductionReplicaService {
  async status(): Promise<ProductionReplicaStatus> {
    this.ensureDesktop();
    return invoke<ProductionReplicaStatus>('production_replica_status');
  }

  async refresh(): Promise<ProductionReplicaStatus> {
    this.ensureDesktop();
    return invoke<ProductionReplicaStatus>('production_replica_refresh');
  }

  async catalog(): Promise<ProductionReplicaCatalog> {
    this.ensureDesktop();
    return invoke<ProductionReplicaCatalog>('production_replica_catalog');
  }

  async chapter(chapterId: string): Promise<ProductionReplicaChapter> {
    this.ensureDesktop();
    return invoke<ProductionReplicaChapter>('production_replica_chapter', { chapterId });
  }

  private ensureDesktop(): void {
    if (!isTauri()) throw new Error('A réplica de produção está disponível somente no desktop de desenvolvimento.');
  }
}
