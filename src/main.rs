use std::str::FromStr;

use clap::Parser;

use serialport::{ClearBuffer, SerialPort};
use simple_logger::SimpleLogger;
use tokio::{
    runtime::{Builder, Runtime},
    sync::broadcast,
};

use odyssey::{
    self, api, configuration::Configuration, display::PrintDisplay, gcode::Gcode, printer::Printer,
    serial_handler,
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

    let mut serial = tokio_serial::new(
        configuration.printer.serial.clone(),
        configuration.printer.baudrate,
    )
    .open_native()
    .expect("Unable to open serial port");

    serial
        .set_exclusive(false)
        .expect("Unable to set serial port exclusivity(false)");
    serial
        .clear(ClearBuffer::All)
        .expect("Unable to clear serialport buffers");

    let (serial_read_sender, serial_read_receiver) = broadcast::channel(200);
    let (serial_write_sender, serial_write_receiver) = broadcast::channel(200);

    let gcode = Gcode::new(
        configuration.clone(),
        serial_read_receiver,
        serial_write_sender,
    );

    let display: PrintDisplay = PrintDisplay::new(configuration.display.clone());

    let mut printer = Printer::new(configuration.printer.clone(), display, gcode);

    let runtime = build_runtime();

    runtime.block_on(async {
        let sender = printer.get_operation_sender().await.clone();
        let receiver = printer.get_status_receiver().await;

        let writer_serial = serial
            .try_clone_native()
            .expect("Unable to clone serial port handler");
        let listener_serial = serial
            .try_clone_native()
            .expect("Unable to clone serial port handler");

        tokio::spawn(
            async move { serial_handler::run_listener(listener_serial, serial_read_sender) },
        );
        tokio::spawn(
            async move { serial_handler::run_writer(writer_serial, serial_write_receiver) },
        );

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
