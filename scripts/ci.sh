#!/bin/sh
# CI locale (§10 étape 1) : à lancer avant chaque commit.
set -e
cd "$(dirname "$0")/.."

echo "== cargo clippy =="
(cd src-tauri && cargo clippy --all-targets -- -D warnings)

echo "== cargo test =="
(cd src-tauri && cargo test)

echo "== tsc =="
npx tsc --noEmit

echo "== vitest =="
npx vitest run --passWithNoTests

echo "CI OK"
