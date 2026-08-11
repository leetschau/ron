# Releases

Binaries are built on GitHub Actions and published as a GitHub Release whenever a
version tag (`v*`) is pushed.

## Workflow

Defined in `.github/workflows/release.yml`. Triggered by a tag matching `v*`.

It builds a release binary natively on three runners (so the bundled SQLite in
`rusqlite` compiles cleanly), archives it, and attaches all three to the release:

| Runner           | Artifact                    |
|------------------|-----------------------------|
| `ubuntu-latest`  | `ron-x86_64-linux.tar.gz`   |
| `macos-latest`   | `ron-aarch64-macos.tar.gz`  |
| `windows-latest` | `ron-x86_64-windows.zip`    |

## Cutting a release

```bash
# 1. bump version in Cargo.toml, commit it
# 2. tag and push to the github remote
git tag v2.0.0
git push github v2.0.0
```

The tag push starts the workflow. Once it finishes, the binaries are available
under *Releases* on GitHub:

```
https://github.com/leetschau/ron/releases/tag/v2.0.0
```

Release notes are auto-generated from commits/PRs since the previous tag.

## Installing a downloaded binary

```bash
# Linux / macOS
tar -xzf ron-x86_64-linux.tar.gz   # or ron-aarch64-macos.tar.gz
chmod +x ron
sudo mv ron /usr/local/bin/

# Windows: unzip ron-x86_64-windows.zip and place ron.exe on PATH
```

## Notes

- `macos-latest` is Apple Silicon (`aarch64-apple-darwin`). If you need an
  Intel macOS build, add a `macos-13` row to the matrix.
- The workflow needs `permissions: contents: write` (already set) so it can
  create the release and upload assets.
- To re-release the same version, delete the tag and release on GitHub, then
  re-push the tag.
