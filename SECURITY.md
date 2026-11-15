# Security Policy

## Supported versions

Security fixes are prioritized for the default branch during early development.

## Reporting a vulnerability

Share reports privately with the maintainer. Keep vulnerability details out of issues
and pull requests. Include the affected version or commit, environment, expected
security boundary, observed impact, and sanitized evidence. Remove credentials, private
data, and sensitive logs.

Coordinate disclosure with the maintainer so they can verify the report and prepare a
fix.

## Scope

Remy'a Launcher handles desktop entries, launch actions, calculator input and clipboard
history. Report boundary violations involving action or subprocess arguments, clipboard
capture and retention policy, or disclosure of clipboard data. Use synthetic clipboard
contents when documenting behavior.

Report standalone dependency vulnerabilities to the upstream project. Report exposure
specific to this application's integration here.
