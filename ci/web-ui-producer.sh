#!/usr/bin/env bash

set -u
set -o pipefail

check_name=${CHECK_NAME:?CHECK_NAME is required}
case "$check_name" in
  web-ui|web-ui-fleet|web-ui-pipeline|web-ui-governance|web-ui-exports|web-ui-design-parity) ;;
  *)
    printf 'Unsupported Web UI check: %s\n' "$check_name" >&2
    exit 64
    ;;
esac

nix_bin=${NIX_BIN:-nix}
copy_bin=${COPY_BIN:-cp}
curl_bin=${CURL_BIN:-curl}
jq_bin=${JQ_BIN:-jq}
system=${NIX_SYSTEM:-x86_64-linux}
blocking=${WEB_UI_BLOCKING:-true}
case "$blocking" in
  true|false) ;;
  *)
    printf 'WEB_UI_BLOCKING must be true or false\n' >&2
    exit 64
    ;;
esac
artifact_root=${WEB_UI_EVIDENCE_ROOT:-web-ui-evidence}
check_dir="$artifact_root/$check_name"
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
mkdir -p "$check_dir"
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
verdict_checker="$script_dir/check-web-ui-verdict.js"
producer_started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
producer_started_ms=$(date +%s%3N)
job_started_at=${CI_JOB_STARTED_AT:-$producer_started_at}
pipeline_created_at=${CI_PIPELINE_CREATED_AT:-}
queue_duration_ms=null
queue_duration_source=unavailable
if [ -n "${CI_API_V4_URL:-}" ] && [ -n "${CI_JOB_TOKEN:-}" ]; then
  job_json=$("$curl_bin" --fail --silent --show-error \
    --connect-timeout 5 --max-time 15 \
    --header "JOB-TOKEN: ${CI_JOB_TOKEN}" \
    "${CI_API_V4_URL}/job" 2>/dev/null || true)
  reported_queue_ms=$(printf '%s' "$job_json" | "$jq_bin" -er \
    'if (.queued_duration | type) == "number" then (.queued_duration * 1000 | round) else empty end' \
    2>/dev/null || true)
  if [ -n "$reported_queue_ms" ]; then
    queue_duration_ms=$reported_queue_ms
    queue_duration_source=gitlab-jobs-api
  fi
fi

# Evaluate the evidence output before realization. Its presence in the local
# store distinguishes a local cache hit from work performed by this job.
lookup_started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
lookup_started_ms=$(date +%s%3N)
evidence_path=$("$nix_bin" eval --raw ".#checks.${system}.${check_name}.evidence")
evidence_lookup_status=$?
evidence_was_realized=false
if [ "$evidence_lookup_status" -eq 0 ] && [ -d "$evidence_path" ]; then
  evidence_was_realized=true
fi
lookup_ended_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
lookup_duration_ms=$(($(date +%s%3N) - lookup_started_ms))

gate_started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gate_started_ms=$(date +%s%3N)
"$nix_bin" build ".#checks.${system}.${check_name}" -L --show-trace \
  --out-link "$work_dir/gate" 2> >(tee "$check_dir/nix-realization.log" >&2)
gate_status=$?
gate_ended_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gate_duration_ms=$(($(date +%s%3N) - gate_started_ms))

if [ "$evidence_lookup_status" -eq 0 ] && [ ! -d "$evidence_path" ]; then
  evidence_lookup_status=1
fi

saw_build=false
saw_substitution=false
while IFS= read -r line; do
  case "$line" in
    *"building '"*".drv'"*) saw_build=true ;;
    *"copying path '"*" from '"*) saw_substitution=true ;;
  esac
done <"$check_dir/nix-realization.log"
if [ "$evidence_was_realized" = true ]; then
  cache_state=local-hit
elif [ "$saw_build" = true ] && [ "$saw_substitution" = true ]; then
  cache_state=mixed-build-and-substitution
elif [ "$saw_build" = true ]; then
  cache_state=built
elif [ "$saw_substitution" = true ]; then
  cache_state=substituted
elif [ "$evidence_lookup_status" -eq 0 ]; then
  cache_state=realized-during-job
else
  cache_state=unavailable
fi

copy_started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
copy_started_ms=$(date +%s%3N)
evidence_copy_status=0
if [ "$evidence_lookup_status" -eq 0 ]; then
  "$copy_bin" -RL "$evidence_path"/. "$check_dir"/
  evidence_copy_status=$?
else
  evidence_copy_status=125
fi
copy_ended_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
copy_duration_ms=$(($(date +%s%3N) - copy_started_ms))

verdict_status=2
if [ "$evidence_copy_status" -eq 0 ]; then
  "$verdict_checker" "$check_dir/screenshots/check-verdict.json" >/dev/null
  verdict_status=$?
fi

if [ "$evidence_lookup_status" -ne 0 ]; then
  producer_status=infrastructure-evidence-lookup-failure
elif [ "$evidence_copy_status" -ne 0 ]; then
  producer_status=infrastructure-evidence-copy-failure
elif [ "$verdict_status" -eq 2 ]; then
  producer_status=infrastructure-invalid-verdict
elif [ "$verdict_status" -eq 1 ] && [ "$blocking" = false ]; then
  producer_status=advisory-failed
elif [ "$gate_status" -ne 0 ] || [ "$verdict_status" -eq 1 ]; then
  producer_status=failed
else
  producer_status=passed
fi

producer_ended_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
producer_duration_ms=$(($(date +%s%3N) - producer_started_ms))
producer_tmp="$work_dir/producer.json"
cat >"$producer_tmp" <<EOF
{
  "schemaVersion": 1,
  "check": "$check_name",
  "blocking": $blocking,
  "status": "$producer_status",
  "gateStatus": $gate_status,
  "evidenceLookupStatus": $evidence_lookup_status,
  "evidenceCopyStatus": $evidence_copy_status,
  "verdictStatus": $verdict_status,
  "jobUrl": "${CI_JOB_URL:-}",
  "pipelineCreatedAt": "$pipeline_created_at",
  "jobStartedAt": "$job_started_at",
  "queueDurationMilliseconds": $queue_duration_ms,
  "queueDurationSource": "$queue_duration_source",
  "cacheState": "$cache_state",
  "startedAt": "$producer_started_at",
  "endedAt": "$producer_ended_at",
  "durationMilliseconds": $producer_duration_ms,
  "phases": {
    "gateBuild": {
      "startedAt": "$gate_started_at",
      "endedAt": "$gate_ended_at",
      "durationMilliseconds": $gate_duration_ms,
      "status": $gate_status
    },
    "evidenceLookup": {
      "startedAt": "$lookup_started_at",
      "endedAt": "$lookup_ended_at",
      "durationMilliseconds": $lookup_duration_ms,
      "status": $evidence_lookup_status
    },
    "evidenceCopy": {
      "startedAt": "$copy_started_at",
      "endedAt": "$copy_ended_at",
      "durationMilliseconds": $copy_duration_ms,
      "status": $evidence_copy_status
    }
  }
}
EOF
cp "$producer_tmp" "$check_dir/producer.json"

printf '%s: gate=%s lookup=%s copy=%s verdict=%s status=%s\n' \
  "$check_name" "$gate_status" "$evidence_lookup_status" \
  "$evidence_copy_status" "$verdict_status" "$producer_status"

if [ "$gate_status" -ne 0 ]; then
  exit "$gate_status"
fi
if [ "$evidence_lookup_status" -ne 0 ] || [ "$evidence_copy_status" -ne 0 ] || [ "$verdict_status" -eq 2 ]; then
  printf '%s gate passed, but complete valid evidence could not be retained\n' "$check_name" >&2
  exit 70
fi
if [ "$blocking" = true ] && [ "$verdict_status" -ne 0 ]; then
  exit 1
fi
exit 0
