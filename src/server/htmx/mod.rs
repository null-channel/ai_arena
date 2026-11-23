mod boards;
mod forms;
mod games;
mod index;
mod layout;
mod leaderboard;
mod matrix;
mod replay;

pub(super) use forms::{render_matrix_form, render_start_form};
pub(super) use games::{
    render_game_fragment, render_game_page, render_game_started, render_games_fragment,
};
pub(super) use index::render_index_page;
pub(super) use leaderboard::render_leaderboard_fragment;
pub(super) use matrix::{render_matrix_fragment, render_matrix_started};
pub(super) use replay::render_replay_page;
