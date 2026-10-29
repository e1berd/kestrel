import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import urllib.request


def main():
    parser = argparse.ArgumentParser(description="Verify or restore pinned public import fixtures")
    parser.add_argument("--download", action="store_true", help="download the pinned revision and verify its hashes")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    fixtures = root / "crates/volley-import/tests/fixtures/upstream/oai"
    manifest = json.loads((fixtures / "manifest.json").read_text())
    repository = manifest["repository"].removeprefix("https://github.com/")
    revision = manifest["revision"]

    def verify(entry):
        path = fixtures / entry["path"]
        if arguments.download:
            source = entry["path"] if entry["path"] == "LICENSE" else "examples/" + entry["path"]
            url = f"https://raw.githubusercontent.com/{repository}/{revision}/{source}"
            with urllib.request.urlopen(url, timeout=30) as response:
                data = response.read()
        else:
            data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if digest != entry["sha256"]:
            raise ValueError(f"{path}: expected {entry['sha256']}, got {digest}")
        if arguments.download:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)

    with ThreadPoolExecutor(max_workers=4) as executor:
        list(executor.map(verify, manifest["files"]))
    print(f"Verified {len(manifest['files'])} files from {repository}@{revision}")


if __name__ == "__main__":
    main()
