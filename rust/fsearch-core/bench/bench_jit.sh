#!/usr/bin/env bash
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
C_MATCHER_BIN="${FSEARCH_C_MATCHER_BIN:-$HERE/../c_parity/c_matcher}"
if [ ! -x "$C_MATCHER_BIN" ]; then
  echo "c_matcher not found or not executable: $C_MATCHER_BIN" >&2
  exit 2
fi

PATTERN="test"
TEXT=$(printf '%0s' ""; for i in {1..2000}; do printf 'The quick brown fox jumps over the lazy dog. '; done)
ITER=200

make_corpus_paths() {
  # generate a corpus of file paths joined by newlines
  local out=""
  for i in $(seq 1 2000); do
    out+="/home/user/projects/repo/src/module${i}/file_${i}.rs\n"
  done
  printf "%b" "$out"
}

make_long_doc() {
  for i in $(seq 1 5000); do
    printf 'Lorem ipsum dolor sit amet, consectetur adipiscing elit. '\
           'Pellentesque habitant morbi tristique senectus et netus. '\
           'Curabitur vulputate. '\
      
  done
}

make_unicode_text() {
  # mix of CJK, emojis and combining accents
  for i in $(seq 1 2000); do
    printf 'ファイル名_测试_é_😀 ' 
  done
}

run_case() {
  local name=$1
  local pattern=$2
  local text="$3"
  echo "\n=== Case: $name ==="
  echo "Pattern: $pattern"
  tmpf=$(mktemp)
  printf "%s" "$text" > "$tmpf"
  start=$(date +%s%N)
  for i in $(seq 1 $ITER); do
    "$C_MATCHER_BIN" --pattern "$pattern" --text-file "$tmpf" > /dev/null
  done
  end=$(date +%s%N)
  total_nojit=$((end-start))
  avg_ns=$((total_nojit/ITER))
  printf "no-JIT total: %d ns, avg: %.3f ms\n" "$total_nojit" "$(echo "scale=6; $avg_ns/1000000" | bc)"

  start=$(date +%s%N)
  for i in $(seq 1 $ITER); do
    "$C_MATCHER_BIN" --jit --pattern "$pattern" --text-file "$tmpf" > /dev/null
  done
  end=$(date +%s%N)
  total_jit=$((end-start))
  avg_ns=$((total_jit/ITER))
  printf "with-JIT total: %d ns, avg: %.3f ms\n" "$total_jit" "$(echo "scale=6; $avg_ns/1000000" | bc)"
}

echo "Using c_matcher: $C_MATCHER_BIN"

paths=$(make_corpus_paths)
longdoc=$(make_long_doc)
unic=$($(declare -f make_unicode_text) >/dev/null 2>&1 || true; make_unicode_text)

# Cases: filename-like corpus, long document, unicode-heavy text, complex regex over paths
run_case "paths_corpus" "file_1234" "$paths"
run_case "long_document" "Lorem" "$longdoc"
run_case "unicode_text" "ファイル名" "$unic"
run_case "complex_path_regex" "module[0-9]{3}/file_[0-9]{3}\\.rs" "$paths"

rm -f "$tmpf"

echo Done
