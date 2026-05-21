import { describe, it, expect, beforeEach, vi } from 'vitest';
import { getSessionToken, initAuth, authHeaders, setSessionToken } from './auth.svelte';

describe('auth store', () => {
  beforeEach(() => {
    setSessionToken(null);
    try { localStorage.clear(); } catch (error) { console.warn('[silent-fail] auth.test.ts: localStorage.clear unsupported', error); }
    try { localStorage.removeItem('gftd_session'); } catch (error) { console.warn('[silent-fail] auth.test.ts: localStorage.removeItem unsupported', error); }
  });

  it('getSessionToken returns null initially', () => {
    expect(getSessionToken()).toBeNull();
  });

  it('setSessionToken stores session', () => {
    setSessionToken({ accessJwt: 'jwt1', refreshJwt: 'ref1', did: 'did:web:test', handle: 'test' });
    expect(getSessionToken()?.accessJwt).toBe('jwt1');
  });

  it('authHeaders includes Authorization when session exists', () => {
    setSessionToken({ accessJwt: 'mytoken', refreshJwt: '', did: '', handle: '' });
    const h = authHeaders();
    expect(h['Authorization']).toBe('Bearer mytoken');
  });

  it('authHeaders has no Authorization when no session', () => {
    const h = authHeaders();
    expect(h['Authorization']).toBeUndefined();
  });

  it('initAuth reads from localStorage', () => {
    try {
      localStorage.setItem('gftd_session', JSON.stringify({ accessJwt: 'stored', refreshJwt: '', did: 'did:web:ls', handle: '' }));
    } catch (error) {
      console.warn('[silent-fail] auth.test.ts: localStorage.setItem unsupported', error);
      return; /* jsdom localStorage not fully supported */
    }
    initAuth();
    expect(getSessionToken()?.accessJwt).toBe('stored');
  });
});

describe('auth store — regression: redirect param name', () => {
  it('redirectToAuth uses camelCase redirectUrl (not redirect_url)', async () => {
    // This was a bug: Genko sent redirect_url but authn.gftd.ai expects redirectUrl
    const { redirectToAuth } = await import('./auth.svelte');
    // Mock location
    const originalHref = Object.getOwnPropertyDescriptor(window, 'location');
    let capturedUrl = '';
    Object.defineProperty(window, 'location', {
      value: { href: '', origin: 'https://mangaka.gftd.ai', pathname: '/at/test' },
      writable: true,
      configurable: true,
    });
    const origAssign = window.location.href;
    Object.defineProperty(window.location, 'href', {
      set(v: string) { capturedUrl = v; },
      get() { return capturedUrl; },
      configurable: true,
    });

    redirectToAuth('mng4k4x1');
    expect(capturedUrl).toContain('redirectUrl=');
    expect(capturedUrl).not.toContain('redirect_url=');

    if (originalHref) Object.defineProperty(window, 'location', originalHref);
  });
});
