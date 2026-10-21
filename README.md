# tufctl

[![ci](https://github.com/thisfilipe/tufctl/actions/workflows/ci.yml/badge.svg)](https://github.com/thisfilipe/tufctl/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/thisfilipe/tufctl)](https://github.com/thisfilipe/tufctl/releases)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A small, dependency-free command line tool to control **ASUS TUF/ROG laptops on Linux**
through the interfaces the kernel already provides: keyboard backlight, keyboard RGB,
platform thermal profile and custom fan curves.

No vendor daemon, no patched kernel, no Electron app.

```console
$ tufctl status
ASUS TUF Gaming A15 FA506NCR_FA506NCR
platform profile : performance  [quiet, balanced, performance]
keyboard         : backlight 3/3 | rgb available
fans             : cpu_fan 3500 rpm | gpu_fan 3600 rpm
fan1 curve       : firmware automatic
fan2 curve       : firmware automatic
```

## Why this exists

ASUS ships **Armoury Crate for Windows only**; there is no official Linux build.
The community answer is [asusctl](https://asus-linux.org/), which is excellent but, as of
October 2025, its own documentation states that Debian/Ubuntu derivatives (including
Pop!\_OS) are **not supported yet** and that distro kernel/package customisations are
likely to conflict with it.

Meanwhile, modern kernels expose almost everything through `asus-wmi` and `asus-armoury`:

| What you want            | Where the kernel exposes it                              |
| ------------------------ | -------------------------------------------------------- |
| Keyboard backlight       | `/sys/class/leds/asus::kbd_backlight/brightness`          |
| Keyboard RGB             | `/sys/class/leds/asus::kbd_backlight/kbd_rgb_mode`        |
| Thermal profile          | `/sys/firmware/acpi/platform_profile`                     |
| Fan curves               | `hwmon` device `asus_custom_fan_curve`                    |
| Fan speeds               | `hwmon` device `asus` (`fan1_input`, `fan2_input`)        |

`tufctl` is a thin, validated front end to those files. It exists so that changing a fan
curve is a typed command with range checking, instead of `echo` into sysfs as root.

## Features

- `status` — one screen with profile, backlight, RGB availability, fan speeds and curves
- `kbd` — keyboard backlight level, bounded by `max_brightness`
- `rgb` — Aura mode + colour + speed, optionally stored in firmware (`--save`)
- `profile` — `quiet`, `balanced`, `performance`
- `fan` — show / apply / reset 8-point fan curves per fan, with strict validation
- `--version`, `--help`, meaningful exit codes
- passwordless operation through a **polkit rule scoped to this single binary**
- no runtime dependencies, no background service

## Requirements

- An ASUS TUF/ROG laptop whose kernel exposes the attributes above
  (`asus-wmi`, `asus_nb_wmi`, and ideally `asus-armoury`)
- Linux kernel with those interfaces — **tested on 7.2.6**; `asus-armoury` and custom fan
  curves exist since 6.14, while `kbd_rgb_mode` may require a newer kernel. Missing
  attributes are reported instead of silently ignored.
- Rust 1.70+ and `cargo` to build
- `polkit` + `pkexec` for passwordless use (optional — `sudo` works too)

## Install

### From a release

Every tag publishes a static `x86_64-unknown-linux-musl` binary with a checksum, so no
Rust toolchain is needed:

```console
$ version=0.1.0
$ base=https://github.com/thisfilipe/tufctl/releases/download/v$version
$ curl -LO $base/tufctl-$version-x86_64-unknown-linux-musl.tar.gz
$ curl -LO $base/tufctl-$version-x86_64-unknown-linux-musl.tar.gz.sha256
$ sha256sum -c tufctl-$version-x86_64-unknown-linux-musl.tar.gz.sha256
$ tar xzf tufctl-$version-x86_64-unknown-linux-musl.tar.gz
$ sudo install -Dm755 tufctl /usr/local/bin/tufctl
$ sudo install -Dm644 org.asus.tufctl.policy /usr/share/polkit-1/actions/
```

### From source

```console
$ git clone https://github.com/thisfilipe/tufctl.git
$ cd tufctl
$ ./install.sh
```

The script builds the release binary and installs:

| File                                             | Purpose                              |
| ------------------------------------------------ | ------------------------------------ |
| `/usr/local/bin/tufctl`                          | the binary (root-owned)              |
| `/usr/share/polkit-1/actions/org.asus.tufctl.policy` | polkit action scoped to that binary |

`PREFIX` and `DESTDIR` are honoured for packaging. A prefix inside `$HOME` is refused on
purpose: the polkit rule elevates the binary, so it must not be user-writable.

Manual installation is also fine:

```console
$ cargo build --release
$ sudo install -Dm755 target/release/tufctl /usr/local/bin/tufctl
$ sed "s|@BINDIR@|/usr/local/bin|g" packaging/org.asus.tufctl.policy.in \
    | sudo tee /usr/share/polkit-1/actions/org.asus.tufctl.policy >/dev/null
```

polkitd watches its action directory, so no restart is normally needed.

## Usage

```console
$ tufctl kbd 2                          # keyboard backlight
$ tufctl rgb static 0 255 255           # static cyan, temporary
$ tufctl rgb static 0 255 255 --save    # ... and stored in the firmware
$ tufctl rgb rainbow-wave --speed 0     # slow rainbow effect
$ tufctl profile quiet                  # quieter fans under light load
$ tufctl fan show                       # current curves and percentages
$ tufctl fan set cpu 45:30,50:40,55:50,60:60,65:80,70:110,80:160,90:220 --yes
$ tufctl fan auto cpu                   # back to firmware control
$ tufctl fan reset cpu                  # restore the factory curve
$ tufctl help fan                       # help for a single command
```

Fan output:

```console
$ tufctl fan show

CPU (pwm1) - firmware automatic
    temp        pwm      %
    51C      2/255      0%
    54C     33/255     12%
    56C     45/255     17%
    ...
```

### Aura modes

`static`, `breathe`, `rainbow-cycle`, `rainbow-wave`, `star`, `rain`, `highlight`,
`laser`, `ripple`, `pulse`, `comet`, or the numeric id `0..=11`.

The numbering follows the vendor packet layout used by asusctl: id `9` is unused, and
effects that generate their own colours ignore the RGB arguments. Which effects a given
model actually renders is decided by its firmware, not by this tool.

## Safety

- Every value is validated before a write: backlight against `max_brightness`, RGB
  channels and mode against the driver range, fan curves against point count,
  monotonic temperatures, temperature and PWM bounds.
- Fan curves require an explicit `--yes`, because a bad curve can cook a laptop.
  `tufctl fan auto` and `tufctl fan reset` revert to firmware behaviour.
- RGB without `--save` is applied for the current session only; the firmware keeps its
  previous setting.
- The polkit action uses `allow_active=yes` and is **scoped to the installed binary**
  (`org.freedesktop.policykit.exec.path`), so it grants no general root access. Before
  elevating, `tufctl` also refuses to run if the binary is not root-owned or is writable
  by group/other.
- If you prefer not to install the polkit rule, run the commands with `sudo`.

## Limitations

- Keyboard lighting is **per-zone or single-zone on most TUF models**; per-key effects are
  not addressable through this interface.
- Fan curves are applied through the firmware, so behaviour depends on the EC. Some
  models expose only one fan, or none.
- `platform_profile` may be managed by another daemon on your distribution; such a daemon
  can override `tufctl profile`.
- Not covered yet: battery charge threshold, panel overdrive, GPU mode switching
  (use `supergfxctl` for that). PRs welcome.
- Do not run this alongside `asusd` for the same attributes: both write the same firmware
  endpoints.

## Tested on

| Model                  | Kernel   | Notes                                      |
| ---------------------- | -------- | ------------------------------------------ |
| TUF Gaming A15 FA506NCR| 7.2.6    | Pop!\_OS 24.04; backlight, RGB, profiles, 2 fan curves |

Adding your model to this table is a useful contribution — open an issue with the output
of `tufctl status` and `ls /sys/class/hwmon/*/name`.

## Alternatives

- [asusctl / rog-control-center](https://asus-linux.org/) — full featured, requires a
  supported distribution and its own daemon.
- Vendor software — Windows only.

## Uninstall

```console
$ sudo rm /usr/local/bin/tufctl /usr/share/polkit-1/actions/org.asus.tufctl.policy
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the design rules, the hardware testing recipe
and the pull request checklist. In short:

```console
$ cargo fmt --check
$ cargo clippy --all-targets -- -D warnings
$ cargo test
```

`main` is protected: changes land through pull requests with green CI and linear history.

Device reports are the most valuable contribution — they build the compatibility table
above. Bug reports are most useful with the model, kernel version and the relevant sysfs
contents. Do not paste serial numbers.

Security issues should follow [SECURITY.md](SECURITY.md) instead of a public issue.
Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

MIT — see [LICENSE](LICENSE).
