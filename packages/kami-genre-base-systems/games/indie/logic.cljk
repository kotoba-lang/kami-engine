;; kami-genre-base-systems / indie -- gameplay, in the kami-clj subset.
;;
;; The smallest possible starter kernel in this repo: one entity, one
;; movement system, nothing else. No enemies, no combat, no economy, no
;; win/lose condition -- a genuinely minimal rapid-iteration starting point
;; to build any single-mechanic prototype on top of (see design/indie.edn).

(defn thing []
  (nearest-tagged "thing" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [t (spawn-entity "thing")]
    (set-position! t (f32 0.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [t (thing)]
    (when (not= t -1)
      (set-velocity! t (axis "MoveX") (axis "MoveY") (f32 0.0)))))
