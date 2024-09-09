use serialport::{SerialPortBuilder, TTYPort};
use std::io::{self, BufRead, BufReader, Error, ErrorKind, Write};
use tokio::sync::mpsc::{self, Receiver as MPSCReceiver, Sender as MPSCSender};
use tokio::sync::broadcast::{self, Receiver as BroadcastReceiver, Sender as BroadcastSender};
use tokio::time::{interval, sleep, Duration};


pub struct SerialHandler {
    pub serial_port: TTYPort,
    pub receiver: BroadcastReceiver<String>,
    pub sender: MPSCSender<String>,
    command_receiver: MPSCReceiver<String>,
    output_sender: BroadcastSender<String>
}

impl SerialHandler {
    pub fn new(serial_port: TTYPort) -> SerialHandler {
        let (sender, command_receiver) = mpsc::channel(100);
        let (output_sender, receiver) = broadcast::channel(100);

        SerialHandler {
            serial_port,
            receiver,
            sender,
            command_receiver,
            output_sender
        }
    }

    async fn initialize(&mut self) {
        // Run the serial port listener tasks
        self.run_writer().await
    }

    pub async fn run_listener(&self) {
        let mut buf_reader = BufReader::new(self.serial_port.try_clone_native().expect("Unable to clone serial port"));
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
                        self.output_sender
                            .send(read_string)
                            .expect("Unable to send message to channel");
                    }
                }
            };
        }
    }

    pub async fn run_writer(&mut self) {
        let mut buf_reader = BufReader::new(self.serial_port.try_clone_native().expect("Unable to clone serial port"));

        let mut interval = interval(Duration::from_millis(100));


        loop {
            interval.tick().await;

            buf_reader.

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
                        self.output_sender
                            .send(read_string)
                            .expect("Unable to send message to channel");
                    }
                }
            };
            


            match self.command_receiver.recv().await {
                Some(message) => {
                    
                    self.serial_port.write(message.as_bytes());
                },
                None => ()
            };
        }
    }

}