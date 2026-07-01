# kami-mangaka-genko-clj

原稿 (**genko**) manga-editor の **document model + 純ロジック**を cljc SSoT に切り出した
モジュール (ADR-2607020100)。`kami-engine-sdk` の `genko-embed.ts`（WebGPU pentab
エディタ = 自己完結 HTML を返す 2500 行の関数）が inline JS に抱えていた

- doc / page / node **データモデル**（`{:pages [{:nodes [{:id :type :visible :data}]}]}`）
- **node-tree ops**（`all-nodes` / `find-by-nid` / `set-node-parent` / `would-cycle?` /
  `node-visible?` / `reorder-nodes` / `node-tree`）
- **oplog（event-sourcing）**（`record-op` / `replay-oplog`）
- **serialize / deserialize**（`read-doc` / `write-doc` / `normalize`、JSON round-trip）

を、**忠実に純 cljc へ移植**したもの。WebGPU 描画・DOM・B2/PDS I/O は host
（TS/Svelte ランタイム、langgraph host-fn）に残す。

## 忠実点（genko-embed.ts と一致）

- 親子は `:_parent` 文字列ポインタ（`""`=root）、`:_layer` は旧別名（`set-parent` は両方書く）。
- 可視判定は `!==false`（欠損=可視）を再現（`self-visible?` / `node-visible?` は祖先鎖を辿る）。
- node types: `stroke panel tone fukidashi text prompt ai-image ai-desc group link layer`。
- **tone と fukidashi は同一生成リテラル由来**で相互のフィールドを持つ。
- **text node は 3 スキーム併存**（`size`/`dir`/float-color、`fontSize`/`vertical`/`fontFamily`、
  `fontSize`/`font`/hex-color）— rename せず共存。
- JSON キー名（camelCase / 先頭 `_`）を verbatim keyword で保持し round-trip
  （`:activePageIdx` `:_nid` `:fukiType` `:x1` …）。
- `replay-oplog` は `aiGenImage`/`aiGenDesc`/`scaleNode` を no-op（op に決定的再現用の
  payload が無いため。genko と同じ）。

## expression 語彙の共有・storyboard 橋渡し

fukidashi 形（`oval/jagged/cloud/square/wavy`）は `kami.mangaka.expression` の
`:bubble` 語彙の部分集合。tone-pattern（`dot/line/cross/grad`）は expression の
背景トーン（`:dot/:hatching/:gradient`）へ写像。`page->storyboard` は genko page を
`kami.mangaka.text` / `analyzeExpression` が消費できる `{:panels [...]}` に射影する
（panel 子孫の fukidashi>text → dialogue(:bubble)、square fuki → narration、tone → :tone）。

```clojure
(require '[kami.mangaka.genko :as g] '[kami.mangaka.expression :as e])
(-> (g/read-doc json-string)          ; B2 の genko doc(JSON) を読む
    :pages first
    g/page->storyboard                ; → {:panels [...]}
    (->> (e/analyze-page (e/load-patterns) cast)))  ; 表情・薄さ・大きさ・トーンを付与
```

## 純度・可搬性

純データ / 純関数のみ — babashka-safe / JVM・cljs・WASM 可搬。JSON read/write のみ
reader-conditional（clj = `clojure.data.json`、cljs = `js/JSON`）。id 生成は明示 id を
渡す純 API が基本で、host 用に `gen-nid`（reader-conditional の非純ヘルパ）を提供。
Sibling of `kami-mangaka-{text,expression,page,scene}-clj`。

## テスト

```bash
clojure -M:test    # model / node-tree / cycle / visibility / reorder / JSON round-trip / oplog replay / bridge
bb test
```
