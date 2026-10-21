//! Platform thermal profile and custom fan curves.
//!
//! Fan curves are exposed by the `asus_custom_fan_curve` hwmon device:
//!
//! * `pwmN_enable`: 1 = custom curve active, 2 = firmware automatic, 3 = factory reset
//! * `pwmN_auto_point{1..8}_{temp,pwm}`: curve points (Celsius / 0..255)
//!
//! Offsets are 1-based and follow the hwmon numbering: 1 = CPU, 2 = GPU.

use std::path::PathBuf;

use crate::sysfs;

pub const CURVE_POINTS: usize = 8;
pub const MAX_TEMP: i32 = 120;

pub fn profile() -> Result<String, String> {
    sysfs::read_trim(sysfs::PLATFORM_PROFILE)
}

pub fn profile_choices() -> Result<Vec<String>, String> {
    let raw = sysfs::read_trim(sysfs::PROFILE_CHOICES)?;
    Ok(raw.split_whitespace().map(str::to_string).collect())
}

pub fn set_profile(name: &str) -> Result<(), String> {
    let choices = profile_choices()?;

    if !choices.iter().any(|choice| choice == name) {
        return Err(format!(
            "invalid profile '{name}'; available: {}",
            choices.join(", ")
        ));
    }

    sysfs::write_str(sysfs::PLATFORM_PROFILE, name)
}

fn curve_dir() -> Result<PathBuf, String> {
    sysfs::find_hwmon("asus_custom_fan_curve")
        .ok_or_else(|| "hwmon 'asus_custom_fan_curve' not found".to_string())
}

fn fan_dir() -> Result<PathBuf, String> {
    sysfs::find_hwmon("asus").ok_or_else(|| "hwmon 'asus' not found".to_string())
}

/// Current fan speeds as (label, rpm).
pub fn fan_speeds() -> Result<Vec<(String, i64)>, String> {
    let dir = fan_dir()?;
    let mut fans = Vec::new();

    for index in 1..=2 {
        let input_path = dir.join(format!("fan{index}_input"));

        if !input_path.exists() {
            continue;
        }

        let label = std::fs::read_to_string(dir.join(format!("fan{index}_label")))
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|_| format!("fan{index}"));
        let rpm = sysfs::read_trim(&sysfs::path_str(&input_path))?
            .parse::<i64>()
            .unwrap_or(-1);

        fans.push((label, rpm));
    }

    Ok(fans)
}

pub fn curve_exists(fan_index: u8) -> bool {
    curve_dir()
        .map(|dir| dir.join(format!("pwm{fan_index}_enable")).exists())
        .unwrap_or(false)
}

/// Returns (custom_curve_active, points) for the given fan.
pub fn curve(fan_index: u8) -> Result<(bool, Vec<(i32, i32)>), String> {
    let dir = curve_dir()?;
    let enable_path = dir.join(format!("pwm{fan_index}_enable"));

    if !enable_path.exists() {
        return Err(format!("fan {fan_index} has no configurable curve"));
    }

    let enabled = sysfs::read_trim(&sysfs::path_str(&enable_path))? == "1";
    let mut points = Vec::with_capacity(CURVE_POINTS);

    for point in 1..=CURVE_POINTS {
        let temp_path = dir.join(format!("pwm{fan_index}_auto_point{point}_temp"));
        let pwm_path = dir.join(format!("pwm{fan_index}_auto_point{point}_pwm"));
        let temp = sysfs::read_trim(&sysfs::path_str(&temp_path))?
            .parse::<i32>()
            .map_err(|_| format!("invalid temperature in {}", sysfs::path_str(&temp_path)))?;
        let pwm = sysfs::read_trim(&sysfs::path_str(&pwm_path))?
            .parse::<i32>()
            .map_err(|_| format!("invalid pwm in {}", sysfs::path_str(&pwm_path)))?;

        points.push((temp, pwm));
    }

    Ok((enabled, points))
}

pub fn validate_points(points: &[(i32, i32)]) -> Result<(), String> {
    if points.len() != CURVE_POINTS {
        return Err(format!(
            "a curve needs exactly {CURVE_POINTS} points, got {}",
            points.len()
        ));
    }

    let mut previous_temp = -1;

    for (temp, pwm) in points {
        if *temp <= previous_temp {
            return Err(format!(
                "temperatures must increase strictly ({temp} came after {previous_temp})"
            ));
        }
        if !(0..=MAX_TEMP).contains(temp) {
            return Err(format!("temperature {temp} out of range 0..={MAX_TEMP}"));
        }
        if !(0..=255).contains(pwm) {
            return Err(format!("pwm {pwm} out of range 0..=255"));
        }
        previous_temp = *temp;
    }

    Ok(())
}

pub fn set_curve(fan_index: u8, points: &[(i32, i32)]) -> Result<(), String> {
    validate_points(points)?;

    let dir = curve_dir()?;
    let enable_path = dir.join(format!("pwm{fan_index}_enable"));
    let enable_str = sysfs::path_str(&enable_path);

    if !enable_path.exists() {
        return Err(format!("fan {fan_index} has no configurable curve"));
    }

    // Disable first: the driver requires an explicit enable after new points.
    sysfs::write_str(&enable_str, "2")?;

    for (offset, (temp, pwm)) in points.iter().enumerate() {
        let point = offset + 1;
        let temp_path = dir.join(format!("pwm{fan_index}_auto_point{point}_temp"));
        let pwm_path = dir.join(format!("pwm{fan_index}_auto_point{point}_pwm"));

        sysfs::write_str(&sysfs::path_str(&temp_path), &temp.to_string())?;
        sysfs::write_str(&sysfs::path_str(&pwm_path), &pwm.to_string())?;
    }

    sysfs::write_str(&enable_str, "1")
}

pub fn set_fan_auto(fan_index: u8) -> Result<(), String> {
    let dir = curve_dir()?;
    sysfs::write_str(
        &sysfs::path_str(&dir.join(format!("pwm{fan_index}_enable"))),
        "2",
    )
}

pub fn reset_fan_curve(fan_index: u8) -> Result<(), String> {
    let dir = curve_dir()?;
    sysfs::write_str(
        &sysfs::path_str(&dir.join(format!("pwm{fan_index}_enable"))),
        "3",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_points() -> Vec<(i32, i32)> {
        vec![
            (40, 20),
            (50, 40),
            (55, 60),
            (60, 80),
            (65, 110),
            (70, 150),
            (80, 200),
            (90, 255),
        ]
    }

    #[test]
    fn accepts_a_valid_curve() {
        assert!(validate_points(&valid_points()).is_ok());
    }

    #[test]
    fn rejects_wrong_point_count() {
        let mut points = valid_points();
        points.pop();
        assert!(validate_points(&points).is_err());
    }

    #[test]
    fn rejects_non_monotonic_temperatures() {
        let mut points = valid_points();
        points[3].0 = points[2].0;
        assert!(validate_points(&points).is_err());
    }

    #[test]
    fn rejects_out_of_range_values() {
        let mut too_hot = valid_points();
        too_hot[7].0 = MAX_TEMP + 1;
        assert!(validate_points(&too_hot).is_err());

        let mut too_noisy = valid_points();
        too_noisy[0].1 = 256;
        assert!(validate_points(&too_noisy).is_err());
    }
}
