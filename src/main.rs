//! tufctl - ASUS TUF/ROG hardware control through the Linux kernel.
//!
//! No vendor daemon, no patched kernel: every operation maps to a documented
//! attribute under /sys, with validation before anything is written.

mod kbd;
mod power;
mod sysfs;

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::{exit, Command};

const PKEXEC: &str = "/usr/bin/pkexec";

enum Action {
    Help,
    Version,
    Status,
    KbdGet,
    KbdSet(u8),
    Rgb {
        mode: u8,
        red: u8,
        green: u8,
        blue: u8,
        speed: u8,
        save: bool,
    },
    ProfileGet,
    ProfileSet(String),
    FanShow,
    FanAuto(u8),
    FanReset(u8),
    FanSet {
        fan: u8,
        points: Vec<(i32, i32)>,
    },
}

impl Action {
    fn needs_root(&self) -> bool {
        matches!(
            self,
            Action::KbdSet(_)
                | Action::Rgb { .. }
                | Action::ProfileSet(_)
                | Action::FanAuto(_)
                | Action::FanReset(_)
                | Action::FanSet { .. }
        )
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let action = match parse_args(&args) {
        Ok(action) => action,
        Err(message) => {
            eprintln!("tufctl: {message}\n");
            print_usage();
            exit(2);
        }
    };

    if let Err(message) = ensure_privileges(&action) {
        eprintln!("tufctl: {message}");
        exit(1);
    }

    if let Err(message) = run(&action) {
        eprintln!("tufctl: {message}");
        exit(1);
    }
}

fn run(action: &Action) -> Result<(), String> {
    match action {
        Action::Help => {
            print_usage();
            Ok(())
        }
        Action::Version => {
            println!("tufctl {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Action::Status => print_status(),
        Action::KbdGet => {
            println!(
                "keyboard backlight: {}/{}",
                kbd::brightness()?,
                kbd::max_brightness()?
            );
            Ok(())
        }
        Action::KbdSet(level) => {
            kbd::set_brightness(*level)?;
            println!(
                "keyboard backlight: {}/{}",
                kbd::brightness()?,
                kbd::max_brightness()?
            );
            Ok(())
        }
        Action::Rgb {
            mode,
            red,
            green,
            blue,
            speed,
            save,
        } => {
            kbd::set_rgb(*mode, *red, *green, *blue, *speed, *save)?;
            println!(
                "rgb applied: mode {mode}, color #{red:02x}{green:02x}{blue:02x}, speed {speed}{}",
                if *save {
                    " (stored in firmware)"
                } else {
                    " (temporary)"
                }
            );
            Ok(())
        }
        Action::ProfileGet => {
            println!("platform profile: {}", power::profile()?);
            println!("available: {}", power::profile_choices()?.join(", "));
            Ok(())
        }
        Action::ProfileSet(name) => {
            power::set_profile(name)?;
            println!("platform profile: {}", power::profile()?);
            Ok(())
        }
        Action::FanShow => print_fans(),
        Action::FanAuto(fan) => {
            power::set_fan_auto(*fan)?;
            println!("fan {fan}: firmware automatic control");
            Ok(())
        }
        Action::FanReset(fan) => {
            power::reset_fan_curve(*fan)?;
            println!("fan {fan}: factory curve restored");
            Ok(())
        }
        Action::FanSet { fan, points } => {
            power::set_curve(*fan, points)?;
            println!("fan {fan}: custom curve active");
            print_fan(*fan)
        }
    }
}

fn print_status() -> Result<(), String> {
    sysfs::check_platform()?;

    let product = sysfs::read_trim(sysfs::DMI_PRODUCT).unwrap_or_else(|_| "unknown".to_string());
    let profile = power::profile()?;
    let choices = power::profile_choices()?;

    println!("{product}");
    println!("platform profile : {profile}  [{}]", choices.join(", "));
    println!(
        "keyboard         : backlight {}/{} | rgb {}",
        kbd::brightness()?,
        kbd::max_brightness()?,
        if kbd::has_rgb() {
            "available"
        } else {
            "unavailable"
        }
    );

    match power::fan_speeds() {
        Ok(fans) if !fans.is_empty() => {
            let rendered: Vec<String> = fans
                .iter()
                .map(|(label, rpm)| format!("{label} {rpm} rpm"))
                .collect();
            println!("fans             : {}", rendered.join(" | "));
        }
        Ok(_) => println!("fans             : not exposed by this firmware"),
        Err(message) => println!("fans             : {message}"),
    }

    for fan in [1u8, 2u8] {
        if power::curve_exists(fan) {
            let active = power::curve(fan)
                .map(|(enabled, _)| enabled)
                .unwrap_or(false);
            println!(
                "fan{fan} curve      : {}",
                if active {
                    "custom (active)"
                } else {
                    "firmware automatic"
                }
            );
        }
    }

    Ok(())
}

fn print_fans() -> Result<(), String> {
    let mut printed = false;

    for fan in [1u8, 2u8] {
        if power::curve_exists(fan) {
            print_fan(fan)?;
            printed = true;
        }
    }

    if !printed {
        return Err("no configurable fan curve found".to_string());
    }

    Ok(())
}

fn print_fan(fan: u8) -> Result<(), String> {
    let (enabled, points) = power::curve(fan)?;

    println!(
        "\n{} (pwm{fan}) - {}",
        fan_label(fan),
        if enabled {
            "custom curve active"
        } else {
            "firmware automatic"
        }
    );
    println!("  {:>6}  {:>9}  {:>5}", "temp", "pwm", "%");
    for (temp, pwm) in points {
        println!("  {temp:>4}C  {pwm:>5}/255  {:>4}%", pwm * 100 / 255);
    }
    Ok(())
}

fn fan_label(fan: u8) -> &'static str {
    match fan {
        1 => "CPU",
        2 => "GPU",
        _ => "unknown",
    }
}

/// Re-executes through pkexec when the action needs root and the binary is
/// installed system-wide and owned by root.
fn ensure_privileges(action: &Action) -> Result<(), String> {
    if !action.needs_root() || sysfs::euid() == 0 {
        return Ok(());
    }

    let exe = std::env::current_exe()
        .map_err(|error| format!("cannot resolve own executable: {error}"))?;

    if let Err(reason) = elevation_is_safe(&exe) {
        return Err(format!(
            "this action needs root ({reason}).\nrun it with sudo, or install tufctl system-wide via ./install.sh."
        ));
    }

    let status = Command::new(PKEXEC)
        .arg(&exe)
        .args(std::env::args().skip(1))
        .status()
        .map_err(|error| format!("cannot run {PKEXEC}: {error}"))?;

    exit(status.code().unwrap_or(1));
}

/// A binary used through pkexec must be root-owned and not writable by
/// group/other, otherwise the polkit rule would be a privilege escalation.
fn elevation_is_safe(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;

    if metadata.uid() != 0 {
        return Err(format!("{} is not owned by root", path.display()));
    }

    if metadata.mode() & 0o022 != 0 {
        return Err(format!("{} is writable by group or others", path.display()));
    }

    Ok(())
}

fn parse_args(args: &[String]) -> Result<Action, String> {
    let command = match args.first() {
        Some(value) => value.as_str(),
        None => return Ok(Action::Help),
    };

    match command {
        "help" | "-h" | "--help" => Ok(Action::Help),
        "version" | "-V" | "--version" => Ok(Action::Version),
        "status" => Ok(Action::Status),
        "kbd" => match args.get(1) {
            None => Ok(Action::KbdGet),
            Some(value) => Ok(Action::KbdSet(parse_u8(value, "brightness")?)),
        },
        "rgb" => parse_rgb(&args[1..]),
        "profile" => match args.get(1) {
            None => Ok(Action::ProfileGet),
            Some(name) => Ok(Action::ProfileSet(name.clone())),
        },
        "fan" => parse_fan_command(&args[1..]),
        other => Err(format!("unknown command '{other}'")),
    }
}

fn parse_rgb(args: &[String]) -> Result<Action, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut speed = 1u8;
    let mut save = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--save" => save = true,
            "--speed" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--speed requires a value (0, 1 or 2)".to_string())?;
                speed = parse_u8(value, "speed")?;

                if speed > 2 {
                    return Err(format!("invalid speed {speed}; use 0, 1 or 2"));
                }
            }
            value => positional.push(value.to_string()),
        }
        index += 1;
    }

    if positional.is_empty() {
        return Err("usage: tufctl rgb <mode> [r g b] [--speed 0|1|2] [--save]".to_string());
    }

    let mode = kbd::parse_mode(&positional[0])?;

    let (red, green, blue) = match positional.len() {
        1 => (255, 255, 255),
        4 => (
            parse_u8(&positional[1], "red")?,
            parse_u8(&positional[2], "green")?,
            parse_u8(&positional[3], "blue")?,
        ),
        _ => {
            return Err("provide either just the mode or the mode plus red green blue".to_string())
        }
    };

    Ok(Action::Rgb {
        mode,
        red,
        green,
        blue,
        speed,
        save,
    })
}

fn parse_fan_command(args: &[String]) -> Result<Action, String> {
    let subcommand = args
        .first()
        .ok_or_else(|| "usage: tufctl fan show|auto|reset|set <cpu|gpu> <points>".to_string())?;

    match subcommand.as_str() {
        "show" => Ok(Action::FanShow),
        "auto" => Ok(Action::FanAuto(parse_fan(args.get(1))?)),
        "reset" => Ok(Action::FanReset(parse_fan(args.get(1))?)),
        "set" => {
            let fan = parse_fan(args.get(1))?;
            let raw_points = args.get(2).ok_or_else(|| {
                "usage: tufctl fan set <cpu|gpu> <t1:p1,t2:p2,...,t8:p8> --yes".to_string()
            })?;

            if !args.iter().any(|value| value == "--yes") {
                return Err(
                    "changing a fan curve requires explicit confirmation: add --yes".to_string(),
                );
            }

            let points = parse_points(raw_points)?;
            power::validate_points(&points)?;

            Ok(Action::FanSet { fan, points })
        }
        other => Err(format!("unknown fan subcommand '{other}'")),
    }
}

fn parse_fan(value: Option<&String>) -> Result<u8, String> {
    match value.map(String::as_str) {
        Some("cpu") | Some("1") => Ok(1),
        Some("gpu") | Some("2") => Ok(2),
        Some(other) => Err(format!("invalid fan '{other}'; use cpu or gpu")),
        None => Err("missing fan: use cpu or gpu".to_string()),
    }
}

fn parse_points(value: &str) -> Result<Vec<(i32, i32)>, String> {
    let mut points = Vec::new();

    for chunk in value.split(',') {
        let (temp, pwm) = chunk
            .split_once(':')
            .ok_or_else(|| format!("invalid point '{chunk}'; expected temp:pwm"))?;
        let temp = temp
            .trim()
            .parse::<i32>()
            .map_err(|_| format!("invalid temperature in '{chunk}'"))?;
        let pwm = pwm
            .trim()
            .parse::<i32>()
            .map_err(|_| format!("invalid pwm in '{chunk}'"))?;

        points.push((temp, pwm));
    }

    Ok(points)
}

fn parse_u8(value: &str, what: &str) -> Result<u8, String> {
    value
        .parse::<u8>()
        .map_err(|_| format!("invalid {what}: '{value}'"))
}

fn print_usage() {
    println!(
        r#"tufctl - ASUS TUF/ROG hardware control through the Linux kernel

USAGE:
  tufctl status                              show current state
  tufctl kbd [0..N]                          keyboard backlight level
  tufctl rgb <mode> [r g b] [--speed 0|1|2] [--save]
                                             keyboard color/effect
  tufctl profile [quiet|balanced|performance]
                                             platform thermal profile
  tufctl fan show                            current fan curves
  tufctl fan auto <cpu|gpu>                  return fan to firmware control
  tufctl fan reset <cpu|gpu>                 restore factory curve
  tufctl fan set <cpu|gpu> <t:p,...> --yes   apply a custom curve (8 points)
  tufctl version                             print version

MODES:
  static, breathe, rainbow-cycle, rainbow-wave, star, rain, highlight,
  laser, ripple, pulse, comet  (or numeric 0..=11)

EXAMPLES:
  tufctl rgb static 0 255 255 --save         static cyan, stored in firmware
  tufctl rgb rainbow-wave                    temporary rainbow effect
  tufctl profile quiet                       quieter fans under light load
  tufctl fan set cpu 45:30,50:40,55:50,60:60,65:80,70:110,80:160,90:220 --yes

NOTES:
  - writes are elevated through pkexec and a polkit rule scoped to this binary
  - rgb without --save is temporary; the firmware keeps its previous setting
  - fan curves use Celsius and pwm 0..255, with strictly increasing temperatures
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_points() {
        let points = parse_points("40:20,50:40").unwrap();
        assert_eq!(points, vec![(40, 20), (50, 40)]);
    }

    #[test]
    fn rejects_malformed_points() {
        assert!(parse_points("40-20").is_err());
        assert!(parse_points("hot:20").is_err());
        assert!(parse_points("40:loud").is_err());
    }

    #[test]
    fn requires_confirmation_for_fan_curves() {
        let curve = "40:20,50:40,55:60,60:80,65:110,70:150,80:200,90:255";
        let without = args(&["fan", "set", "cpu", curve]);
        assert!(parse_fan_command(&without[1..]).is_err());

        let with = args(&["fan", "set", "cpu", curve, "--yes"]);
        assert!(parse_fan_command(&with[1..]).is_ok());
    }

    #[test]
    fn rgb_defaults_to_white() {
        match parse_rgb(&args(&["static"])).unwrap() {
            Action::Rgb {
                mode,
                red,
                green,
                blue,
                ..
            } => {
                assert_eq!(mode, 0);
                assert_eq!((red, green, blue), (255, 255, 255));
            }
            _ => panic!("expected an rgb action"),
        }
    }

    #[test]
    fn rgb_accepts_explicit_color_and_flags() {
        match parse_rgb(&args(&["pulse", "1", "2", "3", "--speed", "2", "--save"])).unwrap() {
            Action::Rgb {
                mode,
                red,
                green,
                blue,
                speed,
                save,
            } => {
                assert_eq!(
                    (mode, red, green, blue, speed, save),
                    (10, 1, 2, 3, 2, true)
                );
            }
            _ => panic!("expected an rgb action"),
        }
    }

    #[test]
    fn rejects_out_of_range_speed() {
        assert!(parse_rgb(&args(&["static", "1", "2", "3", "--speed", "5"])).is_err());
    }
}
