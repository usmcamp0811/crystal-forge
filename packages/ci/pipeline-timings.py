"""Summarizes GitLab pipeline timing from API JSON (TASK-542.1).

Usage:
  glab api "projects/<id>/pipelines/<pipeline>" > pipeline.json
  glab api "projects/<id>/pipelines/<pipeline>/jobs?per_page=100&include_retried=true" > jobs.json
  [glab api "projects/<id>/pipelines/<child>/jobs?per_page=100&include_retried=true" > child.json]
  python3 packages/ci/pipeline-timings.py pipeline.json jobs.json [child.json ...]

Reports, in minutes:
  wall           pipeline created -> last job finished (what a developer waits)
  first_start    pipeline created -> first job started (runner queue at start)
  first_result   pipeline created -> first job finished successfully
  queue_max/sum  per-job queued_duration (time waiting for a runner)
  run_sum        sum of job durations (runner time consumed)
  critical       the job that finished last, its queue and run time
Earlier attempts of a retried job are listed; only the final attempt
(highest job ID per name) enters the sums.
The tool reads only the given files; it makes no network request.
"""

from datetime import datetime
import json
import sys


def parse(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00")) if value else None


def minutes(seconds):
    return round(seconds / 60, 1)


def main(paths):
    pipeline = json.load(open(paths[0]))
    jobs = [job for path in paths[1:] for job in json.load(open(path))]
    created = parse(pipeline["created_at"])
    # The API does not always mark retried attempts; the final attempt of a
    # job name is the one with the highest job ID.
    latest = {}
    for job in jobs:
        if job["name"] not in latest or job["id"] > latest[job["name"]]["id"]:
            latest[job["name"]] = job
    final = list(latest.values())
    final_ids = {job["id"] for job in final}
    started = [parse(job["started_at"]) for job in final if job.get("started_at")]
    finished = [(parse(job["finished_at"]), job) for job in final if job.get("finished_at")]
    successes = [time for time, job in finished if job["status"] == "success"]
    if not started or not finished:
        print(json.dumps({"pipeline": pipeline["id"], "status": pipeline["status"],
                          "error": "no finished jobs"}))
        return 1
    last_time, last_job = max(finished, key=lambda item: item[0])
    report = {
        "pipeline": pipeline["id"],
        "sha": pipeline["sha"][:8],
        "status": pipeline["status"],
        "jobs": len(final),
        "retried_attempts": len(jobs) - len(final),
        "wall": minutes((last_time - created).total_seconds()),
        "first_start": minutes((min(started) - created).total_seconds()),
        "first_result": minutes((min(successes) - created).total_seconds()) if successes else None,
        "queue_max": minutes(max(job.get("queued_duration") or 0 for job in final)),
        "queue_sum": minutes(sum(job.get("queued_duration") or 0 for job in final)),
        "run_sum": minutes(sum(job.get("duration") or 0 for job in final)),
        "critical": {
            "job": last_job["name"],
            "queue": minutes(last_job.get("queued_duration") or 0),
            "run": minutes(last_job.get("duration") or 0),
        },
        "per_job": sorted(
            ({"job": job["name"], "status": job["status"],
              "queue": minutes(job.get("queued_duration") or 0),
              "run": minutes(job.get("duration") or 0),
              "final_attempt": job["id"] in final_ids} for job in jobs),
            key=lambda item: -item["run"]),
    }
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
