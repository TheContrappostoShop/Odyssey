use framebuffer::Framebuffer;

// Wrap the real framebuffer in this, with a corresponding write call
// If none, write to file at given path instead, so we can pretend it's real
// and examine the output manually if we like

// Make python script to read the output and structure it in a graphical window
// emulating the display itself
pub struct FBWrapped {
    pub frame_buffer: Option<Framebuffer>,
    pub fb_path: String
}

impl FBWrapped {
    ///Writes a frame to the Framebuffer, or to the fb_path if not a real buffer
    pub fn write_frame(&mut self, frame: &[u8]) {
        match self.frame_buffer {
            Some(fb) => fb.write_frame(frame),
            None => {
                
            }
        }
    }
}
