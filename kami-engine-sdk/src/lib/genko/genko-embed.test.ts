import { describe, it, expect } from 'vitest';
import { genkoEmbedHTML } from './genko-embed';

describe('genkoEmbedHTML — output structure', () => {
  const html = genkoEmbedHTML('Mangaka', 'mng4k4x1');

  it('returns valid HTML document', () => {
    expect(html).toContain('<!DOCTYPE html>');
    expect(html).toContain('</html>');
    expect(html).toContain('<canvas id="draw">');
  });

  it('includes app name and nanoid', () => {
    expect(html).toContain('Mangaka');
    expect(html).toContain('mng4k4x1');
  });

  it('includes WebGPU shader code', () => {
    expect(html).toContain('@vertex fn vs');
    expect(html).toContain('@fragment fn fs');
  });
});

describe('genkoEmbedHTML — AT URI deep-link (regression)', () => {
  const html = genkoEmbedHTML('Mangaka', 'mng4k4x1');

  it('includes parseAtUriFromPath function', () => {
    expect(html).toContain('parseAtUriFromPath');
    expect(html).toContain('/at/');
  });

  it('includes resolveAtUri function', () => {
    expect(html).toContain('resolveAtUri');
  });

  it('AT URI path regex matches correct segments', () => {
    // The regex should match /at/{authority}/{collection}/{rkey}
    expect(html).toContain("match(/^\\/at\\/([^/]+)\\/([^/]+)\\/(.+)$/)");
  });

  it('skips localStorage when AT URI path present', () => {
    expect(html).toContain("startsWith('/at/')");
  });

  it('_initDone guard prevents node overwrite during AT URI load', () => {
    // Critical regression: resolveAtUri must temporarily set _initDone=false
    // so deserializeDoc→loadPage→saveCurrentPage doesn't wipe new doc's nodes
    expect(html).toContain('prev=_initDone');
    expect(html).toContain('_initDone=false');
  });

  it('handles project collection type', () => {
    expect(html).toContain("collection.endsWith('.project')");
    expect(html).toContain('buildProjectTocDoc');
  });
});

describe('genkoEmbedHTML — image URL support (regression)', () => {
  const html = genkoEmbedHTML('Mangaka', 'mng4k4x1');

  it('supports _genImageUrl for URL-based images', () => {
    expect(html).toContain('_genImageUrl');
  });

  it('renders URL-based images with src=URL (not base64 prefix)', () => {
    // ai-image nodes should use _genImageUrl directly, not prepend data:image/jpeg;base64,
    expect(html).toContain("_genImageUrl||('data:image/jpeg;base64,'");
  });

  it('backward compat: panel-level _genImage still works', () => {
    expect(html).toContain("type==='panel'&&(o._genImage||o._genImageUrl)");
  });
});

describe('genkoEmbedHTML — auth integration (regression)', () => {
  const html = genkoEmbedHTML('Mangaka', 'mng4k4x1');

  it('uses camelCase redirectUrl param (not redirect_url)', () => {
    expect(html).toContain("'?redirectUrl='");
    expect(html).not.toContain("'?redirect_url='");
  });

  it('parses #auth= hash fragment from authn.gftd.ai callback', () => {
    expect(html).toContain("#auth=");
    expect(html).toContain("location.hash.startsWith('#auth=')");
  });

  it('cleans URL after auth callback (preserves pathname)', () => {
    expect(html).toContain('location.origin+location.pathname');
  });
});

describe('genkoEmbedHTML — link node type', () => {
  const html = genkoEmbedHTML('Mangaka', 'mng4k4x1');

  it('supports link node type in allNodes', () => {
    expect(html).toContain("type==='link'");
    expect(html).toContain('linkTitle');
  });

  it('link nodes navigate via location.href', () => {
    expect(html).toContain('location.href=href');
  });

  it('link node navigation integrated in select handler (not separate)', () => {
    // Regression: separate data-href handler was overwritten by data-si handler
    expect(html).toContain("closest('[data-href]')");
  });

  it('renders text/link overlays on canvas', () => {
    expect(html).toContain("o.type!=='text'&&o.type!=='link'");
  });
});

describe('genkoEmbedHTML — document persistence', () => {
  const html = genkoEmbedHTML('Mangaka', 'mng4k4x1');

  it('uses R2-based saveDocument XRPC', () => {
    expect(html).toContain('ai.gftd.mangaka.saveDocument');
  });

  it('uses R2-based loadDocument XRPC', () => {
    expect(html).toContain('ai.gftd.mangaka.loadDocument');
  });

  it('uses R2-based listDocuments XRPC', () => {
    expect(html).toContain('ai.gftd.mangaka.listDocuments');
  });

  it('uses R2-based loadProject XRPC for project AT URIs', () => {
    expect(html).toContain('ai.gftd.mangaka.loadProject');
  });
});
