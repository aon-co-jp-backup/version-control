# version-control

**日本語 → English**

`aon-co-jp` の全リポジトリが、**最新のライブラリ・API・リポジトリ**を使っているかを見張る仕組みです。
`open-cuda` / `aruaru-llm` / `open-cpu` / `open-directx` 側の自動保守で足りないものは、**Claude Code に貼り付けるメッセージ**にして、**メールと LINE** で知らせます。

- 運用画面: https://easy-web.tokyo/version-control/
- デモ(サンプルデータ): https://easy-web.tokyo/version-control/demo/
- ライセンス: MIT

## 何を見るか

| 対象 | 方法 |
| --- | --- |
| Rust(`Cargo.toml`) | crates.io の最新安定版と比較(`[dependencies]`・dev/build・`[workspace.dependencies]`・`target.*`) |
| JavaScript(`package.json`) | npm の最新版と比較 |
| PHP(`composer.json`) | packagist の最新安定版と比較 |
| クローン本体 | `git fetch` して origin より遅れている/履歴が分岐していないか |

- `path`・`git` 依存、`workspace:`、`*` などレジストリで比較できないものは対象外です。
- `^1.2`・`1.2` のような範囲指定は、互換のある新版を既に許しているので**メジャー更新(0.x はマイナー更新)だけ**を「遅れ」とみなします。`~` はマイナー、`=` は完全一致で判定します。
- 外部から取得した JSON は `rust-json` の `parse_secure`(サイズ・深さ・重複キーの上限つき)で読みます。

## 誰が対応するか(振り分け)

1. **メジャー更新・クローンの遅れ/分岐・マニフェストの読み取り失敗** → 常に **Claude へ**。
2. **マイナー/パッチ更新(`~`・`=` 指定のみ)** → 自動保守(`aruaru-llm` / `open-cuda` / `open-cpu` / `open-directx`)のどれかが**稼働していれば**そちらに任せ、**どれも応答しなければ Claude へ**。

稼働の判定は、設定の `health_url` への GET です(空欄は「未稼働」扱い)。**version-control 自身は依存を書き換えず、`git pull` もしません**(通知するだけ)。

## 通知

同じ内容は 1 度しか知らせません(リポジトリごとのメッセージの指紋を `state.json` に保存)。内容が変わると再通知します。

- **メール**: `open-easy-web` と同じ環境変数 `OPEN_EASYWEB_SMTP_HOST/PORT/USERNAME/PASSWORD/FROM`。宛先は `notify.email_to`。
- **LINE**: LINE Notify は 2025-03-31 に終了したため、**LINE Messaging API の push**(チャネルアクセストークン + 自分のユーザー ID)を使います。トークンとユーザー ID は txt ファイルのパスで渡し、値は表示・記録しません。

メッセージはそのまま Claude Code に貼れる形です(対象フォルダ・更新の一覧・手順・「`git pull` しない」「秘密を出さない」の注意つき)。

## 使い方

```bash
cp config.example.toml config.toml        # root・通知先を編集(git 管理外)
cargo build --release
target/release/version-control --dry-run  # 走査して画面データを書き、通知は送らない(内容を表示)
target/release/version-control            # 走査 → 画面データ更新 → 新しい問題だけ通知
target/release/version-control --test-notify   # メール/LINE の疎通試験
target/release/version-control --demo     # 公開デモ用のサンプルデータ(site/demo/)を生成
```

画面は `site/`(`index.html` + `status.json`)に出力されます。**非公開リポジトリ名(`private`)は画面データから除外**されます。crates.io は 1 秒に 1 回までの規約に合わせて待つため、初回の全走査は数分かかります(結果は `registry_cache_hours` の間キャッシュ)。

## 運用

VPS では `easy-web.tokyo/version-control` として静的配信し、systemd タイマーで 1 日数回実行します。手順は [`PORTING.md`](PORTING.md)。

## 確認状況

- メール通知は実働(VPS のタイマー実行で送信、2026-10-28)。**LINE push は未設定**(トークン・ユーザー ID を置いて `--test-notify`)。
- `aruaru-llm` などの稼働判定は「応答するか」だけで、実際の保守能力は見ていません。

## お引越し

別のアカウント・PC・VPS への移し方と**再開用メッセージ**は [`PORTING.md`](PORTING.md)。必要なもの(`config.toml`・`env`・LINE の 2 ファイル)は git 管理外で、値はチャットやコミットに出さない。

---

## English

Watches every `aon-co-jp` repository for outdated libraries and stale clones. What the automatic maintainers (`open-cuda`, `aruaru-llm`, `open-cpu`, `open-directx`) cannot cover is turned into a **paste-ready message for Claude Code** and sent by **e-mail and LINE** (Messaging API push; LINE Notify ended 2025-03-31).

- Live: https://easy-web.tokyo/version-control/ — Demo (sample data): https://easy-web.tokyo/version-control/demo/
- Checks `Cargo.toml` (crates.io), `package.json` (npm), `composer.json` (packagist) and whether each clone is behind/diverged from origin.
- Range requirements (`^1.2`, `1.2`) only count a new major (0.x: a new minor) as a gap; `~` also counts minors, `=` everything.
- Majors, stale/diverged clones and unreadable manifests always go to Claude; minor/patch gaps of `~`/`=` pins go to the maintainers while at least one answers its `health_url`, otherwise to Claude.
- It never rewrites dependencies and never runs `git pull`; an unchanged problem is announced only once. Private repository names are left out of the dashboard data.
- E-mail works (sent by the VPS timer, 2026-10-28). **LINE is not set up yet** (place the token and user id, then `--test-notify`).

### Migration
How to move it to another account / PC / VPS and the **resume message** are in [`PORTING.md`](PORTING.md). What you need (`config.toml`, `env`, the two LINE files) is git-ignored; never put the values in chat or commits.
