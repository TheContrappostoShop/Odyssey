use std::collections::HashMap;

use crate::error::OdysseyError;
use async_trait::async_trait;
use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

//pub mod gcode;
pub mod klipper_uds;
pub mod klipper2;

#[derive(Clone, Debug, Serialize, Deserialize, Enum, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HardwareStatusEnum {
    Ready,
    #[default]
    Startup,
    Shutdown,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize, Object, Default)]
pub struct HardwareState {
    pub status: HardwareStatusEnum,
    pub status_message: String,
    pub z: f64,
    pub curing: bool,
    pub peripheral: HashMap<String, String>,
}

#[async_trait]
pub trait HardwareControl {
    async fn is_ready(&self) -> Result<bool, OdysseyError>;
    async fn initialize(&self) -> Result<(), OdysseyError>;
    async fn home(&self) -> Result<HardwareState, OdysseyError>;
    async fn manual_command(&self, command: String) -> Result<String, OdysseyError>;
    async fn start_print(&self) -> Result<HardwareState, OdysseyError>;
    async fn end_print(&self) -> Result<HardwareState, OdysseyError>;
    async fn move_z(
        &self,
        z_microns: u32,
        speed: f64,
        manual: bool,
    ) -> Result<HardwareState, OdysseyError>;
    async fn start_layer(&self, layer: usize) -> Result<HardwareState, OdysseyError>;
    async fn start_curing(&self) -> Result<HardwareState, OdysseyError>;
    async fn stop_curing(&self) -> Result<HardwareState, OdysseyError>;
    async fn boot(&self) -> Result<HardwareState, OdysseyError>;
    async fn shutdown(&self) -> Result<(), OdysseyError>;
    fn get_hardware_state(&self) -> HardwareState;
    fn get_hardware_state_receiver(&self) -> watch::Receiver<HardwareState>;
    // TODO: Implement variables as streams, using tokio::watchStream for configured interaction variables,
    // and basic repeat/repeat_with streams for simpler variables. Allows single access pattern to cover
    // all of our bases
    async fn add_state_variable(&self, variable: &str, value: String);
    async fn remove_state_variable(&self, variable: &str);
    async fn clear_state_variables(&self);
    async fn run(self, cancellation_token: CancellationToken) -> Result<(), OdysseyError>;
}
