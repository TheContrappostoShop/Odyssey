use core::panic;
use std::io::{self, Write};
use std::{collections::HashMap, str};

use regex::Regex;
use async_trait::async_trait;
use serialport::{SerialPort, SerialPortBuilder};

use crate::configuration::{GcodeConfig, Configuration};
use crate::printer::{HardwareControl, PhysicalState};



pub struct Gcode {
    pub config: GcodeConfig,
    pub state: PhysicalState,
    pub gcode_substitutions: HashMap<String, String>,
    pub serial_port: Box<dyn SerialPort>,
}



impl Gcode {
    pub fn new(config: Configuration, serial_builder: SerialPortBuilder) -> Gcode {
        return Gcode { 
            config: config.gcode, 
            state: PhysicalState { 
                z: 0.0, 
                curing: false 
            }, 
            gcode_substitutions: HashMap::new(),
            serial_port: serial_builder.open().expect("Unable to open serial connection")
        }
    }

    pub fn add_gcode_substitution(&mut self, key: String, value: String) {
        self.gcode_substitutions.insert(key, value);
    }


    fn parse_gcode(&self, code: String) -> String {
        let re: Regex = Regex::new(r"\{(\w*)\}").unwrap();
        let mut parsed_code = code.clone();

        for sub in re.find_iter(&code) {
            let value = self.gcode_substitutions.get(sub.as_str());
            if value.is_some() {
                parsed_code = parsed_code.replace(sub.as_str(), value.unwrap())
            } else {
                panic!("Attempted to use gcode substitution {} in context where it was unavailable: {}", sub.as_str(), code);
            }
        }
        return parsed_code;
    }

    async fn send_gcode(&mut self, code: String) {
        let parsed_code = self.parse_gcode(code.clone());
        println!("Executing gcode: {}", parsed_code);
        
        self.serial_port.write_all(parsed_code.as_bytes());
        self.serial_port.flush();
    }

    async fn await_response(&mut self, response: String) {
        let mut read_bytes: Vec<u8> = vec![0; 512];
        println!("Expecting response: {}", response);

        while !str::from_utf8(read_bytes.as_slice()).unwrap().contains(response.as_str()) {
            read_bytes.clear();

            match self.serial_port.read(&mut read_bytes) {
                Err(e) => match e.kind() {
                    io::ErrorKind::TimedOut => {
                        continue;
                    },
                    other_error => panic!("Error reading from serial port: {:?}", other_error),
                },
                Ok(n) => println!("Read {} bytes from serial: {}", n, str::from_utf8(read_bytes.as_slice()).unwrap()),
            };
        }
        
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
        return self.state;
    }

    /// Set the internally-stored curing state. Any method which uses a send_gcode
    /// method to enable or disable the LED array (or other curing method) should
    /// call this method to reflect that change
    fn set_curing(&mut self, curing: bool) -> PhysicalState {
        self.state.curing = curing;
        return self.state;
    }

}


#[async_trait]
impl HardwareControl for Gcode {
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
}
