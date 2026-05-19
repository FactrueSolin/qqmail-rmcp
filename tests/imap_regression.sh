#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo_cmd="cargo"
if command -v cargo.exe >/dev/null 2>&1; then
  cargo_cmd="cargo.exe"
elif [ -x /mnt/c/Users/Daifuku/.cargo/bin/cargo.exe ] && [ "$(pwd -P | cut -c1-5)" = "/mnt/" ]; then
  cargo_cmd="/mnt/c/Users/Daifuku/.cargo/bin/cargo.exe"
fi

echo "[imap-regression] running Rust unit tests for IMAP parsing and request construction"
"$cargo_cmd" test mail::imap::tests -- --nocapture

echo "[imap-regression] verifying list_messages uses strict RFC-compatible parenthesized FETCH items"
if ! grep -Fq 'const MESSAGE_SUMMARY_FETCH_ITEMS: &str = "(UID FLAGS RFC822.SIZE BODY.PEEK[HEADER])";' src/mail/imap.rs; then
  echo "[imap-regression] FAIL: list_messages summary fetch items are not parenthesized" >&2
  exit 1
fi

echo "[imap-regression] verifying UID mutation operations reject nonexistent messages before side effects"
uid_check_count=$(grep -Fc 'assert_uid_exists(&mut session' src/mail/imap.rs)
if [ "$uid_check_count" -lt 2 ]; then
  echo "[imap-regression] FAIL: expected delete/move to check UID existence, found $uid_check_count checks" >&2
  exit 1
fi

echo "[imap-regression] verifying mark_message reads current flags before writing QQ-compatible mutations"
if ! grep -Fq 'session.uid_fetch(req.uid.to_string(), "(UID FLAGS)")' src/mail/imap.rs; then
  echo "[imap-regression] FAIL: mark_message must fetch current UID FLAGS before STORE" >&2
  exit 1
fi

if ! grep -Fq 'plan_flag_mutation(' src/mail/imap.rs || ! grep -Fq 'test_plan_flag_mutation_skips_noop_false_for_absent_flag' src/mail/imap.rs; then
  echo "[imap-regression] FAIL: mark_message must skip no-op flag mutations" >&2
  exit 1
fi

echo "[imap-regression] PASS: IMAP regression checks completed"
