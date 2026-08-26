export type AppNavigationId =
  | 'inicio'
  | 'escrita'
  | 'entidades'
  | 'conexoes'
  | 'timeline'
  | 'planejamento'
  | 'historico'
  | 'configuracoes';

export interface AppRouteState {
  navId: AppNavigationId;
  universeId: string | null;
}

const workspacePaths: Record<Exclude<AppNavigationId, 'inicio' | 'configuracoes'>, string> = {
  escrita: 'writing',
  entidades: 'entities',
  conexoes: 'connections',
  timeline: 'timeline',
  planejamento: 'planning',
  historico: 'history',
};

const navByWorkspacePath = Object.fromEntries(
  Object.entries(workspacePaths).map(([navId, path]) => [path, navId]),
) as Record<string, Exclude<AppNavigationId, 'inicio' | 'configuracoes'>>;

export function buildAppPath(navId: AppNavigationId, universeId: string | null): string {
  if (navId === 'inicio') return '/library';
  if (navId === 'configuracoes') return '/settings';
  if (!universeId) return '/library';
  return `/workspace/${encodeURIComponent(universeId)}/${workspacePaths[navId]}`;
}

export function parseAppPath(url: string): AppRouteState {
  const segments = url.split(/[?#]/u, 1)[0].split('/').filter(Boolean);
  if (segments[0] === 'settings') return { navId: 'configuracoes', universeId: null };
  if (segments[0] === 'workspace' && segments[1]) {
    const navId = navByWorkspacePath[segments[2] || 'writing'] ?? 'escrita';
    return { navId, universeId: safeDecode(segments[1]) };
  }
  return { navId: 'inicio', universeId: null };
}

function safeDecode(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

