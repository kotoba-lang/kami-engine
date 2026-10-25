(ns kami.modeler.browser
  (:require ["node:fs/promises" :as fs]
            ["node:http" :as http]
            ["node:path" :as path]))

(def content-types {".html" "text/html; charset=utf-8" ".js" "text/javascript"
                    ".css" "text/css" ".json" "application/json"})

(defn start-server! []
  (let [root (.resolve path "public")
        server (.createServer
                http
                (fn [request response]
                  (let [pathname (.-pathname (js/URL. (.-url request) "http://localhost"))
                        relative (if (= pathname "/") "index.html" (.slice pathname 1))
                        file (.resolve path root relative)]
                    (if-not (.startsWith file (str root (.-sep path)))
                      (do (.writeHead response 403) (.end response "forbidden"))
                      (-> (.readFile fs file)
                          (.then (fn [data]
                                   (.writeHead response 200 #js {"content-type" (get content-types (.extname path file) "application/octet-stream")})
                                   (.end response data)))
                          (.catch (fn [_] (.writeHead response 404) (.end response "not found"))))))))]
    (js/Promise.
     (fn [resolve]
       (.listen server 0 "127.0.0.1"
                (fn [] (resolve {:server server
                                 :url (str "http://127.0.0.1:" (.-port (.address server)))})))))))

(defn close-server! [server]
  (js/Promise. (fn [resolve] (.close server resolve))))

(defn browser-errors! [page]
  (let [errors (atom [])]
    (.on page "pageerror" #(swap! errors conj (.-message %)))
    (.on page "console" #(when (= "error" (.type %)) (swap! errors conj (.text %))))
    errors))

(defn assert! [condition message]
  (when-not condition (throw (js/Error. message))))
