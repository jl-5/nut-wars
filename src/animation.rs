pub struct Animation {
    // states are sprite sheet positions
    pub(crate) states: Vec<[f32; 4]>,
    // frame counter is how many frames have passed on the current animation state
    pub(crate) frame_counter: i32,
    // rate is how many frames need to pass to go to the next animation state
    pub(crate) rate: i32,
    // state_number is which frame of the animation we're on
    pub(crate) state_number: usize,
}

impl Animation {
    pub fn tick(&mut self){
        // iterate frame counter
        self.frame_counter += 1;

        // if enough frames have passed, go to the next frame of the animation
        if self.frame_counter > self.rate {
            self.state_number += 1;
            // if we've gone past the last frame of the animation, go back to the first frame
            if self.state_number >= self.states.len() as usize - 1 {
                self.state_number = 0;
            }
            self.frame_counter = 0;
        }
    }
    pub fn stop(&mut self){
        while self.state_number != 0 {
            self.tick();
        }
    }
    pub fn get_current_state(&mut self) -> [f32; 4]{
        return self.states[self.state_number]
    }
}