use config::{Config, ConfigError, File, FileFormat};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
struct Printer {
    serial: String,
    baud: u32,
    frame_buffer: String,
}

impl Default for Printer {
    fn default() -> Self {
        Printer {
            serial: Default::default(),
            baud: 250000,
            frame_buffer: Default::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Gcode {
    boot: String,
    shutdown: String,
    home_command: String,
    move_command: String,
    print_start: String,
    print_end: String,
    cure_start: String,
    cure_end: String,
    sync_message: String,
}

impl Default for Gcode {
    fn default() -> Self {
        Gcode {
            boot: String::from("G90"),
            shutdown: String::from("M84"),
            home_command: String::from("G28"),
            move_command: String::from("G0 Z{position} F200"),
            print_start: String::from("G28"),
            print_end: String::from("G0 Z{max_position}"),
            cure_start: String::from("SET_PIN PIN=led_array VALUE=650"),
            cure_end: String::from("SET_PIN PIN=led_array VALUE=0"),
            sync_message: String::from("Z_move_comp")
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Settings {
    printer: Printer,
    gcode: Gcode,
}

impl Settings {
    pub fn load(config_file: String) -> Result<Self, ConfigError> {
        let default_settings: Settings = Default::default();
        let s = Config::builder()
            .add_source(File::from_str(serde_yaml::to_string(&default_settings).unwrap().as_str(), FileFormat::Yaml))
            .add_source(File::with_name(config_file.as_str()).required(true))
            .build()?;

        return s.try_deserialize();
    }
}