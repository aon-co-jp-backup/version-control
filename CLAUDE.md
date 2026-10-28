# CLAUDE.md — version-control

**日本語 → English**

## 日本語

### 役割
`aon-co-jp` の全リポジトリ(`F:\` 直下のクローン)が最新のライブラリ・API・リポジトリを使っているかを走査し、
自動保守(`aruaru-llm` / `open-cuda` / `open-cpu` / `open-directx`)で足りないものを、**Claude に貼るメッセージ**にしてメールと LINE で知らせる。
運用: `https://easy-web.tokyo/version-control/`、デモ: `/demo/`(サンプルデータ)。
依頼文の原文(2026-10-26): 「全てのリポジトリで全て最新のAPIやライブラリやリポジトリを呼び出して使用しているか管理して、open-cuda・aruaru-llm・open-cpu・open-directx でメンテナンスしきれない場合は、CLAUDEで実行して欲しいメッセージをe-mailとLINEアプリでお知らせして」。

### 構成
- `src/scan.rs`: マニフェスト探索・解析(Cargo/npm/composer)・版の比較(`classify`)。
- `src/latest.rs`: crates.io / npm / packagist の最新版(ディスクキャッシュ。crates.io は 1.1 秒間隔)。
- `src/report.rs`: `status.json` のモデルと Claude 向けメッセージ(`claude_prompt`)・指紋。
- `src/notify.rs`: SMTP(`OPEN_EASYWEB_SMTP_*`)と LINE Messaging API push。
- `src/main.rs`: 走査・振り分け・通知・`--demo`。`web/index.html`: 静的ダッシュボード(`status.json` を読む)。

### ルール
1. **秘密(SMTP パスワード・LINE トークン/ユーザー ID・宛先)を表示・記録・コミットしない**。txt のパスか環境変数で渡す。`config.toml`・`state.json`・`env` は git 管理外。
2. **依存を自動で書き換えない・`git pull` しない**(通知のみ)。書き換え済みリポジトリで `git pull` すると古い履歴が戻る(2026-10-16 の注意)。
3. 外部 JSON は `rust-json` の `parse_secure` で読む(JSON 方針)。
4. 公開画面(`status.json`)に**非公開リポジトリ名を出さない**(`private`)。メッセージ本文(`claude_prompt`)は画面データに含めない。
5. 版の「遅れ」の定義(`^1.2` はメジャーのみ等)を変えるときは `scan.rs` のテストも更新する。
6. 変更後は `cargo test`、画面を触ったら実ブラウザで `/demo/` を確認してから完了と言う。
7. 日本語を含むファイルを PowerShell 5.1 で書かない(文字化け)。Python(UTF-8)か Edit/Write を使う。

### 落とし穴
- LINE Notify は 2025-03-31 に終了。Messaging API の push は、チャネル(公式アカウント)を友だち追加した本人にだけ送れる。
- `health_url` が空の自動保守は「未稼働」扱い → マイナー/パッチ(`~`・`=`)も Claude 行きになる。
- 初回の全走査は crates.io の 1 秒/回の規約のため数分かかる。
- VPS の `open-web-server` の `domains.toml` は `/root/open-web-server/` と `/root/easy-web.tokyo/open-web-server/` が**同一ファイル(ハードリンク)**。追記は 1 回だけ。ポート 8125 は `open-mirror` が使用中で、本機能は **8126**。
- 前面で受けているのは nginx ではなく `open-web-server`。変更後は `systemctl restart open-web-server`。
- Git Bash は `/mnt/...` を書き換える(WSL の呼び出しは PowerShell)。

### お引越し(別のアカウント・PC・VPS へ)
手順の正本は [`PORTING.md`](PORTING.md)。**再開用メッセージは `PORTING.md` 末尾**にある(新しい Claude セッションにそのまま貼る)。

## English

### Role
Scans every `aon-co-jp` repository (the clones directly under `F:\`) for outdated libraries / APIs / stale clones and, for what the automatic maintainers (`aruaru-llm`, `open-cuda`, `open-cpu`, `open-directx`) cannot cover, sends a **paste-ready message for Claude** by e-mail and LINE.
Live: `https://easy-web.tokyo/version-control/`, demo: `/demo/` (sample data).

### Layout
`src/scan.rs` (manifest discovery/parsing for Cargo/npm/composer, version comparison `classify`), `src/latest.rs` (latest versions from crates.io / npm / packagist with an on-disk cache; crates.io is throttled to one call per 1.1 s), `src/report.rs` (`status.json` model, the Claude message `claude_prompt`, fingerprints), `src/notify.rs` (SMTP via `OPEN_EASYWEB_SMTP_*`, LINE Messaging API push), `src/main.rs` (scan, routing, notification, `--demo`), `web/index.html` (static dashboard reading `status.json`).

### Rules
1. Never print, log or commit secrets (SMTP password, LINE token / user id, recipients). Pass txt paths or environment variables. `config.toml`, `state.json` and `env` are git-ignored.
2. Never rewrite dependencies and never run `git pull` automatically (notify only). A `git pull` in a history-rewritten repository brings the old history back.
3. External JSON is read with `rust-json` `parse_secure`.
4. The public `status.json` must not contain private repository names (`private`), nor the message bodies.
5. When you change what counts as "behind" (e.g. `^1.2` = major only), update the tests in `scan.rs`.
6. After changes run `cargo test`; for UI changes verify `/demo/` in a real browser before saying it is done.
7. Do not write files containing Japanese with PowerShell 5.1 (mojibake); use Python (UTF-8) or Edit/Write.

### Pitfalls
- LINE Notify ended on 2025-03-31. A Messaging API push only reaches a user who has added the channel's official account as a friend.
- A maintainer with an empty `health_url` counts as "not running", so minor/patch gaps of `~`/`=` pins also go to Claude.
- The first full scan takes minutes because of crates.io's one-request-per-second rule.
- On the VPS the two `domains.toml` paths are the **same file (hard link)**: append once. Port 8125 belongs to `open-mirror`; this service uses **8126**.
- The edge server is `open-web-server`, not nginx; restart it after route changes.
- Git Bash rewrites `/mnt/...` paths (use PowerShell for WSL calls).

### Migration (to another account / PC / VPS)
The procedure is in [`PORTING.md`](PORTING.md); the **resume message is at its end** (paste it into a fresh Claude session).
