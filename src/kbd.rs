//! Keyboard backlight and TUF/ROG Aura RGB control.

use std::path::Path;

use crate::sysfs;

/// Aura mode identifiers accepted by the firmware through the ASUS WMI method.
/// The numbering matches the vendor packet layout (mode 9 is unused).
pub const MODE_NAMES: &[(&str, u8)] = &[
    ("static", 0),
    ("breathe", 1),
    ("rainbow-cycle", 2),
    ("rainbow-wave", 3),
    ("star", 4),
    ("rain", 5),
    ("highlight", 6),
    ("laser", 7),
    ("ripple", 8),
    ("pulse", 10),
    ("comet", 11),
];

pub fn brightness() -> Result<u8, String> {
    let raw = sysfs::read_trim(&format!("{}/brightness", sysfs::LED_DIR))?;
    raw.parse()
        .map_err(|_| format!("unexpected backlight value in sysfs: '{raw}'"))
}

pub fn max_brightness() -> Result<u8, String> {
    let raw = sysfs::read_trim(&format!("{}/max_brightness", sysfs::LED_DIR))?;
    raw.parse()
        .map_err(|_| format!("unexpected max_brightness value in sysfs: '{raw}'"))
}

pub fn has_rgb() -> bool {
    Path::new(&format!("{}/kbd_rgb_mode", sysfs::LED_DIR)).exists()
}

pub fn set_brightness(level: u8) -> Result<(), String> {
    let max = max_brightness()?;

    if level > max {
        return Err(format!("brightness {level} out of range 0..={max}"));
    }

    sysfs::write_str(
        &format!("{}/brightness", sysfs::LED_DIR),
        &level.to_string(),
    )
}

/// Accepts either a numeric mode (0..=11) or one of `MODE_NAMES`.
pub fn parse_mode(value: &str) -> Result<u8, String> {
    if let Some((_, id)) = MODE_NAMES.iter().find(|(name, _)| *name == value) {
        return Ok(*id);
    }

    if let Ok(id) = value.parse::<u8>() {
        if id <= 11 {
            return Ok(id);
        }
    }

    let known: Vec<&str> = MODE_NAMES.iter().map(|(name, _)| *name).collect();
    Err(format!(
        "unknown mode '{value}'; use 0..=11 or one of: {}",
        known.join(", ")
    ))
}

/// Driver ABI: "cmd mode red green blue speed".
///
/// `cmd` 0 applies the effect for the current session, `cmd` 1 also stores it
/// in the firmware settings. `speed` is 0 (slow), 1 (medium) or 2 (fast).
pub fn set_rgb(
    mode: u8,
    red: u8,
    green: u8,
    blue: u8,
    speed: u8,
    save: bool,
) -> Result<(), String> {
    if !has_rgb() {
        return Err(
            "this kernel does not expose kbd_rgb_mode; keyboard RGB is unavailable".to_string(),
        );
    }

    let cmd = u8::from(save);
    let payload = format!("{cmd} {mode} {red} {green} {blue} {speed}");
    sysfs::write_str(&format!("{}/kbd_rgb_mode", sysfs::LED_DIR), &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_modes() {
        assert_eq!(parse_mode("static").unwrap(), 0);
        assert_eq!(parse_mode("rainbow-wave").unwrap(), 3);
        assert_eq!(parse_mode("pulse").unwrap(), 10);
        assert_eq!(parse_mode("comet").unwrap(), 11);
    }

    #[test]
    fn parses_numeric_modes() {
        assert_eq!(parse_mode("0").unwrap(), 0);
        assert_eq!(parse_mode("8").unwrap(), 8);
        assert_eq!(parse_mode("11").unwrap(), 11);
    }

    #[test]
    fn rejects_unknown_modes() {
        assert!(parse_mode("disco").is_err());
        assert!(parse_mode("12").is_err());
        assert!(parse_mode("255").is_err());
    }
}
