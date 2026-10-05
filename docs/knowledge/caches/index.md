# Operator Guide

* [Crystal Forge — S3 Cache (MinIO) Quickstart](s3-minio-cache-quickstart.md) - Shows how to push Nix store paths to a MinIO-backed S3 cache and use it as a substituter, including path-style addressing, AWS environment variables, the NixOS cache configuration snippet, and troubleshooting.
* [Niks3 Cache Operator Guide](niks3-cache.md) - Describes separate read/write authentication, publication-backed deployment, retained credential tests, proxy configuration, and verification limits.

# Workflow

* [Cache Push Process](cache-push-process.md) - Describes how a build output reaches the binary cache: builder-side signing and push with retry, the server's destination check and nix path-info probe, the cache_push_jobs record that makes an artifact deployable, the cache types, and the server-side worker that the server does not start; open it when configuring or debugging pushes to a binary cache.
