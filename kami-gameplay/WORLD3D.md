# Third-person 3D worlds

`kami.gameplay.world3d` is a portable Kotoba controller, not a renderer. Feed
`render-ir` to the existing `kami.webgpu/draw!` WebGPU/WebGL2 executor. Hosts
supply input, frame time and UI; scene layout and collision live in EDN.

The authored example is `resources/kami/world3d/station.edn`: a walkable
station, six-piece avatar, static walls/platforms/raised steps, three spatial
interactions, and a perspective camera. The Ghost Hacker consumer in
network-awai/network-isekai uses this same layout for browser and Roblox.

```clojure
(def state (initial-state authored-world))
(def next-state (step authored-world state
                      {:move [0 1] :look [0.02 0] :jump? false :zoom 0}
                      0.016))
(nearby authored-world next-state) ; nearest reachable target, or nil
(render-ir authored-world next-state 1.0 true)
```

Positions are feet-origin metres: `:pos [x y z]`, `:size [width height depth]`.
Each `:instances` entry is ordinary Kami render-IR data with an optional
`:solid? true` collision box. Collision is axis-aligned even if a visual has
`:yaw`; rotated boxes, slopes, rigid-body impulses and moving platforms belong
to a physics adapter, not this starter controller. Avatar meshes can be
replaced with authored/skinned rigs while retaining locomotion.

- `:spawn` / `:respawn-y`: spawn and falling recovery.
- `:controller`: overrides `defaults` (speed, radius, height, gravity, jump
  speed, orbit distance/pitch, interaction radius).
- `:interactions`: stable `:id`, `:pos [x y z]`, plus application-owned metadata.
- `:globals`: existing sky/lighting render-IR data.

Input `:move [right forward]` is relative to the orbit camera, diagonals are
normalized, jump is edge-triggered, `:look [yaw-delta pitch-delta]` is radians,
and `:zoom` changes distance in metres. Pitch/zoom are bounded; elapsed time
is capped after suspended tabs. Static collision slides on walls and lands on
raised surfaces. The orbit camera shortens before hitting a wall. `nearby`
uses all three coordinates, so an object on another storey is not selectable
just because its XZ position matches.

The host must release held inputs on blur/cancel and check proximity before
applying an interaction. Networked hosts must validate proximity against the
server-owned avatar; `nearby` in a browser does not authorize a remote action.
This does not add networking, persistence or monetization.

Verification: five behavior tests (18 assertions) under JVM and Node, plus the
existing gameplay suite; real browser walkthrough uses the existing GPU
renderer. Native Roblox character physics/camera stay with Roblox rather
than pretending this controller is its authoritative movement simulation.

## Integrated sound, effects and animation

`kami.gameplay.presentation` consumes authored `presentation.edn`. One event
(target, world position, cue, timestamp) launches a sound recipe, keyframe
track and GPU effect instances together. `advance` emits sounds only once,
prunes expired effects, limits pending/active events to 64/16 and drops stale
sound bursts after suspended tabs. Animation sampling delegates to the existing
`kami.animation` package, pinned in the engine and gameplay manifests.

The bank includes touch/reassurance/movement, discovery, feedback cut,
synchronization/miss, comfort, steps/jump/landing. Matching stimulus/response
uses the same recipe at different spatial sources; mismatched responses use
the returned modality and delay. Music has eight authored 600ms beats with
three arrangements. The host uses the game's beat phase, not a second free
running music clock. Music/sfx buses, master mute and reduced motion are
required host controls; reduced motion retains color and timing information.

The browser consumer routes synth nodes through the existing audio runtime's
music/sfx mixer and HRTF spatial panners, releasing nodes on completion.
Roblox executes native local Sound/Tween/Part effects from the same bank.
Its audio requires uploaded assets: the game build generates 15 PCM WAVs and
an explicit missing-ID map. It must not claim Roblox sound is playable before
IDs/experience permissions and an actual client session are verified.

Validation: portable presentation tests cover one-shot timing, delayed events,
expiration/budgets, reduced motion and arrangement. Browser host graph tests
use a mock device and verify spatial routing/cleanup/mute, not audible output.
