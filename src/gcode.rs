use core::panic;
use std::io::{self, Write, BufReader, BufRead};
use std::collections::HashMap;

use regex::Regex;
use async_trait::async_trait;
use serialport::{TTYPort, SerialPortBuilder, SerialPort, ClearBuffer};
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::time::{interval, Duration};

use crate::configuration::{GcodeConfig, Configuration};
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
        let mut port = serial_builder.open_native().expect("Unable to open serial connection");

        port.set_exclusive(false).expect("Unable to set serial port exclusivity(false)");
        port.clear(ClearBuffer::All).expect("Unable to clear serialport buffers");

        Gcode {
            config: config.gcode, 
            state: PhysicalState { 
                z: 0.0, 
                curing: false 
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
                    },
                    other_error => panic!("Error reading from serial port: {:?}", other_error),
                },
                Ok(n) => {
                    if n>0 {
                        println!("Read {} bytes from serial: {}", n, read_string.trim_end());
                        sender.send(read_string).await.expect("Unable to send message to channel");
                    }
                },
            };
        }
    }

    pub fn add_gcode_substitution(&mut self, key: String, value: String) {
        self.gcode_substitutions.insert(key, value);
    }


    fn parse_gcode(&self, code: String) -> String {
        let re: Regex = Regex::new(r"\{(\w*)\}").unwrap();
        let mut parsed_code = code.clone();

        for sub in re.find_iter(&code) {
            if let Some(value) = self.gcode_substitutions.get(sub.as_str()) {
                parsed_code = parsed_code.replace(sub.as_str(), value)
            } else {
                panic!("Attempted to use gcode substitution {} in context where it was unavailable: {}", sub.as_str(), code);
            }
        }
        parsed_code
    }

    async fn send_gcode(&mut self, code: String) {
        let parsed_code = self.parse_gcode(code)+"\r\n";
        println!("Executing gcode: {}", parsed_code.trim_end());
        
        let n = self.serial_port.write(parsed_code.as_bytes()).unwrap();
        self.serial_port.flush().expect("Unable to flush serial connection");

        println!("Wrote {} bytes", n);
    }

    async fn await_response(&mut self, response: String) {
        let mut msg = String::new();
        println!("Expecting response: {}", response);

        while !msg.contains(response.as_str()) {
            msg = self.transceiver.1.recv().await.expect("Unable to receive message from channel");
        }
        println!("Expected response received");
        
    }

    async fn send_and_await_gcode(&mut self, code: String, expect: String) {
        self.send_gcode(code).await;
        self.await_response(expect).await;
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

}


#[async_trait]
impl HardwareControl for Gcode {
    async fn home(&mut self) -> PhysicalState{
        self.send_gcode(self.config.home_command.clone()).await;

        return self.state;
    }

    async fn move_z(&mut self, z: f32) -> PhysicalState {
        self.gcode_substitutions.insert("{z}".to_string(), z.to_string());

        self.send_and_await_gcode(self.config.move_command.clone(), self.config.sync_message.clone()).await;

        self.gcode_substitutions.remove(&"{z}".to_string());

        return self.set_position(z);
    }

    async fn start_curing(&mut self) -> PhysicalState {
        self.send_gcode(self.config.cure_start.clone()).await;

        return self.set_curing(true);
    }
    

    async fn stop_curing(&mut self) -> PhysicalState {
        self.send_gcode(self.config.cure_end.clone()).await;

        return self.set_curing(false);
    }
    
    async fn start_print(&mut self) -> PhysicalState {
        self.send_gcode(self.config.print_start.clone()).await;

        return self.state;
    }

    async fn end_print(&mut self) -> PhysicalState{
        self.send_gcode(self.config.print_end.clone()).await;

        return self.state;
    }

    async fn boot(&mut self) -> PhysicalState{
        // Run the serial port listener task
        tokio::spawn(Gcode::run_listener(
            self.serial_port.try_clone_native().expect("Unable to clone serial connection"),
            self.transceiver.0.clone()
        ));

        self.send_gcode(self.config.boot.clone()).await;

        return self.state;
    }

    async fn shutdown(&mut self) -> PhysicalState{
        self.send_gcode(self.config.shutdown.clone()).await;

        return self.state;
    }
}
