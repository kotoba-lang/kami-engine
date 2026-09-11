(ns kami.render.style-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.render.style :as style]))

(deftest profiles-compile-to-distinct-reusable-pipelines
  (let [toon (style/pipeline-plan {:contract style/contract :profile :stylized})
        real (style/pipeline-plan {:contract style/contract :profile :photoreal})]
    (is (= :mtoon (:surface-shader toon)))
    (is (= :skinned-mtoon (:skinned-surface-shader toon)))
    (is (= :pbr (:surface-shader real)))
    (is (= :pbr (:skinned-surface-shader real)))
    (is (some map? (:passes toon)))
    (is (not-any? #(= :outline (:pass %)) (filter map? (:passes real))))))

(deftest style-does-not-overwrite-explicit-material-model
  (let [m (style/material {:contract style/contract :profile :stylized}
                          {:id :wet-metal :model :pbr :metallic 0.9 :roughness 0.12})]
    (is (= :pbr (:model m)))
    (is (= 0.9 (:metallic m)))
    (is (= 3 (:shade-bands m)))))

(deftest overrides-use-the-shared-style-v1-shape
  (let [resolved (style/scene-style
                  {:render/style
                   {:contract :kotoba.render/style-v1
                    :profile :stylized
                    :shading {:bands 2}
                    :outline {:width-px 2.25}
                    :color-grading {:saturation 1.2}}})]
    (is (= :toon-pbr (get-in resolved [:shading :model])))
    (is (= 2 (get-in resolved [:shading :bands])))
    (is (= 2.25 (get-in resolved [:outline :width-px])))
    (is (= 1.2 (get-in resolved [:color-grading :saturation])))))

(deftest unsupported-capability-never-silently-downgrades
  (testing "reserved inverted hull must wait for an executable pipeline"
    (is (false? (style/supported?
                 {:contract style/contract :profile :stylized
                  :outline {:mode :inverted-hull}}))))
  (testing "contract mismatch is explicit"
    (is (thrown? #?(:clj Exception :cljs js/Error)
                 (style/normalize {:contract :some.future/style-v2
                                   :profile :stylized}))))
  (testing "an outline wider than the shader kernel must not be clamped silently"
    (is (false? (style/supported?
                 {:contract style/contract :profile :stylized
                  :outline {:width-px 9.0}})))))

(deftest legacy-outline-is-retained-as-a-non-pipeline-hint
  (let [m (style/material {:contract style/contract :profile :stylized}
                          {:model :mtoon :outline 0.02})]
    (is (= 0.02 (:outline m)))
    (is (= {:width-world 0.02} (:outline-hint m)))))

(deftest style-postfx-has-one-stable-executable-abi
  (let [pass (style/postfx-execution
              {:contract style/contract :profile :stylized}
              {:width 1920 :height 1080
               :scene-color :hdr-resolve :scene-depth :depth
               :scene-normal :world-normal :normal-space :world
               :output :swapchain})]
    (is (= :kami-render/style-postfx-v1 (:implementation pass)))
    (is (= [0 1 2 3 4] (mapv :binding (:bindings pass))))
    (is (= 64 (:size-bytes (last (:bindings pass)))))
    (is (= [0 8 12 16 20 24 28 32 48 52 56]
           (mapv :offset style/style-postfx-uniform-layout)))
    (is (= 3 (get-in pass [:draw :vertices])))
    (is (= 1 (get-in pass [:uniform :outline-enabled])))
    (is (= 1 (get-in pass [:uniform :tone-map])))
    (is (= :world (get-in pass [:resources :normal-space])))
    (is (= [(/ 1.0 1920) (/ 1.0 1080)]
           (get-in pass [:uniform :inv-resolution])))))

(deftest capabilities-describe-only-executable-style-features
  (is (= #{:none :screen-space}
         (:outline-modes style/execution-capabilities)))
  (is (= #{:world :view} (:normal-spaces style/execution-capabilities)))
  (is (= :naga (:shader-validation style/execution-capabilities)))
  (is (= 8.0 (:max-outline-width-px style/execution-capabilities))))

(deftest photoreal-postfx-uses-same-layout-with-outline-disabled
  (let [uniform (style/postfx-uniform
                 {:contract style/contract :profile :photoreal} 800 600)]
    (is (= 0 (:outline-enabled uniform)))
    (is (= 0.0 (:outline-width-px uniform)))
    (is (= 1.0 (:exposure uniform)))))

(deftest postfx-refuses-incomplete-frame-resources
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (style/postfx-execution
                {:contract style/contract :profile :stylized}
                {:width 640 :height 480 :scene-color :hdr :output :swapchain}))))
