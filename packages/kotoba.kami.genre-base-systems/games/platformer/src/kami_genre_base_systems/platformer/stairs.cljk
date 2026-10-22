(ns kami-genre-base-systems.platformer.stairs
  "Decoupled extension point for diagonal/stair movement.

   physics-2d is being extended (in parallel, in its own repo) with a
   stair-zone/diagonal-movement capability, but its exact function names
   are not yet final -- so this namespace (and the rest of games/platformer)
   never requires physics-2d directly. Instead a host wires in a
   `provider` map shaped like:

     {:in-zone? (fn [entity] bool)        ; is entity currently over a stair zone?
      :resolve  (fn [entity dt] entity')} ; entity with pos/vel adjusted to
                                          ; follow the stair slope for this tick

   Any implementation satisfying this shape -- e.g. a thin adapter over
   physics-2d.stairs' point-in-zone?/resolve-stair-movement (or whatever
   its functions end up being named) -- can be passed in verbatim. Nothing
   here calls physics-2d code, requires it as a project dependency, or
   assumes its final API surface.")

(def no-op-provider
  "Default provider: no entity is ever considered in a stair zone, so
   apply-stairs is a pass-through until a host wires in a real one."
  {:in-zone? (constantly false)
   :resolve (fn [entity _dt] entity)})

(defn apply-stairs
  "Applies `provider`'s stair resolution to `entity` for this tick, if
   (and only if) `entity` is currently in a stair zone per
   `(:in-zone? provider)`; returns `entity` unchanged otherwise.
   `provider` defaults to no-op-provider."
  ([entity dt] (apply-stairs entity dt no-op-provider))
  ([entity dt provider]
   (if ((:in-zone? provider) entity)
     ((:resolve provider) entity dt)
     entity)))
