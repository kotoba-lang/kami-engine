<script lang="ts">
  /**
   * ChatPanel.svelte — Right chat panel (280px) for Genko manga editor.
   * Dark-themed panel with actor member chips, scrollable message list, and input area.
   */

  interface Member {
    displayName: string;
    style: string;
    role: string;
  }

  interface Message {
    sender: string;
    text: string;
    isUser: boolean;
  }

  interface Props {
    members: Member[];
    messages: Message[];
    onsend: (text: string) => void;
    onactorclick: (style: string) => void;
  }

  let { members, messages, onsend, onactorclick }: Props = $props();

  let inputText = $state('');
  let messagesEnd: HTMLDivElement | undefined = $state();

  /** Deterministic color from string hash for member chip circles. */
  function chipColor(s: string): string {
    let hash = 0;
    for (let i = 0; i < s.length; i++) {
      hash = s.charCodeAt(i) + ((hash << 5) - hash);
    }
    const h = Math.abs(hash) % 360;
    return `hsl(${h}, 55%, 55%)`;
  }

  function handleSend() {
    const trimmed = inputText.trim();
    if (!trimmed) return;
    onsend(trimmed);
    inputText = '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  $effect(() => {
    // Auto-scroll on new messages.
    if (messages.length && messagesEnd) {
      messagesEnd.scrollIntoView({ behavior: 'smooth' });
    }
  });
</script>

<div class="chat-panel">
  <div class="header">Mangaka AI</div>

  <div class="members">
    {#each members as member}
      <button
        class="chip"
        onclick={() => onactorclick(member.style)}
        title={member.role}
      >
        <span class="chip-dot" style="background:{chipColor(member.style)}"></span>
        <span class="chip-name">{member.displayName}</span>
      </button>
    {/each}
  </div>

  <div class="messages">
    {#each messages as msg}
      <div class="msg" class:user={msg.isUser}>
        <span class="msg-sender">{msg.sender}</span>
        <span class="msg-text">{msg.text}</span>
      </div>
    {/each}
    <div bind:this={messagesEnd}></div>
  </div>

  <div class="input-area">
    <textarea
      class="input"
      rows="2"
      placeholder="Ask Mangaka AI..."
      bind:value={inputText}
      onkeydown={handleKeydown}
    ></textarea>
    <button class="send-btn" onclick={handleSend}>Send</button>
  </div>
</div>

<style>
  .chat-panel {
    width: 280px;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: #1a1a1f;
    color: #e0e0e0;
    font-family: 'Nunito', sans-serif;
    font-size: 12px;
    flex-shrink: 0;
  }

  .header {
    padding: 10px 12px;
    font-size: 14px;
    font-weight: 700;
    border-bottom: 1px solid #2a2a30;
    color: #f0ead6;
  }

  .members {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 8px;
    border-bottom: 1px solid #2a2a30;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border: 1px solid #333;
    border-radius: 12px;
    background: #252528;
    color: #ccc;
    font-size: 10px;
    font-family: 'Nunito', sans-serif;
    cursor: pointer;
  }

  .chip:hover {
    background: #333;
  }

  .chip-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .chip-name {
    white-space: nowrap;
  }

  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .msg {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    border-radius: 8px;
    background: #252528;
  }

  .msg.user {
    background: #2a3040;
    align-self: flex-end;
    max-width: 90%;
  }

  .msg-sender {
    font-size: 10px;
    font-weight: 700;
    color: #999;
  }

  .msg-text {
    font-size: 12px;
    line-height: 1.4;
    word-break: break-word;
  }

  .input-area {
    display: flex;
    gap: 4px;
    padding: 8px;
    border-top: 1px solid #2a2a30;
  }

  .input {
    flex: 1;
    resize: none;
    border: 1px solid #333;
    border-radius: 6px;
    background: #252528;
    color: #e0e0e0;
    font-size: 12px;
    font-family: 'Nunito', sans-serif;
    padding: 6px 8px;
  }

  .input::placeholder {
    color: #666;
  }

  .send-btn {
    padding: 0 12px;
    border: none;
    border-radius: 6px;
    background: #f0ead6;
    color: #1a1a1f;
    font-size: 11px;
    font-weight: 700;
    font-family: 'Nunito', sans-serif;
    cursor: pointer;
    align-self: flex-end;
    height: 28px;
  }

  .send-btn:hover {
    background: #e6dfc8;
  }
</style>
