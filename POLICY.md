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
