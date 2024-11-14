use std::time::Duration;

use odyssey::{
    self, api, configuration::Configuration, display::PrintDisplay, gcode::Gcode, printer::Printer,
};
use simple_logger::SimpleLogger;
use tokio::{
    runtime::{Builder, Runtime},
    sync::broadcast::{self, Receiver, Sender},
    time::interval,
};

mod common;

/**
 * Run Odyssey without any hardware
 */
#[test]
#[ignore]
fn no_hardware_mode() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let tmp_dir = tempfile::Builder::new()
        .prefix("odysseyTest")
        .tempdir()
        .unwrap();

    let fifo_path = tmp_dir.path().join("emulatedFramebuffer");

    nix::unistd::mkfifo(&fifo_path, nix::sys::stat::Mode::S_IRWXU).expect("Unable to create FIFO pipe");

    log::info!("Write frames to {}", fifo_path.display());

    let (serial_read_sender, serial_read_receiver) = broadcast::channel(200);
    let (serial_write_sender, serial_write_receiver) = broadcast::channel(200);

    let configuration =
        hardwareless_config(fifo_path.as_os_str().to_str().unwrap().to_owned());

    let gcode = Gcode::new(
        configuration.clone(),
        serial_read_receiver,
        serial_write_sender,
    );

    let display: PrintDisplay = PrintDisplay::new(configuration.display.clone());

    let mut printer = Printer::new(configuration.printer.clone(), display, gcode);
    
    let runtime = build_runtime();

    let handle = runtime.handle().clone();


    handle.block_on(async {
        let sender = printer.get_operation_sender().await.clone();
        let receiver = printer.get_status_receiver().await;

        tokio::spawn(serial_feedback_loop(
            configuration.clone(),
            serial_read_sender,
            serial_write_receiver,
        ));

        tokio::spawn(async move { printer.start_statemachine().await });

        tokio::spawn(async move {api::start_api(configuration, sender, receiver).await});

        tokio::signal::ctrl_c().await.expect("failed to listen for event");
        tmp_dir.close().expect("Unable to remove tempdir");
        log::info!("Shutting down");
    });
}

pub async fn serial_feedback_loop(
    configuration: Configuration,
    sender: Sender<String>,
    mut receiver: Receiver<String>,
) {
    let mut interval = interval(Duration::from_millis(100));

    loop {
        interval.tick().await;
        match receiver.try_recv() {
            Ok(command) => {
                sender
                    .send(
                        if command.as_str().trim()
                            == configuration.gcode.status_check.as_str().trim()
                        {
                            configuration.gcode.status_desired.clone()
                        } else {
                            configuration.gcode.move_sync.clone()
                        },
                    )
                    .expect("Unable to send gcode response message");
            }
            Err(err) => match err {
                broadcast::error::TryRecvError::Empty => continue,
                _ => panic!(),
            },
        };
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

fn hardwareless_config(framebuffer_filename: String) -> Configuration {
    let mut default_config = common::default_test_configuration();

    default_config.display.frame_buffer = framebuffer_filename;
    default_config.printer.serial = String::from("n/a");

    default_config
}
