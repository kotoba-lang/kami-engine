# kami-app-modeler

Interactive polygon-modeling application. It executes immutable topology edits
through `kami-engine-modeling` and uploads arbitrary vertex/index meshes into
the real `kotoba-lang/webgpu` pipeline. The Pages build includes WebGPU-first /
WebGL2-fallback rendering, object and component selection, dimension-driven
sketch constraints, topology tools, BSP Boolean workflows, non-destructive
modifiers, UV/image materials, undo/redo, recovery and EDN/glTF exchange.
UI markup and styling are generated exclusively through `kotoba-lang/html` and
`css`.

The large-scene browser gates render 20,000 sphere instances / 11.2 million
resident triangles as one shared-geometry draw. On the named local Chromium
reference run, WebGPU measured 206.5 ms first submission and 0.2 ms p95 submit;
forced WebGL2 measured 917.8 ms first submission and 6.6 ms p95 submit. Both
report the same stable picking sample and source provenance. WebGL2 uses a
cached mat4/color instance buffer rather than 20,000 individual draw calls.

Public app: <https://kotoba-lang.github.io/kami-app-modeler/>

The current 5/5 rating applies to the declared browser polygon-modeler scope.
The roadmap to commercial CAD/CAE maturity—NURBS/B-rep and STEP, assemblies,
associative manufacturing drawings, qualified CAE, a production modifier DAG,
very-large scenes and collaboration history—is governed by
[`kami-engine` ADR-0049](../kami-engine/90-docs/adr/0049-kotoba-3d-suite-commercial-cad-maturity.md).

Verify the deployed application with:

```bash
MODELER_URL=https://kotoba-lang.github.io/kami-app-modeler/ npm run verify:webgpu
npm run verify:webgl2
```

Start with the [user tutorial](docs/tutorial.md). Versioned production bundles
are created from `v*` tags only after the release verification gate succeeds.
