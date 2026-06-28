(ns sip.page
  "Spirit in Physics PAGE composition — the WORK-SPECIFIC facade over the generic
  mangaka page commons (`kami.mangaka.page`, ADR-2606282100).

  The komawari templates + Java2D DTP (frames, gutters, bubbles, captions) now
  live in `kami-mangaka-page-clj`. This ns keeps only what is about Spirit in
  Physics: wiring `sip.storyboard` pages + the on-disk panel renders into the
  generic `page/compose-page!`."
  (:require [clojure.java.io :as io]
            [kami.mangaka.page :as page]
            [sip.storyboard :as sb]))

(defn compose-chapter!
  "Compose every page of `chapter` (Vol.1) whose panels have images in `img-dir`,
  writing page PNGs to `out-dir`. Returns the list of page PNG paths."
  [chapter img-dir out-dir]
  (let [img-of (fn [id] (io/file img-dir (str id ".png")))]
    (->> (sb/pages :vol01-water-city)
         (filter #(= (:chapter %) chapter))
         (mapv (fn [pg]
                 (let [out (str out-dir "/ch" (format "%02d" chapter)
                                "-p" (format "%02d" (:page pg)) ".png")]
                   (page/compose-page! pg img-of out)))))))

(defn -main
  "page <chapter> <img-dir> <out-dir> — compose a chapter's pages from panel PNGs."
  [& [chap img-dir out-dir]]
  (let [c (Long/parseLong (or chap "2"))
        img (or img-dir ".")
        out (or out-dir "./pages")
        ps (compose-chapter! c img out)]
    (println "composed" (count ps) "page(s):")
    (doseq [p ps] (println "  " p))
    (flush)))
