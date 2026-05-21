<script lang="ts">
  /**
   * FloatingTools.svelte — Bottom floating toolbar for Genko manga editor.
   * Pill-shaped bar with tool modes, brush types, color picker, size slider, undo/redo.
   */

  interface Props {
    activeMode: string;
    activeBrush: string;
    brushColor: string;
    brushSize: number;
    fukidashiShape?: string;
    textFont?: string;
    textSize?: number;
    textColor?: string;
    textStyle?: string;
    onmodechange: (mode: string) => void;
    onbrushchange: (brush: string) => void;
    oncolorchange: (color: string) => void;
    onsizechange: (size: number) => void;
    onundo: () => void;
    onredo: () => void;
    onpanelpreset?: (pid: string) => void;
    onfukidashishape?: (shape: string) => void;
    onaddfukidashi?: () => void;
    ontextfont?: (font: string) => void;
    ontextsize?: (size: number) => void;
    ontextcolor?: (color: string) => void;
    ontextstyle?: (style: string) => void;
    onaddtext?: () => void;
    onaddsfx?: () => void;
  }

  let {
    activeMode,
    activeBrush,
    brushColor,
    brushSize,
    fukidashiShape = 'normal',
    textFont = 'gothic',
    textSize = 5,
    textColor = '#000000',
    textStyle = 'normal',
    onmodechange,
    onbrushchange,
    oncolorchange,
    onsizechange,
    onundo,
    onredo,
    onpanelpreset,
    onfukidashishape,
    onaddfukidashi,
    ontextfont,
    ontextsize,
    ontextcolor,
    ontextstyle,
    onaddtext,
    onaddsfx,
  }: Props = $props();

  const toolModes = [
    { id: 'draw', icon: '\u270F\uFE0F' },
    { id: 'select', icon: '\uD83D\uDD32' },
    { id: 'panel', icon: '\u25FB\uFE0F' },
    { id: 'tone', icon: '\u25A4' },
    { id: 'fukidashi', icon: '\uD83D\uDCAC' },
    { id: 'text', icon: 'T' },
  ];

  const brushTypes = [
    { id: 'fine', label: 'F', title: 'Fine' },
    { id: 'pen', label: 'P', title: 'Pen' },
    { id: 'marker', label: 'M', title: 'Marker' },
    { id: 'brush', label: 'B', title: 'Brush' },
    { id: 'flat', label: 'Fl', title: 'Flat' },
    { id: 'eraser', label: 'E', title: 'Eraser' },
  ];

  const panelPresets = [
    { id: '1', label: '1', title: '1コマ (full)' },
    { id: '2h', label: '2H', title: '2コマ (横)' },
    { id: '3h', label: '3H', title: '3コマ (横)' },
    { id: '4h', label: '4H', title: '4コマ (4コマ漫画)' },
    { id: '2x2', label: '2×2', title: '2×2 grid' },
    { id: 'lshape', label: 'L', title: 'L字レイアウト' },
    { id: 'action', label: 'Act', title: 'アクション (3+1+2)' },
  ];

  const fukidashiShapes = [
    { id: 'normal', label: '○', title: 'セリフ (楕円)' },
    { id: 'thought', label: '◌', title: '思考 (破線)' },
    { id: 'shout', label: '✸', title: '叫び (ジャギー)' },
    { id: 'whisper', label: '⋯', title: 'ささやき (点線)' },
  ];

  const textFonts = [
    { id: 'gothic', label: 'ゴシック', css: '"Noto Sans JP", "Hiragino Kaku Gothic ProN", sans-serif' },
    { id: 'mincho', label: '明朝', css: '"Noto Serif JP", "Hiragino Mincho ProN", serif' },
    { id: 'maru', label: '丸ゴ', css: '"M PLUS Rounded 1c", "Mochiy Pop One", sans-serif' },
    { id: 'handwritten', label: '手書き', css: '"Yusei Magic", "Klee One", "Caveat", cursive' },
    { id: 'sfx', label: 'SFX', css: '"Reggae One", "Bungee", "Mochiy Pop One", sans-serif' },
  ];

  const textStyles = [
    { id: 'normal', label: '|', title: '通常' },
    { id: 'bold', label: 'B', title: '太字' },
    { id: 'italic', label: 'I', title: '斜体' },
    { id: 'bolditalic', label: 'BI', title: '太字斜体' },
  ];
</script>

<div class="floating-tools">
  <div class="section modes">
    {#each toolModes as mode}
      <button
        class="tool-btn"
        class:active={activeMode === mode.id}
        onclick={() => onmodechange(mode.id)}
        title={mode.id}
      >{mode.icon}</button>
    {/each}
  </div>

  <div class="divider"></div>

  <div class="section brushes">
    {#each brushTypes as brush}
      <button
        class="brush-btn"
        class:active={activeBrush === brush.id}
        onclick={() => onbrushchange(brush.id)}
        title={brush.title}
        aria-label={brush.title}
      >{brush.label}</button>
    {/each}
  </div>

  <div class="divider"></div>

  <div class="section controls">
    <input
      type="color"
      class="color-picker"
      value={brushColor}
      oninput={(e) => oncolorchange(e.currentTarget.value)}
    />
    <input
      type="range"
      class="size-slider"
      min="0.5"
      max="20"
      step="0.5"
      value={brushSize}
      oninput={(e) => onsizechange(parseFloat(e.currentTarget.value))}
    />
    <span class="size-label">{brushSize}</span>
  </div>

  <div class="divider"></div>

  <div class="section history">
    <button class="tool-btn" onclick={onundo} title="Undo">↩</button>
    <button class="tool-btn" onclick={onredo} title="Redo">↪</button>
  </div>
</div>

{#if activeMode === 'panel' && onpanelpreset}
  <div class="floating-sub">
    <span class="sub-label">コマ:</span>
    {#each panelPresets as p}
      <button class="brush-btn" onclick={() => onpanelpreset?.(p.id)} title={p.title}>{p.label}</button>
    {/each}
  </div>
{/if}

{#if activeMode === 'fukidashi' && onfukidashishape}
  <div class="floating-sub">
    <span class="sub-label">吹き出し:</span>
    {#each fukidashiShapes as s}
      <button class="brush-btn" class:active={fukidashiShape === s.id} onclick={() => onfukidashishape?.(s.id)} title={s.title}>{s.label}</button>
    {/each}
    {#if onaddfukidashi}
      <button class="brush-btn add-btn" onclick={() => onaddfukidashi?.()} title="挿入">＋挿入</button>
    {/if}
  </div>
{/if}

{#if activeMode === 'text'}
  <div class="floating-sub">
    <span class="sub-label">文字:</span>
    <select class="font-select" value={textFont} onchange={(e) => ontextfont?.(e.currentTarget.value)} title="フォント">
      {#each textFonts as f}
        <option value={f.id}>{f.label}</option>
      {/each}
    </select>
    {#each textStyles as s}
      <button class="brush-btn" class:active={textStyle === s.id} onclick={() => ontextstyle?.(s.id)} title={s.title}>{s.label}</button>
    {/each}
    <input
      type="range"
      class="size-slider"
      min="2"
      max="40"
      step="0.5"
      value={textSize}
      oninput={(e) => ontextsize?.(parseFloat(e.currentTarget.value))}
      title="サイズ (mm)"
    />
    <span class="size-label">{textSize}mm</span>
    <input
      type="color"
      class="color-picker"
      value={textColor}
      oninput={(e) => ontextcolor?.(e.currentTarget.value)}
      title="文字色"
    />
    {#if onaddtext}
      <button class="brush-btn add-btn" onclick={() => onaddtext?.()} title="文字を挿入">＋文字</button>
    {/if}
    {#if onaddsfx}
      <button class="brush-btn add-btn sfx-btn" onclick={() => onaddsfx?.()} title="SFX を挿入 (効果音、大きな文字)">＋SFX</button>
    {/if}
  </div>
{/if}

<style>
  .floating-tools {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: #fff;
    border-radius: 20px;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.15);
    z-index: 10;
    font-family: 'Nunito', sans-serif;
    font-size: 12px;
  }

  .section {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .divider {
    width: 1px;
    height: 20px;
    margin: 0 2px;
    background: #e0e0e0;
  }

  .tool-btn {
    width: 28px;
    height: 28px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    font-size: 14px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .tool-btn:hover {
    background: #f0ead6;
  }

  .tool-btn.active {
    background: #f0ead6;
    border-color: #c8b888;
  }

  .brush-btn {
    min-width: 24px;
    height: 24px;
    padding: 0 4px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    font-size: 11px;
    font-weight: 700;
    font-family: 'Nunito', sans-serif;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .brush-btn:hover {
    background: #f0ead6;
  }

  .brush-btn.active {
    background: #f0ead6;
    border-color: #c8b888;
    font-weight: 700;
  }

  .color-picker {
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 50%;
    padding: 0;
    cursor: pointer;
    background: transparent;
  }

  .color-picker::-webkit-color-swatch-wrapper {
    padding: 2px;
  }

  .color-picker::-webkit-color-swatch {
    border-radius: 50%;
    border: 1px solid #ccc;
  }

  .size-slider {
    width: 56px;
    height: 4px;
    accent-color: #c8b888;
  }

  .size-label {
    font-size: 10px;
    color: #888;
    min-width: 16px;
    text-align: center;
  }

  /* Sub-panel for contextual mode options */
  .floating-sub {
    position: fixed;
    bottom: 60px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: #fff;
    border-radius: 16px;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.15);
    z-index: 10;
    font-family: 'Nunito', 'Noto Sans JP', sans-serif;
    font-size: 11px;
  }
  .sub-label {
    font-size: 10px;
    color: #666;
    font-weight: 700;
    padding-right: 4px;
  }
  .font-select {
    height: 22px;
    border: 1px solid #ccc;
    border-radius: 4px;
    font-size: 11px;
    background: #fff;
    cursor: pointer;
    padding: 0 4px;
  }
  .add-btn {
    background: #f0ead6;
    border-color: #c8b888;
    min-width: 44px;
    margin-left: 2px;
  }
  .add-btn:hover { background: #e6dec0; }
  .sfx-btn {
    background: #ffe8a0;
    border-color: #d4a040;
    color: #663300;
  }
  .sfx-btn:hover { background: #ffd870; }
</style>
