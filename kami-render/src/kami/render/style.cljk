(ns kami.render.style
  "Stable render-style boundary shared by games and KAMI renderer hosts.

  A style selects defaults and a render pipeline; a material `:model` still
  selects the shader for one surface.  Keeping those concepts separate lets a
  stylized scene use a physically shaded metal and a photoreal scene use an
  unlit display without inventing incompatible material schemas.")

(def contract :kotoba.render/style-v1)

(def ^:private capabilities
  {:shading-models #{:toon-pbr :pbr}
   :outline-modes #{:none :screen-space}
   :reserved-outline-modes #{:inverted-hull}})

(def profiles
  {:stylized
   {:contract contract
    :profile :stylized
    :shading {:model :toon-pbr
              :bands 3
              :threshold 0.46
              :smoothness 0.06}
    :outline {:mode :screen-space
              :width-px 1.5
              :color [0.08 0.09 0.12 1.0]
              :depth-threshold 0.1
              :normal-threshold 0.2}
    :color-grading {:tone-map :aces
                    :saturation 1.08
                    :contrast 1.06}}

   :photoreal
   {:contract contract
    :profile :photoreal
    :shading {:model :pbr}
    :outline {:mode :none
              :width-px 0.0
              :color [0.0 0.0 0.0 1.0]
              :depth-threshold 0.1
              :normal-threshold 0.2}
    :color-grading {:tone-map :aces
                    :saturation 1.0
                    :contrast 1.0}}})

(defn- deep-merge
  [& maps]
  (letfn [(merge* [& xs]
            (if (every? map? xs)
              (apply merge-with merge* xs)
              (last xs)))]
    (apply merge* maps)))

(defn normalize
  "Resolve a partial `:render/style` value against its named profile.

  Unknown contracts, profiles and currently unavailable pipeline modes fail
  loudly.  This prevents a requested visual tier from silently becoming a
  cheaper/different renderer."
  [style]
  (let [style (or style {})
        profile (or (:profile style) :photoreal)
        base (get profiles profile)
        result (deep-merge base style)
        requested-contract (:contract result)
        shading-model (get-in result [:shading :model])
        outline-mode (get-in result [:outline :mode])]
    (when-not base
      (throw (ex-info "Unknown KAMI render style profile"
                      {:profile profile :known-profiles (set (keys profiles))})))
    (when-not (= contract requested-contract)
      (throw (ex-info "Unsupported KAMI render style contract"
                      {:contract requested-contract :supported contract})))
    (when-not (contains? (:shading-models capabilities) shading-model)
      (throw (ex-info "Unsupported KAMI shading model"
                      {:model shading-model
                       :supported (:shading-models capabilities)})))
    (when (contains? (:reserved-outline-modes capabilities) outline-mode)
      (throw (ex-info "KAMI outline mode is reserved but not executable"
                      {:mode outline-mode :supported (:outline-modes capabilities)})))
    (when-not (contains? (:outline-modes capabilities) outline-mode)
      (throw (ex-info "Unsupported KAMI outline mode"
                      {:mode outline-mode :supported (:outline-modes capabilities)})))
    (when (and (= :screen-space outline-mode)
               (not (and (number? (get-in result [:outline :width-px]))
                         (<= 1.0 (get-in result [:outline :width-px]) 8.0))))
      (throw (ex-info "Screen-space outline width must be within executable range"
                      {:width-px (get-in result [:outline :width-px])
                       :range [1.0 8.0]})))
    result))

(defn scene-style
  "Return a validated style from a render-IR/scene map."
  [scene]
  (normalize (:render/style scene)))

(defn pipeline-plan
  "Compile style data into renderer-neutral shader/pass intent.

  Hosts map these stable ids to WebGPU/WebGL/native pipelines.  The result is
  data rather than backend objects, so kotoba-lang/webgpu can consume it
  without duplicating style policy."
  [style]
  (let [{:keys [profile shading outline color-grading] :as resolved}
        (normalize style)
        toon? (= :toon-pbr (:model shading))
        outlined? (= :screen-space (:mode outline))]
    {:contract contract
     :profile profile
     :surface-shader (if toon? :mtoon :pbr)
     :skinned-surface-shader (if toon? :skinned-mtoon :pbr)
     :passes (cond-> [:opaque :alpha-mask :alpha-blend]
               outlined? (conj {:pass :outline
                                :implementation :kami-postfx/screen-space
                                :params (dissoc outline :mode)})
               true (conj {:pass :color-grading :params color-grading}))
     :material-defaults (if toon?
                          {:model :mtoon
                           :shade-bands (:bands shading)
                           :shade-threshold (:threshold shading)
                           :shade-smoothness (:smoothness shading)}
                          {:model :pbr :metallic 0.0 :roughness 0.5})
     :resolved-style resolved}))

(def style-postfx-bindings
  "Portable group(0) ABI for `src/shaders/style_postfx.wgsl`."
  [{:binding 0 :name :scene-color :resource :texture-2d-f32
    :semantic :resolved-single-sample-linear-hdr}
   {:binding 1 :name :scene-depth :resource :texture-depth-2d
   :semantic :webgpu-device-depth-0-to-1}
   {:binding 2 :name :scene-normal :resource :texture-2d-f32
    :semantic :unit-normal-encoded-0-to-1-zero-background
    :space :backend-declared-world-or-view}
   {:binding 3 :name :scene-sampler :resource :filtering-sampler}
   {:binding 4 :name :params :resource :uniform-buffer
    :size-bytes 64 :layout :kami.render/style-postfx-v1}])

(def style-postfx-uniform-layout
  "WGSL uniform byte offsets. Scalar enum values are documented by `postfx-uniform`."
  [{:field :inv-resolution :offset 0 :type :vec2-f32}
   {:field :outline-width-px :offset 8 :type :f32}
   {:field :depth-threshold :offset 12 :type :f32}
   {:field :normal-threshold :offset 16 :type :f32}
   {:field :saturation :offset 20 :type :f32}
   {:field :contrast :offset 24 :type :f32}
   {:field :exposure :offset 28 :type :f32}
   {:field :outline-color :offset 32 :type :vec4-f32}
   {:field :outline-enabled :offset 48 :type :u32}
   {:field :tone-map :offset 52 :type :u32}
   {:field :_pad :offset 56 :type :vec2-u32}])

(def execution-capabilities
  "Capabilities proven by the shared style-v1 shader asset and contract tests."
  {:implementation :kami-render/style-postfx-v1
   :shader "shaders/style_postfx.wgsl"
   :shader-validation :naga
   :outline-modes #{:none :screen-space}
   :normal-spaces #{:world :view}
   :tone-maps #{:none :aces}
   :max-outline-width-px 8.0
   :input-color :resolved-single-sample-linear-hdr
   :output-color :display-linear-to-srgb-target})

(defn postfx-uniform
  "Build the named 64-byte style-postfx uniform value.

  The host is responsible for std140/WGSL-compatible packing in the documented
  field order; named data keeps this CLJC contract portable."
  [style width height]
  (when-not (and (pos? width) (pos? height))
    (throw (ex-info "Style postfx target dimensions must be positive"
                    {:width width :height height})))
  (let [{:keys [outline color-grading]} (normalize style)
        outline-enabled (= :screen-space (:mode outline))]
    {:inv-resolution [(/ 1.0 width) (/ 1.0 height)]
     :outline-width-px (if outline-enabled (:width-px outline) 0.0)
     :depth-threshold (:depth-threshold outline)
     :normal-threshold (:normal-threshold outline)
     :saturation (:saturation color-grading)
     :contrast (:contrast color-grading)
     :exposure (or (:exposure color-grading) 1.0)
     :outline-color (:color outline)
     :outline-enabled (if outline-enabled 1 0)
     :tone-map (case (:tone-map color-grading) :aces 1 :none 0
                     (throw (ex-info "Unsupported KAMI tone map"
                                     {:tone-map (:tone-map color-grading)
                                      :supported #{:aces :none}})))
     :_pad [0 0]}))

(defn postfx-execution
  "Return an executable fullscreen-pass contract for a style-v1 frame.

  `attachments` must name real upstream resources. Missing G-buffer inputs are
  rejected even when outline is disabled so one stable bind-group layout works
  for both built-in profiles."
  [style {:keys [width height scene-color scene-depth scene-normal normal-space output]
          :as attachments}]
  (let [missing (->> [:scene-color :scene-depth :scene-normal :normal-space :output]
                     (remove #(some? (get attachments %)))
                     vec)]
    (when (seq missing)
      (throw (ex-info "Style postfx execution is missing frame attachments"
                      {:missing missing})))
    (when-not (#{:world :view} normal-space)
      (throw (ex-info "Style postfx normal space must be :world or :view"
                      {:normal-space normal-space})))
    {:pass :style-postfx
     :implementation :kami-render/style-postfx-v1
     :shader "shaders/style_postfx.wgsl"
     :entry-points {:vertex :vs-main :fragment :fs-main}
     :draw {:vertices 3 :instances 1}
     :target {:texture output :color-space :srgb}
     :bindings style-postfx-bindings
     :resources {:scene-color scene-color
                 :scene-depth scene-depth
                 :scene-normal scene-normal
                 :normal-space normal-space}
     :uniform (postfx-uniform style width height)}))

(defn material
  "Apply style defaults without replacing explicit per-material choices.

  Legacy numeric `:outline` is translated into a material-local hint only;
  scene outline execution remains owned by `:render/style`."
  [style material]
  (let [defaults (:material-defaults (pipeline-plan style))
        legacy-outline (:outline material)]
    (cond-> (merge defaults material)
      (number? legacy-outline)
      (assoc :outline-hint {:width-world legacy-outline}))))

(defn supported?
  "True when a style can execute without a silent fallback."
  [style]
  (try (normalize style) true
       (catch #?(:clj Exception :cljs :default) _ false)))
