//! CLI harness for the engine — the daily driver until the Tauri UI exists.
//!
//! Usage:
//!   sf-cli plan <path> [more paths...]   Show the plan without moving anything
//!   sf-cli organize <path> [...]         Plan and execute (with preview prompt)
//!   sf-cli undo                          Undo the most recent batch
//!   sf-cli history                       Show recorded operations
//!   sf-cli watch <folder> [...]          Watch folders and auto-organize
//!   sf-cli dupes <folder>                Find duplicate files (size + SHA-256)
//!   sf-cli search <folder> <query>       Search files (name/ext/type/size/date)
//!   sf-cli rename [--apply] <folder> <template>   Preview or apply a rename template
//!   sf-cli find <query>                  Universal search over your usual folders + apps
//!   sf-cli demo                          Build a demo tree in a tempdir and plan it

use sf_engine::mover::{self, AskResolution};
use sf_engine::types::{PlanMode, PlanStatus, PlannedOp};
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: sf-cli <plan|organize|undo|history|watch|dupes|search|rename|find|demo> [args...]"
        );
        std::process::exit(2);
    }
    let result = match args[0].as_str() {
        "plan" => cmd_plan(&args[1..]),
        "organize" => cmd_organize(&args[1..]),
        "undo" => cmd_undo(),
        "history" => cmd_history(),
        "watch" => cmd_watch(&args[1..]),
        "dupes" => cmd_dupes(&args[1..]),
        "search" => cmd_search(&args[1..]),
        "rename" => cmd_rename(&args[1..]),
        "find" => cmd_find(&args[1..]),
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
    let plan = sf_engine::plan_inputs(&inputs, &cfg.rules, &cfg.settings, PlanMode::OrganizeRoot);
    print_plan(&plan);
    Ok(())
}

fn cmd_organize(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("organize needs at least one path".into());
    }
    let cfg = load_config();
    let inputs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let plan = sf_engine::plan_inputs(&inputs, &cfg.rules, &cfg.settings, PlanMode::OrganizeRoot);
    print_plan(&plan);

    let moves = plan.iter().filter(|op| op.is_executable()).count();
    if moves == 0 {
        println!("nothing to do.");
        return Ok(());
    }
    print!("Organize {} files? [y/N] ", moves);
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
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

fn cmd_dupes(paths: &[String]) -> Result<(), String> {
    let folder = paths.first().ok_or("dupes needs a folder")?;
    let groups = sf_engine::duplicates::find_duplicates(&PathBuf::from(folder))
        .map_err(|e| e.to_string())?;
    if groups.is_empty() {
        println!("no duplicates found.");
        return Ok(());
    }
    for g in &groups {
        println!(
            "\n{} ({} files, sha256 {}…)",
            human_size(g.size_bytes),
            g.files.len(),
            &g.sha256[..16]
        );
        for f in &g.files {
            let when = f
                .modified
                .map(|t| {
                    chrono::DateTime::<chrono::Local>::from(t)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_else(|| "?".into());
            println!("    {}  ({})", f.path.display(), when);
        }
    }
    println!(
        "\n{} duplicate group(s). Nothing was deleted or moved — decide yourself.",
        groups.len()
    );
    Ok(())
}

fn cmd_search(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("usage: search <folder> <query>".into());
    }
    let folder = PathBuf::from(&args[0]);
    let query = args[1..].join(" ");
    let hits = sf_engine::search::search(&folder, &query).map_err(|e| e.to_string())?;
    for h in &hits {
        println!("    {:>10}  {}", human_size(h.size_bytes), h.path.display());
    }
    println!("{} hit(s).", hits.len());
    Ok(())
}

fn cmd_rename(args: &[String]) -> Result<(), String> {
    let apply = args.first().map(|a| a == "--apply").unwrap_or(false);
    let rest = if apply { &args[1..] } else { args };
    if rest.len() < 2 {
        return Err("usage: rename [--apply] <folder> <template>".into());
    }
    let folder = PathBuf::from(&rest[0]);
    let template = &rest[1];
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&folder)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    let ops = sf_engine::renamer::plan_rename(&paths, template).map_err(|e| e.to_string())?;
    for op in &ops {
        if op.skipped {
            println!(
                "✗ {}  ({})",
                op.src.display(),
                op.reason.as_deref().unwrap_or("?")
            );
        } else {
            println!("→ {}  {}", op.src.display(), op.dst.display());
        }
    }
    let renames = ops.iter().filter(|o| !o.skipped && o.src != o.dst).count();
    if !apply {
        println!(
            "\n{} file(s) would be renamed. Re-run with --apply.",
            renames
        );
        return Ok(());
    }
    let (n, failed) = sf_engine::renamer::apply_rename(&ops);
    println!("\nrenamed {}, {} failed.", n, failed.len());
    for (p, e) in &failed {
        println!("  FAILED {}: {e}", p.display());
    }
    Ok(())
}

fn cmd_find(args: &[String]) -> Result<(), String> {
    use sf_engine::search_index::{default_app_roots, default_file_roots, Index};
    let started = std::time::Instant::now();
    let idx = Index::build(&default_file_roots(), &default_app_roots(), 400_000);
    println!("indexed {} entries in {:?}", idx.len(), started.elapsed());
    let query = args.join(" ");
    let t = std::time::Instant::now();
    let hits = idx.query(&query, 10);
    println!("query {:?} took {:?}", query, t.elapsed());
    for h in hits {
        println!("    {:?}  {}  ({})", h.kind, h.name, h.path.display());
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn cmd_watch(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("watch needs at least one folder".into());
    }
    let folders: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    for f in &folders {
        if !f.is_dir() {
            return Err(format!("not a folder: {}", f.display()));
        }
    }
    let cfg = load_config();
    let (mut handle, rx) = sf_engine::watcher::start(&folders).map_err(|e| e.to_string())?;

    println!(
        "watching {} folder(s). Ctrl+C to stop. auto_organize={}",
        folders.len(),
        cfg.settings.auto_organize
    );
    let mut counter = 0u64;
    while let Ok(batch) = rx.recv() {
        {
            {
                // Re-read config each batch so rule edits apply live.
                let cfg = load_config();
                let plan = sf_engine::plan_inputs(
                    &batch,
                    &cfg.rules,
                    &cfg.settings,
                    PlanMode::WatchFolder,
                );
                print_plan(&plan);
                if cfg.settings.auto_organize && !cfg.settings.paused {
                    let result = mover::execute(
                        &plan,
                        &cfg.history_path(),
                        &cfg.data_dir.join("quarantine"),
                        &mut counter,
                        AskResolution::Skip,
                    )
                    .map_err(|e| e.to_string())?;
                    println!(
                        "organized: {} moved, {} failed, {} skipped",
                        result.entry.items.len(),
                        result.entry.failed.len(),
                        result.skipped.len()
                    );
                }
            }
        }
    }
    handle.stop();
    Ok(())
}

fn cmd_demo() -> Result<(), String> {
    // A throwaway tree so anyone can see the planner work without setup.
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let sf = tmp.path().join("DeskZero");
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
        organize_root: Some(sf),
        ..Default::default()
    };
    let plan = sf_engine::plan_inputs(&inputs, &[], &settings, PlanMode::OrganizeRoot);
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
