use std::str::FromStr;

use clap::Parser;
use configuration::Configuration;

use display::PrintDisplay;
use simple_logger::SimpleLogger;
use tokio::{runtime::{Builder, Runtime}};

use crate::{printer::Printer, gcode::Gcode};

mod api;
mod configuration;
mod sl1;
mod display;
mod printer;
mod gcode;
mod printfile;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Odyssey config file
    #[arg(default_value_t=String::from("./odyssey.yaml"), short, long)]
    config: String,
    #[arg(default_value_t=String::from("INFO"), short, long)]
    loglevel: String
}

fn main() {

    let args = parse_cli();

    SimpleLogger::new()
        .with_level(
            log::LevelFilter::from_str(&args.loglevel)
                .expect("Unable to parse loglevel")
        ).init().unwrap();

    log::info!("Starting Odyssey");

    let configuration = parse_config(args.config);

    let mut printer = build_printer(configuration.clone());

    let runtime = build_runtime();
    
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