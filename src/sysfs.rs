//! Thin, dependency-free wrapper around the ASUS WMI sysfs interfaces
//! exposed by the Linux `asus-wmi` / `asus-armoury` drivers.

use std::fs;
use std::path::{Path, PathBuf};

pub const LED_DIR: &str = "/sys/class/leds/asus::kbd_backlight";
pub const PLATFORM_PROFILE: &str = "/sys/firmware/acpi/platform_profile";
pub const PROFILE_CHOICES: &str = "/sys/firmware/acpi/platform_profile_choices";
pub const DMI_PRODUCT: &str = "/sys/class/dmi/id/product_name";
pub const DMI_VENDOR: &str = "/sys/class/dmi/id/sys_vendor";

/// Reads a sysfs attribute and strips the trailing newline.
pub fn read_trim(path: &str) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|error| format!("cannot read {path}: {error}"))
}

/// Writes a single value to a sysfs attribute.
pub fn write_str(path: &str, value: &str) -> Result<(), String> {
    fs::write(path, value).map_err(|error| format!("cannot write {path}: {error}"))
}

/// Effective UID, read from /proc without external crates.
pub fn euid() -> u32 {
    let status = match fs::read_to_string("/proc/self/status") {
        Ok(status) => status,
        Err(_) => return 0,
    };

    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let mut fields = rest.split_whitespace();
            let _real = fields.next();

            if let Some(effective) = fields.next() {
                return effective.parse().unwrap_or(0);
            }
        }
    }

    0
}

/// Finds a hwmon device by its `name` attribute.
pub fn find_hwmon(name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;

    for entry in entries.flatten() {
        let dir = entry.path();

        if let Ok(content) = fs::read_to_string(dir.join("name")) {
            if content.trim() == name {
                return Some(dir);
            }
        }
    }

    None
}

pub fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Refuses to run on machines that do not expose the expected interfaces.
pub fn check_platform() -> Result<(), String> {
    if !Path::new(LED_DIR).exists() && !Path::new(PLATFORM_PROFILE).exists() {
        return Err(
            "no ASUS sysfs interfaces found; this tool requires the asus-wmi/asus-armoury \
             kernel drivers"
                .to_string(),
        );
    }

    let vendor = read_trim(DMI_VENDOR).unwrap_or_default();

    if !vendor.to_uppercase().contains("ASUS") {
        return Err(format!(
            "detected vendor '{vendor}': tufctl only supports ASUS laptops"
        ));
    }

    Ok(())
}
