import os

import pytest

pytestmark = [pytest.mark.s3cache]


def test_cache_push_on_build_complete(
    completed_derivation_data, cfServer, s3Cache, cf_client
):
    """
    When a derivation is build-complete, verify cache push to MinIO.
    Success criterion: any .narinfo object appears in the S3 bucket.
    """
    pkg_name = completed_derivation_data["pname"]
    pkg_version = completed_derivation_data["version"]
    drv_path = completed_derivation_data["derivation_path"]
    deriv_id = completed_derivation_data["derivation_id"]

    cfServer.log(
        f"Testing cache push for derivation_id={deriv_id}, drv={drv_path}, "
        f"pkg={pkg_name}-{pkg_version}"
    )

    # Verify derivation is build-complete (status_id=10)
    status_row = cf_client.execute_sql(
        "SELECT status_id FROM derivations WHERE id = %s",
        (deriv_id,),
    )
    assert status_row, "Derivation row not found after fixture insert"
    assert (
        status_row[0]["status_id"] == 10
    ), "Derivation is not build-complete (status_id != 10)"

    # Poll for .narinfo files - proves cache push worked
    poll_script = r"""
set -euo pipefail
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin

deadline=$((SECONDS + 180))

while (( SECONDS < deadline )); do
  if aws s3 ls --recursive s3://crystal-forge-cache/ 2>/dev/null | grep -E "\.narinfo$" >/dev/null; then
    echo "FOUND"
    exit 0
  fi
  sleep 5
done

exit 1
"""

    try:
        cfServer.succeed(poll_script)
        cfServer.log("Cache push detected: .narinfo present in crystal-forge-cache")
    except Exception:
        cfServer.log(
            "Cache push not detected within timeout. Collecting diagnostics..."
        )

        # Show builder logs
        try:
            logs = cfServer.succeed(
                "journalctl -u crystal-forge-builder.service --no-pager -n 200 || true"
            )
            cfServer.log("---- builder logs ----\n" + logs)
        except Exception:
            pass

        # Show S3 bucket contents
        try:
            listing = cfServer.succeed(
                r"""
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
aws s3 ls --recursive s3://crystal-forge-cache/ || true
"""
            )
            cfServer.log("---- S3 bucket listing ----\n" + listing)
        except Exception:
            pass

        # Show DB state
        try:
            row = cf_client.execute_sql(
                """
                SELECT id, derivation_name, derivation_path, status_id, completed_at
                FROM derivations WHERE id = %s
                """,
                (deriv_id,),
            )
            cfServer.log(f"---- DB row for derivation_id={deriv_id} ----\n{row}")
        except Exception:
            pass

        assert False, (
            f"Did not find any .narinfo files in MinIO within timeout. "
            "See logs above for details."
        )
