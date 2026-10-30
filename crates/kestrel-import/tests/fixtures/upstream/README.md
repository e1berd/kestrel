# Public import fixtures

The `oai` directory contains unmodified JSON and YAML examples from the [OpenAPI Initiative learning site](https://github.com/OAI/learn.openapis.org/tree/82b82f372aa185a07ea11bc158619895ab21f741/examples), pinned to commit `82b82f372aa185a07ea11bc158619895ab21f741`, downloaded on 2026-10-29. The OpenAPI Initiative publishes these materials under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); the upstream license is retained in `oai/LICENSE`. No endorsement by the OpenAPI Initiative is implied.

`oai/manifest.json` records source revision, SHA-256 hashes, document versions, titles, expected request counts and expected rejection cases. The schema files retain their original bytes, including upstream comments and differences between JSON and YAML examples.

| Source family | JSON | YAML | Coverage |
|---|---:|---:|---|
| Swagger 2.0 | 8 | 8 | Petstore variants, Uber, examples, external references and `allOf` |
| OpenAPI 3.0 | 6 | 6 | Petstore, USPTO, response examples, callbacks and links |
| OpenAPI 3.1 | 3 | 3 | Tic Tac Toe, security scopes and webhook-only documents |
| OpenAPI 3.2 | 2 | 2 | HTTP QUERY with a body and tag metadata |

There are 38 documents: 36 importable and two webhook-only documents. The latter are intentionally rejected because they contain no outgoing HTTP operations. Eight additional files supply the external references used by the Swagger examples.

The examples declare OpenAPI 3.0.0/3.0.1, 3.1.0 and 3.2.0. Patch-version acceptance for 3.0.4, 3.1.2 and 3.2.1 is covered separately in the importer tests; those versions have not been substituted into the upstream documents.

No publicly downloadable native Apidog project export was found. `../apidog.json` is a constructed fixture following the native folder normalization and HTTP endpoint contract from the official [apidog-cli 2.2.12 package](https://www.npmjs.com/package/apidog-cli/v/2.2.12), rather than a downloaded project export. Tests cover both wrapped `apiCollection` 1.0.0 and flat `httpCollection` 1.1.0 representations. A real, redistributable Apidog export would provide additional coverage.

Run the offline checks:

```bash
python3 scripts/import_fixtures.py
cargo test -p kestrel-cli --test import_e2e
```

Restore the exact pinned downloads:

```bash
python3 scripts/import_fixtures.py --download
```

The e2e suite launches `kestrelcli` and checks automatic/explicit format selection, both grouping modes, collection loading, byte-preserving saves, repeated synchronization after user edits, destination protection, credential replacement, and warnings about skipped native data. A loopback HTTP server verifies remote imports with external references, failed downloads, and actual execution of imported Swagger, OpenAPI 3.0/3.1/3.2 and Apidog requests. Tests never call the example APIs on the internet. This suite exercises the shared import backend and CLI; it does not automate the desktop file picker.
