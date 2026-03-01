# SQLx Prepare Required

This branch adds new SQLx queries that need to be added to the query cache.

## To prepare the queries:

1. Start the database:
   ```bash
   cd packages/default
   nix develop -c db-only up
   ```

2. Wait for database to be ready (about 20 seconds)

3. Run SQLx prepare:
   ```bash
   cd packages/default
   nix develop -c cargo sqlx prepare
   ```

4. Commit the updated `.sqlx` directory:
   ```bash
   git add .sqlx/
   git commit -m "chore: update SQLx query cache for build_jobs queries"
   ```

## New queries added:
- `queries/build_jobs.rs::create_build_jobs_for_commit` 
- `queries/build_jobs.rs::get_next_job_for_builder`
- `queries/build_jobs.rs::mark_job_success`
- `queries/build_jobs.rs::mark_job_failed`
