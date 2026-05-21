import { kamiTrackpadHTML } from '../trackpad/trackpad-embed.js';

/** KAMI Engine Genko SDK — WebGPU pentab drawing canvas for manga creation.
 *
 * Generates a self-contained HTML page with:
 * - WebGPU renderer with zoom/pan viewport
 * - Multi-page document model with node tree panel
 * - Brush engine (pressure/tilt-aware, 6 brush types)
 * - 原稿用紙 (genkouyoushi) templates (B4 manga, 4-koma)
 * - Panel (コマ割り) tool with presets
 * - Tone, fukidashi, text overlay tools
 * - PDS persistence + localStorage auto-save
 * - Auth integration (authn.gftd.ai cross-subdomain SSO — ADR-0024)
 * - Unified node tree with drag-drop nesting
 *
 * @param name - Display name for the document/app
 * @param nanoid - Unique nanoid identifier for the app instance
 * @returns Complete HTML string for the manga editor
 */
export function genkoEmbedHTML(name: string, nanoid: string): string {
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no">
<title>${name}</title>
<style>
:root{--nt-w:220px;--mp-w:280px}
*{margin:0;padding:0;box-sizing:border-box}
html,body{width:100%;height:100%;overflow:hidden;background:#f0ead6;font-family:'Nunito',system-ui,sans-serif;touch-action:none}
canvas#draw{display:block;cursor:default;position:fixed;top:36px;left:var(--nt-w);width:calc(100% - var(--nt-w) - var(--mp-w,280px));height:calc(100% - 36px)}
/* Top bar: minimal */
.topbar{position:fixed;top:0;left:var(--nt-w);right:var(--mp-w,280px);height:36px;background:rgba(255,255,255,0.92);backdrop-filter:blur(8px);display:flex;align-items:center;gap:6px;padding:0 10px;z-index:10;border-bottom:1px solid #e0e0e0;font-size:11px}
.topbar .title{color:#333;font-size:13px;font-weight:700;margin-right:auto}
.topbar select,.topbar button{background:#fff;border:1px solid #ccc;color:#555;font-size:10px;padding:3px 8px;border-radius:6px;cursor:pointer;font-weight:600}
.topbar button:hover{background:#f5f5f5}
/* Bottom floating toolbar */
.btbar{position:fixed;bottom:16px;left:50%;transform:translateX(calc(-50% + var(--nt-w)/2));z-index:18;display:flex;align-items:center;gap:4px;padding:6px 10px;background:rgba(40,40,50,0.92);backdrop-filter:blur(12px);border-radius:16px;box-shadow:0 4px 20px rgba(0,0,0,0.3)}
.btbar .tb{width:40px;height:40px;border:none;border-radius:12px;cursor:pointer;display:flex;align-items:center;justify-content:center;background:transparent;color:#ccc;font-size:18px;transition:all .12s;padding:0}
.btbar .tb:hover{background:rgba(255,255,255,0.12);color:#fff}
.btbar .tb:active{transform:scale(0.9)}
.btbar .tb.act{background:rgba(224,96,144,0.25);color:#f0a0c0}
.btbar .sep2{width:1px;height:28px;background:rgba(255,255,255,0.15);margin:0 2px}
.btbar input[type=color]{width:32px;height:32px;border:2px solid rgba(255,255,255,0.2);border-radius:10px;cursor:pointer;background:transparent;padding:0}
.btbar input[type=range]{width:50px;accent-color:#e06090}
.btbar .sz{color:#888;font-size:10px;min-width:16px;text-align:center}
.btbar .blbl{color:#777;font-size:8px;position:absolute;bottom:2px;pointer-events:none;font-weight:700}
/* Tool option panels */
.panel{position:fixed;bottom:76px;left:50%;transform:translateX(-50%);width:220px;background:rgba(40,40,50,0.95);backdrop-filter:blur(12px);border:1px solid rgba(255,255,255,0.1);border-radius:12px;padding:10px;z-index:19;display:none;font-size:11px;box-shadow:0 4px 20px rgba(0,0,0,0.4);color:#ccc}
.panel.show{display:flex;flex-direction:column;gap:2px}
.panel label{display:block;margin:2px 0;font-weight:600;color:#999;font-size:10px}
.panel select,.panel input{width:100%;margin:1px 0 4px;padding:3px 6px;border:1px solid rgba(255,255,255,0.15);border-radius:6px;font-size:11px;background:rgba(255,255,255,0.08);color:#ddd}
.panel button{width:100%;padding:5px;margin:1px 0;border:1px solid rgba(255,255,255,0.12);border-radius:6px;background:rgba(255,255,255,0.06);cursor:pointer;font-weight:600;font-size:11px;color:#ccc}
.panel button:hover{background:rgba(255,255,255,0.12)}
.panel button.sel{border-color:#e06090;color:#f0a0c0;background:rgba(224,96,144,0.15)}
.panel p{color:#777}
.status{position:fixed;bottom:8px;right:12px;color:rgba(0,0,0,0.25);font-size:10px;z-index:10;pointer-events:none;font-weight:600}
/* Node Tree Panel */
.nt{position:fixed;top:0;left:0;bottom:0;width:var(--nt-w);background:#faf9f6;border-right:1px solid #ddd;z-index:20;display:flex;flex-direction:column;font-size:11px;transition:width .15s;overflow:hidden;user-select:none}
.nt.collapsed{--nt-w:28px}
.nt-hdr{display:flex;align-items:center;padding:4px 8px;height:36px;border-bottom:1px solid #eee;font-weight:700;color:#555;gap:6px;background:rgba(255,255,255,0.9)}
.nt-hdr span{flex:1;overflow:hidden;white-space:nowrap}
.nt-hdr button{background:none;border:none;cursor:pointer;font-size:13px;color:#888;padding:2px}
.nt-body{flex:1;overflow-y:auto;padding:2px 0}
.nt-pg{margin:0}
.nt-pg-hdr{display:flex;align-items:center;padding:4px 8px;cursor:pointer;font-weight:700;color:#555;gap:4px;border-bottom:1px solid #f0f0f0}
.nt-pg-hdr:hover{background:#f0f0f0}
.nt-pg-hdr.act{background:#fff0f5;color:#e06090}
.nt-pg-hdr .del{font-size:9px;color:#c00;opacity:0;margin-left:auto;cursor:pointer}
.nt-pg-hdr:hover .del{opacity:0.5}
.nt-nd{display:flex;align-items:center;padding:3px 8px 3px 18px;cursor:pointer;gap:4px;color:#444}
.nt-nd:hover{background:#eee}
.nt-nd.sel{background:#ddeeff}
.nt-nd .eye{font-size:9px;cursor:pointer;opacity:0.6;width:16px;text-align:center}
.nt-nd .eye.off{opacity:0.2}
.nt-nd .nm{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.nt-nd .ndel{font-size:9px;color:#c00;opacity:0;cursor:pointer}
.nt-nd:hover .ndel{opacity:0.7}
.nt-add{padding:6px 8px}
.nt-add button{width:100%;padding:5px;border:1px dashed #ccc;background:transparent;border-radius:6px;cursor:pointer;font-size:11px;font-weight:600;color:#999}
.nt-add button:hover{background:#f5f5f5;border-color:#aaa}
.nt.collapsed .nt-body,.nt.collapsed .nt-hdr span{display:none}
/* Project selector */
.proj-sel{padding:4px 8px;border-bottom:1px solid #eee;background:#fff}
.proj-sel select{width:100%;font-size:11px;padding:4px 6px;border:1px solid #ddd;border-radius:6px;font-weight:600;color:#555;background:#fff;cursor:pointer}
.proj-sel select:focus{border-color:#e06090;outline:none}
.proj-sel .proj-acts{display:flex;gap:4px;margin-top:4px}
.proj-sel .proj-acts button{flex:1;padding:3px;border:1px dashed #ccc;background:transparent;border-radius:4px;cursor:pointer;font-size:10px;font-weight:600;color:#999}
.proj-sel .proj-acts button:hover{background:#f5f5f5;border-color:#aaa}
/* Right chat panel (ChatGPT-style) */
.mp{position:fixed;top:0;right:0;bottom:0;width:280px;background:#1a1a1f;border-left:1px solid #333;z-index:20;display:flex;flex-direction:column;font-size:12px;overflow:hidden;user-select:none}
.mp-hdr{padding:6px 12px;height:36px;border-bottom:1px solid #2a2a2f;font-weight:700;color:#ccc;display:flex;align-items:center;background:#1a1a1f;gap:6px}
.mp-hdr span{flex:1}
/* Context chips (actors) */
.mp-ctx{display:flex;flex-wrap:wrap;gap:4px;padding:6px 10px;border-bottom:1px solid #2a2a2f}
.mp-chip{display:flex;align-items:center;gap:4px;padding:3px 8px;border-radius:16px;background:#2a2a30;color:#aaa;font-size:10px;font-weight:600;cursor:pointer;border:1px solid #333;transition:all .1s}
.mp-chip:hover{background:#333;color:#ddd;border-color:#555}
.mp-chip.active{background:#3a3040;color:#e0a0c0;border-color:#e06090}
.mp-chip .chip-ava{width:16px;height:16px;border-radius:50%;font-size:7px;display:flex;align-items:center;justify-content:center;color:#fff;font-weight:700;flex-shrink:0}
/* Chat messages */
.mp-msgs{flex:1;overflow-y:auto;padding:8px 10px;display:flex;flex-direction:column;gap:6px}
.mp-msg{padding:8px 10px;border-radius:12px;font-size:11px;line-height:1.4;max-width:95%;word-wrap:break-word}
.mp-msg.user{background:#2a3a50;color:#d0e0f0;align-self:flex-end;border-bottom-right-radius:4px}
.mp-msg.ai{background:#2a2a30;color:#ccc;align-self:flex-start;border-bottom-left-radius:4px}
.mp-msg.sys{background:transparent;color:#666;font-size:10px;align-self:center;text-align:center;font-style:italic}
.mp-msg .msg-name{font-size:9px;font-weight:700;margin-bottom:2px;opacity:0.7}
/* Input area */
.mp-input{padding:8px 10px;border-top:1px solid #2a2a2f}
.mp-input-box{display:flex;align-items:flex-end;background:#2a2a30;border:1px solid #444;border-radius:20px;padding:4px 4px 4px 12px;transition:border-color .15s}
.mp-input-box:focus-within{border-color:#e06090}
.mp-input-box textarea{flex:1;background:transparent;border:none;outline:none;color:#ddd;font-size:11px;font-family:inherit;resize:none;max-height:80px;min-height:18px;line-height:1.4;padding:4px 0}
.mp-input-box textarea::placeholder{color:#666}
.mp-input-box button{width:30px;height:30px;border-radius:50%;border:none;cursor:pointer;display:flex;align-items:center;justify-content:center;flex-shrink:0;font-size:14px;transition:all .1s}
.mp-input-box .send-btn{background:#e06090;color:#fff}
.mp-input-box .send-btn:hover{background:#d05080}
.mp-input-box .send-btn:disabled{background:#444;color:#666;cursor:default}
.mp-input-box .ctx-btn{background:transparent;color:#888;font-size:18px}
.mp-input-box .ctx-btn:hover{color:#ccc}
/* AI gen indicator */
.ai-gen-badge{position:absolute;top:2px;right:2px;background:rgba(96,144,224,0.9);color:#fff;font-size:8px;padding:1px 4px;border-radius:3px;font-weight:700;pointer-events:none;z-index:6}
.nt-nd.ai-node{border-left:2px solid #6090e0}
</style>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Nunito:wght@600;700&family=Noto+Serif+JP:wght@700&family=Noto+Sans+JP:wght@700&display=swap" rel="stylesheet">
</head><body>
<!-- Node Tree Panel (left) -->
<div id="ntPanel" class="nt">
  <div class="nt-hdr"><button id="ntToggle" title="Toggle panel">◀</button><span>${name}</span></div>
  <div class="proj-sel">
    <select id="projSelect"><option value="">-- Select Project --</option></select>
    <div class="proj-acts">
      <button id="projNew">+ New</button>
      <button id="projRefresh">Refresh</button>
    </div>
    <div id="projNewForm" style="display:none;margin-top:4px">
      <input id="projNewName" type="text" placeholder="Project name..." style="width:100%;font-size:11px;padding:4px 6px;border:1px solid #e06090;border-radius:6px;outline:none">
      <div style="display:flex;gap:4px;margin-top:3px">
        <button id="projNewOk" style="flex:1;padding:3px;border:1px solid #e06090;background:#fff0f5;border-radius:4px;cursor:pointer;font-size:10px;font-weight:700;color:#e06090">Create</button>
        <button id="projNewCancel" style="flex:1;padding:3px;border:1px solid #ccc;background:#fff;border-radius:4px;cursor:pointer;font-size:10px;font-weight:600;color:#999">Cancel</button>
      </div>
    </div>
  </div>
  <div id="ntBody" class="nt-body"></div>
</div>
<!-- Top bar (minimal) -->
<div class="topbar">
  <span class="title">${name}</span>
  <select id="paperType" title="Paper texture">
    <option value="ic">IC</option><option value="artcolor">Art Color</option>
    <option value="maxon">Maxon</option><option value="deleter">Deleter</option><option value="none">Plain</option>
  </select>
  <select id="youshiType" title="原稿用紙">
    <option value="b4manga" selected>B4 漫画</option><option value="b4koma">4コマ</option><option value="none">Free</option>
  </select>
  <button id="btnSavePNG" title="PNG export">PNG</button>
  <button id="btnSaveSVG" title="SVG export">SVG</button>
  <button id="btnSaveDoc" title="Save">Save</button>
  <button id="btnLoadDoc" title="Load">Load</button>
  <button id="btnExportOplog" title="Export history">History</button>
  <button id="btnReplay" title="Replay from history">Replay</button>
</div>
<!-- Bottom floating toolbar -->
<div class="btbar">
  <button class="tb" data-tool="draw" title="Draw (D)">&#9998;</button>
  <button class="tb act" data-tool="select" title="Select (V)">&#9995;</button>
  <button class="tb" data-tool="panel" title="Panel (K)">&#9634;</button>
  <button class="tb" data-tool="tone" title="Tone (T)">&#9641;</button>
  <button class="tb" data-tool="fukidashi" title="Fukidashi (F)">&#9729;</button>
  <button class="tb" data-tool="text" title="Text (X)">&#65313;</button>
  <div class="sep2"></div>
  <button class="tb" data-brush="fine" title="Fine (1)">.</button>
  <button class="tb" data-brush="pen" title="Pen (2)">~</button>
  <button class="tb" data-brush="marker" title="Marker (3)">/</button>
  <button class="tb" data-brush="brush" title="Brush (4)">*</button>
  <button class="tb" data-brush="flat" title="Flat (5)">=</button>
  <button class="tb" data-brush="eraser" title="Eraser (E)">&#10005;</button>
  <div class="sep2"></div>
  <input type="color" id="colorPicker" value="#111111" title="Color">
  <input type="range" id="sizeSlider" min="1" max="80" value="3">
  <span class="sz" id="sizeLabel">3</span>
  <div class="sep2"></div>
  <button class="tb" id="btnUndo" title="Undo (Ctrl+Z)">&#8630;</button>
  <button class="tb" id="btnRedo" title="Redo (Ctrl+Y)">&#8631;</button>
</div>
<!-- Hidden select for compat (toolModeSel used in JS) -->
<select id="toolMode" style="display:none">
  <option value="draw">Draw</option><option value="select">Select</option><option value="panel">Panel</option>
  <option value="tone">Tone</option><option value="fukidashi">Fukidashi</option><option value="text">Text</option>
</select>
<div id="tonePanel" class="panel">
  <label>Tone density</label>
  <select id="toneDensity">
    <option value="10">10%</option><option value="20">20%</option><option value="30" selected>30%</option>
    <option value="40">40%</option><option value="50">50%</option><option value="60">60%</option>
  </select>
  <label>LPI</label>
  <select id="toneLPI">
    <option value="27.5">27.5 (粗い)</option><option value="32.5" selected>32.5 (標準)</option>
    <option value="42.5">42.5 (細かい)</option><option value="55">55 (極細)</option>
  </select>
  <label>Pattern</label>
  <button class="sel" data-tone="dot">Dot</button>
  <button data-tone="line">Line</button>
  <button data-tone="cross">Cross</button>
  <button data-tone="grad">Gradient</button>
  <p style="color:#888;margin-top:4px">Drag on canvas to apply tone area</p>
</div>
<div id="fukidashiPanel" class="panel">
  <label>Fukidashi type</label>
  <button class="sel" data-fuki="oval">Oval (楕円)</button>
  <button data-fuki="jagged">Jagged (ギザギザ)</button>
  <button data-fuki="cloud">Cloud (もくもく)</button>
  <button data-fuki="square">Square (ナレーション)</button>
  <button data-fuki="wavy">Wavy (波線)</button>
  <label>Tail direction</label>
  <select id="fukiTail"><option value="bottom">Bottom</option><option value="left">Left</option><option value="right">Right</option><option value="top">Top</option><option value="none">None</option></select>
  <p style="color:#888;margin-top:4px">Drag on canvas to place fukidashi</p>
</div>
<div id="textPanel" class="panel">
  <label>Text</label>
  <input type="text" id="textInput" placeholder="セリフを入力…" value="">
  <label>Size</label>
  <input type="range" id="textSize" min="12" max="72" value="24">
  <label>Font</label>
  <select id="textFont"><option value="serif">Serif (明朝)</option><option value="sans">Gothic (ゴシック)</option><option value="manga">Manga (アンチック)</option></select>
  <label>Vertical</label>
  <select id="textDir"><option value="vertical" selected>縦書き</option><option value="horizontal">横書き</option></select>
  <p style="color:#888;margin-top:4px">Click on canvas to place text</p>
</div>
<div id="panelPanel" class="panel">
  <label>Border width (px)</label>
  <input type="range" id="panelBorderW" min="0.3" max="5" value="0.8" step="0.1">
  <label>Gutter (mm)</label>
  <input type="range" id="panelGutter" min="0" max="10" value="3">
  <label>Presets</label>
  <button data-koma="2h">2段</button>
  <button data-koma="3h">3段</button>
  <button data-koma="4h">4段</button>
  <button data-koma="2x2">2x2</button>
  <button data-koma="lshape">L字</button>
  <button data-koma="action">アクション</button>
  <p style="color:#888;margin-top:4px">Drag on canvas to draw panel, or use presets</p>
</div>
<!-- Right chat panel (ChatGPT-style) -->
<div class="mp" id="memberPanel">
  <div class="mp-hdr"><span>Mangaka AI</span><button id="mpAddBtn" style="background:none;border:none;color:#888;cursor:pointer;font-size:14px" title="Add member">+</button></div>
  <div class="mp-ctx" id="mpCtx"></div>
  <div class="mp-msgs" id="chatBody"></div>
  <div class="mp-input">
    <div class="mp-input-box">
      <button class="ctx-btn" id="ctxToggle" title="Toggle context">+</button>
      <textarea id="chatInput" rows="1" placeholder="Message..."></textarea>
      <button class="send-btn" id="chatSend" title="Send">&#9654;</button>
    </div>
  </div>
</div>
<canvas id="draw"></canvas>
<div class="status" id="status">WebGPU | ${nanoid}</div>
<script>
'use strict';
const C=document.getElementById('draw');
const TOOLBAR_H=36;
function dlog(){}
let W,H,dpr;
let needsRedraw=false,isDrawing=false,activePointerId=null;

/* === Document Model === */
let _nidC=1;
function nid(){return 'n'+(_nidC++)}
function pid(){return 'p'+Date.now().toString(36)+Math.random().toString(36).slice(2,6)}
let doc={name:'${name}',pages:[{id:pid(),name:'Page 1',youshi:{id:nid(),type:'b4manga',visible:true},nodes:[]}],activePageIdx:0};
function activePage(){return doc.pages[doc.activePageIdx]}

/* === Mutable state (declared early so localStorage restore works) === */
let strokes=[],redoStack=[],currentStroke=null;
let overlays=[];
let dragStart=null;
let activeBrush='fine',erasing=false;
let brushColor=[0.2,0.2,0.2,1],brushSize=2,brushOpacity=1,brushGamma=0.35,brushTiltEffect=0.2,brushMinWidth=0.3;
let selectedIdx=-1;
let selectDragStart=null,selectDragOffset=null;
let _resizeCorner=null,_resizeStart=null;

/* === Viewport: zoom + pan === */
let zoom=1,panX=0,panY=0;
let isPanning=false,panStartX=0,panStartY=0,panStartPX=0,panStartPY=0;

const MP_W=280; /* right chat panel width */
function resize(){
  dpr=devicePixelRatio||1;
  const ntW=parseInt(getComputedStyle(document.documentElement).getPropertyValue('--nt-w'))||220;
  W=innerWidth-ntW-MP_W;H=innerHeight-TOOLBAR_H;
  C.width=W*dpr;C.height=H*dpr;
  needsRedraw=true;
}
resize();addEventListener('resize',resize);

/* Auto-fit B4 paper on load */
function autoFitYoushi(){
  const y=YOUSHI[activeYoushi];
  if(y&&y.draw){
    const avW=C.width*0.9,avH=C.height*0.9;
    const sc=Math.min(avW/y.wMM,avH/y.hMM);
    const paperW=y.wMM*sc,paperH=y.hMM*sc;
    zoom=1;panX=(C.width-paperW)/2;panY=(C.height-paperH)/2;
    needsRedraw=true;
  }
}
setTimeout(autoFitYoushi,0);

/* === Page Management === */
let _initDone=false;
function saveCurrentPage(){
  const pg=activePage();pg.nodes=[];
  for(const s of strokes)pg.nodes.push({id:s._nid||nid(),type:'stroke',visible:s._visible!==false,data:s});
  for(const o of overlays)pg.nodes.push({id:o._nid||nid(),type:o.type,visible:o._visible!==false,data:o});
}
function loadPage(idx){
  if(_initDone)saveCurrentPage(); /* skip save during restore — strokes are empty, would overwrite saved data */
  doc.activePageIdx=idx;
  const pg=activePage();strokes=[];overlays=[];
  for(const n of pg.nodes){
    n.data._nid=n.id;n.data._visible=n.visible;
    if(n.type==='stroke')strokes.push(n.data);else overlays.push(n.data);
  }
  redoStack=[];activeYoushi=pg.youshi.type;
  document.getElementById('youshiType').value=activeYoushi;
  selectedIdx=-1;autoFitYoushi();needsRedraw=true;rebuildNT();
}

/* === Node Tree === */
const ntPanel=document.getElementById('ntPanel');
const ntBody=document.getElementById('ntBody');
let ntCollapsed=false;
document.getElementById('ntToggle').onclick=()=>{
  ntCollapsed=!ntCollapsed;
  ntPanel.classList.toggle('collapsed',ntCollapsed);
  document.documentElement.style.setProperty('--nt-w',ntCollapsed?'28px':'220px');
  document.getElementById('ntToggle').textContent=ntCollapsed?'▶':'◀';
  resize();
};
let ntTimer=null;
function rebuildNT(){if(ntTimer)return;ntTimer=setTimeout(()=>{ntTimer=null;_rebuildNT()},60)}
const collapsedNodes=new Set();

/** Check if a node is effectively visible (self + all ancestors visible). */
function isNodeVisible(nid){
  let cur=nid;const visited=new Set();
  while(cur){
    if(visited.has(cur))return true;visited.add(cur);
    const n=findByNid(cur);
    if(!n)return true;
    if(n._visible===false)return false;
    cur=n._parent||'';
  }
  return true;
}

/** Agent style → color map for node tree badges. */
const AGENT_COLORS={shonen:'#e06060',shojo:'#e060c0',seinen:'#6060e0',yonkoma:'#60c060',mecha:'#6090e0',horror:'#904090',background:'#609060',genga:'#c07020',director:'#c0a020','':'#888'};
function agentColor(agent){return AGENT_COLORS[agent]||AGENT_COLORS['']}
function agentInitials(agent){return(agent||'').slice(0,2).toUpperCase()||''}

/** Get all nodes as a unified flat list. Each has: gi, kind, nid, _parent, visible, name, ref, hasChildren, agent. */
function allNodes(){
  const out=[];
  let panelCount=0;
  strokes.forEach((s,i)=>out.push({gi:i,kind:'s',idx:i,nid:s._nid||'',par:s._parent||'',vis:s._visible!==false,
    nm:'Stroke '+(i+1),ref:s,hasChildren:false,agent:s._agent||''}));
  overlays.forEach((o,i)=>{
    const gi=strokes.length+i;
    let nm=o.type;
    if(o.type==='panel'){panelCount++;nm='Panel '+(o.panelName||panelCount)}
    else if(o.type==='ai-image')nm='AI Image'+(o._genPrompt?' ('+o._genPrompt.slice(0,12)+')':'');
    else if(o.type==='ai-desc')nm='AI Desc'+(o._genPrompt?' ('+o._genPrompt.slice(0,12)+')':'');
    else if(o.type==='prompt')nm='Prompt: '+(o.prompt||'').slice(0,16);
    else if(o.type==='text')nm='Text: '+(o.text||'').slice(0,8);
    else if(o.type==='link')nm=o.linkTitle||o.text||'Link';
    else if(o.type==='group')nm=o.groupName||'Group';
    else if(o.type==='tone')nm='Tone';
    else if(o.type==='fukidashi')nm='Fukidashi';
    out.push({gi,kind:'o',idx:i,nid:o._nid||'',par:o._parent||o._layer||'',vis:o._visible!==false,nm,ref:o,hasChildren:false,agent:o._agent||''});
  });
  /* Mark nodes that have children */
  const nids=new Set(out.map(n=>n.nid));
  out.forEach(n=>{if(n.par&&nids.has(n.par)){const p=out.find(x=>x.nid===n.par);if(p)p.hasChildren=true}});
  return out;
}

/** Find node by nid in strokes+overlays. */
function findByNid(id){
  const s=strokes.find(s=>s._nid===id);if(s)return s;
  return overlays.find(o=>o._nid===id);
}

/** Set _parent on a node by nid. */
function setParent(childNid,parentNid){
  const n=findByNid(childNid);if(n){n._parent=parentNid;n._layer=parentNid}
}

/** Check if making childNid a child of parentNid would create a cycle. */
function wouldCycle(childNid,parentNid){
  if(!parentNid)return false;
  let cur=parentNid;const visited=new Set();
  while(cur){
    if(cur===childNid)return true;
    if(visited.has(cur))return false;visited.add(cur);
    const n=findByNid(cur);cur=n?n._parent||'':'';
  }
  return false;
}

/** Reorder: move node at fromGi to position toGi within same parent group. */
function reorderNode(fromNid,toNid,position){
  /* position: 'before','after','inside' */
  const nodes=allNodes();
  const src=nodes.find(n=>n.nid===fromNid);
  const tgt=nodes.find(n=>n.nid===toNid);
  if(!src||!tgt||src.nid===tgt.nid)return;
  if(position==='inside'){
    if(wouldCycle(src.nid,tgt.nid))return;
    setParent(src.nid,tgt.nid);
  } else {
    if(wouldCycle(src.nid,tgt.par))return;
    setParent(src.nid,tgt.par);
    /* Reorder within the array */
    const srcArr=src.kind==='s'?strokes:overlays;
    const tgtArr=tgt.kind==='s'?strokes:overlays;
    if(srcArr===tgtArr){
      const fi=src.idx,ti=tgt.idx;
      const item=srcArr.splice(fi,1)[0];
      const newTi=fi<ti?ti-1:ti;
      srcArr.splice(position==='after'?newTi+1:newTi,0,item);
    }
  }
  recordOp('reparent',{childNid:fromNid,parentNid:position==='inside'?toNid:(nodes.find(n=>n.nid===toNid)||{}).par||'',position});
  needsRedraw=true;_rebuildNT();scheduleAutoSave();
}

function renderNodeRow(_,node,depth){
  const sel=node.gi===selectedIdx?' sel':'';
  const pad=8+depth*14;
  const hasKids=node.hasChildren;
  const collapsed=collapsedNodes.has(node.nid);
  const isAI=node.ref&&(node.ref.type==='ai-image'||node.ref.type==='ai-desc'||node.ref._genImage||node.ref._genDesc);
  const isLink=node.ref&&node.ref.type==='link';
  let row='<div class="nt-nd'+sel+'" data-si="'+node.gi+'" data-nid2="'+node.nid+'"'+(isLink?' data-href="'+encodeURIComponent(node.ref._href||'')+'"':'')+' draggable="true" style="padding-left:'+pad+'px'+(isAI?';background:rgba(96,144,224,0.06)':'')+(isLink?';background:rgba(64,160,96,0.08);cursor:pointer':'')+'">';
  if(hasKids){
    row+='<span class="eye" data-tgl="'+node.nid+'" style="cursor:pointer;font-size:9px;width:12px;text-align:center">'+(collapsed?'&#9654;':'&#9660;')+'</span>';
  } else {
    row+='<span style="width:12px;display:inline-block"></span>';
  }
  const vk=node.kind==='s'?'s'+node.idx:'o'+node.idx;
  row+='<span class="eye'+(node.vis?'':' off')+'" data-tv="'+vk+'">&#9673;</span>';
  /* Agent badge */
  if(node.agent){
    row+='<span style="display:inline-block;width:14px;height:14px;border-radius:50%;background:'+agentColor(node.agent)+';color:#fff;font-size:7px;text-align:center;line-height:14px;flex-shrink:0;margin-right:2px" title="'+node.agent+'">'+agentInitials(node.agent)+'</span>';
  }
  if(isLink)row+='<span style="font-size:10px;margin-right:2px;color:#409060">&#10132;</span>';
  row+='<span class="nm"'+(isLink?' style="color:#307050"':'')+'>'+node.nm+'</span>';
  if(isLink){const sub=node.ref._subtitle||'';if(sub)row+='<span style="font-size:9px;color:#888;margin-left:4px">'+sub+'</span>'}
  /* Context pin button — add node to chat context */
  const inCtx=contextNodes.has(node.nid);
  row+='<span class="ndel" data-ctx="'+node.nid+'" title="'+(inCtx?'Remove from context':'Add to context')+'" style="color:'+(inCtx?'#e06090':'#aaa')+';font-size:10px;opacity:'+(inCtx?'1':'0.5')+';cursor:pointer">'+(inCtx?'&#9733;':'&#9734;')+'</span>';
  /* Add-prompt button for panel nodes */
  if(node.ref&&node.ref.type==='panel'){
    row+='<span class="ndel" data-addprompt="'+node.nid+'" title="Add prompt" style="color:#c0a020;font-size:10px;opacity:0.7;cursor:pointer">+P</span>';
  }
  row+='<span class="ndel" data-del="'+node.nid+'">x</span></div>';
  return row;
}

function renderTree(nodes,parentNid,depth){
  let h='';
  const children=nodes.filter(n=>n.par===parentNid);
  children.forEach(node=>{
    h+=renderNodeRow(h,node,depth);
    if(node.hasChildren&&!collapsedNodes.has(node.nid)){
      h+=renderTree(nodes,node.nid,depth+1);
    }
  });
  return h;
}

function _rebuildNT(){
  let h='';
  doc.pages.forEach((pg,pi)=>{
    const act=pi===doc.activePageIdx;
    h+='<div class="nt-pg"><div class="nt-pg-hdr'+(act?' act':'')+'" data-pg="'+pi+'">';
    h+='<span>'+pg.name+'</span>';
    if(doc.pages.length>1)h+='<span class="del" data-dpg="'+pi+'">x</span>';
    h+='</div>';
    if(act){
      h+='<div class="nt-nd" data-nid="y"><span class="eye'+(pg.youshi.visible?'':' off')+'" data-ty="1">&#9673;</span>';
      h+='<span class="nm">genkouyoushi ('+pg.youshi.type+')</span></div>';
      const nodes=allNodes();
      h+=renderTree(nodes,'',0);
    }
    h+='</div>';
  });
  h+='<div class="nt-add"><button id="ntAdd">+ Page</button><button id="ntAddGrp" style="margin-top:4px">+ Group</button></div>';
  ntBody.innerHTML=h;

  /* === Events === */
  ntBody.querySelectorAll('[data-pg]').forEach(el=>{el.onclick=ev=>{
    if(ev.target.dataset.dpg!=null){
      const pi=+ev.target.dataset.dpg;if(doc.pages.length<=1)return;
      recordOp('deletePage',{pageIdx:pi});
      doc.pages.splice(pi,1);if(doc.activePageIdx>=doc.pages.length)doc.activePageIdx=doc.pages.length-1;
      loadPage(doc.activePageIdx);return;
    }
    const pi=+el.dataset.pg;if(pi!==doc.activePageIdx){recordOp('switchPage',{pageIdx:pi});loadPage(pi)}
  }});
  document.getElementById('ntAdd').onclick=()=>{saveCurrentPage();
    const pgId=pid();const pgName='Page '+(doc.pages.length+1);
    doc.pages.push({id:pgId,name:pgName,youshi:{id:nid(),type:'b4manga',visible:true},nodes:[]});
    recordOp('addPage',{pageId:pgId,name:pgName});
    loadPage(doc.pages.length-1);scheduleAutoSave();
  };
  document.getElementById('ntAddGrp').onclick=()=>{
    const gn=prompt('Group name:','Group '+(overlays.filter(o=>o.type==='group'||o.type==='layer').length+1));
    if(!gn)return;
    const gov={type:'group',groupName:gn,_nid:nid(),_visible:true,_parent:''};
    overlays.push(gov);
    recordOp('addGroup',{overlay:{...gov}});
    needsRedraw=true;_rebuildNT();scheduleAutoSave();
  };
  /* Collapse toggle */
  ntBody.querySelectorAll('[data-tgl]').forEach(el=>{el.onclick=ev=>{
    ev.stopPropagation();const id=el.dataset.tgl;
    if(collapsedNodes.has(id))collapsedNodes.delete(id);else collapsedNodes.add(id);_rebuildNT();
  }});
  /* Youshi visibility */
  ntBody.querySelectorAll('[data-ty]').forEach(el=>{el.onclick=()=>{
    activePage().youshi.visible=!activePage().youshi.visible;needsRedraw=true;_rebuildNT();
    recordOp('youshiVis',{});scheduleAutoSave();
  }});
  /* Node visibility */
  ntBody.querySelectorAll('[data-tv]').forEach(el=>{el.onclick=ev=>{
    ev.stopPropagation();const k=el.dataset.tv;
    let toggleNid='';
    if(k[0]==='s'){const i=+k.slice(1);strokes[i]._visible=!(strokes[i]._visible!==false);toggleNid=strokes[i]._nid||''}
    else{const i=+k.slice(1);const o=overlays[i];if(o){o._visible=!(o._visible!==false);toggleNid=o._nid||''}}
    recordOp('toggleVis',{nid:toggleNid});
    needsRedraw=true;_rebuildNT();scheduleAutoSave();
  }});
  /* Context pin — toggle node in chat context */
  ntBody.querySelectorAll('[data-ctx]').forEach(el=>{el.onclick=ev=>{
    ev.stopPropagation();const nid=el.dataset.ctx;
    if(contextNodes.has(nid)){contextNodes.delete(nid);addChat('system','Removed from context')}
    else{contextNodes.add(nid);addChat('system','Added to context')}
    _rebuildNT();renderContextChips();
  }});
  /* Add prompt to panel */
  ntBody.querySelectorAll('[data-addprompt]').forEach(el=>{el.onclick=ev=>{
    ev.stopPropagation();const panelNid=el.dataset.addprompt;
    const txt=prompt('Panel prompt (scene description):');
    if(!txt)return;
    const pNode={type:'prompt',prompt:txt,_nid:nid(),_visible:true,_parent:panelNid,_agent:'director'};
    overlays.push(pNode);
    recordOp('addOverlay',{overlay:{...pNode}});
    needsRedraw=true;_rebuildNT();scheduleAutoSave();
    addChat('system','Prompt added to panel: '+txt.slice(0,30));
  }});
  /* Select — panel selection enables AI artist assignment. Link nodes navigate on click. */
  ntBody.querySelectorAll('[data-si]').forEach(el=>{el.onclick=ev=>{
    if(ev.target.dataset.tv!=null||ev.target.dataset.del!=null||ev.target.dataset.tgl!=null||ev.target.dataset.addprompt!=null||ev.target.dataset.ctx!=null)return;
    /* Link node navigation — check data-href on this element or ancestor */
    const linkEl=el.closest('[data-href]')||el;
    if(linkEl.dataset.href){const href=decodeURIComponent(linkEl.dataset.href);if(href){location.href=href;return}}
    selectedIdx=+el.dataset.si;needsRedraw=true;_rebuildNT();
    if(selectedIdx>=strokes.length){const o=overlays[selectedIdx-strokes.length];
      if(o&&o.type==='panel')addChat('system','Panel selected — click Artist to draw, or +P to add prompt.')}
  }});
  /* Delete */
  ntBody.querySelectorAll('[data-del]').forEach(el=>{el.onclick=ev=>{
    ev.stopPropagation();const dnid=el.dataset.del;
    /* Unparent children */
    strokes.forEach(s=>{if(s._parent===dnid)s._parent=''});
    overlays.forEach(o=>{if(o._parent===dnid||o._layer===dnid){o._parent='';o._layer=''}});
    /* Remove from array */
    const si=strokes.findIndex(s=>s._nid===dnid);
    if(si>=0){strokes.splice(si,1);if(selectedIdx===si)selectedIdx=-1}
    else{const oi=overlays.findIndex(o=>o._nid===dnid);
      if(oi>=0){overlays.splice(oi,1);const gi=strokes.length+oi;if(selectedIdx===gi)selectedIdx=-1}}
    recordOp('deleteNode',{nid:dnid});
    needsRedraw=true;_rebuildNT();scheduleAutoSave();
  }});
  /* Drag & Drop: reorder + nesting */
  let dragNid=null;
  ntBody.querySelectorAll('[draggable]').forEach(el=>{
    el.addEventListener('dragstart',ev=>{dragNid=el.dataset.nid2||'';ev.dataTransfer.effectAllowed='move';
      el.style.opacity='0.5'});
    el.addEventListener('dragend',()=>{el.style.opacity='1'});
  });
  ntBody.querySelectorAll('.nt-nd[data-nid2]').forEach(el=>{
    el.addEventListener('dragover',ev=>{ev.preventDefault();
      const rect=el.getBoundingClientRect();const y=ev.clientY-rect.top;const h=rect.height;
      el.style.borderTop=y<h*0.25?'2px solid #e06090':'';
      el.style.borderBottom=y>h*0.75?'2px solid #e06090':'';
      el.style.background=y>=h*0.25&&y<=h*0.75?'#fff0f5':'';
    });
    el.addEventListener('dragleave',()=>{el.style.borderTop='';el.style.borderBottom='';el.style.background=''});
    el.addEventListener('drop',ev=>{
      ev.preventDefault();el.style.borderTop='';el.style.borderBottom='';el.style.background='';
      if(!dragNid)return;
      const tgtNid=el.dataset.nid2;if(dragNid===tgtNid)return;
      const rect=el.getBoundingClientRect();const y=ev.clientY-rect.top;const h=rect.height;
      if(y<h*0.25)reorderNode(dragNid,tgtNid,'before');
      else if(y>h*0.75)reorderNode(dragNid,tgtNid,'after');
      else reorderNode(dragNid,tgtNid,'inside');
      dragNid=null;
    });
  });
}

/* === Auth (authn.gftd.ai session integration; ADR-0024 T4 split) === */
const AUTH_STORAGE_KEY='gftd_session';
const AUTH_URL='https://authn.gftd.ai/sign-in';
const AUTH_REFRESH_URL='https://atproto.gftd.ai/xrpc/com.atproto.server.refreshSession';
let sessionToken=null;

function getSession(){
  /* Priority: parent window (yoro embed) → localStorage → sessionStorage */
  try{if(window.parent&&window.parent!==window&&window.parent.gftdSession)return window.parent.gftdSession}catch(e){}
  try{
    const raw=localStorage.getItem(AUTH_STORAGE_KEY);
    if(raw){const s=JSON.parse(raw);if(s&&s.accessJwt)return s}
  }catch(e){}
  try{
    const raw=sessionStorage.getItem(AUTH_STORAGE_KEY);
    if(raw){const s=JSON.parse(raw);if(s&&s.accessJwt)return s}
  }catch(e){}
  return null;
}

function authHeaders(){
  const h={'Content-Type':'application/json'};
  if(sessionToken&&sessionToken.accessJwt)h['Authorization']='Bearer '+sessionToken.accessJwt;
  return h;
}

function redirectToAuth(){
  window.location.href=AUTH_URL+'?redirectUrl='+encodeURIComponent(window.location.href)+'&app=mangaka&nanoid=${nanoid}';
}

/* Parse auth callback params (authn.gftd.ai redirects back with #auth={json} or query params) */
function parseAuthCallback(){
  /* Priority 1: hash fragment #auth={json} (authn.gftd.ai standard) */
  if(location.hash.startsWith('#auth=')){
    try{
      const session=JSON.parse(decodeURIComponent(location.hash.slice(6)));
      if(session&&session.accessJwt){
        try{localStorage.setItem(AUTH_STORAGE_KEY,JSON.stringify(session))}catch(e){}
        history.replaceState(null,'',location.origin+location.pathname);
        return session;
      }
    }catch(e){console.warn('auth hash parse:',e)}
  }
  /* Priority 2: query params (legacy/OAuth) */
  const params=new URLSearchParams(location.search);
  const accessJwt=params.get('accessJwt')||params.get('access_token');
  const refreshJwt=params.get('refreshJwt')||params.get('refresh_token');
  const did=params.get('did')||params.get('sub');
  if(accessJwt){
    const session={accessJwt,refreshJwt:refreshJwt||'',did:did||'',handle:params.get('handle')||''};
    try{localStorage.setItem(AUTH_STORAGE_KEY,JSON.stringify(session))}catch(e){}
    history.replaceState(null,'',location.origin+location.pathname);
    return session;
  }
  return null;
}

/* Initialize session */
const callbackSession=parseAuthCallback();
sessionToken=callbackSession||getSession();
const authStatusEl=document.createElement('div');
authStatusEl.style.cssText='position:fixed;top:52px;right:8px;z-index:20;font-size:10px;color:#888;font-weight:600';
document.body.appendChild(authStatusEl);
if(sessionToken){
  authStatusEl.textContent='Logged in'+(sessionToken.did?' ('+sessionToken.did.split(':').pop()+')':'');
} else {
  authStatusEl.innerHTML='<button id="authLoginBtn" style="font-size:10px;padding:3px 8px;border:1px solid #ccc;border-radius:6px;cursor:pointer;background:#fff">Sign In</button>';
  const lb=document.getElementById('authLoginBtn');if(lb)lb.onclick=redirectToAuth;
}

/* Handle 401 → refresh or redirect */
async function handleAuthError(){
  if(sessionToken&&sessionToken.refreshJwt){
    try{
      /* AT Protocol refreshSession: Authorization: Bearer <refreshJwt>, empty body. */
      const resp=await fetch(AUTH_REFRESH_URL,{
        method:'POST',headers:{'Authorization':'Bearer '+sessionToken.refreshJwt}
      });
      if(resp.ok){
        const s=await resp.json();
        if(s.accessJwt&&s.did){
          const next={accessJwt:s.accessJwt,refreshJwt:s.refreshJwt||sessionToken.refreshJwt,did:s.did,handle:s.handle||sessionToken.handle||''};
          sessionToken=next;try{localStorage.setItem(AUTH_STORAGE_KEY,JSON.stringify(next))}catch(e){}return true;
        }
      }
    }catch(e){console.warn('token refresh failed:',e)}
  }
  redirectToAuth();return false;
}

/* === Persistence (PDS + localStorage, auth-gated) === */
const STORE_KEY='mangaka-${nanoid}';
const OPLOG_KEY='mangaka-oplog-${nanoid}';
const XRPC_BASE=location.origin+'/xrpc/';
if(!doc.docId)doc.docId='doc-'+Date.now().toString(36)+Math.random().toString(36).slice(2,6);
function serializeDoc(){saveCurrentPage();return JSON.stringify(doc)}
function deserializeDoc(json){
  try{const d=JSON.parse(typeof json==='string'?json:JSON.stringify(json));
    if(d&&d.pages&&d.pages.length){doc=d;loadPage(doc.activePageIdx||0);return true}}
  catch(e){console.warn('load failed',e)}return false;
}

/* === OpLog — operation history for full replay === */
let oplog=[];
const OPLOG_MAX=5000;
try{const raw=localStorage.getItem(OPLOG_KEY);if(raw)oplog=JSON.parse(raw)||[]}catch(e){}

/** Record an operation. type: string, data: serializable object. */
function recordOp(type,data){
  const op={t:Date.now(),type,page:doc.activePageIdx,data:data||{}};
  oplog.push(op);
  if(oplog.length>OPLOG_MAX)oplog=oplog.slice(-OPLOG_MAX);
  try{localStorage.setItem(OPLOG_KEY,JSON.stringify(oplog))}catch(e){
    /* If oplog exceeds quota, trim aggressively */
    oplog=oplog.slice(-Math.floor(OPLOG_MAX/2));
    try{localStorage.setItem(OPLOG_KEY,JSON.stringify(oplog))}catch(e2){}
  }
}

/** Replay all operations from oplog onto a fresh document. Returns reconstructed doc JSON. */
function replayOplog(ops){
  /* Reset to empty doc */
  const freshDoc={name:doc.name,docId:doc.docId,pages:[{id:pid(),name:'Page 1',youshi:{id:nid(),type:'b4manga',visible:true},nodes:[]}],activePageIdx:0};
  let rStrokes=[],rOverlays=[],rRedoStack=[];
  function rActivePage(){return freshDoc.pages[freshDoc.activePageIdx]}
  function rSavePage(){
    const pg=rActivePage();pg.nodes=[];
    for(const s of rStrokes)pg.nodes.push({id:s._nid||'',type:'stroke',visible:s._visible!==false,data:s});
    for(const o of rOverlays)pg.nodes.push({id:o._nid||'',type:o.type,visible:o._visible!==false,data:o});
  }
  function rLoadPage(idx){
    rSavePage();freshDoc.activePageIdx=idx;
    const pg=rActivePage();rStrokes=[];rOverlays=[];
    for(const n of pg.nodes){n.data._nid=n.id;n.data._visible=n.visible;
      if(n.type==='stroke')rStrokes.push(n.data);else rOverlays.push(n.data)}
    rRedoStack=[];
  }

  for(const op of ops){
    const d=op.data;
    switch(op.type){
      case 'stroke':
        if(d.stroke){d.stroke._nid=d.stroke._nid||nid();d.stroke._visible=true;rStrokes.push(d.stroke);rRedoStack=[]}
        break;
      case 'addOverlay':
        if(d.overlay){d.overlay._nid=d.overlay._nid||nid();d.overlay._visible=true;rOverlays.push(d.overlay)}
        break;
      case 'deleteNode':{
        const dnid=d.nid;
        rStrokes.forEach(s=>{if(s._parent===dnid)s._parent=''});
        rOverlays.forEach(o=>{if(o._parent===dnid)o._parent=''});
        const si=rStrokes.findIndex(s=>s._nid===dnid);
        if(si>=0)rStrokes.splice(si,1);
        else{const oi=rOverlays.findIndex(o=>o._nid===dnid);if(oi>=0)rOverlays.splice(oi,1)}
        break;}
      case 'moveNode':
        if(d.nid&&d.dx!=null){
          const s=rStrokes.find(s=>s._nid===d.nid);
          if(s){for(const p of s.points){p.x+=d.dx;p.y+=d.dy}}
          else{const o=rOverlays.find(o=>o._nid===d.nid);
            if(o){if(o.x1!=null){o.x1+=d.dx;o.y1+=d.dy;o.x2+=d.dx;o.y2+=d.dy}if(o.x!=null){o.x+=d.dx;o.y+=d.dy}}}
        }
        break;
      case 'reparent':
        if(d.childNid!=null){const n=rStrokes.find(s=>s._nid===d.childNid)||rOverlays.find(o=>o._nid===d.childNid);
          if(n)n._parent=d.parentNid||''}
        break;
      case 'toggleVis':{
        const n=rStrokes.find(s=>s._nid===d.nid)||rOverlays.find(o=>o._nid===d.nid);
        if(n)n._visible=!(n._visible!==false);
        break;}
      case 'youshiVis':
        rActivePage().youshi.visible=!rActivePage().youshi.visible;break;
      case 'youshiType':
        if(d.type)rActivePage().youshi.type=d.type;break;
      case 'addPage':
        rSavePage();
        freshDoc.pages.push({id:d.pageId||pid(),name:d.name||'Page',youshi:{id:nid(),type:'b4manga',visible:true},nodes:[]});
        rLoadPage(freshDoc.pages.length-1);break;
      case 'deletePage':
        if(d.pageIdx!=null&&freshDoc.pages.length>1){
          freshDoc.pages.splice(d.pageIdx,1);
          if(freshDoc.activePageIdx>=freshDoc.pages.length)freshDoc.activePageIdx=freshDoc.pages.length-1;
          rLoadPage(freshDoc.activePageIdx)}
        break;
      case 'switchPage':
        if(d.pageIdx!=null)rLoadPage(d.pageIdx);break;
      case 'undo':
        if(rStrokes.length)rRedoStack.push(rStrokes.pop());break;
      case 'redo':
        if(rRedoStack.length)rStrokes.push(rRedoStack.pop());break;
      case 'addGroup':
        if(d.overlay)rOverlays.push(d.overlay);break;
      case 'panelPreset':
        if(d.panels){for(const p of d.panels)rOverlays.push(p)}break;
    }
  }
  rSavePage();
  return freshDoc;
}

/** Export oplog as downloadable JSON. */
function exportOplog(){
  const blob=new Blob([JSON.stringify({docId:doc.docId,name:doc.name,oplog,exportedAt:new Date().toISOString()})],{type:'application/json'});
  const a=document.createElement('a');a.href=URL.createObjectURL(blob);
  a.download='mangaka-oplog-'+Date.now()+'.json';a.click();URL.revokeObjectURL(a.href);
}

/** Import oplog from JSON and replay to reconstruct document. */
function importOplog(json){
  try{
    const d=JSON.parse(json);
    if(d.oplog&&Array.isArray(d.oplog)){
      oplog=d.oplog;
      try{localStorage.setItem(OPLOG_KEY,JSON.stringify(oplog))}catch(e){}
      const rebuilt=replayOplog(oplog);
      doc=rebuilt;loadPage(doc.activePageIdx||0);
      needsRedraw=true;rebuildNT();
      status.textContent='Replayed '+oplog.length+' operations';
      return true;
    }
  }catch(e){console.warn('oplog import failed:',e)}
  return false;
}

let _ast=null;
/** Save to localStorage immediately on every mutation; debounce PDS sync to 5s. */
function scheduleAutoSave(){
  /* localStorage: instant (no debounce) — survives reload */
  try{localStorage.setItem(STORE_KEY,serializeDoc())}catch(e){}
  /* PDS: debounced 5s */
  if(_ast)clearTimeout(_ast);_ast=setTimeout(()=>{
    saveToPDS().catch(e=>console.warn('pds auto-save:',e));
  },5000);
}

/** Authenticated XRPC POST helper. */
async function xrpc(method,body){
  const resp=await fetch(XRPC_BASE+method,{method:'POST',headers:authHeaders(),body:JSON.stringify(body)});
  if(resp.status===401){const ok=await handleAuthError();if(ok){return xrpc(method,body)}throw new Error('auth required')}
  return resp.json();
}

/** Save document to PDS via XRPC (authenticated, convo project-linked). */
async function saveToPDS(){
  const json=serializeDoc();
  try{
    const r=await xrpc('ai.gftd.mangaka.saveDocument',{docId:doc.docId,name:doc.name,document:json,convoId:activeProjectId||doc.convoId||''});
    if(r.error)console.warn('pds save:',r.error);
    else status.textContent='Saved'+(activeProjectId?' (Project)':sessionToken?' (PDS)':' (local)');
  }catch(e){console.warn('pds save:',e)}
}

/** Load document from PDS by docId (authenticated). */
async function loadFromPDS(docId){
  try{
    const r=await xrpc('ai.gftd.mangaka.loadDocument',{docId});
    if(r.error){console.warn('pds load:',r.error);return false}
    const docStr=r.document||r.value_b64;
    if(docStr&&deserializeDoc(docStr)){needsRedraw=true;status.textContent='Loaded from PDS';return true}
  }catch(e){console.warn('pds load:',e)}
  return false;
}

/** List saved documents from PDS (authenticated). */
async function listFromPDS(){
  try{return(await xrpc('ai.gftd.mangaka.listDocuments',{limit:20})).items||[]}
  catch(e){console.warn('pds list:',e);return[]}
}

/* Save button: PDS save + local file download */
document.getElementById('btnSaveDoc').onclick=async()=>{
  await saveToPDS();
  const b=new Blob([serializeDoc()],{type:'application/json'});
  const a=document.createElement('a');a.href=URL.createObjectURL(b);
  a.download=(doc.name||'manga')+'-'+Date.now()+'.json';a.click();URL.revokeObjectURL(a.href);
};

/* Load button: show PDS docs list, or load from file */
document.getElementById('btnLoadDoc').onclick=async()=>{
  const docs=await listFromPDS();
  if(docs.length>0){
    let msg='Saved documents:\\n';
    docs.forEach((d,i)=>msg+=(i+1)+'. '+(d.name||d.id)+' ('+d.createdAt+')\\n');
    msg+='\\nEnter number to load, or "file" to load from file:';
    const choice=prompt(msg);
    if(!choice)return;
    if(choice.toLowerCase()==='file'){loadFromFile();return}
    const idx=parseInt(choice)-1;
    if(idx>=0&&idx<docs.length){const loaded=await loadFromPDS(docs[idx].id);if(loaded)return}
  }
  loadFromFile();
};
function loadFromFile(){
  const inp=document.createElement('input');inp.type='file';inp.accept='.json';
  inp.onchange=()=>{const f=inp.files[0];if(!f)return;const r=new FileReader();
    r.onload=()=>{if(deserializeDoc(r.result))needsRedraw=true};r.readAsText(f)};inp.click();
}

/* Restore: localStorage on init (PDS load is async, deferred). Skip if AT URI deep-link present — resolveAtUri() handles loading. */
if(!location.pathname.startsWith('/at/')){try{const sv=localStorage.getItem(STORE_KEY);if(sv)deserializeDoc(sv)}catch(e){}}

/* === Project Management (ai.gftd.projectors) === */
const projSelect=document.getElementById('projSelect');
let projects=[];
const PROJ_CACHE_KEY='mangaka-projects-${nanoid}';
const PROJ_ACTIVE_KEY='mangaka-active-project-${nanoid}';
/* Restore active project from localStorage */
let activeProjectId='';
try{activeProjectId=localStorage.getItem(PROJ_ACTIVE_KEY)||''}catch(e){}

/** Fetch project list from PDS (convo project XRPC) + localStorage cache. */
async function loadProjects(){
  /* Restore from localStorage first (instant, even before graph catches up) */
  try{const cached=localStorage.getItem(PROJ_CACHE_KEY);if(cached){const cp=JSON.parse(cached);if(Array.isArray(cp)&&cp.length)projects=cp;renderProjectSelect()}}catch(e){}
  /* Then try graph query */
  try{
    const r=await xrpc('ai.gftd.mangaka.listProjects',{limit:50});
    const items=(r.items||[]).map(p=>{
      if(typeof p==='string'){try{p=JSON.parse(p)}catch(e){}}
      if(p.value_b64){try{const v=JSON.parse(p.value_b64);Object.assign(p,v)}catch(e){}}
      return p;
    });
    if(items.length>0){
      /* Merge: graph items + locally-created projects not yet in graph */
      const graphIds=new Set(items.map(p=>p.convoId||p.id));
      const localOnly=projects.filter(p=>(p.convoId||p.id)&&!graphIds.has(p.convoId||p.id));
      projects=[...items,...localOnly];
    }
  }catch(e){
    /* Fallback: try PDS convo project list directly */
    try{
      const r2=await fetch(XRPC_BASE+'ai.gftd.projector.listProjectConvos',{
        method:'POST',headers:authHeaders(),body:JSON.stringify({limit:50})
      });
      if(r2.ok){const d=await r2.json();const items=(d.items||d.projects||[]).map(p=>{
        if(p.value_b64){try{Object.assign(p,JSON.parse(p.value_b64))}catch(e){}}return p;
      });if(items.length>0){const ids=new Set(items.map(p=>p.convoId||p.id));const lo=projects.filter(p=>(p.convoId||p.id)&&!ids.has(p.convoId||p.id));projects=[...items,...lo]}}
    }catch(e2){console.warn('project list fallback:',e2)}
  }
  /* Cache to localStorage */
  try{localStorage.setItem(PROJ_CACHE_KEY,JSON.stringify(projects))}catch(e){}
  renderProjectSelect();
}

/** Render project dropdown. */
function renderProjectSelect(){
  let h='<option value="">-- Select Project --</option>';
  h+='<option value="__none">(No project)</option>';
  for(const p of projects){
    const id=p.convoId||p.rkey||p.vertex_id||p.id||'';
    const nm=p.name||p.displayName||p.display_name||id;
    const sel=id===activeProjectId?' selected':'';
    h+='<option value="'+id+'"'+sel+'>'+nm+'</option>';
  }
  projSelect.innerHTML=h;
}

/** Switch active project — persisted to localStorage. */
projSelect.onchange=async()=>{
  const val=projSelect.value;
  if(val==='__none'){activeProjectId='';doc.convoId='';try{localStorage.setItem(PROJ_ACTIVE_KEY,'')}catch(e){}scheduleAutoSave();return}
  if(!val)return;
  activeProjectId=val;
  doc.convoId=val;
  try{localStorage.setItem(PROJ_ACTIVE_KEY,val)}catch(e){}
  loadMembers();
  /* Try to load saved canvas document first */
  let loaded=false;
  try{
    const r=await xrpc('ai.gftd.mangaka.listDocuments',{limit:1,convoId:val});
    const items=r.items||[];
    if(items.length>0){
      const d=items[0];
      const docStr=d.document||d.value_b64;
      if(docStr&&deserializeDoc(docStr)){needsRedraw=true;loaded=true;status.textContent='Loaded project canvas'}
    }
  }catch(e){console.warn('project doc load:',e)}
  /* Load project structure data (works, characters, pages) into node tree */
  try{
    const proj=projects.find(p=>(p.convoId||p.rkey||p.vertex_id||p.id)===val);
    const projName=proj?.name||proj?.display_name||proj?.displayName||val;
    const [wR,cR]=await Promise.allSettled([
      xrpc('ai.gftd.mangaka.listWorks',{limit:20}),
      xrpc('ai.gftd.mangaka.listCharacters',{limit:50}),
    ]);
    const works=(wR.status==='fulfilled'?wR.value.items:[])||[];
    const chars=(cR.status==='fulfilled'?cR.value.items:[])||[];
    if((works.length>0||chars.length>0)&&!loaded){
      /* Build doc from project structure */
      const pages=[];
      for(const w of works){
        let wp={}; try{wp=JSON.parse(w.props||'{}')}catch(e){}
        const title=wp.title||w.display_name||w.name||w.label||'Work';
        const pgNodes=[];
        pgNodes.push({id:nid(),type:'text',visible:true,data:{type:'text',text:title,x:100,y:100,fontSize:24,fontFamily:'serif',vertical:false,_nid:nid(),_visible:true,_parent:''}});
        pages.push({id:pid(),name:title,youshi:{id:nid(),type:'b4manga',visible:true},nodes:pgNodes});
      }
      if(chars.length>0){
        const charNodes=[];
        for(const c of chars){
          let cp={}; try{cp=JSON.parse(c.props||'{}')}catch(e){}
          const cname=cp.name||c.display_name||c.name||'Character';
          const crole=cp.role||c.description||'';
          charNodes.push({id:nid(),type:'text',visible:true,data:{type:'text',text:cname+(crole?'\\n'+crole:''),x:80,y:80+charNodes.length*60,fontSize:14,fontFamily:'gothic',vertical:false,_nid:nid(),_visible:true,_parent:''}});
        }
        pages.push({id:pid(),name:'Characters ('+chars.length+')',youshi:{id:nid(),type:'b4manga',visible:true},nodes:charNodes});
      }
      if(pages.length>0){
        doc={name:projName,docId:doc.docId,convoId:val,pages,activePageIdx:0};
        loadPage(0);needsRedraw=true;rebuildNT();
        status.textContent='Loaded '+works.length+' works, '+chars.length+' characters';
      }
    }
  }catch(e){console.warn('project structure load:',e)}
  scheduleAutoSave();
};

/** Show/hide inline new project form (no prompt() — works in iframe). */
const projNewForm=document.getElementById('projNewForm');
const projNewName=document.getElementById('projNewName');
document.getElementById('projNew').onclick=()=>{
  projNewForm.style.display=projNewForm.style.display==='none'?'block':'none';
  if(projNewForm.style.display==='block'){projNewName.value='Manga '+(projects.length+1);projNewName.focus();projNewName.select()}
};
document.getElementById('projNewCancel').onclick=()=>{projNewForm.style.display='none'};
projNewName.addEventListener('keydown',e=>{if(e.key==='Enter')document.getElementById('projNewOk').click();if(e.key==='Escape')projNewForm.style.display='none'});

/** Create new project via convo project XRPC. */
document.getElementById('projNewOk').onclick=async()=>{
  const nm=projNewName.value.trim();
  if(!nm)return;
  const btn=document.getElementById('projNewOk');
  btn.textContent='Creating...';btn.disabled=true;
  try{
    const r=await xrpc('ai.gftd.mangaka.createProject',{name:nm,description:'Manga project: '+nm});
    if(r.convoId){
      activeProjectId=r.convoId;
      doc.convoId=r.convoId;
      doc.name=nm;
      try{localStorage.setItem(PROJ_ACTIVE_KEY,r.convoId)}catch(e){}
      /* Optimistic: add to local list immediately (graph index is async) */
      projects.push({convoId:r.convoId,name:nm,status:'active',createdAt:new Date().toISOString()});
      try{localStorage.setItem(PROJ_CACHE_KEY,JSON.stringify(projects))}catch(e){}
      renderProjectSelect();
      projNewForm.style.display='none';
      status.textContent='Project created: '+nm;
      scheduleAutoSave();
    } else {
      status.textContent='Project create failed';
      console.warn('createProject response:',r);
    }
  }catch(e){
    console.warn('project create:',e);
    status.textContent='Error: '+e.message;
  }finally{btn.textContent='Create';btn.disabled=false}
};

document.getElementById('projRefresh').onclick=()=>loadProjects();

/* AT URI deep-link: /at/{authority}/{collection}/{rkey} → auto-load record as document */
function parseAtUriFromPath(){
  const m=location.pathname.match(/^\\/at\\/([^/]+)\\/([^/]+)\\/(.+)$/);
  if(!m)return null;
  return{authority:m[1],collection:m[2],rkey:decodeURIComponent(m[3])};
}

/** Load and deserialize a document, handling _initDone guard. */
function safeDeserialize(docStr){
  const prev=_initDone;_initDone=false;
  const ok=deserializeDoc(docStr);
  _initDone=prev;
  if(ok){needsRedraw=true;renderPanelImages();setTimeout(()=>{needsRedraw=true;renderPanelImages()},1000)}
  return ok;
}

/** Build a TOC document from a project's document list. Single page with link nodes grouped by arc. */
function buildProjectTocDoc(project){
  const docs=project.documents||[];
  const appHost='${nanoid}.gftd.ai';
  const nodes=[];

  /* Title text */
  const titleNid=nid();
  nodes.push({id:titleNid,type:'text',visible:true,data:{type:'text',_nid:titleNid,_visible:true,
    text:project.name||'Project',x:300,y:200,fontSize:52,color:'#222',font:'sans'}});
  const descNid=nid();
  nodes.push({id:descNid,type:'text',visible:true,data:{type:'text',_nid:descNid,_visible:true,
    text:docs.length+' episodes / '+(docs.reduce((s,d)=>s+(d.pages||0),0))+' pages',x:300,y:280,fontSize:24,color:'#888',font:'sans'}});

  /* Group by arc, then create link nodes */
  const arcMap=new Map();
  for(const d of docs){
    const arc=d.arc||'Other';
    if(!arcMap.has(arc))arcMap.set(arc,[]);
    arcMap.get(arc).push(d);
  }

  let y=400;
  for(const [arc,epDocs] of arcMap){
    /* Arc group node */
    const groupNid=nid();
    nodes.push({id:groupNid,type:'group',visible:true,data:{type:'group',_nid:groupNid,_visible:true,groupName:arc}});

    /* Episode link nodes under the arc group */
    for(const d of epDocs){
      const linkNid=nid();
      const href='/at/'+appHost+'/ai.gftd.mangaka.document/'+d.docId;
      const subtitle=(d.pages||0)+'p'+(d.images?' '+d.images+'img':'');
      nodes.push({id:linkNid,type:'link',visible:true,data:{
        type:'link',_nid:linkNid,_visible:true,_parent:groupNid,
        _href:href,linkTitle:d.title||d.docId,_subtitle:subtitle,
        text:d.title||d.docId,x:320,y,fontSize:20,color:'#307050',font:'sans',
      }});
      y+=40;
    }
    y+=20;
  }

  return{name:project.name||'Project',docId:project.projectId||'proj',convoId:project.convoId||'',
    pages:[{id:pid(),name:project.name||'Project',youshi:{id:nid(),type:'b4manga',visible:true},nodes}],activePageIdx:0};
}

async function resolveAtUri(){
  const at=parseAtUriFromPath();
  if(!at)return false;
  status.textContent='Loading '+at.rkey+'...';

  /* Detect collection type from NSID */
  const isProject=at.collection.endsWith('.project');

  try{
    if(isProject){
      /* Project AT URI → load project metadata → build TOC document */
      const r=await xrpc('ai.gftd.mangaka.loadProject',{projectId:at.rkey});
      if(r.error){status.textContent='Project not found: '+at.rkey;return false}
      const tocDoc=buildProjectTocDoc(r);
      if(safeDeserialize(JSON.stringify(tocDoc))){
        status.textContent='Project: '+(r.name||at.rkey)+' ('+((r.documents||[]).length)+' episodes)';
        return true;
      }
    } else {
      /* Document AT URI → load document directly */
      const r=await xrpc('ai.gftd.mangaka.loadDocument',{docId:at.rkey});
      const docStr=r.document||r.value_b64;
      if(docStr&&safeDeserialize(docStr)){
        status.textContent='Loaded: '+at.rkey;
        return true;
      }
    }
  }catch(e){console.warn('AT URI resolve:',e)}
  status.textContent='AT URI: record not found';
  return false;
}

/* Auto-load projects on init (deferred). AT URI deep-link takes priority. */
setTimeout(async()=>{
  const atLoaded=await resolveAtUri();
  await loadProjects();
  if(atLoaded)_initDone=true;
},500);

/* === Chat Panel (ChatGPT-style, right side) === */
const mpCtx=document.getElementById('mpCtx');
const chatBody=document.getElementById('chatBody');
const chatInput=document.getElementById('chatInput');
let members=[];
let chatMessages=[];
let activeActors=new Set(['director']); /* context: which actors are active */
let contextNodes=new Set(); /* context: nids of nodes attached to chat */

/** Default drawing actor types — always available as project members. */
const DEFAULT_ACTORS=[
  {did:'did:web:mangaka.gftd.ai:mangaka:shonen',displayName:'Shonen Artist',role:'artist',isAI:true,style:'shonen'},
  {did:'did:web:mangaka.gftd.ai:mangaka:shojo',displayName:'Shojo Artist',role:'artist',isAI:true,style:'shojo'},
  {did:'did:web:mangaka.gftd.ai:mangaka:seinen',displayName:'Seinen Artist',role:'artist',isAI:true,style:'seinen'},
  {did:'did:web:mangaka.gftd.ai:mangaka:yonkoma',displayName:'4-Koma Artist',role:'artist',isAI:true,style:'yonkoma'},
  {did:'did:web:mangaka.gftd.ai:mangaka:mecha',displayName:'Mecha Artist',role:'artist',isAI:true,style:'mecha'},
  {did:'did:web:mangaka.gftd.ai:mangaka:horror',displayName:'Horror Artist',role:'artist',isAI:true,style:'horror'},
  {did:'did:web:mangaka.gftd.ai:mangaka:bg',displayName:'Background Artist',role:'artist',isAI:true,style:'background'},
  {did:'did:web:mangaka.gftd.ai:mangaka:genga',displayName:'Anime Genga Artist',role:'artist',isAI:true,style:'genga'},
  {did:'did:web:mangaka.gftd.ai:mangaka:director',displayName:'Storyboard Director',role:'director',isAI:true,style:'director'},
];

/** Fetch and render members for active project. */
async function loadMembers(){
  members=[{did:'did:web:mng4k4x1.gftd.ai',displayName:'Mangaka AI',role:'admin',isAI:true},...DEFAULT_ACTORS];
  if(activeProjectId){
    try{
      const r=await xrpc('ai.gftd.mangaka.getMembers',{convoId:activeProjectId});
      if(r.members&&r.members.length){
        const ids=new Set(r.members.map(m=>m.did));
        const extra=DEFAULT_ACTORS.filter(a=>!ids.has(a.did));
        members=[...r.members.map(m=>({...m,isAI:m.did&&m.did.includes('.gftd.ai')})),...extra];
      }
    }catch(e){console.warn('loadMembers:',e)}
  }
  renderMembers();
}

/** Render actors + context node chips. */
function renderMembers(){renderContextChips()}
function renderContextChips(){
  let h='';
  /* Actor chips */
  for(const m of members){
    if(m.role==='admin'&&!m.style)continue;
    const initials=(m.displayName||'?').slice(0,2).toUpperCase();
    const ac=agentColor(m.style||'');
    const isActive=activeActors.has(m.style);
    h+='<div class="mp-chip'+(isActive?' active':'')+'" data-mstyle="'+m.style+'" data-mdid="'+(m.did||'')+'">';
    h+='<div class="chip-ava" style="background:'+ac+'">'+initials+'</div>';
    h+=(m.displayName||m.style)+'</div>';
  }
  /* Context node chips */
  if(contextNodes.size>0){
    const nodes=allNodes();
    for(const nid of contextNodes){
      const node=nodes.find(n=>n.nid===nid);
      if(!node)continue;
      const label=node.nm.slice(0,16);
      h+='<div class="mp-chip active" data-ctxnid="'+nid+'" style="border-color:#e0a020;color:#e0c060;background:#302a1a">';
      h+='<div class="chip-ava" style="background:#c0a020;font-size:8px">N</div>';
      h+=label+'</div>';
    }
  }
  mpCtx.innerHTML=h;
  /* Click actor chip */
  mpCtx.querySelectorAll('[data-mstyle]').forEach(el=>{el.onclick=()=>{
    const style=el.dataset.mstyle;
    const m=members.find(m=>m.style===style);if(!m)return;
    if((m.role==='artist')&&selectedIdx>=strokes.length){
      const oi=selectedIdx-strokes.length;const o=overlays[oi];
      if(o&&o.type==='panel'){
        addChat(m.displayName,'Drawing in '+m.style+' style...');
        generateForPanel(oi,m.style,m.displayName);return;
      }
    }
    if(activeActors.has(style))activeActors.delete(style);else activeActors.add(style);
    renderContextChips();
  }});
  /* Click context node chip → remove */
  mpCtx.querySelectorAll('[data-ctxnid]').forEach(el=>{el.onclick=()=>{
    contextNodes.delete(el.dataset.ctxnid);_rebuildNT();renderContextChips();
  }});
}

document.getElementById('mpAddBtn').onclick=async()=>{
  const did=prompt('Member DID:');
  if(!did)return;
  const name=prompt('Display name:',did.split(':').pop()||did);
  try{
    await xrpc('ai.gftd.mangaka.addMember',{convoId:activeProjectId,memberDid:did,displayName:name});
    members.push({did,displayName:name,role:'member',isAI:did.includes('.gftd.ai'),style:''});
    renderMembers();addChat('system','Member added: '+(name||did));
  }catch(e){console.warn('addMember:',e)}
};
/* Context toggle button (show/hide chips) */
let ctxVisible=true;
document.getElementById('ctxToggle').onclick=()=>{
  ctxVisible=!ctxVisible;
  mpCtx.style.display=ctxVisible?'flex':'none';
};

/* === Chat === */
function addChat(sender,text,agentStyle){
  const m=members.find(m=>m.displayName===sender);
  chatMessages.push({sender,text,t:Date.now(),agentStyle:agentStyle||(m?m.style:'')||''});
  if(chatMessages.length>200)chatMessages=chatMessages.slice(-200);
  renderChat();
}
function renderChat(){
  let h='';
  for(const m of chatMessages.slice(-50)){
    const isSys=m.sender==='system';const isUser=m.sender==='You';
    const cls=isSys?'sys':isUser?'user':'ai';
    h+='<div class="mp-msg '+cls+'">';
    if(!isSys&&!isUser){
      const ac=agentColor(m.agentStyle||'');
      h+='<div class="msg-name" style="color:'+ac+'">'+m.sender+'</div>';
    }
    h+=m.text+'</div>';
  }
  chatBody.innerHTML=h;
  chatBody.scrollTop=chatBody.scrollHeight;
}

/** Textarea auto-resize. */
chatInput.addEventListener('input',()=>{
  chatInput.style.height='auto';
  chatInput.style.height=Math.min(chatInput.scrollHeight,80)+'px';
});
/** Send on Enter (Shift+Enter for newline). */
document.getElementById('chatSend').onclick=sendChat;
chatInput.addEventListener('keydown',e=>{
  if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();sendChat()}
});
/** Build context string from pinned nodes for LLM. */
function buildContextStr(){
  if(contextNodes.size===0)return '';
  const nodes=allNodes();const parts=[];
  for(const nid of contextNodes){
    const n=nodes.find(n=>n.nid===nid);if(!n)continue;
    const ref=n.ref;
    let desc=n.nm;
    if(ref.type==='prompt')desc='Prompt: '+(ref.prompt||'');
    else if(ref.type==='panel')desc='Panel'+(ref.panelName?' '+ref.panelName:'')+' (rect)';
    else if(ref.type==='ai-desc')desc='AI Description: '+(ref._genDesc||'').slice(0,100);
    else if(ref.type==='text')desc='Text: '+(ref.text||'');
    else if(ref.type==='stroke')desc='Stroke ('+((ref.points||[]).length)+' points)';
    parts.push('['+n.nm+'] '+desc);
  }
  return parts.length?'\\n\\nContext nodes:\\n'+parts.join('\\n'):'';
}

async function sendChat(){
  const txt=chatInput.value.trim();if(!txt)return;
  chatInput.value='';chatInput.style.height='auto';
  addChat('You',txt);

  /* Check if addressing an artist with a panel selected */
  const artist=members.find(m=>m.role==='artist'&&txt.toLowerCase().includes(m.style));
  if(artist&&selectedIdx>=strokes.length){
    const oi=selectedIdx-strokes.length;
    if(overlays[oi]&&overlays[oi].type==='panel'){
      addChat(artist.displayName,'Drawing in '+artist.style+' style...');
      generateForPanel(oi,artist.style,artist.displayName);
      return;
    }
  }

  /* Director: story → koma-wari + per-panel prompts */
  const isStoryRequest=txt.length>30||txt.toLowerCase().includes('story')||txt.toLowerCase().includes('director');
  const director=members.find(m=>m.style==='director');
  if(isStoryRequest&&director){
    addChat('Storyboard Director','Analyzing story and creating panel layout...');
    try{
      const r=await xrpc('ai.gftd.mangaka.storyboard',{story:txt+buildContextStr()});
      if(r.panels&&r.panels.length>0){
        /* Create panels + prompts from storyboard */
        const rect=getYoushiInnerRect();
        if(rect){
          const g=(3)*dpr*2;const bw=0.8;
          const panelH=(rect.b-rect.t-g*(r.panels.length-1))/r.panels.length;
          r.panels.forEach((p,i)=>{
            const y1=rect.t+i*(panelH+g);const y2=y1+panelH;
            const panelOv={type:'panel',x1:rect.l,y1,x2:rect.r,y2,borderW:bw,_nid:nid(),_visible:true,_parent:''};
            overlays.push(panelOv);
            /* Attach prompt as child node */
            const promptOv={type:'prompt',prompt:p.prompt||p.description||'',_nid:nid(),_visible:true,_parent:panelOv._nid,_agent:'director'};
            overlays.push(promptOv);
            addChat('Storyboard Director','Panel '+(i+1)+': '+p.prompt);
          });
          recordOp('panelPreset',{panels:r.panels,preset:'storyboard'});
          needsRedraw=true;rebuildNT();scheduleAutoSave();
          addChat('Storyboard Director','Created '+r.panels.length+' panels with prompts. Click an Artist to draw each panel.');
        }
      } else if(r.description){
        addChat('Storyboard Director',r.description);
      } else {
        addChat('Storyboard Director','Could not parse storyboard: '+(r.error||''));
      }
    }catch(e){addChat('system','Director error: '+e.message)}
    return;
  }

  /* General chat → LLM response (with context nodes) */
  try{
    const ctxStr=buildContextStr();
    const r=await xrpc('ai.gftd.mangaka.generateImage',{prompt:txt+ctxStr,style:'manga'});
    if(r.description)addChat('Mangaka AI',r.description);
    else addChat('Mangaka AI',r.error||'Ready to draw!');
  }catch(e){addChat('system','Error: '+e.message)}
}

/** Generate AI content for a specific panel. */
async function generateForPanel(oi,style,artistName){
  const o=overlays[oi];if(!o||o.type!=='panel')return;
  /* Use panel's prompt node if exists, otherwise last user message */
  const panelPrompt=overlays.find(c=>c.type==='prompt'&&c._parent===o._nid);
  const promptText=panelPrompt?.prompt||chatMessages.filter(m=>m.sender==='You').slice(-1)[0]?.text||'manga scene';
  status.textContent='AI generating...';
  try{
    const r=await xrpc('ai.gftd.mangaka.generateImage',{prompt:promptText,style});
    if(r.image){
      /* Create AI image as a child node of the panel */
      const aiNode={type:'ai-image',_nid:nid(),_visible:true,_parent:o._nid,_agent:style,
        _genImage:r.image,_genPrompt:promptText,_artistName:artistName,
        x1:o.x1,y1:o.y1,x2:o.x2,y2:o.y2,createdAt:new Date().toISOString()};
      overlays.push(aiNode);
      /* Also store on panel for backward compat */
      o._genImage=r.image;o._genPrompt=promptText;o._agent=o._agent||style;
      addChat(artistName,'Done! Image generated and added to node tree.');
    } else if(r.description){
      /* Create AI description as a child node of the panel */
      const descNode={type:'ai-desc',_nid:nid(),_visible:true,_parent:o._nid,_agent:style,
        _genDesc:r.description,_genPrompt:promptText,_artistName:artistName,
        x1:o.x1,y1:o.y1,x2:o.x2,y2:o.y2,createdAt:new Date().toISOString()};
      overlays.push(descNode);
      o._genDesc=r.description;o._genPrompt=promptText;o._agent=o._agent||style;
      addChat(artistName,r.description);
    } else {
      addChat(artistName,'Generation failed: '+(r.error||'unknown'));
    }
    needsRedraw=true;renderPanelImages();rebuildNT();
    recordOp('aiGenImage',{nid:o._nid,prompt:promptText,style,artist:artistName});
    scheduleAutoSave();
    status.textContent='Panel updated by '+artistName;
  }catch(e){addChat('system','Error: '+e.message);status.textContent='AI error'}
}

/* Load members on init + when project changes */
setTimeout(()=>loadMembers(),800);

/* === AI Image Generation — draw into panel === */
const imgLayer=document.createElement('div');
imgLayer.style.cssText='position:fixed;top:0;left:0;width:100%;height:100%;pointer-events:none;z-index:4';
document.body.appendChild(imgLayer);

/** Render AI-generated images/descriptions as overlays (from ai-image/ai-desc nodes + panel._genImage compat). */
function renderPanelImages(){
  imgLayer.innerHTML='';
  /* Pre-compute sc for paper-relative (mm-unit) overlays. */
  const _y=YOUSHI[activeYoushi];
  const _sc=(_y&&_y.draw)?Math.min(C.width*0.9/_y.wMM,C.height*0.9/_y.hMM):1;
  const _us=(o)=>o._unit==='mm'?_sc:1;
  for(const o of overlays){
    if(!isNodeVisible(o._nid))continue;
    /* Render ai-image nodes */
    if(o.type==='ai-image'&&(o._genImage||o._genImageUrl)){
      const r=C.getBoundingClientRect();const s=_us(o);
      const x1=(Math.min(o.x1,o.x2)*s*zoom+panX)/dpr+r.left;
      const y1=(Math.min(o.y1,o.y2)*s*zoom+panY)/dpr+r.top;
      const x2=(Math.max(o.x1,o.x2)*s*zoom+panX)/dpr+r.left;
      const y2=(Math.max(o.y1,o.y2)*s*zoom+panY)/dpr+r.top;
      const el=document.createElement('img');
      el.src=o._genImageUrl||('data:image/jpeg;base64,'+o._genImage);
      el.style.cssText='position:absolute;left:'+x1+'px;top:'+y1+'px;width:'+(x2-x1)+'px;height:'+(y2-y1)+'px;object-fit:cover;pointer-events:none;opacity:0.85';
      imgLayer.appendChild(el);
      /* Agent badge */
      const badge=document.createElement('div');
      const ac=agentColor(o._agent||'');
      badge.style.cssText='position:absolute;left:'+(x2-42)+'px;top:'+(y1+2)+'px;background:'+ac+';color:#fff;font-size:8px;padding:1px 4px;border-radius:3px;font-weight:700;pointer-events:none;z-index:6';
      badge.textContent=(o._agent||'AI').slice(0,6);
      imgLayer.appendChild(badge);
    }
    /* Render ai-desc nodes */
    if(o.type==='ai-desc'&&o._genDesc){
      const r=C.getBoundingClientRect();const s=_us(o);
      const x1=(Math.min(o.x1,o.x2)*s*zoom+panX)/dpr+r.left;
      const y1=(Math.min(o.y1,o.y2)*s*zoom+panY)/dpr+r.top;
      const x2=(Math.max(o.x1,o.x2)*s*zoom+panX)/dpr+r.left;
      const y2=(Math.max(o.y1,o.y2)*s*zoom+panY)/dpr+r.top;
      const el=document.createElement('div');
      el.style.cssText='position:absolute;left:'+x1+'px;top:'+y1+'px;width:'+(x2-x1)+'px;height:'+(y2-y1)+'px;pointer-events:none;overflow:hidden;padding:6px;font-size:9px;color:#555;line-height:1.3;background:rgba(255,255,255,0.8)';
      el.textContent=o._genDesc;
      imgLayer.appendChild(el);
      const badge=document.createElement('div');
      const ac=agentColor(o._agent||'');
      badge.style.cssText='position:absolute;left:'+(x2-48)+'px;top:'+(y1+2)+'px;background:'+ac+';color:#fff;font-size:8px;padding:1px 4px;border-radius:3px;font-weight:700;pointer-events:none;z-index:6';
      badge.textContent=(o._agent||'Desc').slice(0,6);
      imgLayer.appendChild(badge);
    }
    /* Render prompt nodes (small label at top of parent panel) */
    if(o.type==='prompt'&&o._parent){
      const parent=overlays.find(p=>p._nid===o._parent);
      if(parent&&parent.x1!=null){
        const r=C.getBoundingClientRect();const s=_us(parent);
        const x1=(Math.min(parent.x1,parent.x2)*s*zoom+panX)/dpr+r.left;
        const y1=(Math.min(parent.y1,parent.y2)*s*zoom+panY)/dpr+r.top;
        const x2=(Math.max(parent.x1,parent.x2)*s*zoom+panX)/dpr+r.left;
        const el=document.createElement('div');
        el.style.cssText='position:absolute;left:'+x1+'px;top:'+(y1-1)+'px;width:'+(x2-x1)+'px;padding:2px 4px;font-size:8px;color:#c0a020;background:rgba(255,250,220,0.9);border-bottom:1px solid #e0d080;pointer-events:none;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;font-weight:600;z-index:6';
        el.textContent='P: '+o.prompt;
        imgLayer.appendChild(el);
      }
    }
    /* Backward compat: panel-level _genImage/_genImageUrl (legacy) */
    if(o.type==='panel'&&(o._genImage||o._genImageUrl)&&!overlays.some(c=>c.type==='ai-image'&&c._parent===o._nid)){
      const r=C.getBoundingClientRect();const s=_us(o);
      const x1=(Math.min(o.x1,o.x2)*s*zoom+panX)/dpr+r.left;
      const y1=(Math.min(o.y1,o.y2)*s*zoom+panY)/dpr+r.top;
      const x2=(Math.max(o.x1,o.x2)*s*zoom+panX)/dpr+r.left;
      const y2=(Math.max(o.y1,o.y2)*s*zoom+panY)/dpr+r.top;
      const el=document.createElement('img');
      el.src=o._genImageUrl||('data:image/jpeg;base64,'+o._genImage);
      el.style.cssText='position:absolute;left:'+x1+'px;top:'+y1+'px;width:'+(x2-x1)+'px;height:'+(y2-y1)+'px;object-fit:cover;pointer-events:none;opacity:0.85';
      imgLayer.appendChild(el);
    }
  }
}

/** Request AI image generation for the selected panel. */
async function generatePanelImage(panelIdx){
  const o=overlays[panelIdx];
  if(!o||o.type!=='panel')return;
  const promptText=prompt('Describe the illustration for this panel:','manga scene');
  if(!promptText)return;
  status.textContent='Generating image...';
  try{
    const w=Math.abs(o.x2-o.x1)/dpr;const h=Math.abs(o.y2-o.y1)/dpr;
    const r=await xrpc('ai.gftd.mangaka.generateImage',{
      prompt:promptText,style:'manga',
      width:Math.min(Math.round(w)||512,1024),
      height:Math.min(Math.round(h)||512,1024),
    });
    if(r.image){
      o._genImage=r.image;
      o._genPrompt=promptText;
      needsRedraw=true;renderPanelImages();
      recordOp('aiGenImage',{nid:o._nid,prompt:promptText});
      scheduleAutoSave();
      status.textContent='Image generated for panel';
    } else if(r.description){
      /* Text description fallback — show as overlay text on the panel */
      o._genDesc=r.description;
      o._genPrompt=promptText;
      needsRedraw=true;renderPanelImages();
      recordOp('aiGenDesc',{nid:o._nid,prompt:promptText,description:r.description});
      scheduleAutoSave();
      status.textContent='AI description generated (image gen unavailable)';
    } else {
      status.textContent='Image gen: '+(r.error||r.status||'failed');
      console.warn('generateImage:',r);
    }
  }catch(e){
    status.textContent='Image gen error: '+e.message;
    console.warn('generateImage:',e);
  }
}

/* Double-click on panel in select mode → generate image */
C.addEventListener('dblclick',e=>{
  if(toolMode!=='select')return;
  const cc=clientToCanvas(e.clientX,e.clientY);const w=screenToWorld(cc.x,cc.y);
  const hit=hitTest(w.x,w.y);
  if(hit>=strokes.length){
    const oi=hit-strokes.length;
    if(overlays[oi]&&overlays[oi].type==='panel')generatePanelImage(oi);
  }
});

/** Client (mouse) coords → canvas-local device px. */
function clientToCanvas(cx,cy){const r=C.getBoundingClientRect();return{x:(cx-r.left)*dpr,y:(cy-r.top)*dpr}}
/** Canvas device px → world (document) px, accounting for zoom+pan. */
function screenToWorld(sx,sy){return{x:(sx-panX)/zoom,y:(sy-panY)/zoom}}
/** World px → normalized GPU coords [0..1]. */
function worldToGPU(wx,wy,cw,ch){return{u:(wx*zoom+panX)/cw,v:(wy*zoom+panY)/ch}}

/* === KAMI Trackpad SDK integration (replaces basic wheel handler) === */
${kamiTrackpadHTML()}

/* Middle mouse or Space+drag to pan */
C.addEventListener('pointerdown',e=>{
  if(e.button===1||(e.button===0&&e.altKey)){
    e.preventDefault();isPanning=true;
    panStartX=panX;panStartY=panY;const _pc=clientToCanvas(e.clientX,e.clientY);panStartPX=_pc.x;panStartPY=_pc.y;
    try{C.setPointerCapture(e.pointerId)}catch(ex){}
    return;
  }
},true); /* capture phase: run before tool handlers */

C.addEventListener('pointermove',e=>{
  if(isPanning){
    const _mc=clientToCanvas(e.clientX,e.clientY);
    panX=panStartX+(_mc.x-panStartPX);
    panY=panStartY+(_mc.y-panStartPY);
    needsRedraw=true;return;
  }
},true);

C.addEventListener('pointerup',e=>{
  if(isPanning){isPanning=false;return}
},true);

/* --- Pressure curve for XP-Pen Deco ---
 * Per-brush gamma. XP-Pen reports low initial pressure (0.008-0.05). */
function applyCurve(p){return Math.max(brushMinWidth,Math.pow(Math.max(0,Math.min(1,p)),brushGamma))}

/* === GENKO SDK: Paper textures (原稿用紙テクスチャ) === */
const PAPERS={
  ic:       {gamma:0.42, jitter:0, minWidth:0.3, label:'IC — ツルツル、安定の高品質'},
  artcolor: {gamma:0.50, jitter:0.3, minWidth:0.4, label:'Art Color — 優しくて快適な描き心地'},
  maxon:    {gamma:0.35, jitter:1.2, minWidth:0.2, label:'Maxon — 驚異的ガリガリ'},
  deleter:  {gamma:0.44, jitter:0.5, minWidth:0.35, label:'Deleter — IC の下位互換'},
  none:     {gamma:0.45, jitter:0, minWidth:0.3, label:'Plain'},
};
let paperJitter=0;
const paperSel=document.getElementById('paperType');
paperSel.onchange=()=>{const p=PAPERS[paperSel.value];if(p){paperJitter=p.jitter;needsRedraw=true}};

/* === GENKO SDK: Youshi (原稿用紙) — 週刊少年ジャンプ特製漫画原稿用紙再現 ===
 * B4判 (257×364mm)。実寸ベース (mm)。
 * 基準線 (mm, 用紙左上原点):
 *   用紙全体:       0,0 → 257,364
 *   裁ち落とし枠:   18,18 → 239,346  (仕上がり B5: 182×257mm + α)
 *   基本枠 (外枠):  25,27 → 232,337  (印刷領域)
 *   内枠:           53.5,72 → 203.5,292 (150×220mm テキスト安全域)
 *   目盛り:         用紙4辺に 5mm 刻み
 *   トンボ:         裁ち落とし枠の4隅 + 4辺中央
 */
const YOUSHI={
  none:{draw:false},
  b4manga:{draw:true, wMM:257,hMM:364,
    /* 裁ち落とし枠 (trim/bleed) */
    trimL:18, trimT:18, trimR:239, trimB:346,
    /* 基本枠 (outer frame) */
    outerL:25, outerT:27, outerR:232, outerB:337,
    /* 内枠 (inner safe frame) = 150x220mm centered */
    innerL:53.5, innerT:72, innerR:203.5, innerB:292,
    /* 目盛り間隔 */
    rulerStep:5, rulerSmall:1,
  },
  b4koma:{draw:true, wMM:257,hMM:364,
    trimL:18, trimT:18, trimR:239, trimB:346,
    outerL:25, outerT:27, outerR:232, outerB:337,
    innerL:53.5, innerT:72, innerR:203.5, innerB:292,
    rulerStep:5, rulerSmall:1, koma:4,
  },
};
let activeYoushi='b4manga';
const youshiSel=document.getElementById('youshiType');
youshiSel.onchange=()=>{
  activeYoushi=youshiSel.value;
  activePage().youshi.type=activeYoushi;
  autoFitYoushi();
  needsRedraw=true;rebuildNT();recordOp('youshiType',{type:activeYoushi});scheduleAutoSave();
};

function tessellateYoushi(out,cw,ch){
  const y=YOUSHI[activeYoushi];if(!y||!y.draw)return;
  const sc=Math.min(cw*0.9/y.wMM,ch*0.9/y.hMM);
  /* Colors matching the Jump manuscript paper */
  const CB=[0.55,0.78,0.92]; /* 水色 (light blue) for guidelines */
  const CG=[0.7,0.7,0.7];    /* gray for outer marks */

  /* --- Helper functions --- */
  function rect(x1,y1,x2,y2,r,g,b,a){
    const u1=x1*sc/cw,v1=y1*sc/ch,u2=x2*sc/cw,v2=y2*sc/ch;
    out.push(u1,v1,r,g,b,a, u2,v1,r,g,b,a, u2,v2,r,g,b,a);
    out.push(u1,v1,r,g,b,a, u2,v2,r,g,b,a, u1,v2,r,g,b,a);
  }
  function line(x1,y1,x2,y2,r,g,b,a,wmm){
    const px1=x1*sc,py1=y1*sc,px2=x2*sc,py2=y2*sc;
    const dx=px2-px1,dy=py2-py1,len=Math.sqrt(dx*dx+dy*dy)||1;
    /* Minimum half-width = 0.5 device px so mm-based lines stay visible at any sc */
    const hw=Math.max(wmm*sc*0.5, 0.5);
    const nx=-dy/len*hw,ny=dx/len*hw;
    out.push((px1+nx)/cw,(py1+ny)/ch,r,g,b,a,(px1-nx)/cw,(py1-ny)/ch,r,g,b,a,(px2+nx)/cw,(py2+ny)/ch,r,g,b,a);
    out.push((px1-nx)/cw,(py1-ny)/ch,r,g,b,a,(px2-nx)/cw,(py2-ny)/ch,r,g,b,a,(px2+nx)/cw,(py2+ny)/ch,r,g,b,a);
  }

  /* --- 1. Paper background (white) --- */
  rect(0,0,y.wMM,y.hMM, 0.98,0.98,0.97,1);

  /* --- 2. Ruler margin area (淡い水色帯) — 4辺 --- */
  const rm=15; /* ruler margin width mm */
  rect(0,0,y.wMM,rm, 0.88,0.94,0.97,1); /* top */
  rect(0,y.hMM-rm,y.wMM,y.hMM, 0.88,0.94,0.97,1); /* bottom */
  rect(0,rm,rm,y.hMM-rm, 0.88,0.94,0.97,1); /* left */
  rect(y.wMM-rm,rm,y.wMM,y.hMM-rm, 0.88,0.94,0.97,1); /* right */

  /* --- 3. Ruler tick marks (目盛り) — 5mm大, 1mm小 --- */
  for(let mm=0;mm<=y.wMM;mm+=y.rulerSmall){
    const big=(mm%y.rulerStep===0);
    const len=big?4:2;const w=big?0.3:0.15;
    /* Top ruler */
    line(mm,0,mm,len, ...CB,0.7,w);
    /* Bottom ruler */
    line(mm,y.hMM,mm,y.hMM-len, ...CB,0.7,w);
  }
  for(let mm=0;mm<=y.hMM;mm+=y.rulerSmall){
    const big=(mm%y.rulerStep===0);
    const len=big?4:2;const w=big?0.3:0.15;
    /* Left ruler */
    line(0,mm,len,mm, ...CB,0.7,w);
    /* Right ruler */
    line(y.wMM,mm,y.wMM-len,mm, ...CB,0.7,w);
  }

  /* --- 4. 裁ち落とし枠 (trim frame) — 水色 thin --- */
  const tl=y.trimL,tt=y.trimT,tr=y.trimR,tb=y.trimB;
  line(tl,tt,tr,tt,...CB,0.6,0.3);line(tr,tt,tr,tb,...CB,0.6,0.3);
  line(tr,tb,tl,tb,...CB,0.6,0.3);line(tl,tb,tl,tt,...CB,0.6,0.3);

  /* --- 5. 基本枠 (outer frame) — 水色 medium --- */
  const ol=y.outerL,ot=y.outerT,or_=y.outerR,ob=y.outerB;
  line(ol,ot,or_,ot,...CB,0.8,0.5);line(or_,ot,or_,ob,...CB,0.8,0.5);
  line(or_,ob,ol,ob,...CB,0.8,0.5);line(ol,ob,ol,ot,...CB,0.8,0.5);

  /* --- 6. 内枠 (inner frame) — 水色 thick --- */
  const il=y.innerL,it=y.innerT,ir=y.innerR,ib=y.innerB;
  line(il,it,ir,it,...CB,0.9,0.7);line(ir,it,ir,ib,...CB,0.9,0.7);
  line(ir,ib,il,ib,...CB,0.9,0.7);line(il,ib,il,it,...CB,0.9,0.7);

  /* --- 7. トンボ (trim marks) — 4隅 + 4辺中央 --- */
  const tmLen=10; /* mm */
  /* Corner tombo: L-shape at each trim corner */
  [[tl,tt,-1,-1],[tr,tt,1,-1],[tr,tb,1,1],[tl,tb,-1,1]].forEach(([cx,cy,dx,dy])=>{
    line(cx,cy,cx-dx*tmLen,cy,0,0,0,0.5,0.25);
    line(cx,cy,cx,cy-dy*tmLen,0,0,0,0.5,0.25);
    /* Cross at corner */
    line(cx-2,cy,cx+2,cy,0,0,0,0.4,0.15);
    line(cx,cy-2,cx,cy+2,0,0,0,0.4,0.15);
  });
  /* Center tombo: cross marks at midpoints of each trim side */
  const midX=(tl+tr)/2,midY=(tt+tb)/2;
  [[midX,tt,0,-1],[midX,tb,0,1],[tl,midY,-1,0],[tr,midY,1,0]].forEach(([cx,cy,dx,dy])=>{
    if(dx===0){line(cx-3,cy,cx+3,cy,0,0,0,0.4,0.2);line(cx,cy,cx,cy-dy*tmLen,0,0,0,0.4,0.2)}
    else{line(cx,cy-3,cx,cy+3,0,0,0,0.4,0.2);line(cx,cy,cx-dx*tmLen,cy,0,0,0,0.4,0.2)}
  });

  /* --- 8. 4コマ分割線 --- */
  if(y.koma){
    const komaH=(ib-it)/y.koma;
    for(let k=1;k<y.koma;k++){
      const ky=it+komaH*k;
      line(il,ky,ir,ky,...CG,0.4,0.4);
    }
  }

  /* --- 9. 内枠⇔基本枠間のセンターマーク (十字) --- */
  const cxI=(il+ir)/2,cyI=(it+ib)/2;
  line(cxI-3,it-4,cxI+3,it-4,...CB,0.5,0.2);line(cxI,it-7,cxI,it-1,...CB,0.5,0.2);
  line(cxI-3,ib+4,cxI+3,ib+4,...CB,0.5,0.2);line(cxI,ib+1,cxI,ib+7,...CB,0.5,0.2);
  line(il-4,cyI-3,il-4,cyI+3,...CB,0.5,0.2);line(il-7,cyI,il-1,cyI,...CB,0.5,0.2);
  line(ir+4,cyI-3,ir+4,cyI+3,...CB,0.5,0.2);line(ir+1,cyI,ir+7,cyI,...CB,0.5,0.2);
}

/* === GENKO SDK: Tool mode (draw/select/panel/tone/fukidashi/text) === */
let toolMode='select'; /* default: select (mouse-friendly) */
const toolModeSel=document.getElementById('toolMode');
const tonePanel=document.getElementById('tonePanel');
const fukidashiPanel=document.getElementById('fukidashiPanel');
const textPanel=document.getElementById('textPanel');
const panelPanel=document.getElementById('panelPanel');
function setToolMode(mode){
  toolMode=mode;toolModeSel.value=mode;
  tonePanel.classList.toggle('show',mode==='tone');
  fukidashiPanel.classList.toggle('show',mode==='fukidashi');
  textPanel.classList.toggle('show',mode==='text');
  panelPanel.classList.toggle('show',mode==='panel');
  if(mode==='select'){C.style.cursor='default';selectedIdx=-1;needsRedraw=true}
  else if(mode==='draw'){C.style.cursor=erasing?'cell':'crosshair';selectedIdx=-1;needsRedraw=true}
  else{C.style.cursor='crosshair';selectedIdx=-1;needsRedraw=true}
  /* Update bottom toolbar active state */
  document.querySelectorAll('[data-tool]').forEach(b=>b.classList.toggle('act',b.dataset.tool===mode));
}
toolModeSel.onchange=()=>setToolMode(toolModeSel.value);
/* Bottom toolbar: tool buttons */
document.querySelectorAll('[data-tool]').forEach(el=>{el.addEventListener('click',()=>setToolMode(el.dataset.tool))});

/* Tone panel buttons */
let tonePattern='dot';
document.querySelectorAll('[data-tone]').forEach(el=>{
  el.addEventListener('click',()=>{
    tonePattern=el.dataset.tone;
    document.querySelectorAll('[data-tone]').forEach(b=>b.classList.toggle('sel',b===el));
  });
});

/* Fukidashi panel buttons */
let fukiType='oval';
document.querySelectorAll('[data-fuki]').forEach(el=>{
  el.addEventListener('click',()=>{
    fukiType=el.dataset.fuki;
    document.querySelectorAll('[data-fuki]').forEach(b=>b.classList.toggle('sel',b===el));
  });
});

/* === GENKO SDK: Tone rendering === */
function tessellateToneRect(out,x1,y1,x2,y2,cw,ch){
  const density=parseInt(document.getElementById('toneDensity').value)/100;
  const dotR=2*dpr;
  const spacing=(1/parseFloat(document.getElementById('toneLPI').value))*25.4*dpr*3; /* mm→px */
  const minX=Math.min(x1,x2),maxX=Math.max(x1,x2),minY=Math.min(y1,y2),maxY=Math.max(y1,y2);
  for(let gx=minX;gx<maxX;gx+=spacing){
    for(let gy=minY;gy<maxY;gy+=spacing){
      if(tonePattern==='dot'){
        const r=dotR*density;const segs=6;const cx=gx/cw,cy=gy/ch;
        for(let j=0;j<segs;j++){
          const a0=2*Math.PI*j/segs,a1=2*Math.PI*(j+1)/segs;
          out.push(cx,cy,0,0,0,density);
          out.push(cx+Math.cos(a0)*r/cw,cy+Math.sin(a0)*r/ch,0,0,0,density);
          out.push(cx+Math.cos(a1)*r/cw,cy+Math.sin(a1)*r/ch,0,0,0,density);
        }
      } else if(tonePattern==='line'){
        const hw=spacing*0.15*density;
        const lx=(gx)/cw,ly1=minY/ch,ly2=maxY/ch;
        out.push(lx-hw/cw,ly1,0,0,0,density, lx+hw/cw,ly1,0,0,0,density, lx+hw/cw,ly2,0,0,0,density);
        out.push(lx-hw/cw,ly1,0,0,0,density, lx+hw/cw,ly2,0,0,0,density, lx-hw/cw,ly2,0,0,0,density);
        break; /* only vertical pass needed per row */
      } else if(tonePattern==='cross'){
        const hw=spacing*0.1*density;
        /* vertical */
        out.push((gx-hw)/cw,minY/ch,0,0,0,density,(gx+hw)/cw,minY/ch,0,0,0,density,(gx+hw)/cw,maxY/ch,0,0,0,density);
        out.push((gx-hw)/cw,minY/ch,0,0,0,density,(gx+hw)/cw,maxY/ch,0,0,0,density,(gx-hw)/cw,maxY/ch,0,0,0,density);
        break;
      }
    }
  }
  /* Cross: horizontal pass */
  if(tonePattern==='cross'){
    const hw=spacing*0.1*density;
    for(let gy=minY;gy<maxY;gy+=spacing){
      out.push(minX/cw,(gy-hw)/ch,0,0,0,density,maxX/cw,(gy-hw)/ch,0,0,0,density,maxX/cw,(gy+hw)/ch,0,0,0,density);
      out.push(minX/cw,(gy-hw)/ch,0,0,0,density,maxX/cw,(gy+hw)/ch,0,0,0,density,minX/cw,(gy+hw)/ch,0,0,0,density);
    }
  }
  /* Gradient: density varies from left to right */
  if(tonePattern==='grad'){
    for(let gx=minX;gx<maxX;gx+=spacing){
      const t=(gx-minX)/(maxX-minX);
      const localD=t*0.8;
      for(let gy=minY;gy<maxY;gy+=spacing){
        const r=dotR*localD;if(r<0.5)continue;const segs=6;const cx=gx/cw,cy=gy/ch;
        for(let j=0;j<segs;j++){
          const a0=2*Math.PI*j/segs,a1=2*Math.PI*(j+1)/segs;
          out.push(cx,cy,0,0,0,localD);
          out.push(cx+Math.cos(a0)*r/cw,cy+Math.sin(a0)*r/ch,0,0,0,localD);
          out.push(cx+Math.cos(a1)*r/cw,cy+Math.sin(a1)*r/ch,0,0,0,localD);
        }
      }
    }
  }
}

/* === GENKO SDK: Fukidashi (吹き出し) rendering === */
function tessellateFukidashi(out,x1,y1,x2,y2,cw,ch){
  const cx=(x1+x2)/2,cy=(y1+y2)/2;
  const rx=Math.abs(x2-x1)/2,ry=Math.abs(y2-y1)/2;
  if(rx<5||ry<5)return;
  const bw=2*dpr; /* border width */
  const segs=fukiType==='jagged'?16:fukiType==='cloud'?24:32;
  const pts=[];
  for(let i=0;i<=segs;i++){
    const a=2*Math.PI*i/segs;
    let px=cx+Math.cos(a)*rx,py=cy+Math.sin(a)*ry;
    if(fukiType==='jagged'){
      const spike=(i%2===0)?1.15:0.85;
      px=cx+Math.cos(a)*rx*spike;py=cy+Math.sin(a)*ry*spike;
    } else if(fukiType==='cloud'){
      const bump=1+0.12*Math.sin(a*6);
      px=cx+Math.cos(a)*rx*bump;py=cy+Math.sin(a)*ry*bump;
    } else if(fukiType==='wavy'){
      const wave=1+0.06*Math.sin(a*10);
      px=cx+Math.cos(a)*rx*wave;py=cy+Math.sin(a)*ry*wave;
    } else if(fukiType==='square'){
      /* Rectangle approximation */
      const t=a/(2*Math.PI);
      if(t<0.25){px=cx+rx;py=cy-ry+ry*2*(t/0.25)}
      else if(t<0.5){px=cx+rx-rx*2*((t-0.25)/0.25);py=cy+ry}
      else if(t<0.75){px=cx-rx;py=cy+ry-ry*2*((t-0.5)/0.25)}
      else{px=cx-rx+rx*2*((t-0.75)/0.25);py=cy-ry}
    }
    pts.push({x:px,y:py});
  }
  /* Fill: white triangles */
  for(let i=1;i<pts.length;i++){
    out.push(cx/cw,cy/ch,1,1,1,1, pts[i-1].x/cw,pts[i-1].y/ch,1,1,1,1, pts[i].x/cw,pts[i].y/ch,1,1,1,1);
  }
  /* Border: thin quads */
  for(let i=1;i<pts.length;i++){
    const a=pts[i-1],b=pts[i];
    const dx=b.x-a.x,dy=b.y-a.y,len=Math.sqrt(dx*dx+dy*dy)||1;
    const nx=-dy/len*bw,ny=dx/len*bw;
    out.push((a.x+nx)/cw,(a.y+ny)/ch,0,0,0,1,(a.x-nx)/cw,(a.y-ny)/ch,0,0,0,1,(b.x+nx)/cw,(b.y+ny)/ch,0,0,0,1);
    out.push((a.x-nx)/cw,(a.y-ny)/ch,0,0,0,1,(b.x-nx)/cw,(b.y-ny)/ch,0,0,0,1,(b.x+nx)/cw,(b.y+ny)/ch,0,0,0,1);
  }
  /* Tail */
  const tailDir=document.getElementById('fukiTail').value;
  if(tailDir!=='none'){
    let tx=cx,ty=cy+ry+ry*0.4;
    if(tailDir==='top')ty=cy-ry-ry*0.4;
    if(tailDir==='left'){tx=cx-rx-rx*0.4;ty=cy}
    if(tailDir==='right'){tx=cx+rx+rx*0.4;ty=cy}
    const tw=rx*0.15;
    out.push((cx-tw)/cw,(tailDir==='top'||tailDir==='bottom'?cy+(tailDir==='bottom'?ry:-ry):cy)/ch,0,0,0,1,
             (cx+tw)/cw,(tailDir==='top'||tailDir==='bottom'?cy+(tailDir==='bottom'?ry:-ry):cy)/ch,0,0,0,1,
             tx/cw,ty/ch,0,0,0,1);
  }
}

/* === GENKO SDK: Panel (コマ) rendering === */
function tessellatePanel(out,o,cw,ch){
  /* Support mm-unit coords (paper-relative): scale by sc when o._unit === 'mm'.
     Legacy docs without _unit keep canvas-internal-pixel coords. */
  let s=1;
  if(o._unit==='mm'){
    const y=YOUSHI[activeYoushi];
    if(y&&y.draw)s=Math.min(cw*0.9/y.wMM,ch*0.9/y.hMM);
  }
  const x1=Math.min(o.x1,o.x2)*s,y1=Math.min(o.y1,o.y2)*s,x2=Math.max(o.x1,o.x2)*s,y2=Math.max(o.y1,o.y2)*s;
  const bw=(o.borderW||0.8)*dpr;
  /* White fill */
  out.push(x1/cw,y1/ch,1,1,1,1, x2/cw,y1/ch,1,1,1,1, x2/cw,y2/ch,1,1,1,1);
  out.push(x1/cw,y1/ch,1,1,1,1, x2/cw,y2/ch,1,1,1,1, x1/cw,y2/ch,1,1,1,1);
  /* Black border */
  function side(ax,ay,bx,by){
    const dx=bx-ax,dy=by-ay,len=Math.sqrt(dx*dx+dy*dy)||1;
    const nx=-dy/len*bw,ny=dx/len*bw;
    out.push((ax+nx)/cw,(ay+ny)/ch,0,0,0,1,(ax-nx)/cw,(ay-ny)/ch,0,0,0,1,(bx+nx)/cw,(by+ny)/ch,0,0,0,1);
    out.push((ax-nx)/cw,(ay-ny)/ch,0,0,0,1,(bx-nx)/cw,(by-ny)/ch,0,0,0,1,(bx+nx)/cw,(by+ny)/ch,0,0,0,1);
  }
  side(x1,y1,x2,y1);side(x2,y1,x2,y2);side(x2,y2,x1,y2);side(x1,y2,x1,y1);
}

/* === Panel presets (コマ割りテンプレート) === */
function getYoushiInnerRect(){
  const y=YOUSHI[activeYoushi];if(!y||!y.draw)return null;
  const sc=Math.min(C.width*0.9/y.wMM,C.height*0.9/y.hMM);
  return{l:y.innerL*sc,t:y.innerT*sc,r:y.innerR*sc,b:y.innerB*sc};
}
function applyPanelPreset(pid){
  const rect=getYoushiInnerRect();if(!rect)return;
  const g=(+document.getElementById('panelGutter').value||3)*dpr*2;
  const bw=+document.getElementById('panelBorderW').value||0.8;
  const w=rect.r-rect.l,h=rect.b-rect.t;
  const ps=[];
  if(pid==='2h'){const hh=(h-g)/2;ps.push([rect.l,rect.t,rect.r,rect.t+hh],[rect.l,rect.t+hh+g,rect.r,rect.b])}
  else if(pid==='3h'){const hh=(h-g*2)/3;for(let i=0;i<3;i++)ps.push([rect.l,rect.t+i*(hh+g),rect.r,rect.t+i*(hh+g)+hh])}
  else if(pid==='4h'){const hh=(h-g*3)/4;for(let i=0;i<4;i++)ps.push([rect.l,rect.t+i*(hh+g),rect.r,rect.t+i*(hh+g)+hh])}
  else if(pid==='2x2'){const ww=(w-g)/2,hh=(h-g)/2;ps.push([rect.l,rect.t,rect.l+ww,rect.t+hh],[rect.l+ww+g,rect.t,rect.r,rect.t+hh],[rect.l,rect.t+hh+g,rect.l+ww,rect.b],[rect.l+ww+g,rect.t+hh+g,rect.r,rect.b])}
  else if(pid==='lshape'){const hh=(h-g)/2,ww=(w-g)/2;ps.push([rect.l,rect.t,rect.r,rect.t+hh],[rect.l,rect.t+hh+g,rect.l+ww,rect.b],[rect.l+ww+g,rect.t+hh+g,rect.r,rect.b])}
  else if(pid==='action'){const hh=(h-g*2)/3,ww=(w-g)/2;ps.push([rect.l,rect.t,rect.l+ww,rect.t+hh],[rect.l+ww+g,rect.t,rect.r,rect.t+hh*2+g],[rect.l,rect.t+hh+g,rect.l+ww,rect.t+hh*2+g],[rect.l,rect.t+hh*2+g*2,rect.r,rect.b])}
  const presetPanels=[];
  ps.forEach((p,i)=>{const pov={type:'panel',x1:p[0],y1:p[1],x2:p[2],y2:p[3],borderW:bw,panelName:pid+'-'+(i+1),_nid:nid(),_visible:true,_parent:''};overlays.push(pov);presetPanels.push({...pov})});
  recordOp('panelPreset',{panels:presetPanels,preset:pid});
  needsRedraw=true;rebuildNT();scheduleAutoSave();
}
document.querySelectorAll('[data-koma]').forEach(el=>{el.addEventListener('click',()=>applyPanelPreset(el.dataset.koma))});

/* Overlay objects (tones, fukidashi, text, panel) — declared at top */

/* === GENKO SDK: Object selection & move — selectedIdx etc declared at top === */

/** Compute bounding box {minX,minY,maxX,maxY} in device px for any object. */
function objBounds(idx){
  if(idx<strokes.length){
    const s=strokes[idx];if(!s||!s.points.length)return null;
    let mnx=Infinity,mny=Infinity,mxx=-Infinity,mxy=-Infinity;
    for(const p of s.points){if(p.x<mnx)mnx=p.x;if(p.y<mny)mny=p.y;if(p.x>mxx)mxx=p.x;if(p.y>mxy)mxy=p.y}
    const pad=s.size*dpr;
    return{minX:mnx-pad,minY:mny-pad,maxX:mxx+pad,maxY:mxy+pad};
  }
  const o=overlays[idx-strokes.length];if(!o)return null;
  /* All rect-based overlays (panel, tone, fukidashi, ai-image, ai-desc) */
  if(o.x1!=null&&o.y1!=null&&o.x2!=null&&o.y2!=null){
    return{minX:Math.min(o.x1,o.x2),minY:Math.min(o.y1,o.y2),maxX:Math.max(o.x1,o.x2),maxY:Math.max(o.y1,o.y2)};
  }
  if(o.type==='text'){
    const w=o.text.length*o.size*dpr*(o.dir==='vertical'?0.7:1);
    const h=o.size*dpr*(o.dir==='vertical'?o.text.length:1)*1.2;
    return{minX:o.x,minY:o.y,maxX:o.x+(o.dir==='vertical'?o.size*dpr:w),maxY:o.y+h};
  }
  return null;
}

/** Find topmost object at (px,py) in device coords. Returns index or -1. */
function hitTest(px,py){
  /* Check overlays first (top), then strokes (bottom) */
  for(let i=overlays.length-1;i>=0;i--){
    const b=objBounds(strokes.length+i);
    if(b&&px>=b.minX&&px<=b.maxX&&py>=b.minY&&py<=b.maxY)return strokes.length+i;
  }
  for(let i=strokes.length-1;i>=0;i--){
    const b=objBounds(i);
    if(b&&px>=b.minX&&px<=b.maxX&&py>=b.minY&&py<=b.maxY)return i;
  }
  return -1;
}

/** Move object by (dx,dy) device px. */
function moveObj(idx,dx,dy){
  if(idx<strokes.length){
    const s=strokes[idx];if(!s)return;
    for(const p of s.points){p.x+=dx;p.y+=dy}
  } else {
    const o=overlays[idx-strokes.length];if(!o)return;
    /* Move any rect-based overlay (panel, tone, fukidashi, ai-image, ai-desc) */
    if(o.x1!=null){o.x1+=dx;o.y1+=dy;o.x2+=dx;o.y2+=dy}
    if(o.x!=null){o.x+=dx;o.y+=dy}
  }
  needsRedraw=true;
}

/** Corner handle size in device px. */
const HANDLE_R=5*dpr;
const HANDLE_HIT=10*dpr; /* hit area slightly larger than visual */

/** Tessellate selection highlight rectangle + 4 corner resize handles. */
function tessellateSelection(out,cw,ch){
  if(selectedIdx<0)return;
  const b=objBounds(selectedIdx);if(!b)return;
  const pad=4*dpr;
  const x1=(b.minX-pad)/cw,y1=(b.minY-pad)/ch,x2=(b.maxX+pad)/cw,y2=(b.maxY+pad)/ch;
  const w=1.5*dpr;
  const c=[0.2,0.5,1,0.7]; /* blue */
  function side(ax,ay,bx,by){
    const dx=bx-ax,dy=by-ay,len=Math.sqrt(dx*dx+dy*dy)||1;
    const nx=(-dy/len)*w/cw,ny=(dx/len)*w/ch;
    out.push(ax+nx,ay+ny,...c,ax-nx,ay-ny,...c,bx+nx,by+ny,...c);
    out.push(ax-nx,ay-ny,...c,bx-nx,by-ny,...c,bx+nx,by+ny,...c);
  }
  side(x1,y1,x2,y1);side(x2,y1,x2,y2);side(x2,y2,x1,y2);side(x1,y2,x1,y1);
  /* 4 corner handles (filled squares) */
  const hr=HANDLE_R/cw,hv=HANDLE_R/ch;
  const hc=[0.2,0.5,1,1]; /* solid blue */
  const corners=[[x1,y1],[x2,y1],[x2,y2],[x1,y2]];
  for(const [cx,cy] of corners){
    out.push(cx-hr,cy-hv,...hc, cx+hr,cy-hv,...hc, cx+hr,cy+hv,...hc);
    out.push(cx-hr,cy-hv,...hc, cx+hr,cy+hv,...hc, cx-hr,cy+hv,...hc);
  }
}

/** Hit-test selection corner handles. Returns 'tl','tr','br','bl' or null. */
function hitHandle(px,py){
  if(selectedIdx<0)return null;
  const b=objBounds(selectedIdx);if(!b)return null;
  const pad=4*dpr;const hr=HANDLE_HIT;
  const corners=[
    {k:'tl',x:b.minX-pad,y:b.minY-pad},
    {k:'tr',x:b.maxX+pad,y:b.minY-pad},
    {k:'br',x:b.maxX+pad,y:b.maxY+pad},
    {k:'bl',x:b.minX-pad,y:b.maxY+pad},
  ];
  for(const c of corners){
    if(Math.abs(px-c.x)<hr&&Math.abs(py-c.y)<hr)return c.k;
  }
  return null;
}

/** Scale object by moving one corner, keeping opposite corner fixed. */
function scaleObj(idx,corner,px,py){
  if(idx<strokes.length){
    /* Stroke scaling: scale all points relative to opposite corner */
    const s=strokes[idx];if(!s||!s.points.length)return;
    const b=objBounds(idx);if(!b)return;
    let fixX,fixY;
    if(corner==='tl'){fixX=b.maxX;fixY=b.maxY}
    else if(corner==='tr'){fixX=b.minX;fixY=b.maxY}
    else if(corner==='br'){fixX=b.minX;fixY=b.minY}
    else{fixX=b.maxX;fixY=b.minY}
    const oldW=b.maxX-b.minX||1,oldH=b.maxY-b.minY||1;
    const newW=Math.abs(px-fixX)||1,newH=Math.abs(py-fixY)||1;
    const sx=newW/oldW,sy=newH/oldH;
    for(const p of s.points){
      p.x=fixX+(p.x-fixX)*sx;
      p.y=fixY+(p.y-fixY)*sy;
    }
  } else {
    /* Rect-based overlay: just move the dragged corner */
    const o=overlays[idx-strokes.length];if(!o)return;
    if(o.x1!=null){
      if(corner==='tl'){o.x1=px;o.y1=py}
      else if(corner==='tr'){o.x2=px;o.y1=py}
      else if(corner==='br'){o.x2=px;o.y2=py}
      else if(corner==='bl'){o.x1=px;o.y2=py}
    }
  }
  needsRedraw=true;
}

/** Cursor for each corner handle. */
const HANDLE_CURSORS={tl:'nwse-resize',tr:'nesw-resize',br:'nwse-resize',bl:'nesw-resize'};

/* --- Brush Presets --- */
const BRUSHES={
  fine:   {size:2,  opacity:1,   minWidth:0.3, gamma:0.35, tiltEffect:0.2, label:'Fine Pen'},
  pen:    {size:4,  opacity:1,   minWidth:0.5, gamma:0.45, tiltEffect:0.4, label:'Pen'},
  marker: {size:12, opacity:0.7, minWidth:0.8, gamma:0.6,  tiltEffect:0.3, label:'Marker'},
  brush:  {size:20, opacity:0.5, minWidth:0.4, gamma:0.4,  tiltEffect:0.8, label:'Brush'},
  flat:   {size:30, opacity:0.6, minWidth:0.6, gamma:0.5,  tiltEffect:1.0, label:'Flat'},
  eraser: {size:20, opacity:1,   minWidth:1.0, gamma:0.5,  tiltEffect:0.0, label:'Eraser'},
};

/* --- State — declared at top === */

/* --- Toolbar --- */
const colorPicker=document.getElementById('colorPicker'),sizeSlider=document.getElementById('sizeSlider');
const sizeLabel=document.getElementById('sizeLabel'),status=document.getElementById('status');
const brushBtns=document.querySelectorAll('[data-brush]');

function selectBrush(name){
  const b=BRUSHES[name];if(!b)return;
  activeBrush=name;erasing=(name==='eraser');
  brushSize=b.size;brushOpacity=b.opacity;brushGamma=b.gamma;brushTiltEffect=b.tiltEffect;brushMinWidth=b.minWidth;
  sizeSlider.value=brushSize;sizeLabel.textContent=brushSize;
  C.style.cursor=erasing?'cell':'crosshair';
  brushBtns.forEach(el=>el.classList.toggle('act',el.dataset.brush===name));
  setToolMode('draw'); /* switch to draw when picking a brush */
}
brushBtns.forEach(el=>el.addEventListener('click',()=>selectBrush(el.dataset.brush)));

colorPicker.oninput=()=>{const h=colorPicker.value;brushColor=[parseInt(h.slice(1,3),16)/255,parseInt(h.slice(3,5),16)/255,parseInt(h.slice(5,7),16)/255,1]};
sizeSlider.oninput=()=>{brushSize=+sizeSlider.value;sizeLabel.textContent=brushSize};
document.getElementById('btnUndo').onclick=undo;
document.getElementById('btnRedo').onclick=redo;
/* btnClear removed from toolbar — clear via node tree delete or undo */

/* --- Save --- */
document.getElementById('btnSavePNG').onclick=savePNG;
document.getElementById('btnSaveSVG').onclick=saveSVG;
document.getElementById('btnExportOplog').onclick=exportOplog;
document.getElementById('btnReplay').onclick=()=>{
  const inp=document.createElement('input');inp.type='file';inp.accept='.json';
  inp.onchange=()=>{const f=inp.files[0];if(!f)return;const r=new FileReader();
    r.onload=()=>{if(importOplog(r.result)){needsRedraw=true;scheduleAutoSave()}};r.readAsText(f)};inp.click();
};

function savePNG(){
  /* Render one clean frame then export */
  needsRedraw=true;
  requestAnimationFrame(()=>{
    C.toBlob(blob=>{
      if(!blob)return;
      const a=document.createElement('a');a.href=URL.createObjectURL(blob);
      a.download='canvas-${nanoid}-'+Date.now()+'.png';a.click();URL.revokeObjectURL(a.href);
    },'image/png');
  });
}

function saveSVG(){
  const cw=C.width,ch=C.height;
  let svg='<svg xmlns="http://www.w3.org/2000/svg" width="'+Math.round(cw/dpr)+'" height="'+Math.round(ch/dpr)+'" viewBox="0 0 '+(cw/dpr)+' '+(ch/dpr)+'">';
  svg+='<rect width="100%" height="100%" fill="#f0ead6"/>';
  for(const s of strokes){
    if(s.points.length<2)continue;
    const r=Math.round(s.color[0]*255),g=Math.round(s.color[1]*255),b=Math.round(s.color[2]*255);
    const col='rgb('+r+','+g+','+b+')';
    const w=s.size*0.5;
    let d='M'+round2(s.points[0].x/dpr)+' '+round2(s.points[0].y/dpr);
    for(let i=1;i<s.points.length;i++){d+=' L'+round2(s.points[i].x/dpr)+' '+round2(s.points[i].y/dpr)}
    svg+='<path d="'+d+'" fill="none" stroke="'+col+'" stroke-width="'+round2(w)+'" stroke-linecap="round" stroke-linejoin="round" opacity="'+s.opacity+'"/>';
  }
  svg+='</svg>';
  const blob=new Blob([svg],{type:'image/svg+xml'});
  const a=document.createElement('a');a.href=URL.createObjectURL(blob);
  a.download='canvas-${nanoid}-'+Date.now()+'.svg';a.click();URL.revokeObjectURL(a.href);
}
function round2(v){return Math.round(v*100)/100}

addEventListener('keydown',e=>{
  if((e.ctrlKey||e.metaKey)&&e.key==='s'){e.preventDefault();savePNG()}
  if((e.ctrlKey||e.metaKey)&&e.key==='z'){e.preventDefault();e.shiftKey?redo():undo()}
  if((e.ctrlKey||e.metaKey)&&e.key==='y'){e.preventDefault();redo()}
  if(!e.ctrlKey&&!e.metaKey){
    if(e.key==='1')selectBrush('fine');
    if(e.key==='2')selectBrush('pen');
    if(e.key==='3')selectBrush('marker');
    if(e.key==='4')selectBrush('brush');
    if(e.key==='5')selectBrush('flat');
    if(e.key==='e'||e.key==='E')selectBrush('eraser');
    if(e.key==='d'||e.key==='D')setToolMode('draw');
    if(e.key==='v'||e.key==='V')setToolMode('select');
    if(e.key==='k'||e.key==='K')setToolMode('panel');
    if(e.key==='t'&&!e.ctrlKey&&!e.metaKey)setToolMode('tone');
    if(e.key==='f'||e.key==='F')setToolMode('fukidashi');
    if(e.key==='x'&&!e.ctrlKey&&!e.metaKey)setToolMode('text');
    if((e.key==='Delete'||e.key==='Backspace')&&toolMode==='select'&&selectedIdx>=0){
      if(selectedIdx<strokes.length){strokes.splice(selectedIdx,1)}
      else{overlays.splice(selectedIdx-strokes.length,1)}
      selectedIdx=-1;needsRedraw=true;e.preventDefault();
    }
    if(e.key==='['&&brushSize>1){brushSize--;sizeSlider.value=brushSize;sizeLabel.textContent=brushSize}
    if(e.key===']'&&brushSize<80){brushSize++;sizeSlider.value=brushSize;sizeLabel.textContent=brushSize}
    if(e.key==='='||e.key==='+'){zoom=Math.min(20,zoom*1.2);needsRedraw=true;status.textContent='zoom='+Math.round(zoom*100)+'%'}
    if(e.key==='-'){zoom=Math.max(0.1,zoom/1.2);needsRedraw=true;status.textContent='zoom='+Math.round(zoom*100)+'%'}
    if(e.key==='0'){zoom=1;panX=0;panY=0;needsRedraw=true;status.textContent='zoom=100% (reset)'}
  }
});
function undo(){if(strokes.length){redoStack.push(strokes.pop());needsRedraw=true;rebuildNT();recordOp('undo',{});scheduleAutoSave()}}
function redo(){if(redoStack.length){strokes.push(redoStack.pop());needsRedraw=true;rebuildNT();recordOp('redo',{});scheduleAutoSave()}}

/* --- Pointer Input: XP-Pen Deco + mouse unified ---
 * After macOS Accessibility + Input Monitoring permissions granted,
 * XP-Pen sends standard pointerdown/move/up with pressure and tilt.
 * Unified handler for both mouse and pen tablet.
 */

function evToPoint(e){
  const rawP=e.pressure||0;
  const pr=rawP>0?applyCurve(rawP):0.5;
  const tx=(e.tiltX||0),ty=(e.tiltY||0);
  const tiltMag=Math.sqrt(tx*tx+ty*ty);
  const tiltAngle=Math.atan2(ty,tx);
  const tiltRatio=1.0-Math.min(tiltMag/90,0.6);
  /* Client → canvas device px → world coords (inverse zoom+pan) */
  const cs=clientToCanvas(e.clientX,e.clientY);
  const wx=(cs.x-panX)/zoom,wy=(cs.y-panY)/zoom;
  /* Paper texture jitter */
  const jx=paperJitter*(Math.random()-0.5)*dpr;
  const jy=paperJitter*(Math.random()-0.5)*dpr;
  return{x:wx+jx,y:wy+jy,pressure:pr,tiltAngle,tiltRatio,pointerType:e.pointerType||'mouse'};
}

function getPoints(e){
  if(e.getCoalescedEvents){const c=e.getCoalescedEvents();if(c.length>0)return c.map(evToPoint)}
  return[evToPoint(e)];
}

function beginStroke(e){
  if(isDrawing)return;
  if((e.clientY||0)<TOOLBAR_H)return;
  isDrawing=true;activePointerId=e.pointerId||0;
  const bgColor=[0.94,0.918,0.84,1];
  const pts=getPoints(e);
  currentStroke={color:erasing?bgColor:[...brushColor],size:brushSize,eraser:erasing,
    opacity:brushOpacity,tiltEffect:brushTiltEffect,brushType:activeBrush,points:pts};
  redoStack=[];needsRedraw=true;
}

function endStroke(){
  if(!isDrawing)return;
  isDrawing=false;activePointerId=null;
  if(currentStroke&&currentStroke.points.length>0){
    currentStroke._nid=nid();currentStroke._visible=true;
    strokes.push(currentStroke);needsRedraw=true;rebuildNT();
    recordOp('stroke',{stroke:{...currentStroke}});scheduleAutoSave();
  }
  currentStroke=null;
}

function addPoints(e){
  if(!isDrawing||!currentStroke)return;
  const pts=getPoints(e);
  for(const p of pts){
    const last=currentStroke.points[currentStroke.points.length-1];
    const dx=p.x-last.x,dy=p.y-last.y;
    if(dx*dx+dy*dy>0.25){currentStroke.points.push(p);needsRedraw=true}
  }
  const lp=pts[pts.length-1];
  status.textContent=lp.pointerType+' p='+lp.pressure.toFixed(2)+' pts='+currentStroke.points.length+' | ${nanoid}';
}

/* ===== Unified pointer handler (mouse + pen tablet) ===== */
let _lastPointerType='mouse';
C.addEventListener('pointerdown',e=>{
  if(isPanning)return;
  e.preventDefault();
  try{C.setPointerCapture(e.pointerId)}catch(ex){}
  /* Auto-switch tool based on input device (pen→draw, mouse→select) */
  const pt=e.pointerType||'mouse';
  if(pt!==_lastPointerType){
    _lastPointerType=pt;
    if(pt==='pen'&&toolMode==='select')setToolMode('draw');
    else if(pt==='mouse'&&toolMode==='draw'&&!isDrawing)setToolMode('select');
  }
  /* World coords for all tools */
  const cc=clientToCanvas(e.clientX,e.clientY);const w=screenToWorld(cc.x,cc.y);
  const px=w.x,py=w.y;
  if(toolMode==='draw'){beginStroke(e)}
  else if(toolMode==='select'){
    /* Check corner handles first (resize) */
    const handle=hitHandle(px,py);
    if(handle&&selectedIdx>=0){
      _resizeCorner=handle;_resizeStart={x:px,y:py};
      C.style.cursor=HANDLE_CURSORS[handle];
    } else {
      _resizeCorner=null;
      const hit=hitTest(px,py);
      selectedIdx=hit;
      if(hit>=0){selectDragStart={x:px,y:py};selectDragOffset={x:0,y:0};C.style.cursor='move'}
      else{C.style.cursor='default'}
    }
    needsRedraw=true;rebuildNT();
  }
  else if(toolMode==='tone'||toolMode==='fukidashi'||toolMode==='panel'){dragStart={x:px,y:py}}
  else if(toolMode==='text'){
    const txt=document.getElementById('textInput').value;
    if(txt){const ov={type:'text',x:px,y:py,text:txt,
      size:+document.getElementById('textSize').value,
      font:document.getElementById('textFont').value,
      dir:document.getElementById('textDir').value,color:[...brushColor],_nid:nid(),_visible:true};
      overlays.push(ov);needsRedraw=true;rebuildNT();recordOp('addOverlay',{overlay:{...ov}});scheduleAutoSave()}
  }
});
C.addEventListener('pointermove',e=>{
  if(isPanning)return;
  const cc=clientToCanvas(e.clientX,e.clientY);const w=screenToWorld(cc.x,cc.y);
  const px=w.x,py=w.y;
  if(toolMode==='draw'&&isDrawing)addPoints(e);
  else if(toolMode==='select'&&_resizeCorner&&selectedIdx>=0){
    /* Resize: scale object by dragging corner */
    scaleObj(selectedIdx,_resizeCorner,px,py);
  }
  else if(toolMode==='select'&&selectDragStart&&selectedIdx>=0){
    const dx=px-selectDragStart.x,dy=py-selectDragStart.y;
    moveObj(selectedIdx,dx-selectDragOffset.x,dy-selectDragOffset.y);
    selectDragOffset={x:dx,y:dy};
  }
  else if(toolMode==='select'&&selectedIdx>=0&&!selectDragStart&&!_resizeCorner){
    /* Hover cursor: show resize cursor when hovering over handles */
    const hh=hitHandle(px,py);
    C.style.cursor=hh?HANDLE_CURSORS[hh]:'move';
  }
  else if((toolMode==='tone'||toolMode==='fukidashi'||toolMode==='panel')&&dragStart){needsRedraw=true}
});
C.addEventListener('pointerup',e=>{
  if(isPanning)return;
  if(toolMode==='draw'){endStroke()}
  else if(toolMode==='select'){
    if(_resizeCorner&&selectedIdx>=0){
      /* Record resize op */
      const nidR=selectedIdx<strokes.length?(strokes[selectedIdx]||{})._nid:(overlays[selectedIdx-strokes.length]||{})._nid;
      if(nidR)recordOp('scaleNode',{nid:nidR,corner:_resizeCorner});
      _resizeCorner=null;_resizeStart=null;
      scheduleAutoSave();
    } else if(selectDragStart&&selectDragOffset&&selectedIdx>=0&&(selectDragOffset.x||selectDragOffset.y)){
      const nidM=selectedIdx<strokes.length?(strokes[selectedIdx]||{})._nid:(overlays[selectedIdx-strokes.length]||{})._nid;
      if(nidM)recordOp('moveNode',{nid:nidM,dx:selectDragOffset.x,dy:selectDragOffset.y});
      scheduleAutoSave();
    }
    selectDragStart=null;selectDragOffset=null;
    if(selectedIdx>=0)C.style.cursor='move';else C.style.cursor='default';
  }
  else if(dragStart&&toolMode==='panel'){
    const cc=clientToCanvas(e.clientX,e.clientY);const w=screenToWorld(cc.x,cc.y);
    const bw=+document.getElementById('panelBorderW').value||0.8;
    const pov={type:'panel',x1:dragStart.x,y1:dragStart.y,x2:w.x,y2:w.y,borderW:bw,_nid:nid(),_visible:true,_parent:''};
    overlays.push(pov);dragStart=null;needsRedraw=true;rebuildNT();recordOp('addOverlay',{overlay:{...pov}});scheduleAutoSave();
  }
  else if(dragStart&&(toolMode==='tone'||toolMode==='fukidashi')){
    const cc=clientToCanvas(e.clientX,e.clientY);const w=screenToWorld(cc.x,cc.y);
    const tov={type:toolMode,x1:dragStart.x,y1:dragStart.y,x2:w.x,y2:w.y,
      tonePattern,toneDensity:document.getElementById('toneDensity').value,
      toneLPI:document.getElementById('toneLPI').value,
      fukiType,fukiTail:document.getElementById('fukiTail').value,_nid:nid(),_visible:true};
    overlays.push(tov);dragStart=null;needsRedraw=true;rebuildNT();recordOp('addOverlay',{overlay:{...tov}});scheduleAutoSave();
  }
});
C.addEventListener('pointercancel',()=>{endStroke();dragStart=null;selectDragStart=null});

/* Coalesced events for high-frequency pen tablet sampling */
try{C.addEventListener('pointerrawupdate',e=>{if(isDrawing&&toolMode==='draw')addPoints(e)})}catch(ex){}

/* --- WebGPU Renderer --- */
let device,ctx,pipeline,vertBuf,vertCount=0,vpUniformBuf,vpBindGroup;
const MAX_VERTS=4_000_000;

async function initGPU(){
  if(!navigator.gpu)throw new Error('WebGPU not supported');
  const adapter=await navigator.gpu.requestAdapter({powerPreference:'high-performance'});
  if(!adapter)throw new Error('No GPU adapter');
  device=await adapter.requestDevice();
  ctx=C.getContext('webgpu');
  const fmt=navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({device,format:fmt,alphaMode:'opaque'});

  const shader=device.createShaderModule({code:\`
struct VP{zoom:f32,panX:f32,panY:f32,cw:f32,ch:f32,_pad1:f32,_pad2:f32,_pad3:f32};
@group(0) @binding(0) var<uniform> vp:VP;
struct VSOut{@builtin(position) pos:vec4f,@location(0) col:vec4f};
@vertex fn vs(@location(0) p:vec2f,@location(1) c:vec4f)->VSOut{
  /* p is in [0..1] normalized coords (world/canvas). Apply zoom+pan. */
  let wx=p.x*vp.cw; let wy=p.y*vp.ch;
  let sx=(wx*vp.zoom+vp.panX)/vp.cw;
  let sy=(wy*vp.zoom+vp.panY)/vp.ch;
  var o:VSOut;o.pos=vec4f(sx*2.0-1.0,(1.0-sy)*2.0-1.0,0,1);o.col=c;return o;}
@fragment fn fs(v:VSOut)->@location(0) vec4f{return v.col;}
\`});

  const bgl=device.createBindGroupLayout({entries:[
    {binding:0,visibility:GPUShaderStage.VERTEX,buffer:{type:'uniform'}}
  ]});
  const pipeLayout=device.createPipelineLayout({bindGroupLayouts:[bgl]});

  pipeline=device.createRenderPipeline({
    layout:pipeLayout,
    vertex:{module:shader,buffers:[
      {arrayStride:24,attributes:[{shaderLocation:0,offset:0,format:'float32x2'},{shaderLocation:1,offset:8,format:'float32x4'}]}
    ]},
    fragment:{module:shader,targets:[{format:fmt,blend:{
      color:{srcFactor:'src-alpha',dstFactor:'one-minus-src-alpha',operation:'add'},
      alpha:{srcFactor:'one',dstFactor:'one-minus-src-alpha',operation:'add'}
    }}]},
    primitive:{topology:'triangle-list'},
  });
  vertBuf=device.createBuffer({size:MAX_VERTS*24,usage:GPUBufferUsage.VERTEX|GPUBufferUsage.COPY_DST});
  vpUniformBuf=device.createBuffer({size:32,usage:GPUBufferUsage.UNIFORM|GPUBufferUsage.COPY_DST});
  vpBindGroup=device.createBindGroup({layout:bgl,entries:[{binding:0,resource:{buffer:vpUniformBuf}}]});
}

/* --- Tessellate: tilt-aware elliptical brush stamps + quad segments --- */
function tessellateAll(){
  const verts=[];const cw=C.width,ch=C.height;
  const pg=activePage();
  /* Youshi frame guides (bottom layer, visibility-gated) */
  if(pg.youshi.visible)tessellateYoushi(verts,cw,ch);
  /* Strokes (cascade visibility: self + all ancestors must be visible) */
  for(const s of strokes){if(isNodeVisible(s._nid))tessellateStroke(s,verts,cw,ch)}
  if(currentStroke)tessellateStroke(currentStroke,verts,cw,ch);
  /* Overlays: panels first (bottom), then tones, fukidashi (cascade visibility) */
  for(const o of overlays){
    if(!isNodeVisible(o._nid))continue;
    if(o.type==='panel')tessellatePanel(verts,o,cw,ch);
  }
  for(const o of overlays){
    if(!isNodeVisible(o._nid))continue;
    if(o.type==='tone')tessellateToneRect(verts,o.x1,o.y1,o.x2,o.y2,cw,ch);
    if(o.type==='fukidashi')tessellateFukidashi(verts,o.x1,o.y1,o.x2,o.y2,cw,ch);
  }
  /* Selection highlight */
  tessellateSelection(verts,cw,ch);
  return new Float32Array(verts);
}

function tessellateStroke(s,out,cw,ch){
  const pts=s.points;if(pts.length<1)return;
  const c=s.color;
  const sop=s.opacity||1;
  const sTilt=s.tiltEffect||0;
  /* Quad segments between consecutive points */
  for(let i=0;i<pts.length-1;i++){
    const a=pts[i],b=pts[i+1];
    const dx=b.x-a.x,dy=b.y-a.y;
    const len=Math.sqrt(dx*dx+dy*dy)||1;
    const nx=-dy/len,ny=dx/len;
    const ra=s.size*a.pressure*dpr*0.5;
    const rb=s.size*b.pressure*dpr*0.5;
    const alphaA=sop*Math.max(0.3,a.pressure);
    const alphaB=sop*Math.max(0.3,b.pressure);
    /* Quad with per-vertex alpha */
    const ax1=(a.x+nx*ra)/cw,ay1=(a.y+ny*ra)/ch;
    const ax2=(a.x-nx*ra)/cw,ay2=(a.y-ny*ra)/ch;
    const bx1=(b.x+nx*rb)/cw,by1=(b.y+ny*rb)/ch;
    const bx2=(b.x-nx*rb)/cw,by2=(b.y-ny*rb)/ch;
    out.push(ax1,ay1,c[0],c[1],c[2],alphaA, ax2,ay2,c[0],c[1],c[2],alphaA, bx1,by1,c[0],c[1],c[2],alphaB);
    out.push(ax2,ay2,c[0],c[1],c[2],alphaA, bx2,by2,c[0],c[1],c[2],alphaB, bx1,by1,c[0],c[1],c[2],alphaB);
  }
  /* Elliptical cap at each sample point (tilt-aware) */
  for(const p of pts){
    const r=s.size*p.pressure*dpr*0.5;
    if(r<0.5)continue;
    const segs=Math.max(8,Math.min(32,Math.round(r*1.5)));
    /* Tilt effect: sTilt=0 → circle, sTilt=1 → full ellipse from tilt */
    const tRatio=1.0-(1.0-p.tiltRatio)*sTilt;
    const rx=r,ry=r*tRatio;
    const ang=p.tiltAngle;
    const cosA=Math.cos(ang),sinA=Math.sin(ang);
    const cx=p.x/cw,cy=p.y/ch;
    const alpha=sop*Math.max(0.3,p.pressure);
    for(let j=0;j<segs;j++){
      const a0=2*Math.PI*j/segs,a1=2*Math.PI*(j+1)/segs;
      const ex0=Math.cos(a0)*rx,ey0=Math.sin(a0)*ry;
      const ex1=Math.cos(a1)*rx,ey1=Math.sin(a1)*ry;
      const px0=(cosA*ex0-sinA*ey0)/cw,py0=(sinA*ex0+cosA*ey0)/ch;
      const px1=(cosA*ex1-sinA*ey1)/cw,py1=(sinA*ex1+cosA*ey1)/ch;
      out.push(cx,cy,c[0],c[1],c[2],alpha);
      out.push(cx+px0,cy+py0,c[0],c[1],c[2],alpha);
      out.push(cx+px1,cy+py1,c[0],c[1],c[2],alpha);
    }
  }
}

function render(){
  if(!needsRedraw&&!isDrawing){requestAnimationFrame(render);return}
  needsRedraw=false;
  /* Upload viewport uniforms */
  device.queue.writeBuffer(vpUniformBuf,0,new Float32Array([zoom,panX,panY,C.width,C.height,0,0,0]));
  const data=tessellateAll();
  vertCount=data.length/6;
  if(vertCount>0&&vertCount<=MAX_VERTS){device.queue.writeBuffer(vertBuf,0,data)}
  const enc=device.createCommandEncoder();
  const pass=enc.beginRenderPass({colorAttachments:[{
    view:ctx.getCurrentTexture().createView(),
    loadOp:'clear',clearValue:{r:0.7,g:0.7,b:0.7,a:1},storeOp:'store'
  }]});
  if(vertCount>0){pass.setPipeline(pipeline);pass.setBindGroup(0,vpBindGroup);pass.setVertexBuffer(0,vertBuf);pass.draw(vertCount)}
  pass.end();device.queue.submit([enc.finish()]);
  renderTextOverlays();
  renderPanelImages();
  requestAnimationFrame(render);
}

/* --- Text overlay (HTML elements, not Canvas 2D) --- */
const textLayer=document.createElement('div');
textLayer.style.cssText='position:fixed;top:0;left:0;width:100%;height:100%;pointer-events:none;z-index:5';
document.body.appendChild(textLayer);

function renderTextOverlays(){
  textLayer.innerHTML='';
  for(const o of overlays){
    if(o.type!=='text'&&o.type!=='link')continue;
    if(!isNodeVisible(o._nid))continue;
    const el=document.createElement('div');
    const fontMap={serif:'"Noto Serif JP",serif',sans:'"Noto Sans JP",sans-serif',manga:'"Noto Serif JP",serif'};
    const isVert=(o.dir==='vertical');
    el.style.cssText='position:absolute;left:'+(o.x/dpr)+'px;top:'+(o.y/dpr)+'px;font-size:'+o.size+'px;'+
      'font-family:'+fontMap[o.font]+';color:rgb('+Math.round(o.color[0]*255)+','+Math.round(o.color[1]*255)+','+Math.round(o.color[2]*255)+');'+
      'font-weight:700;line-height:1.4;white-space:pre;pointer-events:none;'+
      (isVert?'writing-mode:vertical-rl;text-orientation:mixed;':'');
    el.textContent=o.text;
    textLayer.appendChild(el);
  }
}

_initDone=true;
rebuildNT();
initGPU().then(()=>{needsRedraw=true;requestAnimationFrame(render)}).catch(e=>{
  document.body.innerHTML='<p style="color:#e06090;padding:2rem;font-size:14px">WebGPU: '+e.message+'</p>';
});
window.parent?.postMessage({type:'gftd:embed:ready',nanoid:'${nanoid}'},'*');
</script></body></html>`;
}
