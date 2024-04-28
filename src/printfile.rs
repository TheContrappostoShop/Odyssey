use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LocationCategory {
    Local,
    Usb,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileData {
    pub path: String,
    pub name: String,
    pub last_modified: Option<u128>,
    pub location_category: LocationCategory,
    pub parent_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrintMetadata {
    pub file_data: FileData,
    pub used_material: f32,
    pub print_time: f32,
    pub layer_height: f32,
    pub layer_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub file_name: String,
    pub data: Vec<u8>,
    pub exposure_time: f32,
}

#[async_trait]
pub trait PrintFile {
    fn from_file(file_data: FileData) -> Self
    where
        Self: Sized;
    async fn get_layer_data(&mut self, index: usize) -> Option<Layer>;
    fn get_layer_count(&self) -> usize;
    fn get_layer_height(&self) -> f32;
    fn get_metadata(&self) -> PrintMetadata;
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
