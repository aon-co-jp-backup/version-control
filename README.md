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

## API 方針の補足: 用語の整理と使い分け (2026-11-04)

### 日本語
**1つに統一するものではなく、別の層の話です。** 「REST の代わり」になるのは Connect・gRPC・GraphQL だけで、WASM は画面を作る技術です。

| 用語 | 何か | REST の代わりか |
|---|---|---|
| REST | URL と HTTP メソッドで操作を表す API の作り方 | 全廃する側 |
| **Connect** | `POST /サービス名/メソッド名` に JSON を送る形式。`curl` でもブラウザの `fetch` でも呼べる | はい(**採用**) |
| **gRPC** | 同じ「サービス/メソッド」の考え方を、バイナリ(protobuf)と HTTP/2 で行う形式。速く、ストリーミングも得意 | はい(**サービス同士に採用**) |
| gRPC-web | gRPC のブラウザ向けの変形版 | はい(必要なら) |
| GraphQL | 欲しい項目を問い合わせる形式 | 一部だけ(**問い合わせ(読み取り)専用**) |
| WASM | Rust をブラウザの中で動かす仕組み | いいえ(画面を作る技術) |

- **ブラウザの画面や `curl` から呼ぶ API は Connect**(POST、JSON を RS-JSON で読み書き)。ブラウザの WASM は素の gRPC を話せず、Connect なら `fetch` のままで、`prost` を足さずに済む(画面のサイズも増えない)。
- **サービス同士の通信(サーバー間、重い処理、ストリーミング)は gRPC。** 多くは Connect で足りるので、ストリーミングが必要になった所だけ gRPC に上げる。
- **GraphQL は、まとめて問い合わせたいときの読み取り専用。** 書き込み(操作)には使わない。
- **WASM は画面を作る技術として、そのまま使う。** 画面から呼ぶ API が Connect になっただけ。
- 外部の規格が形を決めるもの(ACME、OAuth のコールバック、第三者の Webhook、ヘルスチェック、静的ファイル、OTLP)は、規格どおりの HTTP のまま。
- **迷ったら Connect。**

### English
**These are not alternatives to unify; they sit at different layers.** Only Connect, gRPC and GraphQL can replace REST; WASM is a way to build the UI.

| Term | What it is | Replaces REST? |
|---|---|---|
| REST | API style that expresses operations through URLs and HTTP methods | The thing being abolished |
| **Connect** | `POST /Service/Method` with a JSON body; callable from `curl` and from a browser `fetch` | Yes (**adopted**) |
| **gRPC** | The same service/method idea over binary (protobuf) and HTTP/2; fast, good at streaming | Yes (**for service-to-service**) |
| gRPC-web | A browser-friendly variant of gRPC | Yes (if needed) |
| GraphQL | A query language: ask for exactly the fields you want | Partly (**read-only queries**) |
| WASM | Runs Rust inside the browser | No (a UI technology) |

- **APIs called from browser screens or `curl` use Connect** (POST, JSON read and written with RS-JSON). Browser WASM cannot speak plain gRPC; with Connect it stays a `fetch`, and `prost` is not needed (the binary does not grow).
- **Service-to-service calls (server to server, heavy work, streaming) use gRPC.** Connect is enough in most places; move a call to gRPC only where streaming is actually needed.
- **GraphQL is read-only**, for queries that gather several pieces of data. It is not used for writes (operations).
- **WASM stays as the way to build the UI.** The API it calls simply became Connect.
- Interfaces whose shape an outside standard dictates (ACME, OAuth callbacks, third-party webhooks, health checks, static files, OTLP) stay plain HTTP.
- **When in doubt, use Connect.**

### Deutsch (KI-Übersetzung, sprachlich überarbeitet; noch nicht von Muttersprachlern geprüft)
**Es geht nicht darum, eine einzige Technik auszuwählen – die Begriffe liegen auf unterschiedlichen Ebenen.** Als Ersatz für REST kommen nur Connect, gRPC und GraphQL in Frage; WASM dagegen ist eine Technik, mit der die Benutzeroberfläche gebaut wird.

| Begriff | Was ist das? | Ersatz für REST? |
|---|---|---|
| REST | Ein API-Stil, bei dem Operationen über URLs und HTTP-Methoden ausgedrückt werden | wird abgeschafft |
| **Connect** | `POST /Dienst/Methode` mit JSON im Body; aufrufbar mit `curl` ebenso wie mit `fetch` im Browser | ja (**gewählt**) |
| **gRPC** | Dasselbe Prinzip aus Dienst und Methode, aber über binäres protobuf und HTTP/2; schnell und gut für Streaming geeignet | ja (**zwischen Diensten**) |
| gRPC-web | Eine für den Browser angepasste Variante von gRPC | ja (bei Bedarf) |
| GraphQL | Abfragesprache, mit der man genau die benötigten Felder anfordert | teilweise (**nur für lesende Abfragen**) |
| WASM | Führt Rust-Code direkt im Browser aus | nein (Technik für die Oberfläche) |

- **APIs, die von Browseroberflächen oder per `curl` aufgerufen werden, verwenden Connect** (POST; JSON wird mit RS-JSON gelesen und geschrieben). WASM im Browser kann kein reines gRPC sprechen. Mit Connect bleibt es bei einem gewöhnlichen `fetch`, und `prost` ist nicht nötig – die WASM-Datei wird dadurch nicht größer.
- **Für die Kommunikation zwischen Diensten (Server zu Server, rechenintensive Aufgaben, Streaming) wird gRPC eingesetzt.** In den meisten Fällen reicht Connect aus; auf gRPC wird nur dort umgestellt, wo Streaming tatsächlich gebraucht wird.
- **GraphQL dient ausschließlich lesenden Abfragen**, wenn mehrere Informationen gebündelt abgefragt werden sollen. Für Schreibzugriffe (Operationen) wird es nicht verwendet.
- **WASM bleibt die Technik für die Oberfläche.** Geändert hat sich lediglich, dass die aufgerufene API nun auf Connect beruht.
- Schnittstellen, deren Form durch einen externen Standard festgelegt ist (ACME, OAuth-Callbacks, Webhooks von Drittanbietern, Health-Checks, statische Dateien, OTLP), bleiben gewöhnliches HTTP.
- **Im Zweifel: Connect.**

### Русский (перевод выполнен с помощью ИИ и литературно отредактирован; носителями языка пока не проверен)
**Это не варианты, из которых нужно выбрать один, а технологии разных уровней.** Заменить REST могут только Connect, gRPC и GraphQL; WASM же — это способ создания пользовательского интерфейса.

| Термин | Что это | Заменяет REST? |
|---|---|---|
| REST | Стиль построения API, при котором операции выражаются через URL и HTTP-методы | отменяется |
| **Connect** | `POST /Сервис/Метод` с телом в формате JSON; вызывается как из `curl`, так и из браузерного `fetch` | да (**выбран**) |
| **gRPC** | Тот же принцип «сервис — метод», но поверх двоичного формата protobuf и HTTP/2; работает быстро и хорошо подходит для потоковой передачи | да (**для связи между сервисами**) |
| gRPC-web | Адаптированный для браузера вариант gRPC | да (при необходимости) |
| GraphQL | Язык запросов: клиент запрашивает ровно те поля, которые ему нужны | частично (**только запросы на чтение**) |
| WASM | Позволяет выполнять код на Rust прямо в браузере | нет (технология для интерфейса) |

- **API, которые вызываются из браузерных интерфейсов или через `curl`, строятся на Connect** (POST; JSON читается и записывается через RS-JSON). Браузерный WASM не умеет работать с «чистым» gRPC. С Connect достаточно обычного `fetch`, а `prost` не требуется — размер WASM-файла не увеличивается.
- **Для обмена между сервисами (сервер — сервер, ресурсоёмкие задачи, потоковая передача) применяется gRPC.** В большинстве случаев достаточно Connect; на gRPC переходят только там, где потоковая передача действительно необходима.
- **GraphQL используется только для чтения**, когда нужно получить сразу несколько связанных данных. Для записи (операций) он не применяется.
- **WASM по-прежнему остаётся технологией для интерфейса.** Изменилось лишь то, что вызываемый им API теперь построен на Connect.
- Интерфейсы, форма которых задана внешним стандартом (ACME, обратные вызовы OAuth, вебхуки сторонних сервисов, проверки работоспособности, статические файлы, OTLP), остаются обычным HTTP.
- **Если есть сомнения — выбирайте Connect.**

## ハードウェアに応じた自動保守の拡張(2026-11-06)

### 拡張前後の範囲

| 項目 | 拡張前 | 拡張後 |
|---|---|---|
| 勧告(RustSec)の修正版あり | `--fix --apply`: 一時clone→`cargo update --precise`→テスト→(任意で)main へ push | 同じ。加えて `--pr` で新ブランチ+PR(強制pushなし) |
| 修正版なし | 登録済み緩和策(rsa→rsa-rs)のみ | 同じ |
| マイナー/パッチの互換更新 | 何もしない(通知もしない) | `[fix] lock_refresh` に挙げたリポジトリの `Cargo.lock` を `cargo update`(要件の範囲内)→テスト→コミット/PR |
| 実行資源 | 逐次・1件ずつ | `hw` が コア数・空きメモリ・GPU・aruaru-llm を検出し、リポジトリを並列(コア2つ/メモリ3GiBにつき1)で検証 |
| 失敗時 | 結果を表示のみ | **Claude 宛の通知に回す**(既存の通知と同じメール/LINE)。更新前から失敗か、更新が原因かも判定 |
| 変更の要約 | なし | aruaru-llm が答えれば1文の**参考**要約(判定には使わない) |
| メジャー更新 | Claude 宛通知 | 変更なし(自動では上げない) |

### 資源の使い分け(正直な範囲)
* ビルド・テストは CPU/ディスク/メモリが律速。**GPU(open-cuda / open-directx)では速くならないので使わない**(`--hw` で検出内容は表示)。並列数だけを `hw::workers` が決める。
* aruaru-llm: `Runtime`/`Accelerators`(`POST /aruaru_llm.v1.Llm/...`)で稼働と能力を確認し、答えれば変更の要約(`Generate`)を添える。最終判定は常にテスト結果。
* open-cpu / open-cuda / open-directx は、`[[maintainers]]` の `health_url` が答えれば「稼働」と表示するだけ(それらに処理を委ねる口は、各ツールがAPIを公開してから)。

### 使い方
```bash
version-control --hw                                   # 検出結果と割り振りを表示(走査なし)
version-control --dry-run --no-fetch --fix             # 計画のみ(書き込みなし)
version-control --fix --apply [--only repo] [--pr|--push]
```
安全策: 既定は計画のみ / `[fix] enabled=true` の機械だけ / `--push`・`--pr` は `allow_push`・`allow_pr` が無ければ無視されローカルコミットまで / 強制push禁止 / テスト不合格は push しない / 作業は新規cloneで、手元(VPSを含む)の作業ツリーは**触らない**(未コミット変更があっても影響なし) / 秘密は出さない(`gh` は自身の認証を使う)。

### 確認済み・未確認
* WSL(32コア・GT 730)で `--hw`・計画・`--apply`(RS-JSON の lock 更新→テスト)まで実機確認。RS-JSON は更新前から examples のビルドが通らず(`fast` 機能なし)、これを「更新は無関係の失敗」として Claude 宛に回すことを確認。
* **未確認**: 実際の `--pr`(gh での PR 作成)と `--push` の実往復、aruaru-llm が起動している状態での要約(WSLでは未起動)。

## 自動反復開発 `--loop`(Claude Code Desktop 風)/ Automatic repeat development (2026-11-07)

### 日本語
自動保守で足りないものを、メッセージに貼るだけでなく**無人で数回まわして、ある程度の完成度まで進める**機能。
`version-control --loop --only <リポジトリ> [--goal "目的"] [--push]`(目的の既定はそのリポジトリ向けの Claude 用メッセージ)。
- 1周 = 調査 → 確認 → 試作・開発 → 改善 → テスト → デバッグ。担当は `claude -p`(設定 `agent_command`、プロンプトは標準入力)。
- **テストは本ツール自身が実行**(`Cargo.toml`→`cargo test`、`package.json`→`npm test`、pytest。`test_command` で上書き)。失敗の末尾を次の周へ渡す。
- 終了は「テスト成功 かつ 担当の最終行が `LOOP_DONE`」、または `max_cycles`(既定4)。未完了なら**コミットせず**作業フォルダ(`work_dir`)に残す。
- 作業は origin の**新規 clone** だけで行い、手元の作業ツリーは触らない。成功時のみローカルコミット。push は `--push` かつ `[autoloop] allow_push = true` のときだけ。
- 既定は**無効**(`[autoloop] enabled = false`)。本番 VPS では動かさない(ビルドする機械でだけ)。
- 正直な注意: 担当の判断はモデル任せで、完成度の保証ではない。`LOOP_DONE` とテスト成功は必要条件にすぎず、結果は人が見る。単体テストはループ制御(`src/autoloop.rs`)のみで、実際の `claude` 呼び出しの通し実行は未検証。

### English
Beyond pasting a message, this runs **unattended repeat cycles until the work reaches a reasonable level of completion**.
`version-control --loop --only <repo> [--goal "..."] [--push]` (the default goal is the repository's Claude message).
- One cycle = investigate → confirm → prototype/develop → improve → test → debug, performed by `claude -p` (`agent_command`, prompt on stdin).
- **The tool runs the tests itself** (`cargo test` / `npm test` / pytest, override with `test_command`) and feeds the failure tail into the next cycle.
- It stops when tests pass AND the agent's last line is `LOOP_DONE`, or after `max_cycles` (default 4). An unfinished run is **not committed** and stays in `work_dir`.
- Work happens only in a fresh clone of origin; the working tree is untouched. Local commit on success only; push needs `--push` and `[autoloop] allow_push = true`.
- **Off by default** (`[autoloop] enabled = false`); never run it on the production VPS.
- Honest caveat: the agent's judgement is the model's; `LOOP_DONE` plus green tests is necessary, not sufficient — review the result. Only the loop control (`src/autoloop.rs`) is unit-tested; an end-to-end run with the real `claude` has not been verified.


## 無人の自動メンテナンス `--auto` と自己更新 `--self-update` / Unattended maintenance and self-update (2026-11-13)

### 日本語
通知メールに来ていた「依存が古い・脆弱・クローンが遅れている」を、**人が貼り付けなくても直す**ための機能です。**ビルドする機械でだけ**動かし、本番 VPS は従来どおり走査と通知だけです。

`version-control --auto`(= `--fix --apply --pr --sync`)の流れ:

| 対象 | 自動の中身 | 条件 |
| --- | --- | --- |
| クローンが origin より遅れているだけ | 作業ツリーが綺麗なら `git merge --ff-only`(合流・上書きはしない) | `[fix] sync_clones = true`(ビルド不要なので VPS でも可) |
| 脆弱性(RustSec)で修正版あり | `cargo update --precise`。**Cargo.toml の要求が修正版を許さない場合は要求も書き換え**(例: quick-xml 0.38→0.41、time 0.3.41→0.3.47) | `[fix] enabled` |
| 古い要求(crates.io / npm のメジャー・マイナー・パッチ) | 作業コピーで要求を最新に書き換え(演算子と桁は保つ。`0.60`→`0.61`、`^1`→`^3`)→ lock 更新 → **1つずつビルド確認**(`cargo check --all-targets` / `npm run build`)→ 通ったものだけ別コミット → まとめてテスト → ブランチ `vc/upgrade-日付` + PR | `[fix] upgrade = true` と `allow_pr = true` |
| ビルドが通らない更新(破壊的変更) | その依存だけ取り消し、理由つきで報告。`[autoloop] enabled = true` ならエージェント(`claude -p`)が `vc/loop-*` ブランチ + PR で続きを試みる | 任意 |
| 自分自身(version-control) | `--self-update`: 綺麗で遅れているだけなら fast-forward → ビルド → `--version` で起動確認 → 成功したら新しい build で同じ引数を再実行。**失敗したら元のコミットに戻す** | `[selfupdate] enabled = true` |

- **重複させない**: すでに `vc/upgrade-*` の PR が開いていれば新しく作らず「PR待ち」と扱う。PR 済みの依存は「Claude に頼む」メッセージから外れる(通知は「自動対応の結果」と、自動で直せなかったものだけ)。
- **安全策**: 作業は新規 clone のみ(手元の作業ツリーは触らない)/ 強制 push なし / 既定ブランチへは直接 push しない(`--push` 指定と `allow_push` がある場合のみ)/ テスト不合格は push しない(更新前から失敗するテストは「更新は無関係」と明記)/ composer は未対応(報告のみ)/ `upgrade_skip` に書いた依存は触らない / 秘密は表示しない。
- **使い方**: ビルドする機械(手元 PC)の `config.toml` に `[fix] enabled = true / allow_pr = true / upgrade = true / sync_clones = true`、必要なら `[selfupdate] enabled = true` を書き、タスクスケジューラ等で `version-control --auto` を 1日数回。まず `--dry-run --no-fetch --fix` で計画だけ確認できる。部品: `src/bump.rs`(Cargo.toml は `toml_edit` で書式・コメントを保つ、package.json は文字列置換。`version-control --edit-dep FILE NAME LATEST`)、`scripts/upgrade.sh`、`src/selfupdate.rs`。
- **確認済み**: 単体試験(要求の書き換え・自己更新の判定と巻き戻し・出力の解釈)、実 git/実 cargo での通し(`itoa 0.4`→`1`: 成功時は別コミット+テスト合格、コンパイルが通らない更新は取り消して `ITEM … fail`、脆弱性修正で要求を書き換えて lock が修正版以上になる)。
- **未確認(正直に)**: 実 GitHub での `vc/upgrade-*` PR 作成(コードは `remediate.sh` の既存経路と同じ形だが、この変更では未実行)、npm の通し(コードのみ)、エージェント(`claude -p`)での続行の通し、自己更新の本番(VPS)での実行。Windows では `fix::script_tests` の1件(`bash` にパスを渡す既存の試験)が従来から失敗する(WSL/Linux では通る)。

### English
Features that fix, **without anyone pasting messages**, what the notification e-mails reported (outdated dependencies, vulnerabilities, stale clones). Run them **only on a machine that may build**; the production VPS keeps scanning and notifying.

`version-control --auto` (= `--fix --apply --pr --sync`):

| Target | What is automated | Needs |
| --- | --- | --- |
| Clone only behind origin | `git merge --ff-only` when the tree is clean (never a merge or overwrite) | `[fix] sync_clones = true` (no build, so the VPS may do it) |
| Vulnerability with a fixed release | `cargo update --precise`; **when the Cargo.toml requirement does not allow the fix, the requirement is raised too** (e.g. quick-xml 0.38→0.41) | `[fix] enabled` |
| Outdated requirement (crates.io / npm major/minor/patch) | In a work copy raise the requirement (operator and precision kept: `0.60`→`0.61`, `^1`→`^3`) → update the lock → **build-check one by one** (`cargo check --all-targets` / `npm run build`) → one commit per success → test everything → branch `vc/upgrade-DATE` + pull request | `[fix] upgrade = true`, `allow_pr = true` |
| Update that does not build (breaking change) | Only that dependency is dropped and reported with the reason; with `[autoloop] enabled = true` the agent (`claude -p`) continues on a `vc/loop-*` branch + PR | optional |
| This program itself | `--self-update`: fast-forward when clean and only behind → build → smoke test (`--version`) → on success re-run with the same arguments with the new build; **on failure the clone returns to its previous commit** | `[selfupdate] enabled = true` |

- **No duplicates**: an open `vc/upgrade-*` pull request means "waiting"; nothing new is created. Dependencies already in a pull request drop out of the "ask Claude" message (the notice carries "automatic results" and what could not be fixed).
- **Safeguards**: fresh clones only (your working tree is never touched) / no force push / never straight to the default branch unless `--push` and `allow_push` / failing tests are never pushed (a failure that already existed is marked "unrelated") / composer is report-only / `upgrade_skip` names are never touched / secrets are never printed.
- **Usage**: on the build machine put `[fix] enabled = true / allow_pr = true / upgrade = true / sync_clones = true` (and optionally `[selfupdate] enabled = true`) in `config.toml` and run `version-control --auto` a few times a day (Task Scheduler etc.). Preview with `--dry-run --no-fetch --fix`. Parts: `src/bump.rs` (`toml_edit` keeps Cargo.toml layout and comments; package.json by text replacement; `version-control --edit-dep FILE NAME LATEST`), `scripts/upgrade.sh`, `src/selfupdate.rs`.
- **Verified**: unit tests (requirement rewriting, self-update decisions and rollback, output parsing) and end-to-end runs with real git/cargo (`itoa 0.4`→`1`: success = separate commit + passing tests; a non-compiling update is reverted and reported as `ITEM … fail`; a vulnerability fix that needs a raised requirement ends with the lock at or above the fixed version).
- **Not verified (honestly)**: opening a real `vc/upgrade-*` PR on GitHub (same shape as the existing `remediate.sh` path but not run for this change), the npm path end to end (code only), the agent continuation with a real `claude -p`, and self-update on the production VPS. On Windows one existing test (`fix::script_tests`, which hands a Windows path to `bash`) already failed before this change (it passes on WSL/Linux).
