use super::{
    forms::{render_matrix_form, render_start_form},
    games::render_games_fragment,
    layout::layout,
    leaderboard::render_leaderboard_fragment,
    matrix::render_matrix_fragment,
};
use crate::{
    server::{config::FormDefaults, jobs::MatrixJobResponse, sessions::GameSummary},
    storage::LeaderboardEntry,
};

pub(in crate::server) fn render_index_page(
    profiles: &[String],
    defaults: &FormDefaults,
    games: &[GameSummary],
    jobs: &[MatrixJobResponse],
    leaderboard: Result<Vec<LeaderboardEntry>, String>,
) -> String {
    let content = format!(
        r##"
        <section class="hero">
            <div>
                <p class="eyebrow">AI Arena control room</p>
                <h1>AI Arena</h1>
            </div>
            <div class="hero-card">
                <span class="metric-label">Active Sessions</span>
                <strong hx-get="/games" hx-trigger="load, every 3s" hx-select="[data-session-count]" hx-swap="innerHTML">...</strong>
            </div>
        </section>

        <main class="grid">
            <section class="panel">
                <div class="panel-heading">
                    <div>
                        <p class="eyebrow">new match</p>
                        <h2>Configure Battle</h2>
                    </div>
                </div>
                <div id="start-form">
                    {}
                </div>
            </section>

            <section class="panel">
                <div class="panel-heading">
                    <div>
                        <p class="eyebrow">live room</p>
                        <h2>Sessions</h2>
                    </div>
                    <button class="ghost" hx-get="/games" hx-target="#session-list" hx-swap="outerHTML">Refresh</button>
                </div>
                <div id="session-list" hx-get="/games" hx-trigger="load, every 3s" hx-swap="outerHTML">
                    {}
                </div>
            </section>
        </main>
        <section class="panel stack">
            <div class="panel-heading">
                <div><p class="eyebrow">simulation matrix</p><h2>Run a Series</h2></div>
            </div>
            <div id="matrix-form">{}</div>
            <div id="matrix-list" hx-get="/matrix" hx-trigger="load, every 2s" hx-swap="outerHTML">{}</div>
        </section>
        <section class="panel stack">
            <div class="panel-heading"><div><p class="eyebrow">durable history</p><h2>Leaderboard</h2></div></div>
            <div id="leaderboard" hx-get="/leaderboard" hx-trigger="load, every 5s" hx-swap="outerHTML">{}</div>
        </section>
        "##,
        render_start_form(profiles, defaults, None),
        render_games_fragment(games),
        render_matrix_form(defaults, None),
        render_matrix_fragment(jobs),
        render_leaderboard_fragment(leaderboard),
    );

    layout("AI Arena", &content)
}
