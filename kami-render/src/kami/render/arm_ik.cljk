(ns kami.render.arm-ik
  "Portable analytic two-bone IK with palette-compatible transform deltas.")

(def contract :kotoba.render/two-bone-arm-ik-v1)
(def family-boundary {:families #{:stylized :photoreal}
                      :implemented-families #{:stylized} :same-api? true})

(defn- v+ [a b] (mapv + a b))
(defn- v- [a b] (mapv - a b))
(defn- v* [s v] (mapv #(* s %) v))
(defn- dot [a b] (reduce + (map * a b)))
(defn- cross [[ax ay az] [bx by bz]]
  [(- (* ay bz) (* az by)) (- (* az bx) (* ax bz)) (- (* ax by) (* ay bx))])
(defn- length [v] (#?(:clj Math/sqrt :cljs js/Math.sqrt) (dot v v)))
(defn- normalize [v]
  (let [n (length v)] (if (> n 1.0e-9) (v* (/ 1.0 n) v) [0.0 -1.0 0.0])))
(defn- clamp [lo hi x] (max lo (min hi x)))

(defn- quaternion-from-to [from to]
  (let [a (normalize from) b (normalize to) d (clamp -1.0 1.0 (dot a b))]
    (if (< d -0.999999)
      (let [axis (normalize (if (< (#?(:clj Math/abs :cljs js/Math.abs) (first a)) 0.9)
                              (cross a [1.0 0.0 0.0]) (cross a [0.0 0.0 1.0])))]
        [(nth axis 0) (nth axis 1) (nth axis 2) 0.0])
      (let [[x y z] (cross a b) s (#?(:clj Math/sqrt :cljs js/Math.sqrt) (* 2.0 (+ 1.0 d)))]
        [(/ x s) (/ y s) (/ z s) (* 0.5 s)]))))

(defn- quat-matrix [[x y z w] bind-start solved-start]
  (let [xx (* x x) yy (* y y) zz (* z z)
        xy (* x y) xz (* x z) yz (* y z) wx (* w x) wy (* w y) wz (* w z)
        r00 (- 1.0 (* 2.0 (+ yy zz))) r01 (* 2.0 (- xy wz)) r02 (* 2.0 (+ xz wy))
        r10 (* 2.0 (+ xy wz)) r11 (- 1.0 (* 2.0 (+ xx zz))) r12 (* 2.0 (- yz wx))
        r20 (* 2.0 (- xz wy)) r21 (* 2.0 (+ yz wx)) r22 (- 1.0 (* 2.0 (+ xx yy)))
        [bx by bz] bind-start [sx sy sz] solved-start
        tx (- sx (+ (* r00 bx) (* r01 by) (* r02 bz)))
        ty (- sy (+ (* r10 bx) (* r11 by) (* r12 bz)))
        tz (- sz (+ (* r20 bx) (* r21 by) (* r22 bz)))]
    ;; Column-major affine matrix.
    [r00 r10 r20 0.0 r01 r11 r21 0.0 r02 r12 r22 0.0 tx ty tz 1.0]))

(defn solve
  "Solve shoulder/elbow/hand centers. Pole is an outward-readable direction."
  [{:keys [family side shoulder elbow hand target pole upper-length lower-length]
    :or {family :stylized}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Arm IK family is not implemented" {:family family})))
  (when-not (and (pos? upper-length) (pos? lower-length))
    (throw (ex-info "Arm segment lengths must be positive"
                    {:upper-length upper-length :lower-length lower-length})))
  (let [to-target (v- target shoulder) raw-distance (length to-target)
        direction (normalize to-target)
        min-reach (+ (#?(:clj Math/abs :cljs js/Math.abs) (- upper-length lower-length)) 0.01)
        max-reach (* 0.985 (+ upper-length lower-length))
        distance (clamp min-reach max-reach raw-distance)
        solved-hand (v+ shoulder (v* distance direction))
        pole-projected (v- pole (v* (dot pole direction) direction))
        pole-dir (if (> (length pole-projected) 1.0e-6)
                   (normalize pole-projected)
                   (normalize (cross direction [0.0 1.0 0.0])))
        x (/ (+ (* upper-length upper-length) (* distance distance)
                (- (* lower-length lower-length))) (* 2.0 distance))
        height (#?(:clj Math/sqrt :cljs js/Math.sqrt)
                (max 0.0 (- (* upper-length upper-length) (* x x))))
        solved-elbow (v+ shoulder (v+ (v* x direction) (v* height pole-dir)))
        upper-dir (normalize (v- solved-elbow shoulder))
        lower-dir (normalize (v- solved-hand solved-elbow))
        upper-q (quaternion-from-to (v- elbow shoulder) upper-dir)
        lower-q (quaternion-from-to (v- hand elbow) lower-dir)
        elbow-angle (#?(:clj Math/acos :cljs js/Math.acos)
                     (clamp -1.0 1.0 (dot (normalize (v- shoulder solved-elbow)) lower-dir)))
        target-error (length (v- target solved-hand))
        clamped? (> target-error 1.0e-6)
        max-angle (- #?(:clj Math/PI :cljs js/Math.PI) 0.10)
        continuity {:upper-end solved-elbow :lower-start solved-elbow
                    :lower-end solved-hand :hand-center solved-hand
                    :elbow-gap 0.0 :hand-gap 0.0 :tolerance 1.0e-6}]
    {:contract contract :family family :side side
     :input {:shoulder shoulder :elbow elbow :hand hand :target target :pole pole
             :upper-length upper-length :lower-length lower-length}
     :centers {:shoulder shoulder :elbow solved-elbow :hand solved-hand
               :upper-center (v* 0.5 (v+ shoulder solved-elbow))
               :lower-center (v* 0.5 (v+ solved-elbow solved-hand))}
     :rotations {:upper upper-q :lower lower-q}
     :palette-deltas {:upper (quat-matrix upper-q shoulder shoulder)
                      :lower (quat-matrix lower-q elbow solved-elbow)
                      :hand [1.0 0.0 0.0 0.0 0.0 1.0 0.0 0.0
                             0.0 0.0 1.0 0.0
                             (- (nth solved-hand 0) (nth hand 0))
                             (- (nth solved-hand 1) (nth hand 1))
                             (- (nth solved-hand 2) (nth hand 2)) 1.0]}
     :continuity continuity
     :metrics {:raw-target-distance raw-distance :solved-reach distance
               :target-error target-error :elbow-angle elbow-angle
               :clamped? clamped? :hyperextended? (>= elbow-angle max-angle)}
     :valid? (and (not (>= elbow-angle max-angle))
                  (<= (:elbow-gap continuity) (:tolerance continuity))
                  (<= (:hand-gap continuity) (:tolerance continuity)))}))
