<script lang="ts">
  /**
   * AuthStatus.svelte — Tiny fixed auth indicator for Genko manga editor.
   * Shows DID when logged in or a Sign In button otherwise.
   */

  interface Props {
    session: { accessJwt: string; did: string } | null;
    onsignin: () => void;
  }

  let { session, onsignin }: Props = $props();

  let truncatedDid = $derived(
    session ? (session.did.length > 24 ? session.did.slice(0, 24) + '...' : session.did) : '',
  );
</script>

<div class="auth-status">
  {#if session}
    <span class="logged-in">Logged in ({truncatedDid})</span>
  {:else}
    <button class="signin-btn" onclick={onsignin}>Sign In</button>
  {/if}
</div>

<style>
  .auth-status {
    display: inline-flex;
    align-items: center;
    font-family: 'Nunito', sans-serif;
    font-size: 11px;
  }

  .logged-in {
    color: #666;
    background: rgba(240, 234, 214, 0.8);
    padding: 2px 8px;
    border-radius: 4px;
  }

  .signin-btn {
    padding: 2px 10px;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #f0ead6;
    font-size: 10px;
    font-family: 'Nunito', sans-serif;
    cursor: pointer;
  }

  .signin-btn:hover {
    background: #e6dfc8;
  }
</style>
