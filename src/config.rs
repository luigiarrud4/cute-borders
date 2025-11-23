// src/config.rs

use crate::logger::Logger;
use crate::util::get_file_path;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::fs;
use std::sync::Mutex;
use std::time::SystemTime;

struct ConfigState {
    config: Config,
    last_modified: Option<SystemTime>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Rule {
    #[serde(rename = "Match")]
    pub rule_match: RuleMatch,
    #[serde(default)]
    pub contains: Option<String>,
    pub active_border_color: String,
    pub inactive_border_color: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum RuleMatch {
    Global,
    Title,
    Class,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub rainbow_speed: Option<f32>,
    pub hide_tray_icon: Option<bool>,
    pub window_rules: Vec<Rule>,
}

static CONFIG: Lazy<Mutex<ConfigState>> = Lazy::new(|| Mutex::new(load_or_create_config()));

impl Config {
    pub fn get() -> Config {
        let mut state_guard = CONFIG.lock().unwrap();
        let config_path = get_file_path("config.yaml");
        if let Ok(metadata) = fs::metadata(&config_path) {
            if let Ok(modified_time) = metadata.modified() {
                if state_guard.last_modified.map_or(true, |last| modified_time > last) {
                    Logger::log("[CONFIG] config.yaml changed, reloading.");
                    *state_guard = load_or_create_config();
                }
            }
        }
        state_guard.config.clone()
    }

    pub fn read_for_gui() -> Config {
        let mut state_guard = CONFIG.lock().unwrap();
        *state_guard = load_or_create_config(); // Força a leitura
        state_guard.config.clone()
    }

    pub fn write_config(config_to_write: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = get_file_path("config.yaml");
        let yaml_string = serde_yaml::to_string(config_to_write)?;
        fs::write(config_path, yaml_string)?;
        
        let mut state_guard = CONFIG.lock().unwrap();
        state_guard.config = config_to_write.clone();
        state_guard.last_modified = Some(SystemTime::now());
        Ok(())
    }
}

fn load_or_create_config() -> ConfigState {
    let config_path = get_file_path("config.yaml");
    let config_to_return = if let Ok(config_str) = fs::read_to_string(&config_path) {
        match serde_yaml::from_str(&config_str) {
            Ok(config) => config,
            Err(e) => {
                Logger::log(&format!("[ERROR] Failed to parse config file: {:?}. Loading default.", e));
                create_default_config()
            }
        }
    } else {
        Logger::log("[INFO] Config file not found. Creating a default one.");
        let default_config = create_default_config();
        if let Ok(yaml_string) = serde_yaml::to_string(&default_config) {
            if let Err(e) = fs::write(&config_path, yaml_string) {
                Logger::log(&format!("[ERROR] Failed to write default config file: {:?}", e));
            }
        }
        default_config
    };
    let last_modified = fs::metadata(config_path).and_then(|m| m.modified()).ok();
    ConfigState {
        config: config_to_return,
        last_modified,
    }
}

fn create_default_config() -> Config {
    Config {
        rainbow_speed: Some(1.0),
        hide_tray_icon: Some(false),
        window_rules: vec![Rule {
            rule_match: RuleMatch::Global,
            contains: None,
            active_border_color: "rainbow".to_string(),
            inactive_border_color: "#444444".to_string(),
        }],
    }
}
