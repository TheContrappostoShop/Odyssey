use std::io::Error;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::api_objects::{FileData, FileMetadata, PrintMetadata};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub file_name: String,
    pub data: Vec<u8>,
    pub exposure_time: f32,
}

#[async_trait]
pub trait PrintFile {
    fn from_file(file_data: FileMetadata) -> Self
    where
        Self: Sized;
    async fn get_layer_data(&mut self, index: usize) -> Option<Layer>;
    fn get_layer_count(&self) -> usize;
    fn get_layer_height(&self) -> f32;
    fn get_metadata(&self) -> PrintMetadata;
    fn get_thumbnail(&mut self) -> Result<FileData, Error>;
    // Optional fields not present in every file type
    fn get_lift(&self) -> Option<f32> {
        None
    }
    fn get_up_speed(&self) -> Option<f32> {
        None
    }
    fn get_down_speed(&self) -> Option<f32> {
        None
    }
    fn get_wait_after_exposure(&self) -> Option<f32> {
        None
    }
    fn get_wait_before_exposure(&self) -> Option<f32> {
        None
    }
}
