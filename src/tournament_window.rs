// Zweites Fenster: Engine-vs-Engine-Einzelpartie und SPRT-Serie.
// Läuft in einem Hintergrund-Thread; Ergebnisse kommen per mpsc-Channel
// zurück und werden per Timer im GUI-Thread abgeholt (gleiches Muster wie
// die UCI-Engine-Events in main.rs).

use std::sync::mpsc::{channel, Receiver};
use std::thread;

use fltk::app;
use fltk::button::Button;
use fltk::enums::Align;
use fltk::frame::Frame;
use fltk::group::Pack;
use fltk::input::{FloatInput, Input, IntInput};
use fltk::prelude::*;
use fltk::text::{TextBuffer, TextDisplay};
use fltk::window::Window;

use crate::tournament::{play_game, Outcome, SprtDecision, SprtState};

enum RunnerMsg {
    GameFinished { outcome_for_engine_a: Outcome, plies: usize, termination: String },
    Log(String),
    Finished,
}

pub fn open_tournament_window() {
    let mut win = Window::new(200, 150, 620, 560, "Emblium - Turnier & SPRT");

    let mut root = Pack::new(15, 15, 590, 530, "");
    root.set_spacing(6);

    let mut engine_a_label = Frame::new(0, 0, 590, 20, "Engine A (Test-Engine, z.B. neue Emble-Version):");
    engine_a_label.set_align(Align::Left | Align::Inside);
    let mut engine_a_row = Pack::new(0, 0, 590, 26, "");
    engine_a_row.set_type(fltk::group::PackType::Horizontal);
    engine_a_row.set_spacing(6);
    let mut engine_a_input = Input::new(0, 0, 490, 26, "");
    engine_a_input.set_value("/usr/games/stockfish");
    let mut engine_a_browse = Button::new(0, 0, 94, 26, "Durchsuchen...");
    engine_a_row.end();

    let mut engine_b_label = Frame::new(0, 0, 590, 20, "Engine B (Referenz/Gegner):");
    engine_b_label.set_align(Align::Left | Align::Inside);
    let mut engine_b_row = Pack::new(0, 0, 590, 26, "");
    engine_b_row.set_type(fltk::group::PackType::Horizontal);
    engine_b_row.set_spacing(6);
    let mut engine_b_input = Input::new(0, 0, 490, 26, "");
    engine_b_input.set_value("/usr/games/stockfish");
    let mut engine_b_browse = Button::new(0, 0, 94, 26, "Durchsuchen...");
    engine_b_row.end();

    let mut params_row = Pack::new(0, 0, 590, 26, "");
    params_row.set_type(fltk::group::PackType::Horizontal);
    params_row.set_spacing(10);
    let movetime_label = Frame::new(0, 0, 90, 26, "Movetime (ms):");
    let mut movetime_input = IntInput::new(0, 0, 70, 26, "");
    movetime_input.set_value("100");
    let elo0_label = Frame::new(0, 0, 60, 26, "Elo0:");
    let mut elo0_input = FloatInput::new(0, 0, 60, 26, "");
    elo0_input.set_value("0");
    let elo1_label = Frame::new(0, 0, 60, 26, "Elo1:");
    let mut elo1_input = FloatInput::new(0, 0, 60, 26, "");
    elo1_input.set_value("10");
    let games_label = Frame::new(0, 0, 110, 26, "Max. Partien:");
    let mut games_input = IntInput::new(0, 0, 70, 26, "");
    games_input.set_value("200");
    params_row.end();

    let mut start_btn = Button::new(0, 0, 590, 30, "SPRT-Lauf starten (Engine A vs Engine B)");
    let mut single_game_btn = Button::new(0, 0, 590, 30, "Nur eine Testpartie spielen");

    let status_label = Frame::new(0, 0, 590, 20, "Status:");
    let mut status = Frame::new(0, 0, 590, 24, "Bereit");
    status.set_align(Align::Left | Align::Inside);

    let mut log_display = TextDisplay::new(0, 0, 590, 300, "");
    let log_buffer = TextBuffer::default();
    log_display.set_buffer(log_buffer.clone());

    root.end();
    win.end();
    win.show();

    // ---------- Durchsuchen-Buttons für Engine-Pfade ----------
    {
        let mut engine_a_input = engine_a_input.clone();
        engine_a_browse.set_callback(move |_| {
            let mut chooser = fltk::dialog::NativeFileChooser::new(fltk::dialog::FileDialogType::BrowseFile);
            chooser.set_title("Engine A auswählen");
            chooser.show();
            let path = chooser.filename();
            if !path.as_os_str().is_empty() {
                engine_a_input.set_value(&path.to_string_lossy());
            }
        });
    }
    {
        let mut engine_b_input = engine_b_input.clone();
        engine_b_browse.set_callback(move |_| {
            let mut chooser = fltk::dialog::NativeFileChooser::new(fltk::dialog::FileDialogType::BrowseFile);
            chooser.set_title("Engine B auswählen");
            chooser.show();
            let path = chooser.filename();
            if !path.as_os_str().is_empty() {
                engine_b_input.set_value(&path.to_string_lossy());
            }
        });
    }

    // ---------- Einzelpartie ----------
    {
        let engine_a_input = engine_a_input.clone();
        let engine_b_input = engine_b_input.clone();
        let movetime_input = movetime_input.clone();
        let mut status_h = status.clone();
        let mut log_buffer_h = log_buffer.clone();
        single_game_btn.set_callback(move |_| {
            let a = engine_a_input.value();
            let b = engine_b_input.value();
            let movetime: u32 = movetime_input.value().parse().unwrap_or(100);
            status_h.set_label("Spiele Testpartie...");
            log_buffer_h.append("Starte Einzelpartie A(Weiß) vs B(Schwarz)...\n");

            let (tx, rx): (std::sync::mpsc::Sender<RunnerMsg>, Receiver<RunnerMsg>) = channel();
            thread::spawn(move || {
                match play_game(&a, &b, movetime) {
                    Ok(result) => {
                        let _ = tx.send(RunnerMsg::Log(format!(
                            "Ergebnis: {:?} nach {} Halbzügen ({})\n",
                            result.outcome, result.moves.len(), result.termination
                        )));
                    }
                    Err(e) => {
                        let _ = tx.send(RunnerMsg::Log(format!("Fehler: {e}\n")));
                    }
                }
                let _ = tx.send(RunnerMsg::Finished);
            });
            poll_runner(rx, status_h.clone(), log_buffer_h.clone(), None);
        });
    }

    // ---------- SPRT-Serie ----------
    {
        let engine_a_input = engine_a_input.clone();
        let engine_b_input = engine_b_input.clone();
        let movetime_input = movetime_input.clone();
        let elo0_input = elo0_input.clone();
        let elo1_input = elo1_input.clone();
        let games_input = games_input.clone();
        let mut status_h = status.clone();
        let mut log_buffer_h = log_buffer.clone();
        start_btn.set_callback(move |_| {
            let a = engine_a_input.value();
            let b = engine_b_input.value();
            let movetime: u32 = movetime_input.value().parse().unwrap_or(100);
            let elo0: f64 = elo0_input.value().parse().unwrap_or(0.0);
            let elo1: f64 = elo1_input.value().parse().unwrap_or(10.0);
            let max_games: u32 = games_input.value().parse().unwrap_or(200);

            status_h.set_label("SPRT läuft...");
            log_buffer_h.append(&format!(
                "SPRT-Start: H0 elo0={elo0}, H1 elo1={elo1}, max. {max_games} Partien, movetime={movetime}ms\n"
            ));

            let (tx, rx): (std::sync::mpsc::Sender<RunnerMsg>, Receiver<RunnerMsg>) = channel();
            thread::spawn(move || {
                let mut sprt = SprtState::new(elo0, elo1, 0.05, 0.05);
                for game_no in 0..max_games {
                    let (white, black) = if game_no % 2 == 0 { (a.as_str(), b.as_str()) } else { (b.as_str(), a.as_str()) };
                    match play_game(white, black, movetime) {
                        Ok(result) => {
                            let outcome_for_a = if game_no % 2 == 0 {
                                result.outcome
                            } else {
                                match result.outcome {
                                    Outcome::WhiteWin => Outcome::BlackWin,
                                    Outcome::BlackWin => Outcome::WhiteWin,
                                    Outcome::Draw => Outcome::Draw,
                                }
                            };
                            sprt.record(outcome_for_a);
                            let _ = tx.send(RunnerMsg::GameFinished {
                                outcome_for_engine_a: outcome_for_a,
                                plies: result.moves.len(),
                                termination: result.termination.clone(),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(RunnerMsg::Log(format!("Partie {game_no}: Fehler: {e}\n")));
                            continue;
                        }
                    }

                    let llr = sprt.llr();
                    let (lower, upper) = sprt.bounds();
                    let decision = sprt.decide();
                    let elo_info = sprt
                        .estimated_elo()
                        .map(|(e, err)| format!("{e:+.1} +/- {err:.1}"))
                        .unwrap_or_else(|| "n/a".to_string());
                    let _ = tx.send(RunnerMsg::Log(format!(
                        "Partie {}: W{} D{} L{} | LLR={:.2} [{:.2}, {:.2}] | Elo(A-B)={} \n",
                        game_no + 1, sprt.wins, sprt.draws, sprt.losses, llr, lower, upper, elo_info
                    )));

                    if decision != SprtDecision::Continue {
                        let verdict = match decision {
                            SprtDecision::AcceptH1 => "H1 akzeptiert: Engine A ist signifikant stärker (>= elo1)",
                            SprtDecision::AcceptH0 => "H0 akzeptiert: keine signifikante Verbesserung (<= elo0)",
                            SprtDecision::Continue => unreachable!(),
                        };
                        let _ = tx.send(RunnerMsg::Log(format!("SPRT-Entscheidung: {verdict}\n")));
                        break;
                    }
                }
                let _ = tx.send(RunnerMsg::Finished);
            });
            poll_runner(rx, status_h.clone(), log_buffer_h.clone(), None);
        });
    }

    let _keep = (
        engine_a_label, engine_b_label, movetime_label, elo0_label, elo1_label,
        games_label, status_label, engine_a_input, engine_b_input,
        movetime_input, elo0_input, elo1_input, games_input,
    );

    while win.shown() {
        app::wait();
    }
}

/// Pollt Nachrichten aus dem Hintergrund-Thread und schreibt sie ins Log,
/// bis der Lauf beendet ist. Läuft als eigene Event-Loop innerhalb des
/// Button-Callbacks (fltk erlaubt verschachteltes app::wait()).
fn poll_runner(
    rx: Receiver<RunnerMsg>,
    mut status: Frame,
    mut log_buffer: TextBuffer,
    _unused: Option<()>,
) {
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(RunnerMsg::Log(line)) => {
                log_buffer.append(&line);
            }
            Ok(RunnerMsg::GameFinished { outcome_for_engine_a, plies, termination }) => {
                log_buffer.append(&format!(
                    "  -> Ergebnis für A: {outcome_for_engine_a:?}, {plies} Halbzüge, {termination}\n"
                ));
            }
            Ok(RunnerMsg::Finished) => {
                status.set_label("Fertig");
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                app::wait();
                app::check();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
