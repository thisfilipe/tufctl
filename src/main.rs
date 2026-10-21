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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelpTopic {
    General,
    Status,
    Kbd,
    Rgb,
    Profile,
    Fan,
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Help(HelpTopic),
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
            print_help(HelpTopic::General);
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
        Action::Help(topic) => {
            print_help(*topic);
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
        None => return Ok(Action::Help(HelpTopic::General)),
    };

    if matches!(command, "help" | "--help" | "-h") {
        return match args.get(1) {
            None => Ok(Action::Help(HelpTopic::General)),
            Some(topic) => Ok(Action::Help(parse_help_topic(topic)?)),
        };
    }

    if args.iter().any(|value| value == "--help" || value == "-h") {
        return Ok(Action::Help(parse_help_topic(command)?));
    }

    match command {
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

fn parse_help_topic(value: &str) -> Result<HelpTopic, String> {
    match value {
        "status" => Ok(HelpTopic::Status),
        "kbd" => Ok(HelpTopic::Kbd),
        "rgb" => Ok(HelpTopic::Rgb),
        "profile" => Ok(HelpTopic::Profile),
        "fan" => Ok(HelpTopic::Fan),
        "help" | "version" => Ok(HelpTopic::General),
        other => Err(format!(
            "no help for '{other}'; run `tufctl help` for the command list"
        )),
    }
}

fn print_help(topic: HelpTopic) {
    match topic {
        HelpTopic::General => print_help_general(),
        HelpTopic::Status => print_help_status(),
        HelpTopic::Kbd => print_help_kbd(),
        HelpTopic::Rgb => print_help_rgb(),
        HelpTopic::Profile => print_help_profile(),
        HelpTopic::Fan => print_help_fan(),
    }
}

fn print_help_general() {
    println!(
        r#"tufctl {} - ASUS TUF/ROG hardware control through the Linux kernel

USAGE
  tufctl <COMMAND> [ARGUMENTS]
  tufctl help [COMMAND]

COMMANDS
  status              profile, backlight, RGB availability, fans and curves
  kbd [LEVEL]         read or set the keyboard backlight
  rgb <MODE> [R G B]  keyboard color or effect
  profile [NAME]      read or set the platform thermal profile
  fan show            print both fan curves
  fan auto <FAN>      hand a fan back to firmware control
  fan reset <FAN>     restore the factory curve
  fan set ...         apply a custom 8-point curve (requires --yes)
  version             print version

FAN
  cpu (or 1) and gpu (or 2); points are temperature:pwm pairs

EXIT CODES
  0 success   1 runtime error   2 usage error

Run `tufctl help <COMMAND>` or `tufctl <COMMAND> --help` for details."#,
        env!("CARGO_PKG_VERSION")
    );
}

fn print_help_status() {
    println!(
        r#"tufctl status - current state

USAGE
  tufctl status

PRINTS
  Model, platform profile and available choices, keyboard backlight and RGB
  availability, fan speeds and whether each fan uses a custom curve.

Requires an ASUS laptop exposing the asus-wmi kernel interfaces."#
    );
}

fn print_help_kbd() {
    println!(
        r#"tufctl kbd - keyboard backlight

USAGE
  tufctl kbd            print the current level and the maximum
  tufctl kbd <LEVEL>    set the backlight (0..max_brightness)

NOTES
  The range comes from the kernel, usually 0..3; level 0 turns the
  backlight off on most models. Writes are elevated through pkexec."#
    );
}

fn print_help_rgb() {
    println!(
        r#"tufctl rgb - keyboard color and effects

USAGE
  tufctl rgb <MODE> [R G B] [--speed 0|1|2] [--save]

MODES
  static, breathe, rainbow-cycle, rainbow-wave, star, rain, highlight,
  laser, ripple, pulse, comet, or a numeric id 0..11.

OPTIONS
  R G B       red green blue, 0..255 each; defaults to 255 255 255
  --speed N   0 = slow, 1 = medium (default), 2 = fast
  --save      store the effect in firmware; without it the change is temporary

EXAMPLES
  tufctl rgb static 0 255 255 --save
  tufctl rgb rainbow-wave --speed 0

Some effects generate their own colors and ignore R G B; which effects a
model renders is decided by its firmware."#
    );
}

fn print_help_profile() {
    println!(
        r#"tufctl profile - platform thermal profile

USAGE
  tufctl profile          print the active profile and the available ones
  tufctl profile <NAME>   set quiet, balanced or performance

NOTES
  The choices are read from the kernel, so they may differ on some models.
  Another power daemon on your system may override the setting."#
    );
}

fn print_help_fan() {
    println!(
        r#"tufctl fan - fan curves

USAGE
  tufctl fan show
  tufctl fan auto <FAN>
  tufctl fan reset <FAN>
  tufctl fan set <FAN> <T1:P1,...,T8:P8> --yes

FAN
  cpu (or 1) and gpu (or 2)

POINTS
  Exactly 8 temperature:pwm pairs. Temperatures in Celsius, strictly
  increasing and within 0..120; pwm within 0..255.

EXAMPLE
  tufctl fan set cpu 45:30,50:40,55:50,60:60,65:80,70:110,80:160,90:220 --yes

SAFETY
  A custom curve replaces the firmware behaviour. `fan auto` returns to
  firmware control and `fan reset` restores the factory curve."#
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

    #[test]
    fn routes_help_to_topics() {
        assert_eq!(parse_args(&[]).unwrap(), Action::Help(HelpTopic::General));
        assert_eq!(
            parse_args(&args(&["--help"])).unwrap(),
            Action::Help(HelpTopic::General)
        );
        assert_eq!(
            parse_args(&args(&["help", "fan"])).unwrap(),
            Action::Help(HelpTopic::Fan)
        );
        assert_eq!(
            parse_args(&args(&["rgb", "--help"])).unwrap(),
            Action::Help(HelpTopic::Rgb)
        );
        assert_eq!(
            parse_args(&args(&["fan", "-h"])).unwrap(),
            Action::Help(HelpTopic::Fan)
        );
    }

    #[test]
    fn rejects_help_for_unknown_command() {
        assert!(parse_args(&args(&["help", "nope"])).is_err());
    }
}
