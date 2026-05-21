/**
 * KAMI Engine Trackpad SDK — inline JS for Apple trackpad + mouse gesture unification.
 *
 * Apple Magic Trackpad / MacBook trackpad gesture mapping:
 * - Two-finger scroll → pan (NOT zoom)
 * - Pinch (ctrlKey+wheel on macOS) → zoom toward cursor
 * - Two-finger double-tap → fit-to-view reset
 * - Inertia/momentum scroll → smooth pan with deceleration
 * - Three-finger swipe → page navigation (optional)
 *
 * Mouse wheel mapping:
 * - Scroll wheel → zoom (traditional behavior)
 * - Shift+scroll → horizontal pan
 * - Ctrl+scroll → zoom (same as pinch)
 *
 * @returns Inline JS string to embed in HTML (uses global: C, zoom, panX, panY, needsRedraw, clientToCanvas, status, resize)
 */
export function kamiTrackpadHTML(): string {
  return `
/* === KAMI Trackpad SDK — Apple trackpad + mouse gesture unification === */
(function(){
  const _tpState={
    /* Gesture detection */
    isTrackpad:false,
    lastWheelTime:0,
    wheelEvents:[],
    /* Inertia */
    velX:0,velY:0,
    inertiaId:null,
    /* Pinch state */
    isPinching:false,
    pinchStartZoom:1,
    /* Double-tap detection */
    lastTapTime:0,
    tapCount:0,
    /* Touch tracking for gesture events */
    touches:[],
  };

  /** Detect if wheel event is from trackpad (small, frequent deltaY with no deltaMode) or mouse (large, infrequent). */
  function detectTrackpad(e){
    const now=performance.now();
    _tpState.wheelEvents.push({t:now,dy:Math.abs(e.deltaY),mode:e.deltaMode});
    if(_tpState.wheelEvents.length>8)_tpState.wheelEvents.shift();
    /* Trackpad: deltaMode===0, small deltas, high frequency */
    const recent=_tpState.wheelEvents.filter(w=>now-w.t<300);
    if(recent.length>=3){
      const avgDy=recent.reduce((s,w)=>s+w.dy,0)/recent.length;
      const allPixel=recent.every(w=>w.mode===0);
      _tpState.isTrackpad=allPixel&&avgDy<50;
    }
    _tpState.lastWheelTime=now;
    return _tpState.isTrackpad;
  }

  /** Start inertia deceleration for pan. */
  function startInertia(){
    if(_tpState.inertiaId)cancelAnimationFrame(_tpState.inertiaId);
    const decay=0.92;const threshold=0.5;
    function tick(){
      if(Math.abs(_tpState.velX)<threshold&&Math.abs(_tpState.velY)<threshold){
        _tpState.velX=0;_tpState.velY=0;return;
      }
      panX+=_tpState.velX;panY+=_tpState.velY;
      _tpState.velX*=decay;_tpState.velY*=decay;
      needsRedraw=true;
      _tpState.inertiaId=requestAnimationFrame(tick);
    }
    _tpState.inertiaId=requestAnimationFrame(tick);
  }

  /** Stop inertia immediately. */
  function stopInertia(){
    if(_tpState.inertiaId){cancelAnimationFrame(_tpState.inertiaId);_tpState.inertiaId=null}
    _tpState.velX=0;_tpState.velY=0;
  }

  /* Replace existing wheel handler with unified trackpad/mouse handler */
  C.addEventListener('wheel',function(e){
    e.preventDefault();
    const isTP=detectTrackpad(e);
    const _zc=clientToCanvas(e.clientX,e.clientY);
    const mx=_zc.x,my=_zc.y;

    if(e.ctrlKey){
      /* === Pinch-to-zoom (trackpad pinch or Ctrl+scroll) === */
      stopInertia();
      /* Trackpad pinch sends small deltaY with ctrlKey. Smooth zoom. */
      const zoomSpeed=isTP?0.01:0.05;
      const factor=1-e.deltaY*zoomSpeed;
      const clampedFactor=Math.max(0.5,Math.min(2,factor));
      panX=mx-(mx-panX)*clampedFactor;
      panY=my-(my-panY)*clampedFactor;
      zoom*=clampedFactor;
      zoom=Math.max(0.05,Math.min(50,zoom));
      needsRedraw=true;
      status.textContent='zoom='+Math.round(zoom*100)+'%';
    } else if(isTP){
      /* === Trackpad two-finger scroll → pan === */
      stopInertia();
      const scale=dpr;
      const dx=-e.deltaX*scale;
      const dy=-e.deltaY*scale;
      panX+=dx;panY+=dy;
      /* Accumulate velocity for inertia */
      _tpState.velX=dx*0.6;
      _tpState.velY=dy*0.6;
      needsRedraw=true;
      /* Start inertia after a brief pause */
      clearTimeout(_tpState._inertiaTimer);
      _tpState._inertiaTimer=setTimeout(()=>{
        if(Math.abs(_tpState.velX)>1||Math.abs(_tpState.velY)>1)startInertia();
      },50);
    } else {
      /* === Mouse scroll wheel → zoom (traditional) === */
      if(e.shiftKey){
        /* Shift+scroll → horizontal pan */
        panX-=e.deltaY*dpr*2;
        needsRedraw=true;
      } else {
        const factor=e.deltaY<0?1.1:0.9;
        panX=mx-(mx-panX)*factor;
        panY=my-(my-panY)*factor;
        zoom*=factor;
        zoom=Math.max(0.1,Math.min(20,zoom));
        needsRedraw=true;
        status.textContent='zoom='+Math.round(zoom*100)+'%';
      }
    }
  },{passive:false});

  /* === GestureEvent (Safari-specific: rotate, pinch) === */
  C.addEventListener('gesturestart',function(e){
    e.preventDefault();
    _tpState.isPinching=true;
    _tpState.pinchStartZoom=zoom;
    stopInertia();
  });
  C.addEventListener('gesturechange',function(e){
    e.preventDefault();
    if(!_tpState.isPinching)return;
    const newZoom=_tpState.pinchStartZoom*e.scale;
    const _gc=clientToCanvas(e.clientX||innerWidth/2,e.clientY||innerHeight/2);
    const mx=_gc.x,my=_gc.y;
    const factor=newZoom/zoom;
    panX=mx-(mx-panX)*factor;
    panY=my-(my-panY)*factor;
    zoom=Math.max(0.05,Math.min(50,newZoom));
    needsRedraw=true;
    status.textContent='zoom='+Math.round(zoom*100)+'%';
  });
  C.addEventListener('gestureend',function(e){
    e.preventDefault();
    _tpState.isPinching=false;
  });

  /* === Double-tap to fit-to-view (trackpad two-finger double-tap) === */
  C.addEventListener('dblclick',function(e){
    /* Only reset view if not in select mode with a panel */
    if(toolMode==='select'&&selectedIdx>=0)return; /* let existing dblclick handler run */
    if(e.detail===2||true){
      zoom=1;panX=0;panY=0;
      autoFitYoushi();
      needsRedraw=true;
      status.textContent='zoom=100% (fit)';
    }
  });

  /* === Touch events for iOS/iPadOS trackpad mode === */
  let _touchStartDist=0,_touchStartZoom=1,_touchStartPan={x:0,y:0},_touchStartMid={x:0,y:0};
  C.addEventListener('touchstart',function(e){
    if(e.touches.length===2){
      e.preventDefault();
      stopInertia();
      const t0=e.touches[0],t1=e.touches[1];
      _touchStartDist=Math.hypot(t1.clientX-t0.clientX,t1.clientY-t0.clientY);
      _touchStartZoom=zoom;
      _touchStartPan={x:panX,y:panY};
      _touchStartMid={x:(t0.clientX+t1.clientX)/2,y:(t0.clientY+t1.clientY)/2};
    }
  },{passive:false});
  C.addEventListener('touchmove',function(e){
    if(e.touches.length===2){
      e.preventDefault();
      const t0=e.touches[0],t1=e.touches[1];
      const dist=Math.hypot(t1.clientX-t0.clientX,t1.clientY-t0.clientY);
      const mid={x:(t0.clientX+t1.clientX)/2,y:(t0.clientY+t1.clientY)/2};
      /* Pinch zoom */
      const scale=dist/_touchStartDist;
      zoom=Math.max(0.05,Math.min(50,_touchStartZoom*scale));
      /* Two-finger pan */
      const dx=(mid.x-_touchStartMid.x)*dpr;
      const dy=(mid.y-_touchStartMid.y)*dpr;
      panX=_touchStartPan.x+dx;
      panY=_touchStartPan.y+dy;
      needsRedraw=true;
      status.textContent='zoom='+Math.round(zoom*100)+'%';
    }
  },{passive:false});

  /* Expose state for debugging */
  window.__kamiTrackpad=_tpState;
})();
`;
}
