# P3-B — research data collection（設計）

ADR-0022 P3 の収集（書き込み）フェーズ。P3-A（read-only ダッシュボード）の次。実装は
**同意・プライバシー・インフラ判断が前提**なので、本書で設計を確定してから段階導入する。

## データモデル（実装済み: `sip.research`）

旧 Svelte/TS researcher（`apps/researcher/src/lib/api-types.ts`）の形を clj+Datomic へ写す:

| 旧 (TS) | clj attr | 備考 |
|---|---|---|
| Participant | `:research.participant/{id,age-group,gender,ethnicity,income-range,email,public?,created-at}` | `id` unique。**email/medical は PII**（保存はするがアクセス制御、dashboard 非表示） |
| Session | `:research.session/{id,participant(ref),word-count,created-at}` | |
| EmotionVector（10軸 sum + entry-count） | `:research.emotion/{session(ref),word,joy…confusion,entry-count}` | 単語×session 単位 |
| WordStatistics / WordAggregate | （未実装。反応時間/生理系列は次段で `:research.word/*` に追加） | 大きいので段階追加 |

`ingest-session!`（participant upsert + session + emotion 群を transact）と
`participants`/`sessions-for`/`session-emotion-summary` を実装・テスト済み（datalevin、local）。

## 書き込み先と認証（要決定 → 推奨）

- **shared store = Kotoba の Datomic XRPC**: ingest = `datomic.transact`（**operator JWT 必須**、`sip.store` の Bearer 対応済み）/ read = `datomic.q`。
- **誰が書けるか**:
  - **(推奨) サーバ/authoring 経由のみ**: 研究端末→（認証済み）収集サービス→operator JWT で `datomic.transact`。ブラウザに operator 秘密鍵を置かない。
  - 代替: 参加者個別 DID の **CACAO write-cap**（`kotoba cacao-sign`）。粒度は細かいが鍵配布が重い。
- **as-of / 来歴**: 研究データ継続性が最重要 → datalevin では time-travel 不可。**Datomic Cloud/Peer へ昇格**（ADR-0022 のクリティカルパス）。

## 同意・プライバシー（ブロッカー）

- **収集前に同意フロー必須**（IRB 相当 / オプトイン / 撤回）。`isPublic` と PII 分離は実装済みだが、**実データ投入は同意取得後**。
- dashboard は `isPublic` のみ・PII 非表示（実装済み）。集計の最小化・k-匿名性は次段で。

## Hume 解析の取り込み

- 旧フローは Hume AI で音声→感情 → EmotionVector。clj 側は **ingest 入口を contract 化**（`{:emotions [...]}`）済みなので、Hume 呼び出しは収集サービス側で行い結果を `ingest-session!` に渡す（API キーはサーバ env、リポに置かない）。

## 既存データ移行

- 旧 researcher のストア（D1/XRPC）→ `:research.*` datom へ ETL。`ingest-session!` を移行スクリプトから流用。**移行は Datomic Cloud 確定後**（来歴を壊さないため）。

## 段階導入

1. **本 PR（done）**: データモデル + ingest/query + テスト（local datalevin）。
2. shared 書き込み: `datomic.transact` XRPC ingest（operator JWT）を `sip.research` に追加（実 kotoba デプロイ前提）。
3. 収集サービス + 同意フロー（サーバ側、Hume 連携）。
4. Datomic Cloud 昇格 + 既存データ移行。
5. dashboard を shared read（`datomic.q`）へ接続（P3-A の seam を有効化）。

## 未決（要オーナー判断）

- Datomic Cloud のコスト/運用主体、IRB/同意の所管、Hume の継続利用、移行の停止許容時間。
