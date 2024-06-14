use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::api_objects::DisplayTest;
use crate::api_objects::FileMetadata;
use crate::api_objects::PhysicalState;
use crate::api_objects::PrintMetadata;
use crate::api_objects::PrinterState;
use crate::api_objects::PrinterStatus;
use crate::configuration::*;
use crate::display::*;
use crate::printfile::Layer;
use crate::printfile::PrintFile;
use crate::sl1::*;
use tokio::time::{interval, sleep, Duration};

pub struct Printer<T: HardwareControl> {
    pub config: PrinterConfig,
    pub display: PrintDisplay,
    pub hardware_controller: T,
    pub state: PrinterState,
    pub operation_channel: (mpsc::Sender<Operation>, mpsc::Receiver<Operation>),
    pub status_channel: (
        broadcast::Sender<PrinterState>,
        broadcast::Receiver<PrinterState>,
    ),
}

impl<T: HardwareControl> Printer<T> {
    pub fn new(config: PrinterConfig, display: PrintDisplay, hardware_controller: T) -> Printer<T> {
        Printer {
            config,
            display,
            hardware_controller,
            state: PrinterState {
                print_data: None,
                paused: None,
                layer: None,
                physical_state: PhysicalState {
                    z: 0.0,
                    curing: false,
                },
                status: PrinterStatus::Shutdown,
            },
            operation_channel: mpsc::channel(100),
            status_channel: broadcast::channel(100),
        }
    }

    pub async fn print_event_loop(&mut self) {
        let mut file: Box<dyn PrintFile + Send> =
            Box::new(Sl1::from_file(self.get_file_data().unwrap()));

        let layer_height = file.get_layer_height();

        // Get movement values from file, or configured defaults
        let lift = file.get_lift().unwrap_or(self.config.default_lift);
        let up_speed = file.get_up_speed().unwrap_or(self.config.default_up_speed);
        let down_speed = file
            .get_down_speed()
            .unwrap_or(self.config.default_down_speed);

        let wait_before_exposure = file
            .get_wait_before_exposure()
            .unwrap_or(self.config.default_wait_before_exposure);
        let wait_after_exposure = file
            .get_wait_after_exposure()
            .unwrap_or(self.config.default_wait_after_exposure);

        let mut pause_interv = interval(Duration::from_millis(100));

        self.hardware_controller.add_print_variable(
            "total_layers".to_string(),
            file.get_layer_count().to_string(),
        );

        // Execute start_print command, then report state
        self.wrapped_start_print().await;

        // Fetch and generate the first frame
        let mut optional_frame = Frame::from_layer(file.get_layer_data(0).await).await;

        loop {
            // Run any requested operations that may change the printer state
            self.printing_operation_handler().await;

            match self.state.status {
                PrinterStatus::Printing => {
                    let paused = self.state.paused.unwrap();
                    let layer = self.state.layer.unwrap();
                    if paused {
                        pause_interv.tick().await;
                        continue;
                    } else {
                        match optional_frame {
                            // More frames exist, continue printing
                            Some(cur_frame) => {
                                self.hardware_controller
                                    .add_print_variable("layer".to_string(), layer.to_string());
                                // Start a task to fetch and generate the next
                                // frame while we're exposing the current one
                                let gen_next_frame = tokio::spawn(Frame::from_layer(
                                    file.get_layer_data(layer + 1).await,
                                ));

                                // Print the current frame by moving into
                                // position and curing
                                self.print_frame(
                                    cur_frame,
                                    layer,
                                    layer_height,
                                    lift,
                                    up_speed,
                                    down_speed,
                                    wait_before_exposure,
                                    wait_after_exposure,
                                )
                                .await;

                                // Await generation of the next frame
                                optional_frame =
                                    gen_next_frame.await.expect("Layer generation task failed");

                                // Bump current layer
                                self.set_layer(layer + 1).await;
                            }
                            // No more frames remain, end print
                            None => self.end_print().await,
                        }
                    }
                }
                _ => break,
            }
        }
    }

    async fn print_frame(
        &mut self,
        cur_frame: Frame,
        layer: usize,
        layer_height: f32,
        lift: f32,
        up_speed: f32,
        down_speed: f32,
        wait_before_exposure: f32,
        wait_after_exposure: f32,
    ) {
        log::info!("Begin layer {}", layer);
        self.wrapped_start_layer(layer).await;
        let layer_z = ((layer + 1) as f32) * layer_height;
        //let lift_z = layer_z+

        let exposure_time = cur_frame.exposure_time;

        // Move the plate up first, then down into position
        log::info!("Moving to layer position {}", layer_z);

        self.wrapped_move(layer_z + lift, up_speed).await;
        self.wrapped_move(layer_z, down_speed).await;

        // Wait for configured time before curing
        log::info!("Waiting for {}s before cure", wait_before_exposure);
        sleep(Duration::from_secs_f32(wait_before_exposure)).await;

        // Display the current frame to the LCD
        log::info!("Loading layer to display");
        self.display.display_frame(cur_frame);

        // Activate the UV array for the prescribed length of time
        log::info!("Curing layer for {}s", exposure_time);
        self.wrapped_start_cure().await;
        sleep(Duration::from_secs_f32(exposure_time)).await;
        self.wrapped_stop_cure().await;

        // Wait for configured time after curing
        log::info!("Waiting for {}s after cure", wait_after_exposure);
        sleep(Duration::from_secs_f32(wait_after_exposure)).await;
    }

    async fn wrapped_start_print(&mut self) {
        if let Ok(physical_state) = self.hardware_controller.start_print().await {
            self.update_physical_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    async fn wrapped_start_layer(&mut self, layer: usize) {
        if let Ok(physical_state) = self.hardware_controller.start_layer(layer).await {
            self.update_physical_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Execute command and update printer state
    async fn wrapped_command(&mut self, command: String) {
        if let Ok(physical_state) = self.hardware_controller.manual_command(command).await {
            self.update_physical_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Home and update printer state
    async fn wrapped_home(&mut self) {
        if let Ok(physical_state) = self.hardware_controller.home().await {
            self.update_physical_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Move and update printer state
    async fn wrapped_move(&mut self, z: f32, speed: f32) {
        if let Ok(physical_state) = self.hardware_controller.move_z(z, speed).await {
            self.update_physical_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Start cure and update printer state
    async fn wrapped_start_cure(&mut self) {
        if let Ok(physical_state) = self.hardware_controller.start_curing().await {
            self.update_physical_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Stop cure and update printer state
    async fn wrapped_stop_cure(&mut self) {
        if let Ok(physical_state) = self.hardware_controller.stop_curing().await {
            self.update_physical_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    // Update layer in printer state
    async fn set_layer(&mut self, layer: usize) {
        self.update_layer(layer).await;
    }

    pub async fn start_print(&mut self, file_data: FileMetadata) {
        log::info!("Starting Print");

        let print_data = Sl1::from_file(file_data).get_metadata();
        self.enter_printing_state(print_data).await;
    }

    async fn end_print(&mut self) {
        if let Ok(physical_state) = self.hardware_controller.end_print().await {
            self.hardware_controller
                .remove_print_variable("total_layers".to_string());
            self.hardware_controller
                .remove_print_variable("layer".to_string());
            self.update_idle_state(physical_state).await;
            log::info!("Print complete.");
        } else {
            self.shutdown().await;
        }
    }

    async fn pause_print(&mut self) {
        self.update_paused(true).await;
    }

    async fn resume_print(&mut self) {
        self.update_paused(false).await;
    }

    fn _get_layer(&self) -> usize {
        self.state.layer.unwrap_or(0)
    }

    fn get_file_data(&self) -> Option<FileMetadata> {
        self.state
            .print_data
            .clone()
            .map(|print_data| print_data.file_data)
    }

    async fn enter_printing_state(&mut self, print_data: PrintMetadata) {
        log::info!("Entering printing state");
        match self.state.status {
            PrinterStatus::Idle => {
                log::debug!("Transitioning from Idle State");
                self.state = PrinterState {
                    print_data: Some(print_data),
                    paused: Some(false),
                    layer: Some(0),
                    physical_state: self.state.physical_state,
                    status: PrinterStatus::Printing,
                };
            }
            PrinterStatus::Printing => {
                log::debug!("Already in printing state!");
            }
            PrinterStatus::Shutdown => {
                log::debug!("Cannot start print, Odyssey shutdown");
            }
        }
    }

    async fn update_physical_state(&mut self, new_physical_state: PhysicalState) {
        match self.state.status {
            PrinterStatus::Printing => {
                self.state.physical_state = new_physical_state;
            }
            PrinterStatus::Idle => {
                self.state.physical_state = new_physical_state;
            }
            PrinterStatus::Shutdown {} => (),
        }
        self.send_status().await;
    }

    async fn update_paused(&mut self, new_pause: bool) {
        if matches!(self.state.status, PrinterStatus::Printing) {
            self.state.paused = Some(new_pause);
        }
        self.send_status().await;
    }

    async fn update_layer(&mut self, new_layer: usize) {
        if matches!(self.state.status, PrinterStatus::Printing) {
            self.state.layer = Some(new_layer);
        }
        self.send_status().await;
    }

    async fn printing_operation_handler(&mut self) {
        /*if !self.verify_hardware().await {
            return;
        }*/

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
        log::info!("Booting up printer.");

        let boot_result: Result<PhysicalState, std::io::Error> =
            self.hardware_controller.boot().await;
        if let Ok(physical_state) = boot_result {
            self.update_idle_state(physical_state).await;
        } else {
            self.shutdown().await;
        }
    }

    pub async fn _verify_hardware(&mut self) -> bool {
        if !self.hardware_controller.is_ready().await {
            log::error!("Hardware controller no longer ready! Shutting down Odyssey");
            self.shutdown().await;
            return false;
        }
        true
    }

    pub async fn shutdown(&mut self) {
        log::info!("Shutting down.");
        // If hardware still running, execute shutdown commands
        if self.hardware_controller.is_ready().await {
            if (self.hardware_controller.shutdown().await).is_ok() {
                log::info!("Shut down gcode executed successfully")
            } else {
                log::info!("Unable to execute shutdown gcode")
            }
        }
        self.state.status = PrinterStatus::Shutdown;
        self.state.paused = None;
        self.state.print_data = None;
        self.state.physical_state = PhysicalState {
            z: f32::MAX,
            curing: false,
        }
    }

    pub async fn get_operation_sender(&mut self) -> mpsc::Sender<Operation> {
        self.operation_channel.0.clone()
    }

    pub async fn get_status_receiver(&mut self) -> broadcast::Receiver<PrinterState> {
        self.status_channel.0.subscribe()
    }

    async fn send_status(&mut self) {
        self.status_channel
            .0
            .send(self.state.clone())
            .expect("Failed to send state update");
    }

    pub async fn start_statemachine(&mut self) {
        self.hardware_controller.initialize().await;

        loop {
            match self.state.status {
                PrinterStatus::Idle => self.idle_event_loop().await,
                PrinterStatus::Printing => self.print_event_loop().await,
                PrinterStatus::Shutdown => self.shutdown_event_loop().await,
            }
        }
    }

    async fn shutdown_event_loop(&mut self) {
        let mut shutdown_interv = interval(Duration::from_millis(10000));

        loop {
            self.shutdown_operation_handler().await;

            match self.state.status {
                PrinterStatus::Shutdown => {
                    if self.hardware_controller.is_ready().await {
                        self.boot().await;
                    } else {
                        shutdown_interv.tick().await;
                    }
                }
                _ => break,
            }
        }
    }

    // While in shutdown state, process operations to drop them from queue
    async fn shutdown_operation_handler(&mut self) {
        let mut op_result = self.operation_channel.1.try_recv();

        while let Ok(operation) = op_result {
            if let Operation::QueryState = operation {
                self.send_status().await
            }
            op_result = self.operation_channel.1.try_recv();
        }
    }

    async fn set_idle(&mut self) {
        self.state.status = PrinterStatus::Idle;
        self.state.layer = None;
        self.state.paused = None;
        self.send_status().await;
    }

    async fn update_idle_state(&mut self, physical_state: PhysicalState) {
        self.state.status = PrinterStatus::Idle;
        self.state.physical_state = physical_state;
        self.send_status().await;
    }

    async fn idle_operation_handler(&mut self) {
        /*if !self.verify_hardware().await {
            return;
        }*/

        let mut op_result = self.operation_channel.1.try_recv();

        while let Ok(operation) = op_result {
            match operation {
                Operation::QueryState => self.send_status().await,
                Operation::StartPrint { file_data } => self.start_print(file_data).await,
                Operation::ManualCommand { command } => self.wrapped_command(command).await,
                Operation::ManualHome => self.wrapped_home().await,
                Operation::ManualMove { z } => {
                    self.wrapped_move(z, self.config.default_up_speed).await
                }
                Operation::ManualCure { cure } => {
                    if cure {
                        self.wrapped_start_cure().await;
                    } else {
                        self.wrapped_stop_cure().await;
                    }
                }
                Operation::ManualDisplayTest { test } => {
                    self.display.display_test(test);
                }
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

            match self.state.status {
                PrinterStatus::Idle => {
                    interv.tick().await;
                }
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    StartPrint { file_data: FileMetadata },
    StopPrint,
    PausePrint,
    ResumePrint,
    ManualMove { z: f32 },
    ManualCure { cure: bool },
    ManualHome,
    ManualCommand { command: String },
    ManualDisplay { file_name: String },
    ManualDisplayTest { test: DisplayTest },
    QueryState,
    Shutdown,
}

#[async_trait]
pub trait HardwareControl {
    async fn is_ready(&mut self) -> bool;
    async fn initialize(&mut self);
    async fn home(&mut self) -> std::io::Result<PhysicalState>;
    async fn manual_command(&mut self, command: String) -> std::io::Result<PhysicalState>;
    async fn start_print(&mut self) -> std::io::Result<PhysicalState>;
    async fn end_print(&mut self) -> std::io::Result<PhysicalState>;
    async fn move_z(&mut self, z: f32, speed: f32) -> std::io::Result<PhysicalState>;
    async fn start_layer(&mut self, layer: usize) -> std::io::Result<PhysicalState>;
    async fn start_curing(&mut self) -> std::io::Result<PhysicalState>;
    async fn stop_curing(&mut self) -> std::io::Result<PhysicalState>;
    async fn boot(&mut self) -> std::io::Result<PhysicalState>;
    async fn shutdown(&mut self) -> std::io::Result<()>;
    fn get_physical_state(&self) -> std::io::Result<PhysicalState>;
    fn add_print_variable(&mut self, variable: String, value: String);
    fn remove_print_variable(&mut self, variable: String);
    fn clear_variables(&mut self);
}
