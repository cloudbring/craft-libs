# craft-corpus

Pinned test corpora for the Craft apps: a TOML manifest, an HTTPS-only fetcher that checks every
sha256, a stamped tree that `verify` can trust, requirement gates for CI, ratcheting floors and a
cache key. Lifts photocraft's `xtask/src/pinned.rs`, craft-crw's `samples.json` and vectorcraft's
affinity fetcher into one place. Manifest schema: see the docs on `craft_corpus::manifest`.
Example: `manifests/pngsuite.toml`.

```text
craft-corpus fetch manifests/pngsuite.toml corpus/pngsuite
craft-corpus verify corpus/pngsuite --manifest manifests/pngsuite.toml
craft-corpus cache-key manifests/pngsuite.toml      # CI: key: corpus-pngsuite-${{ steps.k.outputs.key }}
craft-corpus floors check corpus-floors.toml --count pngsuite.files=62
craft-corpus floors raise corpus-floors.toml --count pngsuite.files=70
```

In a test: `let corpus = craft_corpus::corpus_or_skip!("pngsuite", "corpus/pngsuite");` prints
`SKIP corpus ...` when absent. `CRAFT_CORPUS_REQUIRE=1` or `PNGSUITE_CORPUS_REQUIRED=1` turns
absence into a failure. A tree that exists but fails verification is always a failure.
