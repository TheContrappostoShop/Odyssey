use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, broadcast};

use crate::printfile::FileData;
use crate::printfile::Layer;
use crate::printfile::PrintFile;
use crate::sl1::*;
use crate::configuration::*;
use crate::display::*;
use tokio::time::{sleep, Duration, interval};

pub struct Printer<T: HardwareControl> {
    pub config: PrinterConfig,
    pub display: PrintDisplay,
    pub hardware_controller: T,
    pub state: PrinterState,
    pub operation_channel: (mpsc::Sender<Operation>, mpsc::Receiver<Operation>),
    pub status_channel: (broadcast::Sender<PrinterState>, broadcast::Receiver<PrinterState>),
}

impl<T: HardwareControl> Printer<T> {
    pub fn new(config: PrinterConfig, display: PrintDisplay, hardware_controller: T) -> Printer<T>{
        Printer {
            config,
            display,
            hardware_controller,
            state: PrinterState::Idle {
                physical_state: PhysicalState { z: 0.0, curing: false }
            },
            operation_channel: mpsc::channel(100),
            status_channel: broadcast::channel(100)
        }
    }

    pub async fn print_event_loop(&mut self) {
        let mut file = Sl1::from_file(self.get_file_data().unwrap());

        let layer_height = file.get_layer_height();
        
        let mut pause_interv = interval(Duration::from_millis(100));

        // Fetch and generate the first frame
        let mut optional_frame = Frame::from_layer(
            file.get_layer_data(0).await
        ).await;

        loop {
            // Run any requested operations that may change the printer state
            self.printing_operation_handler().await;

            // TODO refactor this
            // Depending on state, either cancel print, wait for unpause, or print as normal
            match self.state {
                PrinterState::Printing { paused, layer, .. } => {
                    if paused {
                        pause_interv.tick().await;
                        continue;
                    }
                    else {
                        match optional_frame {
                            // More frames exist, continue printing
                            Some(cur_frame) => {
                                // Start a task to fetch and generate the next
                                // frame while we're exposing the current one
                                let gen_next_frame = tokio::spawn(
                                    Frame::from_layer(
                                        file.get_layer_data(layer+1).await
                                    )
                                );

                                // Print the current frame by moving into
                                // position and curing
                                self.print_frame(cur_frame, layer, layer_height).await;
                                
                                // Await generation of the next frame
                                optional_frame = gen_next_frame.await
                                    .expect("Layer generation task failed");

                                // Bump current layer
                                self.set_layer(layer+1).await;
                            },
                            // No more frames remain, end print
                            None => self.end_print().await,
                        }
                    }
                },
                _ => break,
            }
        }
    }

    async fn print_frame(&mut self, cur_frame: Frame, layer: usize, layer_height: f32) {
        let layer_z = ((layer+1) as f32)*layer_height;

        let exposure_time = cur_frame.exposure_time;

        // Move the plate up first, then down into position
        self.wrapped_move(layer_z+self.config.z_lift).await;
        self.wrapped_move(layer_z).await;

        // Display the current frame to the LCD
        self.display.display_frame(cur_frame);

        // Activate the UV array for the prescribed length of time
        println!("<Cure for {}>", exposure_time);
        self.wrapped_start_cure().await;
        sleep(Duration::from_secs_f32(exposure_time)).await;
        self.wrapped_stop_cure().await;
    }

    // Move and update printer state
    async fn wrapped_move(&mut self, z: f32) {
        let physical_state = self.hardware_controller.move_z(z).await;
        self.update_physical_state(physical_state).await;
    }

    // Start cure and update printer state
    async fn wrapped_start_cure(&mut self) {
        println!("Start cure");
        let physical_state =  self.hardware_controller.start_curing().await;
        self.update_physical_state(physical_state).await;
    }

    // Stop cure and update printer state
    async fn wrapped_stop_cure(&mut self) {
        let physical_state =  self.hardware_controller.stop_curing().await;
        self.update_physical_state(physical_state).await;
    }

    // Update layer in printer state
    async fn set_layer(&mut self, layer: usize) {
        self.update_layer(layer).await;
    }
    
    pub async fn start_print(&mut self, file_data: FileData) {
        println!("Starting Print");
        self.update_file_data(file_data).await;
        // Home kinematics and execute start_print command, reporting state in
        // between in case of long-running commands
        let mut physical_state = self.hardware_controller.home().await;
        self.update_physical_state(physical_state).await;

        physical_state = self.hardware_controller.start_print().await;
        self.update_physical_state(physical_state).await;
    }

    async fn end_print(&mut self) {
        let physical_state = self.hardware_controller.end_print().await;
        self.update_idle_state(physical_state).await;
        println!("Print complete.");
    }

    async fn pause_print(&mut self) {
        self.update_paused(true).await;
    }

    async fn resume_print(&mut self) {
        self.update_paused(false).await;
    }

    // Retrieve the current physical state
    fn get_physical_state(&self) -> PhysicalState {
        match self.state {
            PrinterState::Idle { physical_state } => physical_state,
            PrinterState::Printing { physical_state, .. } => physical_state,
            _ => panic!("cannot get physical state of shutdown machine"),
        }
    }

    fn get_layer(&self) -> usize {
        match self.state {
            PrinterState::Printing { layer, .. } => layer,
            _ => 0,
        }
    }

    fn get_file_data(&self) -> Option<FileData> {
        match &self.state {
            PrinterState::Printing { file_data, .. } => Some(file_data.clone()),
            _ => None,
        }
    }

    async fn update_physical_state(&mut self, new_physical_state: PhysicalState) {
        match self.state {
            PrinterState::Printing { ref mut physical_state , ..} => {
                *physical_state = new_physical_state;
            },
            PrinterState::Idle { ref mut physical_state } => {
                *physical_state = new_physical_state;
            }
            PrinterState::Shutdown => (),
        }
        self.send_status().await;
    }

    async fn update_paused(&mut self, new_pause: bool) {
        if let PrinterState::Printing { ref mut paused, ..} = self.state {
            *paused = new_pause;
        }
        self.send_status().await;
    }

    async fn update_layer(&mut self, new_layer: usize) {
        if let PrinterState::Printing { ref mut layer, ..} = self.state {
            *layer = new_layer;
        }
        self.send_status().await;
    }

    async fn update_file_data(&mut self, new_file_data: FileData) {
        if let PrinterState::Printing { ref mut file_data, ..} = self.state {
            *file_data = new_file_data;
        }
        self.send_status().await;
    }

    async fn printing_operation_handler(&mut self) {
        let mut op_result = self.operation_channel.1.try_recv();


        while let Ok(operation) = op_result {
            match operation {
                Operation::PausePrint => self.pause_print().await,
                Operation::ResumePrint => self.resume_print().await,
                Operation::StopPrint => self.set_idle().await,
                Operation::QueryState => self.send_status().await,
                Operation::Shutdown => self.shutdown().await,
                _ => (),
            };
            op_result = self.operation_channel.1.try_recv();
        }
    }

    pub async fn boot(&mut self) {
        println!("Booting up printer.");
        
        let physical_state = self.hardware_controller.boot().await;
        self.update_idle_state(physical_state).await;
    }

    pub async fn shutdown(&mut self) {
        println!("Shutting down.");
        self.hardware_controller.shutdown().await;
        self.state = PrinterState::Shutdown;
    }

    pub async fn get_operation_sender(&mut self) -> mpsc::Sender<Operation> {
        self.operation_channel.0.clone()
    }

    pub async fn get_status_receiver(&mut self) -> broadcast::Receiver<PrinterState> {
        self.status_channel.0.subscribe()
    }

    async fn send_status(&mut self) {
        self.status_channel.0.send(self.state.clone())
            .expect("Failed to send state update");
    }

    pub async fn start_statemachine(&mut self) {
        self.boot().await;

        loop {
            match self.state {
                PrinterState::Idle { .. } => self.idle_event_loop().await,
                PrinterState::Printing { .. } => self.print_event_loop().await,
                PrinterState::Shutdown => break,
            }
        }
    }

    async fn set_idle(&mut self) {
        self.state = PrinterState::Idle { physical_state: self.get_physical_state() };
        self.send_status().await;
    }

    async fn update_idle_state(&mut self, physical_state: PhysicalState) {
        self.state = PrinterState::Idle { physical_state };
        self.send_status().await;
    }

    async fn idle_operation_handler(&mut self) {
        let mut op_result = self.operation_channel.1.try_recv();

        while let Ok(operation) = op_result {
            match operation {
                Operation::QueryState => self.send_status().await,
                Operation::StartPrint { file_data } => self.start_print(file_data).await,
                Operation::ManualMove { z } => self.wrapped_move(z).await,
                Operation::ManualCure { cure } => {
                    if cure {
                        self.wrapped_start_cure().await;
                    }
                    else {
                        self.wrapped_stop_cure().await;
                    }
                },
                Operation::Shutdown => self.shutdown().await,
                _ => (),
            };
            op_result = self.operation_channel.1.try_recv();
        }
    }

    async fn idle_event_loop(&mut self) {
        let mut interv = interval(Duration::from_millis(1000));
        loop {
            self.idle_operation_handler().await;

            match self.state {
                PrinterState::Idle { .. } => {
                    interv.tick().await;
                },
                _ => break,
            }
        }
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PhysicalState {
    pub z: f32,
    pub curing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PrinterState {
    Printing { file_data: FileData, paused: bool, layer: usize, physical_state: PhysicalState },
    Idle { physical_state: PhysicalState },
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    StartPrint { file_data: FileData},
    StopPrint,
    PausePrint,
    ResumePrint,
    ManualMove { z: f32 },
    ManualCure { cure: bool },
    ManualDisplay { file_name: String },
    QueryState,
    Shutdown,
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
    fn get_physical_state(&self) -> PhysicalState;
}
