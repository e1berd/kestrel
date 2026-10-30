import re
import shutil
import subprocess
import sys
from pathlib import Path


BINARIES = ('kestrel', 'kestrelcli', 'kestrel-mcp')
HOST_LIBRARIES = re.compile(r'^(ld-linux.*|lib(c|m|pthread|dl|rt|resolv|util|gcc_s)\.so(?:\..*)?)$')
GRAPHICS_LIBRARIES = re.compile(r'^lib(wayland-[^.]+|xcb(?:-[^.]+)?|xkbcommon(?:-[^.]+)?|X11|Xau|Xdmcp)\.so(?:\..*)?$')
DYNAMIC_LIBRARIES = ('libfontconfig.so.1', 'libfreetype.so.6')


def run(*arguments, **kwargs):
    return subprocess.check_output(arguments, text=True, **kwargs)


def copy(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def install_payload(root, binaries, destination):
    for name in BINARIES:
        copy(binaries / name, destination / 'usr/bin' / name)
    copy(root / 'packaging/linux/dev.kestrel.app.desktop', destination / 'usr/share/applications/dev.kestrel.app.desktop')
    for size in (16, 24, 32, 48, 64, 128, 256, 512):
        copy(root / f'packaging/linux/icons/kestrel-{size}.png',
             destination / f'usr/share/icons/hicolor/{size}x{size}/apps/dev.kestrel.app.png')
    copy(root / 'crates/kestrel-app/assets/brand/kestrel.svg',
         destination / 'usr/share/icons/hicolor/scalable/apps/dev.kestrel.app.svg')
    for name in ('Inter', 'JetBrainsMono'):
        copy(root / f'crates/kestrel-app/assets/fonts/{name}-LICENSE.txt',
             destination / f'usr/share/doc/kestrel/{name}-LICENSE.txt')
    for name in ('LICENSE-MIT', 'LICENSE-APACHE'):
        copy(root / name, destination / 'usr/share/doc/kestrel' / name)


def build_deb(root, binaries, staging, output, version):
    destination = staging / 'debian/kestrel'
    install_payload(root, binaries, destination)
    debian = staging / 'debian'
    debian.mkdir(exist_ok=True)
    (debian / 'control').write_text('Source: kestrel\nSection: devel\nPriority: optional\nMaintainer: Kestrel contributors <kestrel@users.noreply.github.com>\n\nPackage: kestrel\nArchitecture: any\nDescription: File-based API client\n')
    control = destination / 'DEBIAN/control'
    control.parent.mkdir()
    dependencies = run('dpkg-shlibdeps', '-O',
                       *(f'-e{destination / "usr/bin" / name}' for name in BINARIES), cwd=staging).strip()
    dependencies = dependencies.removeprefix('shlibs:Depends=')
    if not dependencies:
        raise RuntimeError('dpkg-shlibdeps returned no runtime dependencies')
    dependencies += ', ca-certificates, libfontconfig1, libfreetype6, libwayland-client0, libwayland-cursor0, libvulkan1'
    installed_size = sum(path.stat().st_size for path in destination.rglob('*') if path.is_file()) // 1024
    control.write_text(f'Package: kestrel\nVersion: {version}\nArchitecture: amd64\n'
                       'Maintainer: Kestrel contributors <kestrel@users.noreply.github.com>\n'
                       'Section: devel\nPriority: optional\nHomepage: https://github.com/e1berd/kestrel\n'
                       f'Installed-Size: {installed_size}\nDepends: {dependencies}\n'
                       'Recommends: mesa-vulkan-drivers\n'
                       'Description: Free, file-based API client\n'
                       ' Desktop API client, collection runner and MCP server compatible with\n'
                       ' Bruno 3 OpenCollection YAML. Collections stay as ordinary files in git.\n')
    subprocess.run(('dpkg-deb', '--root-owner-group', '--build', destination,
                    output / f'kestrel_{version}_amd64.deb'), check=True)


def linked_libraries(executable):
    libraries = []
    for line in run('ldd', str(executable)).splitlines():
        if 'not found' in line:
            raise RuntimeError(f'Unresolved library: {line.strip()}')
        match = re.search(r'=> (/\S+)', line)
        if match:
            libraries.append(Path(match[1]))
    return libraries


def install_library_license(library, destination):
    owners = run('dpkg-query', '-S', f'*/{library.resolve().name}')
    for line in owners.splitlines():
        package = line.split(': ', 1)[0].split(':', 1)[0]
        copyright_file = Path('/usr/share/doc') / package / 'copyright'
        if not copyright_file.is_file():
            raise RuntimeError(f'Library license not found: {package}')
        copy(copyright_file, destination / 'usr/share/doc/kestrel/bundled' / package / 'copyright')


def build_appdir(root, binaries, staging):
    destination = staging / 'Kestrel.AppDir'
    install_payload(root, binaries, destination)
    copy(root / 'packaging/linux/AppRun', destination / 'AppRun')
    (destination / 'AppRun').chmod(0o755)
    copy(root / 'packaging/linux/dev.kestrel.app.desktop', destination / 'dev.kestrel.app.desktop')
    copy(root / 'packaging/linux/icons/kestrel-256.png', destination / 'dev.kestrel.app.png')
    (destination / '.DirIcon').symlink_to('dev.kestrel.app.png')
    cache = run('ldconfig', '-p')
    libraries = []
    for name in DYNAMIC_LIBRARIES:
        match = re.search(rf'\s{re.escape(name)} .* => (/\S+)', cache)
        if not match:
            raise RuntimeError(f'Required dynamically loaded library not found: {name}')
        libraries.append(Path(match[1]))
    for name in BINARIES:
        libraries.extend(linked_libraries(binaries / name))
    copied = set()
    while libraries:
        library = libraries.pop()
        if HOST_LIBRARIES.match(library.name) or GRAPHICS_LIBRARIES.match(library.name) or library.name in copied:
            continue
        copied.add(library.name)
        libraries.extend(linked_libraries(library))
        bundled = destination / 'usr/lib' / library.name
        copy(library, bundled)
        install_library_license(library, destination)
        subprocess.run(('patchelf', '--set-rpath', '$ORIGIN', bundled), check=True)
    for name in BINARIES:
        subprocess.run(('patchelf', '--set-rpath', '$ORIGIN/../lib', destination / 'usr/bin' / name), check=True)
    shutil.copytree('/usr/share/common-licenses', destination / 'usr/share/common-licenses')


def main():
    root, binaries, staging, output = map(Path, sys.argv[1:5])
    version, package_format = sys.argv[5:]
    if package_format in ('all', 'deb'):
        build_deb(root, binaries, staging, output, version)
    if package_format in ('all', 'appimage'):
        build_appdir(root, binaries, staging)


if __name__ == '__main__':
    main()
