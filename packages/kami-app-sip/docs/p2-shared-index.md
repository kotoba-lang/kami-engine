# P2 — 瓶詞 (bottle-letter) の実 Kotoba 連携と共有索引（設計）

ADR-0022 P2（durable / 非毒性・非同期マルチプレイ）の残作業の設計。実装は段階導入。

## 現状（実装済み）

| 層 | JVM（Model A authoring, `sip.store`） | web（runtime, `sip.kotoba`） |
|---|---|---|
| 本体（手紙の内容） | Kotoba CAS（block.put/get）または LocalCas | Kotoba block XRPC または localStorage CAS |
| 索引（season → 手紙一覧） | **ローカル datalevin**（`:sip.letter/*` datom） | **localStorage**（ブラウザ内） |

両クライアントとも **本体は content-addressed（CID）で実 Kotoba に載る**が、**索引がプレイヤーローカル**なので、現状は「自分の手紙＋seed」しか拾えない。クロスプレイヤー（他人の瓶を拾う）には**共有・列挙可能な索引**が要る。

## 実 Kotoba サーバ検証で判明した事実（2026-06-27〜28）

- 現行ソースの `kotoba-server` をビルドすると **block XRPC を serve**（`block.put` は router 登録済み, `lib.rs:1227`）。
- **`block.put` は operator JWT（`Authorization: Bearer`）認証が必須**（`graph_auth/require_operator_auth`）。`block.get` は無認証。
  - → `sip.store` の `KotobaHttp` に **Bearer トークン対応を追加済み**（`(kotoba-http base token)` / `$KOTOBA_TOKEN`）。トークンは `kotoba init` のデプロイ identity から CLI が作る operator JWT を渡す。
- ローカル実行は IPFS（cold tier）＋ remote pin（kotobase.net）に依存。IPFS の **gateway は既定で :8080** を取るため kotoba と衝突する（gateway を退避するか kotoba を別ポートに）。

## 決定：共有索引は Kotoba の Datomic XRPC に置く

CAS は列挙できない（store.clj の設計どおり「world が索引・Kotoba が内容」）。共有版では **world（索引）も Kotoba 側の共有 Datomic に置く**。`kotoba-server` は Datomic XRPC を持つ:

```
com.etzhayyim.apps.kotoba.datomic.transact   ; :sip.letter datom を投入（要 operator JWT）
com.etzhayyim.apps.kotoba.datomic.q          ; season で列挙（read; CACAO read-cap の要否は要確認）
com.etzhayyim.apps.kotoba.datomic.pull
```

- **post-letter!**: `block.put`（本体→CID）→ `datomic.transact` で `{:sip.letter/cid :from :text :season}` を共有 Datomic に追記。
- **inbox**: `datomic.q` で `season` の手紙を列挙 → 各 `:sip.letter/cid` を `block.get` で hydrate（無認証 read）。

これで **全プレイヤーが同じ索引を見る** → 他人の瓶を拾える。本体・索引とも単一の Kotoba サーバに集約。

## 段階導入（フォールバック温存）

1. **現状（done）**: ローカル索引（datalevin / localStorage）＋ CAS。サーバ無しでも動く。
2. **本 PR（done）**: `KotobaHttp` の operator-JWT 認証（block.put を実サーバへ通せる）。
3. **次（todo）**: `:index-backend` を切替可能に。`SIP_KOTOBA_URL`(+token) 設定時は **kotoba Datomic XRPC を索引に使う**共有モード、未設定時はローカル索引。
   - JVM: `sip.store` に `kotoba-index`（datomic.q/transact XRPC over `java.net.http`）を追加。
   - web: `sip.kotoba` に同 XRPC fetch を追加（read=`datomic.q`、write は token 必須なので web からの投函はサーバ経由 or read-only 購読に倒す検討）。
4. **デプロイ**: オンライン IPFS 付きの kotoba をデプロイし `SIP_KOTOBA_URL` を向ける。read 系の CACAO read-cap が要るかは `datomic.q` の auth を確認して詰める。

## 未決（要設計判断）

- **web からの投函**: `datomic.transact` は operator JWT 必須。ブラウザに operator 秘密鍵は置けないので、(a) web は read-only（拾うだけ）＋投函は authoring/server 経由、(b) プレイヤー個別 DID の CACAO write-cap を発行、のどちらか。**(a) を既定推奨**（非毒性・モデレーション容易）。
- **季節ローテ / モデレーション / 容量**: 共有 Datomic に貯まる手紙の GC・通報導線（設計の「非毒性」を運用で担保）。
