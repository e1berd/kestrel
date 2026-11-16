import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent


def version():
    value = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?', value):
        raise ValueError(f'Unsupported release version: {value}')
    return value


def release(tag):
    repository = os.environ['GITHUB_REPOSITORY']
    request = Request(f'https://api.github.com/repos/{repository}/releases/tags/{tag}', headers={
        'Authorization': f'Bearer {os.environ["GH_TOKEN"]}',
        'Accept': 'application/vnd.github+json',
        'User-Agent': 'kestrel-release',
    })
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)
    except HTTPError as error:
        if error.code == 404:
            return None
        raise


def prepare():
    current = version()
    tag = f'v{current}'
    reference = os.environ['GITHUB_REF']
    if reference.startswith('refs/tags/') and reference != f'refs/tags/{tag}':
        raise ValueError('The tag must match the Cargo.toml version')
    existing = release(tag)
    publish = os.environ.get('RELEASE_PUBLISH', 'true') == 'true' and (existing is None or existing['draft'])
    with Path(os.environ['GITHUB_OUTPUT']).open('a', encoding='utf-8') as output:
        output.write(f'version={current}\ntag={tag}\npublish={str(publish).lower()}\n')
    print(f'Kestrel {current}: publish={publish}')


def validate(directory, current):
    directory = Path(directory)
    packages = [
        f'kestrel_{current}_amd64.deb',
        f'Kestrel-{current}-x86_64.AppImage',
        *[f'Kestrel-{current}-{architecture}.dmg' for architecture in ('aarch64', 'x86_64', 'universal')],
        f'Kestrel-{current}-x86_64-windows.zip',
        f'Kestrel-{current}-x86_64-setup.exe',
    ]
    expected = set(packages) | {f'{package}.sha256' for package in packages}
    actual = {path.name for path in directory.iterdir()}
    if actual != expected:
        raise ValueError(f'Unexpected release files; missing={sorted(expected - actual)}, extra={sorted(actual - expected)}')
    for name in packages:
        artifact = directory / name
        if not artifact.stat().st_size:
            raise ValueError(f'Empty release package: {name}')
        with artifact.open('rb') as source:
            digest = hashlib.file_digest(source, 'sha256').hexdigest()
        if (directory / f'{name}.sha256').read_text().strip() != f'{digest}  {name}':
            raise ValueError(f'SHA256 mismatch: {name}')
    return sorted(directory.iterdir())


def publish(directory):
    current = version()
    tag = f'v{current}'
    artifacts = validate(directory, current)
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    existing = release(tag)
    if existing is not None and (not existing['draft'] or existing['target_commitish'] != commit):
        raise ValueError('Refusing to overwrite a published release or a draft for another commit')
    notes = ROOT / 'packaging' / 'releases' / f'{current}.md'
    if not notes.is_file():
        raise ValueError(f'Missing release notes: {notes}')
    if existing is None:
        subprocess.run(['gh', 'release', 'create', tag, '--draft', '--target', commit,
                        '--title', f'Kestrel {current}', '--notes-file', str(notes)], check=True)
    subprocess.run(['gh', 'release', 'upload', tag, *map(str, artifacts), '--clobber'], check=True)
    subprocess.run(['gh', 'release', 'edit', tag, '--draft=false', '--latest'], check=True)


if __name__ == '__main__':
    match sys.argv[1:]:
        case ['prepare']:
            prepare()
        case ['validate', directory]:
            validate(directory, version())
        case ['publish', directory]:
            publish(directory)
        case _:
            raise SystemExit('Usage: release.py prepare|validate DIRECTORY|publish DIRECTORY')
