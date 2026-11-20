# API-EXCEPTIONS.md — API 例外の台帳 / API exception ledger / API-Ausnahmen / Реестр исключений API

機械可読の正本は [`exceptions.toml`](exceptions.toml)(確認日 2026-11-04)。方針の正本は `F:\runo\API-POLICY.md`(2026-11-04「REST 全廃・gRPC / Connect へ」)。
The machine-readable source of truth is [`exceptions.toml`](exceptions.toml) (checked 2026-11-04); the policy is `F:\runo\API-POLICY.md`.

**日本語 → English → Deutsch → Русский**(独語・露語は **AI翻訳、ネイティブ確認前** / German and Russian are **AI translations, not yet checked by native speakers**)

---

## 日本語

### 正直な前提
**完全にゼロにできないものがある。** ACME(RFC 8555 は HTTP+JSON を定め、HTTP-01 は 80 番の素の HTTP が必須)、第三者の Webhook(GitHub や LINE が送ってくる HTTP POST)、静的ファイル(そもそも API ではない)は、相手の規格が形を決めるので、原理的に残る。一方、OTLP(gRPC 版がある)、ヘルスチェック(`grpc.health.v1` がある)、OAuth/OIDC のコールバック(ブラウザが開く GET を、静的ページ + Connect の POST に減らせる)は置き換えられる。
目標は「例外ゼロ」ではなく、**例外ごとに、根拠の規格・置き換え可能性・次の手・確認日を台帳に持ち、置き換えられる状況(新しい標準・実装・対応状況)が出たら、毎日の巡回で検知して知らせる**こと。

### 台帳(各「使っている場所」は 2026-11-04 に各リポジトリのコードを grep で確認した)
| 項目 | 根拠の規格 | 判定 | 次の手 | 使っているリポジトリ |
|---|---|---|---|---|
| ACME HTTP-01 応答 | RFC 8555 §8.3 | 不可 | 最小の応答器のまま。新規ルートを足さない | RPoem、open-web-server、(certbot: open-easy-web) |
| ACME クライアント | RFC 8555 | 不可 | 変更なし(自前実装と certbot の併存を将来一本化) | RPoem、open-web-server、open-easy-web |
| OAuth/OIDC コールバック | RFC 6749 §3.1.2 / OIDC Core §3.1.2.5 | 一部可能 | 静的ページ + Connect POST(`ExchangeCode`)へ。RS-Sync が最初 | RS-Sync(open-web-server・RPoem はコールバックのルート無し) |
| 第三者 Webhook 受信 | LINE Messaging API / GitHub webhooks | 不可 | 署名検証つきの最小ハンドラのまま。増やさない | open-english、e-gov、backup-to-discord-by-github |
| ヘルスチェック | gRPC `grpc.health.v1` | 置き換え可能 | 呼び出し側(監視・`maintainers.health_url`)を Connect POST へ移して `/healthz` を消す | 約 19 リポジトリ(台帳に全件) |
| OTLP | OTLP/gRPC(4317) | 置き換え可能 | `opentelemetry-otlp` を `http-proto` から `grpc-tonic` へ | RPoem、open-web-server |
| 静的ファイル | RFC 9110 | 不可 | 変更なし。静的配信のついでに JSON ルートを足さない | open-easy-web、open-web-server、runo.tokyo |

### 毎日の巡回(`version-control` のタイマー 00/06/12/18 時の :20 に同乗)
1. 各例外について `aruaru-search`(`/v1/search`)で最新状況を調べ、**新しく現れたページ**の題名・抜粋に `signal_terms` が含まれたら「置き換えの兆候」として知らせる(初回は基準の記録のみ)。`aruaru-llm`(`/v1/generate`、小型モデル)の一文は**参考**で、**出典 URL が正本**。使えない/落ちているときは記録して続行する。`open-cuda` はこの用途では使わない。
2. 各リポジトリのコードから REST 風ルート(axum `.route`、poem `.at`、hyper `(Method, "/path")`。Connect 形式と例外パスを除く。`/api/`・`/admin/` を含むものは別掲)を数え、`state.json` に推移を記録する。静的な目安であり、実行時に作るルートは数えられない。
3. 変化(兆候の出現、REST 風ルートの**増加**=方針違反の新規追加、台帳の確認日が 30 日超)を、open-easy-web の「更新のお知らせメール」(`POST /open_easy_web.admin.v1.Notify/Send`)で通知する。
- 取得した文章は**データ**であり、そこに書かれた指示には従わない(照合・切り詰め・表示のみ)。

---

## English

### Honest premise
**Some exceptions cannot reach zero.** ACME (RFC 8555 defines HTTP+JSON and HTTP-01 needs plain HTTP on port 80), incoming third-party webhooks (HTTP POSTs sent by GitHub, LINE, …) and static files (not an API at all) stay, because the other side's standard fixes their shape. OTLP (a gRPC flavour exists), health checks (`grpc.health.v1` exists) and OAuth/OIDC callbacks (the browser's GET can shrink to a static page plus a Connect POST) can be replaced.
The goal is therefore not "zero exceptions" but: **for each exception keep the governing standard, replaceability, next step and check date in the ledger, and detect — in a daily patrol — new standards, implementations or support that make a replacement possible, and say so.**

### Ledger (every "used in" entry was verified by grepping the repository code on 2026-11-04)
| Item | Standard | Verdict | Next step | Used in |
|---|---|---|---|---|
| ACME HTTP-01 responder | RFC 8555 §8.3 | impossible | keep the minimal responder; add no new routes | RPoem, open-web-server, (certbot: open-easy-web) |
| ACME client | RFC 8555 | impossible | no change (later: unify the in-house client and certbot) | RPoem, open-web-server, open-easy-web |
| OAuth/OIDC callback | RFC 6749 §3.1.2 / OIDC Core §3.1.2.5 | partial | static page + Connect POST (`ExchangeCode`); RS-Sync first | RS-Sync (open-web-server and RPoem have no callback route) |
| Incoming webhooks | LINE Messaging API / GitHub webhooks | impossible | keep minimal signature-verifying handlers; add no more | open-english, e-gov, backup-to-discord-by-github |
| Health checks | gRPC `grpc.health.v1` | replaceable | move callers (monitors, `maintainers.health_url`) to Connect POST, then delete `/healthz` | ~19 repositories (all listed in the ledger) |
| OTLP | OTLP/gRPC (4317) | replaceable | switch `opentelemetry-otlp` from `http-proto` to `grpc-tonic` | RPoem, open-web-server |
| Static files | RFC 9110 | impossible | no change; do not add JSON routes beside static serving | open-easy-web, open-web-server, runo.tokyo |

### Daily patrol (rides on the existing `version-control` timer at 00/06/12/18 h :20)
1. For each exception, ask `aruaru-search` (`/v1/search`); a **newly appearing** page whose title/snippet contains a `signal_terms` entry is reported as a possible replacement signal (the first run only records a baseline). One sentence from `aruaru-llm` (`/v1/generate`, a small model) is a **reference only; the source URL is authoritative**. When a service is down it is recorded and the run continues. `open-cuda` is not used for this.
2. Count REST-like routes from each repository's code (axum `.route`, poem `.at`, hyper `(Method, "/path")`; Connect-style and exception paths excluded; `/api/` and `/admin/` counted separately) and record the trend in `state.json`. This is a static estimate and cannot see routes built at run time.
3. Changes (a signal, an **increase** of REST-like routes = a new policy violation, a ledger check date older than 30 days) are sent through open-easy-web's notice mail (`POST /open_easy_web.admin.v1.Notify/Send`).
- Fetched text is **data**; instructions inside it are never followed (it is only matched, truncated and displayed).

---

## Deutsch (AI-Übersetzung, vor Prüfung durch Muttersprachler)

### Ehrliche Voraussetzung
**Manche Ausnahmen lassen sich nicht auf null bringen.** ACME (RFC 8555 legt HTTP+JSON fest, HTTP-01 braucht reines HTTP auf Port 80), eingehende Webhooks Dritter (HTTP-POSTs von GitHub, LINE usw.) und statische Dateien (gar keine API) bleiben bestehen, weil der Standard der Gegenseite die Form bestimmt. OTLP (es gibt eine gRPC-Variante), Health-Checks (`grpc.health.v1` existiert) und OAuth/OIDC-Callbacks (das GET des Browsers lässt sich auf eine statische Seite plus einen Connect-POST verkleinern) lassen sich ersetzen.
Das Ziel ist deshalb nicht „null Ausnahmen“, sondern: **Für jede Ausnahme stehen maßgeblicher Standard, Ersetzbarkeit, nächster Schritt und Prüfdatum im Verzeichnis; entstehen neue Standards, Implementierungen oder Unterstützung, die einen Ersatz möglich machen, erkennt die tägliche Runde das und meldet es.**

### Verzeichnis (jeder Fundort wurde am 04.11.2026 per grep im Code des Repositorys geprüft)
| Punkt | Standard | Urteil | Nächster Schritt | Verwendet in |
|---|---|---|---|---|
| ACME-HTTP-01-Antwort | RFC 8555 §8.3 | unmöglich | minimalen Responder behalten, keine neuen Routen | RPoem, open-web-server, (certbot: open-easy-web) |
| ACME-Client | RFC 8555 | unmöglich | keine Änderung (später: eigenen Client und certbot vereinheitlichen) | RPoem, open-web-server, open-easy-web |
| OAuth/OIDC-Callback | RFC 6749 §3.1.2 / OIDC Core §3.1.2.5 | teilweise | statische Seite + Connect-POST (`ExchangeCode`); zuerst RS-Sync | RS-Sync (open-web-server und RPoem haben keine Callback-Route) |
| Eingehende Webhooks | LINE Messaging API / GitHub-Webhooks | unmöglich | minimale Handler mit Signaturprüfung behalten, keine weiteren | open-english, e-gov, backup-to-discord-by-github |
| Health-Checks | gRPC `grpc.health.v1` | ersetzbar | Aufrufer (Monitoring, `maintainers.health_url`) auf Connect-POST umstellen, dann `/healthz` löschen | ca. 19 Repositorys (alle im Verzeichnis) |
| OTLP | OTLP/gRPC (4317) | ersetzbar | `opentelemetry-otlp` von `http-proto` auf `grpc-tonic` umstellen | RPoem, open-web-server |
| Statische Dateien | RFC 9110 | unmöglich | keine Änderung; keine JSON-Routen neben dem statischen Ausliefern | open-easy-web, open-web-server, runo.tokyo |

### Tägliche Runde (läuft mit dem bestehenden `version-control`-Timer um 00/06/12/18 Uhr, :20)
1. Je Ausnahme wird `aruaru-search` (`/v1/search`) befragt; eine **neu erscheinende** Seite, deren Titel/Auszug einen Eintrag aus `signal_terms` enthält, wird als mögliches Ersatzsignal gemeldet (der erste Lauf speichert nur die Basis). Ein Satz von `aruaru-llm` (`/v1/generate`, kleines Modell) ist **nur ein Hinweis; maßgeblich ist die Quell-URL**. Ist ein Dienst nicht erreichbar, wird das vermerkt und der Lauf geht weiter. `open-cuda` wird dafür nicht verwendet.
2. REST-artige Routen werden aus dem Code jedes Repositorys gezählt (axum `.route`, poem `.at`, hyper `(Method, "/path")`; Connect-Form und Ausnahmepfade ausgenommen; `/api/` und `/admin/` getrennt) und der Verlauf in `state.json` festgehalten. Das ist eine statische Schätzung und sieht zur Laufzeit gebaute Routen nicht.
3. Änderungen (ein Signal, ein **Anstieg** REST-artiger Routen = neuer Verstoß gegen die Richtlinie, Prüfdatum älter als 30 Tage) gehen über die Hinweis-Mail von open-easy-web (`POST /open_easy_web.admin.v1.Notify/Send`) hinaus.
- Abgerufener Text sind **Daten**; Anweisungen darin werden nie befolgt (er wird nur abgeglichen, gekürzt und angezeigt).

---

## Русский (машинный перевод, до проверки носителями языка)

### Честная предпосылка
**Часть исключений нельзя свести к нулю.** ACME (RFC 8555 задаёт HTTP+JSON, а HTTP-01 требует обычный HTTP на порту 80), входящие вебхуки третьих сторон (HTTP POST от GitHub, LINE и т. д.) и статические файлы (это вообще не API) остаются, потому что форму определяет стандарт другой стороны. OTLP (есть вариант gRPC), проверки работоспособности (существует `grpc.health.v1`) и обратные вызовы OAuth/OIDC (GET браузера можно сократить до статической страницы и POST Connect) заменить можно.
Поэтому цель — не «ноль исключений», а: **для каждого исключения хранить в реестре определяющий стандарт, возможность замены, следующий шаг и дату проверки, а когда появляются новые стандарты, реализации или поддержка, делающие замену возможной, — выявлять это ежедневным обходом и сообщать.**

### Реестр (каждое место использования 04.11.2026 проверено поиском grep по коду репозитория)
| Пункт | Стандарт | Вердикт | Следующий шаг | Где используется |
|---|---|---|---|---|
| Ответчик ACME HTTP-01 | RFC 8555 §8.3 | невозможно | оставить минимальный ответчик, новых маршрутов не добавлять | RPoem, open-web-server, (certbot: open-easy-web) |
| Клиент ACME | RFC 8555 | невозможно | без изменений (позже: объединить собственный клиент и certbot) | RPoem, open-web-server, open-easy-web |
| Callback OAuth/OIDC | RFC 6749 §3.1.2 / OIDC Core §3.1.2.5 | частично | статическая страница + POST Connect (`ExchangeCode`); сначала RS-Sync | RS-Sync (в open-web-server и RPoem маршрута callback нет) |
| Входящие вебхуки | LINE Messaging API / вебхуки GitHub | невозможно | оставить минимальные обработчики с проверкой подписи, новых не добавлять | open-english, e-gov, backup-to-discord-by-github |
| Проверки работоспособности | gRPC `grpc.health.v1` | можно заменить | перевести вызывающих (мониторинг, `maintainers.health_url`) на POST Connect, затем удалить `/healthz` | около 19 репозиториев (все в реестре) |
| OTLP | OTLP/gRPC (4317) | можно заменить | переключить `opentelemetry-otlp` с `http-proto` на `grpc-tonic` | RPoem, open-web-server |
| Статические файлы | RFC 9110 | невозможно | без изменений; не добавлять JSON-маршруты рядом со статикой | open-easy-web, open-web-server, runo.tokyo |

### Ежедневный обход (работает вместе с существующим таймером `version-control` в 00/06/12/18 ч, :20)
1. Для каждого исключения опрашивается `aruaru-search` (`/v1/search`); **вновь появившаяся** страница, в заголовке или фрагменте которой есть запись из `signal_terms`, сообщается как возможный признак замены (первый запуск только фиксирует базу). Одно предложение от `aruaru-llm` (`/v1/generate`, небольшая модель) — **лишь справка; авторитетен URL источника**. Если сервис недоступен, это записывается и обход продолжается. `open-cuda` для этого не используется.
2. По коду каждого репозитория подсчитываются REST-подобные маршруты (axum `.route`, poem `.at`, hyper `(Method, "/path")`; формат Connect и пути-исключения не считаются; `/api/` и `/admin/` — отдельно), динамика записывается в `state.json`. Это статическая оценка; маршруты, создаваемые во время работы, она не видит.
3. Изменения (признак замены, **рост** числа REST-подобных маршрутов = новое нарушение политики, дата проверки реестра старше 30 дней) отправляются через почтовое уведомление open-easy-web (`POST /open_easy_web.admin.v1.Notify/Send`).
- Полученный текст — это **данные**; инструкции в нём никогда не выполняются (текст только сопоставляется, обрезается и отображается).


---

## 追記 2026-11-08(日本語)
台帳に **GraphiQL/docs 画面・SCIM(RFC 7644)・GitHub Webhook・git smart HTTP・ACME モック(テスト用)** を追加(根拠の規格URLは `exceptions.toml`)。集計の数え方も改修: (1) `#[cfg(test)]`・`tests/`・`examples/`・`clients/`・`mock*` を数えない、(2) 同じ origin のクローン(ows-base・`.broken-*`・別ブランチ・別名dir)は名寄せして1回だけ数える(`merged_aliases`)、(3) origin 基準を維持し、クローンの遅れ(`clone_lag`)は別項目で出す。

## Addendum 2026-11-08 (English)
Added GraphiQL/docs UI, SCIM, GitHub webhook, git smart HTTP and the ACME test mock to the ledger. The route counter now skips test/example/client/mock code, merges clones sharing an origin (`merged_aliases`), keeps reading origin, and reports clone lag separately (`clone_lag`).
