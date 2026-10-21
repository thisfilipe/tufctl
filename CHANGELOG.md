# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-20

First public release, tested on an ASUS TUF Gaming A15 FA506NCR (kernel 7.2.6).

### Added

- `status`: model, platform profile, backlight, RGB availability, fan speeds and curves
- `kbd`: read and set the keyboard backlight within `max_brightness`
- `rgb`: Aura mode, colour and speed, with optional `--save` to store in firmware
- `profile`: read and set `quiet`, `balanced`, `performance`
- `fan show|auto|reset|set`: inspect, apply and revert 8-point fan curves
- per-command help via `tufctl help <COMMAND>` and `tufctl <COMMAND> --help`
- validation before every write: ranges, point count, monotonic temperatures,
  explicit `--yes` for fan curves
- polkit action scoped to the installed binary, plus a root-ownership check before
  elevating through `pkexec`
- unit tests, CI (fmt, clippy, tests) and a tag-driven release workflow producing a
  static `x86_64-unknown-linux-musl` binary

[Unreleased]: https://github.com/thisfilipe/tufctl/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/thisfilipe/tufctl/releases/tag/v0.1.0
