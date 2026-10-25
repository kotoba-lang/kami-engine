(ns kami.creative-studio.media
  "Unified movie + music authoring document. This namespace owns only the
  cross-domain binding; video and music semantics remain authoritative in
  kami-eizo-timeline and kami-ongaku-project."
  (:require [kami.eizo.timeline :as eizo]
            [kami.eizo.timeline.timecode :as timecode]
            [kami.ongaku.project :as ongaku]
            [douga.ffmpeg :as ffmpeg]))

(def schema "kami.media-project/v1")

(defn project
  [{:keys [id name video music assets output metadata]
    :or {assets [] metadata {}}}]
  {:media/schema schema
   :media/id id
   :media/name name
   :media/video video
   :media/music music
   :media/assets (vec assets)
   :media/output (merge {:width 1920 :height 1080 :fps 30
                         :video-codec :h264 :audio-codec :aac}
                        output)
   :media/metadata metadata})

(defn video-duration-seconds [p]
  (let [{:keys [num den]} (get-in p [:media/video :timeline/timebase])]
    (when (and (pos-int? num) (pos-int? den))
      (/ (* (eizo/timeline-duration (:media/video p)) den) num))))

(defn tick->seconds
  "Integrate a tick position across a changing tempo map. Tempo changes take
  effect at their point tick; time signatures do not affect elapsed time."
  [music tick]
  (let [ppq (:project/ppq music)
        points (:project/tempo-map music)]
    (when (and (nat-int? tick) (pos-int? ppq) (seq points))
      (loop [elapsed 0.0
             cursor 0
             bpm (:tempo-point/bpm (first points))
             remaining (next points)]
        (if-let [point (first remaining)]
          (let [boundary (:tempo-point/tick point)]
            (if (<= tick boundary)
              (+ elapsed (/ (* (- tick cursor) 60.0) (* bpm ppq)))
              (recur (+ elapsed (/ (* (- boundary cursor) 60.0) (* bpm ppq)))
                     boundary (:tempo-point/bpm point) (next remaining))))
          (+ elapsed (/ (* (- tick cursor) 60.0) (* bpm ppq))))))))

(defn seconds->tick
  "Inverse transport mapping for a validated tempo map. Rounds to the nearest
  native music tick."
  [music seconds]
  (let [ppq (:project/ppq music)
        points (:project/tempo-map music)]
    (when (and (number? seconds) (not (neg? seconds)) (pos-int? ppq) (seq points))
      (loop [elapsed 0.0
             cursor 0
             bpm (:tempo-point/bpm (first points))
             remaining (next points)]
        (if-let [point (first remaining)]
          (let [boundary (:tempo-point/tick point)
                segment-seconds (/ (* (- boundary cursor) 60.0) (* bpm ppq))]
            (if (<= seconds (+ elapsed segment-seconds))
              (+ cursor (long (Math/round (/ (* (- seconds elapsed) bpm ppq) 60.0))))
              (recur (+ elapsed segment-seconds) boundary
                     (:tempo-point/bpm point) (next remaining))))
          (+ cursor (long (Math/round (/ (* (- seconds elapsed) bpm ppq) 60.0)))))))))

(defn music-duration-seconds [p]
  (let [music (:media/music p)
        end-tick (reduce max 0 (map #(+ (:clip/start-tick %) (:clip/length-ticks %))
                                    (:project/clips music)))]
    (tick->seconds music end-tick)))

(defn asset-index [p]
  (into {} (map (juxt :asset/id identity)) (:media/assets p)))

(defn validate-project [p]
  (let [video-result (eizo/validate-timeline (:media/video p))
        music-errors (ongaku/validate-project (:media/music p))
        assets (asset-index p)
        referenced (concat
                    (map :clip/source-id
                         (mapcat :track/clips (get-in p [:media/video :timeline/tracks])))
                    (keep #(get-in % [:clip/content :audio/uri])
                          (get-in p [:media/music :project/clips])))
        missing (remove #(or (contains? assets %) (re-find #"^(https?://|file:|cid:|blob:)" (str %)))
                        referenced)
        vd (video-duration-seconds p)
        md (music-duration-seconds p)]
    (vec
     (concat
      (when-not (= schema (:media/schema p))
        [{:problem/type :media/unsupported-schema :problem/value (:media/schema p)}])
      (map #(assoc % :problem/domain :video) (:errors video-result))
      (map (fn [message] {:problem/type :media/music-invalid
                          :problem/domain :music :problem/detail message}) music-errors)
      (map (fn [id] {:problem/type :media/missing-asset :asset/id id}) (distinct missing))
      (when (and vd md (> (abs (- vd md)) 0.05))
        [{:problem/type :media/duration-mismatch
          :video/seconds vd :music/seconds md}])))))

(defn valid-project? [p] (empty? (validate-project p)))

(defn playhead
  "One transport position represented without replacing either domain's
  native unit."
  [p frame]
  (let [tb (get-in p [:media/video :timeline/timebase])
        seconds (/ (* frame (:den tb)) (:num tb))
        tempo (first (get-in p [:media/music :project/tempo-map]))
        tick (seconds->tick (:media/music p) seconds)]
    {:transport/frame frame
     :transport/timecode (timecode/format-timecode frame tb)
     :transport/seconds seconds
     :transport/tick tick}))

(defn update-video-clip
  "Update one clip by id and recompute duration. The resulting project is
  returned even if invalid so an editor can display validation problems."
  [p clip-id f]
  (update-in p [:media/video :timeline/tracks]
             (fn [tracks]
               (mapv (fn [track]
                       (update track :track/clips
                               (fn [clips]
                                 (->> clips
                                      (mapv (fn [clip]
                                              (if (= clip-id (:clip/id clip))
                                                (let [edited (f clip)]
                                                  (assoc edited :clip/duration
                                                         (- (:clip/source-out edited)
                                                            (:clip/source-in edited))))
                                                clip)))
                                      (sort-by :clip/timeline-start)
                                      vec))))
                     tracks))))

(defn move-video-clip [p clip-id timeline-start]
  (update-video-clip p clip-id #(assoc % :clip/timeline-start (max 0 timeline-start))))

(defn trim-video-clip
  [p clip-id source-in source-out]
  (update-video-clip p clip-id #(assoc % :clip/source-in source-in :clip/source-out source-out)))

(defn split-video-clip
  "Split at an absolute timeline frame. The right clip gets `new-id`."
  [p clip-id frame new-id]
  (let [target (some #(when (= clip-id (:clip/id %)) %)
                     (mapcat :track/clips (get-in p [:media/video :timeline/tracks])))
        offset (- frame (:clip/timeline-start target))]
    (if (or (nil? target) (<= offset 0) (>= offset (:clip/duration target)))
      p
      (update-in p [:media/video :timeline/tracks]
                 (fn [tracks]
                   (mapv
                    (fn [track]
                      (if (some #(= clip-id (:clip/id %)) (:track/clips track))
                        (let [left (assoc target :clip/source-out (+ (:clip/source-in target) offset)
                                         :clip/duration offset)
                              right (assoc target :clip/id new-id
                                           :clip/source-in (+ (:clip/source-in target) offset)
                                           :clip/timeline-start frame
                                           :clip/duration (- (:clip/duration target) offset))]
                          (assoc track :track/clips
                                 (->> (:track/clips track)
                                      (remove #(= clip-id (:clip/id %)))
                                      (concat [left right])
                                      (sort-by :clip/timeline-start) vec)))
                        track))
                    tracks))))))

(defn render-plan
  "Portable host execution plan. Paths are resolved from the asset registry;
  commands are vectors and are never executed here."
  [p workspace]
  (let [assets (asset-index p)
        resolve-path #(or (:asset/path (get assets %)) %)
        timebase (get-in p [:media/video :timeline/timebase])
        frame-seconds #(/ (* % (:den timebase)) (:num timebase))
        video-clips (->> (get-in p [:media/video :timeline/tracks])
                         (filter #(= :video (:track/type %)))
                         (mapcat :track/clips)
                         (sort-by :clip/timeline-start))
        audio-clips (->> (get-in p [:media/music :project/clips])
                         (keep #(get-in % [:clip/content :audio/uri])))
        output (:media/output p)
        segments (mapv (fn [index clip]
                         (let [asset (get assets (:clip/source-id clip))
                               source (resolve-path (:clip/source-id clip))
                               audio (str workspace "/silence-" index ".wav")
                               out (str workspace "/segment-" index ".mp4")
                               moving? (= :video (:asset/kind asset))]
                           (cond-> {:clip/id (:clip/id clip)
                                    :asset/kind (or (:asset/kind asset) :image)
                                    :render (if moving?
                                              (ffmpeg/video-segment-cmd
                                               source out
                                               (merge output
                                                      {:source-start-sec (frame-seconds (:clip/source-in clip))
                                                       :duration-sec (frame-seconds (:clip/duration clip))}))
                                              (ffmpeg/scene-segment-cmd source audio out output))
                                    :output out}
                             (not moving?)
                             (assoc :prepare-audio
                                    (ffmpeg/silent-audio-cmd
                                     audio {:seconds (frame-seconds (:clip/duration clip))})))))
                       (range) video-clips)
        joined (str workspace "/picture.mp4")
        master (str workspace "/master.mp4")]
    {:render/schema "kami.media-render-plan/v1"
     :render/segments segments
     :render/concat-list (ffmpeg/concat-list-text (map :output segments))
     :render/concat (ffmpeg/concat-segments-cmd (str workspace "/segments.txt") joined)
     :render/master (when-let [music-uri (first audio-clips)]
                      (ffmpeg/bgm-mix-cmd joined (resolve-path music-uri) master))
     :render/output (if (seq audio-clips) master joined)}))
