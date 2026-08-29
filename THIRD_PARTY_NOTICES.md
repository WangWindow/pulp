# Third-party notices

Pulp includes and links to third-party components. They are not covered by the root [MIT License](LICENSE) and retain their upstream licenses and notices.

## 7-Zip SDK

Pulp builds the `Format7zF` bundle from the [7-Zip SDK](https://github.com/ip7z/7zip), which is checked out as a Git submodule at `crates/pulp/native/sdk`.

The SDK contains components covered by the GNU LGPL 2.1 or later, BSD 2-Clause, BSD 3-Clause, and public-domain notices. Its RAR decompression code also carries the unRAR license restriction: it must not be used to recreate the proprietary RAR compression algorithm or to develop a RAR-compatible archiver.

The complete upstream notices are kept in the SDK submodule:

- [`License.txt`](crates/pulp/native/sdk/DOC/License.txt)
- [`copying.txt`](crates/pulp/native/sdk/DOC/copying.txt)
- [`unRarLicense.txt`](crates/pulp/native/sdk/DOC/unRarLicense.txt)

Release archives include these files under `licenses/7zip/`.

## Rust dependencies and UI libraries

Pulp also uses third-party Rust crates, including [GPUI](https://gpui.rs/) and [gpui-component](https://longbridge.github.io/gpui-component/). These dependencies retain their own licenses; Pulp's MIT License does not relicense them. The resolved versions are recorded in [`Cargo.lock`](Cargo.lock), and each dependency's upstream package metadata and source notice are authoritative.
