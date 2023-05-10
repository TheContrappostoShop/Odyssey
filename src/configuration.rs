use config::{Config, ConfigError, File};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrinterConfig {
    pub serial: String,
    pub baudrate: u32,
    pub frame_buffer: String,
    pub fb_bit_depth: u8,
    pub fb_chunk_size: u8,
    pub max_z: f32,
    pub z_lift: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GcodeConfig {
    pub boot: String,
    pub shutdown: String,
    pub home_command: String,
    pub move_command: String,
    pub print_start: String,
    pub print_end: String,
    pub cure_start: String,
    pub cure_end: String,
    pub sync_message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiConfig {
    pub upload_path: String,
    pub usb_glob: String,
    pub port: u16
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub printer: PrinterConfig,
    pub gcode: GcodeConfig,
    pub api: ApiConfig,
}

impl Configuration {
    pub fn load(config_file: String) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name(config_file.as_str()).required(true))
            .build()?;

        s.try_deserialize()
    }
}
