pub struct GameState{
    pub chars_typed: u32,
    pub score: usize,
    pub score_changing: bool,
}

pub fn init_game_state() -> GameState {
    // any necessary functions
    GameState {
        chars_typed : 0,
        score : 0,
        score_changing : false,
    }
}