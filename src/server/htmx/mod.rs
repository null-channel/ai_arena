mod boards;
mod forms;
mod games;
mod history;
mod index;
mod layout;
mod leaderboard;
mod matrix;
mod replay;

pub(super) use forms::{render_form, render_start_form};
pub(super) use games::{render_game_fragment, render_game_page, render_games_fragment};
pub(super) use history::{render_history, render_recent_batches, render_recent_results};
pub(super) use index::render_index_page;
pub(super) use leaderboard::{render_leaderboard_fragment, render_rankings_page};
pub(super) use matrix::{render_batch_activity, render_batch_page};
pub(super) use replay::render_replay_page;

pub(super) use layout::layout;
