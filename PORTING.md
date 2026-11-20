# PORTING.md — version-control

**日本語 → English**

## 日本語

### 現在地(2026-10-28)
- Rust 製の単一バイナリ。単体テスト 6 件成功。デモ画面(`/demo/`)は実ブラウザで表示確認済み。
- VPS 配置済み: `/root/easy-web.tokyo/version-control`、`version-control-web.service`(127.0.0.1:**8126**)、`open-web-server/domains.toml` に `/version-control` を追加、`version-control.timer`(00/06/12/18 時の :20)。`domains.toml` のバックアップは `domains.toml.bak-versioncontrol`。
- 初回走査(VPS、40 リポジトリ・927 依存): メジャー更新 326・クローン遅れ 12・分岐 4・マイナー 3・パッチ 6。2026-10-28 時点で 41 リポジトリを走査。
- **メール通知は実働**: VPS のタイマー実行で `e-mail: sent` を確認(2026-10-28 12:27、宛先 norukia.jp@gmail.com)。`env` は open-easy-web と同じ SMTP 設定の複製(git 管理外)。
- **未了: LINE push**(トークン・ユーザー ID 未設定のため `LINE: FAILED (notify.line_token_file is not set)`。メールが届けば状態は保存されるので再送の山にはならない)。
- 自動保守側の稼働判定: `aruaru-llm` だけ VPS で応答(`http://127.0.0.1:4600/v1/accelerators`)。open-cuda / open-cpu / open-directx は `health_url` が空 = 未稼働扱い。

- **API 例外の台帳と巡回(2026-11-04 追加)**: `exceptions.toml`(7 項目)/ `API-EXCEPTIONS.md`(日英独露)。同じタイマーの実行に同乗し、aruaru-search で兆候を検知・REST 風ルート数を記録・変化を open-easy-web のお知らせメール(`/open_easy_web.admin.v1.Notify/Send`)で通知。画面は「API 例外」欄と `api-exceptions.json`。VPS では `scripts/load-admin-token.sh`(`ExecStartPre`)が `env.api` を作る(値は表示しない)。単体: `version-control --api-only --dry-run`。aruaru-llm は小型モデルで、VPS が忙しいと 45 秒でタイムアウトする(記録して続行)。

- **open-cpu / open-directx / open-cuda を使ったか(2026-11-04 実測)**: 使っていない。API 例外の巡回(手元 PC、release)の実測は、検索の待ち 94.4 秒(ネットワーク待ち・検索語間の待機)、ルート数え 初回 67.9 秒(ディスクが冷えている時)・2 回目 3.7 秒。重いのは I/O と待ちで、SIMD/GPU で速くなる計算(ハッシュ・類似検索など)は巡回に無い。実測で速くなる所が無いので、依存(`../open-cpu` など)は足していない。重い計算(大量文章の類似検索など)を足すときに、再度実測してから検討する。

- **走査の対象は upstream(origin)の最新(2026-11-05〜)**: `git fetch` のあと `git ls-tree` / `git show @{u}:…` で Cargo.toml・Cargo.lock・package.json・composer.json を一時フォルダへ出して走査する(読み取りのみ・`git pull` なし)。クローンの遅れ(behind/ahead)は別項目として従来どおり出す。upstream が無い・fetch に失敗したリポジトリは、クローンの中身を読み、実行ログに `note: <名前> reads the clone as it is` と出る(VPS で 6 件: RUNO・atr-sonic・audiocafe-tokyo・open-github-backup・ows-base・ows-stream。非公開リポジトリの認証が VPS に無いのが主因と思われる。VPS に組織全体のトークンは置かない)。

### 通知の設定
1. メール: `OPEN_EASYWEB_SMTP_HOST/PORT/USERNAME/PASSWORD/FROM`(open-easy-web と同じ)。宛先は `config.toml` の `notify.email_to`。
2. LINE: LINE Developers で Messaging API のチャネルを作り、**長期チャネルアクセストークン**を `F:\line-channel-token.txt`、自分の**ユーザー ID(U で始まる)**を `F:\line-user-id.txt` に保存(各 1 行)。そのチャネルの公式アカウントを友だち追加。`config.toml` の `notify.line_token_file` / `notify.line_to_file` にパスを書く。VPS へは `scp` でファイルを送り、パスを VPS の `config.toml` に書く(値は表示しない)。
3. `version-control --test-notify` で両方 OK を確認。

### お引越し手順(別のアカウント・PC・VPS へ)
**必要なもの(リポジトリには入っていない)**: `config.toml`、`env`(SMTP)、LINE の 2 ファイル。いずれも git 管理外。**値はチャットやコミットに出さない**。
1. **ソース**: `git clone https://github.com/aon-co-jp/version-control`。依存の `rust-json` は兄弟フォルダ `../RS-JSON`(`https://github.com/aon-co-jp/RS-JSON`)が必要。
2. **ビルド**: `cargo build --release`(PowerShell では `%USERPROFILE%\.cargo\bin` を PATH の前に)。`cargo test` で 6 件成功を確認。
3. **設定**: `config.example.toml` を `config.toml` にコピーして `root`(クローンの置き場)・`private`・`notify.*` を編集。
4. **試験**: `version-control --dry-run`(走査して画面データを書くだけ、送信なし)→ `--test-notify`(メール・LINE の疎通)。
5. **公開(VPS・easy-web.tokyo/version-control)**
   1. `/root/easy-web.tokyo/version-control` に clone してビルド、`config.toml`(`root = "/root/repository"`、`site_dir = "/root/easy-web.tokyo/version-control/site"`)と `env`(`OPEN_EASYWEB_SMTP_*` を `KEY=VALUE` で 1 行ずつ、権限 600)を置く。
   2. `version-control-web.service`(`python3 -m http.server 8126 --bind 127.0.0.1`、`WorkingDirectory=…/site`)、`version-control.service`(oneshot、`EnvironmentFile=-…/env`)、`version-control.timer`(`OnCalendar=*-*-* 00,06,12,18:20:00`、`Persistent=true`)を作り `systemctl enable --now`。
   3. `open-web-server/domains.toml` に `path_prefix = "/version-control"`・`backend_addr = "127.0.0.1:8126"`・`strip_prefix = true` のエントリを**1 回だけ**追加(**事前にバックアップ**。2 つのパスは同一ファイル)し `systemctl restart open-web-server`。ポートは `ss -ltn` で空きを確認(8125 は `open-mirror`)。
   4. `version-control --demo` で `site/demo/` を生成。`/version-control/`・`/demo/`・`/demo/status.json` が 200 であることを `curl` で確認。
6. **別の宛先へ移す**: `notify.email_to` と LINE の 2 ファイルを差し替えるだけ。状態(重複通知の抑止)は `state.json` に入る(消えると 1 回だけ全件を再通知する)。
7. **古い場所の撤去**: 旧 VPS では `systemctl disable --now version-control.timer version-control-web.service`、`domains.toml` の該当エントリを削除。

### 次の手
- LINE のトークンとユーザー ID を設定し、`--test-notify` で LINE を確認。
- 初回の大量の指摘(メジャー更新 326 件)を、リポジトリごとに Claude で消化する(メールのメッセージを貼る)。
- open-cuda / open-cpu / open-directx の `health_url`(稼働する保守サービスがあれば)を設定。
- `private` の一覧の見直し(非公開リポジトリ名を画面に出さないため)。

### 再開用メッセージ(新しい Claude セッションにそのまま貼る)
```
version-control(https://github.com/aon-co-jp/version-control)の続きをお願いします。
まず F:\version-control の CLAUDE.md と PORTING.md(「現在地」「お引越し手順」)を読んでください。
これは aon-co-jp の全リポジトリの依存(crates.io/npm/packagist)とクローンの鮮度を走査し、
自動保守(aruaru-llm/open-cuda/open-cpu/open-directx)で足りないものを、Claude向けメッセージにして
メールとLINEで知らせる仕組みです。運用は https://easy-web.tokyo/version-control/ 、デモは /demo/ 。
VPS: /root/easy-web.tokyo/version-control(ssh conoha)、ポート8126、タイマーは00/06/12/18時の:20。
現在地: メール通知は実働(2026-10-28確認)。LINEは未設定(トークン F:\line-channel-token.txt と
ユーザーID F:\line-user-id.txt を置いてもらい、VPSへscpして config.toml の line_* に書き、
version-control --test-notify で確認)。秘密(トークン・パスワード・宛先)は表示・コミットしない。
注意: 依存の書き換えや git pull は自動でしない / domains.toml の2つのパスは同一ファイル(追記は1回)/
8125は open-mirror が使用中 / 日本語ファイルはPowerShell 5.1で書かない。
次にやること: (1) LINEの設定と試験 (2) 初回のメジャー更新326件をリポジトリごとにClaudeで消化
(3) health_url の設定 (4) private の見直し。
```

## English

### Current state (2026-10-28)
- A single Rust binary; 6 unit tests pass; the demo page (`/demo/`) was verified in a real browser.
- Deployed on the VPS: `/root/easy-web.tokyo/version-control`, `version-control-web.service` (127.0.0.1:**8126**), a `/version-control` route in `open-web-server/domains.toml`, `version-control.timer` (00/06/12/18 h at :20). Backup of `domains.toml`: `domains.toml.bak-versioncontrol`.
- First scan (VPS, 40 repositories, 927 dependencies): 326 major gaps, 12 stale clones, 4 diverged, 3 minor, 6 patch. 41 repositories scanned as of 2026-10-28.
- **E-mail works**: the VPS timer run logged `e-mail: sent` (2026-10-28 12:27, to norukia.jp@gmail.com). `env` is a copy of open-easy-web's SMTP settings (git-ignored).
- **Open: LINE push** (no token / user id yet: `LINE: FAILED (notify.line_token_file is not set)`; state is saved once the e-mail goes out, so nothing is re-sent in a loop).
- Maintainer liveness: only `aruaru-llm` answers on the VPS (`http://127.0.0.1:4600/v1/accelerators`); open-cuda / open-cpu / open-directx have an empty `health_url`, i.e. count as not running.

- **API exception ledger and patrol (added 2026-11-04)**: `exceptions.toml` (7 entries) / `API-EXCEPTIONS.md` (ja/en/de/ru). It rides on the same timer run: aruaru-search signals, REST-like route counts, and changes announced through open-easy-web's notice mail (`/open_easy_web.admin.v1.Notify/Send`). The dashboard gets an "API exceptions" section and `api-exceptions.json`. On the VPS `scripts/load-admin-token.sh` (`ExecStartPre`) writes `env.api` (the value is never printed). Stand-alone: `version-control --api-only --dry-run`. aruaru-llm is a small model and times out after 45 s when the VPS is busy (recorded, run continues).

- **Were open-cpu / open-directx / open-cuda used? (measured 2026-11-04)**: No. Patrol timings (local PC, release): search wait 94.4 s (network wait and pauses between queries); route counting 67.9 s cold (cold disk cache) and 3.7 s warm. The cost is I/O and waiting; the patrol has no computation (hashing, similarity search) that SIMD/GPU would speed up, so no dependency (`../open-cpu` etc.) was added. Re-measure before considering them if heavy computation is added.

### Notification setup
1. E-mail: `OPEN_EASYWEB_SMTP_HOST/PORT/USERNAME/PASSWORD/FROM` (same as open-easy-web); recipients in `notify.email_to`.
2. LINE: create a Messaging API channel in LINE Developers, save the **long-lived channel access token** to `F:\line-channel-token.txt` and your **user id (starts with U)** to `F:\line-user-id.txt` (one line each), add the channel's official account as a friend, and put the paths in `notify.line_token_file` / `notify.line_to_file`. For the VPS, `scp` the files and write their paths into the VPS `config.toml` (never print the values).
3. Confirm both with `version-control --test-notify`.

### Migration procedure (to another account / PC / VPS)
**Needed but not in the repository**: `config.toml`, `env` (SMTP), the two LINE files — all git-ignored. **Never put the values in chat or commits.**
1. **Source**: `git clone https://github.com/aon-co-jp/version-control`; it depends on the sibling folder `../RS-JSON` (`https://github.com/aon-co-jp/RS-JSON`).
2. **Build**: `cargo build --release` (on PowerShell put `%USERPROFILE%\.cargo\bin` first on PATH); `cargo test` should pass 6 tests.
3. **Configure**: copy `config.example.toml` to `config.toml` and edit `root` (where the clones live), `private`, `notify.*`.
4. **Test**: `version-control --dry-run` (scan and write the dashboard data, send nothing), then `--test-notify` (e-mail and LINE).
5. **Publish (VPS, easy-web.tokyo/version-control)**
   1. Clone to `/root/easy-web.tokyo/version-control`, build, and place `config.toml` (`root = "/root/repository"`, `site_dir = "/root/easy-web.tokyo/version-control/site"`) and `env` (`OPEN_EASYWEB_SMTP_*` as `KEY=VALUE` lines, mode 600).
   2. Create `version-control-web.service` (`python3 -m http.server 8126 --bind 127.0.0.1`, `WorkingDirectory=…/site`), `version-control.service` (oneshot, `EnvironmentFile=-…/env`) and `version-control.timer` (`OnCalendar=*-*-* 00,06,12,18:20:00`, `Persistent=true`), then `systemctl enable --now`.
   3. Add **once** an entry with `path_prefix = "/version-control"`, `backend_addr = "127.0.0.1:8126"`, `strip_prefix = true` to `open-web-server/domains.toml` (**back it up first**; the two paths are one file), then `systemctl restart open-web-server`. Check a free port with `ss -ltn` (8125 is `open-mirror`).
   4. Generate `site/demo/` with `version-control --demo`; confirm with `curl` that `/version-control/`, `/demo/` and `/demo/status.json` return 200.
6. **Change recipients**: just replace `notify.email_to` and the two LINE files. `state.json` holds the de-duplication state (if lost, everything is announced once more).
7. **Retire the old place**: on the old VPS run `systemctl disable --now version-control.timer version-control-web.service` and remove the route from `domains.toml`.

### Next steps
- Set the LINE token and user id, then confirm LINE with `--test-notify`.
- Work through the first large batch of findings (326 major gaps) repository by repository with Claude (paste the message from the e-mail).
- Set `health_url` for open-cuda / open-cpu / open-directx if a maintaining service exists.
- Review `private` (so private repository names stay off the dashboard).

### Resume message (paste into a fresh Claude session)
```
Please continue version-control (https://github.com/aon-co-jp/version-control).
First read CLAUDE.md and PORTING.md ("Current state", "Migration procedure") in F:\version-control.
It scans the dependencies (crates.io/npm/packagist) and clone freshness of every aon-co-jp repository and,
for what the automatic maintainers (aruaru-llm/open-cuda/open-cpu/open-directx) cannot cover, sends a
Claude-ready message by e-mail and LINE. Live: https://easy-web.tokyo/version-control/ , demo: /demo/.
VPS: /root/easy-web.tokyo/version-control (ssh conoha), port 8126, timer at 00/06/12/18 h :20.
State: e-mail works (confirmed 2026-10-28). LINE is not set up (get F:\line-channel-token.txt and
F:\line-user-id.txt placed, scp them to the VPS, set line_* in config.toml, confirm with
version-control --test-notify). Never print or commit secrets (tokens, passwords, recipients).
Cautions: never rewrite dependencies or git pull automatically / the two domains.toml paths are one file
(append once) / port 8125 is used by open-mirror / do not write Japanese files with PowerShell 5.1.
Next: (1) LINE setup and test (2) work through the first 326 major gaps per repository with Claude
(3) set health_url (4) review private.
```

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
## aruaru-llm 呼び出しの Connect 化 (2026-11-05)

- aruaru-llm の REST 全廃に伴い、本リポジトリの呼び出し先を `POST /aruaru_llm.v1.Llm/<Method>`(JSON本文)へ変更した(旧 `/v1/...` は aruaru-llm 側で削除済み)。
- 対象は aruaru-llm を呼ぶ箇所のみ。本リポジトリ自身のAPI・他サービス(aruaru-search 等)の呼び出しは別途。
- 未検証: ビルド/実サーバー往復(aruaru-llm の `connect-migration` ブランチが VPS 未反映のため)。aruaru-llm の main マージ・VPS反映と同時に反映すること。

## JSON の読み込みを `parse_secure_sonic` に統一(2026-11-06)
方針(`F:\runo\CLAUDE.md`「JSON処理の使用方針」)どおり、JSON を読む4か所を RS-JSON の `parse_secure_sonic`(SIMD版・サイズ/深さ/重複キーの上限つき、値は `sonic_rs::Value`)に切り替えた: 最新版の問い合わせ(`latest.rs`: crates.io/npm/packagist)、`package.json`/`composer.json` の走査(`scan.rs`)、aruaru-search と aruaru-llm の応答(`apiex.rs`)。状態ファイル `state.json`(自分で書いた信頼できるローカルファイル)は型付きで `sonic_rs::from_str`。書き出しは従来どおり。
- **実測(前後)**: 約409 KB(依存2万件)の `package.json` の読み取り+抽出(`scan::parse_json`)が **約30 ms → 約8 ms(約3.5倍)**。`cargo test --release bench_parse_json -- --ignored --nocapture` で再現。
- 試験: `cargo test` 全通過(14件)。

**VPS 反映(2026-11-06)**: WSL でリリースビルド → 最大 GLIBC_2.39 を確認 → scp → `target/release/version-control.bak-20261106` へバックアップ → 入れ替え → `--dry-run --no-api` で走査が動くこと(通知なし)を確認。タイマー(00/06/12/18時の:20)はそのまま。戻すには `bak-20261106` を `version-control` へ戻す。

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

**VPS 反映(2026-11-06 夕)**: リリースビルド(最大 GLIBC_2.39)→scp→`version-control.bak-20261106b` と `config.toml.bak-20261106b` に退避→入れ替え→`--hw`・`--dry-run` で確認。VPS の `config.toml` の aruaru-llm `health_url` を Connect の `.../aruaru_llm.v1.Llm/Accelerators`(POST)に直した(旧 `/v1/accelerators` は REST 廃止後 404 で、常に DOWN と誤判定→全件を Claude 宛にしていた)。VPS 上でタイマー実行(12:20)は 11-05 の Connect 化後も成功、`Generate`/`Accelerators` も応答を確認。戻すには両 `bak-20261106b` を戻す。注意: DOWN 誤判定が直ったため、次回の通知は件数が減り内容が変わる(再送が1回出る)。

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


---

## 追記 2026-11-08(日本語)
台帳に **GraphiQL/docs 画面・SCIM(RFC 7644)・GitHub Webhook・git smart HTTP・ACME モック(テスト用)** を追加(根拠の規格URLは `exceptions.toml`)。集計の数え方も改修: (1) `#[cfg(test)]`・`tests/`・`examples/`・`clients/`・`mock*` を数えない、(2) 同じ origin のクローン(ows-base・`.broken-*`・別ブランチ・別名dir)は名寄せして1回だけ数える(`merged_aliases`)、(3) origin 基準を維持し、クローンの遅れ(`clone_lag`)は別項目で出す。

## Addendum 2026-11-08 (English)
Added GraphiQL/docs UI, SCIM, GitHub webhook, git smart HTTP and the ACME test mock to the ledger. The route counter now skips test/example/client/mock code, merges clones sharing an origin (`merged_aliases`), keeps reading origin, and reports clone lag separately (`clone_lag`).

**VPS 反映(2026-11-08)**: WSL でリリースビルド(最大 GLIBC_2.39、VPS は 2.39)→scp→`target/release/version-control.bak-20261108` と `state.json.bak-20261108` に退避→入れ替え→`--api-only --dry-run --no-fetch` で確認(通知なし・サービス/タイマーは active のまま)。集計(REST風ルート): 反映前の最終記録 251本(総ルート398)→反映後 **179本**(総ルート318)。名寄せ: atr-base・atr-sonic→audiocafe-tokyo-rust、ows-base・ows-stream→open-web-server。クローンの遅れ30件は別項目。戻すには `bak-20261108` を戻す。注意: 次回のタイマー実行で「前日比」が大きく減って見える(実際の移行ではなく数え方の変更)。


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
- **確認済み(2026-11-13)**: 単体試験(要求の書き換え・自己更新の判定と巻き戻し・出力の解釈。Windows でも全39件通る)/ 実 git・実 cargo・実 npm での通し(`itoa 0.4`→`1`、`is-odd ^2`→`^3`: 別コミット+テスト合格、コンパイルが通らない更新は取り消して `ITEM … fail`、脆弱性修正で要求を書き換えて lock が修正版以上に)/ **実 GitHub で PR 作成**(`aon-co-jp/open-fudousan` の PR 1: `thiserror 1→2`、`vc/upgrade-日付`、1コミット、テスト合格)と、**開いた PR がある間は新しく作らない**(再実行で `PR待ち`)/ **自己更新の実バイナリ試験**(古い clone を fast-forward → 実ビルド → `--version` → 新しい build で再実行。ビルド失敗時は元のコミットに戻ることも実機で確認)。
- **未確認(正直に)**: エージェント(`claude -p`)による続行の通し — 子プロセスの `claude` が「Not logged in」で起動できず、エラーが `RESULT … loop error` として正しく報告されることまでしか確認できていない(手元の端末で `claude` にログイン済みなら動くはずだが未実行)。本番 VPS での自己更新の実行(VPS ではビルドを伴うため有効化していない)。composer は未対応(報告のみ)。

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
- **Verified (2026-11-13)**: unit tests (all 39 pass on Windows too); end-to-end with real git/cargo/npm (`itoa 0.4`→`1`, `is-odd ^2`→`^3`: separate commit + passing tests; a non-compiling update is reverted and reported as `ITEM … fail`; a vulnerability fix that needs a raised requirement ends with the lock at or above the fixed version); **a real GitHub pull request** (`aon-co-jp/open-fudousan` PR 1: `thiserror 1→2`, `vc/upgrade-DATE`, one commit, tests passing) and **no second PR while one is open** (a re-run reports `PR待ち`); **self-update with the real binary** (old clone fast-forwarded → real build → `--version` → re-run with the new build; the rollback on a failed build was also seen for real).
- **Not verified (honestly)**: the agent continuation with `claude -p` — the child `claude` process reported "Not logged in", so only the error path (reported as `RESULT … loop error`) was seen; self-update on the production VPS (it builds, so it is not enabled there); composer is report-only.

### 手元PCでの常時運転(2026-11-13登録)/ Always-on setup on the PC
- **日本語**: Windows タスク `version-control-auto`(毎日 12:40、逃した場合は起動後すぐ、1回最大6時間、同時起動なし)が `scriptsun-auto.cmd` を実行し、`version-control --auto --dry-run --no-api` を回す(`--dry-run` は**通知メールを送らない**意味で、修正と PR 作成は行う。メールは VPS 側の分だけ)。ログは `run-auto.log`(git 管理外)。手元の `config.toml` の `[fix]` は `enabled = true / allow_pr = true / upgrade = true / upgrade_max = 3 / sync_clones = false / max_workers = 2`(作業中のクローンには触れない・PR だけ・1リポジトリ3依存まで)。**止める**: `Disable-ScheduledTask version-control-auto`(消すなら `Unregister-ScheduledTask`)、または `config.toml` の `[fix] enabled = false`。PR はマージするまで開いたままで、その間は重ねて作らない。
- **English**: The Windows task `version-control-auto` (daily 12:40, catches up after a missed run, 6 h limit, no overlap) runs `scriptsun-auto.cmd` = `version-control --auto --dry-run --no-api` (`--dry-run` here only means **no notification e-mail**; fixes and pull requests still happen; the VPS sends the e-mails). Log: `run-auto.log` (git-ignored). The PC's `config.toml` `[fix]` is `enabled / allow_pr / upgrade / upgrade_max = 3 / sync_clones = false / max_workers = 2` (never touches working clones, pull requests only, 3 dependencies per repository). **Stop**: `Disable-ScheduledTask version-control-auto` (or `Unregister-ScheduledTask`), or set `[fix] enabled = false`. A pull request stays open until merged; no second one is made meanwhile.

### PR の増えすぎ・見落とし対策(`--review`、2026-11-13)/ Keeping the pull requests manageable
- **日本語**: ①1回に新しい PR を作るリポジトリは最大 `[fix] upgrade_max_repos`(既定5)まで、1リポジトリ3依存まで。残りは翌日。②`--review`(`--auto` に含まれる)が、開いている `vc/upgrade-*` PR を毎回点検する。PR 本文には機械可読の `<!-- vc-upgrade: 名前 | 旧要求 | 新版 -->` が入っていて、**全部が宣言した範囲内(メジャーなし。0.x はマイナーも不可)・GitHub がマージ可能・CI の失敗なし・まとめてのテスト合格**のものだけ、`[review] auto_merge = true` のとき squash マージしてブランチを消す。**メジャーは絶対にマージせず**、開いたまま「何を確認するか」(依存名・新旧・リリースノートの場所)を列挙する。一覧のない PR(古い形式)も人の確認に回す。③結果は `auto-report.txt`(git 管理外、`--auto`/`--review` のたびに上書き)に残る。通知メールが出ない手元 PC ではここを見る。④脆弱性データベース(advisory-db)の更新が履歴の書き換えで `pull --ff-only` に失敗し続け、古いデータで走査していた不具合を、fetch + reset に直した。
- **English**: (1) At most `[fix] upgrade_max_repos` repositories (default 5) get a new pull request per run, 3 dependencies each; the rest wait for tomorrow. (2) `--review` (part of `--auto`) triages the open `vc/upgrade-*` pull requests every run. Each body carries machine-readable `<!-- vc-upgrade: name | old requirement | new version -->` lines; **only** a pull request whose changes all stay inside the declared range (no major; for 0.x no minor either), that GitHub reports mergeable, with no failed check and our own full test run passed is squash-merged (branch deleted) when `[review] auto_merge = true`. **Majors are never merged**: the pull request stays open and the report lists what to check (dependency, old and new version, where the release notes are). Pull requests without the list (old format) go to a person. (3) The outcome is kept in `auto-report.txt` (git-ignored, overwritten on every `--auto`/`--review`); read it on the PC, which sends no e-mail. (4) The advisory-db refresh used `pull --ff-only`, which failed for good after an upstream history rewrite and left the scan on stale advisories; it now fetches and resets.
- **確認済み / Verified**: 判定ロジックの単体試験(マーカー・メジャー判定・マージ条件)、実 GitHub の PR #1 の点検(一覧が無いので開いたまま、マージされない)。**未確認 / Not verified**: 実際の自動マージ(`gh pr merge`)— 互換範囲内の PR がまだ無い。最初の1件が来たときに `auto-report.txt` で確認すること。/ A real automatic merge (`gh pr merge`): no in-range pull request exists yet; check `auto-report.txt` when the first one appears.
