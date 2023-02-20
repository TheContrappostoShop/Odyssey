use framebuffer::Framebuffer;

use crate::sl1::Frame;

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
                let mut has_data = false;
                if pixel_chunk[0]>0 || pixel_chunk[1]>0 || pixel_chunk[2]>0 {
                    has_data = true;
                    println!("New Chunk with data:");
                }
                // raw binary chunk of pixels, to be broken into bytes and repacked in the Vector later
                let mut raw_chunk = 0b00000000000000000000000000000000;
                for i in 0..pixels_per_chunk {
                    // Truncate the pixel data to the display's bit depth, then shift it into place in the raw chunk
                    let shifted_pixel: u64 = ((pixel_chunk[i as usize] as u64) >> depth_difference)<< (i*self.bit_depth+chunk_remainder);

                    if has_data {
                        println!("pixel: {:016b}", pixel_chunk[i as usize]);
                        println!("shifted pixel: {:016b}", shifted_pixel);
                    }

                    raw_chunk = raw_chunk | shifted_pixel;
                }

                if has_data {
                    println!("raw chunk: {:016b}", raw_chunk);
                }

                for i in 0..(self.chunk_size/8) {
                    // pull the raw chunk back apart into bytes, for push into the new buffer
                    let byte = ((raw_chunk >> (8*i)) & 0xFF) as u8;

                    if has_data {
                        println!("byte: {:08b}", byte);
                    }

                    new_buffer.push(byte);
                }
        });

        frame.buffer = new_buffer;
        return frame;
    }

    pub fn display_frame(&mut self, mut frame: Frame) {
        if frame.bit_depth!=self.bit_depth {
            frame = self.re_encode(frame);
        }
        self.frame_buffer.write_frame(&frame.buffer);
    }
}

