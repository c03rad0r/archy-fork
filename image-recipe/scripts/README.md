# Build Scripts

Helper scripts for building Archipelago OS images.

## Scripts

### `build-backend.sh`
Compiles the Archipelago Rust backend binary.
- Output: `../build/backend/archipelago`
- Requires: Rust toolchain (or Docker)
- Builds for Linux x86_64

### `build-frontend.sh`
Builds the Vue.js frontend for production.
- Output: `../build/frontend/`
- Requires: Node.js 18+, npm
- Builds static files for serving

### `convert-iso-to-disk.sh`
Converts ISO image to raw disk image.
- Input: ISO file
- Output: `.img` file ready for `dd`
- Creates partition layout (EFI + root)

### `check-dependencies.sh`
Checks if all build dependencies are available.
- Checks: Rust, Node.js, Docker, xorriso
- Provides installation instructions
- Non-blocking (warns but continues)

### `install-podman.sh`
Installs Podman container runtime.
- For use inside the target system
- Configures rootless Podman

## Usage

These scripts are called automatically by the main build process. You can also run them manually for testing:

```bash
# Build just the backend
./scripts/build-backend.sh

# Build just the frontend
./scripts/build-frontend.sh

# Check dependencies
./scripts/check-dependencies.sh
```
