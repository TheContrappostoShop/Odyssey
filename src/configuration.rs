use config::{Config, ConfigError, File, FileFormat};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrinterConfig {
    pub serial: String,
    pub baud: u32,
    pub frame_buffer: String,
    pub fb_bit_depth: u8,
    pub fb_chunk_size: u8,
    pub max_z: f32,
    pub z_lift: f32,
}

impl Default for PrinterConfig {
    fn default() -> Self {
        PrinterConfig {
            serial: Default::default(),
            baud: 250000,
            frame_buffer: Default::default(),
            fb_bit_depth: 5,
            fb_chunk_size: 16,
            max_z: 350.0,
            z_lift: 10.0,
        }
    }
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

impl Default for GcodeConfig {
    fn default() -> Self {
        GcodeConfig {
            boot: String::from("G90"),
            shutdown: String::from("M84"),
            home_command: String::from("G28"),
            move_command: String::from("G0 Z{z} F200"),
            print_start: String::from("G28"),
            print_end: String::from("G0 Z{max_z}"),
            cure_start: String::from("SET_PIN PIN=led_array VALUE=650"),
            cure_end: String::from("SET_PIN PIN=led_array VALUE=0"),
            sync_message: String::from("Z_move_comp"),
        }
    }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub printer: PrinterConfig,
    pub gcode: GcodeConfig,
}

impl Configuration {
    pub fn load(config_file: String) -> Result<Self, ConfigError> {
        let default_settings: Configuration = Default::default();
        let s = Config::builder()
            .add_source(File::from_str(serde_yaml::to_string(&default_settings).unwrap().as_str(), FileFormat::Yaml))
            .add_source(File::with_name(config_file.as_str()).required(true))
            .build()?;

        s.try_deserialize()
    }
}
