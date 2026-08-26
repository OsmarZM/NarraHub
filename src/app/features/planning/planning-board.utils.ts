import type { PlanningFieldValue, PlanningFieldValues, PlanningItem, PlanningStatus } from '../../core/models';

export const PLANNING_STATUSES: PlanningStatus[] = ['IDEIAS', 'PLANEJADO', 'ESCREVENDO', 'REVISAO', 'FINALIZADO'];

export function parsePlanningFieldValues(raw: string): PlanningFieldValues {
  try {
    const parsed = JSON.parse(raw || '{}');
    if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(([key, value]) => Boolean(key) && isPlanningFieldValue(value)),
    ) as PlanningFieldValues;
  } catch {
    return {};
  }
}

export function serializePlanningFieldValues(values: PlanningFieldValues): string {
  const sanitized = Object.fromEntries(
    Object.entries(values).filter(([key, value]) => Boolean(key) && isPlanningFieldValue(value) && value !== null),
  );
  return JSON.stringify(sanitized);
}

export function reorderPlanningItems(
  items: PlanningItem[],
  draggedId: string,
  targetStatus: PlanningStatus,
  targetIndex: number,
): PlanningItem[] {
  const dragged = items.find((item) => item.id === draggedId);
  if (!dragged || !PLANNING_STATUSES.includes(targetStatus)) return items;

  const withoutDragged = items.filter((item) => item.id !== draggedId);
  const columns = new Map(PLANNING_STATUSES.map((status) => [
    status,
    withoutDragged.filter((item) => item.status === status).sort((a, b) => a.sort_order - b.sort_order),
  ]));
  const target = columns.get(targetStatus) ?? [];
  target.splice(Math.max(0, Math.min(targetIndex, target.length)), 0, { ...dragged, status: targetStatus });
  columns.set(targetStatus, target);

  return PLANNING_STATUSES.flatMap((status) =>
    (columns.get(status) ?? []).map((item, sort_order) => ({ ...item, status, sort_order })),
  );
}

function isPlanningFieldValue(value: unknown): value is PlanningFieldValue {
  return value === null
    || typeof value === 'string'
    || typeof value === 'boolean'
    || (Array.isArray(value) && value.every((entry) => typeof entry === 'string'));
}
