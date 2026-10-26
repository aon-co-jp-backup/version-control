# PORTING.md — version-control

## 現在地(2026-10-26)
- Rust 製の単一バイナリ。単体テスト 6 件成功。デモ画面(`/demo/`)は実ブラウザで表示確認済み。
- 未確認: 実際のメール送信・LINE push(宛先・トークン未設定)、VPS 配置。

## 通知の設定(ユーザー作業が必要)
1. メール: `OPEN_EASYWEB_SMTP_HOST/PORT/USERNAME/PASSWORD/FROM`(open-easy-web と同じ)を環境変数に。宛先は `config.toml` の `notify.email_to`。
2. LINE: LINE Developers で Messaging API のチャネルを作り、**長期チャネルアクセストークン**を `F:\line-channel-token.txt` に、通知を受ける自分の **ユーザー ID(U で始まる)** を `F:\line-user-id.txt` に保存(各 1 行)。そのチャネルの公式アカウントを友だち追加しておく。
3. `version-control --test-notify` で両方 OK になることを確認。

## VPS 配置(easy-web.tokyo/version-control)
他の静的ページ(例: `360-speaker-web.service`)と同じ方式。
1. `/root/easy-web.tokyo/version-control` に clone、`cargo build --release`。
2. `config.toml`(VPS 用: `root = "/root/repository"`、`site_dir = "/root/easy-web.tokyo/version-control/site"`)を置く。
3. `version-control-web.service`: `python3 -m http.server <空きポート> --bind 127.0.0.1`(`WorkingDirectory=…/version-control/site`)。
4. `open-web-server/domains.toml` に `path_prefix = "/version-control"` のエントリを追加し反映(**本番設定の変更。事前にバックアップ**)。
5. `version-control.timer`(1 日数回)で `version-control` を実行。
6. `site/demo/` は `version-control --demo` で生成(サンプルデータ)。

## 次の手
- 全リポジトリの初回走査の結果を確認し、`private` の一覧を整える。
- 自動保守側(`aruaru-llm` 等)の `health_url` を実在のエンドポイントに設定する。
