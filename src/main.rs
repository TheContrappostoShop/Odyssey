//#[macro_use] extern crate rocket;
use clap::Parser;
use configuration::Configuration;

use display::PrintDisplay;
use framebuffer::Framebuffer;
use tokio::runtime::Builder;
use std::{thread, time::Duration};

use crate::{printer::{Printer, HardwareControl}, gcode::Gcode};

//mod api;
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
    #[arg(default_value_t=String::from("./config.json"), short, long)]
    config: String
}


fn main() {
    let args = Args::parse();

    let configuration: Configuration = configuration::Configuration::load(args.config).unwrap();

    let mut serial = serialport::new(
        configuration.printer.serial.clone(), configuration.printer.baud.clone()
    );

    let mut gcode = Gcode::new(configuration.clone(), serial);


    gcode.add_gcode_substitution("{max_z}".to_string(), configuration.printer.max_z.to_string());

    gcode.add_gcode_substitution("{z_lift}".to_string(), configuration.printer.z_lift.to_string());

    println!("settings: {:?}", configuration);

    let mut display: PrintDisplay = PrintDisplay{
        frame_buffer: Framebuffer::new(configuration.printer.frame_buffer.to_owned()).unwrap(),
        //frame_buffer: None,
        bit_depth: configuration.printer.fb_bit_depth,
        chunk_size: configuration.printer.fb_chunk_size,
    };

    let mut printer: Printer<Gcode> = Printer{
        config: configuration.printer,
        display: display,
        hardware_controller: gcode,
    };
    
    if args.file.is_some() {
        let print_file = args.file.unwrap();

        println!("Starting Odyssey in CLI mode, printing {}", print_file);

        // build runtime
        let runtime = Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("odyssey-worker")
            .thread_stack_size(3 * 1024 * 1024)
            .enable_time()
            .enable_io()
            .build()
            .unwrap();



        //let mut file = sl1::Sl1::from_file(&print_file);

        runtime.block_on(printer.print(print_file));

    }
}
