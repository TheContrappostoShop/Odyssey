use framebuffer::Framebuffer;
use png::{Decoder};

#[derive(Clone)]
pub struct Frame {
    pub file_name: String,
    pub buffer: Vec<u8>,
    pub exposure_time: f32,
    pub bit_depth: u8,
}

impl Frame {
    pub fn from_vec(name: String, exposure_time: f32, data: Vec<u8>) -> Frame {
        let decoder = Decoder::new(data.as_slice());

        let mut png_reader = decoder.read_info().expect("Unable to read PNG metadata");

        let mut f = Frame {
            file_name: name,
            buffer: vec![0;png_reader.output_buffer_size()],
            exposure_time,
            bit_depth: png_reader.info().bit_depth as u8,
        };

        png_reader.next_frame(f.buffer.as_mut()).expect("Error reading PNG");

        f
    }
}

pub struct PrintDisplay {
    pub frame_buffer: Framebuffer,
    pub bit_depth: u8,
    pub chunk_size: u8,
}

impl PrintDisplay {
    fn re_encode(&self, mut frame: Frame) -> Frame {
        let pixels_per_chunk = self.chunk_size/self.bit_depth;
        let chunk_remainder = self.chunk_size%self.bit_depth;

        let depth_difference = frame.bit_depth - self.bit_depth;

        let mut new_buffer: Vec<u8> = Vec::new();

        frame.buffer.chunks_exact(pixels_per_chunk.into())
            .for_each(|pixel_chunk| {

                // raw binary chunk of pixels, to be broken into bytes and repacked in the Vector later
                let mut raw_chunk = 0b00000000000000000000000000000000;
                for i in 0..pixels_per_chunk {
                    // Truncate the pixel data to the display's bit depth, then shift it into place in the raw chunk
                    let shifted_pixel: u64 = ((pixel_chunk[i as usize] as u64) >> depth_difference)<< (i*self.bit_depth+chunk_remainder);
                    raw_chunk |= shifted_pixel;
                }

                for i in 0..(self.chunk_size/8) {
                    // pull the raw chunk back apart into bytes, for push into the new buffer
                    let byte = ((raw_chunk >> (8*i)) & 0xFF) as u8;
                    new_buffer.push(byte);
                }
        });

        frame.buffer = new_buffer;
        frame
    }

    pub fn display_frame(&mut self, mut frame: Frame) {
        if frame.bit_depth!=self.bit_depth {
            frame = self.re_encode(frame);
        }
        self.frame_buffer.write_frame(&frame.buffer);
    }
}

