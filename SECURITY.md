# Security policy

## Scope

`tufctl` writes to privileged kernel interfaces and ships a polkit action, so the
security-relevant surface is narrow but real:

- the polkit action must stay scoped to the installed binary
  (`org.freedesktop.policykit.exec.path`), never to a generic interpreter;
- the binary must remain root-owned and not writable by group/other — the tool checks
  this before elevating, and `install.sh` refuses prefixes inside `$HOME`;
- input validation must prevent out-of-range values from reaching sysfs.

## Supported versions

Only the latest release on `main` is supported.

## Reporting a vulnerability

Please do **not** open a public issue. Use GitHub's private vulnerability reporting on
the repository's *Security* tab, or email the maintainer at filipeeleaal@gmail.com.

Include the model, kernel version, the exact command and what happened. You can expect an
initial response within a week. If the issue allows a non-root user to gain privileges,
describe the reproduction privately and give the maintainer time to release a fix before
disclosing it.
