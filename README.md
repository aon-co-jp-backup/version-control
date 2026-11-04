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

## 対処法の提示と自動対応(`--fix`)

未対応の脆弱性(RustSec)には、状態表の各行に **具体的な対処法** を添える(`→ 対処: …`)。
* 修正版がある → 「`Cargo.toml` の crate を X 以上にして `cargo update -p crate` → ビルドとテスト」。
* 修正版がない → 登録済みの緩和策(`[[advisory.mitigations]]`。既定は `RUSTSEC-2023-0071` の `rsa` → `aon-co-jp/rsa-rs`)の手順。登録が無ければ「利用箇所と影響を確認、使わないなら外す、代替crateを検討」と正直に書く。
* 緩和策で **差し替え済み**のものは、消さずに `mitigated`(対策済み)として別枠で見せる(経路は誰にも振らない)。注意書き「rsa-rs は実験的・第三者の暗号監査なし」も併記する。

`--fix` は上記を実際に直す。**既定は計画の表示だけ**。
```bash
version-control --dry-run --no-fetch --fix                     # 計画だけ
version-control --fix --apply [--only <repo>] [--push]         # 実行(下の条件つき)
```
* 実行は `config.toml` の `[fix] enabled = true` の機械だけ(**VPS は false のまま**。重いビルドをしない)。`--push` は `[fix] allow_push = true` も要る。
* 作業は GitHub の `origin` を **新規 clone した別フォルダ**(`~/.vc-fix`)で行い、手元の作業ツリーには触れない。
* 流れ: clone → 修正(緩和策の `[patch]` 追加 / `cargo update --precise`) → 対象が registry 由来で残っていないか確認 → `cargo test --no-fail-fast` → 小さな別コミット。**テストが全て通ったときだけ push**。
* 固定(`.deps/…` や `deps.lock`)の写しは、そのリポジトリでは直せない。依存元を直してから `scripts/update-deps.sh <名前>` で固定を更新する、と計画に出す。
* パッチ前から失敗するテスト(環境依存)がある場合は push されない。事前に比較してから手動で。

重い処理: 巡回の検索待ちは並列化済み(aruaru-search への開始間隔は従来どおり 800ms)。ビルドを伴う `--fix --apply` は手元(WSL 等)で。コンパイルや git の待ちは GPU(open-cuda / open-directx)では短縮できないので使っていない。

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

## API 方針: REST 全廃・gRPC / Connect へ (2026-11-04)

### 日本語
- **REST は全廃する。新しい API は REST で作らない。** ブラウザや `curl` から呼ぶ API は **Connect の単純呼び出し**(`POST /<パッケージ>.<Service>/<Method>`、`content-type: application/json`、本文は JSON)、サービス間は **gRPC / gRPC-web**(RPoem の管理API `open_runo.admin.v1.Admin` と同じ流儀)。ストリーミングは、必要になってから。
- **既存の REST は、触るときに順に移す。** 移したら旧 REST の経路は消す。移行が終わるまで残るが、**新規には追加しない**。
- **JSON は RS-JSON に統一**する(読み込みは `from_str_secure` / `parse_secure` でサイズ・深さ・重複キーの上限つき、書き出しは `rust_json::fast`)。ブラウザの WASM に `prost` は入れない(サイズが増えるため)。
- **例外(自分たちの API ではなく、相手の規格が決めているもの)**: ACME、OAuth/OIDC のコールバック、第三者(GitHub など)からの Webhook、ヘルスチェック、静的ファイル配信、OTLP など。これらは規格どおりの HTTP を受ける。
- 参考実装: `RPoem`(`open_runo.admin.v1.Admin`)、`open-easy-web`(`server/src/connect.rs`、`proto/open_easy_web/admin/v1/admin.proto`)。
- ※ このリポジトリのコードが既に方針どおりかどうかは、各リポジトリの現状による。この節は方針の記録であり、コードの変更を意味しない。

### English
- **REST is being abolished. New APIs are not built as REST.** APIs called from browsers or `curl` use **Connect unary calls** (`POST /<package>.<Service>/<Method>`, `content-type: application/json`, JSON body); service-to-service calls use **gRPC / gRPC-web** (the same style as RPoem's admin API `open_runo.admin.v1.Admin`). Streaming only when it is actually needed.
- **Existing REST endpoints are migrated one by one, when they are touched.** The old REST route is removed once migrated. Until migration is finished they remain, but **no new ones are added**.
- **JSON is unified on RS-JSON**: reading with `from_str_secure` / `parse_secure` (size, depth and duplicate-key limits), writing with `rust_json::fast`. `prost` is not put into browser WASM (it would grow the binary).
- **Exceptions (shapes dictated by someone else's standard, not our own API)**: ACME, OAuth/OIDC callbacks, webhooks from third parties (e.g. GitHub), health checks, static file serving, OTLP, and so on. These accept plain HTTP as the standard requires.
- Reference implementations: `RPoem` (`open_runo.admin.v1.Admin`) and `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Note: whether the code in a given repository already follows this policy depends on that repository. This section records the policy; it does not mean the code has been changed.

### Deutsch (KI-Übersetzung, redaktionell überarbeitet; noch nicht von Muttersprachlern geprüft)
- **REST wird vollständig abgeschafft; neue APIs entstehen nicht mehr als REST.** APIs, die aus dem Browser oder per `curl` aufgerufen werden, verwenden **unäre Connect-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body). Zwischen Diensten kommen **gRPC bzw. gRPC-web** zum Einsatz, im selben Stil wie die Admin-API von RPoem (`open_runo.admin.v1.Admin`). Streaming wird erst ergänzt, wenn es tatsächlich gebraucht wird.
- **Bestehende REST-Endpunkte werden schrittweise umgestellt, und zwar immer dann, wenn man ohnehin an ihnen arbeitet.** Sobald ein Endpunkt umgestellt ist, wird die alte REST-Route entfernt. Bis zum Abschluss der Migration bleiben die übrigen bestehen, es werden aber **keine neuen mehr hinzugefügt**.
- **JSON wird einheitlich über RS-JSON verarbeitet**: Gelesen wird mit `from_str_secure` bzw. `parse_secure` (mit Obergrenzen für Größe, Verschachtelungstiefe und doppelte Schlüssel), geschrieben mit `rust_json::fast`. `prost` wird nicht in das Browser-WASM aufgenommen, da es die Datei vergrößern würde.
- **Ausnahmen (Schnittstellen, deren Form ein fremder Standard vorgibt und die nicht zu unserer eigenen API gehören)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Drittanbietern (z. B. GitHub), Health-Checks, die Auslieferung statischer Dateien, OTLP und Ähnliches. Diese nehmen, wie vom jeweiligen Standard verlangt, gewöhnliches HTTP entgegen.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines einzelnen Repositorys diese Richtlinie bereits erfüllt, hängt vom jeweiligen Repository ab. Dieser Abschnitt dokumentiert lediglich die Richtlinie; er besagt nicht, dass der Code schon angepasst wurde.

### Русский (перевод выполнен с помощью ИИ и отредактирован; носителями языка пока не проверен)
- **От REST мы полностью отказываемся; новые API больше не создаются в стиле REST.** API, к которым обращаются из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело запроса — JSON). Для взаимодействия между сервисами применяются **gRPC и gRPC-web** — в том же стиле, что и административный API RPoem (`open_runo.admin.v1.Admin`). Потоковая передача данных добавляется лишь тогда, когда в ней действительно возникает необходимость.
- **Существующие REST-эндпоинты переводятся поэтапно — по мере того, как с ними приходится работать.** После перевода старый REST-маршрут удаляется. Пока миграция не завершена, оставшиеся эндпоинты сохраняются, но **новые не добавляются**.
- **Работа с JSON унифицирована на основе RS-JSON**: чтение — через `from_str_secure` или `parse_secure` (с ограничениями на размер, глубину вложенности и повторяющиеся ключи), запись — через `rust_json::fast`. `prost` в браузерный WASM не включается, поскольку это увеличило бы размер файла.
- **Исключения (интерфейсы, форму которых определяет чужой стандарт, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и тому подобное. Такие интерфейсы принимают обычный HTTP, как того требует соответствующий стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Данный раздел лишь фиксирует политику и не означает, что код уже изменён.
