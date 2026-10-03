//! CLI harness for the engine — the daily driver until the Tauri UI exists.
//!
//! Usage:
//!   sf-cli plan <path> [more paths...]   Show the plan without moving anything
//!   sf-cli organize <path> [...]         Plan and execute (with preview prompt)
//!   sf-cli undo                          Undo the most recent batch
//!   sf-cli history                       Show recorded operations
//!   sf-cli demo                          Build a demo tree in a tempdir and plan it
//!
//! Later milestones add: watch / dupes / search.

use sf_engine::mover::{self, AskResolution};
use sf_engine::types::{PlanMode, PlanStatus, PlannedOp};
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: sf-cli <plan|organize|undo|history|demo> [paths...]");
        std::process::exit(2);
    }
    let result = match args[0].as_str() {
        "plan" => cmd_plan(&args[1..]),
        "organize" => cmd_organize(&args[1..]),
        "undo" => cmd_undo(),
        "history" => cmd_history(),
        "demo" => cmd_demo(),
        other => Err(format!("unknown command: {other}")),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_plan(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("plan needs at least one path".into());
    }
    let cfg = load_config();
    let inputs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let plan = sf_engine::plan_inputs(&inputs, &cfg.rules, &cfg.settings, PlanMode::SuperFolder);
    print_plan(&plan);
    Ok(())
}

fn cmd_organize(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("organize needs at least one path".into());
    }
    let cfg = load_config();
    let inputs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let plan = sf_engine::plan_inputs(&inputs, &cfg.rules, &cfg.settings, PlanMode::SuperFolder);
    print_plan(&plan);

    let moves = plan.iter().filter(|op| op.is_executable()).count();
    if moves == 0 {
        println!("nothing to do.");
        return Ok(());
    }
    print!("Organize {} files? [y/N] ", moves);
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).map_err(|e| e.to_string())?;
    if !answer.trim().eq_ignore_ascii_case("y") {
        println!("cancelled.");
        return Ok(());
    }

    let mut counter = 0u64;
    let result = mover::execute(
        &plan,
        &cfg.history_path(),
        &cfg.data_dir.join("quarantine"),
        &mut counter,
        AskResolution::Skip,
    )
    .map_err(|e| e.to_string())?;

    println!(
        "done: {} moved, {} failed, {} skipped.",
        result.entry.items.len(),
        result.entry.failed.len(),
        result.skipped.len()
    );
    for f in &result.entry.failed {
        println!("  FAILED {}: {}", f.src.display(), f.error);
    }
    Ok(())
}

fn cmd_undo() -> Result<(), String> {
    let cfg = load_config();
    let hist = cfg.history_path();
    let entries = sf_engine::history::read(&hist);
    let Some(target) = entries
        .iter()
        .find(|e| e.kind == sf_engine::HistoryKind::Move && !e.items.is_empty())
    else {
        println!("nothing to undo.");
        return Ok(());
    };
    let mut counter = 0u64;
    let undo_entry = mover::undo(target, &hist, &mut counter).map_err(|e| e.to_string())?;
    println!(
        "undid {}: {} restored, {} failed.",
        target.id,
        undo_entry.items.len(),
        undo_entry.failed.len()
    );
    for f in &undo_entry.failed {
        println!("  FAILED {}: {}", f.src.display(), f.error);
    }
    Ok(())
}

fn cmd_history() -> Result<(), String> {
    let cfg = load_config();
    for e in sf_engine::history::read(&cfg.history_path()) {
        let kind = match e.kind {
            sf_engine::HistoryKind::Move => "MOVE",
            sf_engine::HistoryKind::Undo => "UNDO",
        };
        println!(
            "{} {}  {} ok, {} failed  {}",
            e.ts.format("%Y-%m-%d %H:%M"),
            kind,
            e.items.len(),
            e.failed.len(),
            e.id
        );
        for item in &e.items {
            println!("    {} → {}", item.src.display(), item.dst.display());
        }
        for f in &e.failed {
            println!("    FAILED {}: {}", f.src.display(), f.error);
        }
    }
    Ok(())
}

fn load_config() -> sf_engine::Config {
    let data_dir = sf_engine::Config::default_dir().unwrap_or_else(|| PathBuf::from("."));
    sf_engine::Config::load(&data_dir)
}

fn cmd_demo() -> Result<(), String> {
    // A throwaway tree so anyone can see the planner work without setup.
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
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
    let plan = sf_engine::plan_inputs(&inputs, &[], &settings, PlanMode::SuperFolder);
    println!("demo tree: {}", tmp.path().display());
    print_plan(&plan);
    Ok(())
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
