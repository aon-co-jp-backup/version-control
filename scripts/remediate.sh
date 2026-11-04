#!/usr/bin/env bash
# version-control --fix --apply から呼ばれる。GitHub の main を新規 clone した作業コピーでだけ直す(手元の作業ツリーには触れない)。
# 使い方: remediate.sh <repo名> <origin URL> <ロックのあるサブディレクトリ(. 可)> <crate> <mitigate|bump> [bump先の版] [--push]
# 流れ: clone → 修正(patch 追加 / cargo update --precise) → 勧告の対象が lock から消えたか確認 → cargo test --no-fail-fast
#       → 小さな別コミット(push は --push のときだけ)。結果は最後に1行で出す。
# 注意: 重いビルドをするので VPS では走らせない。手元(WSL 等)で。
set -uo pipefail
repo=$1; url=$2; sub=$3; crate=$4; kind=$5; ver=${6:-}; push=${7:-}
[ "$ver" = "--push" ] && { push=--push; ver=; }
work="${VC_FIX_DIR:-$HOME/.vc-fix}"
mkdir -p "$work"; cd "$work"
rm -rf "$repo"; git clone -q "$url" "$repo" || { echo "RESULT $repo: clone失敗"; exit 2; }
cd "$repo/$sub" || { echo "RESULT $repo: サブディレクトリ $sub が無い"; exit 2; }
lock=Cargo.lock
[ -f "$lock" ] || { echo "RESULT $repo: $sub に Cargo.lock が無い(追跡されていない)"; exit 3; }

versions=$(grep -A1 "^name = \"$crate\"\$" "$lock" | grep '^version' | sed 's/version = "\(.*\)"/\1/')
[ -n "$versions" ] || { echo "RESULT $repo: ロックに $crate が無い(対応済み?)"; exit 0; }

case "$kind" in
  bump)
    [ -n "$ver" ] || { echo "RESULT $repo: bump先の版が未指定"; exit 2; }
    for v in $versions; do cargo update -p "$crate@$v" --precise "$ver" >/dev/null 2>&1 || cargo update -p "$crate" --precise "$ver" 2>&1 | tail -2; done
    msg="deps: $crate を $ver に更新(RustSec 勧告の修正版)"
    ;;
  mitigate)
    [ "$crate" = rsa ] || { echo "RESULT $repo: $crate の緩和策は未登録"; exit 2; }
    {
      printf '\n# RUSTSEC-2023-0071 (rsa Marvin Attack, 修正版なし) 回避: aon-co-jp/rsa-rs の互換実装へ差し替え。\n[patch.crates-io]\n'
      for v in $versions; do
        case "$v" in
          0.10.0-rc.18) echo 'rsa = { git = "https://github.com/aon-co-jp/rsa-rs", tag = "v0.1.0", version = "=0.10.0-rc.18" }' ;;
          0.9.10)       echo 'rsa-090 = { package = "rsa", git = "https://github.com/aon-co-jp/rsa-rs", tag = "v0.1.0", version = "=0.9.10" }' ;;
          *) echo "# 未対応の rsa 版: $v" >&2; echo "RESULT $repo: rsa $v は rsa-rs が未対応"; exit 4 ;;
        esac
      done
    } >> Cargo.toml
    cargo update -p crypto-bigint --precise 0.7.10 >/dev/null 2>&1 || true   # 無ければ新規解決で >=0.7.10 になる
    msg="deps: rsa を rsa-rs に置き換え(RUSTSEC-2023-0071)"
    ;;
  *) echo "RESULT $repo: 不明な kind $kind"; exit 2 ;;
esac

# 勧告の対象が registry 由来で残っていないか
if [ "$kind" = mitigate ]; then
  left=$(awk -v c="$crate" '/^name = /{n=$3} /^source = /{if(n=="\""c"\"" && $3 ~ /registry/) print n}' "$lock" | wc -l)
  [ "$left" = 0 ] || { echo "RESULT $repo: $crate が registry 由来で残っている($left)"; exit 5; }
fi

if cargo test --no-fail-fast --color never >/tmp/vc-fix-"$repo".log 2>&1; then t=PASS; else t=FAIL; fi
tot=$(awk '/^test result:/ {p+=$4; f+=$6} END {print p" pass / "f" fail"}' /tmp/vc-fix-"$repo".log)
[ "$t" = FAIL ] && ! grep -q "^test result" /tmp/vc-fix-"$repo".log && t=BUILD-ERROR

git add Cargo.toml "$lock"
git -c user.name="${GIT_AUTHOR_NAME:-version-control}" -c user.email="${GIT_AUTHOR_EMAIL:-version-control@localhost}" \
  commit -q -m "$msg" -m "version-control --fix による自動対応。検証: cargo test = $t ($tot)。パッチ前から失敗するテストがある場合は事前に比較すること。" \
  -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>" || { echo "RESULT $repo: 変更なし"; exit 0; }
if [ "$push" = "--push" ] && [ "$t" = PASS ]; then
  git push -q origin HEAD && echo "RESULT $repo: 修正してpush済み(test $tot)" || echo "RESULT $repo: push失敗"
else
  echo "RESULT $repo: 修正をコミット済み(未push) test=$t $tot  作業コピー: $PWD"
fi
