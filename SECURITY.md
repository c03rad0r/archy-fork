# Security Policy

## Reporting vulnerabilities

Please do not open a public issue for a security vulnerability.

Until a dedicated security intake address is published, report privately to the
project maintainer through the repository owner account or the private contact
channel listed on the project homepage.

Include:

- affected commit, version, or release;
- affected component;
- reproduction steps;
- expected impact;
- logs, proof of concept, or packet captures when relevant;
- whether the issue is already public.

We aim to acknowledge credible reports within 48 hours and coordinate fixes
before public disclosure.

## Scope

Security-sensitive areas include:

- authentication, session handling, CSRF, and rate limiting;
- release and app-catalog signature verification;
- container manifest validation and runtime compilation;
- Podman/Quadlet isolation, capabilities, volumes, and secret injection;
- backup encryption and key derivation;
- federation, Tor, Nostr, mesh, DID, and credential flows;
- Android companion pairing and device-token handling.

## Supported versions

Archipelago is currently pre-1.0 alpha software. Security fixes target the
current `main` branch and the latest published alpha release.
