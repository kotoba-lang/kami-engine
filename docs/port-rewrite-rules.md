# kami port rewrite rules (JUN-731)

Rewrites to apply when porting a kami CLJK source to canonical `.kotoba`, for
constructs the pinned Amu CLI refuses **by design** (not as missing features
that a later Amu slice is expected to admit; rule 4 is the exception, an
interim until JUN-730). Each rule names the refusal it removes, the authority
it rests on, a before/after pair and where it stops.

Every before/after below is a file in `scripts/native/rewrite_rules_corpus/`
and is measured by the `rewrite-rules` gate of `scripts/native_gates.py`: a
`.before` file must be refused with the code on its `;; expect-refusal:` line
(the negative control), each `.after*` file must check and run to its
`;; expect-value:`. `.admitted` files show forms that need no rewrite and
`.out-of-scope` files record refusals these rules do not remove. Results at
the CI pin (amu `05668c7`, `bin/amu` sha256 `13bfdade…4ba11c3`):
`docs/evidence/port-rewrite-rules-20261101c.json` (73 cases). Against the
previous pin `c2186d7` (`port-rewrite-rules-20261101b.json`, 60 cases) amu
PR #1751 (JUN-729: destructuring as reader sugar) and PR #1753 (JUN-734: def
literals D5, variadic `assoc` D6, typed-map `get` / `(:k m d)` D10) admitted
two `.before` forms and changed the code of one:
`kwmap-def-f64.before` and `kwmap-variadic-assoc.before` are now admitted
(the def literal became `kwmap-def-f64.admitted`, the variadic assoc
`kwmap-variadic-assoc.admitted`; the pair's remaining refusal, a destructured
unannotated parameter, is `kwmap-state-param`), and
`kwmap-option-default.before` is E2111 instead of E2119. New cases:
`kwmap-def-vec-mixed-keys` (what D5 still refuses), the admitted
`kwmap-option-default-typed-map`, the `kwmap-variadic-assoc-odd` refusal and
the "Destructuring" cases below. Every rule 1-4 case gives the same result as
at `c2186d7`. `seed/HEADS` line numbers below are at `05668c7`.

Authority references are to `kotoba-lang/kotoba-lang` (`lang/`) and
`kotoba-lang/amu` (`seed/HEADS`).

## Rule 1: an atom written inside loop / doseq / dotimes

- **Removes:** `E2140 atom 'a' escapes its let scope`.
- **Authority:** `lang/guest-grammar.edn:527-532` (`:atom-local`, slice 1:
  "reading it inside loop/doseq/dotimes is refused"); `lang/local-state.edn:112-119`
  (`:out-of-scope :writes-under-iteration`: "a future slice may admit it by
  threading the cell through the loop's own accumulator"); `seed/HEADS:409-410`
  (`HD-ATOM`: the cell may appear only as the first argument of
  swap!/reset!/deref in the binding function).
- **Status:** permanent refusal for now; port by hand-threading the value.
- **Rewrite:** drop the atom; carry its value as a `loop` binding (one more
  `recur` argument) or as the accumulator of `reduce`. The atom's final
  `@a` becomes the loop's result / reduce's return value.

```clojure
;; before (E2140)                        ;; after: loop accumulator -> 3
(let [a (atom 0)]                        (loop [i 0 a 0]
  (dotimes [i 3] (swap! a + i))            (if (< i 3) (recur (+ i 1) (+ a i)) a))
  @a)
                                         ;; after: reduce -> 3
                                         (reduce (fn [acc i] (+ acc i)) 0 [0 1 2])
```

`doseq` over a vector becomes `reduce` over the same vector
(`atom-doseq.*`, 6); a hand-written `loop` that `swap!`s gains the atom as one
more loop binding (`atom-loop.*`, 3). Several atoms written in one loop become
several loop bindings, or a record accumulator for `reduce`.

**Not covered** (each is a corpus `.out-of-scope` case):

| construct | refusal | what it needs |
|---|---|---|
| top-level `(def a (atom ..))` | E2141 | state ability / `:state` kit (state that outlives one call), JUN-651 |
| top-level `(defonce a (atom ..))` | E2003 `defonce` | same |
| an atom captured by `fn`, passed or returned | E2140 | hand work: return the new value from the closure, or the state kit |
| `swap!`/`reset!` inside `and` / `or` / `cond->` / `->` … | E2143 | hand work: hoist into `if`/`when`, which slice 1 admits |

Straight-line writes, including under `if`/`when`/`cond`/`case`, are admitted
as is (`atom-straight-line.admitted`, 1).

## Rule 2: a top-level defn passed as a value

- **Removes:** `E2102 unknown symbol 'f'`.
- **Authority:** `seed/HEADS:379-381` (`HD-FN-REF`: `(fn-ref f)` for a
  single-clause, non-variadic, non-aborting defn of the file; `HD-INVOKE`).
- **Rewrite:** where a module defn's name is used as a value (an argument,
  a let init, a record field), write `(fn-ref f)`; call a fn value held in a
  local with `(invoke c a ..)` (a fn-typed parameter can still be called
  directly).

```clojure
;; before (E2102)                ;; after -> 3
(ap inc2 1)                      (ap (fn-ref inc2) 1)
(let [f inc2] (f 1))             (let [f (fn-ref inc2)] (invoke f 1))
```

Passing a defn straight to `map`/`filter`/`reduce` is already admitted
(`(reduce + 0 (map inc2 [1 2]))` runs to 7, `fn-ref-hof.admitted`).
**Scope:** a multi-clause, variadic or aborting defn is refused by `fn-ref`
(E2134); wrap it in a `(fn [x :T] ..)` literal of the arity needed.

## Rule 3: a numeric set literal

- **Removes:** `E2104 type mismatch in '1': expected :keyword`.
- **Authority:** `seed/HEADS:351-352` (`HD-SETLIT`: a set literal is keyword-only;
  `typed-set-new` builds the others) and `seed/HEADS:314` (`HD-SET-NEW`).
- **Rewrite:** `#{1 2}` → `(typed-set-new [:set :i64] 1 2)`.

```clojure
(contains? #{1 2} 1)    ;; E2104  ->  (contains? (typed-set-new [:set :i64] 1 2) 1)  ; 1
(count #{1 2 3})        ;; E2104  ->  (count (typed-set-new [:set :i64] 1 2 2))      ; 2, duplicates dropped
```

Keyword sets are admitted as literals (`set-keyword.admitted`). **Scope:**
`typed-set-new` holds at most 32 items (a 33rd traps); a larger constant set
needs another representation (a sorted vector plus search, or a `case`).

## Rule 4: reify, defprotocol and closed defmulti (interim, until JUN-730)

- **Removes:** `E2101 unknown or unsupported form 'reify'`; `E2003 unsupported
  top-level form 'defprotocol'` / `'defmulti'`.
- **Authority:** `lang/guest-grammar.edn:498-501` (`:closed-multimethod`, desugars
  to one module-local defn with static `case` dispatch: admitted by the spec,
  not yet by the native checker; JUN-730), `seed/HEADS:379` (`fn-ref`), and the
  record field type `[:fn [[T ..] R]]`.
- **reify / defprotocol → record of fn-ref closures.** Each protocol method
  becomes a record field of fn type; each implementation is a defn put in the
  field with `fn-ref`; a call becomes `(invoke (:method obj) args ..)`.

```clojure
;; before (E2101)                          ;; after -> 12
(reify Shape (area [_ w h] (* w h)))       (defn rect-area [w :i64 h :i64] :i64 (* w h))
                                           (defrecord Impl [area [:fn [[:i64 :i64] :i64]]])
                                           (invoke (:area (->Impl (fn-ref rect-area))) 3 4)
```

- **Closed defmulti → `case` on a `:kind` field.** The dispatch value becomes
  a keyword field of a record, the methods become the arms of one `case`, and
  the parameter is typed `[:ref :<ns>/<Record>]` (the namespace is the file's
  own `ns`: `:probe/Shape` in the corpus; another namespace is E2179).

```clojure
;; before (E2003)                          ;; after -> 15
(defmulti area (fn [s] (:kind s)))         (defn area [s [:ref :probe/Shape]] :i64
(defmethod area :rect [s] (* (:w s) (:h s)))  (case (:kind s)
(defmethod area :sq [s] (* (:w s) (:w s)))      :rect (* (:w s) (:h s))
                                                :sq (* (:w s) (:w s))
                                                0))
```

**Scope:** an open multimethod (methods added from other namespaces, hierarchies,
`prefer-method`), a protocol extended to host types (`extend-type js/...`), or a
`reify` that closes over locals needs hand work: the record holds only `fn-ref`
defns, so captured values must become extra record fields passed to the method.
Once JUN-730 admits closed `defmulti`/`defmethod`, the multimethod half of this
rule is dropped.

## Keyword maps (JUN-732)

Since amu `c2186d7` (PR #1750, JUN-728) a keyword map literal such as `{:a 1}`
is a **closed record**: `(:k m)`, `get`/`assoc` of a present key, nesting,
loop/recur, an unannotated defn result and `:f64` values are admitted. What a
port has to change depends on how the map is used. The table gives the
representation per pattern (decided in JUN-732 from a reader-based survey of
the 391 files the former `keyword-map-literal` blocker flagged), the refusal
the CLJK form gets, the ledger blocker that tracks it, and the follow-up issue
where the representation is not yet complete. The corpus cases are
`scripts/native/rewrite_rules_corpus/kwmap-*`.

| pattern | refusal | port as | blocker | follow-up |
|---|---|---|---|---|
| constant table read by a literal key | admitted | the literal (a closed record) | none | |
| constant table holding f64 / bool values, or a vector of maps | admitted at `05668c7` (JUN-734 D5; was E2124) | the literal | none | |
| constant vector whose items differ in type (maps with different keys or value types, `[1 "b"]`, `[0 0.5]`) or holding more than 64 non-integer items | E2116 | one defrecord with `[:option T]` fields built by `->Rec`, or a typed map | `kwmap/def-literal` | JUN-734 |
| table looked up or iterated by a runtime key | E2127, E2104 (`contains?`) | `(typed-map-new [:map :keyword V])`, V a scalar, `[:ref R]` or `[:fn ..]` | `kwmap/table-runtime-key` | JUN-735 |
| local fixed-shape record | admitted | as is | none | |
| accumulator in reduce/loop | admitted when the keys are fixed | a record loop slot; a typed map when keys vary | `kwmap/runtime-key` when keyed at run time | |
| option map with defaults (`:or`, `(get m :k d)`, `(:k m d)`, `merge` of defaults) | on a record: E2104 (`get` default, `or`, `:or`), E2103 (`(:k m d)`), E2111 (`:keys` name written nowhere else); on a typed map `(get m :k d)` and `(:k m d)` are admitted at `05668c7` (JUN-734 D10) | defrecord with `[:option T]` fields + `option-or`, or a typed map + `(get m k d)` / `(:k m d)` | `kwmap/option-default` | JUN-733 (D2) |
| `ex-info` data | admitted (it becomes `:document`) | as is; read back with `document-get` | none | |
| runtime-keyed map, entity table | E2127, E2104 | `[:map :i64\|:keyword [:ref R]]` | `kwmap/runtime-key` | JUN-735 (D7 keyword-key iteration, D8 31-entry cap) |
| EDN I/O, shape tested at run time | E2101 | `:document` and the document ops | `kwmap/dynamic` | JUN-735 (D9: no f64 in a document) |
| map crossing a fn or module boundary | E2104, E2105 (a bare record name as the type, since `05668c7`), E2179 | defrecord and `[:ref :ns/R]` parameters, built with `->R` / `map->R`; `:ns` is the file's own namespace | `kwmap/param-row` | JUN-733 (D1, D2) |
| nil field | E2102 | an `[:option T]` field | `kwmap/nil-field` | JUN-733 (D4) |
| `{}` | E2133 | `(typed-map-new [:map K V])` or a full record | `kwmap/empty` | JUN-733 (D3) |
| `merge` / `dissoc` / `select-keys` / `update` / `*-in` | E2101, E2104 | nested `assoc` on a declared record | `kwmap/row-op` | JUN-733 (D3) |
| fn values in a map | E2124 (def), E2128 | `[:map :keyword [:fn ..]]` with `fn-ref` / `invoke`, or `case` on the key | `kwmap/fn-value` | JUN-732 |
| `contains?` | E2104 | a typed map | `kwmap/runtime-key` (non-literal key) | |
| `#js`, `clj->js`, hiccup attrs | host | a host adapter outside the guest | `host-interop` | |
| variadic `assoc` | admitted at `05668c7` (JUN-734 D6; was E2103); an odd key/value count stays E2103 | as is | none (the `kwmap/variadic-assoc` blocker is removed) | |

Each before/after below is a corpus case (`kwmap-<case>.before` /
`.after`), measured at the pin. Records use the file's own namespace, here
`probe`.

**Constant table read by a runtime key** (`kwmap-table-runtime-key`, before
E2104, after 25):

```clojure
;; before
(def simple-opcode {:drop 26 :unreachable 0 :nop 1})
(defn- op-byte [op :keyword] :i64 (if (contains? simple-opcode op) (get simple-opcode op 0) -1))
;; after
(def simple-opcode (typed-map-new [:map :keyword :i64] :drop 26 :unreachable 0 :nop 1))
(defn- op-byte [op :keyword] :i64 (get simple-opcode op -1))
```

**Table of nested records** (`kwmap-table-of-records`, before E2127, after 880):

```clojure
;; before
(def skills {:fireball {:label "Fireball" :element :fire :audio {:wave "sawtooth" :freq 180 :to 620}} ..})
(defn- skill-freq [id :keyword] :i64 (:freq (:audio (get skills id))))
;; after
(defrecord Audio [wave :string freq :i64 to :i64])
(defrecord Skill [label :string element :keyword audio [:ref :probe/Audio]])
(def skills (typed-map-new [:map :keyword [:ref :probe/Skill]]
              :fireball (->Skill "Fireball" :fire (->Audio "sawtooth" 180 620)) ..))
(defn- skill-freq [id :keyword] :i64
  (option-match [:option [:ref :probe/Skill]] (typed-map-get [:map :keyword [:ref :probe/Skill]] skills id)
    0
    s (:freq (:audio s))))
```

**A state map passed between fns** (`kwmap-state-param`, before E2104 for the
unannotated `{:keys ..}` parameter, after 1). Until `05668c7` this was
`kwmap-def-f64` and its before was refused E2124 at the def; the f64 def is now
admitted as written (`kwmap-def-f64.admitted`, 1), so only the parameter needs
the rewrite:

```clojure
;; before
(def default-whip-durations {:windup 0.12 :active 0.18 :recovery 0.22})
(defn- step-whip [{:keys [state timer]} attack? dt durations] ..)
;; after
(defrecord Whip [state :keyword timer :f64])
(defrecord Durations [windup :f64 active :f64 recovery :f64])
(def default-whip-durations (->Durations 0.12 0.18 0.22))
(defn- step-whip [w [:ref :probe/Whip] attack? :bool dt :f64 durations [:ref :probe/Durations]] [:ref :probe/Whip]
  (case (:state w) ..))
```

**Option map with defaults** (`kwmap-option-default`, before E2111 at
`05668c7`, E2119 at `c2186d7`, after 50). On a typed map the default needs no
rewrite: `(get opts :from 0)` and `(:to opts 100)` are admitted
(`kwmap-option-default-typed-map.admitted`, 300):

```clojure
;; before
(defn- tween-value [{:keys [from to duration] :or {from 0 to 100 duration 300}} elapsed] ..)
(tween-value {:duration 200} 100)
;; after
(defrecord Tween [from [:option :i64] to [:option :i64] duration [:option :i64]])
(defn- tween-value [o [:ref :probe/Tween] elapsed :i64]
  (let [from (option-or (:from o) 0) to (option-or (:to o) 100) duration (option-or (:duration o) 300) ..] ..))
(tween-value (->Tween (option-none-of [:option :i64]) (option-none-of [:option :i64]) (option-some-of [:option :i64] 200)) 100)
```

**Entity table, assoc-in / update** (`kwmap-entity-table`, before E2104, after 1):

```clojure
;; before
(defn- spawn [st tag] (let [id (:next st)] (-> st (assoc-in [:ents id] {:tag tag :x 0 :y 0}) (update :next inc))))
(defn- despawn [st id] (update st :ents dissoc id))
;; after
(defrecord Ent [tag :string x :i64 y :i64])
(defrecord St [ents [:map :i64 [:ref :probe/Ent]] next :i64])
(defn- spawn [st [:ref :probe/St] tag :string] [:ref :probe/St]
  (let [id (:next st)] (assoc (assoc st :ents (assoc (:ents st) id (->Ent tag 0 0))) :next (+ id 1))))
(defn- despawn [st [:ref :probe/St] id :i64] [:ref :probe/St] (assoc st :ents (dissoc (:ents st) id)))
```

**Dynamic shape** (`kwmap-document`, before E2101 for `get-in`, after 460):

```clojure
;; before
(defn- arena [s k] (get-in s [:world k] 0))
;; after
(defn- scene [] :document {:game/id :gftd.games/relaxing :game/title "relaxing" :world {:arena 460 :speed 160}})
(defn- doc-at [d :document k :keyword] :document (option-or (document-get d k) (document-null)))
(defn- arena [s :document k :keyword] :i64 (option-or (document-i64-value (doc-at (doc-at s :world) k)) 0))
```

An f64 inside a document is refused (`kwmap-document-f64.out-of-scope`, E2104;
JUN-735 D9).

**nil field** (`kwmap-nil-field`, before E2102, after 1):

```clojure
;; before
(defn- sub-idle [] {:state :idle :timer 0 :kind nil})
;; after
(defrecord Sub [state :keyword timer :i64 kind [:option :keyword]])
(defn- sub-idle [] [:ref :probe/Sub] (->Sub :idle 0 (option-none-of [:option :keyword])))
(defn- fire [s [:ref :probe/Sub] k :keyword] [:ref :probe/Sub] (assoc s :kind (option-some-of [:option :keyword] k)))
```

**fn values** (`kwmap-fn-value`, before E2124, after 10):

```clojure
;; before
(def handlers {:inc (fn [x] (+ x 1)) :dbl (fn [x] (* x 2))})
((get handlers :dbl) 5)
;; after
(defn- inc1 [x :i64] :i64 (+ x 1))
(defn- dbl [x :i64] :i64 (* x 2))
(def handlers (typed-map-new [:map :keyword [:fn [[:i64] :i64]]] :inc (fn-ref inc1) :dbl (fn-ref dbl)))
(defn- run [k :keyword x :i64] :i64
  (option-match [:option [:fn [[:i64] :i64]]] (typed-map-get [:map :keyword [:fn [[:i64] :i64]]] handlers k) x f (invoke f x)))
```

The smaller cases: a map read through an unannotated parameter (E2104) takes a
defrecord and a `[:ref :probe/P]` parameter (`kwmap-param-row`); a `[:ref]` to
another namespace is E2179 (`kwmap-param-other-ns.out-of-scope`); `merge`
(E2101) becomes `assoc` on the declared keys (`kwmap-merge`); `{}` (E2133)
becomes `(typed-map-new [:map :keyword :i64])` (`kwmap-empty`); variadic
`assoc` is admitted as written since `05668c7` (`kwmap-variadic-assoc.admitted`;
an odd count is `kwmap-variadic-assoc-odd.out-of-scope`, E2103); a def vector of
maps with different keys (E2116) becomes a vector of one record with
`[:option T]` fields (`kwmap-def-vec-mixed-keys`, after 5); `keys` of
a keyword-keyed typed map is still refused (E2104,
`kwmap-keyword-keys.out-of-scope`; JUN-735 D7). `ex-info` data is admitted and
read back as a document (`kwmap-ex-info.admitted`, -3):

```clojure
(defn- weapon-at [i :i64] :i64 (if (< i 0) (throw (ex-info "weapon index out of range" {:index i :kind :weapon})) i))
(try (weapon-at -3) (catch e (option-or (document-i64-value (option-or (document-get (ex-data e) :index) (document-null))) 0)))
```

## Destructuring (JUN-729)

Since amu `5667e44c` (PR #1751, in the `05668c7` pin) a destructuring pattern
is reader sugar that `seed/21-check` expands before typing into plain
temp-bound `nth` / `get` / `vector-drop` bindings (`seed/HEADS:189-197`,
`HD-LET`; `HD-LOOP`; `seed/HEADS:390-395`, `HD-AMP`). Admitted as written in
`let` and `loop`: vector patterns (nested, `:as`, `_`, `&` over a vector) and
map patterns (`:keys`, `{x :k}`, `:as`) over a record or keyword map literal
whose keys are present, `:or` over a typed map (`destructuring-let.admitted`,
44; `destructuring-loop.admitted`, 55). `nth` traps on a short vector where
Clojure binds nil.

| construct | refusal | port as | corpus |
|---|---|---|---|
| pattern in a defn parameter without a type | E2104 | annotate it: `[[a b] :vector-i64]`, `[{:keys [..]} [:ref :ns/R]]` | `destructuring-defn-param` (after 14) |
| pattern in a `fn` parameter | E2139 | a plain parameter and a `let` pattern in the body | `destructuring-fn-param` (after 14) |
| pattern in `doseq` / `for` / `when-let` / `if-let` | E2111, E2101 (`for`), E2103 | bind a name, destructure in a `let` | `destructuring-doseq.out-of-scope` |
| `:strs` / `:syms`, a string or other non-keyword key | E2111 | keyword keys | `destructuring-strs.out-of-scope` |
| `&` over a list | E2104 | a vector | `destructuring-rest-list.out-of-scope` |
| `:or` on a record, an absent key (nil lookup) | E2104, E2127 | `[:option T]` fields, or a typed map with `:or` | (`kwmap-option-default`) |
| a `:keys` name never written as a keyword in the module | E2111 | | |

The ledger's `destructuring` blocker counts a file when it has a pattern in a
defn / fn parameter (no CLJK parameter is annotated), in doseq / for /
when-let / if-let bindings, or a `let` / `loop` map pattern or `&` (whether
the value is a record with every key, a typed map, a vector or a list is type
flow, so these count whole); a file whose only patterns are `let` / `loop`
vector patterns without `&` is no longer counted.

## Reader shorthands (JUN-732)

Two reader forms the ledger did not track are refused before type checking,
`E1005 unsupported reader syntax`:

- **`#(..)` fn literal** (`fn-literal`, before E1005, after 5): write
  `(fn [x] ..)`; the parameter takes its type from the context (an annotated
  fn literal parameter is E2139). amu's refactor rule g (fnlit, amu
  `acf8e1d24`) did this rewrite mechanically on the hosted route; it left amu
  with the hosted routes (`157355127`) and the native CLI has no `refactor`
  command, so the rewrite is by hand for now. Tracked with JUN-651.
- **`::k` auto-resolved keyword** (`auto-resolved-keyword`, before E1005,
  after 1): write the qualified `:ns/k`.

```clojure
(reduce + 0 (map #(+ % 1) [1 2]))        ;; E1005  ->  (reduce + 0 (map (fn [x] (+ x 1)) [1 2]))   ; 5
(if (= ::a :probe/a) 1 0)                ;; E1005  ->  (if (= :probe/a :probe/a) 1 0)              ; 1
```

## Ledger

`scripts/migration_ledger.py` splits the former `atom-escape` blocker:
`atom-local-iteration` (a let-bound atom written inside loop/doseq/dotimes,
rule 1), `atom-global` (top-level def/defonce atom, out of scope) and the
remaining `atom-escape` (swap!/reset! on an atom passed in, stored or captured;
hand work). It splits the former `keyword-map-literal` blocker (JUN-732) into
the `kwmap/*` blockers of the table above, read by `kwmap_patterns()` (a small
reader over the CLJK with comments and strings removed) in files that hold a
keyword map literal; local records, constant tables read by a literal key and
`ex-info` data are no longer blockers; at `05668c7` `kwmap/def-literal` is
narrowed to the E2116 vectors and `kwmap/variadic-assoc` is removed (JUN-734),
and `destructuring` is read by `destructuring_patterns()` (JUN-729, above).
`fn-literal` and `auto-resolved-keyword`
track the reader shorthands. The detection is a regex/form scan over the CLJK,
an estimate of the work rather than a proof. `dispatch-forms` links JUN-730
and this rule set, `destructuring` JUN-729.
