//! CLI harness for the engine — the daily driver until the Tauri UI exists.
//!
//! Usage:
//!   sf-cli plan <path> [more paths...]   Show the plan without moving anything
//!   sf-cli demo                          Build a demo tree in a tempdir and plan it
//!
//! Later milestones add: organize / undo / watch / dupes / search.

use sf_engine::plan_inputs;
use sf_engine::types::{PlanMode, PlanStatus, PlannedOp};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: sf-cli <plan|demo> [paths...]");
        std::process::exit(2);
    }
    match args[0].as_str() {
        "plan" => cmd_plan(&args[1..]),
        "demo" => cmd_demo(),
        other => {
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
    }
}

fn cmd_plan(paths: &[String]) {
    if paths.is_empty() {
        eprintln!("plan needs at least one path");
        std::process::exit(2);
    }
    let data_dir = sf_engine::Config::default_dir().unwrap_or_else(|| PathBuf::from("."));
    let cfg = sf_engine::Config::load(&data_dir);
    let inputs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let plan = plan_inputs(&inputs, &cfg.rules, &cfg.settings, PlanMode::SuperFolder);
    print_plan(&plan);
}

fn cmd_demo() {
    // A throwaway tree so anyone can see the planner work without setup.
    let tmp = tempfile::tempdir().unwrap();
    let sf = tmp.path().join("Super Folder");
    std::fs::create_dir_all(&sf).unwrap();
    for f in [
        "photo.jpg",
        "invoice.pdf",
        "movie.mp4",
        "song.mp3",
        "backup.zip",
        "installer.exe",
        "document.docx",
        "spreadsheet.xlsx",
        "mystery.zzz",
    ] {
        std::fs::write(tmp.path().join(f), b"x").unwrap();
    }
    let inputs: Vec<PathBuf> = std::fs::read_dir(tmp.path())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    let settings = sf_engine::types::Settings {
        super_folder: Some(sf),
        ..Default::default()
    };
    let plan = plan_inputs(&inputs, &[], &settings, PlanMode::SuperFolder);
    println!("demo tree: {}", tmp.path().display());
    print_plan(&plan);
}

fn print_plan(plan: &[PlannedOp]) {
    let (moves, skips, noops, pending) = count(plan);
    for op in plan {
        let mark = match &op.status {
            PlanStatus::Move { replace: false } => "→",
            PlanStatus::Move { replace: true } => "⇒",
            PlanStatus::Noop => "=",
            PlanStatus::Skip { .. } => "✗",
            PlanStatus::NeedsDecision => "?",
        };
        println!(
            "{} {}  {}  {}",
            mark,
            op.src.display(),
            mark,
            op.dst.display()
        );
        if let PlanStatus::Skip { reason } = &op.status {
            println!("    reason: {reason}");
        }
    }
    println!(
        "\n{} planned: {} move, {} skip, {} noop, {} undecided",
        plan.len(),
        moves,
        skips,
        noops,
        pending
    );
}

fn count(plan: &[PlannedOp]) -> (usize, usize, usize, usize) {
    let mut moves = 0;
    let mut skips = 0;
    let mut noops = 0;
    let mut pending = 0;
    for op in plan {
        match &op.status {
            PlanStatus::Move { .. } => moves += 1,
            PlanStatus::Skip { .. } => skips += 1,
            PlanStatus::Noop => noops += 1,
            PlanStatus::NeedsDecision => pending += 1,
        }
    }
    (moves, skips, noops, pending)
}
