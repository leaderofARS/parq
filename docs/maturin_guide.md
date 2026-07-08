# Maturin Wheel Building & Publishing Guide

This guide explains how to package `parq` using **Maturin**, build multi-architecture Python wheels (`.whl`), and publish them to PyPI using secure OIDC Trusted Publishing.

---

## 1. Local Development Setup

To build the Rust dynamic library and install it directly into your local active Python virtual environment:

```bash
# 1. Navigate to the Python wrapper crate
cd crates/parq-python

# 2. Create and activate a virtual environment
python -m venv .venv
# PowerShell (Windows):
.venv\Scripts\Activate.ps1
# Bash/Zsh (Unix):
source .venv/bin/activate

# 3. Install packaging tools
pip install maturin patchelf

# 4. Compile and install in development mode (unoptimized, fast compile)
maturin develop

# 5. Compile and install release mode (fully optimized)
maturin develop --release
```

---

## 2. Manual Wheel Compilation

To package your library as a wheel for local distribution on your current OS/CPU architecture:

```bash
cd crates/parq-python
maturin build --release
```
This builds and compiles the library, outputting the `.whl` package to `target/wheels/` at your workspace root.

---

## 3. CI/CD Release Automation (.github/workflows/release.yml)

To support multiple operating systems and architectures without needing dedicated physical machines, you can configure a GitHub Actions workflow:

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
      id-token: write # Required for Trusted Publishing
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

## 4. OIDC Trusted Publishing (No Static Tokens)

Instead of saving PyPI passwords or API tokens in GitHub repository secrets (which are vulnerable to compromise), use **OIDC (OpenID Connect) Trusted Publishing**:

### How to Configure on PyPI:
1. Log in to [PyPI.org](https://pypi.org/).
2. Navigate to **Account Settings** -> **Publishers** -> **Add Publisher**.
3. Choose **GitHub**.
4. Configure the OIDC details:
   * **Owner**: Your GitHub username or organization (e.g. `leaderofARS`).
   * **Repository**: `parq`.
   * **Workflow name**: `release.yml` (the filename of the release workflow).
   * **Environment name**: Leave blank (unless using GitHub Environments).
5. Trigger publishing by pushing a version tag:
   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

PyPI will dynamically authenticate the short-lived OIDC token issued by GitHub Actions for that run and securely upload your packages.
