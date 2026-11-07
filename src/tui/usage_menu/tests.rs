//! The screen, drawn at widths from 40 to 240 columns and read back as text:
//! fx's own cases (`vercel-labs/fx@481104fd
//! src/ui/footer/usage_presentation.zig:1630-2006`, the Gateway half), and
//! xfx's for what it does differently.

use super::*;
use crate::tui::theme::Mode;
use crate::usage::report::{Budget, RefreshPeriod, API_KEY_LABEL};
use crate::usage::view::View;

const MINUTE_MS: i64 = 60 * 1000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;

/// fx's `test_now`: 2026-10-11 08:16 UTC (`usage_presentation.zig:1633`).
const NOW: i64 = 20737 * DAY_MS + 8 * HOUR_MS + 16 * MINUTE_MS;
const WIDTHS: [u16; 9] = [40, 48, 56, 64, 80, 100, 120, 180, 240];
const LABEL: &str = "acme-design-tools-platform (fx login)";

const PALETTE: Palette = Palette { mode: Mode::Dark };

/// fx's `testRow` (`:1636-1652`).
fn row(model: &str, cost: f64, requests: u64, tokens: u64) -> ModelRow {
    let n = requests as f64;
    ModelRow {
        model: model.to_string(),
        totals: Totals {
            request_count: requests,
            input_tokens: tokens / 100,
            output_tokens: tokens / 50,
            reasoning_tokens: tokens / 60,
            cache_read_tokens: tokens - tokens / 100 - tokens / 50 - tokens / 20,
            cache_write_tokens: tokens / 20,
            total_cost: cost,
            cache_hit_requests: n * 0.954,
            failed_requests: n * 0.004,
            ttft_weighted: 2100.0 * n,
            ttft_requests: requests,
            tps_weighted: 58.0 * n,
            tps_requests: requests,
        },
    }
}

/// fx's `month_models` (`:1654-1660`).
fn month_models() -> Vec<ModelRow> {
    vec![
        row("anthropic/claude-opus-5.5", 6551.12, 43312, 14_500_000_000),
        row("openai/gpt-5.6-luna", 14.10, 5396, 41_000_000),
        row("openai/gpt-6.1-sol", 11.02, 640, 29_000_000),
        row(
            "google/gemini-3.1-flash-lite-image-preview",
            9.80,
            212,
            3_900_000,
        ),
        row("anthropic/claude-haiku-4.5", 6.12, 363, 9_100_000),
    ]
}

/// fx's `testView` (`:1668-1683`).
fn view(period: Period, rows: &[ModelRow], budget: bool) -> View {
    let mut totals = Totals::default();
    for row in rows {
        totals.add(&row.totals);
    }
    View {
        period,
        models: rows.to_vec(),
        totals,
        today_current: true,
        finished_current: true,
        fetched_at_ms: Some(NOW - 2 * MINUTE_MS),
        budget: budget.then_some(Budget {
            limit: 31600.0,
            spent: 6596.55,
            period: RefreshPeriod::Monthly,
        }),
    }
}

/// fx's `testSnapshot` (`:1685-1699`): today the first three, 7d the first
/// four, the month all of them.
fn snapshot(status: Status, budget: bool, month: &[ModelRow]) -> Snapshot {
    Snapshot::of_views(
        [
            view(Period::Today, &month[..month.len().min(3)], budget),
            view(Period::Days7, &month[..month.len().min(4)], budget),
            view(Period::Month, month, budget),
        ],
        status,
        LABEL,
        NOW,
    )
}

fn screen(snapshot: Snapshot) -> Screen {
    Screen::open(Period::Month, snapshot)
}

/// `row` without its escape sequences.
fn plain(row: &str) -> String {
    let mut out = String::new();
    let mut rest = row;
    while let Some(at) = rest.find('\u{1b}') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let end = tail
            .char_indices()
            .skip(1)
            .find(|(_, c)| c.is_ascii_alphabetic())
            .map_or(tail.len(), |(i, c)| i + c.len_utf8());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Every row the screen draws `width` wide, as plain text, trailing blanks
/// trimmed as a terminal grid reads them back (fx's `draw`, `:1726-1758`).
fn draw(screen: &Screen, width: u16, visible: u16) -> Vec<String> {
    let rows = u16::try_from(screen.desired_rows()).unwrap().min(visible);
    screen
        .rows(width, rows, PALETTE)
        .iter()
        .map(|row| {
            assert!(
                cells(&plain(row)) <= usize::from(width),
                "{row:?} past {width}"
            );
            plain(row).trim_end().to_string()
        })
        .collect()
}

fn find(rows: &[String], needle: &str) -> Option<usize> {
    rows.iter().position(|row| row.contains(needle))
}

/// The column where `needle` ends in `line` (fx's `endColumn`, `:1760-1764`).
fn end_column(line: &str, needle: &str) -> Option<usize> {
    let at = line.find(needle)?;
    Some(cells(&line[..at + needle.len()]))
}

fn widest(rows: &[String]) -> usize {
    rows.iter().map(|row| cells(row)).max().unwrap_or(0)
}

#[test]
fn the_df_view_lines_its_numbers_up_under_the_headers_at_every_width() {
    // `:1766-1803@481104fd`.
    let screen = screen(snapshot(Status::Ready, true, &month_models()));
    for width in WIDTHS {
        let rows = draw(&screen, width, 40);
        // The label on the left, the freshness in the right corner.
        assert!(
            rows[0].starts_with("usage: acme-de"),
            "{width}: {}",
            rows[0]
        );
        assert!(rows[0].ends_with("updated 2m ago"), "{width}: {}", rows[0]);
        assert_eq!(cells(&rows[0]), usize::from(width));
        assert_eq!(width >= 61, rows[0].contains(LABEL), "{width}");

        // Period table: numbers end where their headers end.
        assert_eq!(rows[2], "period        spent  requests  tokens");
        let spent_end = end_column(&rows[2], "spent");
        assert_eq!(spent_end, end_column(&rows[3], "$6,576.24"));
        assert_eq!(spent_end, end_column(&rows[5], "$6,592.16"));
        assert_eq!(
            end_column(&rows[2], "requests"),
            end_column(&rows[5], "49,923")
        );
        assert!(rows[4].starts_with("7d "));

        // Budget table, with fewer columns as the screen narrows.
        let budget = find(&rows, "budget").unwrap();
        assert_eq!(
            end_column(&rows[budget], "use%"),
            end_column(&rows[budget + 1], "21%")
        );
        assert_eq!(width >= 55, rows[budget + 1].contains("nov 1"), "{width}");
        assert_eq!(width >= 47, rows[budget].contains("size"), "{width}");

        // Model table: the current model carries the marker.
        let header = find(&rows, "model (month)").unwrap();
        assert!(rows[header + 1].starts_with("\u{276f} anthropi"));
        assert!(rows[header + 2].starts_with("  openai/g"));
        // Names stay whole once the longest fits beside spent and requests.
        assert_eq!(
            usize::from(width) >= GUTTER + 42 + ModelColumns::Small.width(),
            find(&rows, "google/gemini-3.1-flash-lite-image-preview").is_some(),
            "{width}"
        );
        let model_spent = end_column(&rows[header], "spent");
        assert_eq!(model_spent, end_column(&rows[header + 1], "$6,551.12"));
        assert_eq!(model_spent, end_column(&rows[header + 2], "$14.10"));
    }
}

#[test]
fn the_df_view_stays_content_sized_on_wide_screens_and_drops_columns_on_narrow_ones() {
    // `:1805-1841@481104fd`.
    let screen = screen(snapshot(Status::Ready, true, &month_models()));
    let content = GUTTER + 42 + ModelColumns::Full.width();
    let at_100 = draw(&screen, 100, 40);
    let at_240 = draw(&screen, 240, 40);
    // Only the heading spans the screen, to put the status in the corner.
    assert_eq!(widest(&at_100[1..]), content);
    assert_eq!(widest(&at_240[1..]), content);
    assert_eq!(at_100[1..], at_240[1..]);
    assert!(at_240[0].ends_with("updated 2m ago"));
    assert_eq!(cells(&at_240[0]), 240);

    // 80 columns: no room for share beside a 42-column name. 72: tokens go
    // too. The names stay whole in both.
    let at_80 = draw(&screen, 80, 40);
    let h80 = &at_80[find(&at_80, "model (month)").unwrap()];
    assert!(!h80.contains("share") && h80.contains("tokens"), "{h80}");
    assert!(find(&at_80, "google/gemini-3.1-flash-lite-image-preview").is_some());
    let at_72 = draw(&screen, 72, 40);
    let h72 = &at_72[find(&at_72, "model (month)").unwrap()];
    assert!(!h72.contains("tokens") && h72.contains("requests"), "{h72}");
    assert!(find(&at_72, "google/gemini-3.1-flash-lite-image-preview").is_some());
    // 40 columns: the long name is cut in the middle, never wrapped.
    let at_40 = draw(&screen, 40, 40);
    assert!(find(&at_40, "google/gemini-3.1-flash-lite-image-preview").is_none());
    assert!(find(&at_40, "google").is_some());
    assert!(widest(&at_40) <= 40);
}

#[test]
fn the_detail_screen_shows_one_model_across_today_7d_and_month() {
    // `:1843-1875@481104fd`: gpt-6.1-sol is listed today; gemini only in 7d
    // and the month.
    let mut screen = screen(snapshot(Status::Ready, true, &month_models()));
    screen.selected = 3;
    assert!(screen.open_detail());
    for width in WIDTHS {
        let rows = draw(&screen, width, 40);
        assert_eq!(rows.len(), DETAIL_ROWS);
        assert!(
            rows[0].contains("model 4 of 5") || width < 80,
            "{width}: {}",
            rows[0]
        );
        assert!(rows[0].ends_with("updated 2m ago"));
        if usize::from(width) >= cells_width(&DETAIL_PERIOD_HEADER) {
            assert_eq!(rows[2], "period        spent   share  requests  tokens");
        } else {
            assert!(rows[2].starts_with("period        spent   share  requests"));
        }
        // Not used today: dashes, aligned under the headers.
        assert!(rows[3].starts_with("today"));
        assert_eq!(end_column(&rows[2], "spent"), end_column(&rows[3], "-"));
        assert_eq!(end_column(&rows[2], "spent"), end_column(&rows[5], "$9.80"));

        let tokens = find(&rows, "tokens ").unwrap();
        let wide = width >= 62;
        assert_eq!(wide, rows[tokens].contains("cache write"), "{width}");
        assert_eq!(
            !wide && usize::from(width) >= cells_width(&TOKENS_NARROW),
            rows[tokens].contains("cache wr") && !rows[tokens].contains("cache write"),
            "{width}"
        );
        let perf = find(&rows, "perf").unwrap();
        assert_eq!(
            end_column(&rows[perf], "cache hit"),
            end_column(&rows[perf + 3], "95.4%")
        );
        assert!(rows[perf + 3].contains("2.1s"));
        assert!(rows[perf + 3].contains("0.4%"));
        assert!(widest(&rows) <= usize::from(width));
    }
}

#[test]
fn the_model_list_scrolls_and_sums_what_is_below_the_window() {
    // `:1877-1900@481104fd`.
    let many: Vec<ModelRow> = (0..25)
        .map(|i| row(&format!("provider/m{i:02}"), f64::from(100 - i), 10, 1000))
        .collect();
    let screen = screen(snapshot(Status::Ready, false, &many));
    assert_eq!(
        screen.desired_rows(),
        screen.table_rows() + MAX_MODEL_ROWS + 1
    );
    let rows = draw(&screen, 80, 40);
    assert!(find(&rows, "provider/m09").is_some());
    assert!(find(&rows, "provider/m10").is_none());
    // m10 through m24: 15 models, $90 down to $76.
    let more = &rows[find(&rows, "15 more").unwrap()];
    assert!(more.contains("$1,245.00"), "{more}");

    // Fewer rows than wanted: the window shrinks, the "more" line stays.
    let short = draw(&screen, 80, u16::try_from(screen.table_rows() + 4).unwrap());
    assert!(find(&short, "provider/m02").is_some() && find(&short, "provider/m03").is_none());
    assert!(find(&short, "22 more").is_some());
}

#[test]
fn the_window_follows_the_selection_and_more_sums_only_what_is_below_it() {
    // `p5-usage-many`'s `bottom` (`many-40x120-bottom@481104fd`): eleven Downs
    // past the window, `vendor-02` … `❯ vendor-11`, then `2 more`.
    let many: Vec<ModelRow> = (0..14)
        .map(|i| row(&format!("vendor-{i:02}"), f64::from(100 - i), 10, 1000))
        .collect();
    let mut screen = screen(snapshot(Status::Ready, false, &many));
    let visible = screen.visible_models(40);
    assert_eq!(visible, MAX_MODEL_ROWS);
    for _ in 0..11 {
        assert!(screen.move_model(true, visible));
    }
    let rows = draw(&screen, 120, 40);
    let header = find(&rows, "model (month)").unwrap();
    assert!(
        rows[header + 1].starts_with("  vendor-02"),
        "{}",
        rows[header + 1]
    );
    assert!(rows[header + 10].starts_with("\u{276f} vendor-11"));
    let more = &rows[header + 11];
    // vendor-12 and -13 are below, $88 and $87; vendor-00 and -01 above are
    // not summed.
    assert!(
        more.starts_with("  2 more") && more.contains("$175.00"),
        "{more}"
    );
    assert_eq!(screen.marked_row(40), u16::try_from(header + 10).unwrap());
    // The ends hold.
    assert!(screen.move_model(true, visible));
    assert!(screen.move_model(true, visible));
    assert!(!screen.move_model(true, visible));
    for _ in 0..13 {
        assert!(screen.move_model(false, visible));
    }
    assert!(!screen.move_model(false, visible));
    assert_eq!(screen.window_start, 0);
}

#[test]
fn states_without_numbers_say_so_in_one_line() {
    // `:1902-1938@481104fd`, with xfx's own no-sign-in text.
    let draw_one =
        |status: Status, month: &[ModelRow]| draw(&screen(snapshot(status, false, month)), 80, 40);
    let rows = draw_one(Status::Unavailable(Unavailable::NoGatewaySignIn), &[]);
    assert_eq!(
        rows,
        [
            "usage: not signed in to AI Gateway",
            "",
            "  (set AI_GATEWAY_API_KEY)"
        ]
    );
    let rows = draw_one(Status::Unavailable(Unavailable::UnsupportedProvider), &[]);
    assert_eq!(rows[0], "usage: this provider's usage isn't shown yet");
    assert_eq!(
        rows[2],
        "  (use /provider and pick AI Gateway to see AI Gateway usage)"
    );
    let rows = draw_one(Status::Unavailable(Unavailable::ProviderRejected), &[]);
    assert_eq!(rows[0], "usage: the provider settings can't be used");
    assert_eq!(rows[2], "  (send a message to see why, then reopen /usage)");

    // Nothing cached yet, the first fetch out.
    let fetching = snapshot(Status::Fetching, false, &[]);
    let fetching = Snapshot::of_views(
        Period::ALL.map(|period| View {
            today_current: false,
            finished_current: false,
            fetched_at_ms: None,
            ..fetching.of(period).clone()
        }),
        Status::Fetching,
        LABEL,
        NOW,
    );
    let rows = draw(&screen(fetching.clone()), 80, 40);
    assert!(
        rows[0].starts_with(&format!("usage: {LABEL}  ")),
        "{}",
        rows[0]
    );
    assert!(rows[0].ends_with("fetching\u{2026}"));
    assert_eq!(cells(&rows[0]), 80);
    assert_eq!(rows[2], "  fetching usage from AI Gateway");
    // The same failed: `(press r to retry)`, the failure in the corner.
    let failed = Snapshot::of_views(
        Period::ALL.map(|period| fetching.of(period).clone()),
        Status::Failed(Failure::NotAvailable),
        API_KEY_LABEL,
        NOW,
    );
    let rows = draw(&screen(failed), 120, 40);
    assert_eq!(
        rows[0],
        format!(
            "usage: {API_KEY_LABEL}{}usage reports aren't available for this team",
            " ".repeat(120 - 7 - API_KEY_LABEL.len() - cells(failure_text(Failure::NotAvailable)))
        )
    );
    assert_eq!(rows[2], "  (press r to retry)");

    // Signed in, numbers fetched, nothing used today.
    let mut empty_today = snapshot(Status::Ready, false, &month_models());
    empty_today = Snapshot::of_views(
        [
            view(Period::Today, &[], false),
            empty_today.of(Period::Days7).clone(),
            empty_today.of(Period::Month).clone(),
        ],
        Status::Ready,
        LABEL,
        NOW,
    );
    let today = draw(&Screen::open(Period::Today, empty_today), 80, 40);
    assert!(find(&today, "no usage today (UTC)").is_some());
    assert!(today[3].starts_with("today       $0.0000"), "{}", today[3]);
}

#[test]
fn the_heading_drops_its_status_before_its_name_and_cuts_the_name_in_the_middle() {
    // `budget-40x50-open@481104fd`: `usage: API key (AI_…WAY_API_KEY)  updated
    // just now`; a failure too long for the corner is dropped.
    let mut ready = snapshot(Status::Ready, true, &month_models());
    ready = Snapshot::of_views(
        Period::ALL.map(|period| View {
            fetched_at_ms: Some(NOW),
            ..ready.of(period).clone()
        }),
        Status::Ready,
        API_KEY_LABEL,
        NOW,
    );
    let rows = draw(&screen(ready.clone()), 50, 40);
    assert_eq!(
        rows[0],
        "usage: API key (AI_\u{2026}WAY_API_KEY)  updated just now"
    );
    let failed = Snapshot::of_views(
        Period::ALL.map(|period| ready.of(period).clone()),
        Status::Failed(Failure::RateLimited),
        API_KEY_LABEL,
        NOW,
    );
    let rows = draw(&screen(failed.clone()), 50, 40);
    assert_eq!(rows[0], "usage: API key (AI_GATEWAY_API_KEY)");
    // A failure is in normal weight, freshness dim.
    let paint = |snapshot: Snapshot| screen(snapshot).rows(120, 1, PALETTE)[0].clone();
    assert!(!paint(failed).contains(&format!("{}rate limited", PALETTE.dim())));
    assert!(paint(ready).contains(&format!("{}updated just now", PALETTE.dim())));
}

#[test]
fn the_current_model_is_bold_and_colourless() {
    // `:1940-1952@481104fd`.
    let screen = screen(snapshot(Status::Ready, false, &month_models()));
    let rows = screen.rows(120, 40, PALETTE);
    let selected = &rows[screen.table_rows()];
    assert!(
        selected.starts_with(&format!("{}\u{276f} ", PALETTE.bold())),
        "{selected:?}"
    );
    // No foreground or background colour anywhere in the row.
    assert!(!selected.contains("[3") && !selected.contains("[4") && !selected.contains(";3"));
}

/// Model names that would move the cursor, clear or reset the screen, set the
/// clipboard or the title, ring, back up, reorder text or hide it.
const HOSTILE: [&str; 14] = [
    "evil/\u{1b}[2Jmodel",
    "evil/\u{1b}]52;c;aGk=\u{7}clip",
    "evil/\u{1b}]0;title\u{1b}\\st",
    "evil/\u{9b}31mred",
    "evil/\u{9d}0;c1-osc\u{9c}",
    "evil/\u{202e}edoc",
    "evil/\u{2066}isolate\u{2069}",
    "evil/\u{200b}hidden\u{feff}",
    "evil/line\nbreak\rback",
    "evil/\u{1b}[1;1H\u{1b}[0Kreset",
    "evil/\u{1b}cfull-reset",
    "evil/\u{8}\u{8}backspace\u{7f}",
    "evil/\u{7}bell",
    "evil/\u{2028}separator\u{200e}",
];

/// What a terminal would obey, or draw as nothing: every control (C0, DEL,
/// C1) and the bidi, zero-width and separator code points.
fn obeyed(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{feff}'
            | '\u{061c}')
}

/// What is left of a painted row once exactly the palette sequences the
/// screen paints with are taken out: anything a terminal obeys in it is
/// foreign -- a cursor move or a clear as much as a raw control.
fn foreign(row: &str) -> Vec<char> {
    let mut rest = row.to_string();
    for sequence in [PALETTE.bold(), PALETTE.dim(), PALETTE.reset()] {
        rest = rest.replace(sequence, "");
    }
    rest.chars().filter(|c| obeyed(*c)).collect()
}

#[test]
fn a_hostile_model_name_is_drawn_inert_on_the_table_and_on_the_detail_screen() {
    // fx's `:1954-1980@481104fd`, spelled where fx writes `?`
    // (`crate::mcp::terminal_safe`). Every row, at every width, on both
    // screens, holds nothing a terminal obeys but the palette's own
    // sequences.
    for name in HOSTILE {
        let rows = [row(name, 1.0, 1, 1), row("plain/model", 0.5, 1, 1)];
        let mut screen = screen(snapshot(Status::Ready, false, &rows));
        let spelled = crate::mcp::terminal_safe(name);
        assert_ne!(spelled, name);
        assert!(spelled.chars().all(|c| !obeyed(c)), "{spelled:?}");
        for detail in [false, true] {
            if detail {
                assert!(screen.open_detail());
            }
            for width in WIDTHS {
                let painted = screen.rows(width, 40, PALETTE);
                for row in &painted {
                    assert!(foreign(row).is_empty(), "{name:?} at {width}: {row:?}");
                }
                if width >= 120 {
                    let at = if detail { 0 } else { screen.table_rows() };
                    let text = plain(&painted[at]);
                    assert!(text.contains(&spelled), "{name:?}: {text:?}");
                }
            }
        }
    }
    // The check itself: a palette sequence passes, anything else does not.
    assert!(foreign(&format!("{}x{}", PALETTE.bold(), PALETTE.reset())).is_empty());
    assert_eq!(foreign("x\u{1b}[2Jy"), ['\u{1b}']);
    assert_eq!(foreign("x\u{1b}[1;1Hy"), ['\u{1b}']);
}

#[test]
fn the_name_column_is_measured_on_the_names_as_drawn() {
    let name = "evil/\u{1b}[2Jmodel";
    let rows = [row(name, 1.0, 1, 1)];
    let screen = screen(snapshot(Status::Ready, false, &rows));
    let rows = draw(&screen, 120, 40);
    let header = find(&rows, "model (month)").unwrap();
    // `spent` ends where the spelled name's column ends plus its 11.
    let name_width = cells(&crate::mcp::terminal_safe(name));
    assert_eq!(
        end_column(&rows[header], "spent"),
        Some(GUTTER + name_width + 11)
    );
    assert_eq!(
        end_column(&rows[header + 1], "$1.00"),
        Some(GUTTER + name_width + 11)
    );
}

/// The key every case below asks with; neither it nor its first 12 bytes
/// may be on any screen.
const SENTINEL: &str = "xfx-sentinel-0d9e3b7a5c1f-key-must-not-appear";

/// What `/usage` asks with `key` for a Gateway at `origin`: the question the
/// runtime publishes for that configuration.
fn question_at(origin: &str, key: &str) -> crate::usage::report::Question {
    let home = tempfile::tempdir().expect("a home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let config = crate::config::RuntimeConfig::load_with(
        &crate::config::Environment::new(
            Some(home.path().to_path_buf()),
            std::collections::BTreeMap::from([("AI_GATEWAY_API_KEY".to_string(), key.to_string())]),
        ),
        workspace.path(),
    )
    .expect("load a configuration");
    let endpoint =
        crate::gateway::Endpoint::resolve(Some(&format!("{origin}/v4/ai/language-model")))
            .expect("a loopback endpoint");
    crate::usage::report::question(&config, &endpoint).expect("a key is asked")
}

/// A loopback Gateway answering every request with one status and body,
/// until it is dropped (and for at most 20 s).
struct Answering {
    origin: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Answering {
    fn new(status: u16, body: &str) -> Self {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        listener.set_nonblocking(true).expect("a polled listener");
        let origin = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = std::sync::Arc::clone(&stop);
        let answer = format!(
            "HTTP/1.1 {status} Fake\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let thread = std::thread::spawn(move || {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while !stopped.load(std::sync::atomic::Ordering::SeqCst)
                && std::time::Instant::now() < until
            {
                let Ok((mut connection, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                };
                let _ = connection.set_nonblocking(false);
                let _ = connection.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < 65536 {
                    match connection.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&chunk[..n]),
                    }
                }
                let _ = connection.write_all(answer.as_bytes());
                let _ = connection.shutdown(std::net::Shutdown::Write);
            }
        });
        Self {
            origin,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Answering {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Ticks `dashboard` until its fetch has ended (at most 20 s).
fn settled(dashboard: &mut crate::usage::dashboard::Dashboard) -> Snapshot {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        dashboard.tick();
        let snapshot = dashboard.snapshot();
        if snapshot.status != Status::Fetching {
            return snapshot;
        }
        assert!(std::time::Instant::now() < until, "the fetch never ended");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// One state the runtime is driven into: its name, what the Gateway answers
/// (none: nothing listens), and the status it ends in.
type Case<'a> = (&'a str, Option<(u16, String)>, Status);

#[test]
fn no_screen_the_runtime_reaches_names_the_key() {
    // S3 through the production path: a question carrying a sentinel key,
    // the runtime driven into each state it has -- fetching, ready, every
    // failure a Gateway can cause (an echo of the key among them), and each
    // unavailable sign-in -- and every row drawn from its snapshot, both
    // screens, every width, escapes and all.
    use crate::usage::dashboard::Dashboard;
    let spawn = || -> crate::usage::dashboard::Spawn {
        Box::new(|job| {
            std::thread::Builder::new()
                .name("usage-screen-test".to_string())
                .spawn(job)
        })
    };
    let mut reached: Vec<(String, Snapshot)> = Vec::new();

    // Fetching: a worker that has not answered.
    let held = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let holding = std::sync::Arc::clone(&held);
    let mut fetching = Dashboard::new(
        None,
        crate::usage::dashboard::wall_clock(),
        Box::new(move |job| {
            holding.lock().expect("the jobs").push(job);
            std::thread::Builder::new().spawn(|| {})
        }),
    );
    fetching.open(question_at("http://127.0.0.1:9", SENTINEL));
    let snapshot = fetching.snapshot();
    assert_eq!(snapshot.status, Status::Fetching);
    reached.push(("fetching".to_string(), snapshot));
    fetching.quit();

    let report = r#"{"results":[{"model":"anthropic/claude-opus-5","request_count":2,"input_tokens":10,"total_cost":0.5}]}"#;
    let echo = |name: &str| {
        format!(r#"{{"results":[{{"model":"{name}","request_count":1,"total_cost":0.1}}]}}"#)
    };
    let cases: [Case<'_>; 9] = [
        ("ready", Some((200, report.to_string())), Status::Ready),
        (
            "401",
            Some((401, "{}".to_string())),
            Status::Failed(Failure::Refused),
        ),
        (
            "404",
            Some((404, "{}".to_string())),
            Status::Failed(Failure::NotAvailable),
        ),
        (
            "429",
            Some((429, "{}".to_string())),
            Status::Failed(Failure::RateLimited),
        ),
        (
            "500",
            Some((500, "{}".to_string())),
            Status::Failed(Failure::Server),
        ),
        (
            "not json",
            Some((200, "not json".to_string())),
            Status::Failed(Failure::BadResponse),
        ),
        (
            "the key echoed",
            Some((200, echo(SENTINEL))),
            Status::Failed(Failure::BadResponse),
        ),
        (
            "its first 12 bytes echoed",
            Some((200, echo(&SENTINEL[..12]))),
            Status::Failed(Failure::BadResponse),
        ),
        ("no answer", None, Status::Failed(Failure::Network)),
    ];
    for (name, answer, expected) in cases {
        let gateway = answer.map(|(status, body)| Answering::new(status, &body));
        let origin = gateway
            .as_ref()
            .map_or_else(|| "http://127.0.0.1:9".to_string(), |g| g.origin.clone());
        let mut dashboard = Dashboard::new(None, crate::usage::dashboard::wall_clock(), spawn());
        dashboard.open(question_at(&origin, SENTINEL));
        let snapshot = settled(&mut dashboard);
        assert_eq!(snapshot.status, expected, "{name}");
        reached.push((name.to_string(), snapshot));
        dashboard.quit();
    }
    for reason in [
        Unavailable::NoGatewaySignIn,
        Unavailable::UnsupportedProvider,
        Unavailable::ProviderRejected,
    ] {
        let mut dashboard = Dashboard::new(None, crate::usage::dashboard::wall_clock(), spawn());
        dashboard.open(question_at("http://127.0.0.1:9", SENTINEL));
        dashboard.open_unavailable(reason);
        reached.push((format!("{reason:?}"), dashboard.snapshot()));
        dashboard.quit();
    }

    let prefix = &SENTINEL[..12];
    for (name, snapshot) in reached {
        assert!(!format!("{snapshot:?}").contains(prefix), "{name}");
        for detail in [false, true] {
            let mut screen = Screen::open(Period::Month, snapshot.clone());
            if detail {
                screen.open_detail();
            }
            for width in WIDTHS {
                for row in screen.rows(width, 40, PALETTE) {
                    assert!(
                        !row.contains(SENTINEL) && !row.contains(prefix),
                        "{name} at {width}: {row:?}"
                    );
                }
                assert!(!screen.key_help(width).contains(prefix));
            }
            if heading_is_the_label(&screen) {
                assert!(
                    plain(&screen.rows(120, 40, PALETTE)[0]).contains(API_KEY_LABEL),
                    "{name}"
                );
            }
        }
    }
}

/// Whether the screen's heading is the sign-in's label: the tables of a
/// sign-in that can be asked.
fn heading_is_the_label(screen: &Screen) -> bool {
    !screen.detail && !matches!(screen.shown().status, Status::Unavailable(_))
}

#[test]
fn any_clock_draws_without_overflowing() {
    // The pinned clock may read any `i64` (`crate::usage::clock`): elapsed
    // time saturates, and the budget's day floors.
    for (now, at) in [
        (i64::MIN, i64::MAX),
        (i64::MAX, i64::MIN),
        (i64::MIN, i64::MIN),
        (-1, 0),
        (0, -1),
    ] {
        let snapshot = Snapshot::of_views(
            Period::ALL.map(|period| View {
                fetched_at_ms: Some(at),
                ..view(period, &month_models(), true)
            }),
            Status::Ready,
            API_KEY_LABEL,
            now,
        );
        let rows = draw(&screen(snapshot), 120, 40);
        assert!(
            rows[0].ends_with(" ago") || rows[0].ends_with("just now"),
            "{}",
            rows[0]
        );
    }
    assert_eq!(ago_text(i64::MIN), "just now");
    assert_eq!(ago_text(i64::MAX), format!("{}d ago", i64::MAX / DAY_MS));
}

#[test]
fn relative_times() {
    // `:1990-1996@481104fd`.
    assert_eq!(ago_text(59 * 1000), "just now");
    assert_eq!(ago_text(2 * MINUTE_MS), "2m ago");
    assert_eq!(ago_text(3 * HOUR_MS + 5), "3h ago");
    assert_eq!(ago_text(2 * DAY_MS), "2d ago");
    assert_eq!(ago_text(-5), "just now");
}

#[test]
fn numbers_are_written_as_fx_writes_them() {
    // `money`, `grouped`, `compact`, `optionalPercent`, `duration`
    // (`:1560-1625@481104fd`), and the captures' own values.
    assert_eq!(money(0.4335), "$0.4335");
    assert_eq!(money(0.31), "$0.3100");
    assert_eq!(money(23.37), "$23.37");
    assert_eq!(money(1234.567), "$1,234.57");
    assert_eq!(money(6551.12), "$6,551.12");
    assert_eq!(
        money(f64::INFINITY),
        format!("${}.{:02}", grouped(u64::MAX / 100), u64::MAX % 100)
    );
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1234), "1,234");
    assert_eq!(grouped(1_234_567), "1,234,567");
    assert_eq!(compact(950), "950");
    assert_eq!(compact(12_345), "12K");
    assert_eq!(compact(104_300), "104K");
    assert_eq!(compact(5_800_000), "5.8M");
    assert_eq!(compact(9_100_000_000), "9.1B");
    // A tie, rounded as Zig rounds it (`crate::usage::fixed`).
    assert_eq!(compact(14_500_000_000), "15B");
    assert_eq!(percent(Some(1.45)), "1.5%");
    assert_eq!(money(0.12345), "$0.1235");
    assert_eq!(percent(None), "-");
    assert_eq!(percent(Some(81.04)), "81.0%");
    assert_eq!(duration(None), "-");
    assert_eq!(duration(Some(600.0)), "600ms");
    assert_eq!(duration(Some(1300.0)), "1.3s");
    assert_eq!(share_of(1.0, 0.0), None);
    assert_eq!(short_day(20758), "nov 1");
}

#[test]
fn each_screen_has_its_own_hint_and_picks_the_widest_that_fits() {
    let mut table = screen(snapshot(Status::Ready, false, &month_models()));
    assert_eq!(table.key_help(120), TABLE_HELP[0]);
    assert_eq!(
        table.key_help(50),
        "\u{2191}\u{2193} model  enter detail  tab period  r refresh  esc"
    );
    assert_eq!(table.key_help(20), TABLE_HELP[2]);
    assert!(table.open_detail());
    assert_eq!(table.key_help(120), DETAIL_HELP[0]);
    assert_eq!(table.key_help(30), DETAIL_HELP[1]);
    let message = screen(snapshot(
        Status::Unavailable(Unavailable::NoGatewaySignIn),
        false,
        &[],
    ));
    assert_eq!(message.key_help(120), "esc close");
    assert_eq!(message.key_help(5), "esc");
}

#[test]
fn tab_walks_the_periods_on_the_tables_only_and_enter_needs_a_model() {
    // `setPeriod`, `openDetail` (`usage_menu.zig:54-84@481104fd`).
    let mut screen = screen(snapshot(Status::Ready, false, &month_models()));
    assert!(screen.move_model(true, 10));
    assert!(screen.set_period(Period::Month.next()));
    assert_eq!((screen.period(), screen.selected), (Period::Today, 0));
    assert!(!screen.set_period(Period::Today));
    assert!(screen.open_detail());
    assert!(!screen.open_detail());
    assert!(
        !screen.set_period(Period::Days7),
        "Tab on the detail screen"
    );
    // ↓ on the detail screen is the next model's details.
    assert!(screen.move_model(true, 10));
    assert!(draw(&screen, 120, 40)[0].contains("model 2 of 3"));
    assert!(screen.close_detail());
    assert!(!screen.close_detail());
    assert_eq!(screen.selected, 1, "back on the tables, the selection kept");

    let mut empty = Screen::open(
        Period::Month,
        snapshot(
            Status::Unavailable(Unavailable::NoGatewaySignIn),
            false,
            &[],
        ),
    );
    assert!(!empty.open_detail());
    assert!(!empty.move_model(true, 10));
    assert_eq!(empty.desired_rows(), MESSAGE_ROWS);
}

#[test]
fn a_new_snapshot_keeps_the_selected_model_by_name() {
    // `replace`, `usage_menu.zig:33-49@481104fd`.
    let models = month_models();
    let mut screen = screen(snapshot(Status::Ready, false, &models));
    assert!(screen.move_model(true, 10));
    assert!(screen.move_model(true, 10));
    assert!(screen.open_detail());
    // gpt-6.1-sol moves to the top: still selected, still on its details.
    let mut reordered = models.clone();
    reordered.swap(0, 2);
    screen.replace(snapshot(Status::Ready, false, &reordered));
    assert_eq!((screen.selected, screen.detail), (0, true));
    // Gone: the selection stays in range and the details close.
    let fewer: Vec<ModelRow> = models
        .iter()
        .filter(|m| m.model != "openai/gpt-6.1-sol")
        .cloned()
        .collect();
    screen.replace(snapshot(Status::Ready, false, &fewer[..1]));
    assert_eq!((screen.selected, screen.detail), (0, false));
}

#[test]
fn keys_mean_what_fx_binds_them_to() {
    // `app_input_runtime.zig:1196-1211,2127-2129,2177,2228-2244,2825-2832@481104fd`.
    for (action, meant) in [
        (Action::Tab, Some(MenuAction::Next)),
        (Action::Right, Some(MenuAction::Next)),
        (Action::BackTab, Some(MenuAction::Previous)),
        (Action::Left, Some(MenuAction::Previous)),
        (Action::Up, Some(MenuAction::Move(false))),
        (Action::ControlK, Some(MenuAction::Move(false))),
        (Action::Down, Some(MenuAction::Move(true))),
        (Action::ControlJ, Some(MenuAction::Move(true))),
        (Action::Submit, Some(MenuAction::Detail)),
        (Action::Escape, Some(MenuAction::Close)),
        (Action::Backspace, None),
    ] {
        assert_eq!(MenuAction::of_action(action), meant, "{action:?}");
    }
    assert_eq!(MenuAction::of_text('r'), Some(MenuAction::Reload));
    assert_eq!(MenuAction::of_text('R'), Some(MenuAction::Reload));
    assert_eq!(MenuAction::of_text('q'), None);
}

#[test]
fn the_height_is_what_it_wants_never_more_than_the_screen_less_three() {
    let screen = screen(snapshot(Status::Ready, true, &month_models()));
    assert_eq!(screen.desired_rows(), 2 + 4 + 1 + 3 + 1 + 5);
    assert_eq!(screen.height(40), 16);
    assert_eq!(screen.height(10), 7);
    assert_eq!(screen.height(2), 1);
}
