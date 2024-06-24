use framebuffer::Framebuffer;
use png::Decoder;

use crate::api_objects::DisplayTest;

#[derive(Clone)]
pub struct Frame {
    pub file_name: String,
    pub buffer: Vec<u8>,
    pub exposure_time: f64,
    pub bit_depth: u8,
}

impl Frame {
    pub fn from_vec(name: String, exposure_time: f64, data: Vec<u8>) -> Frame {
        let decoder = Decoder::new(data.as_slice());

        let mut png_reader = decoder.read_info().expect("Unable to read PNG metadata");

        let mut f = Frame {
            file_name: name,
            buffer: vec![0; png_reader.output_buffer_size()],
            exposure_time,
            bit_depth: png_reader.info().bit_depth as u8,
        };

        png_reader
            .next_frame(f.buffer.as_mut())
            .expect("Error reading PNG");

        f
    }
}

pub struct PrintDisplay {
    pub frame_buffer: Option<Framebuffer>,
    pub fb_path: String,
    pub bit_depth: Vec<u8>,
}

impl PrintDisplay {
    fn re_encode(&self, mut frame: Frame) -> Frame {
        let chunk_size: u8 = self.bit_depth.iter().sum();
        let pixels_per_chunk = self.bit_depth.len();

        let mut new_buffer: Vec<u8> = Vec::new();

        frame
            .buffer
            .chunks_exact(pixels_per_chunk.into())
            .for_each(|pixel_chunk| {
                // raw binary chunk of pixels, to be broken into bytes and repacked in the Vector later
                let mut raw_chunk = 0b0;
                let mut pos_shift = 0;
                for i in 0..pixels_per_chunk {
                    let depth_difference = frame.bit_depth - self.bit_depth[i];
                    pos_shift += self.bit_depth[i];

                    // Truncate the pixel data to the display's bit depth, then shift it into place in the raw chunk
                    let shifted_pixel: u64 =
                        ((pixel_chunk[i as usize] as u64) >> depth_difference) << (pos_shift);
                    raw_chunk |= shifted_pixel;
                }

                for i in 0..(chunk_size / 8) {
                    // pull the raw chunk back apart into bytes, for push into the new buffer
                    let byte = ((raw_chunk >> (8 * i)) & 0xFF) as u8;
                    new_buffer.push(byte);
                }
            });

        frame.buffer = new_buffer;
        frame
    }

    pub fn display_frame(&mut self, mut frame: Frame) {
        if !(self.bit_depth.len() == 1 && self.bit_depth[0] == frame.bit_depth) {
            frame = self.re_encode(frame);
        }
        if self.frame_buffer.is_some() {
            self.frame_buffer
                .as_mut()
                .unwrap()
                .write_frame(&frame.buffer);
        }
    }

    pub fn display_test(&mut self, test: DisplayTest) {
        match test {
            DisplayTest::White => self.display_test_white(),
            DisplayTest::Blank => self.display_test_blank(),
            _ => (),
        }
    }

    fn display_test_white(&mut self) {
        if let Some(fb) = self.frame_buffer.as_mut() {
            let test_bytes = vec![
                0xFF;
                (fb.fix_screen_info.line_length * fb.var_screen_info.yres_virtual)
                    as usize
            ];

            fb.write_frame(&test_bytes);
        }
    }

    fn display_test_blank(&mut self) {
        if let Some(fb) = self.frame_buffer.as_mut() {
            let test_bytes = vec![
                0x00;
                (fb.fix_screen_info.line_length * fb.var_screen_info.yres_virtual)
                    as usize
            ];

            fb.write_frame(&test_bytes);
        }
    }

    pub fn new(fb_path: String, bit_depth: Vec<u8>) -> PrintDisplay {
        PrintDisplay {
            frame_buffer: Framebuffer::new(fb_path.clone()).ok(),
            fb_path,
            bit_depth,
        }
    }
}

impl Clone for PrintDisplay {
    fn clone(&self) -> Self {
        Self::new(self.fb_path.clone(), self.bit_depth.clone())
    }
}
