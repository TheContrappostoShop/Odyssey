use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum LocationCategory {
    Local,
    Usb,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct FileData {
    pub path: String,
    pub name: String,
    pub last_modified: Option<u64>,
    pub file_size: Option<u64>,
    pub location_category: LocationCategory,
    pub parent_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrintMetadata {
    pub file_data: FileData,
    pub used_material: f32,
    pub print_time: f32,
    pub layer_height: f32,
    pub layer_count: usize,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Object)]
pub struct PhysicalState {
    pub z: f32,
    pub curing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Object)]
pub struct PrinterState {
    pub print_data: Option<PrintMetadata>,
    pub paused: Option<bool>,
    pub layer: Option<usize>,
    pub physical_state: PhysicalState,
    pub status: PrinterStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, Enum)]
pub enum PrinterStatus {
    Printing,
    Idle,
    Shutdown,
}
