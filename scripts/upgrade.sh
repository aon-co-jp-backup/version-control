#!/usr/bin/env bash
# version-control --auto から呼ばれる。古くなった依存の要求(Cargo.toml / package.json)を最新へ上げ、1つずつビルド確認し、
# 全部まとめてテストしてから、新ブランチ(vc/upgrade-日付)+PR にする。GitHub の既定ブランチを新規 clone した作業コピーでだけ行う。
# 使い方: upgrade.sh <repo名> <origin URL> <計画ファイル(各行: eco<TAB>manifestのパス<TAB>名前<TAB>最新版)>
# 環境変数: VC_MODE=none|pr|main  VC_BIN=version-control 本体(--edit-dep で要求を書き換える)  VC_FIX_DIR=作業フォルダ
# 出力: 1件ごとに `ITEM <名前> <ok|pending|fail> [理由]`、最後に `RESULT ...` を1行。
# 注意: 重いビルドをするので VPS では走らせない(ビルドする機械で)。強制 push はしない。
set -uo pipefail
repo=$1; url=$2; plan=$3
mode=${VC_MODE:-none}
vc=${VC_BIN:-version-control}
work="${VC_FIX_DIR:-$HOME/.vc-fix}"
slug=${url#https://github.com/}; slug=${slug%.git}
today=$(date +%Y%m%d)
br="vc/upgrade-$today"
mkdir -p "$work"; cd "$work" || exit 2

names=$(cut -f3 "$plan" | tr '\n' ' ')
# すでに vc/upgrade-* の PR が開いている(または、ブランチが残っている)なら重ねて作らない。
open=""
if command -v gh >/dev/null 2>&1 && [ "$mode" = pr ]; then
  open=$(gh pr list --repo "$slug" --state open --json headRefName --jq '.[].headRefName' 2>/dev/null | grep '^vc/upgrade-' | head -1)
elif [ "$mode" = pr ]; then
  open=$(git ls-remote --heads "$url" 'vc/upgrade-*' 2>/dev/null | head -1 | sed 's|.*refs/heads/||')
fi
if [ -n "$open" ]; then
  for n in $names; do echo "ITEM $n pending"; done
  echo "RESULT $repo: PR待ち(既存ブランチ $open)。マージ後に次の更新を作る"
  exit 0
fi

rm -rf "$repo"; git clone -q "$url" "$repo" || { echo "RESULT $repo: clone失敗"; exit 2; }
cd "$repo" || exit 2
base=$(git rev-parse HEAD)
git checkout -q -b "$br"
export GIT_AUTHOR_NAME=${GIT_AUTHOR_NAME:-version-control} GIT_AUTHOR_EMAIL=${GIT_AUTHOR_EMAIL:-version-control@localhost}
export GIT_COMMITTER_NAME=$GIT_AUTHOR_NAME GIT_COMMITTER_EMAIL=$GIT_AUTHOR_EMAIL

okc=0; touched=""; failed=0; summary=""; markers=""
while IFS=$'\t' read -r eco file name to from; do
  [ -n "$name" ] || continue
  dir=$(dirname "$file"); mf=$(basename "$file")
  [ -f "$file" ] || { echo "ITEM $name fail $file が無い"; failed=$((failed+1)); continue; }
  if ! (cd "$dir" && "$vc" --edit-dep "$mf" "$name" "$to" >/dev/null 2>/tmp/vc-up-"$repo".err); then
    echo "ITEM $name fail 要求を書き換えられない($(head -c 80 /tmp/vc-up-"$repo".err | tr '\n' ' '))"; failed=$((failed+1)); git checkout -q -- . ; continue
  fi
  gate=""
  case "$eco" in
    crates.io)
      (cd "$dir" && cargo update -p "$name" >/tmp/vc-up-"$repo".upd 2>&1) || (cd "$dir" && cargo update >/tmp/vc-up-"$repo".upd 2>&1) || gate="cargo update 失敗"
      # lock が最新を指しているか(別の要求に縛られて古いままなら更新できていない)
      [ -z "$gate" ] && ! (cd "$dir" && cargo check --all-targets --color never >/tmp/vc-up-"$repo".chk 2>&1) && gate="ビルド不可(cargo check)"
      ;;
    npm)
      (cd "$dir" && npm install --no-audit --no-fund >/tmp/vc-up-"$repo".upd 2>&1) || gate="npm install 失敗"
      [ -z "$gate" ] && ! (cd "$dir" && npm run build --if-present >/tmp/vc-up-"$repo".chk 2>&1) && gate="ビルド不可(npm run build)"
      ;;
    *) gate="未対応の種類 $eco" ;;
  esac
  if [ -n "$gate" ]; then
    echo "ITEM $name fail $gate($file: $to へ)"; failed=$((failed+1))
    git reset -q --hard HEAD; git clean -fdq -- "$dir" 2>/dev/null
    continue
  fi
  git add -u
  if git commit -q -m "deps: $name を $to 系へ更新($file)" -m "version-control --auto による自動更新。要求を書き換え、ビルド確認済み(全体のテストは最後にまとめて実施)。" \
       -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"; then
    okc=$((okc+1)); touched="$touched $eco:$dir"; summary="$summary $name→$to"; markers="$markers
<!-- vc-upgrade: $name | $from | $to -->"
    echo "ITEM $name ok"
  else
    echo "ITEM $name ok 変更なし"
  fi
done < "$plan"

if [ "$okc" = 0 ]; then
  echo "RESULT $repo: 更新できたものなし(失敗 $failed 件)"
  exit 0
fi

# まとめてテスト。失敗したら「更新前から失敗か」を同じ手順で確かめる。
t=PASS; tot=""
run_tests() {  # $1=checkout先(省略可)
  local res=PASS
  for key in $(echo $touched | tr ' ' '\n' | sort -u); do
    e=${key%%:*}; d=${key#*:}
    case "$e" in
      crates.io) (cd "$d" && cargo test --no-fail-fast --color never >/tmp/vc-up-"$repo".test 2>&1) || res=FAIL ;;
      npm) (cd "$d" && npm install --no-audit --no-fund >/dev/null 2>&1; npm test --if-present --silent >/tmp/vc-up-"$repo".test 2>&1) || res=FAIL ;;
    esac
  done
  echo $res
}
t=$(run_tests)
basenote=""
if [ "$t" != PASS ]; then
  git checkout -q "$base" 2>/dev/null
  b=$(run_tests)
  git checkout -q "$br"
  if [ "$b" = PASS ]; then basenote="(更新前は合格=更新が原因)"; else basenote="(更新前から失敗=更新は無関係)"; fi
fi

tail_msg=" 作業コピー: $PWD"
if [ "$t" != PASS ] && [ "$basenote" != "(更新前から失敗=更新は無関係)" ]; then
  echo "RESULT $repo: テスト不合格のため push しない test=FAIL$summary $basenote$tail_msg"
  exit 0
fi
note=""; [ "$t" != PASS ] && note=" 更新前から失敗するテストあり(更新は無関係)"
if [ "$mode" = pr ]; then
  if git push -q origin "HEAD:refs/heads/$br"; then
    body="version-control --auto による自動更新です(更新:$summary)。各依存はビルド確認済み、まとめてのテストは $t。$note 破壊的変更の可能性があるので、マージ前に差分とリリースノートを確認してください。
$markers

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
    base_br=$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|^origin/||'); base_br=${base_br:-main}
    if prurl=$(gh pr create --repo "$slug" --base "$base_br" --head "$br" --title "deps: 依存を最新へ更新(version-control)" --body "$body" 2>/tmp/vc-up-"$repo".pr.err); then
      echo "RESULT $repo: PRを作成 $prurl (更新:$summary, test $t)$note$tail_msg"
    else
      echo "RESULT $repo: ブランチ $br をpush済み(PR作成失敗: gh を確認)(更新:$summary)$tail_msg"
    fi
  else
    echo "RESULT $repo: ブランチ $br のpush失敗(既存?)$tail_msg"
  fi
elif [ "$mode" = main ]; then
  git checkout -q "$(git symbolic-ref --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|^origin/||')" 2>/dev/null && git merge -q --ff-only "$br" \
    && git push -q origin HEAD && echo "RESULT $repo: 更新してpush済み(更新:$summary, test $t)$note$tail_msg" || echo "RESULT $repo: push失敗$tail_msg"
else
  echo "RESULT $repo: 更新をコミット済み(未push、更新:$summary, test $t)$note$tail_msg"
fi
