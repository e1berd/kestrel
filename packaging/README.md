# Packages

All packages contain `kestrel`, `kestrelcli` and `kestrel-mcp`, application icons and license notices. Versions come from the Cargo workspace. Artifacts and individual SHA256 files go to `dist/` (override with `KESTREL_DIST_DIR`). The source checkout and collections are not modified by packaging.

## Linux

On an x86_64 Linux host with Docker:

```bash
bash packaging/linux/container.sh
```

Use `deb` or `appimage` as an argument to build only that format. The builder uses Ubuntu 22.04 (glibc 2.35) and installs Rust stable with a minimal profile; Cargo uses `--locked`. Build caches live in `target/packaging/`. Rebuild the image with `docker build --pull --no-cache -t kestrel-packaging:ubuntu22.04 -f packaging/linux/Dockerfile packaging/linux` to update its toolchain and OS packages. `CARGO_BUILD_JOBS` defaults to 2 to limit memory use.

`bash packaging/linux/build.sh` builds directly on the host if the Dockerfile's dependencies are installed. A package built on a newer host can require newer glibc; use the container for releases. The AppImage tool (1.9.1) and runtime (20251108) are pinned and SHA256-verified. Neither packaging nor extraction requires FUSE.

| System | Package | Install / launch |
| --- | --- | --- |
| Ubuntu 22.04+ / Debian 12+ (amd64) | `kestrel_0.1.0_amd64.deb` | `sudo apt install ./dist/kestrel_0.1.0_amd64.deb` |
| Arch, EndeavourOS, Manjaro and other x86_64 systems with glibc 2.35+ | `Kestrel-0.1.0-x86_64.AppImage` | `chmod +x dist/*.AppImage`, then launch the file |

The `.deb` declares ELF dependencies detected by `dpkg-shlibdeps` and explicitly includes libraries GPUI loads at runtime. AppImage bundles fontconfig and FreeType with their dependencies. The graphics stack (X11/XCB, xkbcommon, Wayland, Vulkan and GPU drivers) and glibc come from the host, so older bundled libraries cannot override libraries required by a newer GPU driver. A working display session, Vulkan-capable driver and system CA certificates are required. On Arch install `libxcb`, `libxkbcommon`, `libxkbcommon-x11`, `wayland`, `vulkan-icd-loader`, the Vulkan driver matching your GPU (for example `vulkan-intel` or `vulkan-radeon`) and `ca-certificates`. The compatibility floor is a build target; graphical operation still needs checking on each distribution and GPU.

The AppImage exposes the terminal tools too:

```bash
./dist/Kestrel-0.1.0-x86_64.AppImage --kestrelcli --help
./dist/Kestrel-0.1.0-x86_64.AppImage --kestrel-mcp /path/to/collection
```

Run `bash packaging/linux/verify.sh` after building to install the `.deb` in clean Ubuntu 22.04 and Debian 12 containers, extract and run AppImage tools in both plus Arch Linux, and check that GUI ELF dependencies resolve. This does not test graphical rendering or GPU drivers.

For systems where FUSE mounting is unavailable:

```bash
./dist/Kestrel-0.1.0-x86_64.AppImage --appimage-extract
./squashfs-root/AppRun
```

## macOS VM

From this Linux workstation:

```bash
bash packaging/macos/vm.sh
```

Defaults match the existing OSX-KVM installation: `$HOME/vms/macos/OSX-KVM`, SSH `zift@127.0.0.1:2223`. The script connects to the running VM, or runs its existing `start.sh` if it is stopped. It waits for SSH and leaves the VM running. If OpenCore stops at its boot picker, finish booting from the VM's VNC console and rerun the command. The local script never replaces a VM disk, bootloader or configuration.

Source files are transferred to a temporary directory without `.env`, git data or build outputs. Rust's cache is retained at `~/kestrel-build/target` in the guest. The DMG is copied back to the host's `dist/`; temporary sources are removed, including after a failed build. Use `KESTREL_MACOS_VM_DIR`, `KESTREL_MACOS_SSH` and `KESTREL_MACOS_SSH_PORT` for another guest. SSH key authentication must already work; unknown host keys are recorded on first connection, changed keys are rejected.

The guest needs macOS 11+, Apple's Command Line Tools (`xcode-select --install`), Python 3 and Rust stable installed through rustup. Apple Silicon (`aarch64-apple-darwin`) is built first, followed by Intel (`x86_64-apple-darwin`). By default the script creates three images: `Kestrel-0.1.0-aarch64.dmg` for Apple Silicon, `Kestrel-0.1.0-x86_64.dmg` for Intel and `Kestrel-0.1.0-universal.dmg` for both. Each contains `Kestrel.app` and an Applications shortcut. Drag the app to Applications. CLI and MCP binaries are at `/Applications/Kestrel.app/Contents/MacOS/`.

For only Apple Silicon set `KESTREL_MACOS_ARCH=aarch64`; `x86_64` selects Intel and `universal` selects the combined image. On macOS itself run `bash packaging/macos/build.sh`. `bundle.sh` packages already compiled binaries without invoking the compiler. Intel guests can compile Apple Silicon binaries; running those binaries requires an Apple Silicon Mac. The app uses runtime Metal shaders so the Command Line Tools SDK is sufficient for compilation. The VM's emulated display does not provide a Metal-capable GPU; check the GUI on a Mac or a guest with compatible GPU passthrough.

Local builds use ad-hoc signatures. Distribution through Gatekeeper requires a Developer ID certificate and Apple notarization. For a configured signing machine:

```bash
export KESTREL_CODESIGN_IDENTITY='Developer ID Application: Your Name (TEAMID)'
export KESTREL_NOTARY_PROFILE='kestrel-notary'
bash packaging/macos/vm.sh
```

The profile must already exist in the guest's keychain (`xcrun notarytool store-credentials`). Credentials and certificates are never copied into the source tree. The script signs the binaries and app, creates and verifies the DMG, optionally notarizes and staples it, then writes its checksum.

## CI

`.github/workflows/packages.yml` builds packages on a `v*` tag or manual dispatch and uploads workflow artifacts; it does not publish a release. Linux uses the same Docker builder. macOS runs inside a self-hosted guest with labels `self-hosted`, `macOS`, `kestrel`; install a GitHub Actions runner in the macOS VM and assign the `kestrel` label to enable that job. Runner registration is intentionally a separate machine/account setup step. Only trusted repository code should be scheduled on that guest. No hosted macOS runner or cross-compilation from Linux is used.

References: [AppImage compatibility guidance](https://docs.appimage.org/reference/best-practices.html), [appimagetool](https://github.com/AppImage/appimagetool), [Rust macOS targets](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html), [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
