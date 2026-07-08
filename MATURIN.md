# Maturin Build & Publish Guide for `parq`

This guide explains how to use **Maturin** to compile your PyO3 Rust project into Python wheels and publish them to PyPI, enabling users to install your engine simply by running `pip install parq`.

---

## 1. Directory Layout

The Python package structure is located under [`crates/parq-python/`](file:///C:/Users/Asus/Desktop/DataParser/crates/parq-python/).

It contains:
- [`Cargo.toml`](file:///C:/Users/Asus/Desktop/DataParser/crates/parq-python/Cargo.toml) — FFI crate configuration.
- [`pyproject.toml`](file:///C:/Users/Asus/Desktop/DataParser/crates/parq-python/pyproject.toml) — Maturin project configuration.
- [`src/lib.rs`](file:///C:/Users/Asus/Desktop/DataParser/crates/parq-python/src/lib.rs) — PyO3 bindings.

---

## 2. Local Development & Testing

During development, you don't need to build full wheels. You can compile the Rust code and install it directly into your active Python virtual environment.

```bash
# 1. Navigate to the Python crate directory
cd crates/parq-python

# 2. Create and activate virtual environment
python -m venv .venv
# On Windows (PowerShell):
.venv\Scripts\Activate.ps1
# On Unix:
source .venv/bin/activate

# 3. Install maturin
pip install maturin

# 4. Compile and install development build (unoptimized, fast compile)
maturin develop

# 5. Compile and install release build (optimized, for benchmarking)
maturin develop --release
```

Once `maturin develop` finishes, you can import and test it immediately:
```python
import parq
print(parq.__file__)
```

---

## 3. Building Wheels for Distribution

A "wheel" (`.whl`) is a pre-compiled binary package. When a user runs `pip install parq`, pip downloads the wheel matching their OS and Python version, removing the need for a local Rust compiler.

To build wheels for your local OS/Architecture:
```bash
cd crates/parq-python
maturin build --release
```
The compiled wheels will be written to `target/wheels/` under the workspace root.

---

## 4. Automated Cross-Compilation with GitHub Actions

To support multiple OSs and architectures (Windows, macOS Intel & Apple Silicon, Linux glibc & musl) without native hardware, use GitHub Actions.

The CI workflow in [`.github/workflows/ci.yml`](file:///C:/Users/Asus/Desktop/DataParser/.github/workflows/ci.yml) builds, formats, lints, and runs tests. To package and publish wheels, you can create a dedicated delivery workflow or use Maturin's action templates.

### Automated Publishing Workflow (`.github/workflows/release.yml`)

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

## 5. Trusted Publishing with PyPI (OIDC)

Instead of saving PyPI tokens or passwords in GitHub secrets, configure **Trusted Publishing**:

1. Go to [PyPI.org](https://pypi.org/) and log in.
2. Navigate to **Account Settings** -> **Publishers** -> **Add Publisher**.
3. Choose **GitHub**.
4. Set:
   - **Repository Owner**: Your username or organization.
   - **Repository Name**: `parq` (or the repository name).
   - **Workflow Name**: `release.yml` (matching the file name above).
   - **Environment Name**: Leave blank.
5. Create a git tag and push to publish:
   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```
PyPI will automatically verify the OIDC token from the GitHub Action runner and securely authorize the upload of your wheels.
