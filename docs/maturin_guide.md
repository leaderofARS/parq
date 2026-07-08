# Maturin Publishing Guide

This section outlines how to package `parq` using Maturin and set up automated multi-architecture publishing workflows on GitHub Actions.

---

## 1. Local Development Setup

To build and compile locally directly to your virtual environment:

```bash
cd crates/parq-python
python -m venv .venv
source .venv/bin/activate # Unix
# .venv\Scripts\Activate.ps1 # Windows

pip install maturin
maturin develop --release
```

---

## 2. CI/CD Release Automation (`.github/workflows/release.yml`)

The following workflow builds wheels for macOS, Linux, and Windows platforms automatically when a release tag is pushed:

```yaml
name: Publish to PyPI

on:
  push:
    tags:
      - 'v*'

jobs:
  build_wheels:
    name: Build wheels on ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]

    steps:
      - uses: actions/checkout@v4

      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          target: auto
          args: --release --out dist --manifest-path crates/parq-python/Cargo.toml
          sccache: 'true'

      - name: Upload wheels
        uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.os }}
          path: dist

  publish:
    name: Publish release to PyPI
    runs-on: ubuntu-latest
    needs: [build_wheels]
    permissions:
      id-token: write # required for Trusted Publishing
    steps:
      - name: Download all wheels
        uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          merge-multiple: true
          path: dist

      - name: Publish to PyPI
        uses: PyO3/maturin-action@v1
        with:
          command: upload
          args: --non-interactive --skip-existing dist/*
```

---

## 3. Trusted Publishing on PyPI (OIDC)

We use secure Trusted Publishing (OpenID Connect) which removes the need to store static tokens in GitHub Secrets:

1. Log in to [PyPI.org](https://pypi.org/).
2. Go to **Account Settings** -> **Publishers** -> **Add Publisher**.
3. Choose **GitHub**.
4. Configure:
   * **Owner**: organization or username.
   * **Repository**: repository name.
   * **Workflow name**: `release.yml`.
5. Trigger publish by pushing a release tag:
   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```
PyPI's backend will verify the OIDC token from the GitHub Action runner and authorize the upload automatically.
