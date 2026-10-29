import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time


def generate(path, source_format, count, padding):
    payload = "x" * padding
    with path.open("w") as output:
        if source_format == "apidog":
            output.write('{"apidogProject":"1.0.0","info":{"name":"Large transfer"},"apiCollection":[{"name":"Requests","items":[')
        elif source_format == "yaml":
            output.write("openapi: 3.2.1\ninfo:\n  title: Large transfer\n  version: 1.0.0\npaths:\n")
        else:
            output.write('{"openapi":"3.2.1","info":{"title":"Large transfer","version":"1.0.0"},"paths":{')
        for index in range(count):
            body = {"index": index, "payload": payload}
            if source_format == "apidog":
                if index:
                    output.write(",")
                json.dump({"name": f"Request {index}", "api": {"method": "post", "path": f"/items/{index}",
                    "requestBody": {"type": "application/json", "data": json.dumps(body)}}}, output)
            elif source_format == "yaml":
                output.write(f"  /items/{index}:\n    post:\n      summary: Request {index}\n      requestBody:\n        content:\n          application/json:\n            example: ")
                json.dump(body, output)
                output.write("\n      responses:\n        '200':\n          description: OK\n")
            else:
                if index:
                    output.write(",")
                json.dump(f"/items/{index}", output)
                output.write(":")
                json.dump({"post": {"summary": f"Request {index}", "requestBody": {"content": {"application/json": {"example": body}}},
                    "responses": {"200": {"description": "OK"}}}}, output)
        if source_format == "apidog":
            output.write("]}]}")
        elif source_format != "yaml":
            output.write("}}")


def run(binary, arguments, root, label, limit, count):
    measurement = root / f"{label}.rss"
    command = [str(binary), *map(str, arguments)]
    if Path("/usr/bin/time").exists():
        command = ["/usr/bin/time", "-f", "%M", "-o", str(measurement), *command]
    started = time.monotonic()
    result = subprocess.run(command, capture_output=True, text=True, check=True)
    rss = int(measurement.read_text().strip()) / 1024 if measurement.exists() else None
    print(f"{label}: {time.monotonic() - started:.2f}s; peak RSS {rss:.1f} MiB" if rss is not None else f"{label}: finished")
    print(result.stdout.strip())
    if f"{count} requests" not in result.stdout:
        raise RuntimeError(f"{label} did not preserve all {count} requests")
    if rss is not None and rss > limit:
        raise RuntimeError(f"{label} used {rss:.1f} MiB; the configured limit is {limit} MiB")


def main():
    parser = argparse.ArgumentParser(description="Generate large schemas and measure import/export/reimport memory")
    parser.add_argument("--size-mib", type=int, default=128)
    parser.add_argument("--payload-bytes", type=int, default=32768)
    parser.add_argument("--source-format", choices=["json", "yaml", "apidog"], default="json")
    parser.add_argument("--max-rss-mib", type=int, default=256)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/kestrel-cli"))
    arguments = parser.parse_args()
    if arguments.size_mib < 1 or arguments.payload_bytes < 1:
        parser.error("sizes must be positive")
    binary = arguments.binary.resolve()
    with tempfile.TemporaryDirectory(prefix="kestrel-large-transfer-") as temporary:
        root = Path(temporary)
        source = root / "source"
        count = max(1, arguments.size_mib * 1024 * 1024 // arguments.payload_bytes)
        generate(source, arguments.source_format, count, arguments.payload_bytes)
        print(f"Source: {source.stat().st_size / 1024 / 1024:.1f} MiB; {count} requests; {arguments.source_format}")
        run(binary, ["import", source, "--out", root / "collection"], root, "import", arguments.max_rss_mib, count)
        export_format = "apidog" if arguments.source_format == "apidog" else "openapi32"
        encoding = "yaml" if arguments.source_format == "yaml" else "json"
        run(binary, ["export", root / "collection", "--out", root / "export", "--format", export_format, "--encoding", encoding], root, "export", arguments.max_rss_mib, count)
        run(binary, ["import", root / "export", "--out", root / "restored"], root, "reimport", arguments.max_rss_mib, count)


if __name__ == "__main__":
    main()
