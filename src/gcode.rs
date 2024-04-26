use core::panic;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Error, ErrorKind, Write};

use async_trait::async_trait;
use regex::Regex;
use serialport::{ClearBuffer, SerialPort, SerialPortBuilder, TTYPort};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{interval, sleep, Duration};

use crate::configuration::{Configuration, GcodeConfig};
use crate::printer::{HardwareControl, PhysicalState};

pub struct Gcode {
    pub config: GcodeConfig,
    pub state: PhysicalState,
    pub gcode_substitutions: HashMap<String, String>,
    pub serial_port: TTYPort,
    pub transceiver: (Sender<String>, Receiver<String>),
}

impl Gcode {
    pub fn new(config: Configuration, serial_builder: SerialPortBuilder) -> Gcode {
        let transceiver = mpsc::channel(100);
        let mut port = serial_builder
            .open_native()
            .expect("Unable to open serial connection");

        port.set_exclusive(false)
            .expect("Unable to set serial port exclusivity(false)");
        port.clear(ClearBuffer::All)
            .expect("Unable to clear serialport buffers");

        Gcode {
            config: config.gcode,
            state: PhysicalState {
                z: 0.0,
                curing: false,
            },
            gcode_substitutions: HashMap::new(),
            serial_port: port,
            transceiver,
        }
    }

    pub async fn run_listener(port: TTYPort, sender: Sender<String>) {
        let mut buf_reader = BufReader::new(port);
        let mut interval = interval(Duration::from_millis(100));

        loop {
            interval.tick().await;
            let mut read_string = String::new();
            match buf_reader.read_line(&mut read_string) {
                Err(e) => match e.kind() {
                    io::ErrorKind::TimedOut => {
                        continue;
                    }
                    // Broken Pipe here
                    other_error => panic!("Error reading from serial port: {:?}", other_error),
                },
                Ok(n) => {
                    if n > 0 {
                        log::debug!("Read {} bytes from serial: {}", n, read_string.trim_end());
                        sender
                            .send(read_string)
                            .await
                            .expect("Unable to send message to channel");
                    }
                }
            };
        }
    }

    fn parse_gcode(&mut self, code: String) -> String {
        let re: Regex = Regex::new(r"\{(?P<substitution>\w*)\}").unwrap();
        let mut parsed_code = code.clone();

        self.add_state_variables();

        for caps in re.captures_iter(&code) {
            let sub = &caps["substitution"].to_string();
            if let Some(value) = self.gcode_substitutions.get(sub) {
                parsed_code = parsed_code.replace(&format!("{{{sub}}}"), value)
            } else {
                panic!("Attempted to use gcode substitution {} in context where it was unavailable: {}", sub, code);
            }
        }
        parsed_code
    }

    async fn send_gcode(&mut self, code: String) -> std::io::Result<()> {
        let parsed_code = self.parse_gcode(code) + "\r\n";
        log::debug!("Executing gcode: {}", parsed_code.trim_end());

        let n = self.serial_port.write(parsed_code.as_bytes())?;
        self.serial_port
            .flush()
            .expect("Unable to flush serial connection");

        log::trace!("Wrote {} bytes", n);
        // Force a delay between commands
        sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    async fn await_response(
        &mut self,
        response: String,
        timeout_seconds: usize,
    ) -> std::io::Result<()> {
        log::trace!("Expecting response: {}", response);
        let mut interval = interval(Duration::from_millis(100));
        let intervals = 10 * timeout_seconds;

        for _ in 0..intervals {
            if self.check_response(&response).await {
                log::trace!("Expected response received");
                return Ok(());
            } else {
                interval.tick().await;
            }
        }
        Err(Error::new(
            ErrorKind::TimedOut,
            "Timed out awaiting gcode response",
        ))
    }

    // Consume all available responses in case of ack messages before desired
    async fn check_response(&mut self, response: &String) -> bool {
        let mut has_response = self
            .transceiver
            .1
            .recv()
            .await
            .expect("Unable to receive message from channel")
            .contains(response);

        while let Ok(resp) = self.transceiver.1.try_recv() {
            has_response = has_response || resp.contains(response);
        }
        has_response
    }

    async fn send_and_await_gcode(
        &mut self,
        code: String,
        expect: String,
        timeout_seconds: usize,
    ) -> std::io::Result<()> {
        self.send_gcode(code).await?;
        self.await_response(expect, timeout_seconds).await?;
        Ok(())
    }

    async fn send_and_check_gcode(&mut self, code: String, expect: String) -> bool {
        if self.send_gcode(code).await.is_ok() {
            return self.check_response(&expect).await;
        }
        false
    }

    /// Set the internally-stored position. Any method which uses a send_gcode
    /// method to cause the z axis to move, should call this method to reflect
    /// that change
    fn set_position(&mut self, position: f32) -> PhysicalState {
        self.state.z = position;
        self.state
    }

    /// Set the internally-stored curing state. Any method which uses a send_gcode
    /// method to enable or disable the LED array (or other curing method) should
    /// call this method to reflect that change
    fn set_curing(&mut self, curing: bool) -> PhysicalState {
        self.state.curing = curing;
        self.state
    }

    fn add_state_variables(&mut self) {
        self.gcode_substitutions
            .insert("curing".to_string(), self.state.curing.to_string());
        self.gcode_substitutions
            .insert("z".to_string(), self.state.z.to_string());
    }
}

#[async_trait]
impl HardwareControl for Gcode {
    async fn initialize(&mut self) {
        // Run the serial port listener task
        tokio::spawn(Gcode::run_listener(
            self.serial_port
                .try_clone_native()
                .expect("Unable to clone serial connection"),
            self.transceiver.0.clone(),
        ));
    }

    async fn is_ready(&mut self) -> bool {
        self.send_and_check_gcode(
            self.config.status_check.clone(),
            self.config.status_desired.clone(),
        )
        .await
    }

    async fn home(&mut self) -> std::io::Result<PhysicalState> {
        self.send_gcode(self.config.home_command.clone()).await?;

        Ok(self.state)
    }

    async fn move_z(&mut self, z: f32, speed: f32) -> std::io::Result<PhysicalState> {
        // To handle floating point precision issues, truncate to micron precision
        let z = (z * 1000.0).trunc() / 1000.0;

        // Convert from mm/s to mm/min f value
        let speed = speed * 60.0;

        self.set_position(z);
        self.add_print_variable("speed".to_string(), speed.to_string());

        self.send_and_await_gcode(
            self.config.move_command.clone(),
            self.config.move_sync.clone(),
            self.config.move_timeout,
        )
        .await?;

        self.remove_print_variable("speed".to_string());

        Ok(self.state)
    }

    async fn start_layer(&mut self, _layer: usize) -> std::io::Result<PhysicalState> {
        self.send_gcode(self.config.layer_start.clone()).await?;

        Ok(self.state)
    }

    async fn start_curing(&mut self) -> std::io::Result<PhysicalState> {
        self.set_curing(true);

        self.send_gcode(self.config.cure_start.clone()).await?;

        Ok(self.state)
    }

    async fn stop_curing(&mut self) -> std::io::Result<PhysicalState> {
        self.set_curing(false);
        self.send_gcode(self.config.cure_end.clone()).await?;
        Ok(self.state)
    }

    async fn start_print(&mut self) -> std::io::Result<PhysicalState> {
        self.send_gcode(self.config.print_start.clone()).await?;

        Ok(self.state)
    }

    async fn end_print(&mut self) -> std::io::Result<PhysicalState> {
        self.send_gcode(self.config.print_end.clone()).await?;

        Ok(self.state)
    }

    async fn boot(&mut self) -> std::io::Result<PhysicalState> {
        self.send_gcode(self.config.boot.clone()).await?;

        Ok(self.state)
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.send_gcode(self.config.shutdown.clone()).await?;

        Ok(())
    }

    fn get_physical_state(&self) -> std::io::Result<PhysicalState> {
        Ok(self.state)
    }

    fn add_print_variable(&mut self, variable: String, value: String) {
        self.gcode_substitutions.insert(variable, value);
    }

    fn remove_print_variable(&mut self, variable: String) {
        self.gcode_substitutions.remove(&variable);
    }

    fn clear_variables(&mut self) {
        self.gcode_substitutions.clear();
    }
}
