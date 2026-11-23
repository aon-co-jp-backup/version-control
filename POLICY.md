# POLICY

## 自動反復開発の方針 / Auto-loop policy (2026-11-07)

### 日本語
- 既定は無効。有効化は、ビルドしてよい手元の機械の `config.toml` だけ。本番 VPS では有効にしない。
- 作業は origin の新規 clone のみ。手元の作業ツリー・他リポジトリには触れない。
- 完了条件は「テスト成功 かつ `LOOP_DONE`」。満たさない結果はコミットしない。
- push は `--push` と `allow_push = true` の両方が必要。秘密(トークン・鍵)は出力・コミットしない。
- 依存の書き換え(`--fix`)と同様、結果は人が確認する。完成度の保証はしない。

### English
- Disabled by default; enable it only in `config.toml` on a local machine that may build — never on the production VPS.
- Work happens only in a fresh clone of origin; the working tree and other repositories are never touched.
- Completion = green tests AND `LOOP_DONE`; anything less is not committed.
- Pushing needs both `--push` and `allow_push = true`. Secrets (tokens, keys) are never printed or committed.
- As with `--fix`, a human reviews the result; no completeness is guaranteed.

## REST 全廃の状況 / REST abolition status (2026-11-24)

**日本語.** version-control 自身は HTTP サーバーを持たず(巡回ツール)、公開する REST はありません。呼び出す側は Connect に揃えました: aruaru-llm は `POST /aruaru_llm.v1.Llm/Generate`、aruaru-search は `POST /aruaru_search.v1.Search/Search`(本文 `{"q","n"}`)。旧 `GET /v1/search` は aruaru-search 側で削除済みだったため、この移行で検索の巡回が再び動く。通知先 `notify_url` は easy-web.tokyo 側の仕様に従う。単体テスト 42 件 pass。実サーバーへの通し確認は未実施(未検証)。

**English.** version-control is a patrol tool and runs no HTTP server, so it exposes no REST API. Its client calls now use Connect: aruaru-llm `POST /aruaru_llm.v1.Llm/Generate`, aruaru-search `POST /aruaru_search.v1.Search/Search` (body `{"q","n"}`). The old `GET /v1/search` had already been removed from aruaru-search, so the search patrol was broken until this change. `notify_url` follows easy-web.tokyo. 42 unit tests pass. A run against live servers has **not** been done (unverified).

