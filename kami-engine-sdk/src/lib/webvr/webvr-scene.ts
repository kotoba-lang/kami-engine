/**
 * webvr/webvr-scene.ts — Three.js + WebXR smartphone-first WebVR scene.
 *
 * The scene renders one `LocationKind` room at a time plus a fan of
 * floating choice panels. Selection UX is center-reticle gaze-dwell
 * (1500 ms) with tap-to-confirm fallback so the experience works on
 * smartphones without controllers.
 *
 *   - Android Chrome with a Cardboard-style holder: `immersive-vr` session.
 *   - iOS Safari (no WebXR): magic-window mode driven by deviceorientation.
 *   - Desktop preview: WebGL canvas with mouse-look (pointer-lock).
 *
 * three is a peer dependency. The module is import-only — call
 * `mountIncidentScene(canvas, opts)` to attach.
 */

import * as THREE from 'three';
import type { LocationKind, NodeEffectKind } from './types.js';
import type { SceneDescriptor } from './incident-pregel.js';
import {
  createSplatCloudLayer,
  makeLocationCloud,
  type SplatCloudLayer,
  type SparkLocationKind,
} from '../spark/index.js';
import { buildNodeEffect, type NodeEffectInstance } from './node-effects.js';

// ─────────────────────────────────────────────────────────────────────────
// Public API

export interface MountOpts {
  /** Called when the user confirms a choice (gaze-dwell or tap). */
  onSelect: (choiceId: string) => void;
  /** Gaze-dwell time in ms before auto-confirm. Default 3000 (3s). */
  gazeDwellMs?: number;
  /**
   * Per-node selection deadline in ms. Countdown starts AFTER narration
   * completes (or 360ms / 700ms-fallback / 22s upper-bound when speech is
   * blocked or disabled). On timeout the renderer auto-fires the inaction
   * choice. Default 30000 (30s). Set 0 to disable.
   */
  selectionDeadlineMs?: number;
  /** Initial scene to render. The host typically calls `update()` after. */
  initial?: SceneDescriptor;
  /** Toggle the VR-enter button. Default true if `navigator.xr` is present. */
  enableVrButton?: boolean;
  /**
   * Auto-speak the briefing on every scene transition (Web Speech API,
   * SpeechSynthesis). Default `true` when SpeechSynthesis is available.
   * Browsers block audio until first user gesture; the first transition
   * may stay silent and subsequent ones will speak after any click/tap.
   */
  narrate?: boolean;
  /** BCP-47 voice lang. Default `'ja-JP'`. */
  narrateLang?: string;
  /**
   * Scene-transition fade duration in ms. A camera-attached overlay flashes
   * severity-tinted then eases back to clear. Default `280`. Set `0` to
   * disable.
   */
  transitionFadeMs?: number;
  /**
   * Render a Gaussian splat (3DGS) ambient cloud per location on top of
   * the toon room. Adds volumetric depth — monitor glow, tank vapor,
   * server-rack LEDs, sunset haze, etc. Default `true`. Set `false` for
   * lower-end devices or for the bare toon look.
   */
  useSparkBackdrop?: boolean;
  /** Splat budget per backdrop. Default 6000 (mobile-safe). */
  sparkSplatBudget?: number;
}

export interface SceneHandle {
  /** Replace the rendered scene (briefing + choices + room). */
  update(scene: SceneDescriptor): void;
  /** Tear down renderer + listeners. */
  dispose(): void;
  /** True once the renderer has had at least one frame. */
  ready(): boolean;
  /** The underlying renderer (escape hatch). */
  renderer: THREE.WebGLRenderer;
}

/**
 * Mount the incident VR scene onto a canvas element.
 *
 * The canvas may be 100vw × 100vh; the function manages its own RAF /
 * XR animation loop. Call `handle.dispose()` to clean up.
 */
export function mountIncidentScene(canvas: HTMLCanvasElement, opts: MountOpts): SceneHandle {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false });
  renderer.setPixelRatio(window.devicePixelRatio || 1);
  renderer.setSize(canvas.clientWidth || window.innerWidth, canvas.clientHeight || window.innerHeight, false);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.xr.enabled = true;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0xf0ead6); // KAMI cream
  scene.fog = new THREE.Fog(0xf0ead6, 12, 60);

  const camera = new THREE.PerspectiveCamera(80, (canvas.clientWidth || 1) / (canvas.clientHeight || 1), 0.05, 100);
  camera.position.set(0, 1.6, 0);
  camera.lookAt(0, 1.6, -1); // explicit forward for non-XR / non-deviceorientation

  const root = new THREE.Group();
  scene.add(root);

  // Lights — boosted to compensate for the toon shader's 3-step
  // quantisation. Hemi sky/ground covers the diffuse base; the two
  // directionals provide a Nintendo-style warm key + cool fill; ambient
  // floors the minimum brightness so nothing reads near-black.
  const hemi = new THREE.HemisphereLight(0xfff4d0, 0xb8a890, 2.0);
  scene.add(hemi);
  const key = new THREE.DirectionalLight(0xffffff, 1.8);
  key.position.set(4, 6, 3);
  scene.add(key);
  const fill = new THREE.DirectionalLight(0xc9dcff, 1.0);
  fill.position.set(-3, 4, 2);
  scene.add(fill);
  const ambient = new THREE.AmbientLight(0xffffff, 1.1);
  scene.add(ambient);

  // eslint-disable-next-line no-console
  console.info('[kami webvr] scene mount', { canvasW: canvas.clientWidth, canvasH: canvas.clientHeight });

  // ─── Room layer ──────────────────────────────────────────────────────
  const roomGroup = new THREE.Group();
  root.add(roomGroup);

  // ─── Briefing panel ─────────────────────────────────────────────────
  // Top of view — sits well above the choice ring so the cards never
  // block it. Replace texture each update via _rebuildBriefing.
  const briefingMat = new THREE.MeshBasicMaterial({ color: 0xffffff, side: THREE.DoubleSide });
  const briefing = new THREE.Mesh(new THREE.PlaneGeometry(2.6, 1.3), briefingMat);
  briefing.position.set(0, 2.45, -3.2);
  briefing.renderOrder = 10;
  root.add(briefing);

  // ─── Choice panels ──────────────────────────────────────────────────
  const choiceGroup = new THREE.Group();
  root.add(choiceGroup);

  // ─── kami-cine panel illustration (Stage 5-6 diffusion output) ─────
  // Small side accent — sits up-left of the briefing, angled inward.
  // Texture is replaced when SceneDescriptor.cinePanel.panelUrl lands.
  const panelMat = new THREE.MeshBasicMaterial({ color: 0xeeeeee, side: THREE.DoubleSide });
  const panelMesh = new THREE.Mesh(new THREE.PlaneGeometry(1.4, 0.8), panelMat);
  panelMesh.position.set(-3.0, 2.1, -3.8);
  panelMesh.rotation.y = 0.5;
  panelMesh.visible = false;
  panelMesh.renderOrder = 8;
  root.add(panelMesh);

  // ─── kami-cine geom placeholder (Stage 3 — gsplat receive-ready) ───
  // Until a real splat decoder is wired, render a procedural points-cloud
  // when geomArtifact.url is present, to make the wiring visible.
  const gsplatGroup = new THREE.Group();
  gsplatGroup.position.set(0, 1.4, -5.5);
  gsplatGroup.visible = false;
  root.add(gsplatGroup);

  // ─── Per-node effects group ─────────────────────────────────────────
  // Cleared + rebuilt on every node change. Each effect ticks itself.
  const effectsGroup = new THREE.Group();
  root.add(effectsGroup);
  let activeEffects: NodeEffectInstance[] = [];

  // ─── Spark backdrop — per-location 3DGS Gaussian point cloud ────────
  // Adds volumetric realism (monitor glow / tank vapor / LED clusters /
  // sunset haze) on top of the toon room. Painter-sort runs each tick.
  const useSparkBackdrop = opts.useSparkBackdrop !== false;
  const sparkBudget = opts.sparkSplatBudget ?? 6_000;
  let sparkLayer: SplatCloudLayer | undefined;
  let sparkLoc: LocationKind | undefined;
  if (useSparkBackdrop) {
    sparkLayer = createSplatCloudLayer({
      cloud: makeLocationCloud('scadaRoom' as SparkLocationKind),
      camera,
      splatBudget: sparkBudget,
      additive: true,
      foveation: 0.15,
      opacityMul: 1.4, // boost the ambient haze so the room feels luminous
    });
    sparkLayer.object3D.renderOrder = 2;
    root.add(sparkLayer.object3D);
  }

  // ─── 10-second selection countdown bar ─────────────────────────────
  // Sits just above the choice arc. Track is muted; the fill drains
  // (scale.x 1 → 0) over `selectionDeadlineMs`. On hit zero, the timeout
  // choice is auto-fired (see _pickTimeoutChoice).
  const countdownTrack = new THREE.Mesh(
    new THREE.PlaneGeometry(3.0, 0.08),
    new THREE.MeshBasicMaterial({ color: 0x26303d, transparent: true, opacity: 0.22 }),
  );
  countdownTrack.position.set(0, 2.0, -3.0);
  countdownTrack.visible = false;
  root.add(countdownTrack);
  const countdownFill = new THREE.Mesh(
    new THREE.PlaneGeometry(3.0, 0.08),
    new THREE.MeshBasicMaterial({ color: 0xff8a3d }),
  );
  // Pivot from left edge so the bar drains right→left.
  countdownFill.geometry.translate(1.5, 0, 0);
  countdownFill.position.set(-1.5, 2.0, -2.99);
  countdownFill.visible = false;
  root.add(countdownFill);

  // ─── Scene-transition fade overlay ─────────────────────────────────
  // A camera-attached fullscreen plane that flashes a severity-tinted
  // colour on every update() and eases back to fully-transparent over
  // `transitionFadeMs`. Lives in camera-local space so it always covers
  // the view regardless of head orientation.
  const transitionMat = new THREE.MeshBasicMaterial({
    color: 0xff8a3d, transparent: true, opacity: 0, depthTest: false, depthWrite: false,
  });
  const transitionPlane = new THREE.Mesh(new THREE.PlaneGeometry(4, 3), transitionMat);
  transitionPlane.position.set(0, 0, -1); // 1m in front of camera
  transitionPlane.renderOrder = 999;
  camera.add(transitionPlane);

  // ─── Diagnostic marker (a small bright sphere at briefing center) ──
  // Confirms render pipeline works even before any canvas textures are
  // built. Auto-removed on first successful scene update.
  const diagMarker = new THREE.Mesh(
    new THREE.SphereGeometry(0.08, 16, 12),
    new THREE.MeshBasicMaterial({ color: 0xff3d8a }),
  );
  diagMarker.position.set(0, 1.6, -1.5);
  root.add(diagMarker);

  // ─── Reticle ────────────────────────────────────────────────────────
  const reticleRing = new THREE.Mesh(
    new THREE.RingGeometry(0.012, 0.018, 32),
    new THREE.MeshBasicMaterial({ color: 0x222222, transparent: true, opacity: 0.85 }),
  );
  reticleRing.position.set(0, 0, -0.5);
  camera.add(reticleRing);
  scene.add(camera);

  const reticleFill = new THREE.Mesh(
    new THREE.RingGeometry(0.012, 0.012, 32),
    new THREE.MeshBasicMaterial({ color: 0xff8a3d, transparent: true, opacity: 1 }),
  );
  reticleRing.add(reticleFill);

  // ─── Magic-window deviceorientation (iOS Safari path) ───────────────
  const orient = _attachDeviceOrientation(camera);

  // ─── Desktop mouse-look (drag-to-look) ──────────────────────────────
  // Active only when deviceorientation isn't driving the camera. Click +
  // drag the canvas to pan; release to stop. No pointer-lock (avoids
  // capturing the cursor against the user's will).
  const mouseLook = _attachMouseLook(canvas, camera);

  // ─── Resize ─────────────────────────────────────────────────────────
  const onResize = () => {
    const w = canvas.clientWidth || window.innerWidth;
    const h = canvas.clientHeight || window.innerHeight;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };
  window.addEventListener('resize', onResize);

  // ─── VR enter button ────────────────────────────────────────────────
  let vrButton: HTMLButtonElement | undefined;
  if (opts.enableVrButton !== false && typeof navigator !== 'undefined' && 'xr' in navigator) {
    vrButton = _makeVrButton(renderer);
    canvas.parentElement?.appendChild(vrButton);
  }

  // ─── Gaze raycaster + tap fallback ──────────────────────────────────
  const raycaster = new THREE.Raycaster();
  const tapForward = new THREE.Vector3();
  let gazeTargetId: string | null = null;
  let gazeStart = 0;
  const dwellMs = opts.gazeDwellMs ?? 3000;
  const selectionDeadlineMs = opts.selectionDeadlineMs ?? 30_000;
  const transitionFadeMs = opts.transitionFadeMs ?? 280;
  let currentScene: SceneDescriptor | undefined;
  // Per-node selection countdown. Reset on every non-terminal `update()`.
  let countdownStart = 0;
  let countdownArmed = false;
  let countdownFiredFor: string | undefined; // node id we already timed out, prevent double-fire

  // ─── Scene transition state ──────────────────────────────────────────
  let fadeStart = 0;
  let fadeActive = false;
  let entranceStart = 0;
  let entranceActive = false;
  let lastSceneNodeKey: string | undefined; // briefing text identifies a node
  // Choices reveal AFTER narration completes — held hidden until then.
  let choicesEntranceStart = 0;
  let choicesEntranceActive = false;
  let lastEffectsTick = performance.now();

  // ─── Narrator (Web Speech API) ───────────────────────────────────────
  // Each speak() call accepts an `onEnd` callback that fires exactly
  // once on completion, error, fallback timeout, or supersession. Used
  // by update() to defer the per-node countdown until the narration is
  // done — so the user can hear the whole brief before time pressure.
  const narrate = opts.narrate ?? (typeof window !== 'undefined' && 'speechSynthesis' in window);
  const narrateLang = opts.narrateLang ?? 'ja-JP';
  let speechVersion = 0;

  function speak(text: string, onEnd?: () => void) {
    const myVersion = ++speechVersion;
    let done = false;
    const finish = () => {
      if (done) return;
      if (myVersion !== speechVersion) return; // superseded by a newer speak()
      done = true;
      onEnd?.();
    };
    if (!narrate || typeof window === 'undefined' || !('speechSynthesis' in window)) {
      // No audio path — give the entrance anim ~360 ms before arming the
      // timer so the user still sees a beat of breathing room.
      setTimeout(finish, 360);
      return;
    }
    try {
      window.speechSynthesis.cancel();
      const utter = new SpeechSynthesisUtterance(_narratorClean(text));
      utter.lang = narrateLang;
      utter.rate = 1.05;
      utter.pitch = 1.0;
      utter.volume = 1.0;
      utter.onend = finish;
      utter.onerror = finish;
      window.speechSynthesis.speak(utter);
      // Fallback A — speech blocked pre-gesture: at 700 ms, if nothing's
      // speaking and nothing's pending, treat it as done.
      setTimeout(() => {
        const ss = window.speechSynthesis;
        if (!ss?.speaking && !ss?.pending) finish();
      }, 700);
      // Fallback B — upper bound so a hung utterance can't freeze the
      // countdown forever.
      setTimeout(finish, 22_000);
    } catch (e) {
      // eslint-disable-next-line no-console
      console.debug('[kami webvr] speak() blocked', e);
      finish();
    }
  }

  function pickChoiceUnderReticle(): { id: string; panel: THREE.Object3D } | null {
    tapForward.set(0, 0, -1).applyQuaternion(camera.quaternion).normalize();
    const camWorld = new THREE.Vector3();
    camera.getWorldPosition(camWorld);
    raycaster.set(camWorld, tapForward);
    const hits = raycaster.intersectObjects(choiceGroup.children, true);
    for (const h of hits) {
      const id = (h.object.userData?.choiceId ?? (h.object.parent?.userData?.choiceId)) as string | undefined;
      if (id) return { id, panel: h.object.parent ?? h.object };
    }
    return null;
  }

  function commitSelection(id: string) {
    opts.onSelect(id);
  }

  function onTap() {
    // Don't fire selections that were actually drag-to-look gestures.
    if (mouseLook.dragging()) return;
    const hit = pickChoiceUnderReticle();
    if (hit) commitSelection(hit.id);
  }
  canvas.addEventListener('click', onTap);
  canvas.addEventListener('touchend', onTap, { passive: true });

  function _setAllGazeBars(visible: boolean) {
    for (const child of choiceGroup.children) {
      const bf = (child as THREE.Object3D).userData?.barFill as THREE.Mesh | undefined;
      const bt = (child as THREE.Object3D).userData?.barTrack as THREE.Mesh | undefined;
      if (bt) bt.visible = visible;
      if (bf) { bf.visible = visible; bf.scale.x = 0; }
    }
  }

  function _setGazeBar(id: string, ratio: number) {
    for (const child of choiceGroup.children) {
      const o = child as THREE.Object3D;
      const matchId = o.userData?.choiceId === id;
      const bf = o.userData?.barFill as THREE.Mesh | undefined;
      const bt = o.userData?.barTrack as THREE.Mesh | undefined;
      if (bt) bt.visible = matchId;
      if (bf) {
        bf.visible = matchId;
        bf.scale.x = matchId ? Math.max(0.001, ratio) : 0;
      }
    }
  }

  function _autoFireTimeout() {
    const s = currentScene;
    if (!s || s.terminal || !s.choices.length) return;
    const id = _pickTimeoutChoice(s);
    if (!id) return;
    countdownFiredFor = s.choices[0]?.id ? `${s.location}:${s.stage}:${id}` : undefined;
    commitSelection(id);
  }

  // ─── Animation loop ─────────────────────────────────────────────────
  let firstFrame = false;
  function tick(_t: number) {
    firstFrame = true;
    orient.update();
    const now = performance.now();

    // ── Gaze-dwell + per-card progress bar ──
    const hit = pickChoiceUnderReticle();
    if (hit) {
      if (hit.id !== gazeTargetId) {
        gazeTargetId = hit.id;
        gazeStart = now;
      }
      const dt = now - gazeStart;
      const ratio = Math.min(1, dt / dwellMs);
      (reticleFill.material as THREE.MeshBasicMaterial).opacity = 0.2 + 0.8 * ratio;
      reticleFill.scale.setScalar(1 + ratio * 1.6);
      _setGazeBar(hit.id, ratio);
      if (ratio >= 1) {
        const id = gazeTargetId!;
        gazeTargetId = null;
        gazeStart = 0;
        reticleFill.scale.setScalar(1);
        (reticleFill.material as THREE.MeshBasicMaterial).opacity = 1;
        _setAllGazeBars(false);
        commitSelection(id);
      }
    } else {
      gazeTargetId = null;
      gazeStart = 0;
      reticleFill.scale.setScalar(1);
      (reticleFill.material as THREE.MeshBasicMaterial).opacity = 1;
      _setAllGazeBars(false);
    }

    // ── 10s selection countdown ──
    if (countdownArmed) {
      const elapsed = now - countdownStart;
      const remainRatio = Math.max(0, 1 - elapsed / selectionDeadlineMs);
      countdownFill.scale.x = remainRatio;
      // Color shifts from orange → red as time runs out.
      const mat = countdownFill.material as THREE.MeshBasicMaterial;
      mat.color = new THREE.Color(
        remainRatio > 0.5 ? 0xff8a3d :
        remainRatio > 0.2 ? 0xe07b1c :
        0xc63d3d,
      );
      if (remainRatio <= 0) {
        countdownArmed = false;
        countdownTrack.visible = false;
        countdownFill.visible = false;
        _autoFireTimeout();
      }
    }

    // ── Scene-transition fade ──
    if (fadeActive) {
      const dt = now - fadeStart;
      const total = transitionFadeMs;
      // 0..0.18 = ramp up to 0.55 opacity (the flash), 0.18..1 = ease out.
      const t = dt / total;
      let o = 0;
      if (t < 0.18) o = (t / 0.18) * 0.55;
      else if (t < 1) o = 0.55 * (1 - (t - 0.18) / 0.82);
      transitionMat.opacity = Math.max(0, Math.min(0.55, o));
      if (t >= 1) {
        fadeActive = false;
        transitionMat.opacity = 0;
      }
    }

    // ── Entrance slide-in for briefing (immediate) ──
    if (entranceActive) {
      const dt = now - entranceStart;
      const total = Math.max(120, transitionFadeMs);
      const t = Math.min(1, dt / total);
      const eased = 1 - Math.pow(1 - t, 3); // easeOutCubic
      briefing.position.y = 2.45 + (1 - eased) * 0.5;
      const s = 0.94 + 0.06 * eased;
      briefing.scale.setScalar(s);
      if (t >= 1) {
        entranceActive = false;
        briefing.scale.setScalar(1);
      }
    }

    // ── Choices reveal — fires only AFTER narration finishes ──
    if (choicesEntranceActive) {
      const dt = now - choicesEntranceStart;
      const total = 380;
      const t = Math.min(1, dt / total);
      // easeOutBack
      const c1 = 1.70158;
      const c3 = c1 + 1;
      const eased = 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2);
      const sc = 0.78 + 0.22 * eased;
      for (const child of choiceGroup.children) {
        const o = child as THREE.Object3D;
        const restY = (o.userData?._restY as number | undefined);
        if (typeof restY === 'number') {
          o.position.y = restY - (1 - t) * 0.35;
        }
        o.scale.setScalar(sc);
        o.traverse((node) => {
          const m = (node as THREE.Mesh).material as THREE.MeshBasicMaterial | undefined;
          if (m && (m as any).map !== undefined) {
            m.transparent = true;
            m.opacity = Math.min(1, t * 1.1);
          }
        });
      }
      if (t >= 1) {
        choicesEntranceActive = false;
        for (const child of choiceGroup.children) {
          const o = child as THREE.Object3D;
          o.scale.setScalar(1);
          o.traverse((node) => {
            const m = (node as THREE.Mesh).material as THREE.MeshBasicMaterial | undefined;
            if (m && (m as any).map !== undefined) m.opacity = 1;
          });
        }
      }
    }

    // ── Per-node effects ──
    const effectDt = (now - lastEffectsTick) / 1000;
    lastEffectsTick = now;
    const tSec = now / 1000;
    for (const e of activeEffects) e.tick(effectDt, tSec);


    // ── Spark splat layer painter sort ──
    if (sparkLayer) sparkLayer.tick(canvas.clientHeight || window.innerHeight);

    renderer.render(scene, camera);
  }
  renderer.setAnimationLoop(tick);

  // ─── Update / dispose ───────────────────────────────────────────────
  function update(s: SceneDescriptor) {
    // A "node change" is detected by stage+location+briefing-hash. Async
    // cine / panel updates re-emit the same node and must NOT re-trigger
    // the fade / narration / entrance animation.
    const sceneKey = `${s.location}|${s.stage}|${s.briefing.slice(0, 64)}`;
    const isNodeChange = sceneKey !== lastSceneNodeKey;
    lastSceneNodeKey = sceneKey;

    currentScene = s;
    _rebuildRoom(roomGroup, s.location);
    _rebuildBriefing(briefing, undefined, briefingMat, s);
    _rebuildChoices(choiceGroup, s);
    // Record each card's rest Y so the entrance animation knows where to
    // ease back to (cards are offset DOWN at t=0, ease up to rest).
    for (const child of choiceGroup.children) {
      const o = child as THREE.Object3D;
      o.userData._restY = o.position.y;
    }
    // Hide choices on node change — they'll be revealed when narration ends.
    if (isNodeChange && !s.terminal && s.choices.length) {
      choiceGroup.visible = false;
      choicesEntranceActive = false;
    } else if (s.terminal || !s.choices.length) {
      // Terminal outcome card has no narration gate; show immediately.
      choiceGroup.visible = true;
    }
    // Rebuild per-node effects on node change.
    if (isNodeChange) {
      for (const e of activeEffects) e.dispose();
      while (effectsGroup.children.length) effectsGroup.remove(effectsGroup.children[0]!);
      activeEffects = [];
      const kinds: NodeEffectKind[] = s.effects ? [...s.effects] : [];
      const seed = _hashStr(s.nodeId ?? sceneKey);
      for (let i = 0; i < kinds.length; i++) {
        const inst = buildNodeEffect(kinds[i]!, { severity: s.severity, seed: seed + i * 0x9e37 });
        effectsGroup.add(inst.group);
        activeEffects.push(inst);
      }
    }
    _rebuildPanel(panelMesh, panelMat, s);
    _rebuildGsplatPlaceholder(gsplatGroup, s);
    // Swap spark backdrop only when the actual location changes (cloud
    // build is O(N) — avoid re-running on per-frame cine async re-emits).
    if (sparkLayer && s.location !== sparkLoc) {
      sparkLoc = s.location;
      sparkLayer.setCloud(makeLocationCloud(s.location as SparkLocationKind));
    }
    // Hide the diagnostic marker once we have at least one real scene.
    diagMarker.visible = false;
    // Reset the countdown UI on every node change. The bar stays VISIBLE
    // at 100% while narration plays (signals "your timer is locked"),
    // then arms (starts draining) only when the speak() finishes.
    if (s.terminal || !s.choices.length || selectionDeadlineMs <= 0) {
      countdownArmed = false;
      countdownTrack.visible = false;
      countdownFill.visible = false;
    } else if (isNodeChange) {
      countdownArmed = false;
      countdownTrack.visible = true;
      countdownFill.visible = true;
      countdownFill.scale.x = 1;
      (countdownFill.material as THREE.MeshBasicMaterial).color = new THREE.Color(0xff8a3d);
    }
    _setAllGazeBars(false);
    gazeTargetId = null;
    gazeStart = 0;

    // ── Fire transition fade + narration + entrance ON NODE CHANGE only.
    if (isNodeChange) {
      if (transitionFadeMs > 0) {
        const tint = _severityTint(s.severity);
        transitionMat.color = new THREE.Color(tint);
        transitionMat.opacity = 0;
        fadeStart = performance.now();
        fadeActive = true;
      }
      entranceStart = performance.now();
      entranceActive = true;
      // Capture the scene we narrated for; once narration ends, arm THIS
      // node's countdown — but only if the user hasn't already advanced.
      const armForKey = sceneKey;
      setTimeout(() => speak(_speechFromScene(s), () => {
        if (lastSceneNodeKey !== armForKey) return; // user already picked
        if (s.terminal || !s.choices.length) return;
        // Reveal choices (fade-in + scale pop) now that narration is done.
        choiceGroup.visible = true;
        choicesEntranceStart = performance.now();
        choicesEntranceActive = true;
        // Arm the countdown only if a deadline is configured.
        if (selectionDeadlineMs > 0) {
          countdownArmed = true;
          countdownStart = performance.now();
        }
      }), 80);
    }
    // eslint-disable-next-line no-console
    console.info('[kami webvr] scene update', {
      location: s.location, stage: s.stage, choices: s.choices.length, terminal: s.terminal,
      hasCine: !!s.cine, hasPanel: !!s.cinePanel?.panelUrl, hasGeom: !!s.cine?.geomArtifact?.url,
    });
  }
  if (opts.initial) update(opts.initial);

  function dispose() {
    renderer.setAnimationLoop(null);
    window.removeEventListener('resize', onResize);
    canvas.removeEventListener('click', onTap);
    canvas.removeEventListener('touchend', onTap);
    orient.dispose();
    mouseLook.dispose();
    vrButton?.remove();
    sparkLayer?.dispose();
    for (const e of activeEffects) e.dispose();
    activeEffects = [];
    try { window.speechSynthesis?.cancel(); } catch { /* ignore */ }
    renderer.dispose();
  }

  return {
    update,
    dispose,
    ready: () => firstFrame,
    renderer,
  };
}

// ─────────────────────────────────────────────────────────────────────────
// Internal — room builders (low-poly, no-asset)

// ─────────────────────────────────────────────────────────────────────────
// Stage 5 lite — toon material + reverse-backface outline.
// Approximates the "neural-render" stage (outline + toon) of the
// `gftd:kami-cine@1.0.0` pipeline using stock WebGL. Each toon mesh gets
// a slightly-inflated black sibling rendered with `BackSide`, which
// shows up as a 1-2 px silhouette around the surface.

function _toonGradientTexture(steps = 3): THREE.DataTexture {
  // 1×N gradient with quantised steps — feeds MeshToonMaterial. The
  // canonical setup uses NearestFilter on both axes; LinearFilter would
  // interpolate across steps and dim the shadow band toward black.
  // Floor (darkest band) lifted to 50% so even the unlit side stays
  // readable.
  const data = new Uint8Array(steps * 4);
  for (let i = 0; i < steps; i++) {
    const v = Math.round((0.5 + (i / Math.max(1, steps - 1)) * 0.5) * 255);
    data[i * 4 + 0] = v;
    data[i * 4 + 1] = v;
    data[i * 4 + 2] = v;
    data[i * 4 + 3] = 255;
  }
  const tex = new THREE.DataTexture(data, steps, 1);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.magFilter = THREE.NearestFilter;
  tex.minFilter = THREE.NearestFilter;
  tex.needsUpdate = true;
  return tex;
}

const _TOON_GRADIENT = _toonGradientTexture(3);

function _toonMaterial(color: number, opts: { side?: unknown } = {}): THREE.MeshToonMaterial {
  return new THREE.MeshToonMaterial({
    color,
    gradientMap: _TOON_GRADIENT,
    side: opts.side ?? THREE.FrontSide,
  });
}

function _outlineMesh(geom: THREE.BufferGeometry, scale = 1.04): THREE.Mesh {
  const m = new THREE.Mesh(
    geom,
    new THREE.MeshBasicMaterial({ color: 0x0d1117, side: THREE.BackSide }),
  );
  m.scale.setScalar(scale);
  m.renderOrder = -1;
  return m;
}

function _toonProp(geom: THREE.BufferGeometry, color: number, x: number, y: number, z: number, outlineScale = 1.06): THREE.Object3D {
  const g = new THREE.Group();
  g.position.set(x, y, z);
  const body = new THREE.Mesh(geom, _toonMaterial(color));
  g.add(_outlineMesh(geom, outlineScale));
  g.add(body);
  return g;
}

function _rebuildRoom(group: THREE.Group, loc: LocationKind): void {
  _clearGroup(group);
  const palette = _palette(loc);
  // Floor — single toon plane sitting just above y=0 to avoid z-fighting
  // with the wall shell. `polygonOffset` is the canonical three.js cure
  // for coplanar tearing when the camera angle changes.
  const floorMat = _toonMaterial(palette.floor);
  floorMat.polygonOffset = true;
  floorMat.polygonOffsetFactor = -1;
  floorMat.polygonOffsetUnits  = -1;
  const floor = new THREE.Mesh(new THREE.PlaneGeometry(20, 20), floorMat);
  floor.rotation.x = -Math.PI / 2;
  floor.position.y = 0.01;
  group.add(floor);
  // Room shell — 4 walls + ceiling assembled from planes (no bottom face
  // means no coplanar surface against the floor). All FrontSide → inside
  // faces toward viewer.
  const wallMat = _toonMaterial(palette.wall);
  const wallPanels: Array<{ pos: [number, number, number]; rot: [number, number, number]; size: [number, number] }> = [
    { pos: [0, 3, -10], rot: [0,           0,           0], size: [20, 6] }, // -Z wall
    { pos: [0, 3,  10], rot: [0,           Math.PI,     0], size: [20, 6] }, // +Z wall
    { pos: [-10, 3, 0], rot: [0,           Math.PI / 2, 0], size: [20, 6] }, // -X wall
    { pos: [ 10, 3, 0], rot: [0,          -Math.PI / 2, 0], size: [20, 6] }, // +X wall
    { pos: [0, 6,  0], rot: [ Math.PI / 2, 0,           0], size: [20, 20] }, // ceiling
  ];
  for (const w of wallPanels) {
    const m = new THREE.Mesh(new THREE.PlaneGeometry(w.size[0], w.size[1]), wallMat);
    m.position.set(w.pos[0], w.pos[1], w.pos[2]);
    m.rotation.set(w.rot[0], w.rot[1], w.rot[2]);
    group.add(m);
  }
  // Props get outline + toon.
  for (const prop of _props(loc, palette)) group.add(prop);
}

function _palette(loc: LocationKind) {
  // Palettes were too dark for the toon shader — MeshToonMaterial quantizes
  // diffuse to {33, 67, 100}% of the base color, so a near-black wall stays
  // near-black even when fully lit. Lifted luminance ~2-3× while keeping
  // hue to give each location a recognisable mood without going to black.
  switch (loc) {
    case 'scadaRoom':    return { floor: 0x5b6479, wall: 0x4a5468, accent: 0x6cd1ff };
    case 'cleanroom':    return { floor: 0xf6f8fc, wall: 0xeff4fc, accent: 0x7adfb4 };
    case 'chemicalYard': return { floor: 0x8a7853, wall: 0xa48a5f, accent: 0xff9a4d };
    case 'utilityRoom':  return { floor: 0x808892, wall: 0x6f7884, accent: 0xd0d4db };
    case 'serverRoom':   return { floor: 0x4a525d, wall: 0x3a414c, accent: 0x86eecf };
    case 'executiveRoom':return { floor: 0x7a5a3f, wall: 0xeedfc0, accent: 0xc99a55 };
    case 'press':        return { floor: 0x55595f, wall: 0xfaecec, accent: 0xff6c6c };
  }
}

function _props(loc: LocationKind, p: ReturnType<typeof _palette>): THREE.Object3D[] {
  const out: THREE.Object3D[] = [];
  const make = (geom: THREE.BufferGeometry, color: number, x: number, y: number, z: number, outlineScale = 1.06) => {
    out.push(_toonProp(geom, color, x, y, z, outlineScale));
  };
  // All props live at z<=-5.5 so they sit clearly behind the choice arc
  // (cards at z ~ -2..-2.5) and behind the briefing (z=-3.2).
  switch (loc) {
    case 'scadaRoom':
      make(new THREE.BoxGeometry(1.2, 0.8, 0.4), p.accent, -2.4, 1.4, -6);
      make(new THREE.BoxGeometry(1.2, 0.8, 0.4), p.accent,  0,   1.4, -6);
      make(new THREE.BoxGeometry(1.2, 0.8, 0.4), p.accent,  2.4, 1.4, -6);
      make(new THREE.BoxGeometry(6, 1.0, 1.2), 0x4d5260, 0, 0.5, -5);
      break;
    case 'cleanroom':
      for (let i = -2; i <= 2; i++) make(new THREE.CylinderGeometry(0.6, 0.6, 1.6, 16), p.accent, i * 2.2, 0.8, -6.5);
      break;
    case 'chemicalYard':
      for (let i = -1; i <= 1; i++) make(new THREE.CylinderGeometry(1.0, 1.0, 3.0, 24), p.accent, i * 3.2, 1.5, -7);
      make(new THREE.BoxGeometry(9, 0.2, 4), 0x6e5b3f, 0, 0.05, -7, 1.01);
      break;
    case 'utilityRoom':
      for (let i = -2; i <= 2; i++) make(new THREE.BoxGeometry(0.8, 2.0, 0.8), p.accent, i * 1.8, 1.0, -6);
      break;
    case 'serverRoom':
      for (let i = -3; i <= 3; i++) {
        make(new THREE.BoxGeometry(0.8, 2.2, 1.0), i % 2 === 0 ? 0x111418 : 0x1f242b, i * 1.3, 1.1, -6);
      }
      break;
    case 'executiveRoom':
      make(new THREE.BoxGeometry(4, 0.1, 1.6), p.accent, 0, 1.1, -5.5, 1.02);
      for (let i = -1; i <= 1; i++) make(new THREE.BoxGeometry(0.5, 1.1, 0.5), 0x3a2a1d, i * 1.4, 0.55, -5.5);
      break;
    case 'press':
      make(new THREE.BoxGeometry(3, 1.0, 0.6), p.accent, 0, 0.8, -5.5);
      break;
  }
  return out;
}

function _rebuildBriefing(
  mesh: THREE.Mesh,
  _tex: THREE.CanvasTexture | undefined,
  mat: THREE.MeshBasicMaterial,
  s: SceneDescriptor,
): void {
  const cv = _makeBriefingCanvas(s);
  // Create a fresh CanvasTexture every update — replacing `texture.image`
  // does not invalidate the GPU upload in newer three.js (>=0.150).
  const newTex = new THREE.CanvasTexture(cv);
  newTex.colorSpace = THREE.SRGBColorSpace;
  newTex.anisotropy = 4;
  newTex.needsUpdate = true;
  if (mat.map) {
    try { mat.map.dispose?.(); } catch { /* ignore */ }
  }
  mat.map = newTex;
  mat.color = new THREE.Color(0xffffff);
  mat.transparent = false;
  mat.needsUpdate = true;
  mesh.visible = !s.terminal;
}

function _rebuildChoices(group: THREE.Group, s: SceneDescriptor): void {
  _clearGroup(group);
  if (s.terminal) {
    group.add(_terminalCard(s));
    return;
  }
  // Layout: cards live in a foreground arc closer to the camera than any
  // room prop. Room props have been pushed back to z<=-5 (see _props),
  // so the z range -1.9..-2.4 occupied by cards stays clear of clutter.
  // y=1.5 keeps cards centered between briefing (y=2.45) and floor.
  const radius = 2.5;
  const arc = Math.min(0.5, 0.16 + 0.1 * s.choices.length); // half-arc in radians
  for (let i = 0; i < s.choices.length; i++) {
    const c = s.choices[i];
    const t = s.choices.length === 1 ? 0 : (i / (s.choices.length - 1)) * 2 - 1; // -1..+1
    const yaw = t * arc;
    const panel = _choicePanel(c, _severityTint(s.severity));
    panel.position.set(Math.sin(yaw) * radius, 1.5, -Math.cos(yaw) * radius);
    panel.lookAt(0, 1.6, 0);
    panel.renderOrder = 5;
    group.add(panel);
  }
}

function _severityTint(sev: SceneDescriptor['severity']): string {
  switch (sev) {
    case 'critical': return '#c63d3d';
    case 'high':     return '#e07b1c';
    case 'medium':   return '#d4a73a';
    case 'low':      return '#5e9d56';
    case 'info':
    default:         return '#4d77c4';
  }
}

function _choicePanel(c: SceneDescriptor['choices'][number], tint: string): THREE.Object3D {
  const cv = _makeChoiceCanvas(c.label, c.hint, tint);
  const tex = new THREE.CanvasTexture(cv);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.anisotropy = 4;
  tex.needsUpdate = true;
  const mat = new THREE.MeshBasicMaterial({ map: tex, side: THREE.DoubleSide, color: 0xffffff });
  const cardW = 1.5;
  const cardH = 0.9;
  const mesh = new THREE.Mesh(new THREE.PlaneGeometry(cardW, cardH), mat);
  mesh.userData.choiceId = c.id;
  // Invisible thicker hit-box for raycast stability
  const hit = new THREE.Mesh(
    new THREE.BoxGeometry(cardW, cardH, 0.05),
    new THREE.MeshBasicMaterial({ visible: false }),
  );
  hit.userData.choiceId = c.id;

  // Gaze-progress bar — child plane anchored to the card's bottom edge.
  // Scale.x runs from 0→1 as the gaze accumulates; hidden when the user
  // isn't looking at this card. Updated by the tick loop.
  const barTrack = new THREE.Mesh(
    new THREE.PlaneGeometry(cardW * 0.9, 0.06),
    new THREE.MeshBasicMaterial({ color: 0x26303d, transparent: true, opacity: 0.18 }),
  );
  barTrack.position.set(0, -cardH * 0.5 + 0.07, 0.01);
  barTrack.visible = false;

  const barFill = new THREE.Mesh(
    new THREE.PlaneGeometry(cardW * 0.9, 0.06),
    new THREE.MeshBasicMaterial({ color: parseInt(tint.replace('#', ''), 16) || 0xff8a3d }),
  );
  // Pivot from the left edge so scale.x grows leftwards→rightwards.
  barFill.geometry.translate(cardW * 0.9 * 0.5, 0, 0);
  barFill.position.set(-cardW * 0.45, -cardH * 0.5 + 0.07, 0.012);
  barFill.scale.x = 0;
  barFill.visible = false;

  const group = new THREE.Group();
  group.userData.choiceId = c.id;
  group.userData.barTrack = barTrack;
  group.userData.barFill = barFill;
  group.add(mesh);
  group.add(hit);
  group.add(barTrack);
  group.add(barFill);
  return group;
}

function _terminalCard(s: SceneDescriptor): THREE.Object3D {
  const cv = _makeTerminalCanvas(s);
  const tex = new THREE.CanvasTexture(cv);
  const mat = new THREE.MeshBasicMaterial({ map: tex });
  const mesh = new THREE.Mesh(new THREE.PlaneGeometry(2.8, 1.6), mat);
  mesh.position.set(0, 1.4, -2.8);
  return mesh;
}

// ─────────────────────────────────────────────────────────────────────────
// Canvas helpers (no-asset SDF-light text rendering)

function _makeTextCanvas(text: string): HTMLCanvasElement {
  const cv = document.createElement('canvas');
  cv.width = 1024;
  cv.height = 512;
  return cv;
}

function _makeBriefingCanvas(s: SceneDescriptor): HTMLCanvasElement {
  const cv = document.createElement('canvas');
  cv.width = 1024; cv.height = 512;
  const ctx = cv.getContext('2d')!;
  ctx.fillStyle = '#ffffff';
  _roundRect(ctx, 0, 0, cv.width, cv.height, 32);
  ctx.fill();
  // Stage badge.
  ctx.fillStyle = _severityTint(s.severity);
  _roundRect(ctx, 24, 24, 220, 56, 16);
  ctx.fill();
  ctx.fillStyle = '#ffffff';
  ctx.font = 'bold 28px sans-serif';
  ctx.textBaseline = 'middle';
  ctx.fillText(s.stage.toUpperCase(), 40, 52);
  // kami-cine pill (right side of header) — shows when Stage 1-4 ran.
  if (s.cine) {
    const w = s.cine.geomArtifact ? 360 : 280;
    ctx.fillStyle = s.cine.status === 'mock' ? '#9aa3b2' : '#4dc1ff';
    _roundRect(ctx, cv.width - w - 24, 24, w, 56, 16);
    ctx.fill();
    ctx.fillStyle = '#ffffff';
    ctx.font = 'bold 22px sans-serif';
    const tag = s.cine.status === 'mock' ? 'CINE/mock' : 'CINE/live';
    const cam = s.cine.worldArtifact?.cameraHint ?? '—';
    const fmt = s.cine.geomArtifact?.format ?? 'no-geom';
    ctx.fillText(`${tag} · ${cam} · ${fmt}`, cv.width - w - 8, 52);
  }
  // Briefing body.
  ctx.fillStyle = '#26303d';
  ctx.font = '28px sans-serif';
  const lines = s.briefing.split('\n');
  let y = 120;
  for (const line of lines) {
    _wrapAndFill(ctx, line, 32, y, cv.width - 64, 36);
    y += 36 * _wrapLineCount(ctx, line, cv.width - 64);
  }
  // Cine summary footer.
  if (s.cine?.worldArtifact?.summary) {
    ctx.fillStyle = '#6b7180';
    ctx.font = 'italic 20px sans-serif';
    _wrapAndFill(ctx, '生成: ' + s.cine.worldArtifact.summary, 32, cv.height - 56, cv.width - 64, 24);
  }
  return cv;
}

function _makeChoiceCanvas(label: string, hint: string | undefined, tint: string): HTMLCanvasElement {
  const cv = document.createElement('canvas');
  cv.width = 640; cv.height = 360;
  const ctx = cv.getContext('2d')!;
  ctx.fillStyle = '#ffffff';
  _roundRect(ctx, 0, 0, cv.width, cv.height, 28);
  ctx.fill();
  ctx.strokeStyle = tint;
  ctx.lineWidth = 8;
  _roundRect(ctx, 4, 4, cv.width - 8, cv.height - 8, 26);
  ctx.stroke();
  ctx.fillStyle = '#26303d';
  ctx.font = 'bold 30px sans-serif';
  ctx.textBaseline = 'middle';
  _wrapAndFill(ctx, label, 28, 80, cv.width - 56, 38);
  if (hint) {
    ctx.fillStyle = '#6b7180';
    ctx.font = '22px sans-serif';
    _wrapAndFill(ctx, hint, 28, 240, cv.width - 56, 28);
  }
  return cv;
}

function _makeTerminalCanvas(s: SceneDescriptor): HTMLCanvasElement {
  const cv = document.createElement('canvas');
  cv.width = 1024; cv.height = 576;
  const ctx = cv.getContext('2d')!;
  ctx.fillStyle = '#ffffff';
  _roundRect(ctx, 0, 0, cv.width, cv.height, 32);
  ctx.fill();
  const tint =
    s.terminal === 'success' ? '#5e9d56' :
    s.terminal === 'partial' ? '#d4a73a' : '#c63d3d';
  ctx.fillStyle = tint;
  _roundRect(ctx, 24, 24, cv.width - 48, 80, 18);
  ctx.fill();
  ctx.fillStyle = '#ffffff';
  ctx.font = 'bold 44px sans-serif';
  ctx.textBaseline = 'middle';
  ctx.fillText('演習終了: ' + (s.terminal ?? 'success').toUpperCase(), 56, 64);
  ctx.fillStyle = '#26303d';
  ctx.font = '28px sans-serif';
  _wrapAndFill(ctx, s.briefing, 32, 160, cv.width - 64, 36);
  return cv;
}

function _roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r);
  ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  ctx.lineTo(x + r, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - r);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}

function _wrapAndFill(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, maxWidth: number, lineHeight: number) {
  const words = text.split('');
  let line = '';
  let cursorY = y;
  for (const ch of words) {
    const test = line + ch;
    if (ctx.measureText(test).width > maxWidth && line.length > 0) {
      ctx.fillText(line, x, cursorY);
      cursorY += lineHeight;
      line = ch;
    } else {
      line = test;
    }
  }
  if (line) ctx.fillText(line, x, cursorY);
}

function _wrapLineCount(ctx: CanvasRenderingContext2D, text: string, maxWidth: number): number {
  let n = 1;
  let line = '';
  for (const ch of text) {
    const test = line + ch;
    if (ctx.measureText(test).width > maxWidth && line.length > 0) {
      n++;
      line = ch;
    } else {
      line = test;
    }
  }
  return n;
}

function _rebuildPanel(
  mesh: THREE.Mesh,
  mat: THREE.MeshBasicMaterial,
  s: SceneDescriptor,
): void {
  const url = s.cinePanel?.panelUrl;
  if (!url) {
    mesh.visible = false;
    return;
  }
  // Use an HTMLImageElement → CanvasTexture path; works for both data:
  // URLs (mock) and signed B2 URLs (live). Avoids three's TextureLoader
  // so we don't depend on `three/examples/...`.
  const img = new Image();
  img.crossOrigin = 'anonymous';
  img.onload = () => {
    const cv = document.createElement('canvas');
    cv.width = img.naturalWidth;
    cv.height = img.naturalHeight;
    cv.getContext('2d')?.drawImage(img, 0, 0);
    const tex = new THREE.CanvasTexture(cv);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.anisotropy = 4;
    tex.needsUpdate = true;
    if (mat.map) try { mat.map.dispose?.(); } catch { /* ignore */ }
    mat.map = tex;
    mat.color = new THREE.Color(0xffffff);
    mat.needsUpdate = true;
    mesh.visible = true;
  };
  img.onerror = () => {
    // eslint-disable-next-line no-console
    console.warn('[kami webvr] panel image failed to load', url.slice(0, 80));
    mesh.visible = false;
  };
  img.src = url;
}

/**
 * Procedural "splat-ready" point cloud — shown when a real geomArtifact
 * URL exists but we don't yet have a Gaussian-Splat decoder wired. Each
 * cluster is a small inflated cube with face-rgb tint to mimic a sparse
 * splat preview. Replaced once `kami-engine-sdk/gsplat` (or a generic
 * URL-based decoder) is connected.
 */
function _rebuildGsplatPlaceholder(group: THREE.Group, s: SceneDescriptor): void {
  _clearGroup(group);
  const geom = s.cine?.geomArtifact;
  if (!geom?.url) {
    group.visible = false;
    return;
  }
  group.visible = true;
  const seed = (s.cine?.worldArtifact?.seed ?? 1) >>> 0;
  const rng = _mulberry32(seed);
  const count = Math.min(180, Math.max(40, Math.round((geom.pointCount ?? 8000) / 50)));
  for (let i = 0; i < count; i++) {
    const x = (rng() - 0.5) * 4;
    const y = (rng() - 0.5) * 1.6;
    const z = (rng() - 0.5) * 2;
    const c = 0x303644 + Math.floor(rng() * 0x808080);
    const dot = new THREE.Mesh(
      new THREE.SphereGeometry(0.04 + rng() * 0.05, 6, 4),
      new THREE.MeshBasicMaterial({ color: c }),
    );
    dot.position.set(x, y, z);
    group.add(dot);
  }
}

// ─────────────────────────────────────────────────────────────────────────
// Narrator helpers — turn a SceneDescriptor into something a TTS engine
// reads pleasantly (strip bullets, line breaks, control characters, and
// chop long briefings to a single paragraph).

const _STAGE_NAR: Record<string, string> = {
  detect:      '検知フェーズ。',
  triage:      'トリアージフェーズ。',
  contain:     '封じ込めフェーズ。',
  communicate: '連絡フェーズ。',
  eradicate:   '駆除フェーズ。',
  recover:     '復旧フェーズ。',
  govern:      'ガバナンスフェーズ。',
};

function _speechFromScene(s: SceneDescriptor): string {
  if (s.terminal) {
    const outcome =
      s.terminal === 'success' ? '演習成功。' :
      s.terminal === 'partial' ? '部分的成功。' :
      '演習失敗。';
    return outcome + ' ' + _narratorClean(s.briefing);
  }
  const stagePrefix = _STAGE_NAR[s.stage] ?? '';
  const choices = s.choices.length
    ? ` 選択肢は ${s.choices.length} つ。`
    : '';
  return stagePrefix + _narratorClean(s.briefing) + choices;
}

function _narratorClean(text: string): string {
  return text
    .replace(/[・•・▶]/g, ',')
    .replace(/[#＃]+/g, '')
    .replace(/[*_~`]+/g, '')
    .replace(/\n+/g, ' ')
    .replace(/\s{2,}/g, ' ')
    .trim()
    .slice(0, 280); // SpeechSynthesis chokes on very long strings on iOS Safari
}

/**
 * Pick which choice to fire when the player runs out of time. Heuristic:
 *   1. If the scene exposes any "wait/observe" choice (hint or label),
 *      prefer it — inaction is the canonical timeout in OT-IR drills.
 *   2. Otherwise fall back to the last choice (typically the most-bad
 *      option in our scenarios, since scenarios list "best" first).
 * SceneDescriptor only carries id/label/hint — grade lives on the
 * IncidentChoice object behind the scenes; we can't see it here. The
 * label/hint match is sufficient because authors mark inaction with
 * tokens like 様子見 / 観察 / 待機 / 隠蔽.
 */
function _pickTimeoutChoice(s: SceneDescriptor): string | undefined {
  if (!s.choices.length) return undefined;
  const inactionRe = /様子見|観察|待機|保留|隠蔽|遅延|wait|observe|delay|hold/i;
  for (const c of s.choices) {
    if (inactionRe.test(c.label) || (c.hint && inactionRe.test(c.hint))) return c.id;
  }
  return s.choices[s.choices.length - 1].id;
}

function _hashStr(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193);
  return h >>> 0;
}

function _mulberry32(a: number): () => number {
  return function () {
    a |= 0; a = (a + 0x6D2B79F5) | 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function _clearGroup(g: THREE.Group) {
  for (let i = g.children.length - 1; i >= 0; i--) {
    const c = g.children[i];
    g.remove(c);
    if ((c as THREE.Mesh).geometry) (c as THREE.Mesh).geometry.dispose?.();
    const m = (c as THREE.Mesh).material as THREE.Material | THREE.Material[] | undefined;
    if (Array.isArray(m)) m.forEach((x) => x.dispose?.());
    else m?.dispose?.();
  }
}

// ─────────────────────────────────────────────────────────────────────────
// Magic-window deviceorientation (iOS Safari path)

interface DeviceOrient { update(): void; dispose(): void }
interface MouseLook { dragging(): boolean; dispose(): void }

function _attachMouseLook(canvas: HTMLCanvasElement, camera: THREE.PerspectiveCamera): MouseLook {
  let yaw = 0;
  let pitch = 0;
  let drag = false;
  let lastX = 0;
  let lastY = 0;
  let didDrag = false;
  const SENS = 0.005;
  const PITCH_LIMIT = Math.PI * 0.45;

  function applyToCamera() {
    const eu = new THREE.Euler(pitch, yaw, 0, 'YXZ');
    const q = new THREE.Quaternion().setFromEuler(eu);
    camera.quaternion.copy(q);
  }

  const onDown = (e: PointerEvent) => {
    if (e.button !== undefined && e.button !== 0) return;
    drag = true;
    didDrag = false;
    lastX = e.clientX;
    lastY = e.clientY;
    canvas.setPointerCapture?.(e.pointerId);
  };
  const onMove = (e: PointerEvent) => {
    if (!drag) return;
    const dx = e.clientX - lastX;
    const dy = e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;
    if (Math.abs(dx) + Math.abs(dy) > 3) didDrag = true;
    yaw -= dx * SENS;
    pitch -= dy * SENS;
    if (pitch >  PITCH_LIMIT) pitch =  PITCH_LIMIT;
    if (pitch < -PITCH_LIMIT) pitch = -PITCH_LIMIT;
    applyToCamera();
  };
  const onUp = (e: PointerEvent) => {
    drag = false;
    canvas.releasePointerCapture?.(e.pointerId);
  };
  canvas.addEventListener('pointerdown', onDown);
  canvas.addEventListener('pointermove', onMove);
  canvas.addEventListener('pointerup', onUp);
  canvas.addEventListener('pointercancel', onUp);

  return {
    dragging: () => didDrag,
    dispose: () => {
      canvas.removeEventListener('pointerdown', onDown);
      canvas.removeEventListener('pointermove', onMove);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('pointercancel', onUp);
    },
  };
}

function _attachDeviceOrientation(camera: THREE.PerspectiveCamera): DeviceOrient {
  // Skip the device-orientation magic-window entirely on desktop. Recent
  // MacBooks emit zero-valued deviceorientation events from their built-in
  // accelerometer, which combined with the portrait-phone screen transform
  // would tilt the camera to look at the ceiling immediately after the
  // first frame. Mobile is detected via `pointer: coarse` + maxTouchPoints
  // (the union catches Android Chrome, iOS Safari, and most tablets).
  const isMobile =
    typeof window !== 'undefined' &&
    typeof navigator !== 'undefined' &&
    (
      (typeof window.matchMedia === 'function' && window.matchMedia('(pointer: coarse)').matches) ||
      ((navigator as Navigator & { maxTouchPoints?: number }).maxTouchPoints ?? 0) > 1
    );

  let alpha = 0, beta = 0, gamma = 0;
  const euler = new THREE.Euler();
  const q = new THREE.Quaternion();
  const screenTransform = new THREE.Quaternion();
  let active = false;

  const handler = (e: DeviceOrientationEvent) => {
    const a = (e.alpha ?? 0);
    const b = (e.beta  ?? 0);
    const g = (e.gamma ?? 0);
    // Reject all-zero events (desktop accelerometer noise). A real phone
    // even at rest reports |beta| > 0.5° due to its tilt.
    if (Math.abs(a) + Math.abs(b) + Math.abs(g) < 0.5) return;
    alpha = a * Math.PI / 180;
    beta  = b * Math.PI / 180;
    gamma = g * Math.PI / 180;
    active = true;
  };
  if (isMobile) {
    window.addEventListener('deviceorientation', handler, true);
  }

  function update() {
    if (!active) return;
    if (camera.parent && (camera as any).__xrPresenting) return;
    euler.set(beta, alpha, -gamma, 'YXZ');
    q.setFromEuler(euler);
    // Compensate for portrait orientation of phone
    screenTransform.set(-Math.SQRT1_2, 0, 0, Math.SQRT1_2);
    q.multiply(screenTransform);
    camera.quaternion.copy(q);
  }
  function dispose() {
    if (isMobile) window.removeEventListener('deviceorientation', handler, true);
  }
  return { update, dispose };
}

// ─────────────────────────────────────────────────────────────────────────
// Minimal VR enter button (smartphone Cardboard / Quest browser)

function _makeVrButton(renderer: THREE.WebGLRenderer): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.textContent = 'Enter VR';
  Object.assign(btn.style, {
    position: 'absolute',
    bottom: '24px',
    left: '50%',
    transform: 'translateX(-50%)',
    padding: '14px 24px',
    background: '#ff8a3d',
    color: '#fff',
    border: 'none',
    borderRadius: '999px',
    fontFamily: 'Nunito, sans-serif',
    fontWeight: '700',
    fontSize: '16px',
    boxShadow: '0 4px 16px rgba(0,0,0,0.18)',
    cursor: 'pointer',
    zIndex: '20',
  } as CSSStyleDeclaration);

  let session: XRSession | null = null;
  btn.addEventListener('click', async () => {
    if (session) {
      await session.end();
      session = null;
      btn.textContent = 'Enter VR';
      return;
    }
    try {
      const supported = await (navigator.xr as XRSystem | undefined)?.isSessionSupported?.('immersive-vr');
      if (!supported) {
        btn.textContent = 'WebVR not supported';
        return;
      }
      session = await (navigator.xr as XRSystem).requestSession('immersive-vr', {
        optionalFeatures: ['local-floor', 'bounded-floor'],
      });
      await renderer.xr.setSession(session as unknown as XRSession);
      btn.textContent = 'Exit VR';
      session.addEventListener('end', () => {
        session = null;
        btn.textContent = 'Enter VR';
      });
    } catch (e) {
      btn.textContent = 'VR error';
      // eslint-disable-next-line no-console
      console.warn('[kami webvr] enter VR failed', e);
    }
  });
  return btn;
}
