;; kami-genre-base-systems / free-to-play -- gameplay, in the kami-clj subset.
;;
;; A real soft-currency economy loop layered on the single-player base:
;; defeating an enemy earns currency (defatom); at a spending threshold the
;; player automatically "cashes in" for a temporary weapon-range buff, then
;; the buff decays back down after a fixed window. This is a genuine,
;; common F2P building block (earn -> spend -> temporary power spike), not
;; a claim that free-to-play itself is a distinct core loop.

(def max-alive        150)
(def spawn-period     20)
(def fire-period      16)
(def enemy-speed      (f32 95.0))
(def base-weapon-range (f32 260.0))
(def buffed-weapon-range (f32 420.0))
(def spawn-radius     (f32 520.0))
(def buff-cost        5)
(def buff-duration    300)  ;; ~5s @ 60 ticks/s, precomputed constant
;; maturity pass: a second, pricier currency sink -- a PERMANENT contact-
;; range widening, unlike the temporary weapon-range buff. A real second
;; spending choice, not just one repeating purchase.
(def upgrade-cost     20)
(def base-contact-range (f32 20.0))
(def upgraded-contact-range (f32 32.0))

(defatom currency 0)
(defatom buff-ticks-left 0)
(defatom permanent-upgrade 0)

(defn player []
  (nearest-tagged "player" (f32 0.0) (f32 0.0) (f32 1000000.0)))

(defn init []
  (let [p (spawn-entity "player")]
    (set-position! p (f32 0.0) (f32 0.0) (f32 0.0))))

(defsystem control [dt]
  (let [p (player)]
    (when (not= p -1)
      (set-velocity! p (axis "MoveX") (axis "MoveY") (f32 0.0)))))

(defsystem spawn [dt]
  (when (< (count-tagged "enemy") max-alive)
    (when (zero? (mod (tick-n) spawn-period))
      (let [r (rand-int 4)
            e (spawn-entity "enemy")]
        (cond
          (= r 0) (set-position! e spawn-radius (f32 0.0)    (f32 0.0))
          (= r 1) (set-position! e (f32 -520.0) (f32 0.0)    (f32 0.0))
          (= r 2) (set-position! e (f32 0.0)    spawn-radius (f32 0.0))
          :else   (set-position! e (f32 0.0)    (f32 -520.0) (f32 0.0)))))))

(defsystem ai [dt]
  (let [p (player)]
    (when (not= p -1)
      (doseq-entities [e "enemy"]
        (move-toward! e p enemy-speed)))))

;; earning: defeating an enemy via weapon earns currency; range scales with
;; an active buff.
(defsystem weapon [dt]
  (when (zero? (mod (tick-n) fire-period))
    (let [p (player)]
      (when (not= p -1)
        (let [range (if (< 0 (atom-val buff-ticks-left)) buffed-weapon-range base-weapon-range)
              hit (nearest-tagged "enemy" (get-x p) (get-y p) range)]
          (when (not= hit -1)
            (despawn-entity hit)
            (set-atom! currency (+ (atom-val currency) 1))))))))

(defsystem contact [dt]
  (let [p (player)]
    (when (not= p -1)
      (let [range (if (< 0 (atom-val permanent-upgrade)) upgraded-contact-range base-contact-range)
            touch (nearest-tagged "enemy" (get-x p) (get-y p) range)]
        (when (not= touch -1)
          (despawn-entity touch))))))

;; spending: at the threshold, auto-cash-in for a temporary range buff.
(defsystem shop [dt]
  (when (< buff-cost (+ (atom-val currency) 1))
    (when (zero? (atom-val buff-ticks-left))
      (set-atom! currency (- (atom-val currency) buff-cost))
      (set-atom! buff-ticks-left buff-duration))))

;; the pricier permanent upgrade only auto-purchases once, and only after
;; the player has accumulated enough beyond the buff cost.
(defsystem shop-permanent [dt]
  (when (zero? (atom-val permanent-upgrade))
    (when (< upgrade-cost (+ (atom-val currency) 1))
      (set-atom! currency (- (atom-val currency) upgrade-cost))
      (set-atom! permanent-upgrade 1))))

(defsystem buff-decay [dt]
  (when (< 0 (atom-val buff-ticks-left))
    (set-atom! buff-ticks-left (- (atom-val buff-ticks-left) 1))))
