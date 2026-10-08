#![allow(dead_code)]
#![allow(unused_imports)]
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LinuxCapabilityClass {
    FullControl,
    ProfileOnly,
    TelemetryOnly,
    UnsupportedControl,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinuxCapabilityAssessment {
    pub capability_class: LinuxCapabilityClass,
    pub supports_manual_fan_control: bool,
    pub supports_profile_control: bool,
    pub supports_telemetry: bool,
    pub reason: String,
}

impl LinuxCapabilityAssessment {
    pub fn capability_key(&self) -> &str {
        match self.capability_class {
            LinuxCapabilityClass::FullControl => "full-control",
            LinuxCapabilityClass::ProfileOnly => "profile-only",
            LinuxCapabilityClass::TelemetryOnly => "telemetry-only",
            LinuxCapabilityClass::UnsupportedControl => "unsupported-control",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelCapabilities {
    pub product_id: String,
    pub model_name: String,
    pub model_year: u32,
    pub family: String,
    
    // Fan Control
    pub supports_fan_control_wmi: bool,
    pub supports_fan_control_ec: bool,
    pub supports_fan_curves: bool,
    pub supports_independent_fan_curves: bool,
    pub supports_rpm_readback: bool,
    pub fan_zone_count: u32,
    pub max_fan_speed_percent: u32,
    pub min_fan_speed_percent: u32,
    
    // Performance Modes
    pub supports_performance_modes: bool,
    pub performance_modes: Vec<String>,
    pub allow_decoupled_wmi_thermal_policy_fallback: bool,
    
    // GPU
    pub has_mux_switch: bool,
    pub supports_gpu_power_boost: bool,
    pub supports_advanced_optimus: bool,
    
    // Lighting
    pub has_keyboard_backlight: bool,
    pub has_four_zone_rgb: bool,
    pub has_per_key_rgb: bool,
    pub has_light_bar: bool,
    
    // Power / Undervolt
    pub supports_undervolt: bool,
    pub supports_tcc_offset: bool,
    pub supports_power_limits: bool,
    pub supports_battery_care: bool,
    
    // Low-level OS Assessment
    pub linux_assessment: Option<LinuxCapabilityAssessment>,
    
    pub notes: String,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            product_id: "DEFAULT".to_string(),
            model_name: "Unknown HP System".to_string(),
            model_year: 2023,
            family: "HP".to_string(),
            
            supports_fan_control_wmi: true,
            supports_fan_control_ec: false,
            supports_fan_curves: true,
            supports_independent_fan_curves: true,
            supports_rpm_readback: true,
            fan_zone_count: 2,
            max_fan_speed_percent: 100,
            min_fan_speed_percent: 0,
            
            supports_performance_modes: true,
            performance_modes: vec!["Default".to_string(), "Performance".to_string(), "Cool".to_string()],
            allow_decoupled_wmi_thermal_policy_fallback: false,
            
            has_mux_switch: false,
            supports_gpu_power_boost: true,
            supports_advanced_optimus: false,
            
            has_keyboard_backlight: true,
            has_four_zone_rgb: true,
            has_per_key_rgb: false,
            has_light_bar: false,
            
            supports_undervolt: true,
            supports_tcc_offset: true,
            supports_power_limits: true,
            supports_battery_care: true,
            linux_assessment: None,
            
            notes: "".to_string(),
        }
    }
}

impl ModelCapabilities {
    pub fn is_desktop(&self) -> bool {
        self.family.eq_ignore_ascii_case("desktop")
    }

    /// Conservative profile for an HP desktop whose board is not in the database yet.
    /// Without it an unknown desktop inherits the laptop defaults (battery care, keyboard
    /// backlight, Intel undervolt, GPU boost), none of which exist on a tower.
    fn generic_desktop() -> Self {
        Self {
            product_id: "DESKTOP".to_string(),
            model_name: "OMEN Desktop".to_string(),
            family: "Desktop".to_string(),
            supports_fan_control_wmi: false,
            supports_fan_control_ec: false,
            supports_fan_curves: false,
            supports_rpm_readback: false,
            supports_gpu_power_boost: false,
            has_keyboard_backlight: false,
            has_four_zone_rgb: false,
            supports_undervolt: false,
            supports_tcc_offset: false,
            supports_power_limits: false,
            supports_battery_care: false,
            notes: "Unrecognised HP desktop board: only the HID lighting controller and telemetry are enabled until the board is added to the database.".to_string(),
            ..Self::default()
        }
    }
}

pub struct LinuxCapabilityClassifier;

impl LinuxCapabilityClassifier {
    pub fn assess(is_root: bool, board_id: &str, _model: &str) -> LinuxCapabilityAssessment {
        let is_wmaa_abort_prone = Self::is_wmaa_abort_prone_board(board_id);
        
        let has_ec_access = Path::new("/sys/kernel/debug/ec/ec0/io").exists();
        let has_hp_wmi_path = Path::new("/sys/devices/platform/hp-wmi").exists();
        // Check both underscore and hyphen naming (kernel version dependent)
        let has_thermal_profile =
            Path::new("/sys/devices/platform/hp-wmi/thermal_profile").exists() ||
            Path::new("/sys/devices/platform/hp-wmi/thermal-profile").exists();
        let has_platform_profile =
            Path::new("/sys/devices/platform/hp-wmi/platform_profile").exists() ||
            Path::new("/sys/devices/platform/hp-wmi/platform-profile").exists();
        let has_acpi_platform_profile = Path::new("/sys/firmware/acpi/platform_profile").exists();
        let has_fan1_output = Path::new("/sys/devices/platform/hp-wmi/fan1_output").exists();
        let has_fan2_output = Path::new("/sys/devices/platform/hp-wmi/fan2_output").exists();
        
        // Basic hwmon checks
        let mut has_hwmon_fan_access = false;
        if let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") {
            for entry in entries.filter_map(Result::ok) {
                if entry.path().join("pwm1_enable").exists() {
                    has_hwmon_fan_access = true;
                }
            }
        }
        
        let has_manual_fan_control = has_ec_access || has_fan1_output || has_fan2_output;
        let has_profile_control = has_thermal_profile || has_platform_profile || has_acpi_platform_profile || has_hwmon_fan_access;
        let has_telemetry = has_hp_wmi_path || has_manual_fan_control || has_profile_control;
        
        if is_wmaa_abort_prone && has_manual_fan_control {
            let mut reason = "Board 8BCD has field reports of ACPI WMAA/WHCM aborts where WMI-backed fan, RGB, and battery paths can report success without hardware effect. Treat visible manual/profile fan paths as degraded until an effective write/readback check proves control.".to_string();
            if !is_root {
                reason.push_str(" Run with sudo for write/readback validation.");
            }
            return LinuxCapabilityAssessment {
                capability_class: if has_profile_control { LinuxCapabilityClass::ProfileOnly } else { LinuxCapabilityClass::TelemetryOnly },
                supports_manual_fan_control: false,
                supports_profile_control: has_profile_control,
                supports_telemetry: true,
                reason,
            };
        }

        if has_manual_fan_control {
            let mut reason = if has_hwmon_fan_access {
                "Manual fan control is available through hwmon pwm/fan targets."
            } else if has_fan1_output {
                "Manual fan control is available through hp-wmi fan output files."
            } else {
                "Manual fan control is available through legacy EC access."
            }.to_string();
            
            if !is_root {
                reason.push_str(" Run with sudo to use write-capable controls.");
            }
            
            return LinuxCapabilityAssessment {
                capability_class: LinuxCapabilityClass::FullControl,
                supports_manual_fan_control: true,
                supports_profile_control: has_profile_control,
                supports_telemetry: true,
                reason,
            };
        }

        if has_profile_control {
            let mut reason = "Thermal/platform profile control is available, but firmware does not expose manual fan target/output interfaces on this board.".to_string();
            if !is_root {
                reason.push_str(" Run with sudo to apply profile changes.");
            }
            if is_wmaa_abort_prone {
                reason.push_str(" Board 8BCD is currently treated as degraded profile control because field diagnostics show ACPI WMAA/WHCM aborts.");
            }
            
            return LinuxCapabilityAssessment {
                capability_class: LinuxCapabilityClass::ProfileOnly,
                supports_manual_fan_control: false,
                supports_profile_control: true,
                supports_telemetry: true,
                reason,
            };
        }

        if has_telemetry {
            return LinuxCapabilityAssessment {
                capability_class: LinuxCapabilityClass::TelemetryOnly,
                supports_manual_fan_control: false,
                supports_profile_control: false,
                supports_telemetry: true,
                reason: "Telemetry paths are present, but no writable EC, hp-wmi, hwmon target, or platform profile control interface is exposed.".to_string(),
            };
        }

        LinuxCapabilityAssessment {
            capability_class: LinuxCapabilityClass::UnsupportedControl,
            supports_manual_fan_control: false,
            supports_profile_control: false,
            supports_telemetry: false,
            reason: "No supported Linux control interface was detected. This is usually a kernel exposure gap or missing hp-wmi/ec_sys support.".to_string(),
        }
    }

    pub fn is_wmaa_abort_prone_board(board_id: &str) -> bool {
        let id_upper = board_id.trim().to_uppercase();
        get_hardware_database().wmaa_abort_prone_boards.contains(&id_upper)
    }

    /// Returns true when the board_id appears in the community-verified list.
    ///
    /// A "verified" board is one where a user has run the full checklist
    /// (modes, fans, power gain, GPU power, graphics, lighting) and confirmed
    /// every control behaves as expected.  Boards not in this list still receive
    /// full daemon service; the GUI simply shows a one-time banner asking the
    /// user to file a verification issue.
    pub fn is_board_verified(board_id: &str) -> bool {
        let id_upper = board_id.trim().to_uppercase();
        get_hardware_database().verified_boards.contains(&id_upper)
    }
}

/// Extracts the chassis size from an HP desktop product name and maps it to the
/// placeholder database id, e.g. "OMEN by HP 45L Gaming Desktop GT22-0xxx" -> "DESKTOP-45L".
fn desktop_size_id(product_lower: &str) -> Option<String> {
    product_lower.split_whitespace().find_map(|tok| {
        let digits = tok.strip_suffix('l')?;
        (digits.len() == 2 && digits.chars().all(|c| c.is_ascii_digit()))
            .then(|| format!("DESKTOP-{}L", digits))
    })
}

pub fn detect(board_id: &str, product_name: &str, cpu_model: &str) -> ModelCapabilities {
    let prod_lower = product_name.to_lowercase();
    let mut cap = get_known_model(board_id).or_else(|| {
        // Fallback by product name if exact board_id is not found
        get_all_models().iter().find(|m| prod_lower.contains(&m.model_name.to_lowercase())).cloned()
    }).or_else(|| {
        // HP names desktops "OMEN by HP 45L Gaming Desktop ...", which never contains the
        // database's "OMEN 45L Desktop", so match them by chassis size instead.
        if !(prod_lower.contains("desktop") || prod_lower.contains("tower")) {
            return None;
        }
        desktop_size_id(&prod_lower)
            .and_then(|id| get_known_model(&id))
            .or_else(|| Some(ModelCapabilities::generic_desktop()))
    }).unwrap_or_default();
    
    let cpu_upper = cpu_model.to_uppercase();
    let is_hx = cpu_upper.contains("HX");
    let is_amd = cpu_upper.contains("AMD") || cpu_upper.contains("RYZEN");
    
    if cap.family.to_uppercase().contains("VICTUS")
        && !is_hx && !is_amd {
            cap.supports_undervolt = false;
            cap.supports_tcc_offset = false;
            cap.supports_power_limits = false;
        }
    cap
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HardwareDatabase {
    pub models: Vec<ModelCapabilities>,
    pub verified_boards: Vec<String>,
    pub wmaa_abort_prone_boards: Vec<String>,
}

fn get_hardware_database() -> &'static HardwareDatabase {
    static DB: std::sync::OnceLock<HardwareDatabase> = std::sync::OnceLock::new();
    DB.get_or_init(|| {
        serde_json::from_str(include_str!("boards.json")).unwrap_or_else(|e| {
            log::error!("Failed to parse boards.json: {}", e);
            HardwareDatabase {
                models: Vec::new(),
                verified_boards: Vec::new(),
                wmaa_abort_prone_boards: Vec::new(),
            }
        })
    })
}

fn get_all_models() -> &'static [ModelCapabilities] {
    &get_hardware_database().models
}

fn get_known_model(board_id: &str) -> Option<ModelCapabilities> {
    let id_upper = board_id.to_uppercase();
    get_all_models().iter().find(|m| m.product_id == id_upper).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_board_8786_capabilities() {
        let cap = detect("8786", "OMEN Laptop 15-en0xxx", "AMD Ryzen 5 4600H");
        assert_eq!(cap.product_id, "8786");
        assert!(cap.supports_performance_modes);
        assert!(cap.allow_decoupled_wmi_thermal_policy_fallback);
        assert!(!cap.supports_undervolt); // AMD should not have undervolt enabled
        assert!(LinuxCapabilityClassifier::is_wmaa_abort_prone_board("8786"));
    }

    const GT22_PRODUCT: &str = "OMEN by HP 45L Gaming Desktop GT22-0xxx";

    fn assert_tower_without_laptop_features(cap: &ModelCapabilities) {
        assert!(cap.is_desktop());
        assert!(!cap.has_keyboard_backlight);
        assert!(!cap.supports_battery_care);
        assert!(!cap.supports_undervolt && !cap.supports_tcc_offset);
    }

    #[test]
    fn test_board_8949_omen_45l_desktop() {
        let cap = detect("8949", GT22_PRODUCT, "AMD Ryzen 9 5900X 12-Core Processor");
        assert_eq!(cap.product_id, "8949");
        assert_tower_without_laptop_features(&cap);
    }

    #[test]
    fn test_board_8949_matches_without_product_name() {
        // fan/mod.rs calls detect(board_id, "", "") so the board id alone must be enough.
        assert_eq!(detect("8949", "", "").product_id, "8949");
        assert_tower_without_laptop_features(&detect("8949", "", ""));
    }

    #[test]
    fn test_desktop_size_id_parses_hp_product_names() {
        assert_eq!(desktop_size_id("omen by hp 45l gaming desktop gt22-0xxx").as_deref(), Some("DESKTOP-45L"));
        assert_eq!(desktop_size_id("omen 25l desktop gt12-0xxx").as_deref(), Some("DESKTOP-25L"));
        assert_eq!(desktop_size_id("omen by hp gaming desktop"), None);
        assert_eq!(desktop_size_id("hp 125l"), None);
    }

    #[test]
    fn test_unknown_board_of_known_desktop_size_uses_size_entry() {
        let cap = detect("FFFF", "OMEN by HP 30L Gaming Desktop GT13-0xxx", "");
        assert_eq!(cap.product_id, "DESKTOP-30L");
        assert!(cap.is_desktop());
    }

    #[test]
    fn test_unknown_desktop_does_not_inherit_laptop_defaults() {
        let cap = detect("FFFF", "OMEN Gaming Desktop Something", "");
        assert_eq!(cap.product_id, "DESKTOP");
        assert_tower_without_laptop_features(&cap);
    }

    #[test]
    fn test_unknown_laptop_keeps_laptop_defaults() {
        let cap = detect("FFFF", "OMEN by HP Laptop 99-zz0xxx", "");
        assert_eq!(cap.product_id, "DEFAULT");
        assert!(!cap.is_desktop());
        assert!(cap.supports_battery_care);
    }

    #[test]
    fn test_desktop_entries_never_claim_laptop_only_features() {
        for m in get_all_models().iter().filter(|m| m.is_desktop()) {
            assert!(!m.supports_battery_care, "{} claims battery care", m.product_id);
            assert!(!m.has_keyboard_backlight, "{} claims a keyboard backlight", m.product_id);
        }
    }
}
