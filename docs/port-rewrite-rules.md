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
the CI pin (amu `3c9eb7f`, `bin/amu` sha256 `750159f1…54fb`):
`docs/evidence/port-rewrite-rules-20261031.json`.

Authority references are to `kotoba-lang/kotoba-lang` (`lang/`) and
`kotoba-lang/amu` (`seed/HEADS`).

## Rule 1: an atom written inside loop / doseq / dotimes

- **Removes:** `E2140 atom 'a' escapes its let scope`.
- **Authority:** `lang/guest-grammar.edn:527-532` (`:atom-local`, slice 1:
  "reading it inside loop/doseq/dotimes is refused"); `lang/local-state.edn:112-119`
  (`:out-of-scope :writes-under-iteration`: "a future slice may admit it by
  threading the cell through the loop's own accumulator"); `seed/HEADS:372-373`
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
- **Authority:** `seed/HEADS:346-348` (`HD-FN-REF`: `(fn-ref f)` for a
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
- **Authority:** `seed/HEADS:328-329` (`HD-SETLIT`: a set literal is keyword-only;
  `typed-set-new` builds the others) and `seed/HEADS:291` (`HD-SET-NEW`).
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
  not yet by the native checker; JUN-730), `seed/HEADS:346` (`fn-ref`), and the
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

## Ledger

`scripts/migration_ledger.py` splits the former `atom-escape` blocker:
`atom-local-iteration` (a let-bound atom written inside loop/doseq/dotimes,
rule 1), `atom-global` (top-level def/defonce atom, out of scope) and the
remaining `atom-escape` (swap!/reset! on an atom passed in, stored or captured;
hand work). The detection is a regex/form scan over the CLJK, an estimate of the
work rather than a proof. `dispatch-forms` links JUN-730 and this rule set;
`keyword-map-literal` links JUN-728 and `destructuring` JUN-729.
