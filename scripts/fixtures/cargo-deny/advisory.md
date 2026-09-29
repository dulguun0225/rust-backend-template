```toml
[advisory]
id = "RUSTSEC-2026-9999"
package = "itoa"
date = "2026-09-29"

[versions]
patched = []
```

# Canary advisory

A fixture advisory against every version of itoa, served from a local advisory database, so the advisories
check is seen to fail. It exists only in scripts/cargo-deny.mjs's canary run.
