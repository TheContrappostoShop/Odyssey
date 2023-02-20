//#[macro_use] extern crate rocket;
use clap::Parser;
use settings::Settings;
use display::PrintDisplay;
use framebuffer::Framebuffer;
use std::{thread, time::Duration};

//mod api;
mod settings;
mod sl1;
mod display;

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

    let settings: Settings = settings::Settings::load(args.config).unwrap();

    let mut display: PrintDisplay = PrintDisplay{
        frame_buffer: Framebuffer::new(settings.printer.frame_buffer.to_owned()).unwrap(),
        //frame_buffer: None,
        bit_depth: settings.printer.fb_bit_depth,
        chunk_size: settings.printer.fb_chunk_size,
    };
    
    println!("settings: {:?}", settings);
    if args.file.is_some() {
        let print_file = args.file.unwrap();

        println!("file: {}", print_file);

        let mut file = sl1::Sl1::from_file(print_file);

        file.iter().for_each(|frame| {
            let exposure_time = frame.exposure_time;
            println!("file: {}, exposure: {}", frame.file_name, exposure_time);
            display.display_frame(frame);
            thread::sleep(Duration::from_secs_f32(exposure_time));
        });

    }
}