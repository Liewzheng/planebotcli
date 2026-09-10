#!/usr/bin/env bash
# Live end-to-end smoke for pbot against a real Plane instance.
# Covers: create → markdown comment → state change → attachment upload →
# inline image embed → cleanup. Requires ~/.plane_api credentials and a
# reachable instance; paces calls to stay under the instance rate limit.
#
# Usage: scripts/e2e.sh [-p PROJECT]   (default project: PLANECLI)

set -euo pipefail

PROJECT="${1:-PLANECLI}"
PBOT="${PBOT:-pbot}"
PAUSE="${PAUSE:-4}"          # seconds between calls (rate-limit buffer)
MARK="pbot-e2e-$$"           # unique marker for cleanup
FAILURES=0

say()  { printf '\n== %s\n' "$*"; }
ok()   { printf '   ok: %s\n' "$*"; }
die()  { printf '   FAIL: %s\n' "$*" >&2; FAILURES=$((FAILURES + 1)); }

cleanup() {
  say "cleanup $MARK"
  pbot wi ls -p "$PROJECT" --no-cache --json 2>/dev/null \
    | jq -r '.[] | select(.name | startswith("pbot-e2e")) | .id' \
    | while read -r id; do
        pbot wi delete "$id" -p "$PROJECT" >/dev/null 2>&1 || true
        sleep "$PAUSE"
      done
}
trap cleanup EXIT

export http_proxy="${http_proxy:-}" https_proxy="${https_proxy:-}"
# Use the correct proxy if the env one is malformed (double-encoded).
if [[ "${https_proxy:-}" == *"%3A"* ]]; then
  export https_proxy="http://192.168.1.180:7890" http_proxy="http://192.168.1.180:7890"
fi

say "whoami"
out="$($PBOT whoami --json 2>/dev/null)" && jq -e .id >/dev/null <<<"$out" \
  && ok "whoami" || die "whoami"

say "create work item"
sleep "$PAUSE"
created="$($PBOT wi create "pbot-e2e $MARK" -p "$PROJECT" --state Todo \
  --labels "" --desc-md "# $MARK"$'\n\n''- a'$'\n''- b' --json 2>/dev/null)"
id="$(jq -r .id <<<"$created")"
[[ -n "$id" && "$id" != "null" ]] && ok "created $id" || { die "create"; exit 1; }
sleep "$PAUSE"

say "markdown description landed"
desc="$($PBOT wi show "$id" --no-comments --no-cache --json 2>/dev/null)"
grep -q '<h1>' <<<"$(jq -r .description_html <<<"$desc")" && ok "desc md h1" || die "desc md"

say "comment (--body-md)"
sleep "$PAUSE"
$PBOT comment create "$id" --body-md "## 进度"$'\n\n'"正文" -p "$PROJECT" --json >/dev/null 2>&1 \
  && ok "comment" || die "comment"

say "state change -> In Progress"
sleep "$PAUSE"
$PBOT wi update "$id" -p "$PROJECT" --state "In Progress" --json >/dev/null 2>&1
st="$($PBOT wi show "$id" --no-comments --no-cache --json 2>/dev/null | jq -r .state_detail_name)"
[[ "$st" == "In Progress" ]] && ok "state=$st" || die "state=$st"

say "attachment upload"
png="/tmp/$MARK.png"
python3 -c "import base64;open('$png','wb').write(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='))"
sleep "$PAUSE"
$PBOT attachment attach "$id" -f "$png" -p "$PROJECT" --json >/dev/null 2>&1 && ok "attach" || die "attach"

say "inline image (-i) on update"
sleep "$PAUSE"
$PBOT wi update "$id" -p "$PROJECT" -i "$png" --json >/dev/null 2>&1
html="$($PBOT wi show "$id" --no-comments --no-cache --json 2>/dev/null | jq -r .description_html)"
grep -q '<img ' <<<"$html" && ok "inline img" || die "inline img"
rm -f "$png"

say "verify attachment uploaded"
sleep "$PAUSE"
up="$($PBOT attachment ls "$id" -p "$PROJECT" --no-cache --json 2>/dev/null | jq '[.[] | select(.is_uploaded==true)] | length')"
[[ "$up" -ge 2 ]] && ok "attachments uploaded=$up" || die "attachments uploaded=$up"

say "result"
if [[ "$FAILURES" -eq 0 ]]; then echo "E2E PASS"; else echo "E2E FAIL ($FAILURES)"; exit 1; fi
