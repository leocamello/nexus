# Security Policy

## Reporting a Vulnerability

The Nexus team takes security issues seriously. We appreciate your efforts to responsibly disclose your findings.

**Please do NOT report security vulnerabilities through public GitHub issues.**

### How to Report

1. **GitHub Security Advisories (Preferred)**: Use [GitHub Security Advisories](https://github.com/leocamello/nexus/security/advisories/new) to privately report a vulnerability.

2. **Email**: If you prefer email, contact the maintainer directly. You can find contact information on the [GitHub profile](https://github.com/leocamello).

### What to Include

- Description of the vulnerability
- Steps to reproduce the issue
- Potential impact
- Suggested fix (if any)

### What to Expect

- **Acknowledgment**: Within 48 hours of your report.
- **Assessment**: We will investigate and provide an initial assessment within 7 days.
- **Fix Timeline**: Critical vulnerabilities will be patched as soon as possible. Non-critical issues will be addressed in the next release cycle.
- **Disclosure**: We follow a 90-day coordinated disclosure timeline. We will work with you to ensure the vulnerability is addressed before any public disclosure.

### Recognition

We are happy to credit security researchers who report valid vulnerabilities in our release notes and CHANGELOG (unless you prefer to remain anonymous).

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.3.x   | ✅ Yes    |
| 0.2.x   | ⚠️ Critical fixes only |
| < 0.2   | ❌ No     |

## Security Best Practices for Users

- **API Keys**: Always use `api_key_env` in configuration to reference environment variables. Never store API keys directly in config files.
- **Network Exposure**: When running Nexus on a public network, use a reverse proxy (e.g., Nginx, Traefik) with TLS termination.
- **Dashboard Access**: The web dashboard at `/` has no authentication. Do not expose it to untrusted networks without a reverse proxy.
- **WebSocket**: Use `wss://` (TLS) instead of `ws://` in production environments.
