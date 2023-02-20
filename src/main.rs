//#[macro_use] extern crate rocket;
use clap::Parser;
use settings::Settings;
use framebuffer::Framebuffer;
use std::{thread, time::Duration};

//mod api;
mod settings;
mod sl1;

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
    
    println!("settings: {:?}", settings);
    if args.file.is_some() {
        let print_file = args.file.unwrap();

        println!("file: {}", print_file);

        let mut file = sl1::Sl1::from_file(print_file);

        let mut framebuffer = Framebuffer::new(settings.printer.frame_buffer).unwrap();

        file.iter().for_each(|frame| {
            println!("file: {}, exposure: {}", frame.file_name, frame.exposure_time);
            framebuffer.write_frame(&frame.buffer);
            thread::sleep(Duration::from_secs_f32(frame.exposure_time));
        });

    }
}