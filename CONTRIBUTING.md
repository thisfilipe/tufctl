# Contributing to tufctl

Thanks for considering a contribution. This project exists because ASUS laptops on
Linux need a small, predictable tool instead of a vendor stack, so the most valuable
contributions are **hardware reports** and **careful, minimal code**.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

| Type | Where |
| --- | --- |
| Report a model that works (or does not) | [Device report](.github/ISSUE_TEMPLATE/device_report.yml) |
| Report a bug | [Bug report](.github/ISSUE_TEMPLATE/bug_report.yml) |
| Suggest a feature | [Feature request](.github/ISSUE_TEMPLATE/feature_request.yml) |
| Send code or docs | Pull request |

## Device reports

The project only supports what the kernel exposes, and that varies by model. A device
report is genuinely useful when it includes:

```console
$ tufctl status
$ uname -r
$ cat /sys/class/dmi/id/product_name
$ ls /sys/class/hwmon/*/name
$ ls /sys/class/leds/asus::kbd_backlight/
```

State which features work: backlight, RGB, platform profiles, fan curves, fan speeds.
If something is missing, mention the attribute that is absent — that is usually enough to
tell whether it is a driver limitation or a bug here.

**Do not paste serial numbers, UUIDs or full `dmidecode` dumps.** `product_name` and the
sysfs listings above are enough.

## Development

```console
$ cargo build --release
$ cargo fmt --check
$ cargo clippy --all-targets -- -D warnings
$ cargo test
```

CI runs the same commands; a pull request that fails them will not be merged.

### Design rules

1. **No runtime dependencies.** `std` only. This keeps the binary auditable and small.
2. **No daemon.** Every command is a short-lived process writing to sysfs.
3. **Validate before writing.** Ranges, point counts and ordering are checked in code,
   not left to the firmware.
4. **Procedural code.** Free functions, plain data, one file per concern
   (`sysfs`, `kbd`, `power`, CLI). Avoid abstraction layers that do not pay for themselves.
5. **Fail loudly.** Report the exact path and the kernel error; never swallow an EACCES.

### Project layout

```
src/main.rs    argument parsing, dispatch, elevation, output
src/sysfs.rs   thin read/write helpers, DMI checks
src/kbd.rs     backlight and Aura RGB, mode names
src/power.rs   platform profile, fan curves, validation
```

## Testing on real hardware

Writes touch the machine you run them on. Two safe techniques:

- **Idempotent write.** Write the value that is already there (`tufctl kbd 3`,
  `tufctl profile performance`) to exercise the elevation path without changing state.
- **Round-trip a fan curve.** Read the firmware curve with `tufctl fan show`, apply exactly
  those values with `--yes`, confirm `custom (active)`, then restore with
  `tufctl fan auto <cpu|gpu>`.

Never leave a custom curve enabled when you finish testing. If a change could plausibly
overheat a machine, say so in the pull request instead of testing it blindly.

## Pull requests

- One concern per pull request; separate refactors from behaviour changes.
- Commit subjects in the imperative mood (`add fan curve validation`, not `added`).
- Explain **why** in the body, plus the model and kernel you tested on.
- Update `README.md` when behaviour or the supported-feature table changes.
- Add a unit test for anything that parses or validates input.
- Do not add a dependency without discussing it in an issue first.

Small, well-tested changes are merged quickly. Large rewrites need an issue first.

## Review and merge policy

`main` is protected: changes arrive through pull requests, CI must pass, and history stays
linear (squash merge). Maintainers may merge their own changes after CI.

## Security

Please do not open a public issue for anything that could grant unintended privileges,
such as a path where the polkit-scoped binary could be replaced by a non-root user. See
[SECURITY.md](SECURITY.md).
