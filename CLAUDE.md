# CLAUDE.md — version-control

## 役割
`aon-co-jp` の全リポジトリ(`F:\` 直下のクローン)が最新のライブラリ・API・リポジトリを使っているかを走査し、
自動保守(`aruaru-llm` / `open-cuda` / `open-cpu` / `open-directx`)で足りないものを、**Claude に貼るメッセージ**にしてメールと LINE で知らせる。
運用: `https://easy-web.tokyo/version-control/`、デモ: `/demo/`(サンプルデータ)。
依頼文の原文: 「全てのリポジトリで全て最新のAPIやライブラリやリポジトリを呼び出して使用しているか管理して、open-cuda・aruaru-llm・open-cpu・open-directx でメンテナンスしきれない場合は、CLAUDEで実行して欲しいメッセージをe-mailとLINEアプリでお知らせして」(2026-10-26)。

## 構成
- `src/scan.rs`: マニフェスト探索・解析(Cargo/npm/composer)・版の比較(`classify`)。
- `src/latest.rs`: crates.io / npm / packagist の最新版(ディスクキャッシュ。crates.io は 1.1 秒間隔)。
- `src/report.rs`: `status.json` のモデルと Claude 向けメッセージ(`claude_prompt`)・指紋。
- `src/notify.rs`: SMTP(`OPEN_EASYWEB_SMTP_*`)と LINE Messaging API push。
- `src/main.rs`: 走査・振り分け・通知・`--demo`。`web/index.html`: 静的ダッシュボード(`status.json` を読む)。

## ルール
1. **秘密(SMTP パスワード・LINE トークン/ユーザー ID・宛先)を表示・記録・コミットしない**。txt のパスか環境変数で渡す。`config.toml`・`state.json` は git 管理外。
2. **依存を自動で書き換えない・`git pull` しない**(通知のみ)。書き換え済みリポジトリで `git pull` すると古い履歴が戻る(2026-10-16 の注意)。
3. 外部 JSON は `rust-json` の `parse_secure` で読む(JSON 方針)。
4. 公開画面(`status.json`)に**非公開リポジトリ名を出さない**(`private`)。メッセージ本文(`claude_prompt`)は画面データに含めない。
5. 版の「遅れ」の定義(`^1.2` はメジャーのみ等)を変えるときは `scan.rs` のテストも更新する。
6. 変更後は `cargo test`、画面を触ったら実ブラウザで `/demo/` を確認してから完了と言う。

## 落とし穴
- LINE Notify は 2025-03-31 に終了。Messaging API の push はチャネル(公式アカウント)を友だち追加した本人にだけ送れる。
- `health_url` が空の自動保守は「未稼働」扱い → マイナー/パッチ(`~`・`=`)も Claude 行きになる。
- 初回の全走査は crates.io の 1 秒/回の規約のため数分〜十数分かかる。
- Git Bash は `/mnt/...` を書き換える(WSL の呼び出しは PowerShell)。
