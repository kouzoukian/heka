# Pinned toolchain, dependencies, CI and release workflows

## Toolchain and dependencies

`rust-toolchain.toml` pins the toolchain of the Zed revision that `gpui-pre` 0.3.14 snapshots (`zed-industries/zed@f126950`, its `rust-toolchain.toml`). Local builds and CI both use it, and `rust-version` in the workspace `Cargo.toml` must match it.

- `gpui-kit-assets` supplies the icons `gpui-component` draws with. Without them icons render blank.
- `gpui_platform` (`gpui-pre-platform`) provides `application()`, which is not part of `gpui-pre`'s own dependency tree.
- `windows-sys` is pinned to the version `hidapi` already uses.
- `heka-app`'s `windows-sys` dev-dependency is only for `examples/tray_spike.rs`.

## Static C runtime

`.cargo/config.toml` links the Microsoft C runtime into `heka.exe`, so that it runs on a PC without the Visual C++ Redistributable. A `RUSTFLAGS` environment variable would replace that setting, so `heka-package` checks the binary before it goes into the release zip.

## Workflows

- Both workflows run `rustup toolchain install` with no arguments, which installs what `rust-toolchain.toml` asks for.
- CI and Release share one build cache, under the key `heka`. Only CI on `main` saves it, because every branch and tag can read what `main` saved, while a cache saved for a tag would serve no other run.
- CI on `main` also packages the release: a zip that cannot be made fails before any tag exists, and the release build lands in the cache the Release workflow starts from.
- Pushing a tag such as `v1.2.3` publishes the GitHub Release. Packaging fails unless the tag names the workspace version. A tag with a hyphen, such as `v1.2.3-rc.1`, is published as a pre-release.
