(ns kami.mangaka.page
  "Work-agnostic graphic-novel PAGE composition (ADR-2606282100, Tier-1 mangaka).

  Ported from mangaka.gftd.ai's `manga-layouts.ts` GRAPHIC_NOVEL_TEMPLATES
  (left-to-right reading): place rendered panel images into a B5 page, draw panel
  frames + gutters, overlay dialogue as speech bubbles and narration as caption
  boxes — turning isolated panels into a readable graphic-novel page.

  Generic: a `page` is just {:layout str :panels [{:id :size :narration :dialogue}…]}
  and `img-of` maps a panel-id → image File (or nil → placeholder). No story,
  character, or world. JVM/Java2D headless — no Canvas-2D, no GPU (page DTP, not
  the engine's wgpu render path)."
  (:require [clojure.java.io :as io]
            [clojure.string :as str])
  (:import [java.awt Color Font BasicStroke RenderingHints GraphicsEnvironment]
           [java.awt.geom RoundRectangle2D$Double]
           [java.awt.image BufferedImage]
           [javax.imageio ImageIO]
           [java.io File]))

;; --- graphic-novel layout templates (x y w h in %, LTR reading order) --------

(def gn-templates
  {1 [[0 0 100 100]]
   2 [[0 0 50 100] [50 0 50 100]]
   3 [[0 0 100 55] [0 55 50 45] [50 55 50 45]]
   4 [[0 0 50 50] [50 0 50 50] [0 50 50 50] [50 50 50 50]]
   5 [[0 0 100 40] [0 40 50 30] [50 40 50 30] [0 70 50 30] [50 70 50 30]]
   6 [[0 0 33.34 50] [33.34 0 33.33 50] [66.67 0 33.33 50]
      [0 50 33.34 50] [33.34 50 33.33 50] [66.67 50 33.33 50]]
   7 [[0 0 100 34] [0 34 33.34 33] [33.34 34 33.33 33] [66.67 34 33.33 33]
      [0 67 33.34 33] [33.34 67 33.33 33] [66.67 67 33.33 33]]
   8 [[0 0 25 50] [25 0 25 50] [50 0 25 50] [75 0 25 50]
      [0 50 25 50] [25 50 25 50] [50 50 25 50] [75 50 25 50]]
   9 [[0 0 33.34 33.34] [33.34 0 33.33 33.34] [66.67 0 33.33 33.34]
      [0 33.34 33.34 33.33] [33.34 33.34 33.33 33.33] [66.67 33.34 33.33 33.33]
      [0 66.67 33.34 33.33] [33.34 66.67 33.33 33.33] [66.67 66.67 33.33 33.33]]})

(defn- grid-template [n]
  (let [cols (long (Math/ceil (Math/sqrt n)))
        rows (long (Math/ceil (/ (double n) cols)))
        cw (/ 100.0 cols) ch (/ 100.0 rows)]
    (vec (for [i (range n) :let [r (quot i cols) c (mod i cols)]]
           [(* c cw) (* r ch) cw ch]))))

(defn template-for [n]
  (cond (<= n 0) []
        (get gn-templates n) (get gn-templates n)
        :else (grid-template n)))

;; --- ネーム-driven layout: vary panel size by the storyboard's intent --------
;; A uniform grid reads flat. Real pages breathe: hero bands, split rows, and
;; full-bleed splashes. We derive that from each panel's :size + the page layout,
;; so the composition follows the artist's ネーム, not a generic grid.

(defn- size-weight [s]
  (let [s (str/lower-case (str s))]
    (cond (re-find #"full" s)            2.6
          (re-find #"two-thirds|wide" s) 2.0
          (re-find #"half" s)            1.5
          (re-find #"one-third|narrow" s) 1.05
          :else 1.5)))

(defn- small? [s]
  (boolean (re-find #"one-third|narrow|half" (str/lower-case (str s)))))

(defn- rows-of
  "Pack panels into rows (reading order): two consecutive small panels share a
  row (side-by-side); everything else gets its own full-width band."
  [panels]
  (loop [ps panels rows []]
    (if (empty? ps)
      rows
      (let [a (first ps) b (second ps)]
        (if (and (small? (:size a)) b (small? (:size b)))
          (recur (drop 2 ps) (conj rows [a b]))
          (recur (rest ps) (conj rows [a])))))))

(defn layout-page
  "→ {:bleed bool :pairs [[panel [x y w h]] ...]} in page-percent units, derived
  from the page :layout + each panel's :size."
  [page]
  (let [ps (:panels page) n (count ps)
        lay (str/lower-case (str (:layout page)))]
    (cond
      (or (<= n 1) (re-find #"splash|full|spread" lay))
      {:bleed true :pairs (mapv vector ps [[0 0 100 100]])}

      (re-find #"grid" lay)
      {:bleed false :pairs (mapv vector ps (template-for n))}

      :else
      (let [rows    (rows-of ps)
            weights (mapv (fn [row] (reduce max (map #(size-weight (:size %)) row))) rows)
            total   (reduce + weights)
            pairs   (volatile! []) y (volatile! 0.0)]
        (doseq [[row w] (map vector rows weights)]
          (let [h (* 100.0 (/ w total)) k (count row) cw (/ 100.0 k)]
            (doseq [[i p] (map-indexed vector row)]
              (vswap! pairs conj [p [(* i cw) @y cw h]]))
            (vswap! y + h)))
        {:bleed false :pairs @pairs}))))

;; --- fonts (pick an installed JP-capable family) -----------------------------

(def ^:private jp-font-family
  (let [avail (set (.getAvailableFontFamilyNames
                    (GraphicsEnvironment/getLocalGraphicsEnvironment)))]
    (some avail ["Hiragino Maru Gothic ProN" "Hiragino Sans" "YuGothic"
                 "Yu Gothic" "Noto Sans CJK JP" "Noto Sans JP"])))

(defn- font [style size]
  (Font. (or jp-font-family Font/SANS_SERIF) style (int size)))

;; --- text wrapping (CJK: break by char to fit width) -------------------------

(defn- wrap [g text max-w]
  (let [fm (.getFontMetrics g)]
    (loop [chars (seq (str text)) cur "" lines []]
      (if (empty? chars)
        (cond-> lines (seq cur) (conj cur))
        (let [c (first chars) cand (str cur c)]
          (if (and (seq cur) (> (.stringWidth fm cand) max-w))
            (recur chars "" (conj lines cur))
            (recur (rest chars) cand lines)))))))

;; --- drawing helpers ---------------------------------------------------------

(defn- aa! [g]
  (doto g
    (.setRenderingHint RenderingHints/KEY_ANTIALIASING RenderingHints/VALUE_ANTIALIAS_ON)
    (.setRenderingHint RenderingHints/KEY_INTERPOLATION RenderingHints/VALUE_INTERPOLATION_BILINEAR)
    (.setRenderingHint RenderingHints/KEY_TEXT_ANTIALIASING RenderingHints/VALUE_TEXT_ANTIALIAS_ON)))

(defn- draw-cover [g ^BufferedImage img x y w h]
  (let [g2 (.create g)]
    (try
      (.setClip g2 (int x) (int y) (int w) (int h))
      (let [iw (.getWidth img) ih (.getHeight img)
            s (max (/ (double w) iw) (/ (double h) ih))
            sw (* iw s) sh (* ih s)
            dx (+ x (/ (- w sw) 2.0)) dy (+ y (/ (- h sh) 2.0))]
        (.drawImage g2 img (int dx) (int dy) (int sw) (int sh) nil))
      (finally (.dispose g2)))))

(defn- placeholder [g x y w h id]
  (doto g (.setColor (Color. 233 227 207))
          (.fillRect (int x) (int y) (int w) (int h))
          (.setColor (Color. 150 150 140))
          (.setFont (font Font/PLAIN 22)))
  (.drawString g (str id) (int (+ x 14)) (int (+ y 30))))

(defn- caption-box
  "Narration → a small caption box at the panel's top-left (serif, white)."
  [g text x y w]
  (when (and text (seq (str text)))
    (.setFont g (font Font/PLAIN 22))
    (let [pad 10 maxw (int (- (* w 0.62) (* 2 pad)))
          lines (wrap g text maxw)
          fm (.getFontMetrics g) lh (+ 4 (.getHeight fm))
          bw (+ (* 2 pad) (reduce max 1 (map #(.stringWidth fm %) lines)))
          bh (+ (* 2 pad) (* lh (count lines)))
          bx (+ x 10) by (+ y 10)]
      (doto g (.setColor (Color. 255 253 247 235))
              (.fillRect bx by bw bh)
              (.setColor (Color. 60 60 56)) (.setStroke (BasicStroke. 1.5))
              (.drawRect bx by bw bh))
      (.setColor g (Color. 40 40 38))
      (doseq [[i ln] (map-indexed vector lines)]
        (.drawString g ^String ln (int (+ bx pad)) (int (+ by pad (* (inc i) lh) -6)))))))

(defn- bubble
  "Dialogue → a white rounded speech bubble with a tail + black outline + JP text.
  `cx` is the desired bubble centre; `side` (:l/:r) aims the tail. Returns bottom-y."
  [g text cx top w side]
  (when (and text (seq (str text)))
    (.setFont g (font Font/PLAIN 25))
    (let [pad 17 maxw (int (* w 0.46))
          lines (wrap g text maxw)
          fm (.getFontMetrics g) lh (+ 6 (.getHeight fm))
          tw (reduce max 1 (map #(.stringWidth fm %) lines))
          bw (+ (* 2 pad) tw) bh (+ (* 2 pad) (* lh (count lines)))
          bx (- cx (/ bw 2.0)) by top
          rr (RoundRectangle2D$Double. bx by bw bh 38 38)
          ;; tail: a small triangle dropping from the bubble toward the speaker
          tailx (if (= side :r) (- (+ bx bw) (* bw 0.28)) (+ bx (* bw 0.16)))
          txs (int-array [(int tailx) (int (+ tailx 26)) (int (+ tailx (if (= side :r) -2 28)))])
          tys (int-array [(int (+ by bh -4)) (int (+ by bh -4)) (int (+ by bh 24))])]
      (doto g (.setColor Color/WHITE) (.fillPolygon txs tys 3) (.fill rr)
              (.setColor Color/BLACK) (.setStroke (BasicStroke. 3.0))
              (.drawPolygon txs tys 3) (.draw rr)
              ;; paint over the seam where the tail meets the bubble
              (.setColor Color/WHITE) (.fillRect (int (+ tailx 2)) (int (+ by bh -7)) 22 6))
      (.setColor g Color/BLACK)
      (doseq [[i ln] (map-indexed vector lines)]
        (.drawString g ^String ln
                     (int (- cx (/ (.stringWidth fm ln) 2.0)))
                     (int (+ by pad (* (inc i) lh) -8))))
      (+ by bh 30))))

;; --- page composition --------------------------------------------------------

(def PAGE-W 1075) (def PAGE-H 1518)   ; B5 @ ~150dpi
(def MARGIN 38) (def GUTTER 16)

(defn compose-page!
  "Compose one storyboard `page` ({:layout :panels [{:id :size :narration :dialogue}…]})
  into a B5 PNG at `out`. `img-of` maps a panel-id → image File (or nil → placeholder)."
  [page img-of out]
  (let [{:keys [bleed pairs]} (layout-page page)
        canvas (BufferedImage. PAGE-W PAGE-H BufferedImage/TYPE_INT_RGB)
        g (.createGraphics canvas)
        ;; a full-bleed splash uses the whole sheet; otherwise inset by MARGIN
        m  (if bleed 0 MARGIN)
        gut (if bleed 0 GUTTER)
        cx0 m cy0 m cw (- PAGE-W (* 2 m)) ch (- PAGE-H (* 2 m))]
    (aa! g)
    (.setColor g (Color. 22 20 18)) (.fillRect g 0 0 PAGE-W PAGE-H) ; ink page base = crisp gutters
    (doseq [[idx [panel [px py pw ph]]] (map-indexed vector pairs)]
      (let [x (+ cx0 (* cw (/ px 100.0)) (/ gut 2.0))
            y (+ cy0 (* ch (/ py 100.0)) (/ gut 2.0))
            w (- (* cw (/ pw 100.0)) gut)
            h (- (* ch (/ ph 100.0)) gut)
            f (img-of (:id panel))]
        (if (and f (.exists ^File f))
          (draw-cover g (ImageIO/read ^File f) x y w h)
          (placeholder g x y w h (:id panel)))
        ;; confident black frame (heavier on a bleed splash)
        (doto g (.setColor Color/BLACK) (.setStroke (BasicStroke. (float (if bleed 10 5))))
                (.drawRect (int x) (int y) (int w) (int h)))
        (caption-box g (:narration panel) x y w)
        ;; alternate bubble side per panel for rhythm; keep inside the panel
        (let [side (if (even? idx) :l :r)
              cx (+ x (* w (if (= side :l) 0.42 0.58)))]
          (loop [ds (:dialogue panel) top (+ y 22)]
            (when (seq ds)
              (let [nt (bubble g (:text (first ds)) cx top w side)]
                (recur (rest ds) (or nt (+ top 96)))))))))
    (.dispose g)
    (io/make-parents out)
    (ImageIO/write canvas "png" (File. ^String out))
    out))
