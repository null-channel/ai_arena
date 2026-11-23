use crate::server::sessions::GameStatus;

pub(super) fn layout(title: &str, content: &str) -> String {
    let mut page = String::new();
    page.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    page.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    page.push_str(&format!("<title>{}</title>", html_escape(title)));
    page.push_str("<script src=\"https://unpkg.com/htmx.org@2.0.4\"></script>");
    page.push_str("<style>");
    page.push_str(
        r#"
        :root {
            color-scheme: dark;
            --bg: #080b14;
            --panel: rgba(17, 24, 39, 0.86);
            --panel-strong: rgba(30, 41, 59, 0.92);
            --text: #e5e7eb;
            --muted: #94a3b8;
            --line: rgba(148, 163, 184, 0.18);
            --blue: #60a5fa;
            --purple: #a78bfa;
            --green: #34d399;
            --amber: #fbbf24;
            --red: #f87171;
        }
        * { box-sizing: border-box; }
        body {
            margin: 0;
            min-height: 100vh;
            color: var(--text);
            background:
                radial-gradient(circle at top left, rgba(96, 165, 250, 0.24), transparent 34rem),
                radial-gradient(circle at top right, rgba(167, 139, 250, 0.2), transparent 30rem),
                linear-gradient(135deg, #020617 0%, var(--bg) 48%, #111827 100%);
            font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }
        a { color: inherit; }
        code { color: #bfdbfe; }
        .shell { width: min(1180px, calc(100vw - 32px)); margin: 0 auto; padding: 32px 0 48px; }
        .hero { display: flex; align-items: stretch; justify-content: space-between; gap: 24px; margin-bottom: 28px; }
        h1 { margin: 4px 0 10px; font-size: clamp(3rem, 8vw, 6.5rem); line-height: 0.85; letter-spacing: -0.08em; }
        h2 { margin: 0; font-size: 1.35rem; letter-spacing: -0.03em; }
        h3 { margin: 0 0 12px; font-size: 1.05rem; }
        .eyebrow { margin: 0 0 8px; color: var(--blue); font-size: 0.76rem; font-weight: 800; letter-spacing: 0.16em; text-transform: uppercase; }
        .lede { max-width: 680px; margin: 0; color: var(--muted); font-size: 1.1rem; line-height: 1.6; }
        .hero-card, .panel, .session-card, .notice {
            border: 1px solid var(--line);
            background: linear-gradient(180deg, var(--panel), rgba(15, 23, 42, 0.72));
            box-shadow: 0 24px 80px rgba(0, 0, 0, 0.26);
            backdrop-filter: blur(16px);
            border-radius: 24px;
        }
        .hero-card { min-width: 220px; padding: 24px; display: grid; place-content: center; text-align: center; }
        .hero-card strong { display: block; font-size: 3rem; color: white; }
        .metric-label { color: var(--muted); font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.12em; }
        .grid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(360px, 0.85fr); gap: 24px; align-items: start; }
        .stack { display: grid; gap: 18px; }
        .panel { padding: 22px; }
        .panel-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
        .form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 18px; }
        .field { display: grid; gap: 6px; margin-bottom: 12px; }
        label { color: var(--muted); font-size: 0.78rem; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; }
        input, select {
            width: 100%;
            border: 1px solid rgba(148, 163, 184, 0.28);
            border-radius: 12px;
            background: rgba(2, 6, 23, 0.54);
            color: var(--text);
            padding: 11px 12px;
            outline: none;
        }
        input:focus, select:focus { border-color: var(--blue); box-shadow: 0 0 0 3px rgba(96, 165, 250, 0.16); }
        .game-options { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; margin-bottom: 18px; }
        .radio-card { display: block; position: relative; }
        .radio-card input { position: absolute; opacity: 0; pointer-events: none; }
        .radio-card span {
            min-height: 92px;
            display: flex;
            flex-direction: column;
            justify-content: center;
            gap: 5px;
            border: 1px solid rgba(148, 163, 184, 0.22);
            border-radius: 16px;
            background: rgba(15, 23, 42, 0.64);
            padding: 14px;
            cursor: pointer;
        }
        .radio-card strong { font-size: 0.95rem; }
        .radio-card small { color: var(--muted); line-height: 1.35; }
        .radio-card input:checked + span { border-color: var(--blue); background: rgba(37, 99, 235, 0.24); box-shadow: inset 0 0 0 1px rgba(96, 165, 250, 0.28); }
        .agent-card { border: 1px solid var(--line); border-radius: 18px; background: rgba(2, 6, 23, 0.32); padding: 16px; }
        .actions { display: flex; align-items: center; gap: 12px; margin-top: 18px; }
        button, .button {
            border: 0;
            border-radius: 14px;
            background: linear-gradient(135deg, #2563eb, #7c3aed);
            color: white;
            cursor: pointer;
            display: inline-flex;
            align-items: center;
            justify-content: center;
            font-weight: 800;
            gap: 8px;
            padding: 12px 16px;
            text-decoration: none;
        }
        .button.small { padding: 8px 10px; border-radius: 10px; font-size: 0.85rem; }
        .ghost { background: rgba(15, 23, 42, 0.9); border: 1px solid var(--line); }
        .sessions { display: grid; gap: 12px; }
        .session-card { padding: 16px; }
        .session-top { display: flex; align-items: start; justify-content: space-between; gap: 12px; }
        .session-id { color: var(--muted); font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.8rem; word-break: break-all; }
        .badge { border-radius: 999px; padding: 5px 9px; font-size: 0.72rem; font-weight: 900; text-transform: uppercase; letter-spacing: 0.08em; }
        .pending { color: var(--amber); background: rgba(251, 191, 36, 0.13); }
        .running { color: var(--blue); background: rgba(96, 165, 250, 0.13); }
        .completed { color: var(--green); background: rgba(52, 211, 153, 0.13); }
        .error { color: var(--red); background: rgba(248, 113, 113, 0.13); }
        .empty { color: var(--muted); border: 1px dashed var(--line); border-radius: 18px; padding: 28px; text-align: center; }
        .notice { padding: 16px; display: grid; gap: 8px; }
        .success { border-color: rgba(52, 211, 153, 0.35); }
        .failure { border-color: rgba(248, 113, 113, 0.35); }
        .back-link { color: var(--muted); text-decoration: none; }
        .result-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; margin: 18px 0; }
        .stat { border: 1px solid var(--line); border-radius: 16px; padding: 14px; background: rgba(2, 6, 23, 0.28); }
        .stat span { display: block; color: var(--muted); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.08em; }
        .stat strong { display: block; margin-top: 6px; font-size: 1.25rem; }
        table { width: 100%; border-collapse: collapse; overflow: hidden; border-radius: 14px; }
        th, td { border-bottom: 1px solid var(--line); padding: 10px; text-align: left; vertical-align: top; }
        th { color: var(--muted); font-size: 0.76rem; text-transform: uppercase; letter-spacing: 0.08em; }
        pre { overflow: auto; border: 1px solid var(--line); border-radius: 16px; background: rgba(2, 6, 23, 0.48); padding: 14px; }
        .board-wrap { margin: 18px 0; }
        .board-meta { color: var(--muted); margin: 0 0 12px; }
        .game-board { width: auto; border-collapse: separate; border-spacing: 6px; }
        .game-board td {
            width: 54px;
            height: 54px;
            border: 1px solid var(--line);
            border-radius: 14px;
            background: rgba(15, 23, 42, 0.7);
            text-align: center;
            vertical-align: middle;
            font-size: 1.4rem;
            font-weight: 900;
        }
        .game-board.chess { border-spacing: 0; border: 1px solid var(--line); border-radius: 14px; overflow: hidden; }
        .game-board.chess td { border-radius: 0; border: 0; width: 48px; height: 48px; font-size: 1.65rem; }
        .game-board.chess .light { background: #d6c6a8; color: #111827; }
        .game-board.chess .dark { background: #6b4f3a; color: #f8fafc; }
        .cell-empty { color: rgba(148, 163, 184, 0.35); }
        .piece-x, .piece-red { color: #f87171; }
        .piece-o, .piece-yellow { color: #fbbf24; }
        .piece-white { color: #f8fafc; text-shadow: 0 0 1px #0f172a, 0 0 2px #0f172a, 0 1px 2px rgba(15, 23, 42, 0.6); }
        .piece-black { color: #0b1120; text-shadow: 0 0 1px rgba(248, 250, 252, 0.7), 0 0 2px rgba(248, 250, 252, 0.45); }
        .rps-history { display: grid; gap: 10px; }
        .rps-round { display: flex; justify-content: space-between; gap: 12px; border: 1px solid var(--line); border-radius: 14px; padding: 12px; background: rgba(2, 6, 23, 0.28); }
        .player-legend { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; margin: 0 0 16px; }
        .player-chip { display: flex; align-items: center; gap: 12px; border: 1px solid var(--line); border-radius: 16px; padding: 12px 14px; background: rgba(2, 6, 23, 0.32); }
        .player-mark { width: 42px; height: 42px; border-radius: 12px; display: grid; place-items: center; font-size: 1.2rem; font-weight: 900; background: rgba(15, 23, 42, 0.8); }
        .player-info { display: grid; gap: 2px; min-width: 0; }
        .player-info strong { font-size: 0.95rem; word-break: break-word; }
        .player-info small { color: var(--muted); font-size: 0.76rem; }
        @media (max-width: 860px) {
            .hero, .grid, .form-grid, .result-grid { grid-template-columns: 1fr; display: grid; }
            .game-options { grid-template-columns: 1fr; }
            .hero-card { min-width: 0; }
        }
        "#,
    );
    page.push_str("</style></head><body><div class=\"shell\">");
    page.push_str(content);
    page.push_str("</div></body></html>");
    page
}

pub(super) fn render_status_badge(status: &GameStatus) -> String {
    let status_text = match status {
        GameStatus::Pending => "pending",
        GameStatus::Running => "running",
        GameStatus::Completed => "completed",
        GameStatus::Error => "error",
    };
    format!(
        "<span class=\"badge {}\">{}</span>",
        status_text, status_text
    )
}

pub(super) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
