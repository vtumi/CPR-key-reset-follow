# v0.1.5 — Quota observation freshness fixes

## Fixes

- Reject older quota observations before establishing a new or resumed association baseline, preventing a stale resume response from rolling back a completed cycle and triggering a duplicate reset.
- Check account observation freshness before recording changed-window pending state, so stale responses cannot leave associations blocked while their weekly window is settling.
- Ignore observations at or before an association baseline when advancing settling state, including after resume.

## Preserved behavior

- Equal-time explicit resume and recovery from a newer corrected window remain supported; establishing a baseline does not consume the shared account observation needed by other associations.
- Unknown reset results still stop automatic retries, and pause/resume cannot clear that protection. Per-key early-reset settings and persisted state schema are unchanged.

## Compatibility

- Official Codex Proxy RS **>=3.29.7 and <3.30.0**, using RPC v5, Management v2, and Maintenance v1.
- SDK and packaging CLI remain pinned to official v3.29.7 commit `78bc57c5cf24b01a38e985cdd739bb4ae89ce11f`.
- Linux x86_64 and ARM64 packages are produced by the tagged release workflow, with matching SHA-256 files. Install the archive matching your host architecture.

## Verification

Five added tests cover stale resume, stale changed-window pending state, missing account history, settling after resume, and preserved recovery behavior. Local verification covers all 29 Rust tests, formatting, strict all-target Clippy, release compilation, frontend lint, all 6 frontend tests, and the production frontend build. The tagged GitHub Actions workflow additionally builds and packages both Linux architectures.

Real-host installation, upgrade, and state-preservation integration have **not** been revalidated for this release; local checks and CI do not replace that verification. No production host deployment is performed by this release.
