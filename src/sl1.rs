use std::{
    fs::File,
    io::{Error, Read},
    path::Path,
};

use async_trait::async_trait;
use config::{Config, ConfigError, File as ConfigFile, FileFormat};
use itertools::Itertools;
use serde::Deserialize;
use zip::ZipArchive;

use crate::{
    api_objects::{FileData, FileMetadata, PrintMetadata, ThumbnailSize},
    printfile::{Layer, PrintFile},
};

const CONFIG_FILE: &str = "config.ini";
const THUMBNAIL_SMALL: &str = "thumbnail/thumbnail400x400.png";
const THUMBNAIL_LARGE: &str = "thumbnail/thumbnail800x480.png";

/// PrintConfig object encompassing the fields stored in `config.ini` inside a `.sl1` file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct PrintConfig {
    action: String,
    exp_time: f64,
    exp_time_first: f64,
    exp_user_profile: usize,
    file_creation_timestamp: String,
    hollow: usize,
    job_dir: String,
    layer_height: f64,
    material_name: String,
    num_fade: usize,
    num_fast: usize,
    num_slow: usize,
    print_profile: String,
    print_time: f64,
    printer_model: String,
    printer_profile: String,
    printer_variant: String,
    prusa_slicer_version: String,
    used_material: f64,
}

impl PrintConfig {
    /// Compute the exposure time of the given frame index, based on the PrintConfig
    fn exposure_time(&self, index: usize) -> f64 {
        if index < self.num_fade {
            let fade_rate = (self.num_fade - index) as f64 / self.num_fade as f64;
            self.exp_time + (self.exp_time_first - self.exp_time) * (fade_rate)
        } else {
            self.exp_time
        }
    }

    /// Read the PrintConfig object in from a string representing the .ini contents
    fn from_string(contents: String) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(ConfigFile::from_str(contents.as_str(), FileFormat::Ini))
            .build()?;

        s.try_deserialize()
    }
}

/// The sliced .sl1-format model, with the internal config and the full archive contents
pub struct Sl1 {
    config: PrintConfig,
    archive: ZipArchive<File>,
    frame_list: Vec<String>,
    metadata: PrintMetadata,
}

#[async_trait]
impl PrintFile for Sl1 {
    /// Instantiate the Sl1 from the given file
    fn from_file(file_data: FileMetadata) -> Sl1 {
        log::info!("Loading PrintFile from SL1 {:?}", file_data);

        let full_path = Path::new(file_data.parent_path.as_str()).join(file_data.path.as_str());

        let file = File::open(full_path).unwrap();

        let mut archive = ZipArchive::new(file).unwrap();

        let mut config_contents = String::new();

        archive
            .by_name(CONFIG_FILE)
            .unwrap()
            .read_to_string(&mut config_contents)
            .expect("Unable to read print config.ini");

        let config = PrintConfig::from_string(config_contents).unwrap();

        let frame_list: Vec<String> = archive
            .file_names()
            .map(String::from)
            .filter(|name| name.ends_with(".png") && !name.contains('/'))
            .sorted()
            .collect();

        let metadata = PrintMetadata {
            file_data,
            used_material: config.used_material,
            print_time: config.print_time,
            layer_height: config.layer_height,
            layer_height_microns: ((config.layer_height * 1000.0).trunc() as u32),
            layer_count: frame_list.len(),
        };

        Sl1 {
            frame_list,
            archive,
            config,
            metadata,
        }
    }

    async fn get_layer_data(&mut self, index: usize) -> Option<Layer> {
        if index < self.frame_list.len() {
            let frame_file = self.archive.by_name(self.frame_list[index].as_str());

            if let Ok(mut frame_file) = frame_file {
                let mut ret: Vec<u8> = Vec::new();

                frame_file
                    .read_to_end(&mut ret)
                    .expect("Error reading file from archive");

                return Some(Layer {
                    file_name: self.frame_list[index].clone(),
                    data: ret,
                    exposure_time: self.config.exposure_time(index),
                });
            }
        }
        None
    }

    fn get_layer_count(&self) -> usize {
        self.frame_list.len()
    }

    fn get_layer_height(&self) -> u32 {
        (self.config.layer_height * 1000.0).trunc() as u32
    }

    fn get_metadata(&self) -> PrintMetadata {
        self.metadata.clone()
    }

    fn get_thumbnail(&mut self, size: ThumbnailSize) -> Result<FileData, Error> {
        let mut thumbnail_file = match size {
            ThumbnailSize::Small => self.archive.by_name(THUMBNAIL_SMALL)?,
            ThumbnailSize::Large => self.archive.by_name(THUMBNAIL_LARGE)?,
        };

        let mut ret: Vec<u8> = Vec::new();

        thumbnail_file.read_to_end(&mut ret)?;

        Ok(FileData {
            name: "thumbnail.png".to_string(),
            data: ret,
        })
    }
}
