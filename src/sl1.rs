use std::{io::{BufReader, Read}, fs::File};

use png::Decoder;
use itertools::Itertools;
use config::{Config, ConfigError, File as ConfigFile, FileFormat};
use serde::{Deserialize};
use zip::{ZipArchive, read::ZipFile};

const CONFIG_FILE: &str = "config.ini";

/// PrintConfig object encompassing the fields stored in `config.ini` inside a `.sl1` file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// a single frame of the sliced model, including full image data and exposure time
pub struct Frame {
    pub file_name: String,
    pub buffer: Vec<u8>,
    pub exposure_time: f32,
}

impl Frame {
    /// Load a frame object directly from a ZipFile, and give it the provided exposure time
    fn from_zip_file(mut file: ZipFile, exposure_time: f32) -> Frame {

        let file_path = file.name().to_string();

        let mut png_reader = Decoder::new(file).read_info().unwrap();

        let mut f = Frame { 
            file_name: file_path.clone(),
            buffer: vec![0;png_reader.output_buffer_size()], 
            exposure_time: exposure_time, 
        };
        
        if png_reader.next_frame(f.buffer.as_mut()).is_err() {
            panic!("Encountered an error reading {} from archive", file_path);
        }
        return f;
    }
}

/// The sliced .sl1-format model, with the internal config and the full archive contents
pub struct Sl1 {
    name: String,
    config: PrintConfig,
    archive: ZipArchive<File>,
    frame_list: Vec<String>,
}

impl<'a> Sl1 {
    /// Provide an iterable to extract the frame images as needed from the archive
    pub fn iter(&mut self) -> Sl1Iter {
        Sl1Iter{
            index: 0,
            sl1: self
        }
    }

    /// Instantiate the Sl1 from the given file
    pub fn from_file(file_name: String) -> Sl1 {
        let file = File::open(file_name.clone()).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();

        let mut config_contents = String::new();

        archive.by_name(CONFIG_FILE).unwrap().read_to_string(&mut config_contents);

        let config = PrintConfig::from_string(config_contents).unwrap();

        println!("printconfig: {:?}", config);

        Sl1 {
            name: file_name,
            frame_list: archive.file_names()
                .map(|name| String::from(name))
                .filter(|name| name.ends_with(".png") && !name.contains('/'))
                .sorted()
                .collect(),
            archive: archive,
            config: config,
        }
    }

    pub fn get_frame(&mut self, index: usize) -> Option<Frame> {
        if index<self.frame_list.len() {

            let frame_file = self.archive.by_name(self.frame_list[index].as_str());

            if frame_file.is_ok() {
                return Some(Frame::from_zip_file(frame_file.unwrap(), self.config.exposure_time(index)));
            }
        }
        return None;
    }
}

/// Iterable for extracting the frames sequentially as needed.
pub struct Sl1Iter<'a> {
    index: usize,
    sl1: &'a mut Sl1
}

impl<'a> Iterator for Sl1Iter<'a> {
    type Item = Frame;

    /// Provide the next 
    fn next(&mut self) -> Option<Frame> {
        let f = self.sl1.get_frame(self.index);
        
        self.index+=1;

        return f;
    }

}


