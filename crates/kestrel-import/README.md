# Collection transfers

`PreparedImport::load_with_progress` detects and indexes JSON/YAML without retaining the complete source document. YAML is converted by an event parser into a temporary JSON stream; paths, named schemas, tags and native Apidog items are indexed in temporary files. References are loaded for the current path, and converted requests are spooled to disk. The preview retains counts, names and conversion warnings.

`save_with_progress` returns the saved collection path. It reads one prepared request at a time and writes through `Document::save` into a temporary sibling directory, publishing the collection after writing succeeds. The convenience `save` method additionally opens the entire resulting `Collection` in memory. CLI imports use the path-returning method; the desktop opens the completed collection in the background with separate file progress.

`export::export` indexes saved request paths and inherited defaults, then serializes requests directly into a buffered output file. It does not build a complete OpenAPI or Apidog document in memory. The temporary output is flushed and synchronized before publication. Cancellation or errors remove unfinished output and preserve any previous destination file.

Working memory depends on the path index, metadata, the largest path/request and its referenced schema graph. A single exceptionally large example or schema can still require substantial memory. Temporary disk space is needed for indexing and converted requests, and for YAML's normalized stream. The desktop's opened collection retains its normal application state in memory; the CLI measurements below describe the transfer pipeline. The import dialog offers **Open collection after import**. It defaults to off above 5,000 requests or 128 MiB of prepared request data, allowing large imports to finish on disk without loading them into the workspace.

## Formats

Imports accept OpenAPI 3.0/3.1/3.2 and Swagger 2.0 in JSON/YAML, plus Apidog 1.0.0/1.1.0 JSON exports. Local and HTTP(S) JSON Pointer references are supported. YAML aliases and Unicode are supported; merge keys, alias mapping keys, recursive aliases, schema anchors and `$id`-based resolution produce an error or require expansion before importing. Webhooks, native environments and incompatible processors are reported as skipped.

Exports offer `openapi30` (3.0.4), `openapi31` (3.1.2), `openapi32` (3.2.1), `swagger20` (2.0), and `apidog` (1.0.0). OpenAPI/Swagger support JSON and YAML; Apidog uses JSON. HTTP QUERY and custom methods require OpenAPI 3.2 or Apidog. Duplicate method/path pairs require Apidog because OpenAPI/Swagger cannot represent them. Swagger requires a single public base URL. Native folders, including empty folders and descriptions, are retained in Apidog. OpenAPI organizes requests through tags.

Authentication credentials become process environment placeholders. Secret-marked collection variables are also replaced with placeholders; ordinary collection variables are retained in `x-kestrel-variables`. Environments, scripts, tests and tool-specific runtime configuration require manual migration. Export uses saved files, so save application edits first.

## Progress

`kestrel_core::transfer::Progress` is shared by the worker, desktop and CLI. Each snapshot names the current stage and reports actual consumed bytes, indexed paths, or completed requests/files. Output bytes are counted after writes. Known totals have a determinate progress bar; unknown totals remain indeterminate. There are no time-based percentage estimates. Completion is reported after publication, and incomplete known stages cannot be marked finished.

The desktop refreshes progress from background workers and cancels active work when its dialog closes. CLI terminals display a progress bar on stderr; piped output remains suitable for scripts. Collection opening after import is a separate measured stage. Cancellation during that stage leaves the already published collection on disk.

## Extending formats

`Format::ALL` supplies the import picker; `ExportFormat::ALL` supplies the export picker. Add format metadata/detection, input indexing/conversion and a streaming serializer in separate modules, then extend the CLI values. Keep reference lookup and conversion bounded to the current request, and retain the temporary-file publication rules.

## Verification

The [public fixture corpus](tests/fixtures/upstream/README.md) contains 38 pinned documents from the OpenAPI Initiative. Native Apidog fixtures follow the official CLI contract; they are constructed fixtures, not downloaded project exports. Tests exercise import/export/reimport, synchronization, byte preservation, actual loopback HTTP requests, streamed byte counters, cancellation during writes, Unicode, YAML aliases, XML and multipart fields.

```bash
cargo test -p kestrel-import -p kestrel-cli --tests
cargo build -p kestrel-cli
python3 scripts/benchmark_transfers.py --size-mib 512 --source-format json
python3 scripts/benchmark_transfers.py --size-mib 128 --source-format yaml
python3 scripts/benchmark_transfers.py --size-mib 128 --source-format apidog
```

The generator checks request counts after import, export and reimport. On Linux it measures each process with `/usr/bin/time` and enforces a configurable peak RSS ceiling (`--max-rss-mib`, default 256). Generated data and output are removed after the run.

A development-build run on 2026-10-29 produced these peak RSS measurements; these are workload-specific measurements rather than universal memory limits:

| Source | Size | Requests | Import | Export | Reimport |
|---|---:|---:|---:|---:|---:|
| OpenAPI JSON | 515 MiB | 16,384 | 10.5 MiB | 24.7 MiB | 13.9 MiB |
| OpenAPI YAML | 129 MiB | 4,096 | 8.0 MiB | 8.5 MiB | 8.3 MiB |
| Apidog JSON | 129 MiB | 4,096 | 8.4 MiB | 11.7 MiB | 6.4 MiB |
