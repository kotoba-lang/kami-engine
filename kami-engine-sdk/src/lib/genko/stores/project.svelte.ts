/**
 * Genko project store — project management via XRPC.
 * Svelte 5 runes ($state).
 */

import { authHeaders } from './auth.svelte.js';

// --- Types ---
export interface GenkoProject {
  convoId: string;
  name: string;
  description: string;
  createdAt: string;
  updatedAt: string;
}

// --- Constants ---
const PROJ_CACHE_PREFIX = 'genko_projects_';
const PROJ_ACTIVE_PREFIX = 'genko_active_';

function projCacheKey(nanoid: string): string { return PROJ_CACHE_PREFIX + nanoid; }
function projActiveKey(nanoid: string): string { return PROJ_ACTIVE_PREFIX + nanoid; }

// --- Reactive state ---
let _projects = $state<GenkoProject[]>([]);
let _activeProjectId = $state('');

// --- Accessors ---
export function getProjects(): GenkoProject[] { return _projects; }
export function setProjects(p: GenkoProject[]) { _projects = p; }
export function getActiveProjectId(): string { return _activeProjectId; }
export function setActiveProjectId(id: string) { _activeProjectId = id; }

// --- Load projects from XRPC + cache ---
export async function loadProjects(nanoid: string): Promise<GenkoProject[]> {
  // Try cache first
  try {
    const cached = localStorage.getItem(projCacheKey(nanoid));
    if (cached) {
      const parsed = JSON.parse(cached) as GenkoProject[];
      if (Array.isArray(parsed)) _projects = parsed;
    }
  } catch { /* invalid cache — skip */ }

  // Restore active project
  try {
    const active = localStorage.getItem(projActiveKey(nanoid));
    if (active) _activeProjectId = active;
  } catch { /* skip */ }

  // Fetch from XRPC
  try {
    const res = await fetch(
      `/xrpc/ai.gftd.mangaka.listProjects?nanoid=${encodeURIComponent(nanoid)}`,
      { headers: authHeaders() },
    );
    if (res.ok) {
      const data = await res.json() as { projects: GenkoProject[] };
      if (Array.isArray(data?.projects)) {
        _projects = data.projects;
        try { localStorage.setItem(projCacheKey(nanoid), JSON.stringify(data.projects)); } catch { /* quota */ }
      }
    }
  } catch { /* network error — use cache */ }

  return _projects;
}

// --- Switch active project ---
export function switchProject(convoId: string, nanoid?: string): void {
  _activeProjectId = convoId;
  if (nanoid) {
    try { localStorage.setItem(projActiveKey(nanoid), convoId); } catch { /* quota */ }
  }
}

// --- Create project via XRPC ---
export async function createProject(name: string, desc: string, nanoid?: string): Promise<GenkoProject | null> {
  try {
    const res = await fetch('/xrpc/ai.gftd.mangaka.createProject', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ name, description: desc }),
    });
    if (!res.ok) return null;

    const project = await res.json() as GenkoProject;
    if (project?.convoId) {
      _projects = [..._projects, project];
      _activeProjectId = project.convoId;
      if (nanoid) {
        try {
          localStorage.setItem(projCacheKey(nanoid), JSON.stringify(_projects));
          localStorage.setItem(projActiveKey(nanoid), project.convoId);
        } catch { /* quota */ }
      }
      return project;
    }
  } catch { /* network error */ }

  return null;
}
