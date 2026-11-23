#!/usr/bin/env bash
#
# Runs the checks requested by the composite action and summarises them.
# Called by action.yml, and runnable by hand.
#
# Environment:
#   LEAN_AUDIT_BIN               path to the lean-audit binary        (required)
#   LEAN_AUDIT_TARGET            repository root to audit             (default .)
#   LEAN_AUDIT_CHECKS            inspect,manifest,scan,axioms         (default inspect,manifest,scan)
#   LEAN_AUDIT_VERIFY_HASHES     true|false                           (default true)
#   LEAN_AUDIT_AXIOMS_ARGS       extra arguments for the axioms check (default empty)
#   LEAN_AUDIT_REPORT_DIR        directory for the Markdown reports   (default lean-audit-report)
#   LEAN_AUDIT_FAIL_ON_MISMATCH  true|false                           (default true)
#   GITHUB_STEP_SUMMARY          written to when set
#   GITHUB_OUTPUT                status and report-dir written to when set
#
# Exit codes:
#   0   every check passed
#   2   forbidden tokens found in scope (scan)
#   3   a mismatch was reported (manifest, scan or axioms)
#   4   a check could not be run
#
# A mismatch only fails the job when LEAN_AUDIT_FAIL_ON_MISMATCH is true; a check
# that could not be run always fails it, because that is not a pass.

set -uo pipefail

bin="${LEAN_AUDIT_BIN:?LEAN_AUDIT_BIN is required}"
target="${LEAN_AUDIT_TARGET:-.}"
checks="${LEAN_AUDIT_CHECKS:-inspect,manifest,scan}"
verify="${LEAN_AUDIT_VERIFY_HASHES:-true}"
axioms_args="${LEAN_AUDIT_AXIOMS_ARGS:-}"
report_dir="${LEAN_AUDIT_REPORT_DIR:-lean-audit-report}"
fail_on_mismatch="${LEAN_AUDIT_FAIL_ON_MISMATCH:-true}"

if [ ! -x "$bin" ]; then
  echo "lean-audit: binary not executable: $bin" >&2
  exit 4
fi

mkdir -p "$report_dir"

worst=0
any_error=0
rows=""

for check in $(printf '%s' "$checks" | tr ',' ' '); do
  case "$check" in
    inspect | manifest | scan | axioms) ;;
    *)
      echo "lean-audit: unknown check '$check' (expected inspect, manifest, scan or axioms)" >&2
      exit 4
      ;;
  esac

  args=(--format md --out "$report_dir/$check.md")
  if [ "$check" = "inspect" ] && [ "$verify" != "true" ]; then
    args+=(--no-verify)
  fi
  if [ "$check" = "axioms" ] && [ -n "$axioms_args" ]; then
    # Deliberately word-split: the caller passes a plain argument string.
    # shellcheck disable=SC2206
    args+=($axioms_args)
  fi

  "$bin" "$check" "$target" "${args[@]}" >/dev/null 2>"$report_dir/$check.err"
  rc=$?

  case "$rc" in
    0) result="ok" ;;
    2) result="forbidden tokens in scope" ;;
    3) result="mismatch" ;;
    4) result="could not run" ;;
    *) result="unexpected exit $rc" ;;
  esac

  if [ "$rc" -gt "$worst" ]; then worst=$rc; fi
  if [ "$rc" -ge 4 ]; then any_error=1; fi

  rows="${rows}| \`$check\` | $rc | $result | \`$report_dir/$check.md\` |
"
  echo "lean-audit: $check -> $result (exit $rc)"
done

case "$worst" in
  0) status="ok" ;;
  2 | 3) status="mismatch" ;;
  *) status="error" ;;
esac

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "## lean-audit"
    echo
    echo "Target: \`$target\` · checks: \`$checks\` · status: **$status**"
    echo
    echo "| check | exit | result | report |"
    echo "|---|---|---|---|"
    printf '%s' "$rows"
    echo
    echo "_This verifies reproducibility of published claims. It does not establish"
    echo "mathematical correctness._"
  } >>"$GITHUB_STEP_SUMMARY"
fi

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "status=$status"
    echo "report-dir=$report_dir"
  } >>"$GITHUB_OUTPUT"
fi

if [ "$any_error" = "1" ]; then
  echo "lean-audit: at least one check could not be run" >&2
  exit 4
fi

if [ "$worst" = "0" ]; then
  exit 0
fi

if [ "$fail_on_mismatch" = "true" ]; then
  echo "lean-audit: mismatch reported" >&2
  exit "$worst"
fi

echo "lean-audit: mismatch reported, not failing the job (fail-on-mismatch=false)"
exit 0
