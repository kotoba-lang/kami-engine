;; Ghost Hacker: Shiro & Pico — isekai-network, gameplay, in the kami-clj subset.
;;
;; Faithful reskin of kami-clj-play/games/survivors/logic.clj (ADR-0036/0038):
;; the Rust host (kami-clj-play) compiles this to WASM at startup and drives
;; init + systems; it contains none of this logic itself. The visual profile
;; (colours/sizes) lives in scene.edn, which datalevin owns (author.clj).
;;
;; Design ref: design/01-netsurvivors.edn (concept 1/5). Mapping onto the duo:
;;   - "weapon" system  = シロの一撃 — periodic, ranged, instant "visualize and
;;     purify" of the nearest ghost in range (原作の「もう、視えてる」).
;;   - "contact" system = ピコの刈り取り — anything that touches the duo is
;;     purified on contact (原作の高速連打の代替表現).
;;   - Both read from a single "shiro-pico" entity: in this mode the two move
;;     as one unit (concept-level simplification, not a retcon of the source
;;     material — see design/01-netsurvivors.edn :concept/genre-note).
;;
;; Guest arithmetic is integer-only, so positions use absolute f32 constants and
;; the host keeps the duo inside the arena (see scene.edn :world/arena).

(def max-alive     120)
(def spawn-period  14)
(def fire-period   16)
(def ghost-speed   (f32 95.0))
(def visualize-range (f32 260.0))
(def contact-range (f32 20.0))
(def spawn-radius  (f32 520.0))

(defn duo []
  (nearest-tagged "shiro-pico" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "shiro-pico")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0))))

;; movement: host feeds analog axes (already scaled to px/s) into velocity.
(defsystem control [dt]
  (let [p (duo)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

;; ghost spawn: capped + tick-gated, on a ring around the origin.
(defsystem spawn [dt]
  (when (< (count-tagged "ghost") max-alive)
    (when (zero? (mod (tick-n) spawn-period))
      (let [r (rand-int 4)
            g (spawn-entity "ghost")]
        (cond
          (= r 0) (set-position! g spawn-radius (f32 0.0)    (f32 0.0))
          (= r 1) (set-position! g (f32 -520.0) (f32 0.0)    (f32 0.0))
          (= r 2) (set-position! g (f32 0.0)    spawn-radius (f32 0.0))
          :else   (set-position! g (f32 0.0)    (f32 -520.0) (f32 0.0)))))))

;; ai: every ghost drifts toward the duo (data remanence "haunting" its target).
(defsystem ai [dt]
  (let [p (duo)]
    (when (not= p -1)
      (doseq-entities [g "ghost"]
        (move-toward! g p ghost-speed)))))

;; shiro's visualize-and-purify: each pulse removes the nearest ghost in range.
(defsystem weapon [dt]
  (when (zero? (mod (tick-n) fire-period))
    (let [p (duo)]
      (when (not= p -1)
        (let [hit (nearest-tagged "ghost" (get-x p) (get-y p) visualize-range)]
          (when (not= hit -1)
            (despawn-entity hit)))))))

;; pico's reap: a ghost that reaches the duo is purified on contact.
(defsystem contact [dt]
  (let [p (duo)]
    (when (not= p -1)
      (let [touch (nearest-tagged "ghost" (get-x p) (get-y p) contact-range)]
        (when (not= touch -1)
          (despawn-entity touch))))))
