# Maturity

Current declared level: **逍遥 / production-authoring** (2026-07-18).

For this repository, 逍遥 means an author can move through the complete
creative loop without crossing a mocked boundary:

| Gate | Evidence |
|---|---|
| canonical video and music models | `kami-eizo-timeline` and `kami-ongaku-project` are consumed directly |
| portable project document | `kami.media-project/v1` fixture parses as EDN |
| cross-domain integrity | video, music, asset and duration validation in `media/validate-project` |
| edit operations | frame-native move, trim and split with continuity tests |
| transport | frame/SMPTE/seconds/tick mapping, including variable tempo integration |
| usable app surface | Composer inspector, V1/A1 lanes, seek, play/pause and split-at-playhead |
| real output | generated commands execute with real ffmpeg and are inspected with ffprobe |
| browser proof | release build loaded in Chrome; Composer, validation, transport and split exercised |
| explicit execution boundary | library performs no process, file or network IO; host executes plans |

The label does not mean exhaustive commercial-editor feature parity. Pointer
drag handles, waveform/thumbnail caches, multi-selection, undo history, proxy
media and collaborative editing improve speed and comfort, but do not replace
or invalidate the working authoring-to-master path proven by these gates.

Regression commands:

```sh
kbb -M:test
kbb -M:e2e
amu compile --target wasm32-browser app
```
