use std::io::Error;

use async_trait::async_trait;

use crate::{
    api_objects::{FileData, FileMetadata, PrintMetadata},
    filetypes::printfile::{Layer, PrintFile},
};



// PrintConfig object encompassing the fields stored in the .goo header
#[derive(Debug)]
#[allow(dead_code)]
struct PrintConfig {
    version:  [u8; 4], //format version
    magic_tag:  [u8; 8], // Fix contant:0x07 0x00 0x00 0x00 0x44 0x4C 0x50 0x00
    software_info: [u8; 32], // Software info
    software_version: [u8;24], // Software version
    file_time: [u8;24], // File create time
    printer_name: [u8;32], // Printer name
    printer_type: [u8;32], // Printer type
    profile_name: [u8;32], // Resin profile name
    anti_aliasing_level: u16, // Anti-aliasing level setting by slicer
    grey_level: u16, // Grey level
    blur_level: u16, // Blur level
    small_preview_image_data: [u16;116*116], // The color of a pixel using two bytes (16-bit). Red(5bit), Green(6bit), Blue(5bit)
    preview_delimiter_1: u16, // Fix string: 0xd, 0xa
    big_preview_image_data: [u16;290*290], // The color of a pixel using two bytes (16-bit). Red(5bit), Green(6bit), Blue(5bit)
    preview_delimiter_2: u16, // Fix string: 0xd, 0xa
    total_layers: u32, // Total number of layers
    x_resolution: u16, // Resolution of printing LCD in x direction
    y_resolution: u16, // Resolution of Printing LCD in y direction
    x_mirror: bool, // 1 is mirror. That indicated if the image is mirror by Slicer.
    y_mirror: bool, // 1 is mirror.
    x_size_platform: f32, // unit: mm. Active print area in x direction.
    y_size_platform: f32, // unit: mm. Active print area in y direction.
    z_size_platform: f32, // unit: mm. Active print area in z direction.
    layer_thickness: f32, // unit: mm
    common_exposure_time: f32, // unit: s. Common layer exposure time.
    exposure_dely_mode: bool, // 1: use "Static time"; 0: use "Turn Off time";
    turn_off_time: f32, // unit: s. Delay time of layer exposure in Turn-off-time mode.
    bottom_before_lift_time: f32, // unit: s. Waiting time before lift for bottom layers.
    bottom_after_lift_time: f32, // unit: s. Waiting time after lift for bottom layers.
    bottom_after_retract_time: f32, // unit: s. Waiting time after retract for bottom layers.
    before_lift_time: f32, // unit: s. Waiting time before lift for common layers.
    after_lift_time: f32, // unit: s. Waiting time after lift for common layers.
    after_retract_time: f32, // unit: s. Waiting time after retract for common layers.
    bottom_exposure_time: f32, // unit: s. Exposure time of bottom layers.
    bottom_layers: u16, // The number of bottom layers.
    bottom_lift_distance: f32, // unit: mm. The lift distance for bottom layers.
    bottom_lift_speed: f32, // unit: mm/min. Lift speed for bottom layers.
    lift_distance: f32, // unit: mm. Lift distance for common layers.
    lift_speed: f32, // unit: mm/min. Life speed for common layers.
    bottom_retract_distance: f32, // unit: mm. Retract distance for bottom layers.
    bottom_retract_speed: f32, // unit: mm/min. Retract speed for bottom layers.
    retract_distance: f32, // unit: mm. Retract distance for common layers.
    retract_speed: f32, // unit: mm/min. Retract speed for common layers.
    bottom_second_lift_distance: f32, // unit: mm. Lift distance of second stage for bottom layers.
    bottom_second_lift_speed: f32, // unit: mm/min. Lift speed of second stage for bottom layers.
    second_lift_distance: f32, // unit: mm. Lift distance of second stage for common layers.
    second_lift_speed: f32, // unit: mm/min. Lift speed of second stage for common layers.
    bottom_second_retract_distance: f32, // unit: mm. Retract distance of second stage for bottom layers.
    bottom_second_retract_speed: f32, // unit: mm/min. Retract speed of second stage for bottom layers.
    second_retract_distance: f32, // unit: mm. Retract distance of second stage for common layers.
    second_retract_speed: f32, // unit: mm/min. Retract speed of second stage for common layers.
    bottom_light_pwm: u16, // The power of light for bottom layers. 0-255.
    light_pwm: u16, // The power of light for common layers. 0-255.
    advance_mode: bool, // 0: normal mode; 1: advance mode, printing uses the value of "Layer Definition Content"
    printing_time: u32, // unit: s. The printing time
    total_volume: f32, // unit: mm3. The volume of all parts.
    total_weight: f32, // unit: g. The weight of all parts.
    total_price: f32, // unit: price_unit. The cost of all resin used.
    price_unit: [u8;8], // The unit of price. e.g., $
    offset_layer_content: u32, // The position of LayerContent start address. E.g., 0x2FAB7
    grey_scale_level: bool, // 0: The range of pixel's gray value is from 0x0 - 0xF; 1: The range of pixel's gray value is from 0x0 - 0xFF
    transition_layers: u16 // The number of transition layers.
}

#[derive(Debug)]
#[allow(dead_code)]
struct layer_definition {
    pause_flag: u16, // 0: reserve; 1: current layer pause printing
    pause_position_z: f32, // unit: mm. The lift distance of Z axis when pause_flag equal 1
    layer_position_z: f32, // unit: mm. The height of the current layer
    layer_exposure_time: f32, // unit: s. The exposure time of the current layer
    layer_off_time: f32, // unit: s. The off time of current layer, when exposure_delay_mode set 0.
    before_lift_time: f32, // unit: s. Waiting time before lift for current layer when exposure_delay_mode set to 1.
    after_lift_time: f32, // unit: s. Waiting time after lift for current layer when exposure_delay_mode set to 1.
    after_retract_time: f32, // unit: s. Waiting time after retract for current layer when exposure_delay_mode set to 1.
    lift_distance: f32, // unit: mm. lift distance for current layer
    lift_speed: f32, // unit: mm/min. Lift speed for current layer
    second_lift_distance: f32, // unit: mm. Lift distance for second stage of current layers
    second_lift_speed: f32, // unit mm/min. Lift speed of second stage for current layers
    retract_distance: f32, // unit: mm. Retract distance for current layer
    retract_speed: f32, // unit: mm/min
    second_retract_distance: f32, // unit: mm. Retract distance of second stage for current layer
    second_retract_speed: f32, // unit: mm/min. Retract speed of second stage for current layer
    light_pwm: u16, // The power of the light for the current layer. 0-255
    delimiter: u16, // Fixed string: 0xD, 0xA
    data_size: u32, // The size of the encoded image data
}


/// The sliced .goo-format model
pub struct Goo {
}


#[async_trait]
impl PrintFile for Goo {
    fn from_file(file_data: FileMetadata) -> Self {

    }
    async fn get_layer_data(&mut self, index: usize) -> Option<Layer> {

    }
    fn get_layer_count(&self) -> usize {

    }
    fn get_layer_height(&self) -> f32 {

    }
    fn get_metadata(&self) -> PrintMetadata {

    }
    fn get_thumbnail(&mut self) -> Result<FileData, Error> {

    }
    
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