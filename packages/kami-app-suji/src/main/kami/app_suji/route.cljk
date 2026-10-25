(ns kami.app-suji.route
  "This app's addressable views. The routing itself is `kotoba-lang/route`.

  What is left here is the only part that was ever app-specific: the table. The
  ~76 lines that resolved a fragment and generated a nav were the same in
  `kami-app-daw` and `kami-app-nle` to within a docstring word, and this app made
  three — the extraction trigger the `kotoba-uiux` skill names."
  (:require [route.core :as route]))

(def views
  "Every view this app has, in nav order. The first is the default: the fragment
  is empty on a fresh visit, and an unknown fragment resolves here."
  (route/validate!
   [{:id :simulate :fragment "#/" :label "姿勢シミュレーション"}
    {:id :compare :fragment "#/compare" :label "作業環境の比較"}
    {:id :spine :fragment "#/spine" :label "椎間板の圧縮"}
    {:id :method :fragment "#/method" :label "計算の中身"}]))

(def default-view (first views))

(defn fragment->view [fragment] (route/fragment->view views fragment))
(defn nav [active-id] (route/nav views active-id))
