# Security policy

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

Please report security issues privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue for security problems.

You can expect an initial reply within 7 days. Please include the version, Windows version, steps to reproduce and the impact you observed.

## Scope notes

- ZeroIn makes no network requests. Releases are built by GitHub Actions and published with SHA-256 checksums.
- The bundled model is downloaded in CI from a pinned revision and verified against a fixed SHA-256 before packaging.
- Optional third-party libraries (the Wooting Analog SDK) are loaded only if the user installed them; ZeroIn does not ship them.
