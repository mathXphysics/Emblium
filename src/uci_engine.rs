// UCI-Engine-Adapter.
// Startet eine beliebige UCI-Engine (Emble, Stockfish, ...) als Subprozess,
// liest deren stdout in einem eigenen Thread und liefert geparste Ereignisse
// über einen mpsc-Channel an den GUI-Thread.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Ready,
    IdName(String),
    IdAuthor(String),
    Info(InfoLine),
    BestMove { best: String, ponder: Option<String> },
    RawLine(String),
    Crashed(String),
}

#[derive(Debug, Clone, Default)]
pub struct InfoLine {
    pub depth: Option<u32>,
    pub seldepth: Option<u32>,
    pub multipv: Option<u32>,
    pub nodes: Option<u64>,
    pub nps: Option<u64>,
    pub score_cp: Option<i32>,
    pub score_mate: Option<i32>,
    pub pv: Vec<String>,
}

pub struct UciEngine {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pub events: Receiver<EngineEvent>,
}

impl UciEngine {
    pub fn start(path: &str) -> std::io::Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin konnte nicht geholt werden");
        let stdout = child.stdout.take().expect("stdout konnte nicht geholt werden");
        let stdin = Arc::new(Mutex::new(stdin));

        let (tx, rx): (Sender<EngineEvent>, Receiver<EngineEvent>) = channel();
        spawn_reader_thread(stdout, tx, stdin.clone());

        let engine = UciEngine { child, stdin, events: rx };
        engine.send("uci")?;
        Ok(engine)
    }

    pub fn send(&self, command: &str) -> std::io::Result<()> {
        let mut stdin = self.stdin.lock().expect("stdin-Lock vergiftet");
        writeln!(stdin, "{command}")?;
        stdin.flush()
    }

    pub fn new_game(&mut self) -> std::io::Result<()> {
        self.send("ucinewgame")?;
        self.send("isready")
    }

    pub fn set_option(&mut self, name: &str, value: &str) -> std::io::Result<()> {
        self.send(&format!("setoption name {name} value {value}"))
    }

    pub fn set_position(&mut self, moves: &[String]) -> std::io::Result<()> {
        if moves.is_empty() {
            self.send("position startpos")
        } else {
            self.send(&format!("position startpos moves {}", moves.join(" ")))
        }
    }

    pub fn set_position_fen(&mut self, fen: &str, moves: &[String]) -> std::io::Result<()> {
        if moves.is_empty() {
            self.send(&format!("position fen {fen}"))
        } else {
            self.send(&format!("position fen {fen} moves {}", moves.join(" ")))
        }
    }

    pub fn go_movetime(&mut self, ms: u32) -> std::io::Result<()> {
        self.send(&format!("go movetime {ms}"))
    }

    pub fn go_depth(&mut self, depth: u32) -> std::io::Result<()> {
        self.send(&format!("go depth {depth}"))
    }

    pub fn go_infinite(&mut self) -> std::io::Result<()> {
        self.send("go infinite")
    }

    pub fn go_time(&mut self, wtime: u32, btime: u32, winc: u32, binc: u32) -> std::io::Result<()> {
        self.send(&format!("go wtime {wtime} btime {btime} winc {winc} binc {binc}"))
    }

    pub fn stop_search(&mut self) -> std::io::Result<()> {
        self.send("stop")
    }

    pub fn quit(mut self) {
        let _ = self.send("quit");
        let _ = self.child.wait();
    }
}

fn spawn_reader_thread(
    stdout: std::process::ChildStdout,
    tx: Sender<EngineEvent>,
    stdin: Arc<Mutex<ChildStdin>>,
) {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let _ = tx.send(EngineEvent::RawLine(trimmed.to_string()));

            if trimmed == "readyok" {
                let _ = tx.send(EngineEvent::Ready);
            } else if let Some(rest) = trimmed.strip_prefix("id name ") {
                let _ = tx.send(EngineEvent::IdName(rest.to_string()));
            } else if let Some(rest) = trimmed.strip_prefix("id author ") {
                let _ = tx.send(EngineEvent::IdAuthor(rest.to_string()));
            } else if trimmed == "uciok" {
                // Nach uciok muss isready geschickt werden, um readyok/Ready auszulösen.
                if let Ok(mut s) = stdin.lock() {
                    let _ = writeln!(s, "isready");
                    let _ = s.flush();
                }
            } else if let Some(rest) = trimmed.strip_prefix("bestmove") {
                let tokens: Vec<&str> = rest.split_whitespace().collect();
                let best = tokens.first().unwrap_or(&"(none)").to_string();
                let ponder = if tokens.len() >= 3 && tokens[1] == "ponder" {
                    Some(tokens[2].to_string())
                } else {
                    None
                };
                let _ = tx.send(EngineEvent::BestMove { best, ponder });
            } else if trimmed.starts_with("info") {
                let _ = tx.send(EngineEvent::Info(parse_info(trimmed)));
            }
        }
    });
}

fn parse_info(line: &str) -> InfoLine {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut info = InfoLine::default();
    let mut i = 1;
    while i < tokens.len() {
        match tokens[i] {
            "depth" => { info.depth = tokens.get(i + 1).and_then(|v| v.parse().ok()); i += 2; }
            "seldepth" => { info.seldepth = tokens.get(i + 1).and_then(|v| v.parse().ok()); i += 2; }
            "multipv" => { info.multipv = tokens.get(i + 1).and_then(|v| v.parse().ok()); i += 2; }
            "nodes" => { info.nodes = tokens.get(i + 1).and_then(|v| v.parse().ok()); i += 2; }
            "nps" => { info.nps = tokens.get(i + 1).and_then(|v| v.parse().ok()); i += 2; }
            "score" => {
                if tokens.get(i + 1) == Some(&"cp") {
                    info.score_cp = tokens.get(i + 2).and_then(|v| v.parse().ok());
                } else if tokens.get(i + 1) == Some(&"mate") {
                    info.score_mate = tokens.get(i + 2).and_then(|v| v.parse().ok());
                }
                i += 3;
            }
            "pv" => {
                info.pv = tokens[i + 1..].iter().map(|s| s.to_string()).collect();
                break;
            }
            _ => { i += 1; }
        }
    }
    info
}
