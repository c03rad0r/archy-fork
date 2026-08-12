# ADR-001: Podman Over Docker

**Status**: Accepted
**Date**: 2026-03

## Context

Archipelago needs a container runtime for running applications. Docker and Podman are the two main options.

## Decision

Use Podman as the container runtime instead of Docker.

## Consequences

### Positive
- **Rootless by default**: Containers run without root privileges, reducing attack surface
- **Daemonless**: No persistent daemon process; containers are managed as individual processes under systemd
- **Docker-compatible**: Supports Docker images and most Docker CLI commands
- **Systemd integration**: Podman containers can be managed as systemd services natively
- **No vendor lock-in**: OCI-compliant, works with any container registry

### Negative
- **Smaller ecosystem**: Some Docker-specific tools and compose features require adaptation
- **Docker Compose differences**: Podman Compose exists but has occasional compatibility gaps
- **Documentation**: Most container documentation assumes Docker; developers need to translate
- **Networking**: Podman networking (CNI/netavark) differs from Docker's bridge networking

### Mitigation
- Use `podman` CLI wrapper that provides Docker-compatible interface
- Document Podman-specific commands in developer guide
- Use `archy-net` custom network for inter-container DNS
