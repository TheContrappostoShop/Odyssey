use std::str::FromStr;

use clap::Parser;

use simple_logger::SimpleLogger;
use tokio::runtime::{Builder, Runtime};

use odyssey::{
    self, api, configuration::Configuration, display::PrintDisplay, gcode::Gcode, printer::Printer,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Odyssey config file
    #[arg(default_value_t=String::from("./default.yaml"), short, long)]
    config: String,
    #[arg(default_value_t=String::from("DEBUG"), short, long)]
    loglevel: String,
}

fn main() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    let args = parse_cli();

    SimpleLogger::new()
        .with_level(log::LevelFilter::from_str(&args.loglevel).expect("Unable to parse loglevel"))
        .init()
        .unwrap();

    log::info!("Starting Odyssey");

    let configuration = parse_config(args.config);

    let serial = serialport::new(
        configuration.printer.serial.clone(),
        configuration.printer.baudrate,
    );

    let gcode = Gcode::new(configuration.clone(), serial);

    let display: PrintDisplay = PrintDisplay::new(
        configuration.printer.frame_buffer.clone(),
        configuration.printer.fb_bit_depth,
        configuration.printer.fb_chunk_size,
    );

    let mut printer = Printer::new(configuration.printer.clone(), display, gcode);

    let runtime = build_runtime();

    runtime.block_on(async {
        let sender = printer.get_operation_sender().await.clone();
        let receiver = printer.get_status_receiver().await;

        tokio::spawn(async move { printer.start_statemachine().await });

        api::start_api(configuration, sender, receiver).await;
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
    Configuration::load(config_file)
        .expect("Config could not be parsed. See example odyssey.yaml for expected fields:")
}
