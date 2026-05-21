/**
 * Genko auth store — cross-subdomain SSO via authn.gftd.ai (ADR-0024).
 * Svelte 5 runes ($state).
 */

// --- Types ---
export interface AuthSession {
  accessJwt: string;
  refreshJwt: string;
  did: string;
  handle: string;
}

interface ParentWindowWithSession extends Window {
  __gftd_session?: AuthSession;
}

// --- Constants ---
const AUTH_STORAGE_KEY = 'gftd_session';
// ADR-0024 T4 split: auth.gftd.ai retired 2026-04-16 → authn.gftd.ai (AuthN).
const AUTH_URL = 'https://authn.gftd.ai/sign-in';
const AUTH_REFRESH_URL = 'https://atproto.gftd.ai/xrpc/com.atproto.server.refreshSession';

// --- Reactive state ---
let _sessionToken = $state<AuthSession | null>(null);

// --- Accessors ---
export function getSessionToken(): AuthSession | null { return _sessionToken; }
export function setSessionToken(s: AuthSession | null) { _sessionToken = s; }

// --- Session resolution (parent window > localStorage > sessionStorage) ---
export function getSession(): AuthSession | null {
  // 1. Parent window (iframe SSO)
  try {
    if (typeof window !== 'undefined' && window.parent !== window) {
      const parentSession = (window.parent as ParentWindowWithSession).__gftd_session;
      if (parentSession?.accessJwt) return parentSession;
    }
  } catch { /* cross-origin — skip */ }

  // 2. localStorage
  if (typeof localStorage !== 'undefined') {
    try {
      const raw = localStorage.getItem(AUTH_STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as AuthSession;
        if (parsed?.accessJwt) return parsed;
      }
    } catch { /* invalid JSON — skip */ }
  }

  // 3. sessionStorage
  if (typeof sessionStorage !== 'undefined') {
    try {
      const raw = sessionStorage.getItem(AUTH_STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as AuthSession;
        if (parsed?.accessJwt) return parsed;
      }
    } catch { /* invalid JSON — skip */ }
  }

  return null;
}

// --- Auth callback parsing (#auth={json} or query params) ---
export function parseAuthCallback(): AuthSession | null {
  if (typeof window === 'undefined') return null;

  let session: AuthSession | null = null;

  // Try hash fragment: #auth={json}
  const hash = window.location.hash;
  if (hash.startsWith('#auth=')) {
    try {
      const decoded = decodeURIComponent(hash.slice(6));
      const parsed = JSON.parse(decoded) as AuthSession;
      if (parsed?.accessJwt && parsed?.did) session = parsed;
    } catch { /* invalid — skip */ }
  }

  // Try query params: ?accessJwt=...&did=...&handle=...&refreshJwt=...
  if (!session) {
    const params = new URLSearchParams(window.location.search);
    const accessJwt = params.get('accessJwt');
    const refreshJwt = params.get('refreshJwt');
    const did = params.get('did');
    const handle = params.get('handle');
    if (accessJwt && did) {
      session = { accessJwt, refreshJwt: refreshJwt || '', did, handle: handle || '' };
    }
  }

  if (session) {
    // Persist to localStorage
    try { localStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(session)); } catch { /* quota — skip */ }
    // Clean URL
    window.history.replaceState(null, '', window.location.pathname);
  }

  return session;
}

// --- Redirect to authn.gftd.ai ---
export function redirectToAuth(nanoid: string): void {
  if (typeof window === 'undefined') return;
  const redirectUrl = encodeURIComponent(window.location.origin + window.location.pathname);
  window.location.href = `${AUTH_URL}?redirectUrl=${redirectUrl}&app=mangaka&nanoid=${encodeURIComponent(nanoid)}`;
}

// --- Auth headers ---
export function authHeaders(): Record<string, string> {
  if (!_sessionToken?.accessJwt) return {};
  return { Authorization: `Bearer ${_sessionToken.accessJwt}` };
}

// --- Handle auth error (refresh or redirect) ---
export async function handleAuthError(): Promise<boolean> {
  if (!_sessionToken?.refreshJwt) return false;

  try {
    // AT Protocol refreshSession: Authorization: Bearer <refreshJwt>, empty body.
    const res = await fetch(AUTH_REFRESH_URL, {
      method: 'POST',
      headers: { Authorization: `Bearer ${_sessionToken.refreshJwt}` },
    });
    if (!res.ok) return false;

    const data = await res.json() as { accessJwt?: string; refreshJwt?: string; did?: string; handle?: string };
    if (data?.accessJwt && data?.did) {
      const next: AuthSession = {
        accessJwt: data.accessJwt,
        refreshJwt: data.refreshJwt || _sessionToken.refreshJwt,
        did: data.did,
        handle: data.handle || _sessionToken.handle,
      };
      _sessionToken = next;
      try { localStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(next)); } catch { /* quota */ }
      return true;
    }
  } catch { /* network error */ }

  return false;
}

// --- Init auth (parse callback first, then resolve session) ---
export function initAuth(): AuthSession | null {
  const callbackSession = parseAuthCallback();
  if (callbackSession) {
    _sessionToken = callbackSession;
    return callbackSession;
  }

  const existing = getSession();
  if (existing) {
    _sessionToken = existing;
    return existing;
  }

  _sessionToken = null;
  return null;
}
