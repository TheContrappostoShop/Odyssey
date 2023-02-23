use std::{io::Read, fs::File};

use itertools::Itertools;
use config::{Config, ConfigError, File as ConfigFile, FileFormat};
use serde::{Deserialize};
use zip::ZipArchive;

const CONFIG_FILE: &str = "config.ini";

/// PrintConfig object encompassing the fields stored in `config.ini` inside a `.sl1` file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct PrintConfig {
    action: String,
    exp_time: f32,
    exp_time_first: f32,
    exp_user_profile: usize,
    file_creation_timestamp: String,
    hollow: usize,
    job_dir: String,
    layer_height: f32,
    material_name: String,
    num_fade: usize,
    num_fast: usize,
    num_slow: usize,
    print_profile: String,
    print_time: f32,
    printer_model: String,
    printer_profile: String,
    printer_variant: String,
    prusa_slicer_version: String,
    used_material: f32,
}

impl PrintConfig {
    /// Compute the exposure time of the given frame index, based on the PrintConfig
    fn exposure_time(&self, index: usize) -> f32 {
        if index<self.num_fade {
            let fade_rate = (self.num_fade - index) as f32 / self.num_fade as f32;
            return self.exp_time + (self.exp_time_first-self.exp_time) * (fade_rate);
        } else {
            return self.exp_time;
        }
    }
    
    /// Read the PrintConfig object in from a string representing the .ini contents
    fn from_string(contents: String) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(ConfigFile::from_str(contents.as_str(), FileFormat::Ini))
            .build()?;

        return s.try_deserialize();
    }
}

pub struct Layer {
    pub file_name: String,
    pub data: Vec<u8>,
    pub exposure_time: f32
}

/// The sliced .sl1-format model, with the internal config and the full archive contents
pub struct Sl1 {
    config: PrintConfig,
    archive: ZipArchive<File>,
    frame_list: Vec<String>,
}

impl<'a> Sl1 {
    /// Instantiate the Sl1 from the given file
    pub fn from_file(file_name: String) -> Sl1 {
        let file = File::open(file_name.clone()).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();

        let mut config_contents = String::new();

        archive.by_name(CONFIG_FILE).unwrap().read_to_string(&mut config_contents).expect("Unable to read print config.ini");

        let config = PrintConfig::from_string(config_contents).unwrap();

        Sl1 {
            frame_list: archive.file_names()
                .map(|name| String::from(name))
                .filter(|name| name.ends_with(".png") && !name.contains('/'))
                .sorted()
                .collect(),
            archive: archive,
            config: config,
        }
    }

    pub async fn get_layer_data(&mut self, index: usize) -> Option<Layer> {
        if index<self.frame_list.len() {

            let frame_file = self.archive.by_name(self.frame_list[index].as_str());

            if frame_file.is_ok() {
                let mut frame_file = frame_file.unwrap();
                let mut ret: Vec<u8> = Vec::new();

                frame_file.read_to_end(&mut ret).expect("Error reading file from archive");

                return Some(Layer {
                    file_name: self.frame_list[index].clone(),
                    data: ret,
                    exposure_time: self.config.exposure_time(index).clone()
                });
            }
        }
        return None;
    }

    // Will be used to report status in the future
    #[allow(dead_code)]
    pub fn get_frame_count(& self) -> usize {
        return self.frame_list.len();
    }

    pub fn get_layer_height(& self) -> f32 {
        return self.config.layer_height.clone();
    }
}



