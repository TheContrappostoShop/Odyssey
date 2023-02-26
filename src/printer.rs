
use async_trait::async_trait;

use crate::sl1::*;
use crate::configuration::*;
use crate::display::*;
use tokio::time::{sleep, Duration};

pub struct Printer<T: HardwareControl> {
    pub config: PrinterConfig,
    pub display: PrintDisplay,
    pub hardware_controller: T,
}

impl<T: HardwareControl> Printer<T> {
    pub async fn print<'a>(&mut self, file_name: String) {
        let mut file = Sl1::from_file(file_name);
        let mut index = 0;

        let layer_height = file.get_layer_height();
        let z_lift = self.config.z_lift;

        // Home kinematics and execute start_print command
        self.hardware_controller.home().await;
        self.hardware_controller.start_print().await;

        // Fetch and generate the first frame
        let fetch_layer = file.get_layer_data(index);

        let mut optional_frame = Frame::from_layer(fetch_layer.await).await;

        while optional_frame.is_some() {
            let layer_z = ((index+1) as f32)*layer_height;

            index+=1;

            // Known to exist courtesy of loop condition
            let cur_frame = optional_frame.unwrap();

            let exposure_time = cur_frame.exposure_time;

            // Start a task to fetch and generate the next frame while we're exposing the current one
            let fetch_next_layer = file.get_layer_data(index);
            let gen_next_frame = tokio::spawn(Frame::from_layer(fetch_next_layer.await));

            // Move the plate up first, then down into position
            self.hardware_controller.move_z(layer_z+z_lift).await;
            self.hardware_controller.move_z(layer_z).await;

            // Display the current frame to the LCD
            self.display.display_frame(cur_frame);

            // Activate the UV array for the prescribed length of time
            println!("<Cure for {}>", exposure_time);
            self.hardware_controller.start_curing().await;
            sleep(Duration::from_secs_f32(exposure_time)).await;
            self.hardware_controller.stop_curing().await;

            // Finish fetching the next frame
            optional_frame = gen_next_frame.await.expect("Layer generation task failed");
        }

        self.hardware_controller.end_print().await;
        println!("Print complete.");
    }

    pub async fn boot(&mut self) {
        println!("Booting up printer.");
        self.hardware_controller.boot().await;
    }

    pub async fn shutdown(&mut self) {
        println!("Shutting down.");
        self.hardware_controller.shutdown().await;
    }
}

impl Frame {
    async fn from_layer(layer: Option<Layer>) -> Option<Frame> {
        if layer.is_some() {
            let layer = layer.unwrap();
            let frame = Frame::from_vec(layer.file_name, layer.exposure_time, layer.data);
            return Some(frame);
        }
        None
    }
}

#[derive(Clone, Copy)]
pub struct PhysicalState {
    pub z: f32,
    pub curing: bool,
}


#[async_trait]
pub trait HardwareControl {
    async fn home(&mut self) -> PhysicalState;
    async fn start_print(&mut self) -> PhysicalState;
    async fn end_print(&mut self) -> PhysicalState;
    async fn move_z(&mut self, z: f32) -> PhysicalState;
    async fn start_curing(&mut self) -> PhysicalState;
    async fn stop_curing(&mut self) -> PhysicalState;
    async fn boot(&mut self) -> PhysicalState;
    async fn shutdown(&mut self) -> PhysicalState;
}
