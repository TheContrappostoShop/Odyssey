use std::time::Duration;

//#[macro_use] extern crate rocket;
use clap::Parser;
use configuration::Configuration;

use display::PrintDisplay;
use tokio::{runtime::{Builder, Runtime}, time::sleep};

use crate::{printer::{Printer, Operation}, gcode::Gcode};

mod api;
mod configuration;
mod sl1;
mod display;
mod printer;
mod gcode;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Sliced model, in .sl1 format via PrusaSlicer
    #[arg(short, long)]
    file: Option<String>,

    /// Odyssey config file
    #[arg(default_value_t=String::from("./odyssey.yaml"), short, long)]
    config: String,

    #[arg(default_value_t=false, short, long)]
    test: bool
}


fn main() {
    let args = parse_cli();
    let configuration = parse_config(args.config);

    let mut printer = build_printer(configuration.clone());

    let runtime = build_runtime();
    
    if args.file.is_some() {
        let print_file = args.file.unwrap();

        println!("Starting Odyssey in CLI mode, printing {}", print_file);

        runtime.block_on( async move {
            let sender = printer.get_operation_sender().await.clone();
            let mut receiver = printer.get_status_receiver().await;

            tokio::spawn( async move { printer.start_statemachine().await });

            sender.send(Operation::StartPrint {file_name: print_file}).await
                .expect("Failed to send print start command");

            // Wait for the print to start before looking for a return to IDLE state
            sleep(Duration::from_secs(30)).await;

            let mut state = receiver.recv()
                .await.expect("Error reading printer state");

            while !(matches!(state, printer::PrinterState::Idle { .. })) {
                state = receiver.recv()
                    .await.expect("Error reading printer state")
            }

            sender.send(Operation::Shutdown).await
                .expect("Failed to send shutdown command");
            
            while !(matches!(state, printer::PrinterState::Shutdown)) {
                state = receiver.recv()
                    .await.expect("Error reading printer state")
            }
        });

    }
    else {
        runtime.block_on(async {
            let sender = printer.get_operation_sender().await.clone();
            let receiver = printer.get_status_receiver().await;

            tokio::spawn( async move { printer.start_statemachine().await });

            api::start_api(
                configuration.api, 
                sender,
                 receiver
            ).await;
        });
    }
}

fn build_runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("odyssey-worker")
        .thread_stack_size(3 * 1024 * 1024)
        .enable_time()
        .enable_io()
        .build()
        .expect("Unable to start Tokio runtime")
}

fn parse_cli() -> Args {
    Args::parse()
}

fn parse_config(config_file: String) -> Configuration {
    configuration::Configuration::load(config_file)
        .expect("Config could not be parsed. See example odyssey.yaml for expected fields:")
}

fn build_printer(configuration: Configuration) -> Printer<Gcode> {
    let serial = serialport::new(
        configuration.printer.serial.clone(), configuration.printer.baudrate
    );

    let mut gcode = Gcode::new(configuration.clone(), serial);


    gcode.add_gcode_substitution("{max_z}".to_string(), configuration.printer.max_z.to_string());
    gcode.add_gcode_substitution("{z_lift}".to_string(), configuration.printer.z_lift.to_string());

    let display: PrintDisplay = PrintDisplay::new(
        configuration.printer.frame_buffer.clone(), 
        configuration.printer.fb_bit_depth, 
        configuration.printer.fb_chunk_size
    );

    Printer::new(
        configuration.printer,
        display,
        gcode,
    )
}